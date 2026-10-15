// Mounted Android layout, mock core only. Run in the rig's FIFO/kernel Job.
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {pathToFileURL} from 'node:url';
import {spawnSync} from 'node:child_process';
const repo=process.cwd(),out=path.resolve(process.env.HUBCHAT_RENDERER_OUT||'');
if(!process.env.HUBCHAT_JOB_CONTAINED||!/^C:[\\/]orgtree-build[\\/]/i.test(out))throw Error('Requires contained C output');
const orgdeps=createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT,'package.json'));
const deps=process.env.HUBCHAT_DEPENDENCY_ROOT,esbuild=orgdeps('esbuild'),base=process.env.HUBCHAT_MOBILE_BACK_BASE;
fs.mkdirSync(out,{recursive:true});
try{await esbuild.build({entryPoints:[path.join(repo,'tools/mobile-back-entry.tsx')],outfile:path.join(out,'entry.mjs'),bundle:true,
  format:'esm',platform:'browser',jsx:'automatic',loader:{'.md':'text','.css':'empty'},nodePaths:[path.join(deps,'node_modules')],
  plugins:[{name:'synthetic-restored-route-initializer',setup(build){
    build.onLoad({filter:/[/\\]src[/\\]lib[/\\]visibility\.ts$/},args=>{
      // Each fresh() models a new page boot. Reset only the module's startup
      // take list, which would otherwise remember notifications from an
      // earlier case in this one-process fixture and suppress last-chat restore.
      let text=fs.readFileSync(args.path,'utf8');
      if(base){const r=spawnSync('git',['show',base+':src/components/Android.tsx'],{cwd:repo,encoding:'utf8',windowsHide:true});if(r.status!==0)throw Error(r.stderr);text=r.stdout;}
      if(!text.includes('const startTakes:'))throw Error('Startup take reset seam no longer matches');
      return {contents:text+'\nglobalThis.__proofResetStartupTakes = () => { startTakes.length = 0; };',loader:'ts',resolveDir:path.dirname(args.path)};
    });
    build.onLoad({filter:/[/\\]src[/\\]components[/\\]Android\.tsx$/},args=>{
      const text=fs.readFileSync(args.path,'utf8');
      const initializer='useState<Scr[]>([{ s: "chats" }])';
      if(!text.includes(initializer))throw Error('Android initializer seam no longer matches');
      // v1.2.0 does NOT persist Contact info. This deliberately models the
      // coordinator's missing-history/restored-visible-route hypothesis;
      // it is not proof that a real cold start creates this state.
      return {contents:text.replace(initializer,'useState<Scr[]>(() => globalThis.__proofInitialStack ?? [{ s: "chats" }])'),loader:'tsx',resolveDir:path.dirname(args.path)};
    });
  }}]});}finally{esbuild.stop();}
const {JSDOM}=orgdeps('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://scratch.invalid/?platform=android',pretendToBeVisual:true});
for(const k of ['window','document','navigator','history','HTMLElement','HTMLTextAreaElement','HTMLInputElement','localStorage','sessionStorage','location','Event','MouseEvent','KeyboardEvent','PopStateEvent','MutationObserver'])Object.defineProperty(globalThis,k,{value:dom.window[k],configurable:true});
globalThis.getComputedStyle=dom.window.getComputedStyle.bind(dom.window);
globalThis.requestAnimationFrame=f=>setTimeout(f,0);globalThis.cancelAnimationFrame=clearTimeout;
dom.window.HTMLElement.prototype.scrollIntoView=function(){};
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
const {Android,api,mockApi,store,PlatformCtx,openHubHelp,act,createElement:h,createRoot,StrictMode}=await import(pathToFileURL(path.join(out,'entry.mjs')));
Object.assign(api,mockApi);
const org='orgtree.alex.7c41d2',readChat=api.chat;
api.chat=async(peer,...args)=>{
  const list=await readChat(peer,...args);
  if(peer!==org||!list.length)return list;
  return [{...list[0],attachments:[{local_id:'proof-file',name:'contact-evidence.txt',bytes:123,state:'done',error:null,local_path:null,hub_id:null,source:null,download:null}]},...list.slice(1)];
};
const checks=[],check=(name,ok)=>{checks.push({name,ok:!!ok});console.log(`${ok?'PASS':'FAIL'} ${name}`)};
const tick=()=>act(async()=>{await new Promise(r=>setTimeout(r,50))});
let root;
const click=async(el)=>{if(!el)throw Error('Missing control at '+title());await act(async()=>el.click());await tick()};
const button=(label)=>document.querySelector(`[aria-label="${label}"]`);
const title=()=>document.querySelector('.appbar .title')?.textContent||document.querySelector('.appbar .who')?.textContent||'';
const snapshot=()=>({title:title(),state:history.state,length:history.length,conversation:!!document.querySelector('.composer')});
const fresh=async(saved=null)=>{
  if(root)await act(async()=>root.unmount());
  globalThis.__proofResetStartupTakes();
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
    for(const edge of ['directory-person','directory-org','directory-agent','notification','chat-link','last-chat','last-chat-org','tap-org','rotation','resume','hub-status']){
      const peer=edge==='directory-org'||edge==='last-chat-org'||edge==='tap-org'?org:edge==='directory-agent'?'hubchat-ui.alex.a3f9c1':'maya.e71f2b';
      await fresh(edge.startsWith('last-chat')?peer:null);
      if(edge.startsWith('directory-')){
        await click(button('Directory'));
        await click([...document.querySelectorAll('.dir-row')].find(e=>e.querySelector('.t2')?.textContent===peer));
      }else if(edge==='notification'||edge==='chat-link'){
        await act(async()=>window.__hcIntent(edge==='notification'?{peer}:{link:'hubchat://chat?to='+peer}));await tick();await tick();
      }else if(!edge.startsWith('last-chat')){
        await click([...document.querySelectorAll('.crow')].find(e=>e.querySelector('.crow-name')?.textContent===(edge==='tap-org'?'Orgtree':'Maya Lin')));
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
      if(peer===org)check(mode+'/'+edge+' displays org Files section',document.body.textContent.includes('contact-evidence.txt'));
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
  // A valid route/history mirror whose traversal stalls: parent navigation
  // must work without a popstate event, and a late matching pop must not pop twice.
  for(const mode of ['corner','fallback']){
    for(const screen of ['Contact info','New chat','Directory','Settings','About']){
      await fresh();
      if(screen==='Contact info'){await click(document.querySelector('.crow'));await click(document.querySelector('.appbar .who'));}
      else if(screen==='About'){await click(button('Settings'));await click([...document.querySelectorAll('.li')].find(e=>e.querySelector('.t1')?.textContent==='About'));}
      else await click(button(screen));
      const d=history.state.hc,original=history.back.bind(history),parentLabel=screen==='About'?'Settings':screen==='Contact info'?'conversation':'chats';
      history.back=()=>{};
      try{
        await act(async()=>{if(mode==='corner')button('Back').click();else check('stalled/'+screen+' native route handled',window.__hcBack());});
        const arrived=()=>parentLabel==='Settings'?title()==='Settings':parentLabel==='conversation'?!!document.querySelector('.composer'):!!button('New chat')&&!!button('Settings');
        if(mode==='corner')check('stalled/'+screen+' corner navigates without awaiting history',arrived());
        await tick();
        check('stalled/'+mode+'/'+screen+' reaches explicit parent within frame',arrived()&&history.state?.hc===d-1);
        await act(async()=>window.dispatchEvent(new PopStateEvent('popstate',{state:{hc:d-1}})));
        check('stalled/'+mode+'/'+screen+' late pop keeps parent',arrived());
      }finally{history.back=original;}
    }
  }
  await fresh();await click(document.querySelector('.crow'));
  const menuDepth=history.state.hc;
  await click(button('More'));
  await act(async()=>check('native Back handles conversation menu',window.__hcBack()));await tick();
  check('native Back closes menu without leaving conversation',!document.querySelector('.sheet[role="menu"]')&&!!document.querySelector('.composer')&&history.state.hc===menuDepth);
  await act(async()=>openHubHelp('android'));await tick();
  check('hub help overlay has its own history entry',!!document.querySelector('.hubhelp-root [role="dialog"]')&&typeof history.state?.hubhelp==='number');
  await act(async()=>check('native Back handles hub help overlay',window.__hcBack()));await tick();await tick();
  check('native Back closes hub help without leaving conversation',!document.querySelector('.hubhelp-root [role="dialog"]')&&!!document.querySelector('.composer')&&history.state.hc===menuDepth);
  await fresh();await click(document.querySelector('.crow'));await click(document.querySelector('.appbar .who'));
  const stalledBack=history.back.bind(history);history.back=()=>{};
  try{
    await act(async()=>{window.__hcBack();button('Back').click();});await tick();
    check('newer corner Back cancels pending system fallback',!!document.querySelector('.composer')&&history.state.hc===2);
    await click(document.querySelector('.appbar .who'));
    await act(async()=>{window.__hcBack();root.unmount();});root=null;
    const before=JSON.stringify(history.state);await tick();
    check('unmount cancels pending system fallback',JSON.stringify(history.state)===before);
  }finally{history.back=stalledBack;}
  // Explicitly synthetic cold-restored Contact info with no traversable
  // history. A new native WebView with canGoBack()==false calls __hcBack.
  // A normal browser history.back() emits no popstate when nothing is under it.
  const originalBack=history.back.bind(history);
  try{
    for(const mode of ['corner','fallback','webview-empty']){
      for(const previous of ['conversation','none']){
        globalThis.__proofInitialStack=previous==='conversation'?[{s:'chats'},{s:'conv',p:org},{s:'info',p:org}]:[{s:'info',p:org}];
        await fresh();await tick();
        const prefix='synthetic-cold/'+mode+'/'+previous;
        check(prefix+' renders org Contact info with file',title()==='Contact info'&&document.body.textContent.includes('contact-evidence.txt'));
        check(prefix+' has no route predecessor in history state',history.state?.hc===1);
        let attempts=0;history.back=()=>{attempts++;};
        try{
          if(mode==='corner')await click(button('Back'));
          else{await act(async()=>check(prefix+' page handles native Back with empty history',window.__hcBack()));await tick();}
          check(prefix+' leaves Contact info for '+previous,previous==='conversation'?!!document.querySelector('.composer')&&document.querySelector('.appbar .who .ell')?.textContent==='Orgtree':!!button('New chat')&&!!button('Settings'));
          console.log('SYNTHETIC_COLD '+JSON.stringify({mode,previous,attempts,...snapshot()}));
        }finally{history.back=originalBack;}
      }
    }
  }finally{delete globalThis.__proofInitialStack;}
}finally{
  api.chat=readChat;
  if(root)await act(async()=>root.unmount());
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({base:base??null,checks,failed:checks.filter(c=>!c.ok).length},null,2));
  dom.window.close();
}
console.log(JSON.stringify({checks:checks.length,failed:checks.filter(c=>!c.ok).length}));
// Background singleton watchers have no stop API; this isolated proof owns them.
process.exit(checks.some(c=>!c.ok)?1:0);
