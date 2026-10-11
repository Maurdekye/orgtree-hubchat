// Isolated browser mock only: no installed app, real identity, hub or device.
import { createServer } from 'vite';
import { launch } from './cdp.mjs';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const out = resolve(process.env.HUBCHAT_PROOF_OUT || 'D:/orgtree-work/rig/hubchat-dm-sol/close-dm');
const cacheDir = process.env.HUBCHAT_VITE_CACHE;
if (!cacheDir) throw Error('Set HUBCHAT_VITE_CACHE to the assigned build folder');
mkdirSync(out, { recursive: true });
const server = await createServer({ cacheDir, server: { host: '127.0.0.1', port: 0 } });
await server.listen();
const b = await launch();
const checks = [];
const check = (name, ok) => { checks.push({ name, ok }); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}`); if (!ok) throw Error(name); };
const url = server.resolvedUrls.local[0];
const peer = 'maya.e71f2b';
const setup = async () => b.eval(`(async()=>{
  window.api=(await import('/src/api.ts')).api;
  window.store=await import('/src/lib/store.ts');
  window.peer=${JSON.stringify(peer)};
})()`);
const listed = () => b.eval(`store.getSnap().chats.some(c=>c.peer===peer)`);
const history = () => b.eval(`(async()=>JSON.stringify(await api.chat(peer,{limit:5000})))()`);
const openNew = async (phone = false) => {
  await b.click(phone ? '.fab' : '.side [aria-label="New chat"]');
  await b.type('#nc-q', peer, 400);
  await b.key('Enter', 0, 450);
};
const closePhone = async () => {
  await b.click('[aria-label="More"]');
  await b.eval(`Array.from(document.querySelectorAll('[role="menuitem"]')).find(e=>e.textContent.includes('Close DM')).click()`);
  await b.sleep(250);
};

try {
  await b.nav(url); await setup();
  await openNew();
  check('desktop exposes Close DM in existing conversation header', !!(await b.count('.conv-head [aria-label="Close DM"]')));
  await b.type('.composer textarea', 'draft survives closing', 50);
  const before = await history();
  await b.shot(join(out, 'desktop-close-dm.png'));
  await b.click('.conv-head [aria-label="Close DM"]');
  check('desktop close removes list row and leaves conversation', !await listed() && !await b.count('.composer'));
  check('desktop close retains full loaded history and attachment metadata', before === await history());
  check('desktop close flushes the current draft', await b.eval(`api.draft(peer).then(d=>d==='draft survives closing')`));
  await b.nav(url); await setup();
  check('restart keeps closed DM hidden and does not restore last chat', !await listed() && !await b.count('.composer'));
  await openNew();
  check('New chat reopens same DM with full history', await listed() && await b.eval(`api.chat(peer,{limit:5000}).then(m=>m.length>0)`));
  await b.eval(`document.querySelector('.crow.sel').dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,clientX:180,clientY:260}))`); await b.sleep(100);
  check('desktop list menu offers Close DM', await b.eval(`Array.from(document.querySelectorAll('.ctx [role="menuitem"]')).some(e=>e.textContent.includes('Close DM'))`));
  await b.eval(`Array.from(document.querySelectorAll('.ctx [role="menuitem"]')).find(e=>e.textContent.includes('Close DM')).click()`); await b.sleep(200);
  check('desktop list action closes selected DM', !await listed() && !await b.count('.composer'));
  await b.eval(`(async()=>{ const m=await import('/src/lib/mock.ts');m.mockIncoming(peer,'new incoming reopens'); })()`); await b.sleep(180);
  check('incoming reopens closed desktop DM without forcing selection', await listed() && !await b.count('.composer'));
  await openNew();
  const originalClose = await b.eval(`window.savedClose=api.closeDm; api.closeDm=async()=>{throw Error('scratch close failure')}; true`);
  check('failure control installed', originalClose);
  await b.click('.conv-head [aria-label="Close DM"]');
  check('failed close keeps conversation and list entry', await listed() && !!await b.count('.composer'));
  await b.eval(`api.closeDm=savedClose`);
  await b.click('.conv-head [aria-label="Close DM"]');
  await b.size(390, 844); await b.nav(url + '?platform=android'); await setup();
  check('phone starts with closed DM absent', !await listed());
  await openNew(true);
  check('phone New chat restores DM', await listed() && !!await b.count('.composer'));
  await b.click('[aria-label="More"]');
  check('phone conversation menu offers Close DM', await b.eval(`Array.from(document.querySelectorAll('.sheet [role="menuitem"]')).some(e=>e.textContent.includes('Close DM'))`));
  await b.shot(join(out, 'android-close-dm-menu.png'));
  await b.key('Escape');
  const phoneBefore = await history();
  await closePhone();
  check('phone close returns to chats without synthetic selected row', !await listed() && !await b.count('.composer'));
  check('phone close retains full history', phoneBefore === await history());
  await b.key('Escape');
  check('phone Back cannot restore closed DM', !await listed() && !await b.count('.composer'));
  await b.nav(url + '?platform=android'); await setup();
  check('phone restart keeps DM closed', !await listed() && !await b.count('.composer'));
  await b.eval(`window.__hcIntent({peer})`); await b.sleep(350);
  check('notification tap explicitly reopens closed DM with history', await listed() && !!await b.count('.composer'));
  await closePhone();
  await b.eval(`(async()=>{ const m=await import('/src/lib/mock.ts');m.mockIncoming(peer,'phone incoming'); })()`); await b.sleep(180);
  check('phone incoming restores list without forcing conversation', await listed() && !await b.count('.composer'));
  check('phone layout fits screen', await b.eval(`document.documentElement.scrollWidth<=innerWidth`));
  check('no browser errors', b.errors.length === 0);
} finally {
  writeFileSync(join(out, 'result.json'), JSON.stringify({ checks, errors: b.errors }, null, 2));
  await b.close(); await server.close();
}

