// What the page shows of a hub is what the core reports (user 2026-10-14
// 22:24Z: after the update to 1.2.0-alpha.5 both pendragon hubs stayed
// "Connecting… · hub version unknown" in Settings › Hubs and the Orgtree
// contact had the status-unknown badge, while mail flowed through that hub).
// The core starts its hub connections in setup, but the UI was attached only
// at the end of setup, and every event before that was dropped. Alpha.5's tray
// popup warm-up creates a WebView during setup; creating it pumps the UI
// thread, so the page could load and read the core's state in that gap, and a
// hub that connected after the read never reached it.
//
// Desktop test build under its own identifier (tools/e2e-attach.conf.json, so
// neither the real Hubchat nor an installed Hubchat Test is touched), against
// a scratch hub reached through a proxy in this script. The proxy holds each
// /healthz answer for a set delay, so successive starts move the hub's
// connection across the start-up window. Each start: wait until the core says
// the hub is connected, give the page 3 s, then read what the page shows:
//   - the hub pill and the Settings › Hubs row (its status and hub version)
//   - Pat's chat-list avatar (Pat is only on this hub): no status-unknown badge
// Pass: every start shows the hub connected. The red control (the base build)
// is expected to show the stale row in some starts; how many is timing,
// measured and listed per delay.
//
//   needs: the e2e build, from the worktree:
//     npx tauri build --debug --no-bundle --config src-tauri/tauri.test.conf.json
//       --config tools/e2e-attach.conf.json
//   and a scratch hub (the mail hub binary) at <hub host:port>
//   node tools/e2e-attach.mjs <hubchat.exe e2e build> <hub host:port> <proxy port> <out dir>
//   E2E_DELAYS=0,100,... (ms, the default sweep below)  E2E_ROUNDS=2
//   PEER_STATE=<file> (Pat's identity; default <out dir>/peer2-pat-attach.json)
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import http from 'node:http';
import { join, resolve } from 'node:path';
import { attach } from './cdp.mjs';

const [exeArg, hubArg, proxyArg, outDir] = process.argv.slice(2);
if (!exeArg || !hubArg || !proxyArg || !outDir) {
  console.error('usage: node tools/e2e-attach.mjs <hubchat.exe e2e build> <hub host:port> <proxy port> <out dir>');
  process.exit(2);
}
const EXE = resolve(exeArg);
const [HUB_HOST, HUB_PORT] = hubArg.replace(/^https?:\/\//, '').replace(/\/$/, '').split(':');
const HUB = `http://${HUB_HOST}:${HUB_PORT}`;
const PROXY_PORT = Number(proxyArg);
const PROXY = `http://127.0.0.1:${PROXY_PORT}`;
mkdirSync(outDir, { recursive: true });
const CONF = JSON.parse(readFileSync(join(import.meta.dirname, 'e2e-attach.conf.json'), 'utf8'));
const ID = CONF.identifier;
const DATA = join(process.env.APPDATA, ID);
const WEBVIEW = join(process.env.LOCALAPPDATA, ID);
const TARGET = `identity:${DATA}.dev.orgtree.hubchat`;
const DELAYS = (process.env.E2E_DELAYS || '0,100,200,300,400,500,700,900,1200,1600,2000').split(',').map(Number);
const ROUNDS = Number(process.env.E2E_ROUNDS || 2);
const CDP = 9242;
const J = JSON.stringify;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };
const info = (name, detail) => console.log(`INFO ${name} — ${detail}`);
const cdpEnv = { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP}` };
const sh = (cmd, args) => spawnSync(cmd, args, { encoding: 'utf8' }).stdout || '';
// Pat's identity is kept in the out dir, never in the checkout (PEER_STATE wins)
const PEER_STATE = process.env.PEER_STATE || resolve(outDir, 'peer2-pat-attach.json');
const peer = (...a) => spawnSync('node', [join(import.meta.dirname, 'peer2.mjs'), HUB, ...a], { encoding: 'utf8', env: { ...process.env, PEER: 'Pat Attach', PEER_STATE } }).stdout.trim().split('\n').pop();

// only this build's own data, by exact name
if (!ID.endsWith('.test') || ID === 'dev.orgtree.hubchat.test') throw new Error('not the e2e identifier: ' + ID);
const credExists = () => /Target:/.test(sh('cmdkey', [`/list:${TARGET}`]));
const wipe = () => {
  if (credExists()) sh('cmdkey', [`/delete:${TARGET}`]);
  for (const p of [DATA, WEBVIEW]) { if (!p.endsWith(`\\${ID}`)) throw new Error('not the e2e folder: ' + p); rmSync(p, { recursive: true, force: true }); }
};

// The hub, its /healthz answers held for `delay` ms. `spawned` is when this
// start's process was spawned: each /healthz answer is logged against it.
let delay = 0;
let spawned = 0;
let answers = [];
const proxy = http.createServer((req, res) => {
  const forward = () => {
    const up = http.request({ host: HUB_HOST, port: HUB_PORT, method: req.method, path: req.url, headers: req.headers }, (r) => {
      res.writeHead(r.statusCode, r.headers);
      if (req.url.startsWith('/healthz')) answers.push(Date.now() - spawned);
      r.pipe(res);
    });
    up.on('error', () => res.destroy());
    res.on('close', () => up.destroy());
    req.pipe(up);
  };
  if (req.url.startsWith('/healthz') && delay > 0) setTimeout(forward, delay); else forward();
});
await new Promise((r, j) => { proxy.once('error', j); proxy.listen(PROXY_PORT, '127.0.0.1', r); });

let app = null;
let b = null;
let page = null; // the page this start attached to, and the pages it refused first
const until = async (f, ms, every = 300) => { const t = Date.now(); while (Date.now() - t < ms) { if (await f().catch(() => false)) return true; await sleep(every); } return false; };
const start = async () => {
  answers = [];
  spawned = Date.now();
  app = spawn(EXE, [], { stdio: 'ignore', env: cdpEnv });
  // Hubchat's own page, never the tray popup's: that page may not call
  // plugin:app|identifier (capabilities/tray-menu.json), so a refused
  // identifier is the wrong page. It is logged, with its URL and the refusal,
  // and the attach is tried again.
  const refused = [];
  for (;;) {
    b = null;
    for (let i = 0; i < 60 && !b; i++) { try { b = await attach(CDP); } catch { await sleep(500); } }
    if (!b) throw new Error('could not attach to the e2e build' + (refused.length ? ' after ' + J(refused) : ''));
    if (!await until(() => b.eval(`typeof window.__TAURI_INTERNALS__?.invoke === 'function'`), 20000, 50)) throw new Error("the e2e build's page never loaded: " + J(b.target));
    const loaded = Date.now() - spawned;
    const seen = await b.eval(`window.__TAURI_INTERNALS__.invoke('plugin:app|identifier').then((id) => ({ id, href: location.href }),
      (e) => ({ refused: typeof e === 'string' ? e : (e?.message ?? JSON.stringify(e)), href: location.href }))`)
      .catch((e) => ({ refused: String(e.message || e).split('\n')[0], href: null }));
    if (seen.id !== undefined && seen.id !== ID) throw new Error('this is not the e2e build: ' + seen.id);
    if (seen.id === ID && !/\/tray-menu\.html/.test(seen.href)) {
      page = { href: seen.href, listed: b.target.url, refused };
      info('attached', `${J(seen.href)} (listed as ${J(b.target.url)})` + (refused.length ? `, after ${refused.length} refused: ${J(refused)}` : ''));
      await until(() => b.eval(`window.__TAURI_INTERNALS__.invoke('hc_state').then(() => true)`), 20000, 100);
      return loaded;
    }
    refused.push({ listed: b.target.url, href: seen.href, refused: seen.refused ?? null });
    info("not Hubchat's page", J(refused.at(-1)));
    try { await b.close(); } catch {}
    if (refused.length === 3) throw new Error("no Hubchat page after 3 attaches: " + J(refused));
    await sleep(300);
  }
};
const stop = async () => {
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
const coreHub = async () => (await inv('hc_state')).hubs.find((h) => h.url === PROXY) || null;
const patRow = `[...document.querySelectorAll('.crow')].find((e) => e.innerText.includes('Pat Attach') || e.innerText.includes(PAT))`;
const runs = [];
let me = '';
let pat = '';

try {
  const health = await (await fetch(`${HUB}/healthz`)).json();
  const VERSION = health.version;
  check('the scratch hub reports its version (so "unknown" means a stale row)', !!VERSION, J(VERSION));
  wipe();
  // an identity on the hub (through the proxy, as every start reaches it);
  // onboarding ends with the next start
  await start();
  me = await inv('hc_create_identity', { id: 'attach', name: 'Attach Test' });
  await inv('hc_add_hub', { input: PROXY });
  await stop();
  await start();
  check('signed in and connected', await until(async () => (await coreHub())?.state === 'connected', 20000), me);
  pat = peer('whoami');
  const sent = JSON.parse(peer('send', me, 'hello from the only hub')).sent;
  check("Pat's message arrived", await until(() => inv('hc_message', { id: sent }).then((m) => !!m), 20000), pat);
  const row = patRow.replace('PAT', J(pat));
  check("Pat's chat is listed", await until(() => b.eval(`!!${row}`), 10000));
  await stop();

  for (let round = 1; round <= ROUNDS; round++) {
    for (const d of DELAYS) {
      delay = d;
      const loaded = await start();
      const connected = await until(async () => (await coreHub())?.state === 'connected', 20000, 100);
      await sleep(3000);
      const core = await coreHub();
      const pill = await b.eval(`(() => { const p = document.querySelector('button.hubpill'); return p ? { cls: p.className, title: p.title || p.innerText } : null; })()`);
      const badge = await b.eval(`(() => { const r = ${row}; return r ? (r.querySelector('.pres') || {}).className || '' : null; })()`);
      await b.eval(`document.querySelector('button.hubpill')?.click(), true`);
      await until(() => b.eval(`document.querySelectorAll('.hubst').length > 0`), 5000, 100);
      const shown = await b.eval(`[...document.querySelectorAll('.hubst')].map((e) => ({ status: e.innerText.trim(), version: ((e.nextElementSibling || {}).innerText || '').match(/hub version (\\S+)/)?.[1] || null }))`);
      const run = { round, delay: d, loaded_ms: loaded, page, healthz_answered_ms: answers, core: core && { state: core.state, version: core.version }, pill, shown, badge };
      runs.push(run);
      const fresh = connected && shown.length === 1 && shown[0].status === 'Connected' && shown[0].version === VERSION
        && !!pill && /\bok\b/.test(pill.cls) && !!badge && !badge.includes('p-disconnected');
      check(`round ${round}, /healthz held ${d} ms: the page shows the hub connected`, fresh,
        `core ${J(run.core)}, row ${J(shown)}, pill ${J(pill && pill.cls)}, Pat ${J(badge)}, page loaded ${loaded} ms, healthz answered ${J(answers)} ms after the spawn`);
      await stop();
    }
  }
  const stale = runs.filter((r) => !(r.shown.length === 1 && r.shown[0].status === 'Connected'));
  info('stale starts', `${stale.length} of ${runs.length}` + (stale.length ? ': delays ' + J(stale.map((r) => r.delay)) : ''));
} catch (e) {
  check('ran to the end', false, String(e));
} finally {
  try { await stop(); } catch {}
  try { wipe(); } catch (e) { info('cleanup', String(e)); }
  proxy.close();
  check('cleanup: the e2e identity and folders are gone', !credExists() && !existsSync(DATA) && !existsSync(WEBVIEW));
}
const passed = results.filter(Boolean).length;
console.log(`${passed}/${results.length} passed`);
writeFileSync(join(outDir, 'result.json'), J({ passed, total: results.length, runs }, null, 2));
process.exit(passed === results.length ? 0 : 1);
