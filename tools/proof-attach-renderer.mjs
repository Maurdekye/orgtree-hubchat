// Isolated DOM/IPC-shape proof for desktop-hubchat-1-2-0-alpha-5-can-t-connect-to-t
// (user report 2026-10-14 22:24Z). Run ONLY under a contained runner (a kernel
// Job): it bundles the renderer and mounts it in JSDOM against a scripted
// stand-in for the core. No Hubchat app, WebView, native IPC, hub or user data.
//
// The stand-in delivers an `hc` event the way Tauri does: only to a listener
// that is already registered, and (the shell before this fix) not at all
// before the core is attached to the window.
//   S  the attach gap, desktop and Android: the page reads while both pendragon
//      doors are connecting; they connect while the core is not attached (their
//      events are dropped); then the core attaches and sends {"type":"attached"}.
//      Mounted: Settings › Hubs and the chat list's Orgtree row.
//   L  listen before the first read: the page's listener registers a task late
//      and the hubs connect right after the first read is served.
//   R  the user's badge ruling (2026-10-14 22:30Z): the status-unknown badge
//      shows only when no enabled, connected hub reaches the contact. This pins
//      the rule on both platforms; the base already keeps it.
// Expected on 9d46e62 (red control): the 8 S "after attached" checks and L fail;
// the S controls and every R check pass.
for (const k of ['HUBCHAT_DEPENDENCY_ROOT', 'HUBCHAT_DOM_DEPENDENCY_ROOT', 'HUBCHAT_RENDERER_OUT', 'HUBCHAT_RENDERER_EVIDENCE', 'HUBCHAT_JOB_CONTAINED']) if (!process.env[k]) throw Error('Use the contained runner; missing ' + k);
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';

const repo = process.cwd();
const deps = createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT, 'package.json'));
const orgdeps = createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT, 'package.json'));
const esbuild = orgdeps('esbuild');
const out = process.env.HUBCHAT_RENDERER_OUT; fs.mkdirSync(out, { recursive: true });
await esbuild.build({ entryPoints: [path.join(repo, 'tools/attach-renderer-entry.tsx')], outfile: path.join(out, 'entry.mjs'), bundle: true, format: 'esm', platform: 'browser', loader: { '.md': 'text' }, jsx: 'automatic',
  plugins: [{ name: 'existing-dependencies', setup(b) { b.onResolve({ filter: /^[^./][^:]*/ }, (a) => ({ path: deps.resolve(a.path === 'qrcode' ? 'qrcode/lib/browser.js' : a.path) })); } }] });
esbuild.stop();
const { JSDOM } = orgdeps('jsdom');
const dom = new JSDOM('<div id="root"></div>', { url: 'http://scratch.invalid' });
for (const key of ['window', 'document', 'navigator', 'HTMLElement', 'HTMLTextAreaElement', 'localStorage', 'location', 'getComputedStyle', 'Event', 'MouseEvent', 'MutationObserver'])
  Object.defineProperty(globalThis, key, { value: key === 'getComputedStyle' ? dom.window[key].bind(dom.window) : dom.window[key], configurable: true });
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
globalThis.requestAnimationFrame = (f) => setTimeout(f, 0);
globalThis.cancelAnimationFrame = clearTimeout;
// One module instance per scenario: startStore runs once per page.
const entry = pathToFileURL(path.join(out, 'entry.mjs')).href;
const load = (tag) => import(entry + '?' + tag);

const checks = [];
const check = (name, ok, detail) => {
  checks.push(detail === undefined ? { name, ok: !!ok } : { name, ok: !!ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail === undefined ? '' : ' :: ' + JSON.stringify(detail)}`);
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const P1 = 'http://pendragon.invalid:7370', P2 = 'http://pendragon.invalid:7371', STAR = 'http://star-system.invalid:7378';
const ORG = 'orgtree.scratch.b8301c';

/** The core as the page sees it: the user's three hubs, star-hub connected
 *  first, and the Orgtree org listed on the two pendragon doors only. */
function core(platform, { lateListen = false } = {}) {
  const hub = (url, name, state, version) => ({ url, name, state, error: null, retry_at_ms: null, max_attachment_bytes: 1073741824, features: [], version, answered_ms: null, waiting_since_ms: null, clock_offset_ms: null });
  const hubs = [hub(P1, 'maurdekye-net', 'connecting', null), hub(P2, 'maurdekye-net', 'connecting', null), hub(STAR, 'star-hub', 'connected', '2.1.1')];
  const org = { address: ORG, kind: 'org', org_name: 'Orgtree', username: '', blurb: '', online: true, last_seen: null, hubs: [P1, P2] };
  const listeners = new Set();
  const c = { attached: false, dropped: 0, delivered: 0, reads: { state: 0, chats: 0, directory: 0, transfers: 0 }, afterFirstRead: null };
  c.emit = (e) => {
    if (!c.attached) { c.dropped++; return; }
    for (const f of listeners) { c.delivered++; f(e); }
  };
  c.connect = () => {
    for (const h of hubs.slice(0, 2)) { h.state = 'connected'; h.version = '2.2.0-alpha.0'; }
    c.emit({ type: 'hub', url: P1 }); c.emit({ type: 'hub', url: P2 }); c.emit({ type: 'directory' });
  };
  c.api = {
    onEvent: async (f) => { if (lateListen) await sleep(0); listeners.add(f); return () => { listeners.delete(f); }; },
    state: async () => {
      c.reads.state++;
      const s = { me: { address: 'maurdekye.scratch1' }, hubs: structuredClone(hubs), platform };
      if (c.reads.state === 1 && c.afterFirstRead) queueMicrotask(c.afterFirstRead);
      return s;
    },
    chats: async () => { c.reads.chats++; return [{ peer: ORG, last: null, unread: 0 }]; },
    directory: async () => { c.reads.directory++; return [structuredClone(org)]; },
    transfers: async () => { c.reads.transfers++; return { rows: [], next: null }; },
  };
  return c;
}

async function attachGap(platform) {
  const m = await load('S-' + platform);
  const c = core(platform);
  Object.assign(m.api, c.api);
  // The shell before this fix: the page reads, then the hubs connect, all
  // before the core is attached to the window.
  await m.act(async () => { await m.store.startStore(); });
  c.connect();
  const host = document.createElement('div'); document.body.append(host);
  const root = m.createRoot(host);
  function Chats() { const s = m.store.useSnap(); return m.createElement(m.ChatRows, { rows: s.chats, selected: null, onOpen() {} }); }
  await m.act(async () => {
    root.render(m.createElement(m.PlatformCtx.Provider, { value: platform },
      m.createElement('div', null, m.createElement(Chats), m.createElement(m.SettingsSection, { tab: 'hubs' }))));
  });
  const rows = () => [...host.querySelectorAll('.hubst')].map((e) => ({ status: e.textContent, version: (e.nextElementSibling?.textContent.match(/hub version (\S+)/) || [])[1] }));
  const badge = () => host.querySelector('.crow .pres')?.className || '';
  let r = rows();
  check(`S ${platform} control: before the attach, both pendragon rows say Connecting… with hub version unknown`,
    r.length === 3 && r.filter((x) => x.status === 'Connecting…' && x.version === 'unknown').length === 2, r);
  check(`S ${platform} control: before the attach, Orgtree shows the status-unknown badge`, badge().includes('p-disconnected'), badge());
  check(`S ${platform} control: the core's three events before the attach were dropped`, c.dropped === 3 && c.delivered === 0, { dropped: c.dropped, delivered: c.delivered });
  const before = { ...c.reads };
  c.attached = true;
  await m.act(async () => { c.emit({ type: 'attached' }); await sleep(150); });
  r = rows();
  check(`S ${platform}: after "attached", all three hub rows say Connected`, r.length === 3 && r.every((x) => x.status === 'Connected'), r);
  check(`S ${platform}: after "attached", the pendragon rows show hub version 2.2.0-alpha.0 and star-hub 2.1.1`,
    r.filter((x) => x.version === '2.2.0-alpha.0').length === 2 && r.filter((x) => x.version === '2.1.1').length === 1, r);
  check(`S ${platform}: after "attached", Orgtree shows online, without the status-unknown badge`, badge().includes('p-online') && !badge().includes('p-disconnected'), badge());
  const delta = Object.fromEntries(Object.entries(c.reads).map(([k, v]) => [k, v - before[k]]));
  check(`S ${platform}: "attached" re-reads state, chats, directory and transfers`, Object.values(delta).every((n) => n >= 1), delta);
  await m.act(async () => { root.unmount(); });
  host.remove();
}

async function listenFirst() {
  const m = await load('L');
  const c = core('desktop', { lateListen: true });
  c.attached = true; // the core is attached; only the page's own listener is late
  c.afterFirstRead = () => c.connect();
  Object.assign(m.api, c.api);
  await m.act(async () => { await m.store.startStore(); await sleep(150); });
  const pend = (m.store.getSnap().state?.hubs || []).filter((h) => h.url !== STAR);
  check('L: a change reported right after the first read still reaches the page (listener registered first)',
    pend.length === 2 && pend.every((h) => h.state === 'connected' && h.version === '2.2.0-alpha.0'), pend.map((h) => [h.url, h.state, h.version]));
}

async function ruling() {
  const m = await load('R');
  const A = 'http://a.invalid:7370', B = 'http://b.invalid:7370';
  const h = (url, state) => ({ url, name: url, state, error: null, retry_at_ms: null, max_attachment_bytes: 1, features: [], version: null });
  const who = (hubs, online = true) => ({ address: 'pat.scratch.123456', kind: 'person', org_name: '', username: 'Pat', blurb: '', online, last_seen: null, hubs });
  const cases = [
    ['one hub down, the other reaching them', who([A, B]), [h(A, 'disconnected'), h(B, 'connected')], 'online'],
    ['one hub still connecting, the other reaching them', who([A, B]), [h(A, 'connecting'), h(B, 'connected')], 'online'],
    ['one hub refused, the other reaching them', who([A, B]), [h(A, 'refused'), h(B, 'connected')], 'online'],
    ['one connected hub does not list them, the other reaches them', who([B]), [h(A, 'connected'), h(B, 'connected')], 'online'],
    ['one hub disabled, the other reaching them', who([A, B]), [h(A, 'disabled'), h(B, 'connected')], 'online'],
    ['reachable but offline: the ring, not the slash', who([A, B], false), [h(A, 'disconnected'), h(B, 'connected')], 'offline'],
    ['every hub that lists them failing', who([A, B]), [h(A, 'disconnected'), h(B, 'connecting')], 'disconnected'],
    ['the only hub listing them down, a connected hub that does not list them', who([A]), [h(A, 'disconnected'), h(B, 'connected')], 'disconnected'],
    ['one hub disabled, the other refused', who([A, B]), [h(A, 'disabled'), h(B, 'refused')], 'disconnected'],
  ];
  for (const [name, c, hubs, want] of cases) {
    const got = m.presence(c, hubs).state;
    check(`R presence: ${name} -> ${want}`, got === want, got);
  }
  for (const platform of ['desktop', 'android']) {
    for (const [name, c, hubs, want] of [cases[0], cases[6]]) {
      const host = document.createElement('div'); document.body.append(host);
      const root = m.createRoot(host);
      await m.act(async () => {
        root.render(m.createElement(m.PlatformCtx.Provider, { value: platform }, m.createElement(m.PeerAvatar, { address: c.address, c, hubs, size: platform === 'android' ? 52 : 44 })));
      });
      const cls = host.querySelector('.pres')?.className || '';
      check(`R ${platform} badge: ${name} -> ${want === 'disconnected' ? 'status-unknown badge' : 'no status-unknown badge'}`, cls.includes('p-disconnected') === (want === 'disconnected'), cls);
      await m.act(async () => { root.unmount(); });
      host.remove();
    }
  }
}

await attachGap('desktop');
await attachGap('android');
await listenFirst();
await ruling();
const failed = checks.filter((c) => !c.ok).map((c) => c.name);
console.log(`${checks.length - failed.length}/${checks.length} passed`);
fs.writeFileSync(process.env.HUBCHAT_RENDERER_EVIDENCE, JSON.stringify({ checks, passed: checks.length - failed.length, failed,
  limits: 'Mounted shared renderer in JSDOM with a scripted core stand-in; no native IPC, WebView, Tauri event plumbing, hub or device. The shell half (attach before the tray warm-up, the attached event) is proven by the rig e2e, not here.' }, null, 2));
dom.window.close();
process.exit(failed.length ? 1 : 0);
