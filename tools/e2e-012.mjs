// The 0.1.2 desktop additions on the real app (the desktop test build, real
// WebView2) against a scratch hub. Pat sends a markdown message and Bo a plain
// one, then:
//   1. the markdown renders (headings, lists, a table, a code block with Copy,
//      links that only carry an http(s) target) and its raw HTML stays text;
//   2. Alt+Up / Alt+Down switch chats while typing, focus lands in the box;
//   3. Shift+Tab from the box highlights the newest message, R starts a reply;
//   4. the right-click menu opens on a message and Escape closes it;
//   5. the chat list shrinks to a rail, which survives a restart, shows the
//      version, and Alt+Down still works there;
//   6. the version shows beside the chat list's title.
//
//   needs: MAILHUB_DIR (the v1 hub's folder, run with python); the desktop
//   test build (npx tauri build --debug --no-bundle --config
//   src-tauri/tauri.test.conf.json), whose data this resets (e2e-reset.sh)
//   node tools/e2e-012.mjs <hubchat.exe test build> <shots-dir>
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { attach } from './cdp.mjs';

const [exeArg, shots] = process.argv.slice(2);
const MAILHUB = process.env.MAILHUB_DIR;
if (!MAILHUB || !exeArg || !shots) {
  console.error('usage: MAILHUB_DIR=<v1 hub folder> node tools/e2e-012.mjs <hubchat.exe test build> <shots-dir>');
  process.exit(2);
}
const exe = resolve(exeArg);
mkdirSync(shots, { recursive: true });
const HUB_PORT = 7399;
const CDP = 9333;
const HUB = `http://127.0.0.1:${HUB_PORT}`;
const J = JSON.stringify;
const ALT = 1, SHIFT = 8;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };
const env = { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP}` };
const ps = (cmd) => spawnSync('powershell', ['-NoProfile', '-Command', cmd], { encoding: 'utf8' }).stdout.trim();
const killApp = () => ps(`Get-Process hubchat -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq '${exe}' } | Stop-Process -Force`);
const peer = (name, ...a) => spawnSync('node', [join(import.meta.dirname, 'peer2.mjs'), HUB, ...a], { encoding: 'utf8', env: { ...process.env, ...(name ? { PEER: name } : {}) } }).stdout.trim().split('\n').pop();

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
  '> quoted line',
  '',
  '```rust',
  'if cancelled { return Err(Error::Cancelled); }',
  '```',
  '',
  'See [the notes](https://example.com/notes) and [bad](javascript:alert(1)) <script>window.__pwned = 1</script> <img src=x onerror="window.__pwned = 2">',
].join('\n');

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
const openName = () => b.eval(`((document.querySelector('.conv-head .who') || {}).innerText || '').split(String.fromCharCode(10))[0]`);
const active = () => b.eval(`(() => { const a = document.activeElement; return a ? a.tagName + '.' + a.className : ''; })()`);
const rows = () => b.eval(`[...document.querySelectorAll('.clist .crow, .clist .rrow')].map((r) => r.getAttribute('title') || (r.querySelector('.crow-name') || r).innerText.split(String.fromCharCode(10))[0])`);

spawn(exe, [], { stdio: 'ignore', env });
try {
  for (let i = 0; i < 50; i++) { try { await fetch(HUB + '/healthz'); break; } catch { await sleep(200); } }
  await open();
  await sleep(2500);
  // a new identity on the scratch hub (the onboarding screens, as e2e-desktop.mjs)
  await clickText('.fork-opt', 'Create a new identity');
  await b.type('#ob-id', 'v012', 200);
  await b.type('#ob-name', 'Release Tester', 200);
  await clickText('.ob-foot button', 'Continue', 1500);
  await clickText('.ob-foot button', 'Continue', 600);
  await b.type('#hub-in', `127.0.0.1:${HUB_PORT}`, 200);
  await clickText('button', 'Check', 2000);
  await clickText('.probe-card button', 'Add', 1500);
  await clickText('.ob-foot button', 'Continue', 1200);
  await clickText('button', 'saved', 1500);
  check('signed in and connected', await waitFor(`window.__TAURI_INTERNALS__.invoke('hc_state').then((s) => !!s.me && s.hubs.some((h) => h.state === 'connected'))`));
  const me = (await inv('hc_state')).me.address;
  // the rail starts off whatever an earlier run left in this WebView's storage
  await b.eval(`localStorage.removeItem('hubchat.chatlist.rail'), true`);
  await b.eval('location.reload()').catch(() => {});
  await sleep(2500);
  await open();

  // 6. the version beside the app's name in the title bar (moved there 2026-10-09 19:00Z)
  const ver = await b.eval(`(document.querySelector('.tb-brand .ver') || {}).innerText || ''`);
  check('the version shows beside the name in the title bar', ver === 'v0.1.2', J(ver));

  // two chats: Bo (plain) first, then Pat (markdown), so Pat sorts on top
  const bo = peer('Bo Peer', 'whoami');
  peer('Bo Peer', 'send', me, 'a plain hello from Bo');
  await sleep(1200);
  const pat = peer(null, 'whoami');
  peer(null, 'send', me, MD);
  check('both chats arrived', await waitFor(`window.__TAURI_INTERNALS__.invoke('hc_chats').then((c) => [${J(pat)}, ${J(bo)}].every((p) => c.some((x) => x.peer === p)))`, 20000));
  await sleep(800);

  // 1. markdown
  await clickText('.clist .crow', 'Pat Peer', 1500);
  check('Pat\'s chat is open', (await openName()) === 'Pat Peer', await openName());
  const md = await b.eval(`(() => {
    const m = [...document.querySelectorAll('.msg.in .mtext, .msg.in .mbody')].pop();
    if (!m) return null;
    const links = [...m.querySelectorAll('a.md-a')].map((a) => a.dataset.href || '');
    return { h: m.querySelectorAll('h3.md-h').length, strong: m.querySelectorAll('strong').length, em: m.querySelectorAll('em').length,
      code: m.querySelectorAll('code').length, ol: m.querySelectorAll('ol > li').length, ul: m.querySelectorAll('ul > li').length,
      table: m.querySelectorAll('.md-table table td').length, quote: m.querySelectorAll('blockquote').length,
      block: m.querySelectorAll('.code pre').length, copy: m.querySelectorAll('[data-act=copy-code]').length,
      links, bad: m.querySelectorAll('script, img, iframe, style, [onerror], [onclick], [href]').length,
      text: m.innerText.includes('<script>') && m.innerText.includes('onerror'), pwned: window.__pwned || 0 };
  })()`);
  check('markdown: heading, bold, italics, inline code', !!md && md.h === 1 && md.strong >= 2 && md.em >= 1 && md.code >= 2, J(md));
  check('markdown: numbered and nested lists, table, quote, code block with Copy', !!md && md.ol === 2 && md.ul === 1 && md.table === 4 && md.quote === 1 && md.block === 1 && md.copy === 1);
  check('markdown: links carry only an http(s) target', !!md && md.links.length === 1 && md.links[0] === 'https://example.com/notes', J(md && md.links));
  check('markdown: raw HTML stays text, nothing ran', !!md && md.bad === 0 && md.text && md.pwned === 0);
  await b.shot(join(shots, '012-markdown.png'));

  // 2. Alt+Down / Alt+Up while typing
  await b.type('.composer textarea', 'half a thought', 300);
  const order = await rows();
  const at = order.indexOf('Pat Peer');
  await b.key('ArrowDown', ALT, 900);
  const down = await openName();
  check('Alt+Down opens the next chat in the list', down === order[at + 1], `${J(order)} -> ${down}`);
  check('…with the focus in its message box, nothing typed', (await active()).startsWith('TEXTAREA') && (await b.eval(`document.querySelector('.composer textarea').value`)) === '');
  await b.key('ArrowUp', ALT, 900);
  check('Alt+Up goes back, the draft kept', (await openName()) === 'Pat Peer' && (await b.eval(`document.querySelector('.composer textarea').value`)) === 'half a thought');
  await b.key('ArrowUp', ALT, 700);
  check('Alt+Up on the top chat stays', (await openName()) === 'Pat Peer');

  // 3. Shift+Tab, then R
  await b.eval(`document.querySelector('.composer textarea').focus(), true`);
  await b.key('Tab', SHIFT, 500);
  const hl = await b.eval(`(() => { const h = document.querySelector('.msg.hl'); const all = [...document.querySelectorAll('.msg')]; return h ? { newest: h === all[all.length - 1], focus: document.activeElement && document.activeElement.className } : null; })()`);
  check('Shift+Tab from the box highlights the newest message, not the attach button', !!hl && hl.newest && !String(hl.focus).includes('icon-btn'), J(hl));
  await b.shot(join(shots, '012-highlight.png'));
  await b.key('r', 0, 600);
  const rb = await b.eval(`(document.querySelector('.composer .rb-who') || {}).innerText || ''`);
  check('R starts a reply and puts the cursor in the box', rb.includes('Replying to') && (await active()).startsWith('TEXTAREA') && !(await b.eval(`!!document.querySelector('.msg.hl')`)), J(rb));
  await b.key('Escape', 0, 400);

  // 4. the right-click menu on a message
  const pt = await b.eval(`(() => { const r = [...document.querySelectorAll('.msg.in .bubble')].pop().getBoundingClientRect(); return { x: r.left + 30, y: r.top + 12 }; })()`);
  await b.moveTo(pt.x, pt.y);
  await b.press(pt.x, pt.y, 'right', 500);
  const items = await b.eval(`[...document.querySelectorAll('.pop.ctx .mi')].map((e) => e.innerText.trim())`);
  check('right-click on a message opens Hubchat\'s menu', items.includes('Reply') && items.some((t) => t.startsWith('Copy')), J(items));
  await b.shot(join(shots, '012-menu.png'));
  await b.key('Escape', 0, 400);
  check('Escape closes it', !(await b.eval(`!!document.querySelector('.pop.ctx')`)));

  // 5. the rail, across a restart
  await clickText('button[title="Shrink the chat list to icons"]', '', 800);
  check('the chat list shrinks to a rail of avatars', await b.eval(`!!document.querySelector('.side.rail') && document.querySelectorAll('.side.rail .rrow').length === 2`));
  killApp();
  await sleep(1500);
  spawn(exe, [], { stdio: 'ignore', env });
  await sleep(4000);
  await open();
  check('after a restart it is still a rail', await waitFor(`!!document.querySelector('.side.rail')`, 15000));
  const rver = await b.eval(`(document.querySelector('.tb-brand .ver') || {}).innerText || ''`);
  check('with the rail, the title bar still shows the version', rver === 'v0.1.2', J(rver));
  await b.eval(`document.querySelector('.side.rail .rrow').click(), true`);
  await sleep(900);
  const first = await openName();
  await b.key('ArrowDown', ALT, 900);
  check('Alt+Down works in the rail too', (await openName()) !== first && !!(await openName()), `${first} -> ${await openName()}`);
  await b.shot(join(shots, '012-rail.png'));
  await clickText('button[title="Show the chat list"]', '', 800);
  check('the full chat list comes back', await b.eval(`!document.querySelector('.side.rail') && document.querySelectorAll('.clist .crow').length === 2`));
} catch (e) {
  check('no error', false, String(e && e.stack || e));
} finally {
  try { await b?.close(); } catch {}
  killApp();
  hub.kill();
  const pass = results.filter(Boolean).length;
  console.log(`${pass}/${results.length} passed`);
  process.exit(pass === results.length ? 0 : 1);
}
