// Question cards and multi-reply against the browser mock (docket
// hubchat-question-messages-carry-preset-answers-y): picking options, saved
// picks after a reload, answering and skipping in one send, first answer
// final, desktop Ctrl+click and Android long-press selection, stacked quotes;
// review R1: cards and answers beyond the first page after a restart, and the
// real answers.ts and questions.ts modules (saved answers across an identity
// switch, settlement from the store); review R2: an empty send to a card the
// store hasn't answered for yet (its query held back, then failing) waits,
// mixed with picks, skips and files; review R3: a reply or pick chosen in the
// chat while a message's file is prepared stays for the next message. Each
// R1/R2/R3 part records a failure and goes on, so the same proof shows what
// the reviewed code did.
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

  // ---------------------------------- review R1: cards beyond the first page
  // A restart with answers saved earlier on cards further back than the
  // first page (?pages=1: nightly-ci's chat, 60 build checks after them).
  // Another device answered one of them meanwhile: its saved pick must not
  // come back as an answer. The two nobody answered keep theirs.
  try {
    const CI = 'nightly-ci.alex.8d21e0';
    await ev(`localStorage.setItem('hubchat-mock-draft-answers', JSON.stringify({ '${CI}': { replies: ['q-free'], picks: { 'q-old': ['Restart now'], 'q-fresh': ['Keep'] } } })), true`);
    await b.nav(base + '?pages=1', 1200);
    await openChat('nightly-ci');
    await sleep(500);
    const saved = () => ev(`JSON.parse(localStorage.getItem('hubchat-mock-draft-answers') || '{}')[${JSON.stringify(CI)}] ?? null`);
    check('beyond the page: the three cards and the other device\'s answer are not on screen (the newest 50 messages are build checks)',
      !(await ev(`!!document.querySelector('#m-q-old, #m-a-old, #m-q-fresh, #m-q-free')`)) && (await ev(`document.querySelectorAll('.timeline .msg').length`)) === 50);
    const c1 = await chips(), s1 = await saved();
    const dropped = c1.length === 1 && c1[0].includes('Keep') && !!s1 && !('q-old' in s1.picks) && s1.picks['q-fresh']?.join() === 'Keep';
    check('beyond the page: after a restart the saved pick on the card another device answered is gone (chip and saved); the open card keeps "Keep"', dropped);
    if (!dropped) console.log('   chips:', JSON.stringify(c1), 'saved:', JSON.stringify(s1));
    check('beyond the page: the saved reply to the open free-response card is back, and sending it empty would skip it',
      (await msgText('.composer .replybar .rb-txt'))?.includes('Anything to add to the runbook?') && (await msgText('.composer .skipnote')) === 'Sending with no text skips the question you reply to');
    await b.shot(join(out, '11-beyond-page-desktop-light.png'));
    await sendNow();
    const got = await ev(`[...${sent}.querySelectorAll('.quote')].map((q) => q.querySelector('.q-ans')?.innerText ?? '')`);
    check('beyond the page: one empty send answers the open card and skips the free one, each quote noted ("Answer: Keep", "Skipped"); the answered card is not in it',
      got.join('|') === 'Answer: Keep|Skipped');
    if (got.join('|') !== 'Answer: Keep|Skipped') console.log('   got:', JSON.stringify(got));
    check('beyond the page: nothing stays saved for the chat', (await saved()) === null);
    // older pages until the cards show: they read as the store said
    for (let i = 0; i < 12 && !(await ev(`!!document.querySelector('#m-q-old')`)); i++) {
      await ev(`document.querySelector('.timeline').scrollTop = 0, true`); await sleep(700);
    }
    check('beyond the page: scrolled back, the cards read the same: "Answered" (the other device\'s pick), "Answered" (Keep), "Skipped"',
      /^Answered/.test(await msgText('#m-q-old .q-state')) && (await opts('q-old')).find((o) => o.l === 'Wait for the window')?.on
        && /^Answered/.test(await msgText('#m-q-fresh .q-state')) && /^Skipped/.test(await msgText('#m-q-free .q-state')));
    await ev(`document.querySelector('#m-q-old').scrollIntoView({ block: 'center' }), true`);
    await b.shot(join(out, '12-beyond-page-scrolled-desktop-light.png'));
  } catch (e) {
    check('beyond-the-page scenario ran to the end: ' + e.message, false);
  }

  // ------------- review R2: an empty send to a card the store hasn't answered for
  // window.__hcCards (the mock) holds the store's answer back or makes it
  // fail, and adds a card as if it had just come in. While a card's state is
  // unknown, an empty message replying to it must wait, whatever else it
  // answers, and keep everything; once the store answers it goes.
  try {
    const CI = 'nightly-ci.alex.8d21e0';
    await ev(`localStorage.removeItem('hubchat-mock-draft-answers'), true`);
    await b.nav(base + '?pages=1', 1200);
    await openChat('nightly-ci');
    const cards = (js) => ev(`(window.__hcCards.${js}, true)`);
    const saved = () => ev(`JSON.parse(localStorage.getItem('hubchat-mock-draft-answers') || '{}')[${JSON.stringify(CI)}] ?? null`);
    const outs = () => ev(`document.querySelectorAll('.timeline .msg.out').length`);
    const waits = () => ev(`document.querySelector('.composer .sendbtn').disabled`);
    const note = () => msgText('.composer .skipnote');
    const CHECKING = 'Checking whether the question you reply to is still open';
    const SKIPS = 'Sending with no text skips the question you reply to';
    const notes = () => ev(`[...${sent}.querySelectorAll('.quote')].map((q) => q.querySelector('.q-ans')?.innerText ?? '')`);
    const newCard = async (id, body, options) => { await cards(`card(${JSON.stringify(CI)}, '${id}', ${JSON.stringify(body)}, ${JSON.stringify(options)})`); await sleep(700); };

    // a pick on a known card, a reply to a card not known yet (query held)
    await newCard('q-k1', 'Rotate the signing key tonight?', ['Rotate', 'Later']);
    await cards('hold()');
    await newCard('q-u1', 'Anything to add to the runbook today?', []);
    await pickOpt('q-k1', 'Rotate');
    await b.click('#m-q-u1 .q-hint', 300);
    let n0 = await outs();
    check('R2, query held: a pick on a known card plus an empty reply to one not known yet: Send waits and says it is checking; pick and reply stay',
      (await waits()) && (await note()) === CHECKING && (await chips()).length === 1 && (await msgText('.composer .replybar .rb-txt'))?.includes('Anything to add to the runbook today?'));
    await b.shot(join(out, '13-unknown-card-waits-desktop-light.png'));
    await ev(`document.querySelector('.composer textarea').focus(), true`); await b.key('Enter'); await sleep(500);
    check('R2, query held: Enter with no text sends nothing', (await outs()) === n0);
    await cards('release()'); await sleep(700);
    check('R2: once the store answers, Send is open and says the empty reply skips the now known open card',
      !(await waits()) && (await note()) === SKIPS && (await chips()).length === 1);
    await sendNow();
    check('R2: then one empty send answers the first card ("Answer: Rotate") and skips the second ("Skipped")', (await notes()).join('|') === 'Answer: Rotate|Skipped');

    // an empty reply to a known open card and to one not known (query failing)
    await newCard('q-k2', 'Keep the canary hub running?', ['Keep it', 'Stop it']);
    await cards('fail()');
    await newCard('q-u2', 'Move the nightly to 03:00?', ['Yes', 'No']);
    await clickAt(`document.querySelector('#m-q-k2 .kchip')`, 2);
    await clickAt(`document.querySelector('#m-q-u2 .kchip')`, 2);
    await b.click('.selbar .btn', 300);
    n0 = await outs();
    check('R2, query failing: an empty reply to a known open card and to one not known: Send waits; both replies stay',
      (await waits()) && (await note()) === CHECKING && (await ev(`document.querySelectorAll('.composer .rb-item').length`)) === 2);
    await ev(`document.querySelector('.composer textarea').focus(), true`); await b.key('Enter'); await sleep(500);
    check('R2, query failing: Enter with no text sends nothing', (await outs()) === n0);
    await b.type('.composer textarea', 'Both can wait until Friday.');
    check('R2, query failing: with text the reply can go at once (positive control)', !(await waits()));
    await sendNow();
    check('R2, query failing: the reply with text answers both cards', (await outs()) === n0 + 1 && (await ev(`${sent}.querySelectorAll('.quote').length`)) === 2);
    await cards('release()'); await sleep(700);

    // files and no text: the same wait (query held)
    await newCard('q-k3', 'Archive last week\'s logs?', ['Archive', 'Keep']);
    await cards('hold()');
    await newCard('q-u3', 'Anything for the on-call notes?', []);
    await pickOpt('q-k3', 'Archive');
    await b.click('#m-q-u3 .q-hint', 300);
    await b.click('.composer [aria-label="Attach files"]', 500);
    check('R2, query held: a file but no text, a pick, and a reply to a card not known yet: Send waits',
      (await waits()) && (await note()) === CHECKING && (await ev(`document.querySelectorAll('.composer .attchip, .composer .attthumb').length`)) === 1);
    await cards('release()'); await sleep(700);
    check('R2: once known, a file with no text says it skips the open card, and can go', !(await waits()) && (await note()) === SKIPS);
    await sendNow();
    check('R2: the file goes with the pick ("Answer: Archive") and the skip ("Skipped")', (await notes()).join('|') === 'Answer: Archive|Skipped');

    // a pick on a card not known yet: a text message can't carry it, and keeps it
    await cards('hold()');
    await newCard('q-u4', 'Ship the hotfix to beta?', ['Ship', 'Wait']);
    await pickOpt('q-u4', 'Ship');
    check('R2, query held: a pick on a card not known yet shows on the card, not in the composer',
      (await opts('q-u4')).find((o) => o.l === 'Ship')?.on && (await chips()).length === 0);
    await b.type('.composer textarea', 'Unrelated: the lab hub is back.');
    await sendNow();
    check('R2: a message that couldn\'t carry that pick leaves it saved', (await saved())?.picks?.['q-u4']?.join() === 'Ship');
    await cards('release()'); await sleep(700);
    check('R2: once the store answers, the kept pick is back in the composer', (await chips()).some((x) => x.includes('Ship')));
    await pickOpt('q-u4', 'Ship');
  } catch (e) {
    check('R2 unknown-card scenario ran to the end: ' + e.message, false);
  }

  // ---------- review R3: Reply in the chat while a message's file is prepared
  // window.__hcSend (the mock) holds a message with a file while it is being
  // prepared. The box is locked then, but Reply and picks in the chat still
  // work. What is chosen meanwhile isn't part of that message: once it is
  // queued that must still be in the composer, and saved, and go with the
  // next message. What the message carried is done.
  try {
    const CI = 'nightly-ci.alex.8d21e0';
    await ev(`localStorage.removeItem('hubchat-mock-draft-answers'), true`);
    await b.nav(base + '?pages=1', 1200);
    await openChat('nightly-ci');
    const cards = (js) => ev(`(window.__hcCards.${js}, true)`);
    const files = (js) => ev(`(window.__hcSend.${js}, true)`);
    const saved = () => ev(`JSON.parse(localStorage.getItem('hubchat-mock-draft-answers') || '{}')[${JSON.stringify(CI)}] ?? null`);
    const outs = () => ev(`document.querySelectorAll('.timeline .msg.out').length`);
    const newCard = async (id, body, options) => { await cards(`card(${JSON.stringify(CI)}, '${id}', ${JSON.stringify(body)}, ${JSON.stringify(options)})`); await sleep(700); };
    const bar = () => ev(`[...document.querySelectorAll('.composer .replybar .rb-txt')].map((e) => e.innerText)`);
    const boxText = () => ev(`document.querySelector('.composer textarea').value`);
    const fileCount = () => ev(`document.querySelectorAll('.composer .attchip, .composer .attthumb').length`);
    const preparing = () => ev(`!!document.querySelector('.composer .comp-preparing') && document.querySelector('.composer textarea').disabled`);
    const notes = () => ev(`[...${sent}.querySelectorAll('.quote')].map((q) => q.querySelector('.q-ans')?.innerText ?? '')`);
    // Send, held while its file is prepared; then let it go on and wait
    // until it is in the chat and the box is free again
    const sendAndHold = async () => {
      await files('hold()');
      await b.click('.composer .sendbtn', 100);
      for (let i = 0; i < 30 && !(await preparing()); i++) await sleep(100);
    };
    const letGo = async (n0) => {
      await files('release()');
      for (let i = 0; i < 50 && ((await outs()) <= n0 || (await preparing())); i++) await sleep(100);
      await sleep(500);
    };

    // the message: a reply to one card and a pick on another, text and a file
    await newCard('q-p0', 'Anything QA should know about tonight\'s build?', []);
    await newCard('q-p1', 'Which build goes to QA?', ['Nightly', 'RC']);
    await newCard('q-p2', 'Who signs off on the QA run?', []);
    await newCard('q-p3', 'Rerun the flaky upload test?', ['Rerun', 'Skip it']);
    await pickOpt('q-p1', 'RC');
    await b.click('#m-q-p0 .q-hint', 300);
    await b.type('.composer textarea', 'Nothing unusual. QA gets the RC.');
    await b.click('.composer [aria-label="Attach files"]', 500);
    let n0 = await outs();
    await sendAndHold();
    check('R3: Send with a file: while the file is prepared the box is locked ("Preparing files...") and the message is not in the chat yet',
      (await preparing()) && (await outs()) === n0);
    // meanwhile, in the chat: Reply to a third card (its header hint), a pick
    // on a fourth, and another pick on the card the message answers
    await b.click('#m-q-p2 .q-hint', 300);
    await pickOpt('q-p3', 'Rerun');
    await pickOpt('q-p1', 'Nightly');
    check('R3: meanwhile Reply and picks in the chat still work: the composer now replies to the third card, and the new pick is a chip',
      (await preparing()) && (await bar()).some((t) => t.includes('Who signs off on the QA run?')) && (await chips()).some((x) => x.includes('Rerun')));
    await b.shot(join(out, '14-reply-while-preparing-desktop-light.png'));
    await letGo(n0);
    check('R3: the message goes as it was sent: it quotes the first two cards ("Answer: RC" on the second), and the box is empty',
      (await outs()) === n0 + 1 && (await notes()).join('|') === '|Answer: RC' && (await boxText()) === '' && (await fileCount()) === 0);
    check('R3: the reply chosen meanwhile is still in the composer (the third card)',
      (await bar()).some((t) => t.includes('Who signs off on the QA run?')));
    const c3 = await chips();
    check('R3: the pick made meanwhile on the fourth card is still a chip; the one on the card the message answered is gone (first answer final)',
      c3.length === 1 && c3[0].includes('Rerun'));
    const s3 = await saved();
    const kept = s3?.replies?.join() === 'q-p2' && Object.keys(s3?.picks ?? {}).join() === 'q-p3' && s3.picks['q-p3'].join() === 'Rerun';
    check('R3: and both are still saved (a restart brings them back): replies "q-p2", a pick only on "q-p3"', kept);
    if (!kept) console.log('   saved:', JSON.stringify(s3));
    await b.shot(join(out, '15-kept-after-queued-desktop-light.png'));
    await b.type('.composer textarea', 'Sam signs off.');
    await sendNow();
    check('R3: the next message carries them: it answers the third card with its text and the fourth with "Rerun"; nothing stays saved',
      (await notes()).join('|') === '|Answer: Rerun' && (await saved()) === null);

    // control: nothing chosen meanwhile, so all the message carried clears
    await newCard('q-p4', 'Tag the release candidate?', ['Tag it', 'Not yet']);
    await newCard('q-p5', 'Anything for the changelog?', []);
    await pickOpt('q-p4', 'Tag it');
    await b.click('#m-q-p5 .q-hint', 300);
    await b.type('.composer textarea', 'The upload fix.');
    await b.click('.composer [aria-label="Attach files"]', 500);
    n0 = await outs();
    await sendAndHold();
    await letGo(n0);
    check('R3 control: with nothing chosen meanwhile, the message carries the pick and the reply, and then the composer and the saved answers are empty',
      (await notes()).join('|') === 'Answer: Tag it|' && !(await ev(`!!document.querySelector('.composer .replybar')`)) && (await chips()).length === 0 && (await saved()) === null);

    // a failed preparation sends nothing and keeps the current choice, here
    // two cards picked with Ctrl+click and the selection's Reply meanwhile
    await newCard('q-p6', 'Can QA start before 09:00?', []);
    await newCard('q-p7', 'Should the lab hub stay on 2.1?', ['Stay', 'Upgrade']);
    await newCard('q-p8', 'Anyone else to tell?', []);
    await b.click('#m-q-p6 .q-hint', 300);
    await b.type('.composer textarea', 'Yes, from 08:30.');
    await b.click('.composer [aria-label="Attach files"]', 500);
    n0 = await outs();
    await sendAndHold();
    await clickAt(`document.querySelector('#m-q-p7 .kchip')`, 2);
    await clickAt(`document.querySelector('#m-q-p8 .kchip')`, 2);
    await b.click('.selbar .btn', 300);
    check('R3: meanwhile the selection\'s Reply works too: "Replying to 2 messages"',
      (await preparing()) && (await msgText('.composer .replybar.multi .rb-who')) === 'Replying to 2 messages');
    await files('fail()'); await sleep(700);
    const s4 = await saved();
    check('R3 control: a failed preparation sends nothing, shows the error, and keeps the text, the file and the two replies chosen meanwhile (saved too)',
      (await outs()) === n0 && (await ev(`!!document.querySelector('.composer .comp-err')`)) && (await boxText()) === 'Yes, from 08:30.' && (await fileCount()) === 1
        && (await ev(`document.querySelectorAll('.composer .rb-item').length`)) === 2 && s4?.replies?.join() === 'q-p7,q-p8');
  } catch (e) {
    check('R3 preparation scenario ran to the end: ' + e.message, false);
  }

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

  // ------------------------- review R1: settlement from the store (the module)
  try {
    const Q = await server.ssrLoadModule('/src/lib/questions.ts');
    const q = msg({ id: 'q', outgoing: false, kind: 'question', body: 'Which?', question: { options: ['A', 'B'], multiple: false }, received_at: '2026-10-12T09:00:00.000Z' });
    const a = ans('a', 'A', { received_at: '2026-10-12T09:05:00.000Z' });
    const chat = (n) => Array.from({ length: n }, (_, i) => msg({ id: 'c' + String(i).padStart(2, '0'), outgoing: i % 2 === 0, body: 'chat', received_at: '2026-10-12T10:00:00.000Z' }));
    const s1 = Q.settlement([a, ...chat(49)], [q, a]);
    check("settlement: the reviewer's page (the answer and 49 later, not the card) with the store's card and first answer: answered, not open",
      s1.states.get('q')?.st === 'answered' && Q.isSettled(s1, 'q') && !Q.isOpen(s1, 'q'));
    const s2 = Q.settlement(chat(50), [q, a]);
    check('settlement: card and answer both beyond the page: answered', Q.isSettled(s2, 'q') && !Q.isOpen(s2, 'q'));
    const q2 = msg({ id: 'q2', outgoing: false, kind: 'question', body: 'Still open?', question: { options: ['A', 'B'] }, received_at: '2026-10-12T09:10:00.000Z' });
    const s3 = Q.settlement(chat(50), [q2]);
    check('settlement: a card the store holds with no answer is open (positive control)', Q.isOpen(s3, 'q2') && !Q.isSettled(s3, 'q2'));
    const s4 = Q.settlement(chat(50), []);
    check('settlement: before the store has answered, a card is unknown: neither open nor settled', !Q.isOpen(s4, 'q') && !Q.isSettled(s4, 'q'));
    const s5 = Q.settlement([q, ans('b-late', 'B', { received_at: '2026-10-12T11:00:00.000Z' }), ...chat(48)], [q, a]);
    check("settlement: a later answer on the page doesn't take the place of the store's earlier one", s5.states.get('q')?.st === 'answered' && s5.states.get('q').by.id === 'a');
  } catch (e) {
    check('settlement checks ran: ' + e.message, false);
  }

  // ----------------- review R1: saved answers across an identity switch (module)
  // The real answers.ts, its two API calls held back by the proof: a read
  // the old identity started must not land in the next identity's cache.
  try {
    const { api } = await server.ssrLoadModule('/src/api.ts');
    const A = await server.ssrLoadModule('/src/lib/answers.ts');
    const writes = [], reads = [];
    const orig = { draftAnswers: api.draftAnswers, setDraftAnswers: api.setDraftAnswers };
    api.draftAnswers = () => new Promise((r) => reads.push(r));
    api.setDraftAnswers = async (_peer, d) => { writes.push(JSON.stringify(d)); };
    const last = () => writes.at(-1), P = 'x.000001';
    const OLD = { replies: ['old-q'], picks: { 'old-q': ['Old pick'] } };
    let r = A.loadDraftAnswers(P);
    A.forgetDraftAnswers();
    reads.shift()(OLD); await r;
    A.setReplies(P, ['new-q']);
    check('saved answers: a read the old identity started, landing after the switch, is dropped (the next edit saves no old pick)', last() === JSON.stringify({ replies: ['new-q'], picks: {} }));
    A.forgetDraftAnswers();
    const rOld = A.loadDraftAnswers(P);
    A.forgetDraftAnswers();
    const rNew = A.loadDraftAnswers(P);
    const [old, fresh] = [reads.shift(), reads.shift()];
    fresh(null); await rNew;
    old(OLD); await rOld;
    A.setReplies(P, ['n2']);
    check("saved answers: a new identity's empty read leaves no slot for the old identity's read landing later", last() === JSON.stringify({ replies: ['n2'], picks: {} }));
    A.forgetDraftAnswers();
    r = A.loadDraftAnswers(P);
    reads.shift()({ replies: ['r'], picks: { q: ['A'] } }); await r;
    A.pick(P, 'q2', 'B', false);
    check("saved answers: the same identity's read is taken (positive control)", last() === JSON.stringify({ replies: ['r'], picks: { q: ['A'], q2: ['B'] } }));
    A.forgetDraftAnswers();
    r = A.loadDraftAnswers(P);
    A.setReplies(P, ['current']);
    reads.shift()(OLD); await r;
    A.pick(P, 'q', 'B', false);
    check('saved answers: an edit made while a read was out wins over it (positive control)', last() === JSON.stringify({ replies: ['current'], picks: { q: ['B'] } }));
    Object.assign(api, orig);
  } catch (e) {
    check('saved-answers checks ran: ' + e.message, false);
  }
  // the R2 part makes the store's answer fail on purpose; the view warns
  const unexpected = b.errors.filter((e) => !e.includes('mock: cardAnswers failing on purpose'));
  check('no console errors or exceptions (other than the failures the R2 part causes)', unexpected.length === 0);
  if (unexpected.length) console.log(unexpected.join('\n'));
} catch (e) {
  check('proof ran to the end: ' + e.message, false);
} finally {
  writeFileSync(join(out, 'result.json'), JSON.stringify({ checks, errors: b.errors }, null, 2));
  console.log(`${checks.filter((c) => c.ok).length}/${checks.length} passed`);
  await b.close(); await server.close();
}
process.exit(checks.length && checks.every((c) => c.ok) ? 0 : 1);
