// Mounted Android layout, mock core only. Run in the rig's FIFO/kernel Job.
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {pathToFileURL} from 'node:url';
const repo=process.cwd(),out=path.resolve(process.env.HUBCHAT_RENDERER_OUT||'');
if(!process.env.HUBCHAT_JOB_CONTAINED||!/^C:[\\/]orgtree-build[\\/]/i.test(out))throw Error('Requires contained C output');
const orgdeps=createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT,'package.json'));
const deps=process.env.HUBCHAT_DEPENDENCY_ROOT,esbuild=orgdeps('esbuild');
fs.mkdirSync(out,{recursive:true});
try{await esbuild.build({entryPoints:[path.join(repo,'tools/mobile-back-entry.tsx')],outfile:path.join(out,'entry.mjs'),bundle:true,
  format:'esm',platform:'browser',jsx:'automatic',loader:{'.md':'text','.css':'empty'},nodePaths:[path.join(deps,'node_modules')]});}finally{esbuild.stop();}
const {JSDOM}=orgdeps('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://scratch.invalid/?platform=android',pretendToBeVisual:true});
for(const k of ['window','document','navigator','history','HTMLElement','HTMLTextAreaElement','HTMLInputElement','localStorage','sessionStorage','location','Event','MouseEvent','KeyboardEvent','PopStateEvent','MutationObserver'])Object.defineProperty(globalThis,k,{value:dom.window[k],configurable:true});
globalThis.getComputedStyle=dom.window.getComputedStyle.bind(dom.window);
globalThis.requestAnimationFrame=f=>setTimeout(f,0);globalThis.cancelAnimationFrame=clearTimeout;
dom.window.HTMLElement.prototype.scrollIntoView=function(){};
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
const {Android,api,mockApi,store,PlatformCtx,act,createElement:h,createRoot,StrictMode}=await import(pathToFileURL(path.join(out,'entry.mjs')));
Object.assign(api,mockApi);
const checks=[],check=(name,ok)=>{checks.push({name,ok:!!ok});console.log(`${ok?'PASS':'FAIL'} ${name}`)};
const tick=()=>act(async()=>{await new Promise(r=>setTimeout(r,50))});
let root;
const click=async(el)=>{if(!el)throw Error('Missing control at '+title());await act(async()=>el.click());await tick()};
const button=(label)=>document.querySelector(`[aria-label="${label}"]`);
const title=()=>document.querySelector('.appbar .title')?.textContent||document.querySelector('.appbar .who')?.textContent||'';
const snapshot=()=>({title:title(),state:history.state,length:history.length,conversation:!!document.querySelector('.composer')});
const fresh=async()=>{
  if(root)await act(async()=>root.unmount());
  localStorage.clear();history.replaceState(null,'');
  root=createRoot(document.getElementById('root'));
  await act(async()=>root.render(h(StrictMode,null,h(PlatformCtx.Provider,{value:'android'},h(Android)))));
  await tick();await tick();
};
const back=async(mode)=>{
  console.log('BEFORE_BACK '+JSON.stringify(snapshot()));
  if(mode==='corner')await click(button('Back'));
  else if(mode==='fallback'){await act(async()=>{check('native fallback handles secondary screen',window.__hcBack());});await tick();}
  else {await act(async()=>history.back());await tick();}
  console.log('AFTER_BACK '+JSON.stringify(snapshot()));
};
try{
  await act(async()=>store.startStore());
  for(const mode of ['corner','fallback','webview']){
    for(const via of ['name','menu']){
      await fresh();await click(document.querySelector('.crow'));await tick();
      const who=document.querySelector('.appbar .who')?.textContent;
      check(mode+'/'+via+' opens conversation',!!who&&!!document.querySelector('.composer'));
      if(via==='menu'){await click(button('More'));await click([...document.querySelectorAll('[role="menuitem"]')].find(e=>e.textContent.includes('Contact info')));}
      else await click(document.querySelector('.appbar .who'));
      check(mode+'/'+via+' opens Contact info',title()==='Contact info');
      await back(mode);
      check(mode+'/'+via+' returns from Contact info to same conversation',!!document.querySelector('.composer')&&document.querySelector('.appbar .who')?.textContent===who);
    }
    for(const label of ['About','Help','Appearance','Privacy','Hubs','Devices','Notifications','Recovery words']){
      await fresh();await click(button('Settings'));
      await click([...document.querySelectorAll('.set-row,.li')].find(e=>e.querySelector('.t1')?.textContent===label));
      check(mode+'/'+label+' opens section',title()===label);
      await back(mode);
      check(mode+'/'+label+' returns to Settings',title()==='Settings');
    }
    for(const label of ['Directory','New chat','Settings']){
      await fresh();await click(button(label));check(mode+'/'+label+' opens screen',title()===label);
      await back(mode);check(mode+'/'+label+' returns to chats',!!button('New chat')&&!!button('Settings'));
    }
  }
}finally{
  if(root)await act(async()=>root.unmount());
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({checks,failed:checks.filter(c=>!c.ok).length},null,2));
  dom.window.close();
}
console.log(JSON.stringify({checks:checks.length,failed:checks.filter(c=>!c.ok).length}));
// Background singleton watchers have no stop API; this isolated proof owns them.
process.exit(checks.some(c=>!c.ok)?1:0);
