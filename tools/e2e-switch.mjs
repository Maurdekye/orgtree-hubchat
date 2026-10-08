// Linking the new way round and switching identity, real apps, scratch v2 hub:
// the desktop test build is A and SHOWS the link QR (hc_link_offer); the
// phone test build (already B) is opened with that link as a deep link (as a
// camera app would), joins, the desktop approves, the phone must offer the
// switch screen; ticking and confirming makes the phone A. Opening another
// link from A on the phone must then say "same".
//
//   needs: adb reverse tcp:7397 tcp:7397; adb forward tcp:9334 localabstract:webview_devtools_remote_<pid of dev.orgtree.hubchat.test>
//   node tools/e2e-switch.mjs <hubchat.exe test build> <shots-dir>
import { spawn, spawnSync } from 'node:child_process';
import { join } from 'node:path';
import { attach } from './cdp.mjs';

const [exe, shots] = process.argv.slice(2);
const HUB = '127.0.0.1:7397';
const ADB = '<toolchain>/android-sdk/platform-tools/adb.exe';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };

const app = spawn(exe, [], { stdio: 'ignore', env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9333' } });
let pc, phone;
try {
  for (let i = 0; i < 60 && !pc; i++) { try { pc = await attach(9333); } catch { await sleep(500); } }
  phone = await attach(9334);
  await sleep(2500);
  const inv = (b, cmd, args = {}) => b.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);
  const shot = (b, n) => Promise.race([b.shot(join(shots, n)).catch(() => {}), sleep(8000)]);
  const waitFor = async (b, expr, ms) => { const t = Date.now(); while (Date.now() - t < ms) { if (await b.eval(expr).catch(() => false)) return true; await sleep(400); } return false; };

  // Desktop = A; phone = B.
  const A = await inv(pc, 'hc_create_identity', { id: 'alice', name: 'Alice' });
  await inv(pc, 'hc_add_hub', { input: HUB });
  const B = await inv(phone, 'hc_create_identity', { id: 'bob', name: 'Bob' });
  await inv(phone, 'hc_add_hub', { input: HUB });
  await phone.eval('location.reload()'); await sleep(3000);
  check('desktop is A, phone is B', A.startsWith('alice.') && B.startsWith('bob.'), `${A} / ${B}`);

  // Desktop shows the link QR; the phone is opened with it as a deep link.
  const offer = await inv(pc, 'hc_link_offer', {});
  check('desktop link QR is a hubchat:// link with role=give', offer.qr.startsWith('hubchat://link?') && offer.qr.includes('role=give'), offer.qr);
  spawnSync(ADB, ['shell', 'am', 'start', '-a', 'android.intent.action.VIEW', '-d', `'${offer.qr}'`, 'dev.orgtree.hubchat.test'], { stdio: 'ignore' });
  await sleep(3000);
  await shot(phone, 'switch-phone-joining.png');

  // The desktop sees the phone waiting and approves.
  let look = null;
  for (let i = 0; i < 40; i++) {
    look = await inv(pc, 'hc_link_lookup', { input: offer.code });
    if (look.device_name) break;
    await sleep(3000);
  }
  check('desktop sees the phone by name', !!look?.device_name, JSON.stringify(look));
  await inv(pc, 'hc_link_approve', { code: offer.code });

  // The phone must ask before switching from B to A.
  const asked = await waitFor(phone, `document.body.innerText.includes(${JSON.stringify(A)}) && /Switch/.test(document.body.innerText)`, 40000);
  check('phone shows the switch screen (B -> A)', asked);
  await shot(phone, 'switch-phone-ask.png');
  const blocked = await phone.eval(`(() => { const b = [...document.querySelectorAll('button')].find((x) => /^Switch to/.test(x.innerText.trim())); return b ? b.disabled : null; })()`);
  check('Switch is disabled until the box is ticked', blocked === true, String(blocked));
  // the "I have the recovery words" box (the hub review has boxes of its own)
  await phone.eval(`(() => { const l = [...document.querySelectorAll('label')].find((x) => /recovery words/.test(x.innerText)); l.querySelector('input[type=checkbox]').click(); })()`); await sleep(400);
  await phone.eval(`[...document.querySelectorAll('button')].find((x) => /^Switch to/.test(x.innerText.trim())).click()`);
  const switched = await waitFor(phone, `window.__TAURI_INTERNALS__.invoke('hc_state').then((s) => s.me && s.me.address === ${JSON.stringify(A)})`, 20000);
  check('phone is now A', switched);
  const st = await inv(phone, 'hc_state');
  check('phone has A\'s hub', st.hubs.some((h) => h.url.includes('7397')), JSON.stringify(st.hubs.map((h) => h.url)));
  await sleep(2000);
  await shot(phone, 'switch-phone-after.png');

  // A second link from A: "same".
  const offer2 = await inv(pc, 'hc_link_offer', {});
  spawnSync(ADB, ['shell', 'am', 'start', '-a', 'android.intent.action.VIEW', '-d', `'${offer2.qr}'`, 'dev.orgtree.hubchat.test'], { stdio: 'ignore' });
  await sleep(3000);
  for (let i = 0; i < 40; i++) {
    const l = await inv(pc, 'hc_link_lookup', { input: offer2.code });
    if (l.device_name) break;
    await sleep(3000);
  }
  await inv(pc, 'hc_link_approve', { code: offer2.code });
  const same = await waitFor(phone, `/already/i.test(document.body.innerText)`, 40000);
  check('phone says it is already A', same);
  await shot(phone, 'switch-phone-same.png');
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
