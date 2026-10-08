// The PC is the new device: the phone (signed in, its hub as home-pc:7397)
// offers a link code, the PC types it and joins through the same hub as
// localhost:7397, the phone approves. The PC's review must show ONE row for
// that hub (it answers under both names), keep localhost and say the phone
// knows it as home-pc; confirming makes the PC that identity on localhost.
//
//   needs: the scratch hub on 0.0.0.0:7397; Hubchat Test on the phone signed
//   in with its hub as http://home-pc:7397; adb forward tcp:9334 to its
//   WebView; the desktop test build reset (tools/e2e-reset.sh)
//   node tools/e2e-link-pc-new.mjs <hubchat.exe test build>
import { spawn, spawnSync } from 'node:child_process';
import { attach } from './cdp.mjs';

const [exe] = process.argv.slice(2);
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
  const waitFor = async (b, expr, ms) => { const t = Date.now(); while (Date.now() - t < ms) { if (await b.eval(expr).catch(() => false)) return true; await sleep(500); } return false; };

  const ph = await inv(phone, 'hc_state');
  check('phone is signed in on home-pc:7397', !!ph.me && ph.hubs.some((h) => h.url === 'http://home-pc:7397'), JSON.stringify(ph.hubs.map((h) => h.url)));
  const pcs = await inv(pc, 'hc_state');
  check('PC is new', !pcs.me);

  // the PC hears its link events the way the UI does
  await pc.eval(`(async () => {
    window.__evs = [];
    const cb = window.__TAURI_INTERNALS__.transformCallback((e) => window.__evs.push(e.payload));
    await window.__TAURI_INTERNALS__.invoke('plugin:event|listen', { event: 'hc-link', target: { kind: 'Any' }, handler: cb });
  })()`);

  const offer = await inv(phone, 'hc_link_offer', {});
  check('the phone offers a code through home-pc', offer.hubs.length === 1 && offer.hubs[0] === 'http://home-pc:7397', JSON.stringify(offer.hubs));
  // typed on the PC: a code alone, through the hub as the PC knows it
  await inv(pc, 'hc_link_start', { hub: 'localhost:7397', deviceName: 'PC (test)', code: offer.code, aliases: null });
  let look = null;
  for (let i = 0; i < 40; i++) {
    look = await inv(phone, 'hc_link_lookup', { input: offer.code });
    if (look.device_name) break;
    await sleep(3000);
  }
  check('the phone sees the PC by name', look?.device_name === 'PC (test)', JSON.stringify(look));
  await inv(phone, 'hc_link_approve', { code: offer.code });

  const got = await waitFor(pc, `window.__evs.some((e) => e.state === 'review')`, 40000);
  const ev = got ? await pc.eval(`window.__evs.find((e) => e.state === 'review')`) : null;
  console.log('review:', JSON.stringify(ev));
  check('the PC gets a review', got);
  check('one row for the hub it reached under two names', ev?.hubs.length === 1 && ev.hubs[0].address === 'http://localhost:7397'
    && ev.hubs[0].theirs === 'http://home-pc:7397' && ev.hubs[0].reachable, JSON.stringify(ev?.hubs));
  const still = await inv(pc, 'hc_state');
  check('nothing saved before confirming', !still.me && still.hubs.length === 0);

  const a = await inv(pc, 'hc_link_confirm', { hubs: ev.hubs.map((h) => h.address) });
  check('the PC is now the phone\'s identity', a === ph.me.address, a);
  const ok = await waitFor(pc, `window.__TAURI_INTERNALS__.invoke('hc_state').then((s) => s.hubs.length === 1 && s.hubs[0].url === 'http://localhost:7397' && s.hubs[0].state === 'connected')`, 30000);
  check('the PC keeps localhost and connects', ok, JSON.stringify((await inv(pc, 'hc_state')).hubs.map((h) => [h.url, h.state])));
} catch (e) {
  check('script ran to the end', false, String(e));
} finally {
  try { await pc?.close(); await phone?.close(); } catch { /* ignore */ }
  spawnSync('taskkill', ['/PID', String(app.pid), '/T', '/F'], { stdio: 'ignore' });
  const failed = results.filter((r) => !r).length;
  console.log(`${results.length - failed}/${results.length} passed`);
  process.exit(failed ? 1 : 0);
}
