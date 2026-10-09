// The Android in-app update (user 2026-10-09 08:29Z) on the emulator, with
// three Hubchat Test builds (package dev.orgtree.hubchat.test): N, installed
// with adb before the run, and N+1 and N+2, offered with their updater
// signatures by a feed on this PC (a test build may read a feed other than
// GitHub's; adb reverse carries it):
//   1. N offers N+1 in a banner; the first Update asks for Android's "install
//      unknown apps" permission, and Hubchat carries on by itself after it;
//   2. a tampered APK is refused and nothing changes;
//   3. an older signed APK announced as a newer version is refused;
//   4. N+1 installs, and the identity, the chats and the settings are kept;
//      whether Android asks to confirm this first in-app update is shown;
//   5. N+1 -> N+2 installs the same way, and on Android 12+ without a tap:
//      Hubchat is now the app's installer and may update it without asking.
// Hubchat Test's data is cleared at the start; the real Hubchat is never
// touched. Every adb call names its device (ANDROID_SERIAL).
//
//   needs: MAILHUB_DIR (the v1 hub's folder, run with python), ANDROID_SERIAL,
//   adb on PATH or in ADB
//   node tools/e2e-android-update.mjs <apk-dir> <N+1> <N+2> <shots-dir>
//   (<apk-dir> holds Hubchat_<v>_arm64.apk and its .sig for both versions)
import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { attach } from './cdp.mjs';

const [apkDir, V1, V2, shots] = process.argv.slice(2);
const MAILHUB = process.env.MAILHUB_DIR;
const SERIAL = process.env.ANDROID_SERIAL;
if (!MAILHUB || !SERIAL || !shots) {
  console.error('usage: MAILHUB_DIR=<v1 hub folder> ANDROID_SERIAL=<device> node tools/e2e-android-update.mjs <apk-dir> <N+1> <N+2> <shots-dir>');
  process.exit(2);
}
mkdirSync(shots, { recursive: true });
const ADB = process.env.ADB || 'adb';
const PKG = 'dev.orgtree.hubchat.test';
const HUB_PORT = 7399;
const HUB = `http://127.0.0.1:${HUB_PORT}`;
const FEED_PORT = 7420;
const FEED = `http://127.0.0.1:${FEED_PORT}`;
const J = JSON.stringify;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const adb = (...a) => spawnSync(ADB, ['-s', SERIAL, ...a], { encoding: 'utf8', maxBuffer: 64 << 20, env: { ...process.env, MSYS_NO_PATHCONV: '1' } }).stdout || '';
const shot = (name) => writeFileSync(join(shots, name), spawnSync(ADB, ['-s', SERIAL, 'exec-out', 'screencap', '-p'], { maxBuffer: 64 << 20 }).stdout);
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };
const info = (name, detail) => console.log(`INFO ${name} — ${detail}`);
const peer = (...a) => spawnSync('node', [join(import.meta.dirname, 'peer2.mjs'), HUB, ...a], { encoding: 'utf8' }).stdout.trim().split('\n').pop();
const until = async (f, ms) => { const t = Date.now(); while (Date.now() - t < ms) { if (await f()) return true; await sleep(1000); } return false; };

// the installed app, as Android's package manager sees it
const pkgInfo = () => {
  const d = adb('shell', 'dumpsys', 'package', PKG);
  return { version: (d.match(/versionName=(\S+)/) || [])[1] || '', installer: (d.match(/installerPackageName=(\S+)/) || [])[1] || '' };
};
// what is on the screen, from uiautomator
const nodes = () => {
  adb('shell', 'uiautomator', 'dump', '/sdcard/hc-ui.xml');
  const xml = adb('exec-out', 'cat', '/sdcard/hc-ui.xml');
  return [...xml.matchAll(/<node [^>]*>/g)].map(([n]) => {
    const a = (k) => (n.match(new RegExp(` ${k}="([^"]*)"`)) || [])[1] || '';
    const [x0, y0, x1, y1] = (a('bounds').match(/\d+/g) || [0, 0, 0, 0]).map(Number);
    return { text: a('text'), id: a('resource-id'), pkg: a('package'), x: (x0 + x1) >> 1, y: (y0 + y1) >> 1 };
  });
};
const findNode = async (pred, ms) => { const t = Date.now(); while (Date.now() - t < ms) { const n = nodes().find(pred); if (n) return n; await sleep(1000); } return null; };
const tapNode = (n) => adb('shell', 'input', 'tap', String(n.x), String(n.y));
const confirmButton = (n) => /packageinstaller/.test(n.pkg) && /^(update|install)$/i.test(n.text);

// the feed: N+1 and N+2 with their signatures, and a copy of N+1 with one
// byte changed
const apk = (v) => readFileSync(join(apkDir, `Hubchat_${v}_arm64.apk`));
const sig = (v) => readFileSync(join(apkDir, `Hubchat_${v}_arm64.apk.sig`), 'utf8').trim();
const files = { [`/Hubchat_${V1}_arm64.apk`]: apk(V1), [`/Hubchat_${V2}_arm64.apk`]: apk(V2) };
files['/tampered.apk'] = Buffer.from(files[`/Hubchat_${V1}_arm64.apk`]);
files['/tampered.apk'][files['/tampered.apk'].length >> 1] ^= 0xff;
let feed = null;
const offer = (version, file, signature) => {
  feed = { version, notes: 'The update test.', pub_date: new Date().toISOString(), platforms: { 'android-aarch64': { url: FEED + file, signature } } };
};
const server = createServer((req, res) => {
  const path = req.url.split('?')[0];
  if (path === '/latest.json') { res.writeHead(200, { 'content-type': 'application/json' }); res.end(J(feed)); return; }
  const f = files[path];
  if (!f) { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { 'content-type': 'application/vnd.android.package-archive', 'content-length': f.length });
  res.end(f);
});
await new Promise((r) => server.listen(FEED_PORT, '127.0.0.1', r));

const hub = spawn('python', ['-m', 'mailhub.serve'], {
  cwd: MAILHUB, stdio: 'ignore',
  env: { ...process.env, HUB_DATA: mkdtempSync(join(tmpdir(), 'hc-hub-')), HUB_PORT: String(HUB_PORT), HUB_BIND: '127.0.0.1', HUB_NAME: 'updhub' },
});
let b;
const attachApp = async () => {
  let pid = '';
  for (let i = 0; i < 40 && !(pid = adb('shell', 'pidof', PKG).trim()); i++) await sleep(500);
  // after an update the process may be only the background connection, which has
  // no WebView: its debug socket appears once the app's window has made one
  await until(() => adb('shell', 'cat', '/proc/net/unix').includes(`webview_devtools_remote_${pid}`), 30000);
  adb('forward', 'tcp:9336', `localabstract:webview_devtools_remote_${pid}`);
  try { await b?.close(); } catch {}
  b = null;
  for (let i = 0; i < 40 && !b; i++) { try { b = await attach(9336); } catch { await sleep(500); } }
  if (!b) throw new Error('could not attach to Hubchat Test');
};
const inFront = () => /topResumedActivity=.* dev\.orgtree\.hubchat\.test\//.test(adb('shell', 'dumpsys', 'activity', 'activities'));
// Right after an update Android is still finishing the replacement, and a launch then
// goes nowhere: launch again until Hubchat Test is the app in front.
const launch = async () => {
  for (let i = 0; i < 6 && !inFront(); i++) {
    adb('shell', 'monkey', '-p', PKG, '-c', 'android.intent.category.LAUNCHER', '1');
    await until(inFront, 5000);
  }
  await attachApp();
  await sleep(2000);
};
const reload = async () => { await b.eval('location.reload()').catch(() => {}); await sleep(2500); await attachApp(); };
const waitFor = async (expr, ms = 15000) => { const t = Date.now(); while (Date.now() - t < ms) { if (await b.eval(expr).catch(() => false)) return true; await sleep(300); } return false; };
const banner = () => b.eval(`(document.querySelector('.banner.upd') || {}).innerText || ''`).catch(() => '');
const bannerHas = (text, ms) => waitFor(`(document.querySelector('.banner.upd') || {}).innerText?.includes(${J(text)})`, ms);
const tapBanner = () => b.eval(`(() => { const x = document.querySelector('.banner.upd button.btn'); if (!x) return false; x.click(); return true; })()`);
const inv = (cmd, args = {}) => b.eval(`window.__TAURI_INTERNALS__.invoke(${J(cmd)}, ${J(args)})`);
const state = () => b.eval(`(async () => {
  const s = await window.__TAURI_INTERNALS__.invoke('hc_state');
  const chats = await window.__TAURI_INTERNALS__.invoke('hc_chats').catch(() => []);
  return { me: s.me && s.me.address, connected: s.hubs.some((h) => h.state === 'connected'), chats: chats.map((c) => (c.last && c.last.body) || ''),
    kept: localStorage.getItem('hubchat.e2e.kept'), theme: localStorage.getItem('hubchat.theme'), feed: localStorage.getItem('hubchat.updates.feed'),
    ver: (document.querySelector('.appbar .title .ver') || {}).innerText || '' };
})()`);

// after an update: Hubchat starts again with everything it had
const keptAfter = async (v, before) => {
  info(`after the update to ${v}, before Hubchat is opened`, `its process ${adb('shell', 'pidof', PKG).trim() ? 'is running (the connection came back by itself)' : 'is not running'}`);
  await launch();
  const ok = await waitFor(`window.__TAURI_INTERNALS__.invoke('hc_state').then((s) => s.hubs.some((h) => h.state === 'connected'))`, 30000);
  const s = await state();
  check(`${v}: the version beside the title`, s.ver === 'v' + v, J(s.ver));
  check(`${v}: the same identity, and it connects`, ok && s.me === before.me, J({ me: s.me, connected: s.connected }));
  check(`${v}: the chat and its message are still there`, s.chats.some((c) => c.includes('Before the update')), J(s.chats));
  check(`${v}: the settings are kept (theme, the test feed, a marker)`, s.kept === 'yes' && s.theme === 'light' && s.feed === FEED + '/latest.json', J({ kept: s.kept, theme: s.theme, feed: s.feed }));
};

// Update in the banner, then wait for the install. Android shows its own
// confirmation unless Hubchat may update itself without one; a confirmation is
// tapped when it appears, and whether one appeared is returned.
const SDK = Number(adb('shell', 'getprop', 'ro.build.version.sdk').trim());
const installOffered = async (v, confirmShot) => {
  await tapBanner();
  let tapped = false;
  const done = await until(async () => {
    if (pkgInfo().version === v) return true;
    const n = nodes().find(confirmButton);
    if (n) { tapped = true; shot(confirmShot); tapNode(n); }
    return false;
  }, 180000);
  return { done, tapped };
};

try {
  const N = pkgInfo();
  check('Hubchat Test N is installed, older than both updates', !!N.version && N.version !== V1 && N.version !== V2, J(N));
  adb('reverse', `tcp:${HUB_PORT}`, `tcp:${HUB_PORT}`);
  adb('reverse', `tcp:${FEED_PORT}`, `tcp:${FEED_PORT}`);
  for (let i = 0; i < 50; i++) { try { await fetch(HUB + '/healthz'); break; } catch { await sleep(200); } }
  peer('whoami');
  adb('shell', 'pm', 'clear', PKG);
  adb('shell', 'appops', 'set', PKG, 'REQUEST_INSTALL_PACKAGES', 'default');
  adb('shell', 'pm', 'grant', PKG, 'android.permission.POST_NOTIFICATIONS');
  check('Hubchat Test may not install apps yet', !/allow/.test(adb('shell', 'appops', 'get', PKG, 'REQUEST_INSTALL_PACKAGES')));

  // N with an identity, a chat and settings
  offer(V1, '/tampered.apk', sig(V1));
  await launch();
  const me = await inv('hc_create_identity', { id: 'upd', name: 'Update Tester' });
  await inv('hc_add_hub', { input: HUB });
  await inv('hc_recovery_saved');
  await b.eval(`localStorage.setItem('hubchat.updates.feed', ${J(FEED + '/latest.json')}); localStorage.setItem('hubchat.e2e.kept', 'yes'); localStorage.setItem('hubchat.theme', 'light'); true`);
  peer('send', me, 'Before the update');
  await reload();
  check('N: signed in and connected', await waitFor(`window.__TAURI_INTERNALS__.invoke('hc_state').then((s) => !!s.me && s.hubs.some((h) => h.state === 'connected'))`, 30000));
  check("N: Pat's message arrives", await waitFor(`!!document.querySelector('.crow')`, 70000));
  const before = await state();

  // 1. the offer, and the permission the first time
  check(`1. the banner offers ${V1}`, await bannerHas(`Hubchat ${V1} is ready`, 30000), await banner());
  shot('upd-1-offer.png');
  await tapBanner();
  check('1. the first Update asks for the permission', await bannerHas('Allow Hubchat to install its updates', 15000), await banner());
  shot('upd-2-permission.png');
  await tapBanner();
  const allow = await findNode((n) => /allow from this source/i.test(n.text), 15000);
  check("1. Allow opens Android's Install unknown apps setting for Hubchat Test", !!allow);
  shot('upd-3-setting.png');
  if (allow) tapNode(allow);
  check('1. the setting is on', await until(() => /allow/.test(adb('shell', 'appops', 'get', PKG, 'REQUEST_INSTALL_PACKAGES')), 10000));
  adb('shell', 'input', 'keyevent', 'KEYCODE_BACK');

  // 2. back in Hubchat it carries on, with the tampered copy
  await sleep(1500);
  await attachApp();
  check('2. back in Hubchat, the update carries on by itself and the tampered APK is refused',
    await bannerHas("The update didn't install. The download isn't signed by Hubchat's update key.", 90000), await banner());
  shot('upd-4-tampered.png');
  check('2. nothing changed', pkgInfo().version === N.version, J(pkgInfo()));

  // 3. N+1's signed APK announced as N+2
  offer(V2, `/Hubchat_${V1}_arm64.apk`, sig(V1));
  await reload();
  check(`3. the banner offers ${V2}`, await bannerHas(`Hubchat ${V2} is ready`, 30000), await banner());
  await tapBanner();
  check(`3. ${V1}'s APK announced as ${V2} is refused`, await bannerHas(`The update didn't install. The download isn't signed as Hubchat ${V2}.`, 90000), await banner());
  shot('upd-5-wrong-version.png');
  check('3. nothing changed', pkgInfo().version === N.version, J(pkgInfo()));

  // 4. N+1
  offer(V1, `/Hubchat_${V1}_arm64.apk`, sig(V1));
  await reload();
  check(`4. the banner offers ${V1}`, await bannerHas(`Hubchat ${V1} is ready`, 30000), await banner());
  const first = await installOffered(V1, 'upd-6-confirm.png');
  check(`4. ${V1} is installed`, first.done, J(pkgInfo()));
  info('4. Android wanted a tap for the first in-app update', first.tapped ? 'yes' : 'no');
  check('4. Hubchat Test is now its own installer', pkgInfo().installer === PKG, J(pkgInfo()));
  offer(V2, `/Hubchat_${V2}_arm64.apk`, sig(V2));
  await keptAfter(V1, before);
  shot('upd-7-after-first.png');

  // 5. N+1 -> N+2
  check(`5. the banner offers ${V2}`, await bannerHas(`Hubchat ${V2} is ready`, 30000), await banner());
  const second = await installOffered(V2, 'upd-8-second-confirm.png');
  check(`5. ${V2} is installed`, second.done, J(pkgInfo()));
  if (SDK >= 31) check('5. Android 12+ installs it without asking for a tap', !second.tapped);
  else info('5. Android wanted a tap for the second in-app update', second.tapped ? 'yes' : 'no');
  await keptAfter(V2, before);
  shot('upd-9-after-second.png');
} catch (e) {
  check('no error', false, String(e && e.stack || e));
} finally {
  try { await b?.close(); } catch {}
  adb('forward', '--remove', 'tcp:9336');
  adb('reverse', '--remove', `tcp:${HUB_PORT}`);
  adb('reverse', '--remove', `tcp:${FEED_PORT}`);
  server.close();
  hub.kill();
  const pass = results.filter(Boolean).length;
  console.log(`${pass}/${results.length} passed`);
  process.exit(pass === results.length ? 0 : 1);
}
