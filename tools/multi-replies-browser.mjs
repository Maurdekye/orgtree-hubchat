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
  // The reported alpha.4 phone sequence: type to two cards and pick answers
  // on a third. The picks go first; the text then answers both original cards.
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
  let phone = await ev(`window.__multiSent`);
  check('android: control phone picks send only third-card answers and retain all typed bytes', phone.length===1 && phone[0].reply_to==='repro-q2' && phone[0].body==='' && !phone[0].attachments?.length && phone[0].answers?.[0]?.picked.join()==='Red,Green,Blue' && (await ev(`document.querySelector('.composer textarea').value`))==='One typed answer to questions one and three.');
  check('android: append phone answer send retains both independent reply targets', (await ev(`document.querySelector('.composer .rb-who')?.innerText`))==='Replying to 2 messages');
  await b.shot(join(out,'phone-sequence-retained-text.png'));
  await b.click('.composer .sendbtn',100); await wait(1000); phone=await ev(`window.__multiSent`);
  check('android: append phone later Send carries typed text to both cards without a skip', phone.length===2 && phone[1].body==='One typed answer to questions one and three.' && phone[1].reply_to==='repro-q1' && phone[1].replies?.join()==='repro-q1,repro-q3' && !phone[1].answers?.length);
  check('android: append phone both text targets settle as answers, not skips', await ev(`/^Answered/.test(document.querySelector('#m-repro-q1 .q-state')?.innerText||'') && /^Answered/.test(document.querySelector('#m-repro-q3 .q-state')?.innerText||'')`));
  await b.shot(join(out,'phone-sequence-two-text-answers.png'));
}
