// One contained batch: run once with HUBCHAT_PHONE_BASE=74d209f (red), once
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
    const attemptedStart=attempted.length, acceptedStart=accepted.length;
    const p = platform + ': '; await mount('mixed-' + platform, platform);
    await type('  One text for two cards  '); await step(() => ref.current.addFiles(['file']));
    await step(() => { answers.addReplies(current.peer, ['q1','q3']); answers.pick(current.peer,'q2','One',true); answers.pick(current.peer,'q2','Three',true); });
    check(p+'control picks wait for Send', attempted.length===attemptedStart && accepted.length===acceptedStart && document.querySelectorAll('.anschip').length===1);
    const n=accepted.length; await clickSend(); const m=accepted[n];
    check(p+'mixed send carries typed text and file', m?.body==='One text for two cards' && m.attachments?.length===1,true,m);
    check(p+'mixed send carries all three references and only Q2 picks', m?.reply_to==='q1' && m.replies?.join()==='q1,q2,q3' && m.answers?.length===1 && m.answers[0].question==='q2' && m.answers[0].picked.join()==='One,Three',true);
    check(p+'mixed queue clears sent text/files/targets', ta().value==='' && !document.querySelector('.attchip,.attthumb,.replybar,.anschip'),true);
    check(p+'mixed native draft cleared after admission', drafts.get(current.peer)==='',true);
    await unmount();

    await mount('empty-'+platform,platform);
    await step(()=>{ answers.addReplies(current.peer,['q1','q3']); answers.pick(current.peer,'q2','Two',false); });
    check(p+'empty mixed send visibly warns it skips two text targets', /skips 2 questions/.test(document.querySelector('.skipnote')?.textContent||''),true);
    const e=accepted.length; await clickSend(); const empty=accepted[e];
    check(p+'empty mixed sends both skip references and separate picked card', empty?.body==='' && empty.replies?.join()==='q1,q2,q3' && empty.answers?.length===1 && empty.answers[0].question==='q2',true,empty);
    await unmount();

    await mount('conflict-'+platform,platform);await type('Reply text');
    await step(()=>{ answers.addReplies(current.peer,['q1','q3']); answers.pick(current.peer,'q1','One',false); });
    check(p+'control pick moves Q1 out of text targets', !document.querySelector('.replybar')?.textContent.includes('q1') && !!document.querySelector('.anschip')?.textContent.includes('q1'));
    await step(()=>answers.addReplies(current.peer,['q1']));
    check(p+'control Reply moves Q1 back and removes its pick', !document.querySelector('.anschip') && !!document.querySelector('.replybar')?.textContent.includes('q1'));
    await unmount();

    await mount('failure-'+platform,platform);await type('retry draft');await step(()=>ref.current.addFiles(['retry-file']));
    await step(()=>{answers.addReplies(current.peer,['q3']);answers.pick(current.peer,'q1','One',false)});sendMode='fail';const f=accepted.length;await clickSend();
    check(p+'control failed admission preserves draft/files/targets/picks',accepted.length===f && ta().value==='retry draft' && !!document.querySelector('.attchip,.attthumb') && !!document.querySelector('.anschip') && !!document.querySelector('.replybar') && !!document.querySelector('[role=alert]'));
    const peer=current.peer;await unmount();await mount(peer,platform);
    check(p+'control failed admission remount restores complete draft',ta().value==='retry draft' && !!document.querySelector('.attchip,.attthumb,.replybar'));
    sendMode='ok';await clickSend();
    check(p+'retry queues the complete mixed payload',accepted.at(-1)?.body==='retry draft' && accepted.at(-1)?.attachments?.length===1 && accepted.at(-1)?.replies?.join()==='q1,q3' && ta().value==='',true);
    await unmount();

    cardState=id=>id==='q3'?'unknown':'open';await mount('unknown-'+platform,platform);
    await step(()=>{answers.addReplies(current.peer,['q3']);answers.pick(current.peer,'q1','One',false)});
    const u=attempted.length;
    check(p+'blank mixed waits for unknown card before skipping',document.querySelector('.sendbtn').disabled,true);
    await clickSend();check(p+'blank unknown does not admit a partial picked send',attempted.length===u,true);
    // Independently reset any base admission so the known-card control is real.
    await step(()=>{answers.unpick(current.peer,['q1']);answers.setReplies(current.peer,['q3']);answers.pick(current.peer,'q1','One',false)});
    cardState=()=> 'open';await step(()=>root.render(createElement(Fixture,{...current,composerRef:ref,cardState})));
    const known=accepted.length;await clickSend();const knownReply=accepted[known];check(p+'known mixed reply includes intended Skip and pick',accepted.length===known+1 && knownReply?.body==='' && knownReply.replies?.join()==='q1,q3' && knownReply.answers?.length===1 && knownReply.answers[0].question==='q1' && knownReply.answers[0].picked.join()==='One',true);
    await unmount();
    await mount('ordinary-'+platform,platform);await type('Ordinary text');await step(()=>answers.addReplies(current.peer,['ordinary']));await clickSend();
    check(p+'control ordinary single reply unchanged',accepted.at(-1)?.body==='Ordinary text' && accepted.at(-1)?.reply_to==='ordinary' && !accepted.at(-1)?.answers?.length);await unmount();
  }
  await mount('oversize','android',10);await type('draft larger than hub limit');await step(()=>answers.pick(current.peer,'q1','One',false));
  const big=attempted.length;check('oversize combined text blocks the whole send',document.querySelector('.sendbtn').disabled,true);await clickSend();check('oversize mixed never queues partial picks',attempted.length===big,true);await unmount();
  await mount('delayed','android');await type('held draft');await step(()=>ref.current.addFiles(['held-file']));await step(()=>answers.pick(current.peer,'q1','One',false));
  sendMode='hold';await clickSend();check('control preparing prevents duplicate sends',ta().disabled && document.querySelector('.sendbtn').disabled);
  await step(()=>{answers.addReplies(current.peer,['q3']);answers.pick(current.peer,'q2','Two',false)});
  const held=current.peer;await unmount();await mount(held,'android');check('control preparing remount retains complete draft',ta().value==='held draft' && !!document.querySelector('.attchip,.attthumb'));
  sendMode='ok';await step(()=>release());
  check('control late completion keeps targets and picks chosen meanwhile',document.querySelectorAll('.anschip').length===1 && !!document.querySelector('.replybar')?.textContent.includes('q3'));
  check('late mixed completion clears only its sent text and files',ta().value==='' && !document.querySelector('.attchip,.attthumb'),true);await unmount();
  holdDraft=true;const before=attempted.length;
  const pending=store.prepareSend({id:'old-draft',peer:'old',body:'text',answers:[{question:'q1',picked:['One']}],reply_to:'q1'});
  await store.reloadAfterSwitch();holdDraft=false;draftHold();await pending;
  check('control identity change during draft write prevents send',attempted.length===before && store.getPreparation('old')===null);
  sendMode='hold';const pendingSend=store.prepareSend({id:'old-send',peer:'old',body:'text',reply_to:'q1',answers:[{question:'q1',picked:['One']}]});
  await tick();await store.reloadAfterSwitch();sendMode='ok';release();await pendingSend;
  check('control identity change during admission prevents stale completion',store.getPreparation('old')===null);
} catch(e) {check('proof completed',false,false,String(e.stack||e))}
finally {try{await unmount()}catch{}dom.window.close()}
const failures=checks.filter(c=>!c.ok).map(c=>c.name);
const passed=base ? expectedRed.size>0 && failures.length===expectedRed.size && failures.every(n=>expectedRed.has(n)) : checks.length>=35 && failures.length===0;
fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({base:base||null,passed,checks,expectedRed:[...expectedRed],failures,scope:'actual Composer/preparation/saved-answer modules, synthetic native seam; red overrides only Composer/store; no real sends/device'},null,2));
console.log(JSON.stringify({passed,checks:checks.length,failures:failures.length,expectedRed:expectedRed.size}));process.exit(passed?0:1);
