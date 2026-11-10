// Orgtree mentions as chips (docket hubchat-render-orgtree-mentions-as-chips-linking;
// contract: Orgtree's docs/rust-engine/orgtree-links.md). Real Conversation and
// MessageView, mounted in an isolated DOM with a stubbed api: nothing is opened,
// no window, no network, no clipboard. Run ONLY through verify-orgtree-chips.ps1.
// Phase green: every check passes. Phase red (the base before the change):
// exactly the checks the change adds fail, so the controls really ran.
for (const k of ['HUBCHAT_DEPENDENCY_ROOT', 'HUBCHAT_DOM_DEPENDENCY_ROOT', 'HUBCHAT_RENDERER_OUT', 'HUBCHAT_RENDERER_EVIDENCE', 'HUBCHAT_JOB_CONTAINED']) {
  if (!process.env[k]) throw Error('Use the contained verifier; missing ' + k);
}
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
const repo = process.cwd();
const deps = createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT, 'package.json'));
const orgdeps = createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT, 'package.json'));
const esbuild = orgdeps('esbuild');
const out = process.env.HUBCHAT_RENDERER_OUT; fs.mkdirSync(out, { recursive: true });
await esbuild.build({ entryPoints: [path.join(repo, 'tools/orgtree-chips-entry.tsx')], outfile: path.join(out, 'entry.mjs'), bundle: true, format: 'esm', platform: 'browser', loader: { '.md': 'text' }, jsx: 'automatic',
  plugins: [{ name: 'existing-dependencies', setup(b) { b.onResolve({ filter: /^[^./][^:]*/ }, a => ({ path: deps.resolve(a.path === 'qrcode' ? 'qrcode/lib/browser.js' : a.path) })); } }] });
esbuild.stop();
const { JSDOM } = orgdeps('jsdom');
const dom = new JSDOM('<div id="root"></div>', { url: 'http://scratch.invalid' });
for (const key of ['window', 'document', 'navigator', 'HTMLElement', 'HTMLTextAreaElement', 'localStorage', 'location', 'getComputedStyle', 'Event', 'MouseEvent', 'KeyboardEvent', 'MutationObserver'])
  Object.defineProperty(globalThis, key, { value: key === 'getComputedStyle' ? dom.window[key].bind(dom.window) : dom.window[key], configurable: true });
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
// the context menu places itself inside the window
Object.assign(globalThis, { innerWidth: 1280, innerHeight: 800 });
globalThis.requestAnimationFrame = f => setTimeout(f, 0);
globalThis.cancelAnimationFrame = clearTimeout;
dom.window.HTMLElement.prototype.scrollIntoView = function () {};

const ORG = 'alpha.someone.1a2b3c', FOREIGN = 'alpha.other.9z9z9z', PAT = 'pat.abcdef';
const msg = (id, peer, outgoing, body) => ({ id, peer, outgoing, body, created_at: '2026-11-08T15:00:0' + id.length + 'Z', state: outgoing ? 'sent' : 'received', hub: null,
  attachments: [], error: null, seen: true, sent_at: null, received_at: null, fetched_at: null, delivered_at: null, read_at: null, kind: null, reply_to: null });
const chats = {
  [ORG]: [msg('o1', ORG, false, 'See @item:alpha/fix-login and @doc:beta/notes, ask @luna-reserve or @pat, not @user. Also @net:pat.abcdef'),
    msg('o22', ORG, true, 'And @item:gamma/plan please')],
  [FOREIGN]: [msg('f1', FOREIGN, false, 'Ours: @item:alpha/fix-login by @luna')],
  [PAT]: [msg('p1', PAT, false, 'Theirs: @item:alpha/fix-login and @luna'), msg('p22', PAT, true, 'Mine: @item:alpha/fix-login and @luna')],
};
const directory = [
  { address: ORG, kind: 'org', org_name: 'Alpha Works', username: '', blurb: '', online: true, last_seen: null, hubs: ['h'] },
  { address: FOREIGN, kind: 'org', org_name: 'Their Alpha', username: '', blurb: '', online: true, last_seen: null, hubs: ['h'] },
  { address: PAT, kind: 'person', org_name: '', username: 'pat', blurb: '', online: true, last_seen: null, hubs: ['h'] },
];
const localOrgs = [{ slug: 'alpha', name: 'Alpha Works', address: ORG }, { slug: 'gamma', name: 'Gamma Lab', address: 'gamma.someone.4d5e6f' }];
const checks = []; const check = (name, ok) => { ok = !!ok; checks.push({ name, ok }); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}`); };
const wait = (ms = 20) => new Promise(r => setTimeout(r, ms));

async function mount(tag, platform) {
  const m = await import(pathToFileURL(path.join(out, 'entry.mjs')).href + '?' + tag);
  const opened = [];
  Object.assign(m.api, {
    state: async () => ({ me: { address: 'scratch.123456' }, hubs: [], platform }),
    chats: async () => Object.entries(chats).map(([peer, list]) => ({ peer, last: list[list.length - 1], unread: 0 })),
    directory: async () => directory, transfers: async () => ({ rows: [], next: null }), onEvent: async () => () => {},
    draft: async () => '', setDraft: async () => {}, setupStatus: async () => null,
    sendRoute: async () => ({ pinned: null, automatic: null, next: null, hubs: [] }), loadOlder: async () => ({ added: 0, more: [], unreachable: [] }),
    chat: async p => chats[p] ?? [], send: async () => {},
    localOrgtreeOrgs: async () => (platform === 'desktop' ? localOrgs : []),
    orgtreeLinks: async () => platform === 'desktop',
    openOrgtreeLink: async (org, token) => { opened.push([org, token]); },
  });
  await m.act(async () => { await m.store.startStore(); });
  const host = document.createElement('div'); document.body.appendChild(host);
  const root = m.createRoot(host);
  const show = async peer => {
    await m.act(async () => { root.render(m.createElement(m.PlatformCtx.Provider, { value: platform },
      m.createElement(m.Conversation, { key: peer, peer, onBack: () => {}, onInfo: () => {}, onOpenAddr: () => {}, onContact: () => {} }),
      m.createElement(m.CtxMenu), m.createElement(m.ToastProbe))); });
    await m.act(async () => { await wait(40); });
  };
  const act = async f => { await m.act(async () => { f(); await wait(); }); };
  const closeMenu = () => act(() => m.closeMenu());
  return { host, root, show, opened, act, closeMenu, done: async () => { await m.act(async () => { root.unmount(); }); host.remove(); } };
}
const bubble = (host, id) => host.querySelector(`[data-id="${id}"] .bubble`) ?? host.querySelector(`#m-${id}`);
const chip = (host, id, text) => [...(bubble(host, id)?.querySelectorAll('a.oref') ?? [])].find(a => a.textContent === text) ?? null;
const click = (el, act) => act(() => el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true })));
const menuAt = async (el, act) => { await act(() => el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 5, clientY: 5 }))); return document.querySelector('.pop.ctx'); };
const menuButtons = () => [...document.querySelectorAll('.pop.ctx button.mi')];

// ------------------------------------------------------------ desktop
{
  const d = await mount('desktop', 'desktop');
  await d.show(ORG);
  const item = chip(d.host, 'o1', '@item:alpha/fix-login');
  check('D1 a ticket token in a local org\'s message is a chip that opens here, its copy the token',
    item && item.classList.contains('on') && item.classList.contains('k-item') && item.dataset.oref === '@item:alpha/fix-login'
    && item.title === 'Ticket alpha/fix-login · Open in Orgtree' && item.getAttribute('role') === 'link' && item.querySelector('svg.omark'));
  if (item) await click(item, d.act);
  check('D2 clicking it opens that item in Orgtree, navigation only', JSON.stringify(d.opened) === JSON.stringify([['alpha', '@item:alpha/fix-login']]));
  const doc = chip(d.host, 'o1', '@doc:beta/notes');
  if (doc) await click(doc, d.act);
  check('D3 a token of an org not on this PC is display-only with the hint',
    doc && !doc.classList.contains('on') && doc.title === 'Opens in Orgtree on the PC that runs beta' && !doc.hasAttribute('tabindex') && d.opened.length === 1);
  const node = chip(d.host, 'o1', '@luna-reserve');
  check('D4 a bare name in an org conversation is that org\'s node chip',
    node && node.classList.contains('on') && node.classList.contains('k-node') && node.dataset.oref === '@agent:alpha/luna-reserve'
    && node.title === 'Agent or project alpha/luna-reserve · Open in Orgtree');
  const o1 = bubble(d.host, 'o1');
  check('D5 a bare name that is a contact stays the contact mention',
    [...(o1?.querySelectorAll('a.addr.mention') ?? [])].some(a => a.textContent.includes('@pat')) && !chip(d.host, 'o1', '@pat'));
  check('D6 @user stays plain text', o1 && o1.textContent.includes('@user') && ![...o1.querySelectorAll('a')].some(a => a.textContent === '@user'));
  check('D7 @net: mentions are unchanged', o1 && o1.querySelector('a.addr.mention[data-slug="pat.abcdef"]'));
  const mine = chip(d.host, 'o22', '@item:gamma/plan');
  check('D8 my message to the org: a token of another org of this PC opens', mine && mine.classList.contains('on'));
  if (node) await d.act(() => node.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })));
  check('D9 Enter on an opening chip opens it', JSON.stringify(d.opened.at(-1)) === JSON.stringify(['alpha', '@agent:alpha/luna-reserve']));
  let menu = item ? await menuAt(item, d.act) : null;
  const first = menuButtons()[1];
  check('D10 the right-click menu's second entry opens just the org (the first opens the ticket)', menu && first && first.textContent.endsWith('Open Alpha Works in Orgtree') && !first.disabled && menuButtons()[0]?.textContent.endsWith('Open ticket in Orgtree'));
  const before = d.opened.length;
  if (first) await d.act(() => first.click());
  check('D11 the menu entry opens the org, not the item', d.opened.length === before + 1 && JSON.stringify(d.opened.at(-1)) === JSON.stringify(['alpha', null]));
  await d.closeMenu();
  menu = doc ? await menuAt(doc, d.act) : null;
  const off = menuButtons()[0];
  check('D12 the entry is disabled, with the hint, when the org is not here',
    menu && off && off.textContent.endsWith('Open beta in Orgtree') && off.disabled && off.title === 'Opens in Orgtree on the PC that runs beta');
  await d.closeMenu();
  await d.show(FOREIGN);
  const theirs = chip(d.host, 'f1', '@item:alpha/fix-login'), theirNode = chip(d.host, 'f1', '@luna');
  check('D13 another person\'s org that shares a slug: its chips are display-only (identity is the address)',
    theirs && !theirs.classList.contains('on') && theirNode && !theirNode.classList.contains('on') && !theirNode.dataset.oref
    && theirNode.title === 'Opens in Orgtree on the PC that runs Their Alpha');
  await d.show(PAT);
  const p1 = chip(d.host, 'p1', '@item:alpha/fix-login');
  check('D14 between people: their tokens are display-only chips', p1 && !p1.classList.contains('on'));
  check('D15 between people: bare names stay text', !chip(d.host, 'p1', '@luna') && !chip(d.host, 'p22', '@luna')
    && bubble(d.host, 'p1')?.textContent.includes('@luna'));
  const p22 = chip(d.host, 'p22', '@item:alpha/fix-login');
  check('D16 between people: my own tokens of this PC open', p22 && p22.classList.contains('on'));
  await d.done();
}
// ------------------------------------------------------------ phone
{
  const a = await mount('android', 'android');
  await a.show(ORG);
  const item = chip(a.host, 'o1', '@item:alpha/fix-login');
  if (item) await click(item, a.act);
  check('A1 on a phone a chip never opens, and a tap shows the hint',
    item && !item.classList.contains('on') && a.opened.length === 0 && document.getElementById('toasts')?.textContent === 'Opens in Orgtree on the PC that runs alpha');
  const target = item ?? bubble(a.host, 'o1');
  if (target) await a.act(() => target.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 5, clientY: 5 })));
  const sheet = [...document.querySelectorAll('.sheet [role="menuitem"]')].map(e => e.textContent.trim());
  check('A2 a phone's long-press sheet offers only Copy mention', JSON.stringify(sheet) === JSON.stringify(['Copy mention']));
  await a.done();
}
fs.writeFileSync(process.env.HUBCHAT_RENDERER_EVIDENCE, JSON.stringify({ checks }, null, 2));
const failed = checks.filter(c => !c.ok).map(c => c.name.split(' ')[0]).sort();
const expectedRed = ['A1', 'D1', 'D10', 'D11', 'D12', 'D13', 'D14', 'D16', 'D2', 'D3', 'D4', 'D8', 'D9'].sort();
console.log(`failed: ${JSON.stringify(failed)}`);
dom.window.close();
process.exit(process.env.HUBCHAT_PROOF_PHASE === 'red' ? (JSON.stringify(failed) === JSON.stringify(expectedRed) ? 0 : 1) : (checks.every(c => c.ok) ? 0 : 1));
