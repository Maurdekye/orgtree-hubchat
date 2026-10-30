// Inline video in the real app (Hubchat 1.2.7; docs/inline-media.md): the desktop test build, WebView2 and the
// hcmedia scheme against a scratch hub, through a proxy here that logs every attachment request. Pat (tools/peer2.mjs)
// sends a silent synthetic MP4 (no audio track, made with ffmpeg); then:
//   1. the video shows its first frame, paused, read from the hub a bounded range at a time, only part of the file,
//      and the attachment is still not downloaded;
//   2. a click on it plays it;
//   3. fullscreen (the element's request with a user gesture, as its own button makes it) fills the screen: the element
//      and the window;
//   4. Escape leaves fullscreen and does nothing else: the chat and its Contact info panel stay open;
//   5. a seek near the end plays to the end: the file's last piece is fetched and nothing fails.
// Desktop test build under its own identifier (tools/e2e-media.conf.json), so neither the real Hubchat nor another
// test build is touched; its identity and folders are wiped before and after. Muted (--mute-audio); the page's
// clipboard is stubbed to refuse, and nothing here asks for it. Run from Session 0; both sessions are recorded.
//
//   needs: the e2e build, from the worktree:
//     npx tauri build --debug --no-bundle --config src-tauri/tauri.test.conf.json --config tools/e2e-media.conf.json
//   a scratch hub (the mail hub binary) at <hub host:port>, and ffmpeg (FFMPEG, default on PATH)
//   node tools/e2e-media.mjs <hubchat.exe e2e build> <hub host:port> <proxy port> <out dir>
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import http from 'node:http';
import { join, resolve } from 'node:path';
import { attach } from './cdp.mjs';

const [exeArg, hubArg, proxyArg, outDir] = process.argv.slice(2);
if (!exeArg || !hubArg || !proxyArg || !outDir) {
  console.error('usage: node tools/e2e-media.mjs <hubchat.exe e2e build> <hub host:port> <proxy port> <out dir>');
  process.exit(2);
}
const EXE = resolve(exeArg);
const [HUB_HOST, HUB_PORT] = hubArg.replace(/^https?:\/\//, '').replace(/\/$/, '').split(':');
const HUB = `http://${HUB_HOST}:${HUB_PORT}`;
const PROXY_PORT = Number(proxyArg);
const PROXY = `http://127.0.0.1:${PROXY_PORT}`;
mkdirSync(outDir, { recursive: true });
const CONF = JSON.parse(readFileSync(join(import.meta.dirname, 'e2e-media.conf.json'), 'utf8'));
const ID = CONF.identifier;
const DATA = join(process.env.APPDATA, ID);
const WEBVIEW = join(process.env.LOCALAPPDATA, ID);
const TARGET = `identity:${DATA}.dev.orgtree.hubchat`;
const CDP = 9243;
const J = JSON.stringify;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, ok, detail = '') => { results.push(!!ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };
const info = (name, detail) => console.log(`INFO ${name} — ${detail}`);
const sh = (cmd, args) => spawnSync(cmd, args, { encoding: 'utf8', windowsHide: true }).stdout || '';
const sessionOf = (pid) => Number(sh('powershell', ['-NoProfile', '-NonInteractive', '-Command', `(Get-Process -Id ${pid}).SessionId`]).trim());
const cdpEnv = { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP} --mute-audio` };
const PEER_STATE = process.env.PEER_STATE || resolve(outDir, 'peer2-pat-media.json');
const peer = (...a) => spawnSync('node', [join(import.meta.dirname, 'peer2.mjs'), HUB, ...a], { encoding: 'utf8', env: { ...process.env, PEER: 'Pat Media', PEER_STATE } }).stdout.trim().split('\n').pop();

// the clip: a numbered test pattern, 25 s, about 8 MB, so the player reads it in several ranges; no audio track
const CLIP = join(outDir, 'clip.mp4');
const ff = spawnSync(process.env.FFMPEG || 'ffmpeg', ['-hide_banner', '-loglevel', 'error', '-y', '-f', 'lavfi', '-i', 'testsrc2=size=640x360:rate=25', '-t', '25', '-an',
  '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-b:v', '2500k', '-movflags', '+faststart', CLIP], { encoding: 'utf8', windowsHide: true });
if (ff.status !== 0) { console.error('ffmpeg: ' + (ff.stderr || ff.error)); process.exit(2); }
const CLIP_BYTES = readFileSync(CLIP).length;
info('clip', `${CLIP_BYTES} bytes, sha256 ${createHash('sha256').update(readFileSync(CLIP)).digest('hex')}`);

// only this build's own data, by exact name
if (!ID.endsWith('.test') || ID === 'dev.orgtree.hubchat.test') throw new Error('not the e2e identifier: ' + ID);
const credExists = () => /Target:/.test(sh('cmdkey', [`/list:${TARGET}`]));
const wipe = () => {
  if (credExists()) sh('cmdkey', [`/delete:${TARGET}`]);
  for (const p of [DATA, WEBVIEW]) { if (!p.endsWith(`\\${ID}`)) throw new Error('not the e2e folder: ' + p); rmSync(p, { recursive: true, force: true }); }
};

// The hub, through a proxy that logs each attachment download: its Range, status and the bytes it carried.
const fetched = [];
const proxy = http.createServer((req, res) => {
  const up = http.request({ host: HUB_HOST, port: HUB_PORT, method: req.method, path: req.url, headers: req.headers }, (r) => {
    const row = req.method === 'GET' && req.url.startsWith('/api/attachments/') ? { url: req.url, range: req.headers.range ?? null, status: r.statusCode, bytes: 0, at: Date.now() } : null;
    if (row) { fetched.push(row); r.on('data', (c) => { row.bytes += c.length; }); }
    res.writeHead(r.statusCode, r.headers);
    r.pipe(res);
  });
  up.on('error', () => res.destroy());
  res.on('close', () => up.destroy());
  req.pipe(up);
});
await new Promise((r, j) => { proxy.once('error', j); proxy.listen(PROXY_PORT, '127.0.0.1', r); });

let app = null;
let b = null;
const until = async (f, ms, every = 200) => { const t = Date.now(); while (Date.now() - t < ms) { if (await f().catch(() => false)) return true; await sleep(every); } return false; };
const start = async () => {
  app = spawn(EXE, [], { stdio: 'ignore', env: cdpEnv });
  for (let i = 0; i < 60 && !b; i++) { try { b = await attach(CDP); } catch { await sleep(500); } }
  if (!b) throw new Error('could not attach to the e2e build');
  if (!await until(() => b.eval(`typeof window.__TAURI_INTERNALS__?.invoke === 'function'`), 20000, 50)) throw new Error("the e2e build's page never loaded");
  const id = await b.eval(`window.__TAURI_INTERNALS__.invoke('plugin:app|identifier')`);
  if (id !== ID) throw new Error('this is not the e2e build: ' + id);
  await until(() => b.eval(`window.__TAURI_INTERNALS__.invoke('hc_state').then(() => true)`), 20000, 100);
  // the page's clipboard refuses; nothing here asks for it
  await b.eval(`(() => { const no = () => Promise.reject(new DOMException('blocked by the e2e', 'NotAllowedError'));
    for (const k of ['read', 'readText', 'write', 'writeText']) try { Object.defineProperty(navigator.clipboard, k, { value: no, configurable: true }); } catch {} ; return true; })()`);
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
const VIDEO = `document.querySelector('.att-media.video video')`;
const video = () => b.eval(`(() => { const v = ${VIDEO}; if (!v) return null; const r = v.getBoundingClientRect();
  return { src: v.getAttribute('src'), paused: v.paused, t: +v.currentTime.toFixed(2), d: +(v.duration || 0).toFixed(2), ended: v.ended,
    err: v.error && [v.error.code, v.error.message], ready: v.readyState, w: v.videoWidth, preload: v.preload,
    rect: { x: r.x, y: r.y, width: r.width, height: r.height }, fullscreen: document.fullscreenElement === v, inner: [innerWidth, innerHeight], screen: [screen.width, screen.height] }; })()`);
const header = () => b.eval(`document.querySelector('.conv-head .name')?.textContent ?? ''`);
const contactInfo = () => b.eval(`!!document.querySelector('aside.info[aria-label="Contact info"]')`);
const shot = async (name) => { try { await b.shot(join(outDir, name)); } catch (e) { info('capture ' + name, String(e.message || e)); } };

try {
  check('this run is in Session 0 (and so is the app, once started)', sessionOf(process.pid) === 0, String(sessionOf(process.pid)));
  const health = await (await fetch(`${HUB}/healthz`)).json();
  check('the scratch hub answers', !!health.version, J({ version: health.version, name: health.name }));
  wipe();
  await start();
  check('the app runs in Session 0', sessionOf(app.pid) === 0, String(sessionOf(app.pid)));
  const me = await inv('hc_create_identity', { id: 'media', name: 'Media Test' });
  await inv('hc_add_hub', { input: PROXY });
  await stop();
  await start();
  check('signed in and connected', await until(async () => (await coreHub())?.state === 'connected', 20000), me);
  const pat = peer('whoami');
  const sent = JSON.parse(peer('send', me, "Here's the clip", CLIP)).sent;
  check("Pat's video arrived", await until(() => inv('hc_message', { id: sent }).then((m) => !!m), 20000), pat);
  const before = fetched.length;
  // open Pat's chat
  await until(() => b.eval(`!![...document.querySelectorAll('.crow')].find((e) => e.innerText.includes('Pat Media'))`), 10000);
  await b.eval(`[...document.querySelectorAll('.crow')].find((e) => e.innerText.includes('Pat Media'))?.click(), true`);
  check("Pat's chat is open", await until(async () => (await header()).includes('Pat Media'), 8000), await header());
  await b.eval(`${VIDEO}?.scrollIntoView({ block: 'center' }), true`);

  // 1. the first frame, paused, from the hub a range at a time, without a download
  const ready = await until(async () => { const v = await video(); return !!v && v.ready >= 2 && v.w > 0; }, 20000);
  await sleep(1500);
  const v1 = await video();
  check('the video shows its first frame, paused, at the start (preload metadata)', ready && v1.paused && v1.t === 0 && v1.preload === 'metadata', J(v1));
  const reads = fetched.slice(before), bytes = reads.reduce((n, r) => n + r.bytes, 0);
  check('it was read from the hub in ranges, each a bounded piece, and only part of the file',
    reads.length > 0 && reads.every((r) => r.status === 206 && /^bytes=\d+-\d+$/.test(r.range || '') && r.bytes <= 1048576) && bytes < CLIP_BYTES,
    J({ requests: reads.map((r) => [r.range, r.status, r.bytes]), bytes, file: CLIP_BYTES }));
  const m1 = await inv('hc_message', { id: sent });
  const a1 = m1.attachments[0];
  check('and the attachment is still not downloaded (no file, no download)', a1.state === 'remote' && !a1.local_path && !a1.download, J({ state: a1.state, local_path: a1.local_path, download: a1.download }));
  await shot('e2e-1-first-frame.png');

  // 2. a click plays it
  const c = v1.rect;
  await b.press(c.x + c.width / 2, c.y + c.height / 2);
  const plays = await until(async () => { const v = await video(); return !!v && !v.paused && v.t > 0.5; }, 8000);
  check('a click on it plays it', plays, J(await video()));
  await shot('e2e-2-playing.png');

  // 3. fullscreen: what its own button does, the element's request with a user gesture (the button's place in the
  // control row changes with the player's width, so it isn't clicked by position)
  await b.key('i', 2);
  check('Contact info is open beside the chat (the canary for Escape)', await until(contactInfo, 3000));
  const v2 = await video();
  const asked = await b.eval(`${VIDEO}.requestFullscreen().then(() => true, (e) => String(e))`, { gesture: true });
  const full = await until(async () => { const v = await video(); return !!v && v.fullscreen && v.inner[0] === v.screen[0] && v.inner[1] === v.screen[1]; }, 5000);
  await sleep(800);
  const v3 = await video();
  check('fullscreen fills the screen: the video and the window', full, J({ asked, ...v3 }));
  check('and it keeps playing there', !!v3 && !v3.paused && !!v3.src, J(v3));
  await shot('e2e-3-fullscreen.png');

  // 4. Escape leaves fullscreen and nothing else
  await b.key('Escape');
  // back to the size it had before
  const left = await until(async () => { const v = await video(); return !!v && !v.fullscreen && v.inner[0] === v2.inner[0] && v.inner[1] === v2.inner[1]; }, 5000);
  await sleep(600);
  check('Escape leaves fullscreen: the video and the window', left, J(await video()));
  check('and does nothing else: the chat and its Contact info stay open', (await header()).includes('Pat Media') && await contactInfo(), J({ header: await header(), contactInfo: await contactInfo() }));
  await shot('e2e-4-escaped.png');
  // 5. near the end: a seek there and on to the end, fetching the file's last piece, with no error
  const tailFrom = fetched.length;
  await b.eval(`(() => { const v = ${VIDEO}; v.currentTime = Math.max(0, v.duration - 1.5); return v.play().then(() => true, (e) => String(e)); })()`, { gesture: true });
  const ended = await until(async () => { const v = await video(); return !!v && (v.ended || v.t >= v.d - 0.1) && !v.err; }, 15000);
  const v5 = await video();
  const tail = fetched.slice(tailFrom), last = Math.max(0, CLIP_BYTES - 1048576);
  check('a seek near the end plays to the end: its last piece is fetched, nothing fails', ended && !v5.err && tail.some((r) => r.status === 206 && Number((r.range || '').match(/^bytes=(\d+)-/)?.[1]) >= last),
    J({ video: v5, requests: tail.map((r) => [r.range, r.status, r.bytes]), last }));
  const m2 = await inv('hc_message', { id: sent });
  check('after playing, the attachment is still not downloaded', m2.attachments[0].state === 'remote' && !m2.attachments[0].local_path, J(m2.attachments[0]));
  info('attachment requests', J(fetched.slice(before).map((r) => [r.range, r.status, r.bytes])));
  info('page errors and warnings', J(b.errors.slice(0, 10)));
} catch (e) {
  check('ran to the end', false, String(e?.stack || e));
} finally {
  try { await stop(); } catch {}
  try { wipe(); } catch (e) { info('cleanup', String(e)); }
  proxy.close();
  rmSync(CLIP, { force: true });
  check('cleanup: the e2e identity and folders are gone', !credExists() && !existsSync(DATA) && !existsSync(WEBVIEW));
}
const passed = results.filter(Boolean).length;
writeFileSync(join(outDir, 'e2e-media.json'), J({ passed: passed === results.length, checks: results.length, fetched }, null, 1));
console.log(`${passed}/${results.length} passed`);
process.exitCode = passed === results.length ? 0 : 1;
