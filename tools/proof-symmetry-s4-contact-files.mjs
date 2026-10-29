// Symmetry S4 #28 (audit ux07 S4): Contact info Files says it is loading, says why a read failed and offers
// Retry, and says "No files yet" only after a read that succeeded. Mounted real ContactInfo, synthetic API.
import {boot} from './symmetry-s4-harness.mjs';
const {mod, h, check, settle, click, render, finish, document} = await boot('symmetry-s4-contact-files-entry.tsx');
const {api, ContactInfo, PlatformCtx} = mod;
const peer = 'casey.abcdef';
const att = (id, name) => ({local_id: id, name, bytes: 1200, hub_id: id, source: null, local_path: null, state: 'remote', error: null});
const msg = (id, attachments = []) => ({id, peer, outgoing: false, hub: 'https://fixture.invalid', body: 'x', kind: null, reply_to: null,
  created_at: '2026-10-17T00:00:00Z', sent_at: null, received_at: null, state: 'received', attachments});
let calls = 0, mode = 'ok', gate = null, files = [];
api.chat = async () => { calls++; if (gate) await gate; if (mode === 'fail') throw 'the history could not be read'; return files.length ? [msg('m1', files)] : [msg('m0')]; };
api.onEvent = async () => () => {};
api.download = async () => {}; api.openAttachment = async () => {};
const files$ = () => {
  const head = [...document.querySelectorAll('h4,.sec-h')].find(e => e.textContent.trim() === 'Files');
  const sec = head && (head.closest('.info-sec') || head.parentElement);
  return head && sec ? (head.closest('.info-sec') ? sec.textContent : (() => { let t = ''; for (let n = head.nextElementSibling; n && !n.classList.contains('sec-h'); n = n.nextElementSibling) t += n.textContent + '|'; return t; })()) : '';
};
const btn = text => [...document.querySelectorAll('button')].find(b => b.textContent.trim() === text);
const mount = platform => render(h(PlatformCtx.Provider, {value: platform}, h(ContactInfo, {peer, onClose() {}, onShowMessage() {}})));
for (const platform of ['desktop', 'android']) {
  const c = (name, ok) => check(platform + ': ' + name, ok);
  // loading: the read has not answered yet
  mode = 'ok'; files = []; calls = 0; let release; gate = new Promise(r => release = r);
  await render(null); await mount(platform);
  c('Files says it is loading while the read is out', /Loading/i.test(files$()));
  c('"No files yet" is not claimed before the read answers', !/No files yet/.test(files$()));
  release(); gate = null; await settle();
  c('after a successful empty read it says No files yet', /No files yet/.test(files$()) && !/Loading/i.test(files$()));
  // failure: the read throws
  mode = 'fail'; calls = 0; await render(null); await mount(platform); await settle();
  c('a failed read says why, in the Files section', /the history could not be read/.test(files$()));
  c('a failed read does not claim there are no files', !/No files yet/.test(files$()));
  c('a failed read offers Retry', !!btn('Retry'));
  const before = calls; mode = 'ok'; files = [att('a1', 'report.pdf')];
  await click(btn('Retry')); await settle();
  c('Retry asks for the messages again', calls > before);
  c('after Retry the error and Retry are gone and the file is listed', !/the history could not be read/.test(files$()) && !btn('Retry') && /report\.pdf/.test(files$()));
  // control: a good read lists its file
  mode = 'ok'; files = [att('a2', 'kept.txt')]; await render(null); await mount(platform); await settle();
  c('a good read lists its file', /kept\.txt/.test(files$()));
}
await finish('Mounted desktop/Android ContactInfo Files with synthetic API; no native opener, hub or pixel acceptance');
