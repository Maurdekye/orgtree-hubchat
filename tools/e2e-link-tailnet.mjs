// Linking when the PC reaches its hub as localhost (the user's setup), over
// the tailnet, real apps (coordinator 20:28Z: not through the USB tunnel):
// the desktop test build is A on http://localhost:7397 and shows the link
// QR; it must name the hub as this PC's name and addresses. The phone test
// build (fresh) is opened with that link as a deep link, reaches the hub by
// one of them, joins and is approved; then it reviews the hubs that come
// with the identity (user 20:38Z) and confirms; it must keep the address it
// reached, never localhost, and connect.
//
//   needs: the scratch hub listening on 0.0.0.0:7397; Hubchat Test installed
//   on the phone; NO adb reverse for the hub (the script removes any)
//   node tools/e2e-link-tailnet.mjs <hubchat.exe test build> <shots-dir>
import { spawn, spawnSync } from 'node:child_process';
import { join } from 'node:path';
import { attach } from './cdp.mjs';

const [exe, shots] = process.argv.slice(2);
const ADB = '<toolchain>/android-sdk/platform-tools/adb.exe';
const PKG = 'dev.orgtree.hubchat.test';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const adb = (...a) => spawnSync(ADB, a, { encoding: 'utf8' }).stdout.trim();
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };

// A fresh phone app with no tunnel to the hub.
adb('reverse', '--remove-all');
adb('shell', 'pm', 'clear', PKG);
adb('shell', 'pm', 'grant', PKG, 'android.permission.POST_NOTIFICATIONS');
adb('shell', 'monkey', '-p', PKG, '-c', 'android.intent.category.LAUNCHER', '1');
await sleep(4000);
const pid = adb('shell', 'pidof', PKG);
adb('forward', 'tcp:9334', `localabstract:webview_devtools_remote_${pid}`);

const app = spawn(exe, [], { stdio: 'ignore', env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9333' } });
let pc, phone;
try {
  for (let i = 0; i < 60 && !pc; i++) { try { pc = await attach(9333); } catch { await sleep(500); } }
  phone = await attach(9334);
  await sleep(2500);
  const inv = (b, cmd, args = {}) => b.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);
  const shot = (b, n) => Promise.race([b.shot(join(shots, n)).catch(() => {}), sleep(8000)]);
  const text = (b) => b.eval('document.body.innerText');
  const waitFor = async (b, expr, ms) => { const t = Date.now(); while (Date.now() - t < ms) { if (await b.eval(expr).catch(() => false)) return true; await sleep(500); } return false; };
  const click = (b, re) => b.eval(`(() => { const el = [...document.querySelectorAll('button')].find((x) => ${re}.test(x.innerText.trim()) && !x.disabled); if (!el) return false; el.click(); return true; })()`);

  // Desktop = A on the hub as localhost.
  const A = await inv(pc, 'hc_create_identity', { id: 'alice', name: 'Alice' });
  await inv(pc, 'hc_add_hub', { input: 'localhost:7397' });
  const up = await waitFor(pc, `window.__TAURI_INTERNALS__.invoke('hc_state').then((s) => s.hubs.some((h) => h.state === 'connected'))`, 20000);
  check('desktop is A, connected to its hub as localhost', A.startsWith('alice.') && up, A);

  const offer = await inv(pc, 'hc_link_offer', {});
  console.log('QR:', offer.qr);
  check('the QR names the hub as this PC is reached', offer.hub === 'http://localhost:7397' && offer.hubs[0] === 'http://home-pc:7397'
    && offer.hubs.includes('http://100.101.102.103:7397') && offer.hubs.at(-1) === 'http://localhost:7397', JSON.stringify(offer.hubs));
  check('the QR carries the hub name', offer.qr.includes('name=v2scratch'));

  // The phone opens the link as a camera app would.
  spawnSync(ADB, ['shell', 'am', 'start', '-a', 'android.intent.action.VIEW', '-d', `'${offer.qr}'`, PKG], { stdio: 'ignore' });
  // No name to type (user 00:16Z): once the hub answers, the phone waits for
  // approval under its own name.
  const phoneName = (await inv(phone, 'hc_state')).device_name;
  const waiting = await waitFor(phone, `/is waiting there/.test(document.body.innerText)`, 30000);
  const t1 = await text(phone);
  check('the phone asks no name and waits under its own', waiting && !/Name this phone/.test(t1) && !!phoneName && t1.includes(phoneName), phoneName + ' / ' + t1.replace(/\s+/g, ' ').slice(0, 300));
  await shot(phone, 'tailnet-phone-wait.png');

  // The desktop sees the phone waiting and approves.
  let look = null;
  for (let i = 0; i < 40; i++) {
    look = await inv(pc, 'hc_link_lookup', { input: offer.code });
    if (look.device_name) break;
    await sleep(3000);
  }
  check('desktop sees the phone by its own name', !!look?.device_name && look.device_name === phoneName, JSON.stringify(look));
  await inv(pc, 'hc_link_approve', { code: offer.code });

  // The phone reviews the hubs before anything is saved (user 20:38Z).
  const review = await waitFor(phone, `/Confirm/.test(document.body.innerText) && document.querySelectorAll('input[type=checkbox]').length > 0`, 40000);
  if (review) {
    await sleep(2500); // the rows' reachability checks
    await shot(phone, 'tailnet-phone-review.png');
    const before = await inv(phone, 'hc_state');
    check('nothing saved before Confirm', !before.me && before.hubs.length === 0, JSON.stringify(before.hubs));
    const rows = await phone.eval(`[...document.querySelectorAll('.hubrev input[type=text], .hubrev input:not([type])')].map((i) => i.value)`);
    check('the review proposes the reached address, not localhost', rows.some((v) => /home-pc|100\.101\.102\.103/.test(v)) && !rows.some((v) => /localhost|127\.0\.0\.1/.test(v)), JSON.stringify(rows));
    check('Confirm', await click(phone, /^Confirm/));
  } else {
    check('the phone shows the hub review', false, (await text(phone)).replace(/\s+/g, ' ').slice(0, 300));
  }

  const adopted = await waitFor(phone, `window.__TAURI_INTERNALS__.invoke('hc_state').then((s) => s.me && s.me.address === ${JSON.stringify(A)})`, 30000);
  check('phone is now A', adopted);
  const connected = await waitFor(phone, `window.__TAURI_INTERNALS__.invoke('hc_state').then((s) => s.hubs.length > 0 && s.hubs.every((h) => h.state === 'connected'))`, 40000);
  const st = await inv(phone, 'hc_state');
  const urls = st.hubs.map((h) => h.url);
  check('phone keeps the tailnet address, never localhost', urls.length === 1 && /home-pc|100\.101\.102\.103/.test(urls[0]), JSON.stringify(urls));
  check('phone connects to the hub over the tailnet', connected, JSON.stringify(st.hubs.map((h) => [h.url, h.state, h.error])));
  await sleep(1500);
  await shot(phone, 'tailnet-phone-after.png');
  check('no script errors on the phone', phone.errors.length === 0, phone.errors.join(' | '));
} catch (e) {
  check('script ran to the end', false, String(e));
} finally {
  try { await pc?.close(); await phone?.close(); } catch { /* ignore */ }
  spawnSync('taskkill', ['/PID', String(app.pid), '/T', '/F'], { stdio: 'ignore' });
  const failed = results.filter((r) => !r).length;
  console.log(`${results.length - failed}/${results.length} passed`);
  process.exit(failed ? 1 : 0);
}
