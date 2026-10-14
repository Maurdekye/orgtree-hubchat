// A Hubchat hidden in the tray reads nothing (user 2026-10-14: the phone's
// notifications sounded and vanished, because the PC Hubchat, hidden after a
// sign-in start, still counted as on screen and marked each new message in
// its reopened chat read; read on any device takes the phone's notification
// away). Tauri's window hide() leaves WebView2's IsVisible on, so the page
// stayed "visible"; desktop::hide()/show() now hide and show the page too.
//
// Desktop test build under its own identifier (tools/e2e-hidden-read.conf.json,
// so neither the real Hubchat nor an installed Hubchat Test is touched),
// against a scratch hub, driven through WebView2's debug port. A sign-in start
// left the hidden page with focus; the test gives it focus the same way every
// time (CDP focus emulation), so only the page's visibility decides.
//   0. an identity on the hub; a normal start: Pat's first message, Pat's
//      chat opened (the chat a restart reopens)
//   1. a --hidden start (Start with Windows): the page says hidden, and
//      Pat's chat is open again
//   2. the page given focus: Pat's next message stays unread
//   3. a second launch shows the window: the page says visible, and the open
//      chat is read
//   4. closed to the tray: the page says hidden; Pat's next message stays unread
//   5. shown, focused and untouched past the in-use limit (150 s): Pat's next
//      message stays unread (IDLE_STOPS_READING on; IDLE_READS=1 expects it
//      read instead); a mouse move then reads it. E2E_IDLE=0 skips this step.
//
//   needs: the e2e build, from the worktree:
//     npx tauri build --debug --no-bundle --config src-tauri/tauri.test.conf.json
//       --config tools/e2e-hidden-read.conf.json
//   and a scratch hub (the mail hub binary) at <hub host:port>
//   node tools/e2e-hidden-read.mjs <hubchat.exe e2e build> <hub host:port> <shots dir>
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { attach } from './cdp.mjs';

const [exeArg, hubArg, shots] = process.argv.slice(2);
if (!exeArg || !hubArg || !shots) {
  console.error('usage: node tools/e2e-hidden-read.mjs <hubchat.exe e2e build> <hub host:port> <shots dir>');
  process.exit(2);
}
const EXE = resolve(exeArg);
const HUB = 'http://' + hubArg.replace(/^https?:\/\//, '').replace(/\/$/, '');
mkdirSync(shots, { recursive: true });
const CONF = JSON.parse(readFileSync(join(import.meta.dirname, 'e2e-hidden-read.conf.json'), 'utf8'));
const ID = CONF.identifier;
const DATA = join(process.env.APPDATA, ID);
const WEBVIEW = join(process.env.LOCALAPPDATA, ID);
const TARGET = `identity:${DATA}.dev.orgtree.hubchat`;
const VERSION = JSON.parse(readFileSync(join(import.meta.dirname, '..', 'src-tauri', 'tauri.conf.json'), 'utf8')).version;
const IDLE = process.env.E2E_IDLE !== '0';
const IDLE_READS = process.env.IDLE_READS === '1';
const CDP = 9241;
const J = JSON.stringify;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };
const info = (name, detail) => console.log(`INFO ${name} — ${detail}`);
const cdpEnv = { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP}` };
const plainEnv = { ...process.env };
delete plainEnv.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS;
const sh = (cmd, args) => spawnSync(cmd, args, { encoding: 'utf8' }).stdout || '';
const peer = (...a) => spawnSync('node', [join(import.meta.dirname, 'peer2.mjs'), HUB, ...a], { encoding: 'utf8', env: { ...process.env, PEER: 'Pat Hidden' } }).stdout.trim().split('\n').pop();

// only this build's own data, by exact name
if (!ID.endsWith('.test') || ID === 'dev.orgtree.hubchat.test') throw new Error('not the e2e identifier: ' + ID);
const credExists = () => /Target:/.test(sh('cmdkey', [`/list:${TARGET}`]));
const wipe = () => {
  if (credExists()) sh('cmdkey', [`/delete:${TARGET}`]);
  for (const p of [DATA, WEBVIEW]) { if (!p.endsWith(`\\${ID}`)) throw new Error('not the e2e folder: ' + p); rmSync(p, { recursive: true, force: true }); }
};

let app = null;
let b = null;
const until = async (f, ms) => { const t = Date.now(); while (Date.now() - t < ms) { if (await f().catch(() => false)) return true; await sleep(300); } return false; };
const start = async (args) => {
  app = spawn(EXE, args, { stdio: 'ignore', env: cdpEnv });
  b = null;
  for (let i = 0; i < 60 && !b; i++) { try { b = await attach(CDP); } catch { await sleep(500); } }
  if (!b) throw new Error('could not attach to the e2e build');
  if (!await until(() => b.eval(`typeof window.__TAURI_INTERNALS__?.invoke === 'function'`), 20000)) throw new Error("the e2e build's page never loaded");
  const id = await b.eval(`window.__TAURI_INTERNALS__.invoke('plugin:app|identifier')`);
  if (id !== ID) throw new Error('this is not the e2e build: ' + id);
  await until(() => b.eval(`window.__TAURI_INTERNALS__.invoke('hc_state').then(() => true)`), 20000);
};
const stop = async () => {
  // a clean exit, so the page's storage (the chat to reopen) is written
  try { await inv('plugin:process|exit', { code: 0 }); } catch { /* the page goes with it */ }
  try { await b?.close(); } catch {}
  b = null;
  if (app && !await until(async () => app.exitCode !== null, 10000)) {
    spawnSync('taskkill', ['/pid', String(app.pid), '/T', '/F']);
    await until(async () => app.exitCode !== null, 10000);
  }
  app = null;
  await until(async () => { try { await fetch(`http://127.0.0.1:${CDP}/json/version`); return false; } catch { return true; } }, 10000);
};
const inv = (cmd, args = {}) => b.eval(`window.__TAURI_INTERNALS__.invoke(${J(cmd)}, ${J(args)})`);
const page = () => b.eval(`({ visibility: document.visibilityState, focus: document.hasFocus() })`);
const openChat = () => b.eval(`(document.querySelector('.conv-head') || {}).innerText || ''`);
// the focus a sign-in start left the hidden page with; the focus event makes
// the UI read it again (useForeground listens for focus and blur)
const giveFocus = async () => {
  await b.send('Emulation.setFocusEmulationEnabled', { enabled: true });
  await b.eval(`window.dispatchEvent(new Event('focus')), true`);
  await sleep(500);
};
const send = (text) => JSON.parse(peer('send', me, text)).sent;
const arrived = (id) => until(() => inv('hc_message', { id }).then((m) => !!m), 20000);
const seen = async (id) => (await inv('hc_message', { id }))?.seen === true;
const staysUnread = async (id, ms = 4000) => { await sleep(ms); return !(await seen(id)); };
const isPat = (t) => t.includes('Pat Hidden') || (!!pat && t.includes(pat));
const shot = async (name) => { try { await Promise.race([b.shot(join(shots, name)), sleep(8000)]); } catch { /* a hidden page may not paint */ } };
let me = '';
let pat = '';

try {
  wipe();
  // 0. an identity on the hub, made through the API: onboarding ends with
  // the next start, a normal one (the window shows)
  await start([]);
  me = await inv('hc_create_identity', { id: 'hiddenread', name: 'Hidden Read' });
  await inv('hc_add_hub', { input: HUB });
  await stop();
  await start([]);
  check('0. signed in and connected', await until(() => inv('hc_state').then((s) => !!s.me && s.hubs.some((h) => h.state === 'connected')), 20000), me);
  pat = peer('whoami');
  const first = send('hello, open my chat');
  check("0. Pat's first message arrived", await arrived(first), pat);
  const opened = await b.eval(`(() => { const r = [...document.querySelectorAll('.crow')].find((e) => e.innerText.includes('Pat Hidden') || e.innerText.includes(${J(pat)})); if (r) r.click(); return !!r; })()`);
  check("0. Pat's chat opens", opened && await until(async () => isPat(await openChat()), 5000), J(await openChat()));
  info('0. a normal start', J(await page()));
  await sleep(3000);
  await stop();

  // 1. Start with Windows: --hidden, not the first start of this version
  check('1. this version has run before (so --hidden stays hidden)', existsSync(join(DATA, 'last-run-version')) && readFileSync(join(DATA, 'last-run-version'), 'utf8').trim() === VERSION);
  await start(['--hidden']);
  await sleep(1500);
  const p1 = await page();
  check('1. a --hidden start: the page says hidden', p1.visibility === 'hidden', J(p1));
  check("1. Pat's chat is open again", await until(async () => isPat(await openChat()), 8000), J(await openChat()));

  // 2. with the focus a sign-in start leaves, Pat writes
  await giveFocus();
  info('2. the hidden page, given focus', J(await page()));
  const second = send('are you there? (hidden)');
  check("2. Pat's message arrived", await arrived(second));
  check('2. hidden, it stays unread', await staysUnread(second), `seen ${await seen(second)}`);
  await shot('2-hidden.png');

  // 3. a second launch (the Start menu, a tray click) shows the window
  const relaunch = spawnSync(EXE, [], { env: plainEnv, timeout: 30000 });
  info('3. the second launch exited', `status ${relaunch.status}`);
  check('3. shown: the page says visible', await until(async () => (await page()).visibility === 'visible', 8000), J(await page()));
  check('3. shown: the open chat is read', await until(() => seen(second), 8000));
  await shot('3-shown.png');

  // 4. the window's close button: to the tray
  await inv('plugin:window|close', { label: 'main' });
  check('4. closed to the tray: the page says hidden', await until(async () => (await page()).visibility === 'hidden', 8000), J(await page()));
  const fourth = send('still there? (closed)');
  check("4. Pat's message arrived", await arrived(fourth));
  check('4. in the tray, it stays unread', await staysUnread(fourth), `seen ${await seen(fourth)}`);

  // 5. on screen, focused and left alone past the in-use limit
  if (IDLE) {
    spawnSync(EXE, [], { env: plainEnv, timeout: 30000 });
    check('5. shown again: the page says visible', await until(async () => (await page()).visibility === 'visible', 8000), J(await page()));
    check('5. shown again: the open chat is read', await until(() => seen(fourth), 8000));
    info('5. untouched for 170 s', 'no input reaches the page meanwhile');
    await sleep(170_000);
    const fifth = send('anyone at the PC?');
    check("5. Pat's message arrived", await arrived(fifth));
    if (IDLE_READS) check('5. untouched (IDLE_READS=1): it is read', await until(() => seen(fifth), 4000));
    else check('5. untouched past the in-use limit: it stays unread', await staysUnread(fifth), `seen ${await seen(fifth)}`);
    await b.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 300, y: 300 });
    check('5. a mouse move: it is read', await until(() => seen(fifth), 5000));
    await shot('5-touched.png');
  }
} catch (e) {
  check('ran to the end', false, String(e));
} finally {
  try { await stop(); } catch {}
  try { wipe(); } catch (e) { info('cleanup', String(e)); }
  check('cleanup: the e2e identity and folders are gone', !credExists() && !existsSync(DATA) && !existsSync(WEBVIEW));
}
const passed = results.filter(Boolean).length;
console.log(`${passed}/${results.length} passed`);
writeFileSync(join(shots, 'result.json'), J({ passed, total: results.length }));
process.exit(passed === results.length ? 0 : 1);
