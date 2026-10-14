// One contained batch: run once with HUBCHAT_PHONE_BASE=78474d3 (red), once
// without it (green). Existing dependencies read in place; no native sends.
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';
for (const k of ['HUBCHAT_DEPENDENCY_ROOT', 'HUBCHAT_DOM_DEPENDENCY_ROOT', 'HUBCHAT_PHONE_OUT']) if (!process.env[k]) throw Error('Missing ' + k);
if (process.env.HUBCHAT_JOB_CONTAINED !== '1') throw Error('Existing shared lock and kernel Job required');
const repo = process.env.HUBCHAT_PHONE_ROOT || process.cwd(), out = process.env.HUBCHAT_PHONE_OUT;
if (!/^c:\/orgtree-build\/(hubchat-dm-sol|rig-opus)\//i.test(path.resolve(out).replaceAll('\\', '/'))) throw Error('Owned C output required');
const deps = createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT, 'package.json'));
const orgDeps = createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT, 'package.json'));
const esbuild = orgDeps('esbuild'), base = process.env.HUBCHAT_PHONE_BASE;
fs.mkdirSync(out, { recursive: true });
await esbuild.build({ entryPoints: [path.join(repo, 'tools/answer-separation.fixture.tsx')], outfile: path.join(out, 'entry.mjs'),
  bundle: true, format: 'esm', platform: 'browser', jsx: 'automatic', plugins: [{ name: 'dependencies-and-red', setup(b) {
    b.onResolve({ filter: /^[^./][^:]*/ }, a => ({ path: deps.resolve(a.path === 'qrcode' ? 'qrcode/lib/browser.js' : a.path) }));
    if (base) b.onLoad({ filter: /[/\\](Composer\.tsx|store\.ts)$/ }, a => {
      const rel = path.relative(repo, a.path).replaceAll('\\', '/');
      if (!['src/components/Composer.tsx', 'src/lib/store.ts'].includes(rel)) return;
      const p = spawnSync('git', ['show', base + ':' + rel], { cwd: repo, encoding: 'utf8', windowsHide: true });
      if (p.status !== 0) throw Error(p.stderr || String(p.error));
      return { contents: p.stdout, loader: rel.endsWith('tsx') ? 'tsx' : 'ts', resolveDir: path.dirname(a.path) };
    });
  } }] }); await esbuild.stop();
const { JSDOM } = orgDeps('jsdom');
const dom = new JSDOM('<div id="root"></div>', { url: 'https://synthetic.invalid' });
for (const key of ['window','document','navigator','HTMLElement','HTMLTextAreaElement','localStorage','location','Event','MouseEvent','KeyboardEvent','MutationObserver'])
  Object.defineProperty(globalThis, key, { value: dom.window[key], configurable: true });
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
const { api, store, answers, Fixture, act, createElement, createRef, createRoot } = await import(pathToFileURL(path.join(out, 'entry.mjs')));
const drafts = new Map(), accepted = [], attempted = [], writes = [];
let sendMode = 'ok', release, draftHold, holdDraft = false;
Object.assign(api, {
  draft: async peer => drafts.get(peer) || '', setDraft: async (peer, body) => { writes.push({ peer, body }); if (holdDraft) await new Promise(r => { draftHold = r; }); drafts.set(peer, body); },
  draftAnswers: async () => null, setDraftAnswers: async () => {}, fileInfo: async source => ({ name: source + '.txt', bytes: 25 }),
  composing: async () => {}, filePreview: async () => null,
  state: async () => ({ me: { address: 'synthetic.123456' }, hubs: [], platform: 'desktop' }),
  chats: async () => [], directory: async () => [], transfers: async () => ({ rows: [], next: null }),
  send: async m => { attempted.push(structuredClone(m)); if (sendMode === 'hold') await new Promise(r => { release = r; }); if (sendMode === 'fail') throw Error('Synthetic admission failure'); accepted.push(structuredClone(m)); },
});
const checks = [], expectedRed = new Set();
const check = (name, ok, red = false, detail) => { if (red) expectedRed.add(name); checks.push({ name, ok: !!ok, detail }); console.log((ok ? 'PASS ' : 'FAIL ') + name); };
const tick = () => new Promise(r => setTimeout(r, 5));
const step = fn => act(async () => { await fn(); await tick(); });
let root, ref, current, cardState = () => 'open';
const card = (id, question = true) => ({ id, peer: current?.peer, outgoing: false, body: id, created_at: '2026-10-14T10:00:00Z', kind: question ? 'question' : null, question: question ? { options: ['One', 'Two', 'Three'], multiple: true } : undefined, attachments: [] });
const ta = () => document.querySelector('textarea');
const type = v => step(() => { Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(ta(), v); ta().dispatchEvent(new Event('input', { bubbles: true })); });
const clickSend = () => step(() => document.querySelector('.sendbtn').click());
const unmount = () => step(() => root.unmount());
async function mount(peer, platform, limit = 1000000) {
  current = { peer, platform, limit, cards: [card('q1'), card('q2'), card('q3'), card('ordinary', false)].map(m => ({ ...m, peer })) };
  ref = createRef(); root = createRoot(document.getElementById('root'));
  await step(() => root.render(createElement(Fixture, { ...current, composerRef: ref, cardState })));
}
try {
  for (const platform of ['desktop', 'android']) {
    const peer = 'separate-' + platform, prefix = platform + ': ';
    await mount(peer, platform);
    await type('  retained typed reply  '); await step(() => ref.current.addFiles(['fixture-file']));
    const beforePick = attempted.length;
    await step(() => { answers.setReplies(peer, ['q3']); answers.pick(peer, 'q1', 'Two', false); answers.pick(peer, 'q2', 'One', true); answers.pick(peer, 'q2', 'Three', true); });
    check(prefix + 'control picks accumulate without immediate send', attempted.length === beforePick && document.querySelectorAll('.anschip').length === 2 && ta().value === '  retained typed reply  ');
    const index = accepted.length;
    await clickSend(); const m = accepted[index];
    check(prefix + 'answers send with empty body and no files', m?.body === '' && m.attachments?.length === 0, true, m);
    check(prefix + 'only selected cards answered, cross-question/multi-choice preserved', m?.reply_to === 'q1' && m.replies?.join() === 'q1,q2' && JSON.stringify(m.answers?.map(a => a.picked)) === JSON.stringify([['Two'], ['One','Three']]), true);
    check(prefix + 'text and file retained after answer queues', ta().value === '  retained typed reply  ' && document.querySelectorAll('.attchip,.attthumb').length === 1, true);
    check(prefix + 'native draft never cleared by answer send', drafts.get(peer) === '  retained typed reply  ', true, writes.filter(w => w.peer === peer));
    check(prefix + 'plain reply retained, sent picks cleared', document.querySelectorAll('.anschip').length === 0 && document.querySelector('.replybar')?.textContent.includes('q3'), true);
    // Seed the same retained draft if testing the predecessor lost it: the
    // normal-send controls still execute, rather than collapsing into failures.
    if (!ta().value) { await type('  retained typed reply  '); await step(() => { ref.current.addFiles(['fixture-file']); answers.setReplies(peer, ['q3']); }); }
    await clickSend(); const normal = accepted[index + 1];
    check(prefix + 'control later text/files send separately and clear normally', normal?.body === 'retained typed reply' && normal.attachments?.length === 1 && !normal.answers?.length && normal.reply_to === 'q3' && ta().value === '' && !document.querySelector('.attchip,.attthumb'));
    await unmount();

    await mount('failure-' + platform, platform); await type('retry draft'); await step(() => ref.current.addFiles(['retry-file']));
    await step(() => answers.pick(current.peer, 'q1', 'One', false)); sendMode = 'fail'; const n = accepted.length;
    await clickSend();
    check(prefix + 'failed answer admission preserves draft/files/picks and error', accepted.length === n && ta().value === 'retry draft' && !!document.querySelector('.attchip,.attthumb') && !!document.querySelector('.anschip') && !!document.querySelector('[role=alert]'));
    const failedPeer = current.peer; await unmount(); await mount(failedPeer, platform);
    check(prefix + 'control failed admission remount restores draft/files', ta().value === 'retry draft' && !!document.querySelector('.attchip,.attthumb'));
    sendMode = 'ok'; await clickSend();
    check(prefix + 'retry queues answers alone and keeps draft', accepted.at(-1)?.body === '' && accepted.at(-1)?.attachments?.length === 0 && ta().value === 'retry draft', true);
    await unmount();

    cardState = id => id === 'q3' ? 'unknown' : 'open'; await mount('unknown-' + platform, platform);
    await step(() => { answers.setReplies(current.peer, ['q3']); answers.pick(current.peer, 'q1', 'One', false); });
    check(prefix + 'known answer is not blocked by an unrelated unknown reply', !document.querySelector('.sendbtn').disabled, true);
    const unknownIndex = accepted.length;
    await clickSend();
    check(prefix + 'unknown plain card is not skipped by answer send', accepted[unknownIndex]?.reply_to === 'q1' && !accepted[unknownIndex]?.replies?.includes('q3') && !!document.querySelector('.replybar'), true);
    // In red, the predecessor blocked the answer. Clear that pick so the
    // normal empty-reply guard can still be checked independently.
    await step(() => answers.unpick(current.peer, ['q1']));
    check(prefix + 'control empty unknown reply still waits after picks clear', document.querySelector('.sendbtn').disabled);
    cardState = () => 'open'; await step(() => root.render(createElement(Fixture, { ...current, composerRef: ref, cardState })));
    await clickSend(); check(prefix + 'control known empty reply still skips normally', accepted.at(-1)?.reply_to === 'q3' && accepted.at(-1)?.body === '' && !accepted.at(-1)?.answers?.length);
    await unmount();
  }

  await mount('oversize', 'android', 10); await type('a retained draft far over the hub limit'); await step(() => ref.current.addFiles(['big-file'])); await step(() => answers.pick(current.peer, 'q1', 'One', false));
  check('oversize retained draft/files do not block the separate answer', !document.querySelector('.sendbtn').disabled, true);
  await clickSend(); check('control retained oversized normal draft still has its limit', document.querySelector('.sendbtn').disabled && ta().value.includes('over the hub limit')); await unmount();

  await mount('delayed', 'android'); await type('held draft'); await step(() => ref.current.addFiles(['held-file'])); await step(() => answers.pick(current.peer, 'q1', 'One', false));
  sendMode = 'hold'; await clickSend(); check('control preparing prevents duplicate sends', ta().disabled && document.querySelector('.sendbtn').disabled);
  await step(() => { answers.setReplies(current.peer, ['q3']); answers.pick(current.peer, 'q2', 'Two', false); });
  const heldPeer = current.peer; await unmount(); await mount(heldPeer, 'android');
  check('control preparing send remount retains the draft/files', ta().value === 'held draft' && !!document.querySelector('.attchip,.attthumb'));
  sendMode = 'ok'; await step(() => release());
  check('late queue clears only its own picked card, retaining newer pick/reply', document.querySelectorAll('.anschip').length === 1 && document.querySelector('.anschip')?.textContent.includes('q2') && document.querySelector('.replybar')?.textContent.includes('q3'));
  check('late queue retains text/files after remount', ta().value === 'held draft' && !!document.querySelector('.attchip,.attthumb'), true); await unmount();

  // Direct controlled barriers on the actual preparation pipeline verify
  // both admission and completion under the existing identity generation.
  holdDraft = true; const before = attempted.length;
  const pending = store.prepareSend({ id: 'old-draft', peer: 'old', body: '', answers: [{ question: 'q1', picked: ['One'] }], reply_to: 'q1' }, { body: 'keep', attachments: [] });
  await store.reloadAfterSwitch(); holdDraft = false; draftHold(); await pending;
  check('control identity change during draft write prevents send admission', attempted.length === before && store.getPreparation('old') === null);
  sendMode = 'hold';
  const pendingSend = store.prepareSend({ id: 'old-send', peer: 'old', body: '', reply_to: 'q1', answers: [{ question: 'q1', picked: ['One'] }] }, { body: 'keep', attachments: [] });
  await tick(); await store.reloadAfterSwitch(); sendMode = 'ok'; release(); await pendingSend;
  check('control identity change during admission prevents stale queued completion', store.getPreparation('old') === null);
} catch (e) { check('proof completed', false, false, String(e.stack || e)); }
finally { try { await unmount(); } catch {} dom.window.close(); }
const failures = checks.filter(c => !c.ok).map(c => c.name);
const passed = base ? expectedRed.size > 0 && failures.length === expectedRed.size && failures.every(n => expectedRed.has(n)) : checks.length >= 35 && failures.length === 0;
fs.writeFileSync(path.join(out, 'result.json'), JSON.stringify({ base: base || null, passed, checks, expectedRed: [...expectedRed], failures, scope: 'actual Composer/preparation/saved-answer modules, desktop and Android DOM, controlled native seam; no live send/device' }, null, 2));
console.log(JSON.stringify({ passed, checks: checks.length, failures: failures.length, expectedRed: expectedRed.size }));
process.exit(passed ? 0 : 1);
