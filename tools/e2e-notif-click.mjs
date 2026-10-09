// Clicking a message notification opens its chat (user 2026-10-09 06:19Z).
// On Windows the toast opens hubchat://chat?to=<sender> when clicked (its
// XML is tested in src-tauri, desktop::toast); Windows hands that link to the
// registered Hubchat. This drives what the click starts, on the desktop test
// build against a scratch hub:
//   1. Hubchat runs hidden in the tray and a second launch carries the link,
//      as Windows does for the click: the window comes back, that chat opens,
//      and no second Hubchat stays running;
//   2. Hubchat isn't running and a start carries the link: that chat opens.
// The toast itself and the click can't be driven from here. Whether the
// window is shown can only be seen from the user's own (interactive)
// session: from a service session it reads as hidden even at a normal start,
// so those checks are skipped there.
//
//   needs: MAILHUB_DIR (the v1 hub's folder, run with python); the desktop
//   test build (npx tauri build --debug --no-bundle --config
//   src-tauri/tauri.test.conf.json), whose data this resets (e2e-reset.sh)
//   node tools/e2e-notif-click.mjs <hubchat.exe test build> <shots-dir>
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { attach } from './cdp.mjs';

const [exeArg, shots] = process.argv.slice(2);
const MAILHUB = process.env.MAILHUB_DIR;
if (!MAILHUB || !exeArg || !shots) {
  console.error('usage: MAILHUB_DIR=<v1 hub folder> node tools/e2e-notif-click.mjs <hubchat.exe test build> <shots-dir>');
  process.exit(2);
}
const exe = resolve(exeArg);
mkdirSync(shots, { recursive: true });
const HUB_PORT = 7399;
const CDP = 9333;
const HUB = `http://127.0.0.1:${HUB_PORT}`;
const J = JSON.stringify;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };
const cdpEnv = { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP}` };
const plainEnv = { ...process.env };
delete plainEnv.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS;
const ps = (cmd) => spawnSync('powershell', ['-NoProfile', '-Command', cmd], { encoding: 'utf8' }).stdout.trim();
const running = () => Number(ps(`@(Get-Process hubchat -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq '${exe}' }).Count`));
const killApp = () => ps(`Get-Process hubchat -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq '${exe}' } | Stop-Process -Force`);
const peer = (...a) => spawnSync('node', [join(import.meta.dirname, 'peer2.mjs'), HUB, ...a], { encoding: 'utf8' }).stdout.trim().split('\n').pop();

spawnSync('bash', [join(import.meta.dirname, 'e2e-reset.sh')], { stdio: 'ignore' });
const hub = spawn('python', ['-m', 'mailhub.serve'], {
  cwd: MAILHUB, stdio: 'ignore',
  env: { ...process.env, HUB_DATA: mkdtempSync(join(tmpdir(), 'hc-hub-')), HUB_PORT: String(HUB_PORT), HUB_BIND: '127.0.0.1', HUB_NAME: 'e2ehub' },
});
let b;
const open = async () => {
  b = null;
  for (let i = 0; i < 60 && !b; i++) { try { b = await attach(CDP); } catch { await sleep(500); } }
  if (!b) throw new Error('could not attach to the app WebView');
  const raw = b.shot.bind(b);
  b.shot = (p) => Promise.race([raw(p).catch(() => {}), sleep(8000)]);
};
const inv = (cmd, args = {}) => b.eval(`window.__TAURI_INTERNALS__.invoke(${J(cmd)}, ${J(args)})`);
const waitFor = async (expr, ms = 15000) => { const t = Date.now(); while (Date.now() - t < ms) { if (await b.eval(expr).catch(() => false)) return true; await sleep(300); } return false; };
const clickText = async (sel, text, wait = 500) => {
  const ok = await b.eval(`(() => { const e = [...document.querySelectorAll(${J(sel)})].find((x) => x.innerText.includes(${J(text)}) && !x.disabled); if (!e) return false; e.click(); return true; })()`);
  if (!ok) throw new Error(`no ${sel} with "${text}"`);
  await sleep(wait);
};
const visible = () => inv('plugin:window|is_visible', { label: 'main' }).catch((e) => 'error: ' + e);
const openChat = () => b.eval(`(document.querySelector('.conv-head') || {}).innerText || ''`);

spawn(exe, [], { stdio: 'ignore', env: cdpEnv });
try {
  for (let i = 0; i < 50; i++) { try { await fetch(HUB + '/healthz'); break; } catch { await sleep(200); } }
  await open();
  await sleep(2500);
  const canSee = (await visible()) === true;
  if (!canSee) console.log('SKIP window visibility: this session has no interactive desktop');

  // a new identity on the scratch hub (the onboarding screens, as e2e-desktop.mjs)
  await clickText('.fork-opt', 'Create a new identity');
  await b.type('#ob-id', 'click', 200);
  await b.type('#ob-name', 'Click Tester', 200);
  await clickText('.ob-foot button', 'Continue', 1500);
  await clickText('.ob-foot button', 'Continue', 600);
  await b.type('#hub-in', `127.0.0.1:${HUB_PORT}`, 200);
  await clickText('button', 'Check', 2000);
  await clickText('.probe-card button', 'Add', 1500);
  await clickText('.ob-foot button', 'Continue', 1200);
  await clickText('button', 'saved', 1500);
  check('signed in and connected', await waitFor(`window.__TAURI_INTERNALS__.invoke('hc_state').then((s) => !!s.me && s.hubs.some((h) => h.state === 'connected'))`));
  const me = (await inv('hc_state')).me.address;

  // a message from Pat makes the chat (and, in the real app, the toast)
  const pat = peer('whoami');
  peer('send', me, 'hello from the notification test');
  check('the message arrived', await waitFor(`window.__TAURI_INTERNALS__.invoke('hc_chats').then((c) => c.some((x) => x.peer === ${J(pat)}))`, 20000), pat);
  check('no chat is open yet', (await openChat()) === '');
  const link = `hubchat://chat?to=${encodeURIComponent(pat)}`;

  // 1. hidden in the tray (the window's close button hides it), then the click's launch
  await inv('plugin:window|close', { label: 'main' });
  await sleep(1000);
  check('closing the window leaves Hubchat in the tray', (!canSee || (await visible()) === false) && running() === 1, `visible ${await visible()}, running ${running()}`);
  const t0 = Date.now();
  const second = spawnSync(exe, [link], { env: plainEnv, timeout: 30000 });
  check('the second launch hands the link over and exits', second.status === 0, `status ${second.status} after ${Date.now() - t0} ms`);
  if (canSee) check('the window comes back', await waitFor(`window.__TAURI_INTERNALS__.invoke('plugin:window|is_visible', { label: 'main' })`, 10000));
  check('that chat opens', await waitFor(`((document.querySelector('.conv-head') || {}).innerText || '').includes('Pat Peer')`, 10000), J(await openChat()));
  check('still one Hubchat', running() === 1, `running ${running()}`);
  await b.shot(join(shots, 'click-running.png'));

  // 2. not running: the click's launch starts Hubchat on that chat
  killApp();
  await sleep(1500);
  spawn(exe, [link], { stdio: 'ignore', env: cdpEnv });
  await open();
  check('a start with the link opens that chat', await waitFor(`((document.querySelector('.conv-head') || {}).innerText || '').includes('Pat Peer')`, 20000), J(await openChat()));
  if (canSee) check('its window is shown', (await visible()) === true);
  await b.shot(join(shots, 'click-cold.png'));
} catch (e) {
  check('ran to the end', false, String(e));
} finally {
  try { await b?.close(); } catch {}
  killApp();
  hub.kill();
}
const passed = results.filter(Boolean).length;
console.log(`${passed}/${results.length} passed`);
process.exit(passed === results.length ? 0 : 1);
