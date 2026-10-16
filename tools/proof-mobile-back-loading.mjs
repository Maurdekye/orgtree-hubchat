// Back to a conversation whose messages are still loading must settle.
// Measured on the user's phone (Hubchat 1.2.0 and Test 1.2.1-alpha.1,
// hubchat-opus 2026-10-16): Back from Contact info re-opens Conversation;
// before its messages loaded, useMessages handed out a new empty list on every
// render, the card-answers effect stored a new state each time, and the
// conversation re-rendered without end. On the phone that ran as microtasks,
// so the load never arrived and the page froze. Here api.chat is held open for
// the conversation across Back. A commit counter stops a runaway loop at LIMIT,
// so a red run fails instead of hanging.
// Mounted Android layout, mock core only. Run in the rig's FIFO/kernel Job:
//   HUBCHAT_JOB_CONTAINED=1 HUBCHAT_RENDERER_OUT=C:/orgtree-build/<scope>/<dir>
//   HUBCHAT_DOM_DEPENDENCY_ROOT=<esbuild+jsdom> HUBCHAT_DEPENDENCY_ROOT=<Hubchat deps>
//   node tools/proof-mobile-back-loading.mjs
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {pathToFileURL} from 'node:url';
const repo=process.cwd(),out=path.resolve(process.env.HUBCHAT_RENDERER_OUT||'');
if(!process.env.HUBCHAT_JOB_CONTAINED||!/^C:[\\/]orgtree-build[\\/]/i.test(out))throw Error('Requires contained C output');
const orgdeps=createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT,'package.json'));
const deps=process.env.HUBCHAT_DEPENDENCY_ROOT,esbuild=orgdeps('esbuild');
fs.mkdirSync(out,{recursive:true});
try{await esbuild.build({entryPoints:[path.join(repo,'tools/mobile-back-loading-entry.tsx')],outfile:path.join(out,'entry.mjs'),bundle:true,
  format:'esm',platform:'browser',jsx:'automatic',loader:{'.md':'text','.css':'empty'},nodePaths:[path.join(deps,'node_modules')],
  plugins:[{name:'startup-take-reset',setup(build){
    build.onLoad({filter:/[/\\]src[/\\]lib[/\\]visibility\.ts$/},args=>{
      // Each fresh() models a new page boot: reset only the module's startup take list.
      const text=fs.readFileSync(args.path,'utf8');
      if(!text.includes('const startTakes:'))throw Error('Startup take reset seam no longer matches');
      return {contents:text+'\nglobalThis.__proofResetStartupTakes = () => { startTakes.length = 0; };',loader:'ts',resolveDir:path.dirname(args.path)};
    });
  }}]});}finally{esbuild.stop();}
const {JSDOM}=orgdeps('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://scratch.invalid/?platform=android',pretendToBeVisual:true});
for(const k of ['window','document','navigator','history','HTMLElement','HTMLTextAreaElement','HTMLInputElement','localStorage','sessionStorage','location','Event','MouseEvent','KeyboardEvent','PopStateEvent','MutationObserver'])Object.defineProperty(globalThis,k,{value:dom.window[k],configurable:true});
globalThis.getComputedStyle=dom.window.getComputedStyle.bind(dom.window);
globalThis.requestAnimationFrame=f=>setTimeout(f,0);globalThis.cancelAnimationFrame=clearTimeout;
dom.window.HTMLElement.prototype.scrollIntoView=function(){};
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
const {Android,api,mockApi,store,PlatformCtx,act,createElement:h,createRoot,Profiler,StrictMode}=await import(pathToFileURL(path.join(out,'entry.mjs')));
Object.assign(api,mockApi);
const org='orgtree.alex.7c41d2',readChat=api.chat;
// the conversation's message load, held open while `holding` is set
let holding=false;const held=[];
api.chat=(peer,...args)=>holding&&peer===org?new Promise(resolve=>held.push(()=>resolve(readChat(peer,...args)))):readChat(peer,...args);
const checks=[],check=(name,ok,detail='')=>{checks.push({name,ok:!!ok,detail});console.log(`${ok?'PASS':'FAIL'} ${name}${detail?' — '+detail:''}`)};
// commits of the whole tree; past LIMIT the render loop is runaway: stop it
const LIMIT=400,SETTLED=60;
let commits=0,runaway=false;
const onRender=()=>{if(++commits>LIMIT&&!runaway){runaway=true;throw Error(`render loop: more than ${LIMIT} commits`);}};
const tick=()=>act(async()=>{await new Promise(r=>setTimeout(r,50))});
let root;
const click=async(el)=>{if(!el)throw Error('Missing control at '+title());await act(async()=>el.click());await tick()};
const button=(label)=>document.querySelector(`[aria-label="${label}"]`);
const title=()=>document.querySelector('.appbar .title')?.textContent||document.querySelector('.appbar .who')?.textContent||'';
const who=()=>document.querySelector('.appbar .who .ell')?.textContent;
const fresh=async()=>{
  if(root)await act(async()=>root.unmount());
  globalThis.__proofResetStartupTakes();
  localStorage.clear();history.replaceState(null,'');
  holding=false;held.splice(0).forEach(f=>f());
  commits=0;runaway=false;
  root=createRoot(document.getElementById('root'),{onUncaughtError:(e)=>console.log('UNCAUGHT '+(e?.message||e))});
  await act(async()=>root.render(h(StrictMode,null,h(PlatformCtx.Provider,{value:'android'},h(Profiler,{id:'android',onRender},h(Android))))));
  await tick();await tick();
};
try{
  await act(async()=>store.startStore());
  for(const mode of ['corner','fallback','webview']){
    await fresh();
    await click([...document.querySelectorAll('.crow')].find(e=>e.querySelector('.crow-name')?.textContent==='Orgtree'));
    check(mode+': the org conversation opens with its messages',who()==='Orgtree'&&!!document.querySelector('.composer')&&document.querySelectorAll('.msg').length>0);
    await click(document.querySelector('.appbar .who'));
    check(mode+': Contact info opens',title()==='Contact info'&&history.state?.hc===3);
    // Back re-opens the conversation while its messages are still loading
    holding=true;commits=0;
    if(mode==='corner')await click(button('Back'));
    else if(mode==='fallback'){await act(async()=>{window.__hcBack();});await tick();}
    else{await act(async()=>history.back());await tick();}
    await tick();await tick();
    const settled=commits;
    check(mode+': Back to a loading conversation settles',!runaway&&settled<=SETTLED,`${settled} commits${runaway?' (runaway, stopped at '+LIMIT+')':''}`);
    check(mode+': the conversation is back on top while its messages load',who()==='Orgtree'&&!!document.querySelector('.composer')&&history.state?.hc===2);
    holding=false;
    await act(async()=>{held.splice(0).forEach(f=>f());});await tick();await tick();
    check(mode+': its messages appear once they load',document.querySelectorAll('.msg').length>0);
    check(mode+': no further commits once loaded',!runaway&&commits<=settled+SETTLED,`${commits} commits in all`);
  }
}finally{
  api.chat=readChat;
  if(root)await act(async()=>root.unmount());
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({checks,failed:checks.filter(c=>!c.ok).length},null,2));
  dom.window.close();
}
console.log(JSON.stringify({checks:checks.length,failed:checks.filter(c=>!c.ok).length}));
// Background singleton watchers have no stop API; this isolated proof owns them.
process.exit(checks.some(c=>!c.ok)?1:0);
