// Symmetry S4 #38/#39 (audit ux10 S3/S4): desktop Hubchat gets Orgtree's Enter-key choice (Enter sends, or Ctrl+Enter
// sends and Enter makes a new line) and stages any pasted clipboard file like a picked one. Mounted real
// Conversation composer and General settings, synthetic API.
import {boot} from './symmetry-s4-harness.mjs';
const {mod, h, act, check, settle, click, render, finish, document, window} = await boot('symmetry-s4-composer-entry.tsx');
const {api, store, Conversation, SettingsSection, PlatformCtx} = mod;
const peer = 'pat.abcdef';
const msg = {id: 'in-1', peer, outgoing: false, body: 'hello', created_at: '2026-10-13T21:00:00Z', state: 'received', hub: null, attachments: [], error: null, seen: true, sent_at: null, received_at: null, fetched_at: null, delivered_at: null, read_at: null, kind: null};
const sent = [], saved = [], infos = [];
Object.assign(api, {
  state: async () => ({me: {id: 'me', address: 'scratch.123456', name: 'Me', about: ''}, hubs: [], platform: 'desktop', recovery_saved: true, file_background_hint: null}),
  chats: async () => [{peer, last: msg, unread: 0}], directory: async () => [], transfers: async () => ({rows: [], next: null}),
  chat: async () => [msg], onEvent: async () => () => {}, draft: async () => '', setDraft: async () => {}, draftAnswers: async () => ({replies: [], picks: {}}),
  sendRoute: async () => ({pinned: null, automatic: null, next: null, hubs: []}), markRead: async () => {}, openDm: async () => {}, setupStatus: async () => null,
  loadOlder: async () => ({added: 0, more: [], unreachable: []}),
  onSettingsChanged: () => Promise.resolve(() => {}),
  send: async m => { sent.push(m); },
  savePasted: async (name, data) => { saved.push([name, data.length]); return 'C:/pasted/' + name; },
  fileInfo: async source => { infos.push(source); return {name: source.split('/').pop(), bytes: 10}; },
});
await act(async () => { await store.startStore(); });
const ta = () => document.querySelector('textarea');
const type = async v => { await act(async () => { const set = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set; set.call(ta(), v); ta().dispatchEvent(new window.Event('input', {bubbles: true})); }); await settle(); };
const key = async (init, name = 'Enter') => { const e = new window.KeyboardEvent('keydown', {key: name, bubbles: true, cancelable: true, ...init}); await act(async () => { ta().dispatchEvent(e); await Promise.resolve(); }); await settle(8); return e; };
const mount = async () => { await render(null); await render(h(PlatformCtx.Provider, {value: 'desktop'}, h(Conversation, {peer, onBack() {}, onInfo() {}, onOpenAddr() {}, onContact() {}}))); await settle(8); };
const meta = () => document.querySelector('.comp-meta')?.textContent || '';
const sendBtn = () => document.querySelector('.sendbtn');

// ---- #38: the Enter-key choice
localStorage.removeItem('hubchat.enter-key');
await mount(); await type('first');
check('by default the hint says Enter sends', /Enter\s*send/.test(meta()) && /Shift\+Enter\s*new line/.test(meta()));
await key({shiftKey: true});
check('by default Shift+Enter does not send', sent.length === 0);
await key({});
check('by default Enter sends', sent.length === 1 && sent[0].body === 'first');
sent.length = 0;
localStorage.setItem('hubchat.enter-key', 'newline');
await mount(); await type('second');
check('with newline chosen the hint says Ctrl+Enter sends and Enter makes a new line', /Ctrl\+Enter\s*send/.test(meta()) && /Enter\s*new line/.test(meta()));
check('with newline chosen the Send button names Ctrl+Enter', /Ctrl\+Enter/.test(sendBtn()?.getAttribute('title') || ''));
const plain = await key({});
check('Enter does not send and keeps its newline', sent.length === 0 && !plain.defaultPrevented);
await key({shiftKey: true});
check('Shift+Enter still does not send', sent.length === 0);
await key({}, 'Enter'); // control for the next: an IME composition
await key({ctrlKey: true, isComposing: true});
check('an IME composition never sends', sent.length === 0);
await key({ctrlKey: true});
check('Ctrl+Enter sends', sent.length === 1 && sent[0].body === 'second');
sent.length = 0; await mount(); await type('third');
await key({metaKey: true});
check('Command+Enter sends too', sent.length === 1 && sent[0].body === 'third');
// the choice is made in Settings > General and read by the composer
localStorage.removeItem('hubchat.enter-key'); sent.length = 0;
await render(h(PlatformCtx.Provider, {value: 'desktop'}, h(SettingsSection, {tab: 'general'})));
const select = document.querySelector('select[aria-label="Enter key"]');
check('General offers an Enter key choice', !!select && [...select.options].map(o => o.textContent.trim()).join('|') === 'Send message|Insert new line');
if (select) { await act(async () => { const set = Object.getOwnPropertyDescriptor(window.HTMLSelectElement.prototype, 'value').set; set.call(select, 'newline'); select.dispatchEvent(new window.Event('change', {bubbles: true})); }); await settle(); }
check('choosing Insert new line is kept', localStorage.getItem('hubchat.enter-key') === 'newline');
localStorage.removeItem('hubchat.enter-key');

// ---- #39: any pasted clipboard file joins the attachments
const paste = async (files, types = ['Files']) => {
  const e = new window.Event('paste', {bubbles: true, cancelable: true});
  Object.defineProperty(e, 'clipboardData', {value: {types, files, getData: () => ''}});
  await act(async () => { ta().dispatchEvent(e); await Promise.resolve(); }); await settle(10);
  return e;
};
const file = (name, type, bytes = 8) => { const f = new window.File([new Uint8Array(bytes)], name, {type}); if (!f.arrayBuffer) f.arrayBuffer = async () => new Uint8Array(bytes).buffer; return f; };
const chips = () => [...document.querySelectorAll('.attrow .attchip, .attrow [class*="att"]')].map(e => e.textContent).join('|');
await mount();
saved.length = 0; infos.length = 0;
const img = await paste([file('shot.png', 'image/png')]);
check('control: a pasted image is staged as before', saved.length === 1 && /^pasted-.*\.png$/.test(saved[0][0]) && img.defaultPrevented);
saved.length = 0;
const pdf = await paste([file('report.pdf', 'application/pdf')]);
check('a pasted PDF is saved under its own name', saved.length === 1 && saved[0][0] === 'report.pdf');
check('a pasted file is not swallowed into the text', pdf.defaultPrevented);
check('a pasted file appears among the attachments', /report\.pdf/.test(document.querySelector('.attrow')?.textContent || ''));
saved.length = 0;
const mixed = await paste([file('notes.txt', 'text/plain')], ['text/plain', 'Files']);
check('with text on the clipboard too, text wins and nothing is staged', saved.length === 0 && !mixed.defaultPrevented);
const big = await paste([file('huge.bin', 'application/octet-stream', 1)].map(f => { Object.defineProperty(f, 'size', {value: 5e9}); return f; }));
check('a file too large to paste says so and is not saved', saved.length === 0 && /too large/i.test(document.querySelector('.comp-err')?.textContent || ''));
await finish('Mounted desktop composer and General settings with synthetic API; no native clipboard, file or pixel acceptance');
