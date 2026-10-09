// From the Hubchat design work (hubchat-design-opus/tools/cdp.mjs), plus attach().
// Minimal Chrome DevTools Protocol driver (no dependencies; Node 24 has a
// global WebSocket). Launches headless Chrome, drives one page, collects
// console errors and uncaught exceptions, takes screenshots. Always call
// close(): it kills Chrome and removes the throwaway profile.
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const CHROME = 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function launch({ width = 1440, height = 900, dpr = 1, port = 9300 + Math.floor(Math.random() * 400) } = {}) {
  const prof = mkdtempSync(join(tmpdir(), 'hc-chrome-'));
  const proc = spawn(CHROME, ['--headless=new', '--disable-gpu', '--hide-scrollbars', '--no-first-run', '--no-default-browser-check',
    '--disable-extensions', `--remote-debugging-port=${port}`, `--user-data-dir=${prof}`, `--window-size=${width},${height}`, 'about:blank'],
    { stdio: 'ignore' });
  return connect(port, proc, prof, { width, height, dpr });
}

/** Attach to a browser already listening on `port` (e.g. a Tauri app's
 *  WebView2 started with --remote-debugging-port). close() then only
 *  disconnects; the caller owns the process. */
export async function attach(port, { width = 0, height = 0, dpr = 1 } = {}) {
  return connect(port, null, null, { width, height, dpr });
}

async function connect(port, proc, prof, { width, height, dpr }) {
  let targets = null;
  for (let i = 0; i < 60 && !targets; i++) {
    await sleep(150);
    try { targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json(); } catch { /* not up yet */ }
  }
  if (!targets) { proc?.kill(); throw new Error('browser not reachable on port ' + port); }
  const page = targets.find((t) => t.type === 'page');
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
  let id = 0; const pending = new Map(); const errors = []; const listeners = [];
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) { const { r, j } = pending.get(msg.id); pending.delete(msg.id); msg.error ? j(new Error(msg.error.message)) : r(msg.result); return; }
    if (msg.method === 'Runtime.exceptionThrown') errors.push('EXC ' + (msg.params.exceptionDetails.exception?.description || msg.params.exceptionDetails.text));
    if (msg.method === 'Runtime.consoleAPICalled' && (msg.params.type === 'error' || msg.params.type === 'warning')) errors.push('CON ' + msg.params.args.map((a) => a.value ?? a.description).join(' '));
    if (msg.method === 'Log.entryAdded' && msg.params.entry.level === 'error') errors.push('LOG ' + msg.params.entry.text);
    listeners.forEach((f) => f(msg));
  };
  const send = (method, params = {}) => new Promise((r, j) => { const i = ++id; pending.set(i, { r, j }); ws.send(JSON.stringify({ id: i, method, params })); });
  await send('Page.enable'); await send('Runtime.enable'); await send('Log.enable');
  const metrics = async (w, h, d) => send('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: d, mobile: false });
  if (width) await metrics(width, height, dpr);

  const api = {
    errors,
    send,
    sleep,
    async size(w, h, d = dpr) { await metrics(w, h, d); },
    async nav(url, wait = 700) {
      // a hash-only change is a same-document navigation with no load event,
      // so always pass through about:blank; and never wait forever
      const go = async (u) => {
        const loaded = new Promise((r) => { const f = (m) => { if (m.method === 'Page.loadEventFired') { listeners.splice(listeners.indexOf(f), 1); r(); } }; listeners.push(f); });
        await send('Page.navigate', { url: u });
        await Promise.race([loaded, sleep(8000)]);
      };
      await go('about:blank'); await go(url); await sleep(wait);
    },
    file: (p, hash = '') => pathToFileURL(p).href + (hash ? '#' + hash : ''),
    async eval(expr) {
      const r = await send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) throw new Error('eval failed: ' + (r.exceptionDetails.exception?.description || r.exceptionDetails.text) + '\n' + expr.slice(0, 200));
      return r.result.value;
    },
    async click(sel, wait = 250) {
      const ok = await api.eval(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (!e) return false; e.scrollIntoView({block:'nearest'}); e.click(); return true; })()`);
      if (!ok) throw new Error('no element for click: ' + sel);
      await sleep(wait);
    },
    async mouse(sel, type = 'click', wait = 250) {
      const box = await api.eval(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (!e) return null; const r = e.getBoundingClientRect(); return {x: r.left + r.width/2, y: r.top + r.height/2}; })()`);
      if (!box) throw new Error('no element for mouse: ' + sel);
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: box.x, y: box.y });
      if (type === 'click') {
        await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: box.x, y: box.y, button: 'left', clickCount: 1 });
        await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: box.x, y: box.y, button: 'left', clickCount: 1 });
      }
      await sleep(wait);
    },
    /** Moves the pointer to (x, y) (hover states, before measuring a press). */
    async moveTo(x, y, wait = 150) {
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
      await sleep(wait);
    },
    /** A trusted press and release at (x, y): `button` 'left', or 'right' for
     *  the context menu. */
    async press(x, y, button = 'left', wait = 250) {
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
      await send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button, clickCount: 1 });
      await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button, clickCount: 1 });
      await sleep(wait);
    },
    async type(sel, text, wait = 150) {
      const ok = await api.eval(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (!e) return false; e.focus(); return true; })()`);
      if (!ok) throw new Error('no element for type: ' + sel);
      await send('Input.insertText', { text });
      await sleep(wait);
    },
    async key(key, mods = 0, wait = 200) {
      const codes = { Enter: 13, Escape: 27, ArrowDown: 40, ArrowUp: 38, Backspace: 8, Tab: 9 };
      const vk = codes[key] || key.toUpperCase().charCodeAt(0);
      const base = { key, code: key.length === 1 ? 'Key' + key.toUpperCase() : key, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers: mods };
      await send('Input.dispatchKeyEvent', Object.assign({ type: 'rawKeyDown' }, base, key === 'Enter' ? { text: '\r', unmodifiedText: '\r' } : {}));
      if (key === 'Enter') await send('Input.dispatchKeyEvent', Object.assign({ type: 'char', text: '\r', unmodifiedText: '\r' }, base));
      await send('Input.dispatchKeyEvent', Object.assign({ type: 'keyUp' }, base));
      await sleep(wait);
    },
    async text(sel) { return api.eval(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); return e ? e.innerText : null; })()`); },
    async count(sel) { return api.eval(`document.querySelectorAll(${JSON.stringify(sel)}).length`); },
    async shot(path, clipSel, pad = 0) {
      let clip;
      if (clipSel) {
        const r = await api.eval(`(() => { const e = document.querySelector(${JSON.stringify(clipSel)}); if (!e) return null; const b = e.getBoundingClientRect(); const p = ${+pad}; const x = Math.max(0, b.left - p), y = Math.max(0, b.top - p); return {x, y, width: Math.min(innerWidth - x, b.width + 2 * p), height: Math.min(innerHeight - y, b.height + 2 * p)}; })()`);
        if (!r) throw new Error('no element to clip: ' + clipSel);
        clip = Object.assign(r, { scale: 1 });
      }
      const res = await send('Page.captureScreenshot', Object.assign({ format: 'png', captureBeyondViewport: false }, clip ? { clip } : {}));
      writeFileSync(path, Buffer.from(res.data, 'base64'));
    },
    async close() {
      try { ws.close(); } catch { /* already closed */ }
      if (proc) {
        proc.kill();
        await sleep(600);
        try { rmSync(prof, { recursive: true, force: true }); } catch { /* chrome may hold files briefly */ }
      }
    },
  };
  return api;
}
