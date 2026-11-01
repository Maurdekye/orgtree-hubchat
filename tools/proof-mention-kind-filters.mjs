// Hubchat mention kind prefix filters (docket
// hubchat-mention-list-org-person-and-chat-prefix-f): typing @org:, @person:
// or @chat: in the composer filters the mention hint list to hub addresses of
// that kind; picking an entry still inserts its normal @net:<address>.
// Isolated DOM fixture proof. Run ONLY through verify-mention-kind-filters.ps1.
// Phase green: every check passes. Phase red (base d462929, Hubchat 1.3.0):
// exactly the kind-filter checks fail, so the controls really ran.
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

const baseRev = 'd46292941870d404acc183352aee4ab3710a49c6';
const baseFiles = phase === 'red' ? new Map([
  [path.resolve(repo, 'src/lib/mentions.ts').toLowerCase(), execFileSync('git', ['show', `${baseRev}:src/lib/mentions.ts`], { cwd: repo, encoding: 'utf8' })],
  [path.resolve(repo, 'src/components/Composer.tsx').toLowerCase(), execFileSync('git', ['show', `${baseRev}:src/components/Composer.tsx`], { cwd: repo, encoding: 'utf8' })],
]) : null;

await esbuild.build({
  entryPoints: [path.join(repo, 'tools/mentions-renderer-entry.tsx')],
  outfile: path.join(out, `entry-kind-${phase}.mjs`),
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

const { api, store, Conversation, act, createElement, createRoot } = await import(pathToFileURL(path.join(out, `entry-kind-${phase}.mjs`)));

const peer = 'pat.abcdef';
const directory = [
  { address: peer, kind: 'person', org_name: '', username: 'Pat', blurb: '', online: true, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'alice.111111', kind: 'person', org_name: '', username: 'Alice', blurb: '', online: true, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'macy.555555', kind: 'person', org_name: '', username: 'Macy', blurb: '', online: true, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'acme.222222', kind: 'org', org_name: 'acme', username: 'Acme Org', blurb: '', online: true, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'helper.333333', kind: 'chat', org_name: '', username: 'Helper Chat', blurb: '', online: true, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'oldhub.444444', kind: '', org_name: '', username: 'Old Hub', blurb: '', online: false, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'scratch.123456', kind: 'person', org_name: '', username: 'Me', blurb: '', online: true, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'link.999999', kind: 'person', org_name: '', username: '', blurb: 'Waiting to be linked to a Hubchat identity', online: false, last_seen: null, hubs: ['wss://hub.invalid'] },
];
const msg = {
  id: 'in-1',
  peer,
  outgoing: false,
  body: 'Hello there.',
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

const sentBodies = [];
Object.assign(api, {
  state: async () => ({
    me: { address: 'scratch.123456', id: 'scratch', tag: '123456', name: 'Me' },
    hubs: [{ url: 'wss://hub.invalid', name: 'Hub', state: 'connected', error: null, max_bytes: 1048576 }],
    platform: 'desktop',
  }),
  chats: async () => [{ peer, last: msg, unread: 0 }],
  directory: async () => directory,
  transfers: async () => ({ rows: [], next: null }),
  onEvent: async () => () => {},
  draft: async () => '',
  setDraft: async () => {},
  composing: async () => {},
  setupStatus: async () => null,
  sendRoute: async () => ({ pinned: null, automatic: 'wss://hub.invalid', next: 'wss://hub.invalid', hubs: [] }),
  loadOlder: async () => ({ added: 0, more: [], unreachable: [] }),
  chat: async (p) => (p === peer ? [msg] : []),
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
    onOpenAddr: () => {},
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
const items = () => [...document.querySelectorAll('.mention-item')].map((x) => x.getAttribute('data-mention-address')).sort();
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

// 1. Control: plain @ lists every kind (excluding self and link-waiting)
await setInputValue(ta(), 'Hello @');
check(
  'control: plain @ lists contacts of every kind',
  same(items(), ['acme.222222', 'alice.111111', 'helper.333333', 'macy.555555', 'oldhub.444444', 'pat.abcdef']),
);

// 2. @org: filters to org addresses only
await setInputValue(ta(), 'Hello @org:');
check('@org: filters the mention list to org addresses only', same(items(), ['acme.222222']));

// 3. @person: filters to person addresses only; an address with no kind
//    follows kindOf() and counts as a person, as it shows everywhere else
await setInputValue(ta(), 'Hello @person:');
check(
  '@person: filters the mention list to person addresses only (no kind counts as person, as kindOf shows it)',
  same(items(), ['alice.111111', 'macy.555555', 'oldhub.444444', 'pat.abcdef']),
);

// 4. @chat: filters to independent chat addresses only
await setInputValue(ta(), 'Hello @chat:');
check('@chat: filters the mention list to chat addresses only', same(items(), ['helper.333333']));

// 5. Text after the prefix narrows within that kind: "ac" also matches the
//    person Macy, who must stay out of the org list
await setInputValue(ta(), 'Hello @org:ac');
check('text after @org: narrows within org addresses only', same(items(), ['acme.222222']));

// 6. The prefix is case-insensitive, like @net:
await setInputValue(ta(), 'Hello @PERSON:al');
check('kind prefixes are case-insensitive (@PERSON:al)', same(items(), ['alice.111111']));

// 7. Picking from a kind-filtered list inserts the normal @net: address;
//    the filter word is never inserted or sent
await setInputValue(ta(), 'Ping @chat:');
await pressKey(ta(), 'Tab');
check(
  'picking from a kind-filtered list inserts @net:<address> and never the filter word',
  ta().value === 'Ping @net:helper.333333 ' && document.querySelector('.mention-list') === null && sentBodies.length === 0,
);

// 8. Control: @net: still filters by text across every kind
await setInputValue(ta(), 'Hello @net:al');
check('control: @net: still filters by text across every kind', same(items(), ['alice.111111']));

// 9. Control: an unknown prefix opens no mention list
await setInputValue(ta(), 'Hello @foo:bar');
check('control: an unknown prefix opens no mention list', document.querySelector('.mention-list') === null);

await act(async () => { root.unmount(); });
const failed = checks.filter((c) => !c.ok).map((c) => c.name).sort();
const expected = [
  '@chat: filters the mention list to chat addresses only',
  '@org: filters the mention list to org addresses only',
  '@person: filters the mention list to person addresses only (no kind counts as person, as kindOf shows it)',
  'kind prefixes are case-insensitive (@PERSON:al)',
  'picking from a kind-filtered list inserts @net:<address> and never the filter word',
  'text after @org: narrows within org addresses only',
].sort();
fs.writeFileSync(process.env.HUBCHAT_RENDERER_EVIDENCE, JSON.stringify({ phase, checks, failed }, null, 2));
dom.window.close();
process.exit(
  phase === 'red'
    ? JSON.stringify(failed) === JSON.stringify(expected) ? 0 : 1
    : checks.every((c) => c.ok) ? 0 : 1,
);
