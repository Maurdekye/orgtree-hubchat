// "Copy mention" and the left-click action in Hubchat's context menus (docket hubchat-copy-mention-context-menu-entry;
// spec: hubchat-design-opus/spec-copy-mention.md at 19f24b8). Real MessageView, Conversation, ChatRows, RailRows,
// ContactRow and ConvMenu mounted in an isolated DOM with a synthetic API: nothing is opened, no window, no network.
// The clipboard is a recording stub (navigator.clipboard.writeText); the real clipboard is never touched.
// Run under the repository Job limiter (HUBCHAT_JOB_CONTAINED=1, an assigned C:\orgtree-build output).
// Phase green: every check passes. Phase red (HUBCHAT_PROOF_PHASE=red, the base before the change): exactly C1-C13
// fail and the controls CT1-CT4 pass, so the controls really ran.
import fs from 'node:fs';
import path from 'node:path';
import {boot} from './symmetry-s4-harness.mjs';
const {mod, h, act, checks, settle, render, document, window} = await boot('copy-mention-entry.tsx');
const {api, store, MessageView, Conversation, ChatRows, RailRows, ContactRow, ConvMenu, CtxMenu, closeMenu, PlatformCtx, ToastProbe} = mod;
for (const k of ['getComputedStyle', 'MutationObserver']) Object.defineProperty(globalThis, k, {value: k === 'getComputedStyle' ? window[k].bind(window) : window[k], configurable: true});
globalThis.requestAnimationFrame = f => setTimeout(f, 0);
globalThis.cancelAnimationFrame = clearTimeout;
window.HTMLElement.prototype.scrollIntoView = function () {};

// the recording clipboard stub
const copied = [];
Object.defineProperty(window.navigator, 'clipboard', {value: {writeText: async t => { copied.push(t); }}, configurable: true});

const HUB = 'https://fixture.invalid';
const MAYA = 'maya.e71f2b', ORG = 'acme.4f2a9c', GHOST = 'ghost.000000', FOREIGN = 'beta.other.9z9z9z';
const hubs = [{url: HUB, name: 'office', state: 'connected', error: null, retry_at_ms: null, max_attachment_bytes: 1e9, features: []}];
const directory = [
  {address: MAYA, kind: 'person', org_name: '', username: 'maya', blurb: '', online: true, last_seen: null, hubs: [HUB]},
  {address: ORG, kind: 'org', org_name: 'Alpha Works', username: '', blurb: '', online: true, last_seen: null, hubs: [HUB]},
  {address: FOREIGN, kind: 'org', org_name: 'Their Beta', username: '', blurb: '', online: true, last_seen: null, hubs: [HUB]}];
const localOrgs = [{slug: 'alpha', name: 'Alpha Works', address: ORG}];
const opened = [];
let platform = 'desktop';
const msg = (id, peer, body, outgoing = false) => ({id, peer, outgoing, body, hub: HUB, kind: null, reply_to: null, created_at: '2026-11-10T00:00:0' + id.length + 'Z',
  sent_at: null, received_at: null, state: outgoing ? 'sent' : 'received', attachments: [], error: null, seen: true, fetched_at: null, delivered_at: null, read_at: null});
const chats = {[ORG]: [msg('o1', ORG, 'hello')], [MAYA]: [msg('p1', MAYA, 'hi')]};
Object.assign(api, {
  state: async () => ({me: {id: 'me', address: 'scratch.123456', name: 'Me', about: ''}, hubs, platform, recovery_saved: true, file_background_hint: null}),
  chats: async () => Object.entries(chats).map(([peer, list]) => ({peer, last: list[list.length - 1], unread: 0})),
  directory: async () => directory, transfers: async () => ({rows: [], next: null}), onEvent: async () => () => {},
  draft: async () => '', setDraft: async () => {}, setupStatus: async () => null,
  sendRoute: async () => ({pinned: null, automatic: null, next: null, hubs: []}), loadOlder: async () => ({added: 0, more: [], unreachable: []}),
  chat: async p => chats[p] ?? [], send: async () => {},
  localOrgtreeOrgs: async () => (platform === 'desktop' ? localOrgs : []),
  orgtreeLinks: async () => platform === 'desktop',
  openOrgtreeLink: async (org, token) => { opened.push([org, token]); },
});
await act(async () => { await store.startStore(); });
await store.refreshAll();

const wait = (ms = 40) => act(async () => { await new Promise(r => setTimeout(r, ms)); });
const items = () => [...document.querySelectorAll('[role="menuitem"]')];
const labels = () => items().map(e => e.textContent.trim());
const pick = label => items().find(e => e.textContent.trim() === label);
const toasts = () => document.getElementById('toasts')?.textContent ?? '';
const json = JSON.stringify;
const noop = () => {};
const check = (name, ok) => { checks.push({name, ok: !!ok}); console.log((ok ? 'PASS ' : 'FAIL ') + name); };
const group = async (id, fn) => { try { await fn(); } catch (e) { check(id + ' threw: ' + (e?.message || e), false); } finally { await act(async () => { closeMenu(); }); copied.length = 0; } };
/** dispatches a contextmenu event; returns whether the page left it alone (not default-prevented) */
const rightClick = async el => {
  let free = true;
  await act(async () => { free = el.dispatchEvent(new window.MouseEvent('contextmenu', {bubbles: true, cancelable: true, clientX: 30, clientY: 30})); });
  await settle();
  return !free;
};
const chip = (text) => [...document.querySelectorAll('a.oref')].find(a => a.textContent === text);
const view = (m, props = {}) => h(PlatformCtx.Provider, {value: platform}, h(CtxMenu), h(ToastProbe),
  h(MessageView, {m, first: true, peerName: m.peer === ORG ? 'Alpha Works' : m.peer === FOREIGN ? 'Their Beta' : 'Maya', peerKind: m.peer === MAYA ? 'person' : 'org', quoted: undefined, hover: false,
    onReply: noop, onInfo: noop, onDelete: noop, onJump: noop, onOpenAddr: noop, ...props}));
const showView = async (m, props) => { await render(view(m, props)); await wait(); };

// ------------------------------------------------------------ desktop
platform = 'desktop';
await group('C1', async () => {
  const picked = [], infos = [], closed = [];
  const list = [{peer: MAYA, last: null, unread: 0}];
  await render(h(PlatformCtx.Provider, {value: 'desktop'}, h(CtxMenu), h(ToastProbe),
    h(ChatRows, {rows: list, selected: null, onOpen: p => picked.push(p), onInfo: p => infos.push(p), onCloseDm: p => closed.push(p)})));
  await wait();
  await rightClick(document.querySelector('.crow'));
  check('C1 chat-row menu is Open, Contact info, Copy mention, Close DM (no Copy address)', json(labels()) === '["Open","Contact info","Copy mention","Close DM"]');
  await act(async () => { pick('Copy mention')?.click(); await Promise.resolve(); }); await settle();
  check('C1 Copy mention copies @net:<address> and says "Mention copied"', json(copied) === json(['@net:' + MAYA]) && toasts().endsWith('Mention copied'));
});
await group('C2', async () => {
  const list = [{peer: MAYA, last: null, unread: 0}];
  await render(h(PlatformCtx.Provider, {value: 'desktop'}, h(CtxMenu), h(ToastProbe),
    h(RailRows, {rows: list, selected: null, onOpen: noop, onInfo: noop, onCloseDm: noop})));
  await wait();
  await rightClick(document.querySelector('.rrow'));
  check('C2 collapsed-rail menu has the same entries as the chat list', json(labels()) === '["Open","Contact info","Copy mention","Close DM"]');
});
await group('C3', async () => {
  const row = c => h(PlatformCtx.Provider, {value: 'desktop'}, h(CtxMenu), h(ToastProbe), h(ContactRow, {c, hubs, onOpen: noop, onInfo: noop}));
  await render(row(directory[0]));
  await rightClick(document.querySelector('.nc-row'));
  check('C3 directory-row menu is Message, Contact info, Copy mention', json(labels()) === '["Message","Contact info","Copy mention"]');
  await act(async () => { closeMenu(); });
  await render(row(directory[1]));
  await rightClick(document.querySelector('.nc-row'));
  await act(async () => { pick('Copy mention')?.click(); await Promise.resolve(); }); await settle();
  check('C3 an org copies its @net: address, not @org:', json(copied) === json(['@net:' + ORG]));
});
await group('C4', async () => {
  const opens = [], infos = [];
  const m = msg('m4', ORG, 'ask @maya about it');
  await showView(m, {onOpenAddr: a => opens.push(a), onContactInfo: a => infos.push(a)});
  const link = document.querySelector('a.addr');
  await act(async () => { link.click(); await Promise.resolve(); }); await settle();
  const leftClick = [...opens];
  await rightClick(link);
  check('C4 an in-text mention menu leads with Open chat, Contact info, Copy mention', json(labels().slice(0, 3)) === '["Open chat","Contact info","Copy mention"]');
  await act(async () => { pick('Open chat')?.click(); await Promise.resolve(); }); await settle();
  check('C4 Open chat does exactly what a left-click on the link does', json(leftClick) === json([MAYA]) && json(opens) === json([MAYA, MAYA]));
  await rightClick(link);
  await act(async () => { pick('Copy mention')?.click(); await Promise.resolve(); }); await settle();
  check('C4 Copy mention copies the resolved @net: address, not the text as written', json(copied) === json(['@net:' + MAYA]));
});
await group('C5', async () => {
  await showView(msg('m5', ORG, 'ask @net:' + GHOST + ' later'));
  await rightClick(document.querySelector('a.addr'));
  const got = labels();
  check('C5 a mention no hub lists offers only Copy mention, then the message actions', got[0] === 'Copy mention' && got[1] === 'Reply' && !got.includes('Open chat') && !got.includes('Contact info'));
});
await group('C6', async () => {
  const token = '@item:alpha/fix-login';
  await showView(msg('m6', ORG, 'see ' + token + ' now'));
  const c = chip(token);
  await rightClick(c);
  check('C6 a chip that opens here: Open ticket in Orgtree, Open Alpha Works in Orgtree, Copy mention',
    json(labels().slice(0, 3)) === '["Open ticket in Orgtree","Open Alpha Works in Orgtree","Copy mention"]');
  await act(async () => { c.click(); await Promise.resolve(); }); await settle();
  const leftClick = [...opened];
  opened.length = 0;
  await rightClick(c);
  await act(async () => { pick('Open ticket in Orgtree')?.click(); await Promise.resolve(); }); await settle();
  check('C6 the first entry opens exactly what a left-click opens', json(leftClick) === json([['alpha', token]]) && json(opened) === json([['alpha', token]]));
  await rightClick(c);
  await act(async () => { pick('Copy mention')?.click(); await Promise.resolve(); }); await settle();
  check('C6 Copy mention copies the token as written', json(copied) === json([token]));
});
await group('C7', async () => {
  const token = '@doc:beta/plan';
  await showView(msg('m7', ORG, 'see ' + token + ' now'));
  await rightClick(chip(token));
  const first = items()[0];
  check('C7 a chip of an org not on this PC: no "Open document in Orgtree"; the org entry stays, disabled',
    !labels().includes('Open document in Orgtree') && first && first.textContent.trim() === 'Open beta in Orgtree' && first.disabled);
  await act(async () => { pick('Copy mention')?.click(); await Promise.resolve(); }); await settle();
  check('C7 Copy mention copies the token as written', json(copied) === json([token]));
});
await group('C8', async () => {
  await showView(msg('m8', ORG, 'ask @luna-reserve please'));
  await rightClick(chip('@luna-reserve'));
  await act(async () => { pick('Copy mention')?.click(); await Promise.resolve(); }); await settle();
  const local = [...copied];
  copied.length = 0;
  await act(async () => { closeMenu(); });
  await showView(msg('m9', FOREIGN, 'ask @luna-reserve please'));
  await rightClick(chip('@luna-reserve'));
  await act(async () => { pick('Copy mention')?.click(); await Promise.resolve(); }); await settle();
  check('C8 a bare-name chip copies @agent:<org>/<name> in an org on this PC, @<name> as written otherwise',
    json(local) === json(['@agent:alpha/luna-reserve']) && json(copied) === json(['@luna-reserve']));
});
await group('C9', async () => {
  const infos = [], closed = [];
  await render(h(PlatformCtx.Provider, {value: 'desktop'}, h(Conversation, {key: MAYA, peer: MAYA, onBack: noop, onInfo: noop, onOpenAddr: noop,
    onContact: () => infos.push(1), onCloseDm: () => closed.push(1)}), h(CtxMenu), h(ToastProbe)));
  await wait(80);
  await rightClick(document.querySelector('.conv-head .who'));
  check('C9 the chat header has a menu: Contact info, Copy mention, Close DM', json(labels()) === '["Contact info","Copy mention","Close DM"]');
  await act(async () => { pick('Contact info')?.click(); await Promise.resolve(); }); await settle();
  check('C9 Contact info does what a click on the header does', infos.length === 1);
  await rightClick(document.querySelector('.conv-head .conv-av'));
  check('C9 the avatar opens the same menu', json(labels()) === '["Contact info","Copy mention","Close DM"]');
  await act(async () => { pick('Copy mention')?.click(); await Promise.resolve(); }); await settle();
  check('C9 Copy mention copies the peer\'s @net: address', json(copied) === json(['@net:' + MAYA]));
});

// ------------------------------------------------------------ phone
platform = 'android';
await store.refreshAll();
await group('C10', async () => {
  await render(h(PlatformCtx.Provider, {value: 'android'}, h(ToastProbe), h(ConvMenu, {peer: MAYA, onContact: noop, onClose: noop, onCloseDm: noop})));
  await wait();
  const got = labels();
  check('C10 the conversation menu has Copy mention, not Copy address', got.includes('Copy mention') && !got.includes('Copy address'));
  await act(async () => { pick('Copy mention')?.click(); await Promise.resolve(); }); await settle();
  check('C10 it copies @net:<peer> and says "Mention copied"', json(copied) === json(['@net:' + MAYA]) && toasts().endsWith('Mention copied'));
});
await group('C11', async () => {
  const selected = [], opens = [];
  await showView(msg('m11', ORG, 'ask @maya about it'), {onSelect: m => selected.push(m.id), onOpenAddr: a => opens.push(a), onContactInfo: noop});
  const prevented = await rightClick(document.querySelector('a.addr'));
  check('C11 long-press on an in-text mention opens a sheet: Open chat, Contact info, Copy mention', json(labels()) === '["Open chat","Contact info","Copy mention"]' && !!document.querySelector('.sheet'));
  check('C11 the page claims the gesture and selects no message', prevented && selected.length === 0);
  await act(async () => { pick('Open chat')?.click(); await Promise.resolve(); }); await settle();
  check('C11 Open chat is what a tap does', json(opens) === json([MAYA]));
});
await group('C12', async () => {
  const token = '@item:alpha/fix-login';
  await showView(msg('m12', ORG, 'see ' + token + ' now'));
  await rightClick(chip(token));
  check('C12 long-press on a chip opens a sheet with only Copy mention', json(labels()) === '["Copy mention"]');
  await act(async () => { pick('Copy mention')?.click(); await Promise.resolve(); }); await settle();
  check('C12 it copies the token as written', json(copied) === json([token]));
});
await group('C13', async () => {
  platform = 'android';
  await showView(msg('m13', ORG, 'ask @maya about it'), {mentionOpen: false, onContactInfo: noop});
  await rightClick(document.querySelector('a.addr'));
  const phone = labels();
  await act(async () => { closeMenu(); });
  platform = 'desktop';
  await showView(msg('m14', ORG, 'ask @maya about it'), {mentionOpen: false, onContactInfo: noop});
  await rightClick(document.querySelector('a.addr'));
  const desk = labels();
  check('C13 a Message info preview offers only Copy mention for its target (phone and desktop)',
    json(phone) === '["Copy mention"]' && desk[0] === 'Copy mention' && !desk.includes('Open chat') && !desk.includes('Contact info'));
});

// ------------------------------------------------------------ controls (unchanged by the feature)
platform = 'desktop';
await group('CT1', async () => {
  await showView(msg('c1', ORG, 'see https://example.com/a now'));
  await rightClick(document.querySelector('a.md-a'));
  check('CT1 the link menu still starts Open link, Copy link', json(labels().slice(0, 2)) === '["Open link","Copy link"]');
});
await group('CT2', async () => {
  platform = 'android';
  await showView(msg('c2', ORG, 'just some words here'));
  const prevented = await rightClick(document.querySelector('.mtext') || document.querySelector('.bubble'));
  check('CT2 on a phone, long-press on plain message text opens no menu and is not claimed', labels().length === 0 && !prevented);
  platform = 'desktop';
});
await group('CT3', async () => {
  await render(h(PlatformCtx.Provider, {value: 'desktop'}, h(Conversation, {key: 'ct3', peer: MAYA, onBack: noop, onInfo: noop, onOpenAddr: noop, onContact: noop}), h(CtxMenu)));
  await wait(80);
  check('CT3 the header copy button beside the shown address is still "Copy address"', !!document.querySelector('.conv-head button[aria-label="Copy address"]'));
});
{
  const repo = process.cwd();
  const settings = fs.readFileSync(path.join(repo, 'src/components/Settings.tsx'), 'utf8');
  check('CT4 Settings\' own-address buttons are still "Copy address" (source check)', (settings.match(/aria-label="Copy address"/g) || []).length >= 1 && /Address copied/.test(settings));
}

const out = process.env.HUBCHAT_RENDERER_OUT;
if (process.env.HUBCHAT_RENDERER_EVIDENCE) fs.writeFileSync(process.env.HUBCHAT_RENDERER_EVIDENCE, JSON.stringify({checks}, null, 2));
fs.writeFileSync(path.join(out, 'result.json'), JSON.stringify({passed: checks.every(c => c.ok), checks, scope: 'Mounted desktop and phone menus with a synthetic API and a recording clipboard stub; no native or pixel acceptance'}, null, 2));
const failed = [...new Set(checks.filter(c => !c.ok).map(c => c.name.split(' ')[0]))].sort();
const expectedRed = ['C1', 'C10', 'C11', 'C12', 'C13', 'C2', 'C3', 'C4', 'C5', 'C6', 'C7', 'C8', 'C9'].sort();
console.log('failed ids: ' + json(failed));
await act(async () => { closeMenu(); });
window.close();
process.exit(process.env.HUBCHAT_PROOF_PHASE === 'red' ? (json(failed) === json(expectedRed) ? 0 : 1) : (checks.every(c => c.ok) ? 0 : 1));
