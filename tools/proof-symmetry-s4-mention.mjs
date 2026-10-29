// Symmetry S4 #10 (audit ux02 S4): right-clicking an address in a message offers the contact's own actions,
// Contact info included, the same block the chat list and the Directory offer. Mounted real MessageView and the
// shared chat menu, synthetic API.
import {boot} from './symmetry-s4-harness.mjs';
const {mod, h, act, check, settle, click, render, finish, document, window} = await boot('symmetry-s4-mention-entry.tsx');
const {api, store, MessageView, chatMenu, CtxMenu, PlatformCtx} = mod;
const peer = 'maya.e71f2b';
const m = {id: 'm1', peer: 'casey.abcdef', outgoing: false, hub: 'https://fixture.invalid', body: 'Ask @net:' + peer + ' about it', kind: null, reply_to: null,
  created_at: '2026-10-17T00:00:00Z', sent_at: null, received_at: null, state: 'received', attachments: []};
const opened = [], infos = [];
const labels = () => [...document.querySelectorAll('[role="menuitem"]')].map(e => e.textContent.trim());
const rightClick = async el => { await act(async () => { el.dispatchEvent(new window.MouseEvent('contextmenu', {bubbles: true, cancelable: true, clientX: 30, clientY: 30})); }); await settle(); };
const noop = () => {};
const view = (withInfo) => h(PlatformCtx.Provider, {value: 'desktop'}, h(CtxMenu), h(MessageView, {m, first: true, peerName: 'Casey', peerKind: 'person', quoted: undefined, hover: false,
  onReply: noop, onInfo: noop, onDelete: noop, onJump: noop, onOpenAddr: a => opened.push(a), ...(withInfo ? {onContactInfo: a => infos.push(a)} : {})}));
// the directory is not known yet: a mention blocks nothing (inline mentions, 2026-10-29)
await render(view(true));
const link = document.querySelector('a.addr');
check('the address in the message is a link', !!link);
await rightClick(link);
const got = labels();
check('right-click on an address leads with Open chat, Contact info, Copy address', JSON.stringify(got.slice(0, 3)) === '["Open chat","Contact info","Copy address"]');
check('the message actions follow the contact block', got.includes('Reply') && got.indexOf('Reply') > 2);
await click([...document.querySelectorAll('[role="menuitem"]')].find(e => e.textContent.trim() === 'Contact info'));
check('Contact info opens that address\'s contact, not the open chat\'s', JSON.stringify(infos) === '["maya.e71f2b"]' && opened.length === 0);
// the chat list / Directory menu for the same contact: same entries, same order (only the first label differs by place)
await render(h(PlatformCtx.Provider, {value: 'desktop'}, h(CtxMenu), h('div', {id: 'row', onContextMenu: e => chatMenu(e, peer, noop, noop, 'Message')}, 'row')));
await rightClick(document.getElementById('row'));
const list = labels();
check('the chat menu offers the same block for the same contact', JSON.stringify(list.slice(1)) === JSON.stringify(got.slice(1, 3)) || JSON.stringify(list) === '["Message","Contact info","Copy address"]');
// without a Contact info handler (a surface that cannot open it) the entry is not offered
await render(view(false));
await rightClick(document.querySelector('a.addr'));
check('no Contact info entry where the surface cannot open it', !labels().includes('Contact info') && labels()[0] === 'Open chat');
// an address no hub of mine lists has no chat to open: only Copy address (the inline-mentions rule), no Contact info either
const HUB = 'https://a.invalid', maya = {address: peer, kind: 'person', org_name: '', username: 'Maya Lin', blurb: '', online: true, last_seen: null, hubs: [HUB]};
api.state = async () => ({me: {id: 'me', address: 'scratch.123456', name: 'Me', about: ''}, hubs: [{url: HUB, name: 'office', state: 'connected', error: null, retry_at_ms: null, max_attachment_bytes: 1e9, features: []}], platform: 'desktop', recovery_saved: true, file_background_hint: null});
api.chats = async () => []; api.directory = async () => [maya]; api.transfers = async () => ({rows: [], next: null});
await store.refreshAll();
await render(view(true));
await rightClick(document.querySelector('a.addr'));
check('a mention some hub of mine lists keeps the whole contact block', JSON.stringify(labels().slice(0, 3)) === '["Open chat","Contact info","Copy address"]');
m.body = 'Ask @net:ghost.aaaaaa about it';
await render(h(PlatformCtx.Provider, {value: 'desktop'}, h(CtxMenu), h(MessageView, {m: {...m, id: 'm2'}, first: true, peerName: 'Casey', peerKind: 'person', quoted: undefined, hover: false, onReply: noop, onInfo: noop, onDelete: noop, onJump: noop, onOpenAddr: a => opened.push(a), onContactInfo: a => infos.push(a)})));
await rightClick(document.querySelector('a.addr'));
check('an address no hub lists offers only Copy address, no Open chat or Contact info', labels()[0] === 'Copy address' && !labels().includes('Open chat') && !labels().includes('Contact info'));
await finish('Mounted desktop MessageView address menu and the chat menu with synthetic API; no native or pixel acceptance');
