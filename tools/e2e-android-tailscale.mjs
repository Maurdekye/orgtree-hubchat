// The Tailscale-off notice on the emulator (user 2026-10-10 06:44Z:
// "tailscale on my phone turned itself off on its own"), both ways, in
// Hubchat Test with the Tailscale stand-in (tools/android-test-vpn: package
// com.tailscale.ipn, a VPN that routes only 100.64.0.0/10, like Tailscale's).
// Whether a VPN is up is read from the emulator itself (its tun0), not from
// Hubchat.
//   1. a hub at a tailnet address (100.101.102.103, unreachable here), no VPN:
//      the hub notice says Tailscale seems to be off;
//   2. Open Tailscale opens "Tailscale", whose VPN comes on: Hubchat sees it,
//      and the notice turns into the usual one (the hub is still down);
//   3. the VPN off again: the Tailscale notice is back;
//   4. a hub outside the tailnet (10.0.2.99, unreachable), no VPN: only the
//      usual notice;
//   5. a hub reached only with the VPN on (a local test hub, adb reverse):
//      once it is gone with the VPN off, the Tailscale notice; once it is
//      back, no notice at all, and it is no longer blamed on Tailscale;
//   6. without Tailscale installed, Open Tailscale opens its store page (or
//      whatever takes that link here); the stand-in is installed again after.
// Hubchat Test's data is cleared at the start; the real Hubchat is never
// touched. Every adb call names its device (ANDROID_SERIAL).
//   needs: MAILHUB_DIR (the v1 hub's folder, run with python), ANDROID_SERIAL,
//   adb on PATH or in ADB; Hubchat Test and the stand-in installed
//   node tools/e2e-android-tailscale.mjs <stand-in apk> <shots-dir>
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { attach } from './cdp.mjs';

const [standIn, shots] = process.argv.slice(2);
const MAILHUB = process.env.MAILHUB_DIR;
const SERIAL = process.env.ANDROID_SERIAL;
if (!MAILHUB || !SERIAL || !shots) {
  console.error('usage: MAILHUB_DIR=<v1 hub folder> ANDROID_SERIAL=<device> node tools/e2e-android-tailscale.mjs <stand-in apk> <shots-dir>');
  process.exit(2);
}
mkdirSync(shots, { recursive: true });
const ADB = process.env.ADB || 'adb';
const PKG = 'dev.orgtree.hubchat.test';
const TSPKG = 'com.tailscale.ipn';
const STANDIN = `${TSPKG}/dev.orgtree.hubchat.vpntest.Main`;
const TAILNET = 'http://100.101.102.103:7371';
const OUTSIDE = 'http://10.0.2.99:7371';
const HUB_PORT = 7396;
const LOCAL = `http://127.0.0.1:${HUB_PORT}`;
const TS = 'Tailscale seems to be off. Your hub is only reachable through it.';
const J = JSON.stringify;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const adb = (...a) => spawnSync(ADB, ['-s', SERIAL, ...a], { encoding: 'utf8', maxBuffer: 64 << 20, env: { ...process.env, MSYS_NO_PATHCONV: '1' } }).stdout || '';
const shot = (name) => writeFileSync(join(shots, name), spawnSync(ADB, ['-s', SERIAL, 'exec-out', 'screencap', '-p'], { maxBuffer: 64 << 20 }).stdout);
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };
const until = async (f, ms) => { const t = Date.now(); while (Date.now() - t < ms) { if (await f()) return true; await sleep(500); } return false; };

let b;
const attachApp = async () => {
  let pid = '';
  for (let i = 0; i < 40 && !(pid = adb('shell', 'pidof', PKG).trim()); i++) await sleep(500);
  await until(() => adb('shell', 'cat', '/proc/net/unix').includes(`webview_devtools_remote_${pid}`), 30000);
  adb('forward', 'tcp:9337', `localabstract:webview_devtools_remote_${pid}`);
  try { await b?.close(); } catch {}
  b = null;
  for (let i = 0; i < 40 && !b; i++) { try { b = await attach(9337); } catch { await sleep(500); } }
  if (!b) throw new Error('could not attach to Hubchat Test');
};
const top = () => (adb('shell', 'dumpsys', 'activity', 'activities').match(/topResumedActivity=\S+ \S+ (\S+?)\//) || [])[1] || '';
const launch = async () => {
  for (let i = 0; i < 6 && top() !== PKG; i++) {
    adb('shell', 'monkey', '-p', PKG, '-c', 'android.intent.category.LAUNCHER', '1');
    await until(() => top() === PKG, 5000);
  }
  await attachApp();
  await sleep(1500);
};
const inv = (cmd, args = {}) => b.eval(`window.__TAURI_INTERNALS__.invoke(${J(cmd)}, ${J(args)})`);
const hubState = async (url) => ((await inv('hc_state')).hubs.find((h) => h.url === url) || {}).state;
// the hub notice as shown: its words, and whether it is the Tailscale one
const notice = () => b.eval(`(() => { const e = document.querySelector('[aria-label="Hub notice"]'); return e ? { text: e.innerText.trim().replace(/\\s+/g, ' '), why: e.dataset.why || '' } : null; })()`).catch(() => null);
const noticeIs = (pred, ms) => until(async () => pred(await notice()), ms);
const isTs = (n) => !!n && n.why === 'tailscale' && n.text.startsWith(TS);
const isUsual = (n) => !!n && !n.why && /^Can't reach hub/.test(n.text) && !n.text.includes('Tailscale');
const reach = () => b.eval(`JSON.parse(localStorage.getItem('hubchat.hubs.reach') || '{}')`);
const tapOpen = () => b.eval(`(() => { const x = [...document.querySelectorAll('[aria-label="Hub notice"] button')].find((x) => x.innerText.includes('Open Tailscale')); if (x) x.click(); return !!x; })()`);
const starts = () => adb('logcat', '-d', '-s', 'ActivityTaskManager:I').split('\n').filter((l) => /START/.test(l));
// the emulator's own answer: the stand-in's VPN interface
const tunUp = () => /inet 100\.90\.1\.2/.test(adb('shell', 'ip', 'addr', 'show', 'tun0'));
const tunRoutes = () => adb('shell', 'ip', 'route', 'show', 'table', 'all').split('\n').filter((l) => /tun0/.test(l)).slice(0, 3).join(' | ');
const vpn = async (on) => {
  adb('shell', 'am', 'start', '-n', STANDIN, '--es', 'cmd', on ? 'on' : 'off');
  const ok = await until(() => tunUp() === on, 15000);
  await launch(); // the stand-in's page was in front for a moment
  return ok;
};

let hub = null;
const hubData = mkdtempSync(join(tmpdir(), 'hc-tshub-'));
const startHub = async () => {
  hub = spawn('python', ['-m', 'mailhub.serve'], {
    cwd: MAILHUB, stdio: 'ignore',
    env: { ...process.env, HUB_DATA: hubData, HUB_PORT: String(HUB_PORT), HUB_BIND: '127.0.0.1', HUB_NAME: 'tshub' },
  });
  for (let i = 0; i < 50; i++) { try { await fetch(LOCAL + '/healthz'); return; } catch { await sleep(200); } }
};
const stopHub = () => { if (hub) { spawnSync('taskkill', ['/pid', String(hub.pid), '/T', '/F']); hub = null; } };

try {
  check('Hubchat Test and the Tailscale stand-in are installed', adb('shell', 'pm', 'path', PKG).includes('package:') && adb('shell', 'pm', 'path', TSPKG).includes('package:'));
  adb('shell', 'appops', 'set', TSPKG, 'ACTIVATE_VPN', 'allow');
  adb('shell', 'am', 'start', '-n', STANDIN, '--es', 'cmd', 'off');
  check('the VPN is off to begin with', await until(() => !tunUp(), 15000));
  adb('shell', 'pm', 'clear', PKG);
  adb('shell', 'pm', 'grant', PKG, 'android.permission.POST_NOTIFICATIONS');
  await launch();
  await inv('hc_create_identity', { id: 'tsnotice', name: 'Tailscale Tester' });
  await inv('hc_recovery_saved');
  await b.eval('location.reload()').catch(() => {});
  await sleep(2500);
  await attachApp();

  // 1. a hub at a tailnet address, no VPN
  await inv('hc_add_hub', { input: TAILNET });
  check("1. the tailnet hub can't be reached", await until(async () => (await hubState(TAILNET)) === 'disconnected', 90000), J(await hubState(TAILNET)));
  check('1. Hubchat sees no VPN', (await inv('hc_vpn_active')) === false, J(await inv('hc_vpn_active')));
  check(`1. no VPN + a tailnet hub down -> "${TS}"`, await noticeIs(isTs, 15000), J(await notice()));
  check('1. its one button is Open Tailscale', /Open Tailscale$/.test((await notice())?.text || ''), J(await notice()));
  shot('ts-1-off.png');

  // 2. Open Tailscale: "Tailscale" opens and its VPN comes on; the hub is still unreachable
  adb('logcat', '-c');
  await tapOpen();
  check('2. Open Tailscale opens the Tailscale app (the stand-in)', await until(() => starts().some((l) => l.includes(`cmp=${STANDIN}`)), 10000), starts().slice(-1)[0] || '');
  check('2. ...whose VPN comes on (tun0 100.90.1.2, a route for 100.64.0.0/10 only)', await until(tunUp, 15000), tunRoutes());
  await launch();
  check('2. Hubchat sees the VPN, though it routes only 100.64.0.0/10', await until(async () => (await inv('hc_vpn_active')) === true, 10000), J(await inv('hc_vpn_active')));
  const t2 = Date.now();
  check('2. VPN on + the tailnet hub still down -> only the usual notice', await noticeIs(isUsual, 12000), `${J(await notice())} after ${Date.now() - t2} ms`);
  check('2. the hub is still unreachable (the notice is about Tailscale only)', (await hubState(TAILNET)) !== 'connected', J(await hubState(TAILNET)));
  shot('ts-2-vpn-on.png');

  // 3. the VPN off again
  check('3. the VPN is down', await vpn(false));
  const t3 = Date.now();
  check('3. VPN off again -> the Tailscale notice is back', await noticeIs(isTs, 12000), `${J(await notice())} after ${Date.now() - t3} ms`);

  // 4. a hub outside the tailnet, no VPN
  await inv('hc_remove_hub', { url: TAILNET, unregister: false });
  await inv('hc_add_hub', { input: OUTSIDE });
  check("4. the hub outside the tailnet can't be reached", await until(async () => (await hubState(OUTSIDE)) === 'disconnected', 90000), J(await hubState(OUTSIDE)));
  await sleep(6000);
  check('4. no VPN + a hub outside the tailnet down -> only the usual notice', isUsual(await notice()), J(await notice()));
  shot('ts-3-outside.png');
  await inv('hc_remove_hub', { url: OUTSIDE, unregister: false });

  // 5. a hub reached only with the VPN on, then gone, then back
  await startHub();
  adb('reverse', `tcp:${HUB_PORT}`, `tcp:${HUB_PORT}`);
  check('5. the VPN is up', await vpn(true));
  await inv('hc_add_hub', { input: LOCAL });
  check('5. the local hub connects', await until(async () => (await hubState(LOCAL)) === 'connected', 60000), J(await hubState(LOCAL)));
  check('5. ...and Hubchat notes it was reached with a VPN on', await until(async () => (await reach())[LOCAL] === 'vpn', 10000), J(await reach()));
  check('5. the VPN is down; the hub stays connected', (await vpn(false)) && (await hubState(LOCAL)) === 'connected', J(await hubState(LOCAL)));
  check('5. no notice while it is connected', (await notice()) === null, J(await notice()));
  stopHub();
  check('5. the hub is gone: Hubchat loses it', await until(async () => (await hubState(LOCAL)) === 'disconnected', 90000), J(await hubState(LOCAL)));
  check('5. no VPN + a hub reached only with a VPN before -> the Tailscale notice', await noticeIs(isTs, 15000), J(await notice()));
  shot('ts-4-memory.png');
  await startHub();
  const t5 = Date.now();
  const back = await until(async () => (await hubState(LOCAL)) === 'connected', 180000);
  check('5. the hub is back: Hubchat reconnects by itself', back, `${J(await hubState(LOCAL))} after ${Math.round((Date.now() - t5) / 1000)} s`);
  check('5. ...and the notice is gone by itself', await noticeIs((n) => n === null, 10000), J(await notice()));
  check('5. a hub reached without a VPN is no longer blamed on Tailscale', (await reach())[LOCAL] === 'direct', J(await reach()));
  shot('ts-5-back.png');

  // 6. Tailscale not installed: Open Tailscale opens its store page
  await inv('hc_remove_hub', { url: LOCAL, unregister: false });
  adb('uninstall', TSPKG);
  await inv('hc_add_hub', { input: TAILNET });
  await until(async () => (await hubState(TAILNET)) === 'disconnected', 90000);
  check('6. without Tailscale installed, the Tailscale notice still shows', await noticeIs(isTs, 15000), J(await notice()));
  adb('logcat', '-c');
  await tapOpen();
  const elsewhere = await until(() => top() && top() !== PKG, 10000);
  check('6. Open Tailscale then opens its store page, or what takes that link here', elsewhere, `${top()} · ${starts().slice(-1)[0] || ''}`);
  shot('ts-6-store.png');
  adb('shell', 'input', 'keyevent', 'KEYCODE_BACK');
} catch (e) {
  check('no error', false, String(e && e.stack || e));
} finally {
  try { await b?.close(); } catch {}
  if (!adb('shell', 'pm', 'path', TSPKG).includes('package:')) adb('install', '-r', standIn);
  adb('shell', 'appops', 'set', TSPKG, 'ACTIVATE_VPN', 'allow');
  adb('shell', 'am', 'start', '-n', STANDIN, '--es', 'cmd', 'off');
  adb('forward', '--remove', 'tcp:9337');
  adb('reverse', '--remove', `tcp:${HUB_PORT}`);
  stopHub();
  const pass = results.filter(Boolean).length;
  console.log(`${pass}/${results.length} passed`);
  process.exit(pass === results.length ? 0 : 1);
}
