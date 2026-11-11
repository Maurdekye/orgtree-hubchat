// End-to-end proof of Hubchat Web: the built page (tools/web-build.mjs) in a
// real browser against a real Mail hub, everything on loopback and thrown
// away afterwards.
//
//   an iPhone in a tab only says how to add Hubchat to the Home Screen ->
//   from the Home Screen it starts, and follows the keyboard -> make an
//   identity -> a server that refuses the page says so -> add the hub ->
//   save the recovery words -> a scripted peer writes -> the message shows
//   -> reply from the composer -> the peer gets it and receipts it -> the
//   tick climbs to read -> Settings open -> a reload keeps the identity and
//   the chat -> a second tab is told Hubchat is open elsewhere, takes over,
//   and the first tab is told the same.
//
//   node tools/web-e2e.mjs --dist <built page> --hub <orgtree-mailhub exe> --pg <HUB_TEST_PG folder> --out <folder>
//        [--browser <chromium exe>] [--width 390 --height 844] [--any-session]
//
// --hub: a Mail hub with web origins (2.5.0 line: HUB_WEB_ORIGINS). --pg: the
// hub suite's PostgreSQL template (its make-hub-test-pg.sh): bin/ and
// template/{data,password.txt}. --width: below 900 the phone layout, from
// 900 the desktop one. The page is served the way the spec has it (R3.4,
// tools/web-static.mjs): with its Content Security Policy, compressed.
//
// It never touches a real hub, key or clipboard, sends no OS input (CDP
// only), runs the browser muted and able to resolve nothing but loopback,
// and on Windows refuses to run outside Session 0 (--any-session lifts
// that, for a machine with no signed-in user to disturb). It stops only the
// processes it started, by their own handles.
import { execFileSync, spawn, spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, rmSync } from 'node:fs';
import { createServer } from 'node:http';
import { createServer as tcpServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { launch } from './cdp.mjs';
import { webServer } from './web-static.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const flag = (name) => process.argv.includes('--' + name);
const arg = (name, fallback = null) => { const i = process.argv.indexOf('--' + name); return i > 0 && process.argv[i + 1] ? process.argv[i + 1] : fallback; };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const exe = (name) => (process.platform === 'win32' ? name + '.exe' : name);
const results = [];
const check = (name, ok, detail = '') => { results.push({ name, ok: !!ok }); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' :: ' + detail : ''}`); return !!ok; };

const dist = resolve(arg('dist', join(here, '..', 'dist-web')));
const hubExe = arg('hub'), pgRoot = arg('pg'), out = resolve(arg('out', join(tmpdir(), 'hubchat-web-e2e')));
const width = Number(arg('width', 390)), height = Number(arg('height', 844));
const phone = width < 900;
if (!hubExe || !pgRoot || !existsSync(hubExe) || !existsSync(join(pgRoot, 'template', 'data')) || !existsSync(join(dist, 'index.html')) || (arg('browser') && !existsSync(arg('browser')))) {
  console.error('usage: node tools/web-e2e.mjs --dist <built page> --hub <orgtree-mailhub exe> --pg <HUB_TEST_PG folder> --out <folder>');
  process.exit(2);
}
const sessionOf = (pid) => {
  if (process.platform !== 'win32') return 0;
  try { return Number(execFileSync('powershell', ['-NoProfile', '-Command', `(Get-Process -Id ${pid}).SessionId`], { encoding: 'utf8' }).trim()); } catch { return -1; }
};
const mySession = sessionOf(process.pid);
if (mySession !== 0 && !flag('any-session')) {
  console.error(`REFUSED: this run is in Windows session ${mySession}. Browser tests run in Session 0 (or pass --any-session where no one is signed in).`);
  process.exit(3);
}
mkdirSync(out, { recursive: true });
const freePort = () => new Promise((r, j) => { const s = tcpServer(); s.once('error', j); s.listen(0, '127.0.0.1', () => { const p = s.address().port; s.close(() => r(p)); }); });
const run = mkdtempSync(join(tmpdir(), 'hcw-e2e-'));
const downloads = join(run, 'downloads');
mkdirSync(downloads);
const started = [];
const browsers = [];

const served = [];
const web = webServer(dist, served);

const IPHONE = 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Mobile/15E148 Safari/604.1';
// before any of the page's own script: note policy violations, and when the app first shows
const WATCH = `window.__csp = []; window.__shown = null;
  document.addEventListener('securitypolicyviolation', (e) => window.__csp.push(e.violatedDirective + ' <- ' + e.blockedURI));
  new MutationObserver(() => { if (window.__shown === null && document.querySelector('.fork-opt, .crow, .gate b')) window.__shown = Math.round(performance.now()); }).observe(document, { childList: true, subtree: true });`;

/** A browser of its own (a fresh profile: no storage, no cache), and what drives its page. */
async function open({ w = width, h = height, ua = null, before = '' } = {}) {
  const b = await launch({ width: w, height: h, dpr: 2, mobile: w < 900, ...(arg('browser') ? { exe: arg('browser') } : {}),
    args: ['--mute-audio', '--disable-background-networking', '--disable-sync', '--disable-component-update', '--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1', ...(ua ? ['--user-agent=' + ua] : [])] });
  browsers.push(b);
  const session = sessionOf(b.pid);
  console.log(`receipt: runner pid ${process.pid} session ${mySession}; browser pid ${b.pid} session ${session}; muted; throwaway profile; loopback only; no clipboard, no OS input`);
  if (session !== 0 && !flag('any-session')) throw new Error('the browser is not in Session 0');
  await b.send('Page.addScriptToEvaluateOnNewDocument', { source: WATCH + before });
  const rawShot = b.shot.bind(b);
  return {
    b,
    shot: (name) => Promise.race([rawShot(join(out, name)).catch(() => {}), sleep(8000)]),
    waitFor: async (expr, ms = 20000) => { const t = Date.now(); while (Date.now() - t < ms) { try { if (await b.eval(expr)) return true; } catch { /* the page is between documents */ } await sleep(250); } return false; },
    clickText: async (sel, text, wait = 500) => {
      const ok = await b.eval(`(() => { const e = [...document.querySelectorAll(${JSON.stringify(sel)})].find((x) => x.innerText.includes(${JSON.stringify(text)}) && !x.disabled); if (!e) return false; e.click(); return true; })()`);
      if (!ok) throw new Error(`no ${sel} with "${text}"`);
      await sleep(wait);
    },
    /** Replace what a field holds. */
    fill: async (sel, text, wait = 200) => {
      const ok = await b.eval(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (!e) return false; e.focus(); e.select(); return true; })()`);
      if (!ok) throw new Error('no field ' + sel);
      await b.send('Input.insertText', { text });
      await sleep(wait);
    },
    close: async () => { browsers.splice(browsers.indexOf(b), 1); try { await b.close(); } catch { /* gone */ } },
  };
}

async function main() {
  await new Promise((r) => web.listen(0, '127.0.0.1', r));
  const origin = `http://127.0.0.1:${web.address().port}`;

  // ------------------------------------------ 0. what a Home Screen needs
  const manifest = await (await fetch(origin + '/manifest.webmanifest')).json().catch(() => null);
  const index = await (await fetch(origin + '/')).text();
  const icons = [...(manifest?.icons || []).map((i) => i.src), /rel="apple-touch-icon" href="([^"]+)"/.exec(index)?.[1] || '/missing-apple-touch-icon'];
  const png = async (path) => { const r = await fetch(origin + path); return r.ok && Buffer.from(await r.arrayBuffer()).subarray(1, 4).toString() === 'PNG'; };
  const iconsOk = (await Promise.all(icons.map(png))).every(Boolean);
  check('it can be added to a Home Screen: a manifest (standalone), its icons, the iPhone marks',
    manifest?.display === 'standalone' && manifest.name === 'Hubchat' && icons.length >= 4 && iconsOk && /rel="manifest"/.test(index) && /name="apple-mobile-web-app-capable" content="yes"/.test(index) && /viewport-fit=cover/.test(index),
    JSON.stringify({ display: manifest?.display, icons, iconsOk }));

  // ------------------------------------------------- 1. an iPhone in a tab
  let t = await open({ w: 390, h: 844, ua: IPHONE });
  let from = served.length;
  await t.b.nav(origin + '/', 0);
  check('an iPhone in a browser tab only says how to add Hubchat to the Home Screen',
    await t.waitFor(`!!document.querySelector('.install') && document.body.innerText.includes('Add Hubchat to your Home Screen') && document.body.innerText.includes('Tap Share, then Add to Home Screen, then open Hubchat from there.') && !document.querySelector('.fork-opt')`, 20000),
    await t.b.eval(`document.body.innerText.slice(0, 200)`));
  await sleep(1500); // whatever it would load has been asked for by now
  const inTab = served.slice(from).map((s) => s.path);
  check('and loads no core there', !inTab.some((p) => /\.wasm$|core\.worker/.test(p)), inTab.join(' '));
  await t.shot('web-10-iphone-tab.png');
  check('the install screen has no script error', t.b.errors.length === 0, t.b.errors.join(' | '));
  await t.close();

  // ------------------------------------- 2. an iPhone, from the Home Screen
  // (what the phone itself says there: navigator.standalone; and its keyboard, which covers the page)
  t = await open({ w: 390, h: 844, ua: IPHONE, before: `Object.defineProperty(Navigator.prototype, 'standalone', { get: () => true, configurable: true });
    (() => { const vv = new EventTarget(); Object.assign(vv, { width: 390, height: 844, offsetLeft: 0, offsetTop: 0, pageLeft: 0, pageTop: 0, scale: 1 });
      Object.defineProperty(window, 'visualViewport', { get: () => vv, configurable: true }); window.__vv = vv; })();` });
  await t.b.nav(origin + '/', 0);
  check('opened from the Home Screen, the app starts', await t.waitFor(`!!document.querySelector('.fork-opt')`, 60000), await t.b.eval(`document.body.innerText.slice(0, 200)`));
  check('an iPhone is kept from zooming into a field', await t.b.eval(`document.querySelector('meta[name=viewport]').content.includes('maximum-scale=1')`));
  const screenBox = `(() => { const s = document.querySelector('.screen'); const r = s.getBoundingClientRect(); return { top: Math.round(r.top), height: Math.round(r.height), pad: getComputedStyle(s).paddingBottom, up: document.documentElement.hasAttribute('data-keyboard') }; })()`;
  const full = await t.b.eval(screenBox);
  const viewport = (hgt, top, ev) => t.b.eval(`(() => { __vv.height = ${hgt}; __vv.offsetTop = ${top}; __vv.dispatchEvent(new Event(${JSON.stringify(ev)})); return new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => r(true)))); })()`);
  await viewport(480, 0, 'resize');
  const up = await t.b.eval(screenBox);
  await viewport(480, 60, 'scroll');
  const panned = await t.b.eval(screenBox);
  await t.shot('web-11-iphone-keyboard.png');
  await viewport(full.height, 0, 'resize');
  const down = await t.b.eval(screenBox);
  check('with the keyboard up the phone layout takes the part still visible, and gives it back after',
    !full.up && up.up && up.top === 0 && up.height === 480 && up.pad === '0px' && panned.top === 60 && panned.height === 480 && !down.up && down.top === full.top && down.height === full.height,
    JSON.stringify({ full, up, panned, down }));
  check('the Home Screen start has no script error', t.b.errors.length === 0, t.b.errors.join(' | '));
  await t.close();

  // ------------------------------------------------- the hub and its database
  const pgBin = join(pgRoot, 'bin'), pgData = join(run, 'pg');
  cpSync(join(pgRoot, 'template', 'data'), pgData, { recursive: true });
  const password = readFileSync(join(pgRoot, 'template', 'password.txt'), 'utf8').trim();
  const pgPort = await freePort();
  const pgLog = openSync(join(run, 'postgres.log'), 'w');
  const pg = spawn(join(pgBin, exe('postgres')), ['-D', pgData, '-p', String(pgPort), '-c', 'listen_addresses=127.0.0.1', '-c', 'fsync=off',
    '-c', 'synchronous_commit=off', '-c', 'full_page_writes=off', '-c', 'unix_socket_directories='], { stdio: ['ignore', pgLog, pgLog] });
  started.push(['postgres', pg, () => spawnSync(join(pgBin, exe('pg_ctl')), ['stop', '-m', 'immediate', '-D', pgData], { stdio: 'ignore' })]);
  const pgEnv = { ...process.env, PGPASSWORD: password };
  let ready = false;
  for (let i = 0; i < 120 && !ready; i++) { ready = spawnSync(join(pgBin, exe('pg_isready')), ['-h', '127.0.0.1', '-p', String(pgPort)], { stdio: 'ignore' }).status === 0; if (!ready) await sleep(250); }
  if (!ready) throw new Error('the test PostgreSQL did not start (see ' + join(run, 'postgres.log') + ')');
  const made = spawnSync(join(pgBin, exe('createdb')), ['-h', '127.0.0.1', '-p', String(pgPort), '-U', 'hubtest', 'hubweb'], { env: pgEnv, encoding: 'utf8' });
  if (made.status !== 0) throw new Error('createdb: ' + made.stderr);

  const hubPort = await freePort();
  const HUB = `http://127.0.0.1:${hubPort}`;
  // nothing of this machine's own hub settings reaches the test hub
  const hubEnv = Object.fromEntries(Object.entries(process.env).filter(([k]) => !/^(HUB_|MAILHUB_|HUBCHAT_)/i.test(k)));
  const hubLog = openSync(join(run, 'hub.log'), 'w');
  const hub = spawn(resolve(hubExe), [], { stdio: ['ignore', hubLog, hubLog], env: { ...hubEnv, HUB_DATABASE_URL: `postgres://hubtest:${password}@127.0.0.1:${pgPort}/hubweb`,
    HUB_DATA: join(run, 'hub'), HUB_NAME: 'e2ehub', HUB_PORT: String(hubPort), HUB_BIND: '127.0.0.1', HUB_WEB_ORIGINS: origin } });
  started.push(['hub', hub, null]);
  let health = null;
  for (let i = 0; i < 150 && !health; i++) { try { const r = await fetch(HUB + '/healthz', { headers: { Origin: origin } }); if (r.ok) health = r; } catch { /* not up yet */ } if (!health) await sleep(200); }
  if (!health) throw new Error('the test hub did not start (see ' + join(run, 'hub.log') + ')');
  check('the hub names this page as one that may call it', health.headers.get('access-control-allow-origin') === origin, String(health.headers.get('access-control-allow-origin')));
  // a server that answers and lets no page in (a hub whose owner has not listed this page), and an address where nothing listens
  const stranger = createServer((req, res) => { res.writeHead(200, { 'content-type': 'text/plain' }); res.end('ok'); });
  await new Promise((r) => stranger.listen(0, '127.0.0.1', r));
  started.push(['stranger', { kill: () => stranger.close() }, null]);
  const STRANGER = `http://127.0.0.1:${stranger.address().port}`, NOBODY = `http://127.0.0.1:${await freePort()}`;

  // --------------------------------------------------------------- the browser
  t = await open();
  const { b, shot, waitFor, clickText, fill } = t;
  const second = `window.open('', 'hcw2')`;
  const violations = [];
  const collect = async () => { try { violations.push(...await b.eval('window.__csp || []')); } catch { /* no page */ } };
  try { await b.send('Browser.setDownloadBehavior', { behavior: 'allow', downloadPath: downloads }); }
  catch { await b.send('Page.setDownloadBehavior', { behavior: 'allow', downloadPath: downloads }); }

  // ---------------------------------------------------------- 3. first visit
  from = served.length;
  await b.nav(origin + '/', 0);
  check('the app starts in the browser', await waitFor(`!!document.querySelector('.fork-opt')`, 60000), await b.eval(`document.body.innerText.slice(0, 200)`));
  const firstShown = await b.eval('window.__shown');
  const first = served.slice(from).filter((s) => s.status === 200);
  const sent = first.reduce((n, s) => n + s.sent, 0), raw = first.reduce((n, s) => n + s.raw, 0);
  console.log(`measured: first visit shows the app after ${firstShown} ms; ${first.length} files, ${sent} B over the wire (gzip), ${raw} B unpacked`);
  for (const s of first) console.log(`  ${String(s.sent).padStart(9)} B  ${s.path}`);
  check('the first visit is within the size budget (3 MB compressed)', sent <= 3_000_000, sent + ' B');
  check('it is the web shell: no window title bar, the layout by width', await b.eval(`!document.querySelector('.titlebar') && document.documentElement.dataset.platform === ${JSON.stringify(phone ? 'android' : 'desktop')}`));
  check('the welcome screen offers what a page can do: no camera scan, the key made in this browser',
    await b.eval(`!document.body.innerText.includes('Scan setup code') && !document.body.innerText.includes('No code?') && document.body.innerText.includes('Made in this browser') && [...document.querySelectorAll('.fork-opt')].length === 2`),
    await b.eval(`[...document.querySelectorAll('.fork-opt')].map((o) => o.innerText.split('\\n')[0]).join(' | ')`));
  await shot('web-01-welcome.png');

  // ------------------------------------------------- 4. make an identity
  await clickText('.fork-opt', 'Create a new identity');
  await b.type('#ob-id', 'e2e', 200); await b.type('#ob-name', 'E2E Tester', 200);
  await clickText('.ob-foot button', 'Continue', 1500);
  const address = await (async () => { for (let i = 0; i < 40; i++) { const m = /e2e\.[0-9a-f]{6}/.exec(await b.eval('document.body.innerText')); if (m) return m[0]; await sleep(250); } return null; })();
  check('an identity is made in the browser', !!address, String(address));
  await shot('web-02-address.png');
  await clickText('.ob-foot button', 'Continue', 600);

  // ------------------------------------------------------- 5. add the hub
  // the browser itself logs each refused request: that noise is expected here, and only here
  const quiet = b.errors.length;
  await fill('#hub-in', STRANGER);
  await clickText('button', 'Check', 500);
  check("a server that doesn't let the page in is told apart from one that isn't there, with the address its owner must add",
    await waitFor(`(() => { const c = document.querySelector('.probe-card.bad'); return !!c && c.innerText.includes("doesn't allow this web app yet") && c.innerText.includes(${JSON.stringify(origin)}); })()`, 30000),
    await b.eval(`document.querySelector('.probe-card')?.innerText || document.body.innerText.slice(0, 300)`));
  await shot('web-03-not-allowed.png');
  await fill('#hub-in', NOBODY);
  await clickText('button', 'Check', 500);
  check('an address where nothing answers is unreachable', await waitFor(`(() => { const c = document.querySelector('.probe-card.bad'); return !!c && c.innerText.includes("Can't reach that hub"); })()`, 30000),
    await b.eval(`document.querySelector('.probe-card')?.innerText || document.body.innerText.slice(0, 300)`));
  const noise = b.errors.splice(quiet);
  b.errors.push(...noise.filter((e) => !/Failed to load resource|CORS policy|net::ERR_|Failed to fetch/.test(e)));
  await fill('#hub-in', HUB);
  await clickText('button', 'Check', 500);
  check('the hub answers the page (CORS)', await waitFor(`!!document.querySelector('.probe-card') && document.querySelector('.probe-card').innerText.includes('e2ehub')`, 20000), await b.eval(`document.querySelector('.probe-card')?.innerText || document.body.innerText.slice(0, 300)`));
  await shot('web-04-hub.png');
  await clickText('.probe-card button', 'Add', 1500);
  await clickText('.ob-foot button', 'Continue', 1200);

  // ------------------------------------------------ 6. the recovery words
  const words = await b.eval(`document.querySelectorAll('.words li').length`);
  check('recovery words are shown, and must be saved before the chats open (no Later)', words === 24 && await b.eval(`![...document.querySelectorAll('.ob-foot button')].some((x) => x.innerText.includes('Later'))`), words + ' words');
  await sleep(600); // the file comes with the words, so the click can hand it over at once
  await clickText('.keyacts button', 'Download', 300);
  const saved = join(downloads, 'hubchat-recovery-e2e.txt');
  for (let i = 0; i < 40 && !existsSync(saved); i++) await sleep(250);
  const file = existsSync(saved) ? readFileSync(saved, 'utf8') : '';
  check('Download hands the recovery words file to the browser', file.startsWith('Hubchat recovery words') && file.includes('Address: @net:' + address) && /\n24\. [a-z]+\r?\n/.test(file) && await waitFor(`document.body.innerText.includes('Recovery words saved')`, 5000),
    file ? file.split(/\r?\n/).slice(0, 3).join(' / ') : 'no file in ' + downloads);
  await shot('web-05-words.png');
  await clickText('button', 'saved', 1500);
  check('the chat list opens', await waitFor(`!document.querySelector('.fork-opt') && !document.querySelector('.words')`, 10000));
  await shot('web-06-empty.png');

  // --------------------------------------- 7. receive, reply, be receipted
  const peer = spawn('python', ['-I', join(here, 'peer.py'), HUB, address, '60'], { stdio: ['ignore', 'pipe', 'inherit'], env: hubEnv });
  started.push(['peer', peer, null]);
  let peerOut = ''; peer.stdout.on('data', (d) => { peerOut += d; });
  const peerDone = new Promise((r) => peer.on('exit', r));
  check('a message from a peer arrives', await waitFor(`[...document.querySelectorAll('.crow')].some((r) => r.innerText.includes('hello from') || r.innerText.includes('Test Peer'))`, 30000), await b.eval(`document.body.innerText.slice(0, 300)`));
  await shot('web-07-incoming.png');
  await clickText('.crow', 'Test Peer', 1200);
  check('its text shows, Markdown and all', await waitFor(`[...document.querySelectorAll('.msg.in')].some((m) => m.querySelector('strong')?.innerText === 'peer')`, 10000));
  check('no paperclip: the web app sends no files yet', await b.eval(`!document.querySelector('button[aria-label="Attach files"]') && !!document.querySelector('.comp-box textarea')`));
  await b.type('.comp-box textarea', 'reply from the web', 200);
  await b.click('button[aria-label="Send"]', 800);
  check('the reply shows in the chat', await waitFor(`[...document.querySelectorAll('.msg.out')].some((m) => m.innerText.includes('reply from the web'))`, 10000));
  await Promise.race([peerDone, sleep(70000)]);
  const p = (() => { try { return JSON.parse(peerOut.trim().split('\n').pop() || '{}'); } catch { return {}; } })();
  check('the peer received the reply', p.reply === 'reply from the web', JSON.stringify(p));
  check("the peer's message was receipted", ['delivered', 'read'].includes(p.our_receipt), String(p.our_receipt));
  check('the reply tick climbs to read', await waitFor(`(() => { const m = [...document.querySelectorAll('.msg.out')].pop(); return !!m && /read/.test(m.querySelector('.tick')?.className || ''); })()`, 30000), await b.eval(`[...document.querySelectorAll('.msg.out')].pop()?.querySelector('.tick')?.className`));
  await shot('web-08-conversation.png');

  // ------------------------------------------------------------ 8. Settings
  if (phone) {
    await b.click('button[aria-label="Back"]', 700);
    await b.click('button[aria-label="Settings"]', 900);
    const tabs = await b.eval(`[...document.querySelectorAll('.li .t1')].map((x) => x.innerText)`);
    await clickText('.li', 'About', 900);
    const about = await b.eval(`document.body.innerText`);
    check('Settings open; About names the version and the server, and offers no update check',
      ['Hubs', 'Recovery words', 'About'].every((x) => tabs.includes(x)) && /Version \d+\.\d+\.\d+/.test(about) && about.includes('Served by ' + origin) && !about.includes('Check for updates'), tabs.join(', ') + ' :: ' + about.slice(0, 400).replace(/\n+/g, ' / '));
    await shot('web-09-about.png');
    await b.click('button[aria-label="Back"]', 600);
    await b.click('button[aria-label="Back"]', 600);
    await clickText('.crow', 'Test Peer', 1000);
  } else {
    await b.click('button[aria-label="Settings"]', 900);
    check('Settings open', await waitFor(`document.body.innerText.includes('Profile') && document.body.innerText.includes('Recovery words')`, 5000));
    await shot('web-09-settings.png');
    await b.key('Escape', 0, 600);
  }

  // ----------------------------------- 9. a reload keeps identity and chat
  await collect();
  // (the phone layout comes back in the chat it was in; the wide one shows the list beside it)
  const kept = (d) => `([...${d}.querySelectorAll('.crow')].some((r) => r.innerText.includes('Test Peer')) || [...${d}.querySelectorAll('.msg.out')].some((m) => m.innerText.includes('reply from the web')))`;
  from = served.length;
  await b.nav(origin + '/', 0);
  check('after a reload the identity, the hub and the chat are still here', await waitFor(kept('document'), 60000), await b.eval(`document.body.innerText.slice(0, 300)`));
  const again = served.slice(from).filter((s) => s.status === 200);
  console.log(`measured: a return visit shows the chats after ${await b.eval('window.__shown')} ms; ${again.length} files asked for again, ${again.reduce((n, s) => n + s.sent, 0)} B`);
  await shot('web-12-reloaded.png');

  // --------------------------------------------- 10. one tab at a time
  await b.eval(`void window.open(location.href, 'hcw2')`, { gesture: true });
  check('a second tab is told Hubchat is open elsewhere', await waitFor(`(() => { const w = ${second}; return !!w && !!w.document.querySelector('.gate b') && w.document.querySelector('.gate b').innerText.includes('open in another tab'); })()`, 30000),
    await b.eval(`(() => { const w = ${second}; return w ? w.document.body.innerText.slice(0, 200) : 'no second tab'; })()`));
  await b.eval(`${second}.document.querySelector('.gate button').click()`);
  check('the first tab gives the core up and says so', await waitFor(`!!document.querySelector('.gate b') && document.querySelector('.gate b').innerText.includes('open in another tab')`, 30000), await b.eval(`document.body.innerText.slice(0, 200)`));
  check('the second tab took over and shows the chats', await waitFor(`(() => { const w = ${second}; return !!w && ${kept('w.document')}; })()`, 60000),
    await b.eval(`(() => { const w = ${second}; return w ? w.document.body.innerText.slice(0, 300) : 'no second tab'; })()`));
  await shot('web-13-other-tab.png');

  await collect();
  check('no Content Security Policy violation', violations.length === 0, violations.join(' | '));
  check('no script errors', b.errors.length === 0, b.errors.join(' | '));
  check('nothing was asked for that the server does not have', served.every((s) => s.status === 200), served.filter((s) => s.status !== 200).map((s) => s.status + ' ' + s.path).join(' '));
}

let crashed = null;
try {
  await main();
} catch (e) {
  crashed = e;
  check('the script ran to the end', false, String(e && e.stack ? e.stack : e));
  const b = browsers[browsers.length - 1];
  try { await b?.shot(join(out, 'web-failure.png')); } catch { /* the browser is gone */ }
  try { console.log('page: ' + await b?.eval('document.body.innerText.slice(0, 600)')); } catch { /* no page */ }
  if (b?.errors.length) console.log('page errors: ' + b.errors.join(' | '));
} finally {
  for (const b of [...browsers]) { try { await b.close(); } catch { /* ignore */ } }
  for (const [, proc, stop] of started.reverse()) { try { stop?.(); proc.kill(); } catch { /* gone */ } }
  web.close();
  await sleep(800);
  if (crashed || results.some((r) => !r.ok)) console.log('kept for a look: ' + run);
  else { try { rmSync(run, { recursive: true, force: true }); } catch { /* a handle may linger */ } }
  const failed = results.filter((r) => !r.ok).length;
  console.log(`${results.length - failed}/${results.length} passed (${phone ? 'phone' : 'desktop'} layout, ${width}x${height}); screenshots in ${out}`);
  process.exit(failed ? 1 : 0);
}
