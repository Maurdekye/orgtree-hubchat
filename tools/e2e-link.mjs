// Device linking end to end, real apps on both sides through a scratch hub:
// the desktop app gets a fresh identity; the phone (cleared, on onboarding)
// starts "link through a hub"; the desktop looks the code up and approves;
// the phone must end up with the desktop's address (v1: devices share the
// one key), its profile and its hub list, and the link address must be gone.
//
//   adb reverse tcp:7399 tcp:7399; adb forward tcp:9334 localabstract:webview_devtools_remote_<pid>
//   node tools/e2e-link.mjs <hubchat.exe> <shots-dir>
import { spawn, spawnSync } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { attach } from './cdp.mjs';

const [exe, shots] = process.argv.slice(2);
const MAILHUB = process.env.MAILHUB_DIR || '<orgtree>/engine/mailhub';
const HUB = 'http://127.0.0.1:7399';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };

const hub = spawn('python', ['-m', 'mailhub.serve'], { cwd: MAILHUB, stdio: 'ignore',
  env: { ...process.env, HUB_DATA: mkdtempSync(join(tmpdir(), 'hc-hub-')), HUB_PORT: '7399', HUB_BIND: '127.0.0.1', HUB_NAME: 'linkhub' } });
const app = spawn(exe, [], { stdio: 'ignore', env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9333' } });
let pc, phone;
try {
  for (let i = 0; i < 50; i++) { try { await fetch(HUB + '/healthz'); break; } catch { await sleep(200); } }
  for (let i = 0; i < 60 && !pc; i++) { try { pc = await attach(9333); } catch { await sleep(500); } }
  phone = await attach(9334);
  await sleep(2500);
  const inv = (b, cmd, args = {}) => b.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);
  const shot = (b, n) => Promise.race([b.shot(join(shots, n)).catch(() => {}), sleep(8000)]);

  // Desktop: a fresh identity on the scratch hub.
  check('desktop starts without identity', (await inv(pc, 'hc_state')).me === null);
  check('phone starts without identity', (await inv(phone, 'hc_state')).me === null);
  const addr = await inv(pc, 'hc_create_identity', { id: 'linker', name: 'Link Tester' });
  await inv(pc, 'hc_add_hub', { input: '127.0.0.1:7399' });
  await pc.eval('location.reload()'); await sleep(2500);

  // Phone: link through the hub (through its UI's command, listening for the event).
  const start = await inv(phone, 'hc_link_start', { hub: '127.0.0.1:7399', deviceName: 'Test phone' });
  check('phone shows a link code', /^[0-9A-Z]{4}(-[0-9A-Z]{4}){3}$/.test(start.code), start.code);

  // Desktop: look the code up until the waiting device shows in the directory.
  let look = null;
  for (let i = 0; i < 40; i++) {
    look = await inv(pc, 'hc_link_lookup', { input: start.qr });
    if (look.device_name) break;
    await sleep(3000);
  }
  check('desktop finds the waiting device by name', look?.device_name === 'Test phone', JSON.stringify(look));
  await inv(pc, 'hc_link_approve', { code: start.code });

  let ps = null;
  for (let i = 0; i < 60; i++) { ps = await inv(phone, 'hc_state'); if (ps.me) break; await sleep(1000); }
  check('phone adopted an identity', !!ps?.me);
  check('phone now has the same address', ps.me?.address === addr, `${ps.me?.address} vs ${addr}`);
  check('phone got the profile name', ps.me?.name === 'Link Tester', ps.me?.name);
  check('phone got the hub list', ps.hubs.some((h) => h.url === 'http://127.0.0.1:7399'), JSON.stringify(ps.hubs.map((h) => h.url)));
  await phone.eval('location.reload()'); await sleep(3000);
  await shot(phone, 'link-phone-after.png');
  // The throwaway link address must be gone from the hub's roster.
  const roster = await (await fetch(HUB + '/ui/data')).json().catch(() => null);
  const linkLeft = JSON.stringify(roster || {}).includes(look?.address || '@@');
  check('link address unregistered from the hub', !linkLeft);
} catch (e) {
  check('script ran to the end', false, String(e));
} finally {
  try { await pc?.close(); await phone?.close(); } catch { /* ignore */ }
  spawnSync('taskkill', ['/PID', String(app.pid), '/T', '/F'], { stdio: 'ignore' });
  hub.kill();
  const failed = results.filter((r) => !r).length;
  console.log(`${results.length - failed}/${results.length} passed`);
  process.exit(failed ? 1 : 0);
}
