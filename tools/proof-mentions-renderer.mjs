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

const baseFiles = phase === 'red' ? new Map([
  [path.resolve(repo, 'src/lib/md.ts').toLowerCase(), execFileSync('git', ['show', '2c72dce:src/lib/md.ts'], { cwd: repo, encoding: 'utf8' })],
  [path.resolve(repo, 'src/components/Composer.tsx').toLowerCase(), execFileSync('git', ['show', '2c72dce:src/components/Composer.tsx'], { cwd: repo, encoding: 'utf8' })],
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

const { api, store, Conversation, setReplies, act, createElement, createRoot } = await import(pathToFileURL(path.join(out, `entry-${phase}.mjs`)));

const peer = 'pat.abcdef';
const directory = [
  { address: peer, kind: 'person', org_name: '', username: 'Pat', blurb: '', online: true, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'alice.111111', kind: 'person', org_name: '', username: 'Alice', blurb: '', online: true, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'acme.222222', kind: 'org', org_name: 'acme', username: 'Acme Org', blurb: '', online: true, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'scratch.123456', kind: 'person', org_name: '', username: 'Me', blurb: '', online: true, last_seen: null, hubs: ['wss://hub.invalid'] },
  { address: 'link.999999', kind: 'person', org_name: '', username: '', blurb: 'Waiting to be linked to a Hubchat identity', online: false, last_seen: null, hubs: ['wss://hub.invalid'] },
];
const msg = {
  id: 'in-1',
  peer,
  outgoing: false,
  body: 'Ping @net:pat.abcdef or @alice or @org:acme, not @stranger or user@alice.111111.',
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

const openedAddrs = [];
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

await act(async () => { root.unmount(); });
const failed = checks.filter((c) => !c.ok).map((c) => c.name).sort();
const expected = [
  'typing @ in the composer opens the contact mention hint list',
  'typing after @ filters the contact mention hint list',
  'Tab completes the active contact mention as @net:<address> without sending',
  'clicking or tapping a mention hint completes @net:<address>',
  'Escape dismisses the mention hint list before cancelling an active reply',
  '@org:<slug> and @<known-contact> mentions render as clickable a.addr.mention links that open the contact chat',
].sort();
fs.writeFileSync(process.env.HUBCHAT_RENDERER_EVIDENCE, JSON.stringify({ phase, checks, failed }, null, 2));
dom.window.close();
process.exit(
  phase === 'red'
    ? JSON.stringify(failed) === JSON.stringify(expected) ? 0 : 1
    : checks.every((c) => c.ok) ? 0 : 1,
);
