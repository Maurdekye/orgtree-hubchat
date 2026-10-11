// Actual file-card controls against the browser mock; core races have HTTP tests.
import {createServer} from 'vite';
import {launch} from './cdp.mjs';
import {mkdirSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
const out='D:/orgtree-work/rig/agy-astra/hubchat-upload-controls';mkdirSync(out,{recursive:true});
const server=await createServer({server:{host:'127.0.0.1',port:0}});await server.listen();
const browser=await launch();const checks=[];
const check=(name,ok)=>{checks.push({name,ok});console.log(`${ok?'PASS':'FAIL'} ${name}`);if(!ok)throw Error(name);};
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
try {
 await browser.nav(server.resolvedUrls.local[0]);
 await browser.eval(`(async()=>{
  const {api}=await import('/src/api.ts');window.api=api;
  await api.send({id:'held-upload-proof',peer:'unlisted-proof.abcdef',body:'Keep this message',attachments:[{name:'held.zip',bytes:1024,source:'mock-private-source'}]});
  document.querySelector('#root').style.display='none';const node=document.createElement('div');node.id='proof';node.style.cssText='padding:24px;max-width:600px';document.body.append(node);
  const React=await import('/node_modules/.vite/deps/react.js');const DOM=await import('/node_modules/.vite/deps/react-dom_client.js');const {AttCard}=await import('/src/components/MessageView.tsx');
  const root=DOM.default.createRoot(node);
  window.render=async()=>{window.current=await api.message('held-upload-proof');const m=window.current;root.render(React.default.createElement('div',null,React.default.createElement('p',null,m.body),...m.attachments.map(a=>React.default.createElement(AttCard,{key:a.local_id,m,a}))));};
  await api.onEvent(e=>{if(e.type==='chat')void window.render();});await window.render();
 })()`);
 await sleep(1000);
 check('failed message exposes Remove file',await browser.eval(`window.current.state==='failed'&&document.querySelector('#proof').textContent.includes('Remove file')`));
 check('held file offers Retry, not waiting or cancellation',await browser.eval(`document.querySelector('#proof').textContent.includes('Message held')&&document.querySelector('#proof').textContent.includes('Retry')&&!document.querySelector('#proof .att-x')`));
 await browser.size(390,844);await browser.shot(join(out,'held-file-phone.png'));
 check('Remove fits on phone',await browser.eval(`document.querySelector('#proof button.link').getBoundingClientRect().right<=390`));
 await browser.click('#proof button.link',80);
 check('Remove drops just the attachment',await browser.eval(`window.current.attachments.length===0&&window.current.body==='Keep this message'`));
 check('message requeues without retrying removed bytes',await browser.eval(`window.current.state==='queued'&&!document.querySelector('#proof .att-file')`));
 await browser.eval(`window.api.send({id:'file-only-proof',peer:'unlisted-proof.abcdef',body:'',attachments:[{name:'only.zip',bytes:3,source:'mock-source'}]})`);await sleep(1000);
 check('file-only removal rejects blank message',await browser.eval(`(async()=>{const m=await api.message('file-only-proof');try{await api.removeAttachment(m.id,m.attachments[0].local_id);return false;}catch{return (await api.message(m.id)).attachments.length===1;}})()`));
 writeFileSync(join(out,'result.json'),JSON.stringify({checks},null,2));
} finally {await browser.close();await server.close();}
