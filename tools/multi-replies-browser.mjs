// Actual desktop/Android Conversation UI, ordinary Reply twice, selection
// merge, remove, reload and one typed Send. Used unchanged on base and fix.
import { join } from 'node:path';
export async function multiReplies({ b, base, out, check }) {
  const ev = js => b.eval(js);
  const wait = ms => new Promise(r => setTimeout(r, ms));
  const row = text => `[...document.querySelectorAll('.timeline .msg')].find(e => e.innerText.includes(${JSON.stringify(text)}))`;
  const replies = () => ev(`JSON.parse(localStorage.getItem('hubchat-mock-draft-answers') || '{}')['orgtree.alex.7c41d2']?.replies || []`);
  const open = async () => {
    for (let i = 0; i < 30; i++) {
      if (await ev(`(() => { const r = [...document.querySelectorAll('.crow')].find(e => e.innerText.includes('Orgtree')); if (!r) { document.querySelector('.appbar [aria-label=Back]')?.click(); return false; } r.click(); return true; })()`)) { await wait(700); return; }
      await wait(150);
    }
    throw Error('synthetic Orgtree chat unavailable');
  };
  for (const platform of ['desktop', 'android']) {
    const p = platform + ': ';
    await b.size(platform === 'android' ? 390 : 1440, platform === 'android' ? 844 : 900, 1);
    await b.nav(base, 900);
    await ev(`localStorage.removeItem('hubchat-mock-draft-answers'), true`);
    const url = base + (platform === 'android' ? '?platform=android' : '');
    await b.nav(url, 1200); await open();
    // Public Reply actions, not a test-only setter of the reply list.
    await b.click('#m-q-channel .q-hint', 250);
    await b.click('#m-q-notes .q-hint', 250);
    check(p + 'append two ordinary Reply actions collects both cards', (await ev(`document.querySelector('.composer .rb-who')?.innerText`)) === 'Replying to 2 messages');
    const before = await replies();
    await b.click('#m-q-notes .q-hint', 250);
    check(p + 'control repeating Reply dedupes the target', JSON.stringify(await replies()) === JSON.stringify(before));
    const file = row('Done. Here is the change');
    if (platform === 'desktop') {
      await ev(`(() => { const e = ${file}; e.dispatchEvent(new MouseEvent('click',{bubbles:true,ctrlKey:true})); document.querySelector('#m-q-notes .kchip').dispatchEvent(new MouseEvent('click',{bubbles:true,ctrlKey:true})); return true; })()`);
      await wait(200);
      await b.click('.selbar .btn', 250);
    } else {
      const box = await ev(`(() => { const e = ${file}; e.scrollIntoView({block:'center'}); const r=e.getBoundingClientRect(); return {x:r.left+30,y:r.top+r.height/2}; })()`);
      await b.send('Input.dispatchMouseEvent', {type:'mousePressed', x:box.x, y:box.y, button:'left',clickCount:1});
      await wait(650);
      await b.send('Input.dispatchMouseEvent', {type:'mouseReleased', x:box.x, y:box.y, button:'left',clickCount:1});
      await wait(200);
      await b.click('#m-q-notes .kchip', 200);
      await b.click('.appbar.select [aria-label=Reply]', 250);
    }
    check(p + 'append bulk Reply merges targets and dedupes overlap', (await ev(`document.querySelector('.composer .rb-who')?.innerText`)) === 'Replying to 3 messages');
    await ev(`(() => { const e = [...document.querySelectorAll('.composer .rb-item')].find(e=>e.innerText.includes('rc3-test-summary.txt')); e.querySelector('button').click(); return true; })()`);
    check(p + 'control file target can be removed individually', !(await ev(`document.querySelector('.composer .replybar')?.innerText.includes('rc3-test-summary.txt')`)));
    await b.nav(url, 1200); await open();
    check(p + 'append targets survive reload as two cards', (await ev(`document.querySelector('.composer .rb-who')?.innerText`)) === 'Replying to 2 messages');
    await b.shot(join(out, `multi-before-${platform}.png`));
    await ev(`(async()=>{ const {api}=await import('/src/api.ts'); window.__multiSent=[]; const send=api.send; api.send=async m=>{window.__multiSent.push(structuredClone(m)); return send(m);}; return true; })()`);
    await b.type('.composer textarea', 'One text reply to both cards.');
    await b.click('.composer .sendbtn', 100); await wait(1000);
    const sent = await ev(`window.__multiSent`);
    check(p + 'append then one Send carries both targets and typed text', sent.length === 1 && sent[0].body === 'One text reply to both cards.' && sent[0].reply_to === 'q-channel' && sent[0].replies?.join() === 'q-channel,q-notes' && !sent[0].answers?.length);
    check(p + 'append typed reply settles both targeted cards', await ev(`/^Answered/.test(document.querySelector('#m-q-channel .q-state')?.innerText||'') && /^Answered/.test(document.querySelector('#m-q-notes .q-state')?.innerText||'')`));
    check(p + 'append received rendering shows both reply quotes', (await ev(`[...document.querySelectorAll('.timeline .msg.out')].pop().querySelectorAll('.quote').length`)) === 2);
    check(p + 'control sent draft clears its targets', !(await ev(`!!document.querySelector('.composer .replybar')`)));
    await b.shot(join(out, `multi-after-${platform}.png`));
  }
  // The final signed phone sequence: Reply Q1, then Q3, picks on Q2.
  // One Send carries text for Q1/Q3 and picks for Q2, exclusive per card.
  await b.size(390,844,1);
  await b.nav(base,900); await ev(`localStorage.removeItem('hubchat-mock-draft-answers'), true`);
  await b.nav(base+'?platform=android',1200); await open();
  await ev(`window.__hcCards.card('orgtree.alex.7c41d2','repro-q1','Phone question one',['One','Two']), window.__hcCards.card('orgtree.alex.7c41d2','repro-q2','Phone question two',['Red','Green','Blue'],true), window.__hcCards.card('orgtree.alex.7c41d2','repro-q3','Phone question three',[]), true`);
  await wait(700);
  await b.click('#m-repro-q1 .q-hint',250); await b.click('#m-repro-q3 .q-hint',250);
  check('android: append phone sequence keeps both typed-reply targets', (await ev(`document.querySelector('.composer .rb-who')?.innerText`))==='Replying to 2 messages');
  await b.type('.composer textarea','One typed answer to questions one and three.');
  for (const label of ['Red','Green','Blue']) {
    await ev(`(()=>{ const e=[...document.querySelectorAll('#m-repro-q2 .q-opt')].find(e=>e.innerText.trim()===${JSON.stringify(label)}); e.click(); return true;})()`); await wait(200);
  }
  await ev(`(async()=>{ const {api}=await import('/src/api.ts'); window.__multiSent=[]; const send=api.send; api.send=async m=>{window.__multiSent.push(structuredClone(m)); return send(m);}; return true; })()`);
  await b.click('.composer .sendbtn',100); await wait(1000);
  const phone = await ev(`window.__multiSent`);
  check('android: control phone typed bytes survive one mixed send', phone.length===1 && phone[0].body==='One typed answer to questions one and three.');
  check('android: append phone one Send replies to all three with picks only on Q2', phone.length===1 && phone[0].reply_to==='repro-q1' && phone[0].replies?.join()==='repro-q1,repro-q2,repro-q3' && phone[0].answers?.length===1 && phone[0].answers[0].question==='repro-q2' && phone[0].answers[0].picked.join()==='Red,Green,Blue');
  check('android: append phone both text targets settle as answers, not skips', await ev(`/^Answered/.test(document.querySelector('#m-repro-q1 .q-state')?.innerText||'') && /^Answered/.test(document.querySelector('#m-repro-q3 .q-state')?.innerText||'')`));
  check('android: control phone picked card settles with all three picked options', await ev(`/^Answered/.test(document.querySelector('#m-repro-q2 .q-state')?.innerText||'') && [...document.querySelectorAll('#m-repro-q2 .q-opt.on')].map(e=>e.innerText.trim()).join()==='Red,Green,Blue'`));
  check('android: append phone mixed send shows all three quotes', (await ev(`[...document.querySelectorAll('.timeline .msg.out')].pop().querySelectorAll('.quote').length`))===3);
  await b.shot(join(out,'phone-sequence-two-text-answers.png'));
  await ev(`window.__hcCards.card('orgtree.alex.7c41d2','empty-q1','Empty question one',[]), window.__hcCards.card('orgtree.alex.7c41d2','empty-q2','Empty question two',['Red']), window.__hcCards.card('orgtree.alex.7c41d2','empty-q3','Empty question three',[]), true`);
  await wait(600);await b.click('#m-empty-q1 .q-hint',200);await b.click('#m-empty-q3 .q-hint',200);await b.click('#m-empty-q2 .q-opt',200);
  check('android: append blank mixed warns of both intended skips',await ev(`/skips 2 questions/.test(document.querySelector('.skipnote')?.innerText||'')`));
  await b.click('.composer .sendbtn',100);await wait(1000);
  const blank=await ev(`window.__multiSent.at(-1)`);
  check('android: append blank mixed sends every skip and only the picked answer',blank?.body==='' && blank.replies?.join()==='empty-q1,empty-q2,empty-q3' && blank.answers?.length===1 && blank.answers[0].question==='empty-q2');
  check('android: append blank mixed settles both text targets as Skipped',await ev(`/^Skipped/.test(document.querySelector('#m-empty-q1 .q-state')?.innerText||'') && /^Skipped/.test(document.querySelector('#m-empty-q3 .q-state')?.innerText||'')`));
  check('android: control blank picked card is Answered',await ev(`/^Answered/.test(document.querySelector('#m-empty-q2 .q-state')?.innerText||'')`));
  await b.shot(join(out,'phone-empty-mixed-skips.png'));
  await ev(`window.__hcCards.card('orgtree.alex.7c41d2','mode-q','Mode question',['Red']), true`);await wait(500);
  await b.click('#m-mode-q .q-hint',200);await b.click('#m-mode-q .q-opt',200);
  check('android: control picking moves the card out of text targets',await ev(`!document.querySelector('.composer .replybar') && !!document.querySelector('.composer .anschip')`));
  await b.click('#m-mode-q .q-hint',200);
  check('android: append Reply clears that card pick and restores its text target',await ev(`!!document.querySelector('.composer .replybar') && !document.querySelector('.composer .anschip')`));
  await b.shot(join(out,'phone-exclusive-card-slot.png'));
}
