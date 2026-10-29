// Symmetry S4 #12/#13 (audit ux05 S3/S4): a contact in New chat is the Directory's contact row (about text,
// one chip per hub with the not-connected distinction) and has its right-click menu (desktop). Mounted real
// NewChatResults and Directory, synthetic API.
import {boot} from './symmetry-s4-harness.mjs';
const {mod, h, act, check, settle, click, render, finish, document, window} = await boot('symmetry-s4-newchat-entry.tsx');
const {api, Directory, NewChatResults, CtxMenu, PlatformCtx, store, closeMenu} = mod;
const A = 'https://a.invalid', B = 'https://b.invalid';
const hubs = [
  {url: A, name: 'office', state: 'connected', error: null, retry_at_ms: null, max_attachment_bytes: 1e9, features: ['v2']},
  {url: B, name: 'lab', state: 'disconnected', error: 'refused', retry_at_ms: null, max_attachment_bytes: 1e6, features: []}];
const maya = {address: 'maya.e71f2b', kind: 'person', org_name: '', username: 'Maya Lin', blurb: 'Design · Android', online: false, last_seen: null, hubs: [A, B]};
const kim = {address: 'kim.19ac02', kind: 'person', org_name: '', username: 'Kim Okafor', blurb: '', online: true, last_seen: null, hubs: [A]};
api.state = async platform => ({me: {id: 'me', address: 'me.aaaaaa', name: 'Me', about: ''}, hubs, platform: 'desktop', recovery_saved: true, file_background_hint: null});
api.chats = async () => [{peer: maya.address, last: null, unread: 0}];
api.directory = async () => [maya, kim];
api.transfers = async () => ({rows: [], next: null});
await store.refreshAll();
const rowOf = (root, addr) => [...root.querySelectorAll('.nc-row')].find(r => r.textContent.includes(addr) || r.textContent.includes(addr.split('.')[0]));
const opened = [], infos = [], dirs = [];
const results = (platform, q, r) => h(PlatformCtx.Provider, {value: platform}, h(CtxMenu), h(NewChatResults, {q, r, onOpen: a => opened.push(a), onInfo: a => infos.push(a), onDirectory: () => dirs.push(1)}));
const menuLabels = () => [...document.querySelectorAll('[role="menuitem"]')].map(e => e.textContent.trim());
const rightClick = async el => { await act(async () => { el.dispatchEvent(new window.MouseEvent('contextmenu', {bubbles: true, cancelable: true, clientX: 20, clientY: 20})); }); await settle(); };
const match = {valid: true, address: 'maya', exact: null, matches: [maya], is_me: false};
for (const platform of ['desktop', 'android']) {
  const c = (name, ok) => check(platform + ': ' + name, ok);
  await act(async () => { closeMenu(); }); // a menu left open by the other layout's checks
  opened.length = infos.length = 0;
  await render(results(platform, 'maya', match));
  const row = rowOf(document, 'Maya');
  c('a matching contact is listed', !!row);
  c('its about text shows, as in the Directory', /Design · Android/.test(row?.textContent || ''));
  const chips = [...(row?.querySelectorAll('.hubchip') || [])];
  c('one chip per hub that lists it', chips.length === 2 && chips.map(e => e.textContent).join() === 'office,lab');
  c('a hub that is not connected is marked as such', chips.find(e => e.textContent === 'lab')?.classList.contains('stale') && !chips.find(e => e.textContent === 'office')?.classList.contains('stale'));
  c('it is the Directory row (same row class)', row?.classList.contains('dir-row'));
  await click(row);
  c('clicking the row opens the chat', JSON.stringify(opened) === '["maya.e71f2b"]');
  // the Directory's own row for the same contact: same text, same chips
  await render(h(PlatformCtx.Provider, {value: platform}, h(Directory, {onOpen() {}, onInfo() {}, onClose() {}, onBack() {}})));
  const drow = rowOf(document, 'Maya');
  c('the Directory row for the same contact reads the same', !!drow && drow.querySelector('.t1')?.textContent === row?.querySelector('.t1')?.textContent && drow.querySelector('.t3')?.textContent === row?.querySelector('.t3')?.textContent && drow.querySelectorAll('.hubchip').length === chips.length);
  if (platform === 'desktop') {
    await render(results(platform, 'maya', match));
    await rightClick(rowOf(document, 'Maya'));
    c('right-click offers Message, Contact info and Copy address', JSON.stringify(menuLabels()) === '["Message","Contact info","Copy address"]');
    await click([...document.querySelectorAll('[role="menuitem"]')].find(e => e.textContent.trim() === 'Contact info'));
    c('Contact info opens that contact', JSON.stringify(infos) === '["maya.e71f2b"]');
    // the Recent rows of an empty search are the same row with the same menu
    await render(results(platform, '', null));
    const rec = rowOf(document, 'Maya');
    c('a Recent row has the same about text and chips', /Design · Android/.test(rec?.textContent || '') && rec?.querySelectorAll('.hubchip').length === 2);
    await rightClick(rec);
    c('a Recent row has the same menu', JSON.stringify(menuLabels()) === '["Message","Contact info","Copy address"]');
  } else {
    await render(results(platform, 'maya', match));
    await rightClick(rowOf(document, 'Maya'));
    c('Android has no right-click menu', menuLabels().length === 0);
  }
}
await finish('Mounted desktop/Android New chat rows and Directory with synthetic API; no hub, native or pixel acceptance');
