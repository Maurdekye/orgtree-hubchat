// Ctrl+V of a picture on Linux (ticket hubchat-on-linux-ctrl-v-pastes-text-but-not-imag): WebKitGTK's paste event
// carries no picture, so the keyboard paste found nothing to attach while the right-click Paste, which reads the
// clipboard natively, attached it. The composer now reads the native clipboard the same way when a Linux desktop
// paste event brings neither text nor files. Mounted real Conversation composer, synthetic API: the native clipboard
// is a stub (api.clipboardText / api.clipboardImage), never the real one.
import {boot} from './symmetry-s4-harness.mjs';
const {mod, h, act, check, settle, render, finish, document, window} = await boot('linux-keypaste-entry.tsx');
const {api, store, Conversation, OsCtx, PlatformCtx, WebCtx} = mod;
const peer = 'pat.abcdef';
const msg = {id: 'in-1', peer, outgoing: false, body: 'hello', created_at: '2026-10-13T21:00:00Z', state: 'received', hub: null, attachments: [], error: null, seen: true, sent_at: null, received_at: null, fetched_at: null, delivered_at: null, read_at: null, kind: null};
const saved = [], reads = [], inserted = [];
let clip = {text: null, png: null};
const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3, 4]);
Object.assign(api, {
  state: async () => ({me: {id: 'me', address: 'scratch.123456', name: 'Me', about: ''}, hubs: [], platform: 'desktop', recovery_saved: true, file_background_hint: null}),
  chats: async () => [{peer, last: msg, unread: 0}], directory: async () => [], transfers: async () => ({rows: [], next: null}),
  chat: async () => [msg], onEvent: async () => () => {}, draft: async () => '', setDraft: async () => {}, draftAnswers: async () => ({replies: [], picks: {}}),
  sendRoute: async () => ({pinned: null, automatic: null, next: null, hubs: []}), markRead: async () => {}, openDm: async () => {}, setupStatus: async () => null,
  loadOlder: async () => ({added: 0, more: [], unreachable: []}),
  onSettingsChanged: () => Promise.resolve(() => {}),
  send: async () => {},
  savePasted: async (name, data) => { saved.push([name, data.length]); return 'C:/pasted/' + name; },
  fileInfo: async source => ({name: source.split('/').pop(), bytes: 10}),
  clipboardText: async () => { reads.push('text'); return clip.text; },
  clipboardImage: async () => { reads.push('image'); if (!clip.png) throw 'no picture on the clipboard'; return clip.png.buffer.slice(0); },
});
// jsdom has no execCommand, DataTransfer or clipboardData on ClipboardEvent: minimal stand-ins for what the
// native Paste builds (a DataTransfer holding the picture, dispatched as a paste event).
document.execCommand = (cmd, _ui, value) => { inserted.push([cmd, value]); return true; };
class StubTransfer { constructor() { this.files = []; this.types = []; this.items = {add: f => { this.files.push(f); if (!this.types.includes('Files')) this.types.push('Files'); }}; } getData() { return ''; } }
class StubClipboardEvent extends window.Event { constructor(type, init = {}) { super(type, init); this.clipboardData = init.clipboardData ?? null; } }
if (!window.File.prototype.arrayBuffer) window.File.prototype.arrayBuffer = function () { return new Promise((ok, no) => { const r = new window.FileReader(); r.onload = () => ok(r.result); r.onerror = no; r.readAsArrayBuffer(this); }); };
for (const [k, v] of [['DataTransfer', StubTransfer], ['ClipboardEvent', StubClipboardEvent]]) { globalThis[k] = v; window[k] = v; }
await act(async () => { await store.startStore(); });
const ta = () => document.querySelector('textarea');
const mount = async (os, {web = false, platform = 'desktop'} = {}) => {
  await render(null);
  await render(h(WebCtx.Provider, {value: web}, h(OsCtx.Provider, {value: os}, h(PlatformCtx.Provider, {value: platform}, h(Conversation, {peer, onBack() {}, onInfo() {}, onOpenAddr() {}, onContact() {}})))));
  await settle(8);
};
// A paste event as the webview builds it: `types`, `files` and getData('text/plain').
const paste = async ({types = [], files = [], text = ''} = {}) => {
  const e = new window.Event('paste', {bubbles: true, cancelable: true});
  Object.defineProperty(e, 'clipboardData', {value: {types, files, getData: t => (t === 'text/plain' ? text : '')}});
  await act(async () => { ta().dispatchEvent(e); await Promise.resolve(); }); await settle(12);
  return e;
};
const file = (name, type, bytes = 8) => { const f = new window.File([new Uint8Array(bytes)], name, {type}); if (!f.arrayBuffer) f.arrayBuffer = async () => new Uint8Array(bytes).buffer; return f; };
const reset = () => { saved.length = 0; reads.length = 0; inserted.length = 0; };

// ---- Linux desktop (WebKitGTK)
await mount('linux');
reset(); clip = {text: null, png: PNG};
const pic = await paste();
check('Linux: Ctrl+V of a picture the paste event does not carry reads the native clipboard', reads.includes('image'));
check('Linux: that picture is staged as a pasted image, like right-click Paste', saved.length === 1 && /^pasted-.*\.png$/.test(saved[0][0]) && saved[0][1] === PNG.length);
check('Linux: the empty webview paste is not let through', pic.defaultPrevented);
check('Linux: it appears among the attachments', /pasted-.*\.png/.test(document.querySelector('.attrow')?.textContent || ''));

reset(); clip = {text: 'from the native clipboard', png: PNG};
await paste();
check('Linux: with text on the native clipboard, text wins as in right-click Paste (typed, nothing staged)', saved.length === 0 && inserted.some(([c, v]) => c === 'insertText' && v === 'from the native clipboard'));

reset(); clip = {text: null, png: PNG};
const typed = await paste({types: ['text/plain'], text: 'hello there'});
check('control: Linux text the paste event carries pastes as before (no native read, not prevented)', reads.length === 0 && saved.length === 0 && !typed.defaultPrevented);

reset(); clip = {text: null, png: PNG};
const carried = await paste({types: ['Files'], files: [file('shot.png', 'image/png')]});
check('control: Linux picture the paste event does carry is staged once, without a native read', reads.length === 0 && saved.length === 1 && carried.defaultPrevented);

reset(); clip = {text: null, png: null};
await paste();
check('Linux: an empty clipboard stages nothing and types nothing', saved.length === 0 && inserted.length === 0);

// ---- other platforms keep their webview's own paste
for (const [label, os, opts] of [['Windows', 'windows', {}], ['macOS', 'macos', {}], ['Android', 'android', {platform: 'android'}], ['Hubchat Web on Linux', 'linux', {web: true}]]) {
  await mount(os, opts);
  reset(); clip = {text: null, png: PNG};
  const e = await paste();
  check(label + ': an empty paste event never reads the native clipboard', reads.length === 0 && saved.length === 0 && !e.defaultPrevented);
}
await finish('Mounted composer with synthetic API and a stubbed native clipboard; WebKitGTK paste-event contents are modelled, not measured');
