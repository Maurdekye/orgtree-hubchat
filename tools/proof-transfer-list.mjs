import {createServer} from 'vite';
import {launch} from './cdp.mjs';
import {mkdirSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
const out='D:/orgtree-work/rig/agy-astra/hubchat-transfer-list';mkdirSync(out,{recursive:true});
const server=await createServer({server:{host:'127.0.0.1',port:0}});await server.listen();
const b=await launch();const checks=[];const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const check=(name,ok)=>{checks.push({name,ok});console.log(`${ok?'PASS':'FAIL'} ${name}`);if(!ok)throw Error(name);};
try {
 await b.nav(server.resolvedUrls.local[0]);
 await b.eval(`(async()=>{
  const {api}=await import('/src/api.ts');window.api=api;window.oldTransfers=api.transfers;
  const s=await import('/src/lib/store.ts');window.s=s;
  window.rows=Array.from({length:205},(_,i)=>({local_id:String(i).padStart(4,'0'),message_id:'proof'+i,created_at:String(205-i).padStart(4,'0'),peer:'proof-peer',name:'File '+i,upload:false,state:'queued',done:0,total:1048576,error:null}));
  window.reads=0;api.transfers=async(after)=>{window.reads++;const rows=window.rows.filter(r=>!after||r.local_id>after);return {rows:rows.slice(0,100),next:rows.length>100?rows[99].local_id:null};};
  await s.refreshTransfers();
 })()`);
 await sleep(100);
 check('all durable pages loaded without transfer events',await b.eval(`window.reads===3&&document.querySelector('.tb-xfer .n').textContent==='205'`));
 await b.click('.tb-xfer',70);
 check('display follows message time rather than UUID order',await b.eval(`document.querySelector('.xrow b').textContent==='File 204'`));
 await b.click('.tb-xfer',70);
 await b.eval(`window.rows=[{...window.rows[0],state:'exporting',done:1048576}];window.s.refreshTransfers()`);await sleep(80);
 await b.click('.tb-xfer',70);
 check('100 percent exporting stays listed without Cancel',await b.eval(`document.querySelector('.xrow').textContent.includes('Finishing')&&!document.querySelector('.xrow button')`));
 await b.shot(join(out,'finishing-list.png'));
 await b.eval(`window.rows=[];window.s.refreshTransfers()`);await sleep(80);
 check('completed snapshot removes global entry',await b.eval(`!document.querySelector('.tb-xfer')`));
 await b.size(360,800);await b.nav(server.resolvedUrls.local[0]+'?platform=android');
 await b.eval(`(async()=>{
  const {api}=await import('/src/api.ts');const chats=await api.chats();let m,a;
  for(const c of chats){for(const x of await api.chat(c.peer)){const image=x.attachments.find(a=>/\\.(png|jpg)$/i.test(a.name));if(image){m=x;a=image;break;}}if(a)break;}
  if(!a)throw Error('no image fixture');
  a={...a,bytes:9437184,state:'downloading',source:null,local_path:null,download:{state:'running',confirmed:8388608,retry_at:0,error:null}};
  m={...m,outgoing:false,attachments:[a]};window.picture=m;
  const old=api.message;api.message=async id=>id===m.id?m:old(id);
  const {openImage}=await import('/src/lib/images.ts');openImage(m,a);
 })()`);await sleep(400);
 for(const theme of ['light','dark']){
  await b.eval(`(async()=>{const t=await import('/src/lib/theme.ts');t.setThemePref('${theme}');})()`);await sleep(350);
  check('phone viewer progress fits '+theme,await b.eval(`(()=>{const e=document.querySelector('.imgview .iv-progress');if(!e)return false;const r=e.getBoundingClientRect();return e.textContent.includes('Downloading')&&r.width>0&&r.left>=0&&r.right<=innerWidth&&document.documentElement.scrollWidth<=innerWidth;})()`));
  console.log(await b.eval(`(()=>{const e=document.querySelector('.imgview .iv-progress');return {text:e.textContent,color:getComputedStyle(e).color,fill:getComputedStyle(e).webkitTextFillColor,opacity:getComputedStyle(e).opacity,visibility:getComputedStyle(e).visibility,ancestors:Array.from(document.querySelector('.iv-progress').getClientRects()).map(x=>x.toJSON()),fonts:document.fonts.status,hit:document.elementFromPoint(100,456)?.outerHTML.slice(0,200)};})()`));
  await b.eval(`document.fonts.ready.then(()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r))))`);
  console.log(await b.eval(`({viewers:document.querySelectorAll('.imgview').length,cap:document.querySelector('.iv-cap').getBoundingClientRect().toJSON()})`));
  await b.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});
  await sleep(150);
  await b.shot(join(out,'viewer-'+theme+'.png')); 
 }
 writeFileSync(join(out,'result.json'),JSON.stringify({checks},null,2));
}finally{await b.close();await server.close();}
