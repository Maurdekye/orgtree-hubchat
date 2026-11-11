// Long-press a DM row in Android's chat list (docket hubchat-mobile-long-press-a-dm-in-the-chat-list-o; USER 2026-11-11
// "Build it as listed": Open, Contact info, Copy mention, Close DM). The real Android Chats screen (and ChatRows)
// mounted in an isolated DOM with a synthetic API: nothing is opened, no window, no network. A long-press is the WebView's
// contextmenu event, the same one the in-text mention long-press already uses. The clipboard is a recording stub.
// Run under the repository Job limiter (HUBCHAT_JOB_CONTAINED=1, an assigned C:\orgtree-build output).
// Phase green: every check passes. Phase red (HUBCHAT_PROOF_PHASE=red, the base before the change): exactly L1-L6
// fail and the controls L7, L8, CT1-CT3 pass, so the controls really ran.
import fs from 'node:fs';
import path from 'node:path';
import {boot} from './symmetry-s4-harness.mjs';
const {mod, h, act, checks, settle, render, document, window} = await boot('dm-long-press-entry.tsx');
const {api, store, Chats, ChatRows, CtxMenu, closeMenu, PlatformCtx, ToastProbe} = mod;
for (const k of ['getComputedStyle', 'MutationObserver']) Object.defineProperty(globalThis, k, {value: k === 'getComputedStyle' ? window[k].bind(window) : window[k], configurable: true});
Object.defineProperty(globalThis, 'requestAnimationFrame', {value: f => setTimeout(f, 0), configurable: true, writable: true});
Object.defineProperty(globalThis, 'cancelAnimationFrame', {value: clearTimeout, configurable: true, writable: true});
window.HTMLElement.prototype.scrollIntoView = function () {};

const copied = [];
Object.defineProperty(window.navigator, 'clipboard', {value: {writeText: async t => { copied.push(t); }}, configurable: true});

const HUB = 'https://fixture.invalid';
const MAYA = 'maya.e71f2b', ORG = 'acme.4f2a9c';
const hubs = [{url: HUB, name: 'office', state: 'connected', error: null, retry_at_ms: null, max_attachment_bytes: 1e9, features: []}];
const directory = [
  {address: MAYA, kind: 'person', org_name: '', username: 'maya', blurb: '', online: true, last_seen: null, hubs: [HUB]},
  {address: ORG, kind: 'org', org_name: 'Alpha Works', username: '', blurb: '', online: true, last_seen: null, hubs: [HUB]}];
let platform = 'android';
const msg = (id, peer, body) => ({id, peer, outgoing: false, body, hub: HUB, kind: null, reply_to: null, created_at: '2026-11-11T00:00:0' + id.length + 'Z',
  sent_at: null, received_at: null, state: 'received', attachments: [], error: null, seen: true, fetched_at: null, delivered_at: null, read_at: null});
const chats = {[ORG]: [msg('o1', ORG, 'hello')], [MAYA]: [msg('p1', MAYA, 'hi')]};
Object.assign(api, {
  state: async () => ({me: {id: 'me', address: 'scratch.123456', name: 'Me', about: ''}, hubs, platform, recovery_saved: true, file_background_hint: null}),
  chats: async () => Object.entries(chats).map(([peer, list]) => ({peer, last: list[list.length - 1], unread: 0})),
  directory: async () => directory, transfers: async () => ({rows: [], next: null}), onEvent: async () => () => {},
  draft: async () => '', setDraft: async () => {}, setupStatus: async () => null,
  localOrgtreeOrgs: async () => [], orgtreeLinks: async () => false,
});
await act(async () => { await store.startStore(); });
await store.refreshAll();

const wait = (ms = 40) => act(async () => { await new Promise(r => setTimeout(r, ms)); });
const items = () => [...document.querySelectorAll('[role="menuitem"]')];
const labels = () => items().map(e => e.textContent.trim());
const pick = label => items().find(e => e.textContent.trim() === label);
const toasts = () => document.getElementById('toasts')?.textContent ?? '';
const json = JSON.stringify;
const check = (name, ok) => { checks.push({name, ok: !!ok}); console.log((ok ? 'PASS ' : 'FAIL ') + name); };
const group = async (id, fn) => { try { await fn(); } catch (e) { check(id + ' threw: ' + (e?.message || e), false); } finally { await act(async () => { closeMenu(); }); copied.length = 0; } };
const press = async el => { await act(async () => { el.dispatchEvent(new window.MouseEvent('contextmenu', {bubbles: true, cancelable: true, clientX: 30, clientY: 30})); }); await settle(); };
const tap = async el => { if (!el) return; await act(async () => { el.click(); await Promise.resolve(); }); await settle(); };
const rowOf = peer => [...document.querySelectorAll('.crow')].find(r => r.textContent.includes(peer === MAYA ? 'hi' : 'hello'));
let screens = 0;
// every check gets a freshly mounted screen (new key): a press in one must not leave its lift guard running in the next
const screen = async () => {
  platform = 'android';
  const went = [], closed = [];
  await render(h(PlatformCtx.Provider, {value: 'android'}, h(CtxMenu), h(ToastProbe), h(Chats, {key: 's' + (++screens), go: s => went.push(s), close: p => closed.push(p)})));
  await wait(80);
  return {went, closed};
};

await group('L1', async () => {
  const {went} = await screen();
  await press(rowOf(MAYA));
  check('L1 a long-press on a DM row opens a sheet with exactly Open, Contact info, Copy mention, Close DM', json(labels()) === '["Open","Contact info","Copy mention","Close DM"]');
  check('L1 the sheet is the phone bottom sheet (with its scrim)', !!document.querySelector('.sheet.contact-file-menu') && !!document.querySelector('.sheet-scrim'));
  check('L1 the long-press itself opens no chat', went.length === 0);
});
await group('L2', async () => {
  const {went} = await screen();
  await press(rowOf(MAYA)); await tap(pick('Open'));
  check('L2 Open opens that chat', json(went) === json([{s: 'conv', p: MAYA}]) && labels().length === 0);
});
await group('L3', async () => {
  const {went} = await screen();
  await press(rowOf(MAYA)); await tap(pick('Contact info'));
  check('L3 Contact info opens that contact info screen', json(went) === json([{s: 'info', p: MAYA}]));
});
await group('L4', async () => {
  await screen();
  await press(rowOf(MAYA)); await tap(pick('Copy mention')); await wait();
  check('L4 Copy mention copies @net:<address> and toasts Mention copied', json(copied) === json(['@net:' + MAYA]) && /Mention copied/.test(toasts()));
});
await group('L5', async () => {
  const {closed, went} = await screen();
  await press(rowOf(ORG)); await tap(pick('Close DM'));
  check('L5 Close DM closes that DM through the screen own close path and opens nothing', json(closed) === json([ORG]) && went.length === 0);
});
await group('L6', async () => {
  const {went} = await screen();
  const row = rowOf(MAYA);
  await press(row); await tap(row);
  check('L6 the click that follows a long-press does not open the chat', went.length === 0);
});
await group('L7', async () => {
  const {went} = await screen();
  await tap(rowOf(MAYA));
  check('L7 a plain tap still opens the chat and no sheet shows', json(went) === json([{s: 'conv', p: MAYA}]) && labels().length === 0);
});
await group('L8', async () => {
  const {went} = await screen();
  const row = rowOf(MAYA);
  await press(row); await act(async () => { closeMenu(); }); await wait(900); await tap(row);
  check('L8 a tap well after a long-press opens the chat again (the suppression is brief)', json(went) === json([{s: 'conv', p: MAYA}]));
});
await group('CT1', async () => {
  await render(h(PlatformCtx.Provider, {value: 'desktop'}, h(CtxMenu), h(ChatRows, {rows: [{peer: MAYA, last: null, unread: 0}], selected: null, onOpen: () => {}, onInfo: () => {}, onCloseDm: () => {}})));
  await wait();
  await press(document.querySelector('.crow'));
  check('CT1 desktop right-click on a row still gives the same four entries', json(labels()) === '["Open","Contact info","Copy mention","Close DM"]');
});
await group('CT2', async () => {
  const went = [];
  await render(h(PlatformCtx.Provider, {value: 'desktop'}, h(CtxMenu), h(ChatRows, {rows: [{peer: MAYA, last: null, unread: 0}], selected: null, onOpen: p => went.push(p), onInfo: () => {}, onCloseDm: () => {}})));
  await wait();
  await tap(document.querySelector('.crow'));
  check('CT2 desktop click on a row still opens the chat', json(went) === json([MAYA]));
});
await group('CT3', async () => {
  const {went} = await screen();
  await tap(rowOf(ORG)); await tap(rowOf(MAYA));
  check('CT3 two plain taps in a row both open (a tap sets no suppression)', went.length === 2);
});

const out = process.env.HUBCHAT_RENDERER_OUT;
if (process.env.HUBCHAT_RENDERER_EVIDENCE) fs.writeFileSync(process.env.HUBCHAT_RENDERER_EVIDENCE, JSON.stringify({checks}, null, 2));
fs.writeFileSync(path.join(out, 'result.json'), JSON.stringify({passed: checks.every(c => c.ok), checks, scope: 'Mounted Android chat list with a synthetic API and a recording clipboard stub; long-press = the contextmenu event; no native or pixel acceptance'}, null, 2));
const failed = [...new Set(checks.filter(c => !c.ok).map(c => c.name.split(' ')[0]))].sort();
const expectedRed = ['L1', 'L2', 'L3', 'L4', 'L5', 'L6'];
console.log('failed ids: ' + json(failed));
await act(async () => { closeMenu(); });
window.close();
process.exit(process.env.HUBCHAT_PROOF_PHASE === 'red' ? (json(failed) === json(expectedRed) ? 0 : 1) : (checks.every(c => c.ok) ? 0 : 1));
