// Question cards and multi-reply against the browser mock (docket
// hubchat-question-messages-carry-preset-answers-y): picking options, saved
// picks after a reload, answering and skipping in one send, first answer
// final, desktop Ctrl+click and Android long-press selection, stacked quotes.
// Run it only through tools/verify-questions.ps1, which starts it inside the
// Orgtree kernel Job runner: bundling, the server and Chrome end with the job.
for (const k of ['HUBCHAT_JOB_CONTAINED', 'HUBCHAT_VITE_CACHE', 'HUBCHAT_PROOF_OUT']) if (!process.env[k]) throw Error('Use tools/verify-questions.ps1; missing ' + k);
import { createServer } from 'vite';
import { launch } from './cdp.mjs';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const out = process.env.HUBCHAT_PROOF_OUT;
mkdirSync(out, { recursive: true });
const server = await createServer({ cacheDir: process.env.HUBCHAT_VITE_CACHE, server: { host: '127.0.0.1', port: 0 }, logLevel: 'error' });
await server.listen();
const base = server.resolvedUrls.local[0];
const b = await launch();
const checks = [];
let n = 0;
const check = (name, ok) => { n += 1; checks.push({ n, name, ok: !!ok }); console.log(`${ok ? 'PASS' : 'FAIL'} ${n}. ${name}`); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const ev = (js) => b.eval(js);
const theme = async (t) => { await ev(`localStorage.setItem('hubchat.theme', '${t}'), true`); };
const openChat = async (name) => {
  let ok = false;
  for (let i = 0; i < 30 && !ok; i++) {
    // the phone reopens the last chat after a restart: back to the list first
    ok = await ev(`(() => { const r = [...document.querySelectorAll('.crow')].find((e) => e.innerText.includes(${JSON.stringify(name)})); if (!r) { document.querySelector('.appbar [aria-label=Back]')?.click(); return false; } r.click(); return true; })()`);
    if (!ok) await sleep(150);
  }
  if (!ok) throw new Error('no chat row ' + name + ' in ' + JSON.stringify(await ev(`[...document.querySelectorAll('.crow')].map((e) => e.innerText.split(String.fromCharCode(10))[0])`)));
  await sleep(700);
};
const msgText = (sel) => ev(`document.querySelector(${JSON.stringify(sel)})?.innerText ?? null`);
const opts = (id) => ev(`[...document.querySelectorAll('#m-${id} .q-opt')].map((o) => ({ l: o.innerText.trim(), on: o.classList.contains('on'), role: o.getAttribute('role'), dis: o.disabled }))`);
const chips = () => ev(`[...document.querySelectorAll('.composer .anschip')].map((c) => c.innerText.replace(/\\s+/g, ' ').trim())`);
const pickOpt = async (id, label) => {
  const ok = await ev(`(() => { const o = [...document.querySelectorAll('#m-${id} .q-opt')].find((x) => x.innerText.trim() === ${JSON.stringify(label)}); if (!o) return false; o.click(); return true; })()`);
  if (!ok) throw new Error('no option ' + label);
  await sleep(250);
};
const find = (text) => `[...document.querySelectorAll('.timeline .msg')].find((e) => e.innerText.includes(${JSON.stringify(text)}))`;
const bubble = (text) => `${find(text)}?.querySelector('.bubble')`;
// press Send and wait until the new message is on screen
const sendNow = async () => {
  const k = await ev(`document.querySelectorAll('.timeline .msg.out').length`);
  await b.click('.composer .sendbtn', 100);
  for (let i = 0; i < 50 && (await ev(`document.querySelectorAll('.timeline .msg.out').length`)) <= k; i++) await sleep(100);
  await sleep(300);
};
// a trusted click at an element's centre, with modifier keys (2 = Ctrl)
const clickAt = async (js, modifiers = 0) => {
  const box = await ev(`(() => { const e = ${js}; if (!e) return null; e.scrollIntoView({ block: 'center' }); const r = e.getBoundingClientRect(); return { x: r.left + Math.min(40, r.width / 2), y: r.top + r.height / 2 }; })()`);
  if (!box) throw new Error('nothing to click');
  await b.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: box.x, y: box.y });
  await b.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: box.x, y: box.y, button: 'left', clickCount: 1, modifiers });
  await b.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: box.x, y: box.y, button: 'left', clickCount: 1, modifiers });
  await sleep(250);
};
const longPress = async (js) => {
  const box = await ev(`(() => { const e = ${js}; if (!e) return null; e.scrollIntoView({ block: 'center' }); const r = e.getBoundingClientRect(); return { x: r.left + Math.min(30, r.width / 2), y: r.top + r.height / 2 }; })()`);
  if (!box) throw new Error('nothing to press');
  await b.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: box.x, y: box.y, button: 'left', clickCount: 1 });
  await sleep(650);
  await b.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: box.x, y: box.y, button: 'left', clickCount: 1 });
  await sleep(300);
};

try {
  // ------------------------------------------------------------ desktop
  await b.nav(base); await theme('dark'); await ev(`localStorage.removeItem('hubchat-mock-draft-answers'), true`);
  await b.nav(base, 1200);
  await openChat('Orgtree');
  const ch = await opts('q-channel');
  check('single-choice card: 3 options as radio buttons, header hint "Reply to answer something else", note "Pick one"',
    ch.length === 3 && ch.every((o) => o.role === 'radio' && !o.dis) && (await msgText('#m-q-channel .q-hint')) === 'Reply to answer something else' && (await msgText('#m-q-channel .q-note')) === 'Pick one');
  const pl = await opts('q-platforms');
  check('multiple-choice card: 4 options as checkboxes, note "Pick one or more"',
    pl.length === 4 && pl.every((o) => o.role === 'checkbox') && (await msgText('#m-q-platforms .q-note')) === 'Pick one or more');
  check('free-response card: no options, header hint "Reply to answer"',
    (await opts('q-notes')).length === 0 && (await msgText('#m-q-notes .q-hint')) === 'Reply to answer');
  const ship = await opts('q-ship');
  check('settled card: header "Answered · <time>", its picked option marked, every option disabled, no hint',
    /^Answered · /.test(await msgText('#m-q-ship .q-state')) && ship.find((o) => o.l === 'Yes, ship it')?.on && ship.every((o) => o.dis) && !(await ev(`!!document.querySelector('#m-q-ship .q-hint')`)));
  check('free-response card answered by an earlier reply shows "Answered"',
    await ev(`/^Answered · /.test(${find('Keep the old log location')}?.querySelector('.q-state')?.innerText ?? '')`));
  check('the answering message quotes the card with "Answer: Yes, ship it"',
    await ev(`${find('Great. Ship it to the beta channel.')}?.querySelector('.q-ans')?.innerText === 'Answer: Yes, ship it'`));
  await b.shot(join(out, '01-cards-desktop-dark.png'));

  const before = await ev(`document.querySelectorAll('.timeline .msg').length`);
  await pickOpt('q-channel', 'Stable');
  let c = await chips();
  check('picking an option marks it and puts an answer chip in the composer; nothing is sent; the box says "(optional)"',
    (await opts('q-channel')).find((o) => o.l === 'Stable').on && c.length === 1 && c[0].includes('Stable') && (await ev(`document.querySelectorAll('.timeline .msg').length`)) === before
      && (await ev(`document.querySelector('.composer textarea').placeholder`)) === 'Add a message (optional)' && (await msgText('#m-q-channel .q-note')) === 'Your answer goes with your next message');
  await pickOpt('q-channel', 'Beta');
  c = await chips();
  check('single-choice: another option replaces the pick',
    (await opts('q-channel')).filter((o) => o.on).map((o) => o.l).join() === 'Beta' && c.length === 1 && c[0].includes('Beta') && !c[0].includes('Stable'));
  await pickOpt('q-platforms', 'Windows'); await pickOpt('q-platforms', 'Android');
  check('multiple-choice: picks add up (Windows, Android)', (await opts('q-platforms')).filter((o) => o.on).map((o) => o.l).join() === 'Windows,Android' && (await chips()).some((x) => x.includes('Windows, Android')));
  await pickOpt('q-platforms', 'Android');
  check('multiple-choice: tapping a picked option again removes it', (await opts('q-platforms')).filter((o) => o.on).map((o) => o.l).join() === 'Windows');

  // a reload stands in for restarting the app
  await b.nav(base, 1200); await openChat('Orgtree');
  c = await chips();
  check('picks survive a restart: Beta and Windows still marked, both chips back',
    (await opts('q-channel')).filter((o) => o.on).map((o) => o.l).join() === 'Beta' && (await opts('q-platforms')).filter((o) => o.on).map((o) => o.l).join() === 'Windows' && c.length === 2);

  await b.click('#m-q-notes .q-hint', 400);
  check('the header hint starts a reply to that card, like Reply, and focuses the box',
    (await msgText('.composer .replybar .rb-txt'))?.includes('Anything else for the rc4 release notes?') && (await ev(`document.activeElement === document.querySelector('.composer textarea')`)));
  check('with no text: Send is enabled and the composer warns that sending skips the question',
    !(await ev(`document.querySelector('.composer .sendbtn').disabled`)) && (await msgText('.composer .skipnote')) === 'Sending with no text skips the question you reply to');
  await b.shot(join(out, '02-composer-answers-desktop-dark.png'));
  await sendNow();
  const sent = `[...document.querySelectorAll('.timeline .msg.out')].pop()`;
  const notes = await ev(`[...${sent}.querySelectorAll('.quote')].map((q) => q.querySelector('.q-ans')?.innerText ?? '')`);
  check('one send answers two cards and skips the third: 3 quotes "Answer: Beta", "Answer: Windows", "Skipped"',
    notes.join('|') === 'Answer: Beta|Answer: Windows|Skipped');
  if (notes.join('|') !== 'Answer: Beta|Answer: Windows|Skipped') console.log('   got:', JSON.stringify(notes));
  check('the cards settle: two "Answered", one "Skipped", options locked, no hints',
    /^Answered/.test(await msgText('#m-q-channel .q-state')) && /^Answered/.test(await msgText('#m-q-platforms .q-state')) && /^Skipped/.test(await msgText('#m-q-notes .q-state'))
      && (await opts('q-channel')).every((o) => o.dis) && !(await ev(`!!document.querySelector('#m-q-channel .q-hint, #m-q-platforms .q-hint, #m-q-notes .q-hint')`)));
  check('the composer is empty again and nothing stays saved',
    (await chips()).length === 0 && !(await ev(`!!document.querySelector('.composer .replybar')`)) && (await ev(`localStorage.getItem('hubchat-mock-draft-answers')`)) === '{}');
  const answeredAt = await msgText('#m-q-channel .q-state');
  // first answer is final: a later reply is ordinary chat
  await clickAt(`document.querySelector('#m-q-channel .bubble')`);
  await ev(`(() => { const e = document.querySelector('#m-q-channel .bubble'); e.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 300, clientY: 300 })); return true; })()`);
  await sleep(200);
  check('the message menu offers Select', await ev(`[...document.querySelectorAll('.ctxmenu button, .ctx button, [role=menuitem]')].some((x) => x.innerText.trim() === 'Select')`));
  await b.key('Escape');
  await ev(`(() => { const e = document.querySelector('#m-q-channel .bubble'); e.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 300, clientY: 300 })); return true; })()`);
  await sleep(200);
  await ev(`[...document.querySelectorAll('.ctxmenu button, .ctx button, [role=menuitem]')].find((x) => x.innerText.trim() === 'Reply').click()`);
  await sleep(300);
  await b.type('.composer textarea', 'Actually, make it Stable.');
  await sendNow();
  check('first answer is final: a later reply to an answered card leaves it as it was and says "Not counted: answered earlier"',
    (await msgText('#m-q-channel .q-state')) === answeredAt && (await ev(`${sent}.querySelector('.q-ans')?.innerText`)) === 'Not counted: answered earlier');

  // desktop multi-select
  await clickAt(bubble('Done. Here is the change'), 2);
  await clickAt(bubble('Thanks! Can you also check the updater manifest?'), 2);
  check('Ctrl+click selects two messages; a bar above the composer says "2 selected"',
    (await ev(`document.querySelectorAll('.timeline .msg.sel').length`)) === 2 && (await msgText('.selbar .sb-n')) === '2 selected');
  await b.shot(join(out, '03-selection-desktop-dark.png'));
  await b.click('.selbar .btn', 300);
  check('the bar\'s Reply makes a multi-reply: "Replying to 2 messages", one row each, selection cleared',
    (await msgText('.composer .replybar.multi .rb-who')) === 'Replying to 2 messages' && (await ev(`document.querySelectorAll('.composer .rb-item').length`)) === 2 && !(await ev(`!!document.querySelector('.selbar')`)));
  check('an empty multi-reply to ordinary messages can\'t be sent', await ev(`document.querySelector('.composer .sendbtn').disabled`));
  await b.shot(join(out, '04-multireply-composer-desktop-dark.png'));
  await b.click('.composer .rb-item .icon-btn', 250);
  check('removing one row leaves a single reply in the usual bar', (await ev(`!document.querySelector('.composer .replybar.multi') && !!document.querySelector('.composer .replybar')`)));
  await b.key('Escape');
  check('Esc cancels the reply', !(await ev(`!!document.querySelector('.composer .replybar')`)));

  await openChat('Maya');
  check('a received multi-reply shows two quotes and "and 2 more"',
    await ev(`(() => { const m = ${find('All noted. See you at 14:00')}; return m && m.querySelectorAll('.quote').length === 2 && m.querySelector('.quote-more')?.innerText === 'and 2 more'; })()`));
  await ev(`${find('All noted. See you at 14:00')}.querySelector('.quote-more').click()`); await sleep(200);
  check('"and 2 more" opens all four quotes', await ev(`${find('All noted. See you at 14:00')}.querySelectorAll('.quote').length === 4`));
  await b.shot(join(out, '05-multi-quotes-desktop-dark.png'));
  await theme('light'); await b.nav(base, 1200); await openChat('Maya');
  await ev(`${find('All noted. See you at 14:00')}.scrollIntoView({ block: 'center' }), true`);
  await b.shot(join(out, '06-multi-quotes-desktop-light.png'));
  await openChat('Orgtree');
  await ev(`document.querySelector('#m-q-ship').scrollIntoView({ block: 'center' }), true`);
  await b.shot(join(out, '07-cards-desktop-light.png'));

  // ------------------------------------------------------------ android
  await ev(`localStorage.removeItem('hubchat-mock-draft-answers'), true`);
  await b.size(390, 844, 2);
  for (const t of ['dark', 'light']) {
    await theme(t); await b.nav(base + '?platform=android', 1300);
    await openChat('Orgtree');
    await ev(`document.querySelector('#m-q-channel').scrollIntoView({ block: 'center' }), true`); await sleep(200);
    check(`phone (${t}): the cards fit the screen width`, await ev(`[...document.querySelectorAll('.qcard')].every((e) => e.getBoundingClientRect().right <= 390 + 0.5)`));
    await b.shot(join(out, `08-cards-phone-${t}.png`));
    if (t === 'light') break;
    await pickOpt('q-channel', 'Hold it back');
    await longPress(find('Done. Here is the change'));
    check('phone: long-press selects a message ("1 selected", with Copy and Message info)',
      (await msgText('.appbar.select .title')) === '1 selected' && (await ev(`!!document.querySelector('.appbar.select [aria-label=Copy]')`)));
    await clickAt(find('Thanks! Can you also check the updater manifest?'));
    check('phone: a tap adds a second one ("2 selected"); Copy and Message info step aside',
      (await msgText('.appbar.select .title')) === '2 selected' && !(await ev(`!!document.querySelector('.appbar.select [aria-label=Copy]')`)));
    await b.click('.appbar.select [aria-label=Reply]', 300);
    check('phone: Reply makes a multi-reply beside the picked answer',
      (await msgText('.composer .replybar.multi .rb-who')) === 'Replying to 2 messages' && (await chips()).length === 1);
    await b.shot(join(out, '09-multireply-phone-dark.png'));
    check('phone: the composer rows fit the screen width', await ev(`[...document.querySelectorAll('.composer .rb-item, .composer .anschip')].every((e) => e.getBoundingClientRect().right <= 390 + 0.5)`));
  }
  // ----------------------------------------- settling order (the real module)
  // Two devices can answer one card at once; every device must settle it the
  // same way (docs/question-cards.md "Settling"), whatever order it lists them in.
  const { cardStates } = await server.ssrLoadModule('/src/lib/questions.ts');
  const msg = (o) => ({ peer: 'orgtree', hub: 'h', kind: null, reply_to: null, sent_at: null, received_at: null, created_at: '2026-10-12T10:00:00.000Z',
    state: 'sent', fetched_at: null, delivered_at: null, read_at: null, error: null, seen: true, attachments: [], outgoing: true, body: '', ...o });
  const card = msg({ id: 'q', outgoing: false, kind: 'question', body: 'Which channel?', question: { options: ['Beta', 'Stable'], multiple: false }, received_at: '2026-10-12T09:00:00.000Z' });
  const ans = (id, pick, o = {}) => msg({ id, reply_to: 'q', answers: pick ? [{ question: 'q', picked: [].concat(pick) }] : [], ...o });
  const settle = (list) => { const s = cardStates([card, ...list]).get('q'); return s.st === 'open' ? 'open' : `${s.st}:${s.by.id}:${s.picked ? s.picked.join('+') : ''}`; };
  const both = (list, want) => settle(list) === want && settle([...list].reverse()) === want;
  const T = '2026-10-12T10:00:00.000Z';
  check('settling: two answers the hub received at the same moment: the lower message id counts, in either list order',
    both([ans('b-2', 'Beta', { received_at: T }), ans('b-1', 'Stable', { received_at: T })], 'answered:b-1:Stable'));
  check('settling: the answer the hub received first counts, even when the other was written first',
    both([ans('a1', 'Beta', { created_at: '2026-10-12T10:00:00.000Z', received_at: '2026-10-12T10:10:00.000Z' }), ans('a2', 'Stable', { created_at: '2026-10-12T10:05:00.000Z', received_at: '2026-10-12T10:06:00.000Z' })], 'answered:a2:Stable'));
  check('settling: an answer not yet on a hub comes after every answer that is; unsent ones in writing order',
    both([ans('u1', 'Beta', { created_at: '2026-10-12T08:00:00.000Z', state: 'queued' }), ans('s1', 'Stable', { received_at: T })], 'answered:s1:Stable') &&
    both([ans('u2', 'Beta', { created_at: '2026-10-12T08:01:00.000Z', state: 'queued' }), ans('u1', 'Stable', { created_at: '2026-10-12T08:00:00.000Z', state: 'queued' })], 'answered:u1:Stable'));
  check('settling: an empty plain reply first skips the card for good; a later pick is not counted',
    both([ans('e1', null, { received_at: T }), ans('p1', 'Beta', { received_at: '2026-10-12T10:01:00.000Z' })], 'skipped:e1:'));
  check("settling: the card's own side can't answer it; a pick that isn't one of its options counts as a plain reply",
    settle([ans('own', 'Beta', { outgoing: false, received_at: T })]) === 'open' &&
    settle([ans('x1', 'Nope', { body: 'hello', received_at: T })]) === 'answered:x1:' &&
    settle([ans('x2', ['Beta', 'Stable'], { received_at: T })]) === 'skipped:x2:');
  check('no console errors or exceptions', b.errors.length === 0);
  if (b.errors.length) console.log(b.errors.join('\n'));
} catch (e) {
  check('proof ran to the end: ' + e.message, false);
} finally {
  writeFileSync(join(out, 'result.json'), JSON.stringify({ checks, errors: b.errors }, null, 2));
  console.log(`${checks.filter((c) => c.ok).length}/${checks.length} passed`);
  await b.close(); await server.close();
}
process.exit(checks.length && checks.every((c) => c.ok) ? 0 : 1);
