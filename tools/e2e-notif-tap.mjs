// Tapping a message notification on Android opens its chat (user 2026-10-09
// 06:19Z), on the phone test build (Hubchat Test) against a scratch v1 hub
// reached through adb reverse:
//   1. Hubchat Test in the background: Pat writes, the notification shows,
//      and a real tap on it in the notification shade brings Hubchat Test to
//      the front on Pat's chat;
//   2. its screen gone (its task removed, as swiping it out of Recents does;
//      the connection service still running): Pat writes again, and the tap
//      starts Hubchat Test's screen on Pat's chat.
// Android may fold the message into one group with the "Connected"
// notification; the first tap then only unfolds the group, as for a person,
// and the test taps again (it reports how many taps it took).
// The phone must be awake and unlocked, on its home screen; it is left there.
//
//   needs: MAILHUB_DIR (the v1 hub's folder, run with python); Hubchat Test
//   installed (its data is cleared); adb on PATH or in ADB
//   node tools/e2e-notif-tap.mjs <shots-dir>
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { attach } from './cdp.mjs';

const [shots] = process.argv.slice(2);
const MAILHUB = process.env.MAILHUB_DIR;
if (!MAILHUB || !shots) {
  console.error('usage: MAILHUB_DIR=<v1 hub folder> node tools/e2e-notif-tap.mjs <shots-dir>');
  process.exit(2);
}
mkdirSync(shots, { recursive: true });
const ADB = process.env.ADB || 'adb';
const PKG = 'dev.orgtree.hubchat.test';
const HUB_PORT = 7399;
const HUB = `http://127.0.0.1:${HUB_PORT}`;
const J = JSON.stringify;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const adb = (...a) => spawnSync(ADB, a, { encoding: 'utf8', maxBuffer: 64 << 20, env: { ...process.env, MSYS_NO_PATHCONV: '1' } }).stdout;
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };
const waitUntil = async (f, ms) => { const t = Date.now(); while (Date.now() - t < ms) { if (await f()) return true; await sleep(500); } return false; };
const peer = (...a) => spawnSync('node', [join(import.meta.dirname, 'peer2.mjs'), HUB, ...a], { encoding: 'utf8' }).stdout.trim().split('\n').pop();

/** The resumed activity's package. */
const top = () => (/topResumedActivity=ActivityRecord\{\S+ u0 ([^/\s]+)\//.exec(adb('shell', 'dumpsys', 'activity', 'activities')) || [])[1] || '';
/** Our live message notifications: [{tag, title, text}] */
const ours = () => {
  const d = adb('shell', 'dumpsys', 'notification', '--noredact');
  const list = d.slice(Math.max(0, d.indexOf('Notification List:')));
  const out = [];
  for (const block of list.split(/\n\s*NotificationRecord\(/).slice(1)) {
    if (!block.includes(`pkg=${PKG}`) || !/ id=2 /.test(block)) continue;
    out.push({ tag: (/ tag=(\S+)/.exec(block) || [])[1], title: (/android\.title=String \(([^)]*)\)/.exec(block) || [])[1], text: (/android\.text=String \(([^)]*)\)/.exec(block) || [])[1] });
  }
  return out;
};
/** Opens the notification showing `text` from the shade, as a person would:
 *  tap it, and if Android only unfolded the group it sits in (with the
 *  "Connected" notification), tap it again. The taps it took; 0 if none
 *  opened anything. Taps land only on our own notification's line. */
const openFromShade = async (text) => {
  const nodes = () => [...adb('exec-out', 'uiautomator', 'dump', '/dev/tty').matchAll(/<node [^>]*>/g)].map((m) => m[0]).filter((n) => n.includes('package="com.android.systemui"'));
  const center = (n) => { const b = /bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/.exec(n); return [String((+b[1] + +b[3]) >> 1), String((+b[2] + +b[4]) >> 1)]; };
  adb('logcat', '-c');
  for (let taps = 1; taps <= 3; taps++) {
    adb('shell', 'cmd', 'statusbar', 'expand-notifications');
    await sleep(1500);
    // tap only once the shade has settled: the same place in two dumps
    const node = nodes().find((n) => n.includes(`text="${text}"`));
    await sleep(700);
    const again = node && nodes().find((n) => n.includes(`text="${text}"`));
    if (!again || center(node).join() !== center(again).join()) { console.log(`   tap ${taps}: not on screen`); continue; }
    const [x, y] = center(again);
    console.log(`   tap ${taps}: ${(/resource-id="([^"]*)"/.exec(again) || [])[1]} at ${x},${y}`);
    adb('shell', 'input', 'tap', x, y);
    if (await waitUntil(() => top() === PKG, 4000)) return taps;
  }
  adb('shell', 'cmd', 'statusbar', 'collapse');
  return 0;
};
/** What Android started since the tap (ActivityTaskManager's START lines). */
const starts = () => adb('logcat', '-d', '-s', 'ActivityTaskManager:I').split('\n').filter((l) => l.includes('START u0') && l.includes(PKG)).map((l) => l.replace(/^.*START u0 /, '').slice(0, 160));
const attachApp = async () => {
  let pid = '';
  await waitUntil(() => (pid = adb('shell', 'pidof', PKG).trim()) !== '', 20000);
  adb('forward', 'tcp:9334', `localabstract:webview_devtools_remote_${pid}`);
  let app = null;
  for (let i = 0; i < 40 && !app; i++) { try { app = await attach(9334); } catch { await sleep(500); } }
  if (!app) throw new Error('could not attach to the phone WebView');
  return app;
};
const shoot = (name) => writeFileSync(join(shots, name), spawnSync(ADB, ['exec-out', 'screencap', '-p'], { maxBuffer: 64 << 20 }).stdout);
/** The open chat's header text (the phone's app bar), '' on any other screen. */
const chatOpen = (app) => app.eval(`(document.querySelector('.scr .appbar .who') || {}).innerText || ''`).catch(() => '');
/** Hubchat Test has a screen in a task (its activity exists). */
const hasScreen = () => adb('shell', 'dumpsys', 'activity', 'activities').split('\n').some((l) => /Hist +#\d+: ActivityRecord/.test(l) && l.includes(`${PKG}/`));

if (top() !== 'com.sec.android.app.launcher' && !/launcher/i.test(top())) {
  console.error(`the phone isn't on its home screen (${top()}); not touching it`);
  process.exit(3);
}
const hub = spawn('python', ['-m', 'mailhub.serve'], {
  cwd: MAILHUB, stdio: 'ignore',
  env: { ...process.env, HUB_DATA: mkdtempSync(join(tmpdir(), 'hc-hub-')), HUB_PORT: String(HUB_PORT), HUB_BIND: '127.0.0.1', HUB_NAME: 'e2ehub' },
});
let app;
try {
  for (let i = 0; i < 50; i++) { try { await fetch(HUB + '/healthz'); break; } catch { await sleep(200); } }
  adb('reverse', `tcp:${HUB_PORT}`, `tcp:${HUB_PORT}`);
  adb('shell', 'pm', 'clear', PKG);
  adb('shell', 'pm', 'grant', PKG, 'android.permission.POST_NOTIFICATIONS');
  adb('shell', 'monkey', '-p', PKG, '-c', 'android.intent.category.LAUNCHER', '1');
  app = await attachApp();
  await sleep(2500);
  const inv = (cmd, args = {}) => app.eval(`window.__TAURI_INTERNALS__.invoke(${J(cmd)}, ${J(args)})`);
  // signed in on the scratch hub, staying connected (the commands onboarding calls)
  const me = await inv('hc_create_identity', { id: 'tap', name: 'Tap Tester' });
  await inv('hc_add_hub', { input: HUB });
  await inv('hc_recovery_saved');
  await inv('hc_set_stay_connected', { on: true });
  await app.eval('location.reload()').catch(() => {});
  await sleep(3000);
  app = await attachApp();
  check('signed in and connected', await waitUntil(() => app.eval(`window.__TAURI_INTERNALS__.invoke('hc_state').then((s) => s.hubs.some((h) => h.state === 'connected'))`).catch(() => false), 20000), me);
  const pat = peer('whoami');

  // 1. in the background
  adb('shell', 'input', 'keyevent', 'KEYCODE_HOME');
  await sleep(2000);
  const t1 = 'hello from the tap test';
  peer('send', me, t1);
  check('the notification shows', await waitUntil(() => ours().some((n) => n.tag === pat && n.text === t1), 20000), J(ours()));
  check('Hubchat Test is in the background', top() !== PKG, top());
  const taps1 = await openFromShade(t1);
  check('tapping it in the shade brings Hubchat Test to the front', taps1 > 0, `${taps1} tap(s); top ${top()}`);
  app = await attachApp();
  check("on Pat's chat", await waitUntil(async () => (await chatOpen(app)).includes('Pat Peer'), 10000), J(await chatOpen(app)));
  console.log('   started:', J(starts()), '| tapped chat still waiting:', J(await app.eval(`window.__TAURI_INTERNALS__.invoke('hc_take_pending_chat')`).catch((e) => 'error ' + e)));
  check('and the notification is gone', await waitUntil(() => !ours().some((n) => n.tag === pat), 10000), J(ours()));
  shoot('tap-background.png');

  // 2. the screen gone (its task removed, as swiping it out of Recents does),
  //    the connection service still running
  adb('shell', 'input', 'keyevent', 'KEYCODE_HOME');
  await sleep(1500);
  const task = adb('shell', 'am', 'stack', 'list').split('RootTask id=').find((t) => t.includes(`${PKG}/`));
  if (task) adb('shell', 'am', 'stack', 'remove', task.split(' ')[0]);
  await sleep(2000);
  check('its screen is gone, its connection stays', !hasScreen() && await waitUntil(() => adb('shell', 'dumpsys', 'activity', 'services', PKG).includes('ConnectionService') && adb('shell', 'pidof', PKG).trim() !== '', 15000));
  await sleep(3000); // a restarted service reconnects to the hub
  const t2 = 'and again, with the screen closed';
  peer('send', me, t2);
  check('the notification shows', await waitUntil(() => ours().some((n) => n.tag === pat && n.text === t2), 20000), J(ours()));
  const taps2 = await openFromShade(t2);
  check('tapping it in the shade starts Hubchat Test in front', taps2 > 0, `${taps2} tap(s); top ${top()}`);
  console.log('   started:', J(starts()));
  app = await attachApp();
  check("on Pat's chat", await waitUntil(async () => (await chatOpen(app)).includes('Pat Peer'), 15000), J(await chatOpen(app)));
  shoot('tap-closed.png');

  // 3. two chats waiting: their own group (with its summary), and a tap on
  //    one opens that chat while the other's notification stays
  const bo = (...a) => spawnSync('node', [join(import.meta.dirname, 'peer2.mjs'), HUB, ...a], { encoding: 'utf8', env: { ...process.env, PEER: 'Bo Peer' } }).stdout.trim().split('\n').pop();
  const boSlug = bo('whoami');
  adb('shell', 'input', 'keyevent', 'KEYCODE_HOME');
  await sleep(1500);
  const t3a = 'two chats: from Pat', t3b = 'two chats: from Bo';
  peer('send', me, t3a);
  bo('send', me, t3b);
  check('two chats: both notifications and their summary show', await waitUntil(() => { const o = ours(); return o.some((n) => n.tag === pat && n.text === t3a) && o.some((n) => n.tag === boSlug && n.text === t3b) && o.some((n) => n.tag === 'messages-summary'); }, 20000), J(ours()));
  const taps3 = await openFromShade(t3a);
  check("two chats: tapping Pat's opens Hubchat Test", taps3 > 0, `${taps3} tap(s); top ${top()}`);
  app = await attachApp();
  check("two chats: …on Pat's chat", await waitUntil(async () => (await chatOpen(app)).includes('Pat Peer') && await app.eval(`[...document.querySelectorAll('.bubble')].some((b) => b.innerText.includes(${J(t3a)}))`), 15000), J(await chatOpen(app)));
  check("two chats: Bo's notification stays", ours().some((n) => n.tag === boSlug && n.text === t3b), J(ours()));
  shoot('tap-two-chats.png');
} catch (e) {
  check('ran to the end', false, String(e));
} finally {
  adb('shell', 'cmd', 'statusbar', 'collapse');
  adb('shell', 'input', 'keyevent', 'KEYCODE_HOME');
  try { await app?.close(); } catch {}
  adb('forward', '--remove', 'tcp:9334');
  adb('reverse', '--remove', `tcp:${HUB_PORT}`);
  hub.kill();
}
const passed = results.filter(Boolean).length;
console.log(`${passed}/${results.length} passed`);
process.exit(passed === results.length ? 0 : 1);
