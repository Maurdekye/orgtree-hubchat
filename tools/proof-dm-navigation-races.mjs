// Controlled deferred API replies in the actual mounted layouts, against mock data only.
import { createServer } from 'vite';
import { launch } from './cdp.mjs';
import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
const out = resolve(process.env.HUBCHAT_PROOF_OUT || 'D:/orgtree-work/rig/hubchat-dm-sol/navigation-races');
if (!process.env.HUBCHAT_VITE_CACHE) throw Error('Set assigned HUBCHAT_VITE_CACHE');
mkdirSync(out, { recursive: true });
const server = await createServer({ cacheDir: process.env.HUBCHAT_VITE_CACHE, server: { host: '127.0.0.1', port: 0 } });
await server.listen();
const b = await launch();
const checks = [];
const check = (name, ok) => { checks.push({ name, ok }); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}`); };
const A = 'maya.e71f2b', B = 'pat.4d5e6f';
const fresh = async (phone = false) => {
  await b.size(phone ? 390 : 1440, phone ? 844 : 900);
  await b.nav(server.resolvedUrls.local[0] + (phone ? '?platform=android' : ''));
  await b.eval(`localStorage.clear()`);
  await b.nav(server.resolvedUrls.local[0] + (phone ? '?platform=android' : ''));
  await b.eval(`(async()=>{
    window.api=(await import('/src/api.ts')).api;
    window.store=await import('/src/lib/store.ts');
    window.pending=[];window.realOpen=api.openDm;window.realClose=api.closeDm;
    api.openDm=peer=>new Promise((resolve,reject)=>pending.push({op:'open',peer,resolve,reject}));
    api.closeDm=peer=>new Promise((resolve,reject)=>pending.push({op:'close',peer,resolve,reject}));
    window.settle=async(i,fail=false)=>{const p=pending[i];if(fail)p.reject(Error('deferred stale failure'));else {await (p.op==='open'?realOpen:realClose)(p.peer);p.resolve();}};
  })()`);
};
const row = async (who) => {
  await b.eval(`Array.from(document.querySelectorAll('.crow')).find(e=>e.querySelector('.crow-name')?.textContent===${JSON.stringify(who)}).click()`);
  await b.sleep(60);
};
const settle = async (i, fail = false) => { await b.eval(`settle(${i},${fail})`); await b.sleep(180); };
const selected = (name) => b.eval(`document.querySelector('.conv-head .name,.appbar .who .ell')?.textContent===${JSON.stringify(name)}`);
const closePhone = async () => {
  await b.click('[aria-label="More"]', 60);
  await b.eval(`Array.from(document.querySelectorAll('.sheet [role="menuitem"]')).find(e=>e.textContent.includes('Close DM')).click()`);
  await b.sleep(60);
};
try {
  await fresh(); await row('Maya Lin'); await row('Pat Peer');
  check('desktop predecessor control queues A then B', await b.eval(`pending.length===2&&pending[0].peer===${JSON.stringify(A)}&&pending[1].peer===${JSON.stringify(B)}`));
  await settle(1); await settle(0);
  check('desktop A then B, A completes last: B stays selected', await selected('Pat Peer'));

  await fresh(); await row('Maya Lin'); await b.click('.side [aria-label="Settings"]'); await settle(0);
  check('desktop newer Settings overlay survives delayed open', !!await b.count('.modal.settings'));

  await fresh(); await row('Maya Lin'); await b.click('.side [aria-label="New chat"]'); await b.key('Escape'); await settle(0);
  check('desktop dismissed newer overlay cancels pending open', !await b.count('.composer') && !await b.count('#nc-q'));

  await fresh(); await row('Maya Lin'); await settle(0);
  await b.click('.conv-head [aria-label="Close DM"]', 60); await row('Maya Lin'); await settle(2); await settle(1);
  check('desktop newer same-peer reopen survives delayed close', await selected('Maya Lin'));

  await fresh(); await row('Maya Lin'); await b.click('.side [aria-label="Settings"]'); await settle(0, true);
  check('desktop stale open rejection does not show an obsolete error', !await b.eval(`document.body.innerText.includes('deferred stale failure')`));

  await fresh(true); await row('Maya Lin'); await row('Pat Peer'); await settle(1); await settle(0);
  check('phone A then B, A completes last: B stays selected', await selected('Pat Peer'));

  await fresh(true); await b.click('.fab'); await b.type('#nc-q', A, 400); await b.key('Enter', 0, 60);
  check('phone Back control has an unresolved open', await b.eval(`pending.length===1`));
  await b.click('[aria-label="Back"]'); await settle(0);
  check('phone Back cancels delayed open from New chat', !await b.count('.composer') && !!await b.count('.fab'));

  await fresh(true); await row('Maya Lin');
  await b.eval(`window.__hcBack()`); await settle(0);
  check('phone native Back at root cancels pending open', !await b.count('.composer') && !!await b.count('.fab'));

  await fresh(true); await row('Maya Lin'); await settle(0); await closePhone();
  await b.key('Escape'); await b.key('Escape'); await row('Pat Peer'); await settle(2); await settle(1);
  check('phone newer conversation survives delayed close', await selected('Pat Peer'));

  await fresh(true); await row('Maya Lin'); await settle(0); await closePhone();
  await b.key('Escape'); await b.key('Escape'); await b.click('[aria-label="Settings"]'); await settle(1);
  check('phone newer Settings screen survives delayed close', await b.eval(`!!document.querySelector('.appbar')&&document.querySelector('.appbar').textContent.includes('Settings')`));

  await fresh(true); await row('Maya Lin'); await settle(0); await closePhone();
  await b.eval(`(async()=>{window.oldState=api.state;api.state=async()=>{const s=await oldState();return {...s,me:{...s.me,address:'scratch-switch.222222'}}};await store.refreshState();})()`); await b.sleep(150);
  await b.eval(`history.replaceState({hc:1,proof:'new-identity'},'')`); await settle(1);
  check('phone unmounted layout cannot reset new identity history', await b.eval(`history.state.proof==='new-identity'`));

  check('browser has no exceptions', b.errors.length === 0);
} finally {
  writeFileSync(join(out, 'result.json'), JSON.stringify({ predecessor: process.env.HUBCHAT_PREDECESSOR === '1', checks, errors: b.errors }, null, 2));
  await b.close(); await server.close();
}
const failures = checks.filter(c => !c.ok);
if (process.env.HUBCHAT_PREDECESSOR === '1') {
  if (failures.length < 6) throw Error('Predecessor did not reproduce enough independent races');
  console.log(`PREDECESSOR: ${failures.length} navigation invariants failed as expected`);
} else if (failures.length) throw Error(`${failures.length} navigation invariants failed`);
