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
const fresh=async(saved=null)=>{
  if(root)await act(async()=>root.unmount());
  localStorage.clear();history.replaceState(null,'');
  if(saved)localStorage.setItem('hubchat.lastchat.'+store.getSnap().state.me.address,saved);
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
      await fresh();await click(button(label));check(mode+'/'+label+' opens screen',label==='Directory'?document.querySelector('.appbar .title')?.firstChild?.textContent===label:title()===label);
      await back(mode);check(mode+'/'+label+' returns to chats',!!button('New chat')&&!!button('Settings'));
    }
    for(const edge of ['directory-person','directory-org','directory-agent','notification','chat-link','last-chat','rotation','resume','hub-status']){
      const peer=edge==='directory-org'?'orgtree.alex.7c41d2':edge==='directory-agent'?'hubchat-ui.alex.a3f9c1':'maya.e71f2b';
      await fresh(edge==='last-chat'?peer:null);
      if(edge.startsWith('directory-')){
        await click(button('Directory'));
        await click([...document.querySelectorAll('.dir-row')].find(e=>e.querySelector('.t2')?.textContent===peer));
      }else if(edge==='notification'||edge==='chat-link'){
        await act(async()=>window.__hcIntent(edge==='notification'?{peer}:{link:'hubchat://chat?to='+peer}));await tick();await tick();
      }else if(edge!=='last-chat'){
        await click([...document.querySelectorAll('.crow')].find(e=>e.querySelector('.crow-name')?.textContent==='Maya Lin'));
      }
      await tick();
      const who=document.querySelector('.appbar .who .ell')?.textContent;
      check(mode+'/'+edge+' reaches requested conversation',!!who&&!!document.querySelector('.composer')&&history.state?.hc===2);
      const draft=mode+'/'+edge+' unsaved draft';
      const input=document.querySelector('.composer textarea');
      if(input)await act(async()=>{
        Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype,'value').set.call(input,draft);
        input.dispatchEvent(new Event('input',{bubbles:true}));
      });
      await click(document.querySelector('.appbar .who'));
      check(mode+'/'+edge+' opens Contact info at depth3',title()==='Contact info'&&history.state?.hc===3);
      if(edge==='rotation'){
        // JS viewport/orientation notifications only, not native Activity recreation.
        await act(async()=>{
          Object.defineProperty(window,'innerWidth',{configurable:true,value:844});
          Object.defineProperty(window,'innerHeight',{configurable:true,value:390});
          window.dispatchEvent(new Event('resize'));window.dispatchEvent(new Event('orientationchange'));
        });await tick();
      }else if(edge==='resume'){
        await act(async()=>{
          Object.defineProperty(document,'visibilityState',{configurable:true,value:'hidden'});
          document.dispatchEvent(new Event('visibilitychange'));window.dispatchEvent(new Event('blur'));
        });
        await act(async()=>{
          Object.defineProperty(document,'visibilityState',{configurable:true,value:'visible'});
          document.dispatchEvent(new Event('visibilitychange'));window.dispatchEvent(new Event('focus'));
        });await tick();
      }else if(edge==='hub-status'){
        // Synthetic mock-core reconnect/route state refresh; never connects a hub.
        const readState=api.state;
        api.state=async()=>{const s=await readState();return {...s,hubs:s.hubs.map(h=>({...h,state:h.state==='connected'?'disconnected':'connected'}))};};
        try{await act(async()=>store.refreshState());await tick();}finally{api.state=readState;}
      }
      check(mode+'/'+edge+' retains info and history before Back',title()==='Contact info'&&history.state?.hc===3);
      await back(mode);
      check(mode+'/'+edge+' returns to same conversation/depth2',!!document.querySelector('.composer')&&document.querySelector('.appbar .who .ell')?.textContent===who&&history.state?.hc===2);
      check(mode+'/'+edge+' retains composer draft',document.querySelector('.composer textarea')?.value===draft);
      if(edge==='hub-status'){await act(async()=>store.refreshState());await tick();}
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
