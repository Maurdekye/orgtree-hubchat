// Symmetry S4 (audit ux05 S5/S7, ux07 S5): Settings > Transfers history keeps its peer, loads the next page
// near the end of the list, and retries only the page that failed. Mounted real component, synthetic API,
// never a live hub. Run under the repository Job limiter; no dependency copies.
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {pathToFileURL} from 'node:url';
const repo=process.cwd(), out=process.env.HUBCHAT_RENDERER_OUT;
if(process.env.HUBCHAT_JOB_CONTAINED!=='1'||!/^C:[\\/]orgtree-build[\\/]/i.test(out||''))throw Error('Assigned C output and Job required');
const hub=createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT,'package.json'));
const deps=createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT,'package.json'));
fs.mkdirSync(out,{recursive:true});
const esbuild=deps('esbuild');
try {await esbuild.build({entryPoints:[repo+'/tools/symmetry-s4-transfers-entry.tsx'],outfile:out+'/entry.mjs',bundle:true,
  format:'esm',platform:'browser',jsx:'automatic',plugins:[{name:'existing-dependencies',setup(b){
    b.onResolve({filter:/^[^./][^:]*/},a=>({path:hub.resolve(a.path==='qrcode'?'qrcode/lib/browser.js':a.path)}));
  }}]});} finally {await esbuild.stop();}
const {JSDOM}=deps('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://scratch.invalid'});
for(const k of ['window','document','navigator','HTMLElement','localStorage','location','Event','MouseEvent','KeyboardEvent'])
  Object.defineProperty(globalThis,k,{value:dom.window[k],configurable:true});
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
// a controllable IntersectionObserver: the proof decides when the end-of-list marker comes into view
const observers=[];
globalThis.IntersectionObserver=class{constructor(cb){this.cb=cb;this.live=true;this.els=[];observers.push(this);}
  observe(el){this.els.push(el);}unobserve(){}disconnect(){this.live=false;}takeRecords(){return [];}};
const {api,TransfersPanel,PlatformCtx,Toasts,store,act,createElement:h,createRoot}=await import(pathToFileURL(out+'/entry.mjs'));
const root=createRoot(document.getElementById('root')),checks=[],opened=[];
const check=(name,ok)=>{checks.push({name,ok:!!ok});console.log((ok?'PASS ':'FAIL ')+name);};
const row=(id,extra={})=>({local_id:id,message_id:'m-'+id,created_at:'2026-10-17T00:00:00Z',peer:'test.peer',name:id+'.pdf',
  upload:false,state:'done',total:100,error:null,available:true,can_redownload:true,...extra});
let pages=[],failNext=0,failInitial=0,gate=null;
api.transfers=async()=>({rows:[],next:null});
api.transferHistory=async(after=null)=>{pages.push(after);
  if(gate)await gate;
  if(!after&&failInitial>0){failInitial--;throw 'history read failed';}
  if(after&&failNext>0){failNext--;throw 'next page failed';}
  if(after==='p2')return {rows:[row('older-a'),row('older-b')],next:'p3'};
  if(after==='p3')return {rows:[row('oldest')],next:null};
  return {rows:[row('incoming'),row('outgoing',{upload:true,can_redownload:false})],next:'p2'};};
api.openAttachment=async()=>{};api.download=async()=>{};api.cancelTransfer=async()=>{};
const click=async el=>{if(!el||el.disabled)return false;await act(async()=>{el.click();await Promise.resolve();});return true;};
const settle=async()=>{for(let i=0;i<4;i++)await act(async()=>{await Promise.resolve();});};
const item=id=>document.querySelector('[data-transfer="'+id+'"]');
const btn=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
const sees=el=>act(async()=>{for(const o of observers.filter(o=>o.live))if(o.els.includes(el))o.cb([{isIntersecting:true,target:el}]);});
const mount=async platform=>{pages=[];opened.length=0;observers.length=0;
  await act(async()=>root.render(null));
  await act(async()=>{root.render(h(PlatformCtx.Provider,{value:platform},h(TransfersPanel,{onOpen:p=>opened.push(p)}),h(Toasts)));});
  await settle();};
try {
 for(const platform of ['desktop','android']) {
  const c=(name,ok)=>check(platform+': '+name,ok);
  // ---- #14: a finished transfer keeps the peer an active one shows
  failNext=failInitial=0;gate=null;await mount(platform);
  c('a received row says who it came from',/Received from test\.peer/.test(item('incoming')?.textContent||''));
  c('a sent row says who it went to',/Sent to test\.peer/.test(item('outgoing')?.textContent||''));
  await click(item('incoming')?.querySelector('.xpeer'));
  c('the peer name opens that chat',JSON.stringify(opened)==='["test.peer"]');
  opened.length=0;await click([...item('incoming').querySelectorAll('button')].find(b=>b.textContent.trim()==='View'));
  c('View stays independent: it does not open the chat',opened.length===0);
  // ---- #15: the next page loads when the end of the list comes into view; the button stays as the manual path
  c('the end of the list is observed while another page exists',observers.some(o=>o.live&&o.els.length===1));
  c('Load more stays as the manual path',!!btn('Load more'));
  const end=document.querySelector('.xhistory-end');
  c('there is an end-of-list marker to observe',!!end);
  pages=[];await sees(end);await settle();
  c('reaching the end loads the next page without a click',JSON.stringify(pages)==='["p2"]'&&!!item('older-a')&&!!item('older-b'));
  c('earlier rows stay when the next page arrives',!!item('incoming')&&!!item('outgoing'));
  // a second sighting while a page is in flight must not double-request
  let release;gate=new Promise(r=>release=r);pages=[];
  await sees(document.querySelector('.xhistory-end'));await sees(document.querySelector('.xhistory-end'));
  release();gate=null;await settle();
  c('one page in flight at a time',pages.filter(p=>p==='p3').length===1&&!!item('oldest'));
  c('nothing is observed once the last page is in',!observers.some(o=>o.live&&o.els.length));
  // ---- #45: Retry repeats the failed request only
  failNext=1;await mount(platform);pages=[];
  await click(btn('Load more'));
  c('a failed later page keeps the rows already loaded',!!item('incoming')&&!!item('outgoing')&&/next page failed/.test(document.body.textContent));
  c('no automatic loading while a page error stands',!observers.some(o=>o.live&&o.els.length));
  pages=[];await click(btn('Retry'));await settle();
  c('Retry asks again for the failed page, not the first',JSON.stringify(pages)==='["p2"]');
  c('the retried page lands after the rows kept',!!item('older-a')&&!!item('incoming')&&!/next page failed/.test(document.body.textContent));
  failInitial=1;await mount(platform);
  c('a failed first read shows the error and no rows',/history read failed/.test(document.body.textContent)&&!item('incoming'));
  pages=[];await click(btn('Retry'));await settle();
  c('Retry of the first read asks for the first page',JSON.stringify(pages)==='[null]'&&!!item('incoming'));
  await act(async()=>root.render(null));
 }
} finally {await act(async()=>root.unmount());dom.window.close();}
fs.writeFileSync(out+'/result.json',JSON.stringify({passed:checks.every(c=>c.ok),checks,
 scope:'Mounted desktop/Android Settings transfer history with synthetic native API and a driven IntersectionObserver; no native, hub or pixel acceptance'},null,2));
process.exit(checks.every(c=>c.ok)?0:1);
