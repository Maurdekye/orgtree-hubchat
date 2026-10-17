// Mounted real transfer components; synthetic API/files only, never a live hub.
// Run in the shared slot under the repository Job limiter. No dependency copies.
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {pathToFileURL} from 'node:url';
const repo=process.cwd(), out=process.env.HUBCHAT_RENDERER_OUT;
if(process.env.HUBCHAT_JOB_CONTAINED!=='1'||!/^C:[\\/]orgtree-build[\\/]hubchat-dm-sol[\\/]/i.test(out||''))throw Error('Own C output and Job required');
const hub=createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT,'package.json'));
const deps=createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT,'package.json'));
fs.mkdirSync(out,{recursive:true});
const esbuild=deps('esbuild');
try {await esbuild.build({entryPoints:[repo+'/tools/transfer-history-entry.tsx'],outfile:out+'/entry.mjs',bundle:true,
  format:'esm',platform:'browser',jsx:'automatic',plugins:[{name:'existing-dependencies',setup(b){
    b.onResolve({filter:/^[^./][^:]*/},a=>({path:hub.resolve(a.path==='qrcode'?'qrcode/lib/browser.js':a.path)}));
  }}]});} finally {await esbuild.stop();}
const {JSDOM}=deps('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://scratch.invalid'});
for(const k of ['window','document','navigator','HTMLElement','localStorage','location','Event','MouseEvent','KeyboardEvent'])
  Object.defineProperty(globalThis,k,{value:dom.window[k],configurable:true});
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
const {api,TransfersChip,TransfersStrip,PlatformCtx,Toasts,store,act,createElement:h,createRoot}=await import(pathToFileURL(out+'/entry.mjs'));
const root=createRoot(document.getElementById('root')),checks=[],calls=[],opened=[];
const check=(name,ok)=>{checks.push({name,ok:!!ok});console.log((ok?'PASS ':'FAIL ')+name);};
const row=(id,extra={})=>({local_id:id,message_id:'m-'+id,created_at:'2026-10-17T00:00:00Z',peer:'test.peer',name:id+'.pdf',
  upload:false,state:'done',total:100,error:null,available:true,can_redownload:true,...extra});
let active=[],pages=[],reject=false,pending=null;
api.transfers=async()=>({rows:active,next:null});
api.transferHistory=async(after=null)=>{pages.push(after);return {rows:after?[row('older')]:[
  row('incoming'),row('missing',{available:false}),row('outgoing',{upload:true,can_redownload:false}),
  row('outgoing-missing',{upload:true,available:false,can_redownload:false}),
  row('expired',{available:false,can_redownload:false,state:'expired',error:'File no longer available on the hub.'}),
],next:after?null:'older-page'};};
api.openAttachment=async(...args)=>{calls.push(['view',...args]);};
api.download=async(...args)=>{calls.push(['download',...args]);if(reject)throw 'File no longer available on the hub.';if(pending)await pending;};
api.cancelTransfer=async id=>{calls.push(['cancel',id]);};
const click=async el=>{if(!el||el.disabled)return false;await act(async()=>{el.click();await Promise.resolve();});return true;};
const item=id=>document.querySelector('[data-transfer="'+id+'"]');
const button=(id,label)=>[...(item(id)?.querySelectorAll('button')||[])].find(b=>b.textContent.trim()===label);
try {
 for(const platform of ['desktop','android']) {
  const checkP=(name,ok)=>check(platform+': '+name,ok);
  const Component=platform==='desktop'?TransfersChip:TransfersStrip;
  const render=()=>act(async()=>{root.render(h(PlatformCtx.Provider,{value:platform},h(Component,{onOpen:p=>opened.push(p)}),h(Toasts)));});
  active=[];pages=[];await store.refreshTransfers();await render();
  const trigger=()=>document.querySelector(platform==='desktop'?'.tb-xfer':'.xstrip .strip');
  checkP('transfers entry remains accessible after active work finishes',!!trigger());
  active=[{local_id:'active',message_id:'active-message',created_at:'now',peer:'test.peer',name:'busy.pdf',upload:false,state:'queued',done:0,total:100,error:null}];
  await act(async()=>{await store.refreshTransfers();});
  await click(trigger());
  checkP('unfinished transfer still offers Cancel',!!document.querySelector('.xrow:not(.xhistory-row) button'));
  checkP('finished incoming and outgoing rows are listed',!!item('incoming')&&!!item('outgoing'));
  calls.length=0;opened.length=0;
  await click(button('incoming','View'));
  checkP('incoming View opens the bound attachment',JSON.stringify(calls)==='[["view","m-incoming","incoming",false]]');
  checkP('missing local View is disabled with a visible reason',button('missing','View')?.disabled===true&&/local file.*(missing|available)/i.test(item('missing')?.textContent||''));
  calls.length=0;await click(button('outgoing','View'));
  checkP('outgoing View uses the bound local source',JSON.stringify(calls)==='[["view","m-outgoing","outgoing",false]]');
  checkP('outgoing rows do not offer Redownload',!!item('outgoing')&&!button('outgoing','Redownload'));
  checkP('missing outgoing View is disabled',button('outgoing-missing','View')?.disabled===true);
  calls.length=0;await click(button('missing','Redownload'));
  checkP('missing incoming Redownload explicitly queues a fresh copy',JSON.stringify(calls)==='[["download","m-missing","missing",true]]');
  calls.length=0;await click(button('incoming','Redownload'));
  checkP('existing incoming copy can be saved again',JSON.stringify(calls)==='[["download","m-incoming","incoming",true]]');
  checkP('expired blob has a visible no-longer-available state',/no longer available/i.test(item('expired')?.textContent||''));
  checkP('known unavailable blob has no enabled Redownload',!!item('expired')&&(!button('expired','Redownload')||button('expired','Redownload').disabled));
  reject=true;await click(button('incoming','Redownload'));reject=false;
  checkP('a download rejection is visibly reported',/no longer available/i.test(document.querySelector('.snacks')?.textContent||document.body.textContent)&&!!item('incoming'));
  let resolve;pending=new Promise(r=>resolve=r);calls.length=0;
  await click(button('incoming','Redownload'));
  await click(button('incoming','Redownload'));resolve();pending=null;
  await act(async()=>{await Promise.resolve();});
  checkP('a pending action cannot be queued twice',calls.filter(c=>c[0]==='download').length===1);
  const more=[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='Load more');
  await click(more);
  checkP('older finished transfers use bounded history pagination',!!item('older')&&pages.includes('older-page'));
  checkP('attachment actions do not navigate the chat',opened.length===0);
  await act(async()=>root.render(null));
 }
} finally {await act(async()=>root.unmount());dom.window.close();}
fs.writeFileSync(out+'/result.json',JSON.stringify({passed:checks.every(c=>c.ok),checks,
 scope:'Mounted desktop/Android transfer components with synthetic native API; no native file, hub, device or pixel acceptance'},null,2));
process.exit(checks.every(c=>c.ok)?0:1);
