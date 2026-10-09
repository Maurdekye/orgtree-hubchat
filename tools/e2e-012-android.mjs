// The 0.1.2 phone additions on the phone test build (Hubchat Test) against a
// scratch v1 hub reached through adb reverse:
//   1. an empty chat's intro card is one plain line;
//   2. a markdown message renders (heading, lists, a table, a code block with
//      Copy, links with only an http(s) target) and its raw HTML stays text;
//   3. the version shows beside the app's title.
// The phone must be awake and unlocked. Hubchat Test's data is cleared; the
// real Hubchat is never touched.
//
//   needs: MAILHUB_DIR (the v1 hub's folder, run with python); Hubchat Test
//   installed; adb on PATH or in ADB (ANDROID_SERIAL picks the phone)
//   node tools/e2e-012-android.mjs <shots-dir>
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { attach } from './cdp.mjs';

const [shots] = process.argv.slice(2);
const MAILHUB = process.env.MAILHUB_DIR;
if (!MAILHUB || !shots) {
  console.error('usage: MAILHUB_DIR=<v1 hub folder> node tools/e2e-012-android.mjs <shots-dir>');
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
const shot = (name) => writeFileSync(join(shots, name), spawnSync(ADB, ['exec-out', 'screencap', '-p'], { maxBuffer: 64 << 20 }).stdout);
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };
const peer = (...a) => spawnSync('node', [join(import.meta.dirname, 'peer2.mjs'), HUB, ...a], { encoding: 'utf8' }).stdout.trim().split('\n').pop();

const MD = [
  '### Review: transfer code',
  'Overall **sound**, with *two* fixes and `retry()` to check.',
  '',
  '1. Upload retries restart',
  '2. Cancel races the POST',
  '   - re-check the flag',
  '',
  '| Area | Severity |',
  '|:-----|:--------:|',
  '| Retry | low |',
  '| Cancel | **high** |',
  '',
  '```rust',
  'if cancelled { return Err(Error::Cancelled); }',
  '```',
  '',
  'See [the notes](https://example.com/notes) and [bad](javascript:alert(1)) <script>window.__pwned = 1</script>',
].join('\n');

const hub = spawn('python', ['-m', 'mailhub.serve'], {
  cwd: MAILHUB, stdio: 'ignore',
  env: { ...process.env, HUB_DATA: mkdtempSync(join(tmpdir(), 'hc-hub-')), HUB_PORT: String(HUB_PORT), HUB_BIND: '127.0.0.1', HUB_NAME: 'e2ehub' },
});
let b;
const attachApp = async () => {
  let pid = '';
  for (let i = 0; i < 40 && !(pid = adb('shell', 'pidof', PKG).trim()); i++) await sleep(500);
  adb('forward', 'tcp:9335', `localabstract:webview_devtools_remote_${pid}`);
  b = null;
  for (let i = 0; i < 40 && !b; i++) { try { b = await attach(9335); } catch { await sleep(500); } }
  if (!b) throw new Error('could not attach to Hubchat Test');
};
const waitFor = async (expr, ms = 15000) => { const t = Date.now(); while (Date.now() - t < ms) { if (await b.eval(expr).catch(() => false)) return true; await sleep(300); } return false; };
try {
  for (let i = 0; i < 50; i++) { try { await fetch(HUB + '/healthz'); break; } catch { await sleep(200); } }
  // Pat and Bo are on the hub before the phone joins
  peer('whoami');
  spawnSync('node', [join(import.meta.dirname, 'peer2.mjs'), HUB, 'whoami'], { env: { ...process.env, PEER: 'Bo Peer' } });
  adb('reverse', `tcp:${HUB_PORT}`, `tcp:${HUB_PORT}`);
  adb('shell', 'pm', 'clear', PKG);
  adb('shell', 'pm', 'grant', PKG, 'android.permission.POST_NOTIFICATIONS');
  adb('shell', 'monkey', '-p', PKG, '-c', 'android.intent.category.LAUNCHER', '1');
  await attachApp();
  await sleep(2500);
  const inv = (cmd, args = {}) => b.eval(`window.__TAURI_INTERNALS__.invoke(${J(cmd)}, ${J(args)})`);
  const me = await inv('hc_create_identity', { id: 'phone012', name: 'Phone Tester' });
  await inv('hc_add_hub', { input: HUB });
  await inv('hc_recovery_saved');
  await b.eval('location.reload()').catch(() => {});
  await sleep(3000);
  await attachApp();
  check('signed in and connected', await waitFor(`window.__TAURI_INTERNALS__.invoke('hc_state').then((s) => !!s.me && s.hubs.some((h) => h.state === 'connected'))`, 20000));

  // 3. the version beside the title
  const ver = await b.eval(`(document.querySelector('.appbar .title .ver') || {}).innerText || ''`);
  check('the version shows beside the title', ver === 'v0.1.2', J(ver));

  // 2. Pat sends markdown; the chat opens from the list
  peer('send', me, MD);
  check('the markdown message arrives', await waitFor(`!!document.querySelector('.crow')`, 30000));
  await b.eval(`document.querySelector('.crow').click(), true`);
  check('the markdown message shows in the chat', await waitFor(`!!document.querySelector('.msg.in')`, 10000));
  await sleep(800);
  const md = await b.eval(`(() => {
    const m = [...document.querySelectorAll('.msg.in .mtext, .msg.in .mbody')].pop();
    if (!m) return null;
    return { h: m.querySelectorAll('h3.md-h').length, strong: m.querySelectorAll('strong').length, ol: m.querySelectorAll('ol > li').length,
      ul: m.querySelectorAll('ul > li').length, table: m.querySelectorAll('.md-table td').length, block: m.querySelectorAll('.code pre').length,
      links: [...m.querySelectorAll('a.md-a')].map((a) => a.dataset.href || ''), bad: m.querySelectorAll('script, img, iframe, style, [href], [onclick]').length,
      text: m.innerText.includes('<script>'), pwned: window.__pwned || 0, wide: document.documentElement.scrollWidth <= innerWidth };
  })()`);
  check('markdown renders: heading, bold, lists, table, code block', !!md && md.h === 1 && md.strong >= 2 && md.ol === 2 && md.ul === 1 && md.table === 4 && md.block === 1, J(md));
  check('links carry only an http(s) target; raw HTML stays text; nothing ran', !!md && md.links.length === 1 && md.links[0] === 'https://example.com/notes' && md.bad === 0 && md.text && md.pwned === 0);
  check('the page never scrolls sideways', !!md && md.wide);
  shot('012-phone-markdown.png');

  // 1. Bo is on the hub with no messages (the roster came with Pat's message):
  // from the Directory, Bo's empty chat shows the intro card
  await b.eval(`document.querySelector('.appbar button[aria-label=Back]').click(), true`); await sleep(1200);
  await b.eval(`document.querySelector('button[aria-label=Directory]').click(), true`);
  const boRow = `[...document.querySelectorAll('.scr [role=button], .scr button, .scr .crow')].filter((e) => e.innerText.includes('Bo Peer')).sort((x, y) => x.innerText.length - y.innerText.length)[0]`;
  check('Bo is in the Directory', await waitFor(`!!(${boRow})`, 30000));
  await b.eval(`(${boRow}).click(), true`);
  check('the empty chat shows its intro card', await waitFor(`!!document.querySelector('.chat-start .note-card')`, 10000));
  const intro = await b.eval(`(() => { const n = document.querySelector('.chat-start .note-card'); if (!n) return null; const p = n.lastElementChild; const lh = parseFloat(getComputedStyle(p).lineHeight); return { text: n.innerText.trim(), words: n.innerText.trim().split(/ +/).length, lines: Math.round(p.getBoundingClientRect().height / lh) }; })()`);
  check('the intro card is one plain line of 5-10 words', !!intro && intro.words >= 5 && intro.words <= 10 && intro.lines === 1, J(intro));
  shot('012-phone-intro.png');
} catch (e) {
  check('no error', false, String(e && e.stack || e));
} finally {
  try { await b?.close(); } catch {}
  adb('forward', '--remove', 'tcp:9335');
  adb('reverse', '--remove', `tcp:${HUB_PORT}`);
  hub.kill();
  const pass = results.filter(Boolean).length;
  console.log(`${pass}/${results.length} passed`);
  process.exit(pass === results.length ? 0 : 1);
}
