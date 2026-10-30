// Hubchat contact mentions (docket hubchat-mentions-mention-hubchat-contacts-as-lin).
// Isolated DOM fixture proof. Run ONLY through verify-mentions-renderer.ps1.
// Phase green: every check passes. Phase red (the base before the change):
// exactly the checks the change adds fail, so the controls really ran.
for (const k of ['HUBCHAT_DEPENDENCY_ROOT', 'HUBCHAT_DOM_DEPENDENCY_ROOT', 'HUBCHAT_RENDERER_OUT', 'HUBCHAT_RENDERER_EVIDENCE', 'HUBCHAT_JOB_CONTAINED']) {
  if (!process.env[k]) throw Error('Use the contained verifier; missing ' + k);
}
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';

const repo = process.cwd();
const phase = process.env.HUBCHAT_PROOF_PHASE || 'green';
const deps = createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT, 'package.json'));
const orgdeps = createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT, 'package.json'));
const esbuild = orgdeps('esbuild');
const out = process.env.HUBCHAT_RENDERER_OUT;
fs.mkdirSync(out, { recursive: true });

const baseRev = '6e33076b6c41bf0bf1a624707298d213faf7300c';
const baseFiles = phase === 'red' ? new Map([
  [path.resolve(repo, 'src/lib/mentions.ts').toLowerCase(), execFileSync('git', ['show', `${baseRev}:src/lib/mentions.ts`], { cwd: repo, encoding: 'utf8' })],
  [path.resolve(repo, 'src/components/Composer.tsx').toLowerCase(), execFileSync('git', ['show', `${baseRev}:src/lib/mentions.ts`], { cwd: repo, encoding: 'utf8' }) ? execFileSync('git', ['show', `${baseRev}:src/components/Composer.tsx`], { cwd: repo, encoding: 'utf8' }) : ''],
  [path.resolve(repo, 'src/components/Conversation.tsx').toLowerCase(), execFileSync('git', ['show', `${baseRev}:src/components/Conversation.tsx`], { cwd: repo, encoding: 'utf8' })],
]) : null;

await esbuild.build({
  entryPoints: [path.join(repo, 'tools/mentions-renderer-entry.tsx')],
  outfile: path.join(out, `entry-${phase}.mjs`),
  bundle: true,
  format: 'esm',
  platform: 'browser',
  loader: { '.md': 'text' },
  jsx: 'automatic',
  plugins: [{
    name: 'existing-dependencies',
    setup(b) {
      if (baseFiles) {
        b.onLoad({ filter: /\.(ts|tsx)$/ }, (args) => {
          const hit = baseFiles.get(path.resolve(args.path).toLowerCase());
          if (hit !== undefined) {
            return { contents: hit, loader: args.path.endsWith('.tsx') ? 'tsx' : 'ts', resolveDir: path.dirname(args.path) };
          }
          return null;
        });
      }
      b.onResolve({ filter: /^[^./][^:]*/ }, (a) => ({
        path: deps.resolve(a.path === 'qrcode' ? 'qrcode/lib/browser.js' : a.path),
      }));
    },
  }],
});
esbuild.stop();

const { JSDOM } = orgdeps('jsdom');
const dom = new JSDOM('<div id="root"></div>', { url: 'http://scratch.invalid' });
for (const key of ['window', 'document', 'navigator', 'HTMLElement', 'HTMLTextAreaElement', 'localStorage', 'location', 'getComputedStyle', 'Event', 'MouseEvent', 'KeyboardEvent', 'MutationObserver']) {
  Object.defineProperty(globalThis, key, {
    value: key === 'getComputedStyle' ? dom.window[key].bind(dom.window) : dom.window[key],
    configurable: true,
  });
}
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
globalThis.requestAnimationFrame = (f) => setTimeout(f, 0);
globalThis.cancelAnimationFrame = clearTimeout;

Object.defineProperties(dom.window.HTMLElement.prototype, {
  clientHeight: {
    configurable: true,
    get() {
      if (this.classList?.contains('mention-list')) {
        const total = 8 + this.querySelectorAll('.mention-item').length * 36;
        return Math.min(224, total);
      }
      return 0;
    },
  },
  scrollHeight: {
    configurable: true,
    get() {
      if (this.classList?.contains('mention-list')) {
        return 8 + this.querySelectorAll('.mention-item').length * 36;
      }
      return 0;
    },
  },
  offsetTop: {
    configurable: true,
    get() {
      if (this.classList?.contains('mention-item') && this.parentElement) {
        const items = [...this.parentElement.querySelectorAll('.mention-item')];
        const idx = items.indexOf(this);
        return idx >= 0 ? 4 + idx * 36 : 0;
      }
      return 0;
    },
  },
  offsetHeight: {
    configurable: true,
    get() {
      if (this.classList?.contains('mention-item')) return 34;
      return 0;
    },
  },
});

const { api, store, Conversation, setReplies, act, createElement, createRoot } = await import(pathToFileURL(path.join(out, `entry-${phase}.mjs`)));

const peer = 'pat.abcdef';
const otherPeer = 'bob.333333';
let directory = [
  { address: peer, kind: 'person', org_name: '', username: 'Pat', blurb: '', online: true, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'alice.111111', kind: 'person', org_name: '', username: 'Alice', blurb: '', online: true, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'acme.222222', kind: 'org', org_name: 'acme', username: 'Acme Org', blurb: '', online: true, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: otherPeer, kind: 'person', org_name: '', username: 'Bob', blurb: '', online: true, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'zara.444444', kind: 'person', org_name: '', username: 'Zara', blurb: '', online: false, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'old_mention.555555', kind: 'person', org_name: '', username: 'Old Mention', blurb: '', online: false, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'aa_quiz.777777', kind: 'person', org_name: '', username: 'Aa-Quiz', blurb: '', online: false, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'quiz.888888', kind: 'person', org_name: '', username: 'Quiz', blurb: '', online: true, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'scratch.123456', kind: 'person', org_name: '', username: 'Me', blurb: '', online: true, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'link.999999', kind: 'person', org_name: '', username: '', blurb: 'Waiting to be linked to a Hubchat identity', online: false, last_seen: null, hubs: ['wss://hub.invalid'] },
];
const msg = {
  id: 'in-1',
  peer,
  outgoing: false,
  body: 'Ping @net:pat.abcdef or @alice or @org:acme, not @stranger or user@alice.111111. Code: `@net:zara.444444`.',
  created_at: '2026-10-17T21:00:00Z',
  state: 'received',
  hub: null,
  attachments: [],
  error: null,
  seen: true,
  sent_at: null,
  received_at: null,
  fetched_at: null,
  delivered_at: null,
  read_at: null,
  kind: null,
};

// Build 250 outgoing messages in otherPeer's chat:
// - index 0 (250th from end, outside MENTION_SCAN_LIMIT = 200) mentions @net:old_mention.555555
// - index 248 (within newest 200 outgoing messages) mentions @net:zara.444444 at 2026-10-18T12:00:00Z
const otherChatMsgs = Array.from({ length: 250 }, (_, idx) => ({
  id: `bob-out-${idx}`,
  peer: otherPeer,
  outgoing: true,
  body:
    idx === 0
      ? 'Ancient mention @net:old_mention.555555'
      : idx === 248
        ? 'Recent outgoing mention @net:zara.444444'
        : `Filler message ${idx}`,
  created_at:
    idx === 0
      ? '2026-10-19T00:00:00Z'
      : idx === 248
        ? '2026-10-18T12:00:00Z'
        : '2026-10-15T00:00:00Z',
  state: 'sent',
  hub: null,
  attachments: [],
  error: null,
  seen: true,
  sent_at: null,
  received_at: null,
  fetched_at: null,
  delivered_at: null,
  read_at: null,
  kind: null,
}));
const bobLastMsg = {
  id: 'bob-last',
  peer: otherPeer,
  outgoing: false,
  body: 'Hello from Bob',
  created_at: '2026-10-16T10:00:00Z',
  state: 'received',
  hub: null,
  attachments: [],
  error: null,
  seen: true,
  sent_at: null,
  received_at: null,
  fetched_at: null,
  delivered_at: null,
  read_at: null,
  kind: null,
};

const openedAddrs = [];
const sentBodies = [];
const chatCalls = [];
Object.assign(api, {
  state: async () => ({
    me: { address: 'scratch.123456', id: 'scratch', tag: '123456', name: 'Me' },
    hubs: [{ url: 'wss://hub.invalid', name: 'Hub', state: 'connected', error: null, max_bytes: 1048576 }],
    platform: 'desktop',
  }),
  chats: async () => [
    { peer, last: msg, unread: 0 },
    { peer: otherPeer, last: bobLastMsg, unread: 0 },
  ],
  directory: async () => directory,
  transfers: async () => ({ rows: [], next: null }),
  onEvent: async () => () => {},
  draft: async () => '',
  setDraft: async () => {},
  composing: async () => {},
  setupStatus: async () => null,
  sendRoute: async () => ({ pinned: null, automatic: 'wss://hub.invalid', next: 'wss://hub.invalid', hubs: [] }),
  loadOlder: async () => ({ added: 0, more: [], unreachable: [] }),
  chat: async (p) => {
    chatCalls.push(p);
    if (p === peer) return [msg];
    if (p === otherPeer) return [...otherChatMsgs, bobLastMsg];
    return [];
  },
  prepareSend: async (m) => { sentBodies.push(m.body); },
  send: async (m) => { sentBodies.push(m.body); },
});

await act(async () => { await store.startStore(); });
const root = createRoot(document.getElementById('root'));
await act(async () => {
  root.render(createElement(Conversation, {
    peer,
    onBack: () => {},
    onInfo: () => {},
    onOpenAddr: (a) => { openedAddrs.push(a); },
    onContact: () => {},
  }));
});
await act(async () => { await new Promise((r) => setTimeout(r, 30)); });

const checks = [];
const check = (name, ok) => {
  ok = !!ok;
  checks.push({ name, ok });
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}`);
};

const setInputValue = async (el, val, caretPos = val.length) => {
  const setter = Object.getOwnPropertyDescriptor(dom.window.HTMLTextAreaElement.prototype, 'value')?.set;
  await act(async () => {
    setter.call(el, val);
    el.selectionStart = caretPos;
    el.selectionEnd = caretPos;
    el.dispatchEvent(new dom.window.Event('input', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 15));
  });
};

const pressKey = async (el, key, init = {}) => {
  const e = new dom.window.KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init });
  await act(async () => {
    el.dispatchEvent(e);
    await new Promise((r) => setTimeout(r, 15));
  });
  return e;
};

const ta = () => document.querySelector('textarea');

// 1. Control: ordinary text without @
await setInputValue(ta(), 'hello world');
check('control: ordinary text without @ does not open a mention list', document.querySelector('.mention-list') === null);

// 2. Typing @ opens the mention list with known contacts (excluding self and link-waiting)
await setInputValue(ta(), 'Hello @');
const initialItems = [...document.querySelectorAll('.mention-item')].map((x) => x.getAttribute('data-mention-address'));
check(
  'typing @ in the composer opens the contact mention hint list',
  initialItems.includes('alice.111111') &&
    initialItems.includes('acme.222222') &&
    initialItems.includes('pat.abcdef') &&
    !initialItems.includes('scratch.123456') &&
    !initialItems.includes('link.999999'),
);

// 3. Typing after @ filters the contact mention hint list
await setInputValue(ta(), 'Hello @al');
const filteredItems = [...document.querySelectorAll('.mention-item')].map((x) => x.getAttribute('data-mention-address'));
check('typing after @ filters the contact mention hint list', JSON.stringify(filteredItems) === JSON.stringify(['alice.111111']));

// 4. Tab completes the active contact mention as @net:<address> without sending
await pressKey(ta(), 'Tab');
check(
  'Tab completes the active contact mention as @net:<address> without sending',
  ta().value === 'Hello @net:alice.111111 ' && document.querySelector('.mention-list') === null && sentBodies.length === 0,
);

// 5. Clicking or tapping a mention hint completes @net:<address>
await setInputValue(ta(), 'Ping @ac');
const acmeBtn = document.querySelector('.mention-item[data-mention-address="acme.222222"]');
if (acmeBtn) {
  await act(async () => {
    acmeBtn.dispatchEvent(new dom.window.MouseEvent('mousedown', { bubbles: true, cancelable: true }));
    acmeBtn.click();
    await new Promise((r) => setTimeout(r, 15));
  });
}
check(
  'clicking or tapping a mention hint completes @net:<address>',
  ta().value === 'Ping @net:acme.222222 ' && document.querySelector('.mention-list') === null,
);

// 6. Escape dismisses the mention hint list before cancelling an active reply
await act(async () => {
  setReplies(peer, [msg.id]);
  await new Promise((r) => setTimeout(r, 15));
});
await setInputValue(ta(), 'Reply @al');
await pressKey(ta(), 'Escape');
check(
  'Escape dismisses the mention hint list before cancelling an active reply',
  document.querySelector('.mention-list') === null && document.querySelector('.replybar') !== null,
);

// 7. Control: @net:<address> link in message opens chat on click
const patLink = document.querySelector('.bubble a.addr[data-slug="pat.abcdef"]');
if (patLink) {
  await act(async () => { patLink.click(); await new Promise((r) => setTimeout(r, 10)); });
}
check('control: @net:<address> link in message opens chat on click', openedAddrs.includes('pat.abcdef'));

// 8. Control: unknown @word and email address stay plain text
const allSlugs = [...document.querySelectorAll('.bubble a.addr')].map((a) => a.getAttribute('data-slug'));
check('control: unknown @word and email address stay plain text', !allSlugs.includes('stranger'));

// 9. @org:<slug> and @<known-contact> mentions render as clickable a.addr.mention links that open the contact chat
const aliceLink = document.querySelector('.bubble a.addr.mention[data-slug="alice.111111"]');
const acmeLink = document.querySelector('.bubble a.addr.mention[data-slug="acme.222222"]');
if (aliceLink) {
  await act(async () => { aliceLink.click(); await new Promise((r) => setTimeout(r, 10)); });
}
if (acmeLink) {
  await act(async () => { acmeLink.click(); await new Promise((r) => setTimeout(r, 10)); });
}
check(
  '@org:<slug> and @<known-contact> mentions render as clickable a.addr.mention links that open the contact chat',
  !!aliceLink &&
    aliceLink.textContent === '@alice' &&
    !!acmeLink &&
    acmeLink.textContent === '@org:acme' &&
    openedAddrs.includes('alice.111111') &&
    openedAddrs.includes('acme.222222'),
);

// 10. Bottom-up ordering: Bucket 1 (mentioned in open chat, newest first, ignoring Markdown code) above Bucket 2
//     (max(lastChatted, lastMentionedByMe) desc, then lowercased name/kind/address asc), reversed so rank #1 is at the bottom row.
await setInputValue(ta(), 'Order @');
const domOrder = [...document.querySelectorAll('.mention-item')].map((x) => x.getAttribute('data-mention-address'));
const expectedDomOrder = [
  'quiz.888888',
  'old_mention.555555',
  'aa_quiz.777777',
  'bob.333333',
  'zara.444444',
  'pat.abcdef',
  'alice.111111',
  'acme.222222',
];
check(
  'mentions menu sorts bottom-up: Bucket 1 (mentioned in open chat, newest first, ignoring Markdown code) above Bucket 2 (max(lastChatted, lastMentionedByMe) then name/kind/address)',
  JSON.stringify(domOrder) === JSON.stringify(expectedDomOrder),
);

// 11. Initial highlight is on the bottom row (rank #1) and Tab completes that bottom row
const bottomItem = document.querySelectorAll('.mention-item')[domOrder.length - 1];
const bottomActive =
  !!bottomItem &&
  bottomItem.getAttribute('data-mention-address') === 'acme.222222' &&
  bottomItem.classList.contains('active') &&
  bottomItem.getAttribute('aria-selected') === 'true';
await pressKey(ta(), 'Tab');
check(
  'initial highlight is on the bottom row (rank #1) and Tab completes that bottom row',
  bottomActive && ta().value === 'Order @net:acme.222222 ',
);

// 12. ArrowUp moves selection upward away from the input and ArrowDown moves downward toward the input
await setInputValue(ta(), 'Keys @');
const activeAddr = () => document.querySelector('.mention-item.active')?.getAttribute('data-mention-address');
const step0 = activeAddr(); // acme.222222 (bottom row, index 7)
await pressKey(ta(), 'ArrowUp');
const stepUp1 = activeAddr(); // alice.111111 (index 6)
await pressKey(ta(), 'ArrowUp');
const stepUp2 = activeAddr(); // pat.abcdef (index 5)
await pressKey(ta(), 'ArrowDown');
const stepDown1 = activeAddr(); // alice.111111 (index 6)
await pressKey(ta(), 'ArrowDown');
const stepDown2 = activeAddr(); // acme.222222 (index 7)
check(
  'ArrowUp moves selection upward away from the input and ArrowDown moves downward toward the input',
  step0 === 'acme.222222' &&
    stepUp1 === 'alice.111111' &&
    stepUp2 === 'pat.abcdef' &&
    stepDown1 === 'alice.111111' &&
    stepDown2 === 'acme.222222',
);

// 13. Word-substring filtering narrows without reordering (exact/prefix score and online keys dropped)
const callsBeforeTyping = chatCalls.length;
await setInputValue(ta(), 'Filter @q');
await setInputValue(ta(), 'Filter @qu');
await setInputValue(ta(), 'Filter @qui');
await setInputValue(ta(), 'Filter @quiz');
const callsAfterTyping = chatCalls.length;
const quizOrder = [...document.querySelectorAll('.mention-item')].map((x) => x.getAttribute('data-mention-address'));
check(
  'word-substring filtering narrows without reordering (exact/prefix score and online keys dropped)',
  JSON.stringify(quizOrder) === JSON.stringify(['quiz.888888', 'aa_quiz.777777']) &&
    activeAddr() === 'aa_quiz.777777',
);

// 14. Account mention recency scans at most MENTION_SCAN_LIMIT = 200 outgoing messages per chat and never calls api.chat per keystroke
const zaraIdx = domOrder.indexOf('zara.444444');
const bobIdx = domOrder.indexOf('bob.333333');
const oldMentionIdx = domOrder.indexOf('old_mention.555555');
check(
  'account mention recency scans at most MENTION_SCAN_LIMIT = 200 outgoing messages per chat and never calls api.chat per keystroke',
  callsAfterTyping === callsBeforeTyping &&
    zaraIdx > bobIdx &&
    oldMentionIdx < bobIdx,
);

// Add 42 extra contacts so 50 total contacts match @ (more than 2 pages of MENU_STEP = 20 and taller than 224px max-height)
const extraContacts = Array.from({ length: 42 }, (_, idx) => ({
  address: `extra_${String(idx).padStart(2, '0')}.666666`,
  kind: 'person',
  org_name: '',
  username: `Extra ${String(idx).padStart(2, '0')}`,
  blurb: '',
  online: true,
  last_seen: null,
  hubs: ['wss://hub.invalid'],
}));
directory = [...directory, ...extraContacts];
await act(async () => {
  await store.refreshAll();
  await new Promise((r) => setTimeout(r, 20));
});

const isActiveInView = () => {
  const list = document.querySelector('.mention-list');
  const active = document.querySelector('.mention-item.active');
  if (!list || !active) return false;
  const top = active.offsetTop;
  const bottom = top + active.offsetHeight;
  return top >= list.scrollTop && bottom <= list.scrollTop + list.clientHeight;
};

// 15. Menu opens scrolled to the bottom and stays scrolled to the bottom when the filter changes or reopens
await setInputValue(ta(), 'Many @');
const listEl15 = document.querySelector('.mention-list');
const openAtBottom =
  !!listEl15 &&
  document.querySelectorAll('.mention-item').length === 20 &&
  listEl15.scrollTop === listEl15.scrollHeight - listEl15.clientHeight &&
  activeAddr() === 'acme.222222' &&
  isActiveInView();
for (let i = 0; i < 10; i++) await pressKey(ta(), 'ArrowUp');
const movedAwayFromBottom = !!listEl15 && listEl15.scrollTop < listEl15.scrollHeight - listEl15.clientHeight;
await setInputValue(ta(), 'Many @extra_');
const listAfterFilter = document.querySelector('.mention-list');
const filterAtBottom =
  !!listAfterFilter &&
  document.querySelectorAll('.mention-item').length === 20 &&
  listAfterFilter.scrollTop === listAfterFilter.scrollHeight - listAfterFilter.clientHeight &&
  activeAddr() === 'extra_00.666666' &&
  isActiveInView();
for (let i = 0; i < 10; i++) await pressKey(ta(), 'ArrowUp');
await pressKey(ta(), 'Escape');
await setInputValue(ta(), 'Many @extra');
await setInputValue(ta(), 'Many @extra_');
const listAfterReopen = document.querySelector('.mention-list');
const reopenAtBottom =
  !!listAfterReopen &&
  listAfterReopen.scrollTop === listAfterReopen.scrollHeight - listAfterReopen.clientHeight &&
  activeAddr() === 'extra_00.666666' &&
  isActiveInView();
check(
  'menu opens scrolled to the bottom and stays scrolled to the bottom when the filter changes',
  openAtBottom && movedAwayFromBottom && filterAtBottom && reopenAtBottom,
);

// 16. Arrow keys scroll the menu just enough to keep the highlighted row fully in view in both directions
await setInputValue(ta(), 'View @');
const listEl16 = document.querySelector('.mention-list');
let allUpInView = true;
for (let i = 0; i < 12; i++) {
  await pressKey(ta(), 'ArrowUp');
  if (!isActiveInView()) allUpInView = false;
}
const scrollAfterUp12 = listEl16 ? listEl16.scrollTop : -1;
const activeTopAfterUp12 = document.querySelector('.mention-item.active')?.offsetTop ?? -2;
let allDownInView = true;
for (let i = 0; i < 12; i++) {
  await pressKey(ta(), 'ArrowDown');
  if (!isActiveInView()) allDownInView = false;
}
const scrollAfterDown12 = listEl16 ? listEl16.scrollTop : -1;
const activeBottomAfterDown12 =
  (document.querySelector('.mention-item.active')?.offsetTop ?? 0) +
  (document.querySelector('.mention-item.active')?.offsetHeight ?? 0);
check(
  'arrow keys scroll the menu just enough to keep the highlighted row fully in view in both directions',
  allUpInView &&
    allDownInView &&
    scrollAfterUp12 === activeTopAfterUp12 &&
    !!listEl16 &&
    scrollAfterDown12 + listEl16.clientHeight === activeBottomAfterDown12,
);

// 17. Lazy loading upward on scroll: first page of 20 rows at the bottom, more loaded above on scroll with no jump in scroll position
await setInputValue(ta(), 'Lazy @');
const listEl17 = document.querySelector('.mention-list');
const firstPage17 = document.querySelectorAll('.mention-item').length;
const topItemBeforeScroll = document.querySelector('.mention-item');
const topAddrBeforeScroll = topItemBeforeScroll?.getAttribute('data-mention-address');
const offsetBeforeScroll = topItemBeforeScroll ? 20 - topItemBeforeScroll.offsetTop : -999;
if (listEl17) {
  await act(async () => {
    listEl17.scrollTop = 20;
    listEl17.dispatchEvent(new dom.window.Event('scroll', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));
  });
}
const secondPage17 = document.querySelectorAll('.mention-item').length;
const anchoredItemAfterScroll = topAddrBeforeScroll
  ? document.querySelector(`.mention-item[data-mention-address="${topAddrBeforeScroll}"]`)
  : null;
const offsetAfterScroll =
  listEl17 && anchoredItemAfterScroll ? listEl17.scrollTop - anchoredItemAfterScroll.offsetTop : -998;
check(
  'lazy loading upward on scroll: first page at bottom, more loaded above on scroll with no jump in scroll position',
  firstPage17 === 20 &&
    secondPage17 === 40 &&
    topAddrBeforeScroll === 'extra_13.666666' &&
    offsetBeforeScroll === offsetAfterScroll &&
    activeAddr() === 'acme.222222',
);

// 18. No wrapping: Down on the bottom row does nothing, Up on the top loaded row loads more above and moves onto it, Up at the very top does nothing
await setInputValue(ta(), 'Nowrap @');
const bottomAddrBeforeDown = activeAddr();
await pressKey(ta(), 'ArrowDown');
await pressKey(ta(), 'ArrowDown');
const bottomAddrAfterDown = activeAddr();
for (let i = 0; i < 19; i++) {
  await pressKey(ta(), 'ArrowUp');
}
const countAtTopPage1 = document.querySelectorAll('.mention-item').length;
const addrAtTopPage1 = activeAddr();
await pressKey(ta(), 'ArrowUp'); // on top loaded row (index 0 of 20) -> loads page 2 (40 items) and steps onto extra_14.666666
const countAfterStepPage2 = document.querySelectorAll('.mention-item').length;
const addrAfterStepPage2 = activeAddr();
const inViewAfterStepPage2 = isActiveInView();
for (let i = 0; i < 19; i++) {
  await pressKey(ta(), 'ArrowUp');
}
const countAtTopPage2 = document.querySelectorAll('.mention-item').length;
const addrAtTopPage2 = activeAddr();
await pressKey(ta(), 'ArrowUp'); // on top loaded row (index 0 of 40) -> loads page 3 (50 items) and steps onto extra_34.666666
const countAfterStepPage3 = document.querySelectorAll('.mention-item').length;
const addrAfterStepPage3 = activeAddr();
for (let i = 0; i < 9; i++) {
  await pressKey(ta(), 'ArrowUp');
}
const addrAtVeryTop = activeAddr();
await pressKey(ta(), 'ArrowUp');
await pressKey(ta(), 'ArrowUp');
const addrAfterUpAtVeryTop = activeAddr();
check(
  'no wrapping: Down on bottom row does nothing, Up on top loaded row loads more above and moves onto it, Up at very top does nothing',
  bottomAddrBeforeDown === 'acme.222222' &&
    bottomAddrAfterDown === 'acme.222222' &&
    countAtTopPage1 === 20 &&
    addrAtTopPage1 === 'extra_13.666666' &&
    countAfterStepPage2 === 40 &&
    addrAfterStepPage2 === 'extra_14.666666' &&
    inViewAfterStepPage2 &&
    countAtTopPage2 === 40 &&
    addrAtTopPage2 === 'extra_33.666666' &&
    countAfterStepPage3 === 50 &&
    addrAfterStepPage3 === 'extra_34.666666' &&
    addrAtVeryTop === 'quiz.888888' &&
    addrAfterUpAtVeryTop === 'quiz.888888',
);

await act(async () => { root.unmount(); });
const failed = checks.filter((c) => !c.ok).map((c) => c.name).sort();
const expected = [
  'menu opens scrolled to the bottom and stays scrolled to the bottom when the filter changes',
  'arrow keys scroll the menu just enough to keep the highlighted row fully in view in both directions',
  'lazy loading upward on scroll: first page at bottom, more loaded above on scroll with no jump in scroll position',
  'no wrapping: Down on bottom row does nothing, Up on top loaded row loads more above and moves onto it, Up at very top does nothing',
].sort();
fs.writeFileSync(process.env.HUBCHAT_RENDERER_EVIDENCE, JSON.stringify({ phase, checks, failed }, null, 2));
dom.window.close();
process.exit(
  phase === 'red'
    ? JSON.stringify(failed) === JSON.stringify(expected) ? 0 : 1
    : checks.every((c) => c.ok) ? 0 : 1,
);
