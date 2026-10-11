// Browser mock proof of queue admission and real file-card rendering.
import {createServer} from 'vite';
import {launch} from './cdp.mjs';
import {mkdirSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
const out='D:/orgtree-work/rig/agy-astra/hubchat-queued-download';mkdirSync(out,{recursive:true});
const server=await createServer({server:{host:'127.0.0.1',port:0}});await server.listen();
const browser=await launch();const checks=[];
const check=(name,ok)=>{checks.push({name,ok});console.log(`${ok?'PASS':'FAIL'} ${name}`);if(!ok)throw Error(name);};
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
try {
 await browser.nav(server.resolvedUrls.local[0]);
 await browser.eval(`(async()=>{
  const {api}=await import('/src/api.ts');window.api=api;
  const chats=await api.chats();let target;
  for(const c of chats){const msgs=await api.chat(c.peer);target=msgs.find(m=>m.attachments.some(a=>a.state==='remote'&&!/\\.(png|jpg|gif)$/i.test(a.name)));if(target)break;}
  if(!target)throw Error('no remote fixture');
  window.mid=target.id;window.lid=target.attachments.find(a=>a.state==='remote').local_id;
  document.querySelector('#root').style.display='none';const node=document.createElement('div');node.id='proof';node.style.cssText='padding:48px;max-width:600px';document.body.append(node);
  const React=await import('/node_modules/.vite/deps/react.js');const DOM=await import('/node_modules/.vite/deps/react-dom_client.js');const {AttCard}=await import('/src/components/MessageView.tsx');
  const root=DOM.default.createRoot(node);window.shown=true;
  window.render=async()=>{const m=await api.message(window.mid);window.current=m.attachments.find(a=>a.local_id===window.lid);root.render(window.shown?React.default.createElement(AttCard,{m,a:window.current}):null);};
  await api.onEvent(e=>{if(e.type==='chat')void window.render();});await window.render();
 })()`);await sleep(100);
 check('remote card offers Download',await browser.eval(`document.querySelector('#proof').textContent.includes('Download')`));
 check('API returns queued before completion',await browser.eval(`(async()=>{await window.api.download(window.mid,window.lid);const m=await window.api.message(window.mid);return m.attachments.find(a=>a.local_id===window.lid).download.state==='queued';})()`));
 await sleep(60);check('queued card visible with cancellation',await browser.eval(`document.querySelector('#proof').textContent.includes('Waiting to download')&&!!document.querySelector('#proof .att-x')`));
 await browser.click('#proof .att-x',30);
 check('queued cancellation is visible',await browser.eval(`document.querySelector('#proof').textContent.includes('cancelled')`));
 await browser.click('#proof .att-file',40);
 check('explicit Retry queues again',await browser.eval(`window.current.download.state==='queued'`));
 await browser.eval(`window.shown=false;window.render()`);await sleep(800);
 await browser.eval(`window.shown=true;window.render()`);await sleep(50);
 check('reopened card restores confirmed progress',await browser.eval(`window.current.download.confirmed>0&&document.querySelector('#proof').textContent.includes(' of ')`));
 await browser.shot(join(out,'progress-desktop.png'));
 await sleep(1300);
 check('finishing shown at complete bytes without cancel',await browser.eval(`document.querySelector('#proof').textContent.includes('Finishing\u2026')&&!document.querySelector('#proof .att-x')`));
 await browser.shot(join(out,'finishing-desktop.png'));
 await sleep(600);
 check('completed file offers Open silently',await browser.eval(`document.querySelector('#proof').textContent.includes('Saved')&&window.current.local_path&&!document.body.textContent.includes('Saved to Downloads')`));
 // Exercise durable snapshot presentation independent of ephemeral progress.
 check('sent-file download failure retries download',await browser.eval(`(async()=>{const {attView}=await import('/src/lib/peers.ts');const a={...window.current,download:{state:'failed',confirmed:8388608,retry_at:0,error:'connection failed'}};return attView(a,{outgoing:true}).retry==='download';})()`));
 await browser.size(390,844);
 await browser.eval(`document.querySelector('#proof').style.padding='16px';window.current.download={state:'paused',confirmed:1024,retry_at:0,error:null};`);
 check('confirmed progress does not require a live event',await browser.eval(`(async()=>{const {attView}=await import('/src/lib/peers.ts');return attView(window.current,{outgoing:false}).sub.includes('Paused');})()`));
 writeFileSync(join(out,'result.json'),JSON.stringify({checks},null,2));
} finally {await browser.close();await server.close();}
