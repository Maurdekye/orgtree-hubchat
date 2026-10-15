// Source-only proof: mount the real shared message renderer in both platforms.
// Run inside Rig's shared lock and kernel Job. HUBCHAT_KIND_TAG_BASE substitutes
// the three changed components; no checkout or source mutation for the red.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

for (const key of ['HUBCHAT_DEPENDENCY_ROOT', 'HUBCHAT_DOM_DEPENDENCY_ROOT', 'HUBCHAT_RENDERER_OUT', 'HUBCHAT_JOB_CONTAINED']) {
  if (!process.env[key]) throw Error('Use the contained verifier; missing ' + key);
}
const repo = process.cwd();
const out = path.resolve(process.env.HUBCHAT_RENDERER_OUT);
if (!/^C:\/orgtree-build\//i.test(out.replaceAll('\\', '/'))) throw Error('Output must be in the assigned C build scope');
fs.mkdirSync(out, { recursive: true });
const orgdeps = createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT, 'package.json'));
const esbuild = orgdeps('esbuild');
const predecessor = process.env.HUBCHAT_KIND_TAG_BASE;
try {
  await esbuild.build({
    entryPoints: [path.join(repo, 'tools/question-tags-renderer-entry.tsx')],
    outfile: path.join(out, 'entry.mjs'), bundle: true, format: 'esm',
    platform: 'browser', jsx: 'automatic', loader: { '.md': 'text' },
    nodePaths: [path.join(process.env.HUBCHAT_DEPENDENCY_ROOT, 'node_modules')],
    plugins: [{ name: 'kind-tag-predecessor', setup(build) {
      build.onLoad({ filter: /[/\\]src[/\\]components[/\\](MessageView|Conversation|MessageInfo)\.tsx$/ }, (args) => {
        if (!predecessor) return;
        const file = path.relative(repo, args.path).replaceAll('\\', '/');
        const old = spawnSync('git', ['show', predecessor + ':' + file], { cwd: repo, encoding: 'utf8', windowsHide: true });
        if (old.status !== 0) throw Error(old.stderr || String(old.error));
        return { contents: old.stdout, loader: 'tsx', resolveDir: path.join(repo, 'src/components') };
      });
    } }],
  });
} finally { esbuild.stop(); }

const { JSDOM } = orgdeps('jsdom');
const dom = new JSDOM('<div id="root"></div>', { url: 'http://scratch.invalid' });
for (const key of ['window', 'document', 'navigator', 'HTMLElement', 'HTMLTextAreaElement', 'localStorage', 'location', 'getComputedStyle', 'Event', 'MouseEvent', 'KeyboardEvent', 'MutationObserver']) {
  Object.defineProperty(globalThis, key, { value: key === 'getComputedStyle' ? dom.window[key].bind(dom.window) : dom.window[key], configurable: true });
}
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
globalThis.requestAnimationFrame = f => setTimeout(f, 0);
globalThis.cancelAnimationFrame = clearTimeout;
const checks = [];
const check = (name, ok, target = false) => {
  try { assert.equal(!!ok, true, name); checks.push({ name, ok: true, target }); }
  catch { checks.push({ name, ok: false, target }); }
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}`);
};
const { MessageView, MessageInfoBody, Conversation, api, store, PlatformCtx, act, createElement: h, createRoot } = await import(pathToFileURL(path.join(out, 'entry.mjs')));
const root = createRoot(document.getElementById('root'));
const base = { peer: 'fixture.abcdef', hub: null, attachments: [], error: null, seen: true, sent_at: null, received_at: null, fetched_at: null, delivered_at: null, read_at: null, reply_to: null, created_at: '2026-10-15T00:00:00Z', state: 'received' };
const noop = () => {};
const kinds = ['message', 'status', 'request', 'decision', 'future-kind', null, 'question'];
let replyCalls = 0, pickCalls = 0, jumpCalls = 0;
const props = { first: true, peerName: 'Fixture', peerKind: 'person', quoted: null, onReply: () => replyCalls++, onPick: () => pickCalls++, onInfo: noop, onDelete: noop, onJump: () => jumpCalls++, onOpenAddr: noop };
const mount = async (platform, m, extra = {}) => {
  await act(async () => { root.render(h(PlatformCtx.Provider, { value: platform }, h('div', null,
    h(MessageView, { ...props, m, hover: platform === 'desktop', ...extra }), h(MessageInfoBody, { m })))); });
};
try {
  for (const platform of ['desktop', 'android']) {
    for (const outgoing of [false, true]) {
      for (const kind of kinds) {
        const label = `${platform}/${outgoing ? 'outgoing' : 'incoming'}/${kind ?? 'null'}`;
        const m = { ...base, id: label, outgoing, kind, body: 'Unchanged body', ...(kind === 'question' ? { question: { options: ['Red', 'Blue'], multiple: false } } : {}) };
        const before = JSON.stringify(m);
        await mount(platform, m, { card: { st: 'open' } });
        const chips = [...document.querySelectorAll('.kchip')];
        if (kind === 'question') check(label + ': Question tag remains', chips.length === 1 && chips[0].textContent === 'Question');
        else check(label + ': no kind tag', document.querySelector('.kchip') === null, ['status', 'request', 'decision'].includes(kind));
        check(label + ': accessible prefix', document.querySelector('.bubble > .sr')?.textContent === (outgoing ? 'You' : 'Fixture') + (kind === 'question' ? ', Question: ' : ': '), ['status', 'request', 'decision'].includes(kind));
        check(label + ': body and kind data preserved', document.querySelector('.mtext')?.textContent.includes(m.body) && JSON.stringify(m) === before);
        check(label + ': no redundant Kind info row', ![...document.querySelectorAll('.kv .k')].some(e => e.textContent === 'Kind'), !outgoing);
        if (kind === 'question') {
          const buttons = [...document.querySelectorAll('.q-opt')];
          check(label + ': question buttons remain', buttons.length === 2 && buttons.map(b => b.textContent.trim()).join('|') === 'Red|Blue' && buttons.every(b => b.disabled === outgoing));
        }
      }
    }
    const q = { ...base, id: 'interactive', outgoing: false, kind: 'question', body: 'Choose', question: { options: ['Red', 'Blue'], multiple: true } };
    await mount(platform, q, { card: { st: 'open' }, picks: ['Red'] });
    check(platform + ': picks remain selected', document.querySelector('.q-opt[aria-checked="true"]')?.textContent.trim() === 'Red');
    const pickedBefore = pickCalls, repliesBefore = replyCalls;
    await act(async () => { document.querySelectorAll('.q-opt')[1].click(); document.querySelector('.q-hint').click(); });
    check(platform + ': option and reply actions remain', pickCalls === pickedBefore + 1 && replyCalls === repliesBefore + 1);
    for (const st of ['answered', 'skipped']) {
      await mount(platform, q, { card: { st, by: { ...base, id: 'answer', outgoing: true, kind: null, body: st }, picked: ['Blue'] } });
      check(platform + ': ' + st + ' locks options', [...document.querySelectorAll('.q-opt')].every(b => b.disabled) && document.querySelector('.q-state')?.textContent.startsWith(st === 'answered' ? 'Answered' : 'Skipped'));
      const before = jumpCalls;
      await act(async () => { document.querySelector('.q-state').click(); });
      check(platform + ': ' + st + ' still jumps to answer', jumpCalls === before + 1);
    }
    await mount(platform, { ...q, question: undefined }, { card: { st: 'open' } });
    check(platform + ': free-response question keeps tag and Reply', document.querySelectorAll('.kchip').length === 1 && document.querySelectorAll('.q-opt').length === 0 && document.querySelector('.q-hint')?.textContent === 'Reply to answer');
  }
  // The real Conversation calculates the first-row spacing. All messages are
  // in one direction within one minute; only the question starts a new group.
  const rows = [null, ...kinds].map((kind, i) => ({ ...base, id: 'group-' + i, outgoing: false, kind, body: 'group body ' + i }));
  Object.assign(api, {
    state: async () => ({ me: { address: 'scratch.123456' }, hubs: [], platform: 'desktop' }),
    chats: async () => [{ peer: base.peer, last: rows.at(-1), unread: 0 }],
    directory: async () => [], transfers: async () => ({ rows: [], next: null }), onEvent: async () => noop,
    draft: async () => '', setDraft: async () => {}, draftAnswers: async () => null, setDraftAnswers: async () => {},
    setupStatus: async () => null, sendRoute: async () => ({ pinned: null, automatic: null, next: null, hubs: [] }),
    loadOlder: async () => ({ added: 0, more: [], unreachable: [] }), chat: async () => rows, cardAnswers: async () => rows,
    composing: async () => {},
  });
  await act(async () => { await store.startStore(); });
  for (const platform of ['desktop', 'android']) {
    await act(async () => { root.render(h(PlatformCtx.Provider, { value: platform }, h(Conversation, { key: platform, peer: base.peer, onInfo: noop, onOpenAddr: noop }))); });
    await act(async () => { await new Promise(r => setTimeout(r, 20)); });
    check(platform + ': grouping fixture loaded', document.querySelectorAll('.timeline .msg').length === rows.length);
    for (const m of rows.slice(1)) {
      const row = document.getElementById('m-' + m.id);
      check(platform + '/' + (m.kind ?? 'null') + ': only questions force a group', !!row && row.classList.contains('first') === (m.kind === 'question'), ['status', 'request', 'decision'].includes(m.kind));
    }
  }
  const css = predecessor
    ? spawnSync('git', ['show', predecessor + ':src/styles/base.css'], { cwd: repo, encoding: 'utf8', windowsHide: true })
    : { status: 0, stdout: fs.readFileSync(path.join(repo, 'src/styles/base.css'), 'utf8') };
  if (css.status !== 0) throw Error('Cannot read predecessor CSS');
  check('unused decision/status tag styles removed', !/\.kchip\.kc-[ds]\b/.test(css.stdout), true);
} finally {
  await act(async () => { root.unmount(); });
  dom.window.close();
}
const failed = checks.filter(c => !c.ok).map(c => c.name).sort();
const expected = predecessor ? checks.filter(c => c.target).map(c => c.name).sort() : [];
const passed = JSON.stringify(failed) === JSON.stringify(expected);
fs.writeFileSync(path.join(out, 'result.json'), JSON.stringify({ predecessor: predecessor ?? null, passed, checks, failed, expected }, null, 2));
console.log(JSON.stringify({ passed, checks: checks.length, failed: failed.length, expected: expected.length }));
process.exitCode = passed ? 0 : 1;
