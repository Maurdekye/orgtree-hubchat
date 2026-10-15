// Synthetic mounted UI only. Run in rig's shared slot/kernel Job; never starts
// Tauri, connects a hub or writes a real identity/profile.
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
import {pathToFileURL} from 'node:url';
const repo=process.cwd(),out=path.resolve(process.env.HUBCHAT_RENDERER_OUT||'');
if(!process.env.HUBCHAT_JOB_CONTAINED||!/^C:[\\/]orgtree-build[\\/]/i.test(out))throw Error('Requires contained C output');
const orgdeps=createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT,'package.json'));
const deps=process.env.HUBCHAT_DEPENDENCY_ROOT,esbuild=orgdeps('esbuild'),base=process.env.HUBCHAT_SHORTCUT_BASE;
fs.mkdirSync(out,{recursive:true});
try{await esbuild.build({entryPoints:[path.join(repo,'tools/desktop-shortcuts-entry.tsx')],outfile:path.join(out,'entry.mjs'),
  bundle:true,format:'esm',platform:'browser',jsx:'automatic',loader:{'.md':'text'},nodePaths:[path.join(deps,'node_modules')],
  plugins:[{name:'shortcut-base-and-native-reload-seam',setup(build){
    build.onLoad({filter:/[/\\]src[/\\](components[/\\](Desktop|TitleBar|Composer|Conversation)\.tsx|lib[/\\]refresh\.ts)$/},args=>{
      const file=path.relative(repo,args.path).replaceAll('\\','/');let text=fs.readFileSync(args.path,'utf8');
      if(base&&file.startsWith('src/components/')){const r=spawnSync('git',['show',base+':'+file],{cwd:repo,encoding:'utf8',windowsHide:true});if(r.status!==0)throw Error(r.stderr);text=r.stdout;}
      // Replace only the browser's reload boundary; test real snapshot/callback flow.
      text=text.replaceAll('window.location.reload()','globalThis.__proofReload()');
      return {contents:text,loader:file.endsWith('.tsx')?'tsx':'ts',resolveDir:path.dirname(args.path)};
    });
  }}]});}finally{esbuild.stop();}
const {JSDOM}=orgdeps('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://scratch.invalid/'});
for(const k of ['window','document','navigator','HTMLElement','HTMLTextAreaElement','HTMLInputElement','localStorage','sessionStorage','location','Event','MouseEvent','KeyboardEvent','MutationObserver'])Object.defineProperty(globalThis,k,{value:dom.window[k],configurable:true});
globalThis.getComputedStyle=dom.window.getComputedStyle.bind(dom.window);
globalThis.requestAnimationFrame=f=>setTimeout(f,0);globalThis.cancelAnimationFrame=clearTimeout;
dom.window.HTMLElement.prototype.scrollIntoView=function(){};
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
let reloads=0;globalThis.__proofReload=()=>{reloads++};
const {Desktop,TitleBar,api,mockApi,store,PlatformCtx,act,createElement:h,createRoot,StrictMode}=await import(pathToFileURL(path.join(out,'entry.mjs')));
Object.assign(api,mockApi);
const checks=[],check=(name,ok)=>{checks.push({name,ok:!!ok});console.log(`${ok?'PASS':'FAIL'} ${name}`)};
const tick=()=>act(async()=>{await new Promise(r=>setTimeout(r,25))});
let root=createRoot(document.getElementById('root'));
const mount=()=>act(async()=>{root.render(h(StrictMode,null,h(PlatformCtx.Provider,{value:'desktop'},h('div',null,h(TitleBar),h(Desktop)))))});
const press=async(key,opts={},target=document.body)=>{const event=new window.KeyboardEvent('keydown',{key,...opts,bubbles:true,cancelable:true});await act(async()=>{target.dispatchEvent(event)});await tick();return event};
const click=async(el)=>{if(el)await act(async()=>el.click());await tick()};
const button=(label)=>document.querySelector(`[aria-label="${label}"]`);
const close=async()=>{await click(document.querySelector('.modal-h [aria-label="Close"]'));};
try{
  await act(async()=>store.startStore());await mount();await tick();
  const refresh=()=>button('Refresh app view');
  check('Refresh button exists with tooltip',!!refresh()&&refresh().title==='Refresh (F5)');
  check('Refresh outside native control indices',!!refresh()&&!refresh().closest('.window-controls')&&!refresh().hasAttribute('data-tauri-drag-region'));
  await click(refresh());check('button calls reload once',reloads===1);
  for(const [key,title] of [['F1','Settings'],['F2','Directory'],['F3','New chat'],['F4','Hubs']]){
    const ev=await press(key);check(key+' suppresses WebView default',ev.defaultPrevented);
    check(key+' opens existing control destination',!!document.querySelector('[role="dialog"]')&&document.querySelector('[role="dialog"]').textContent.includes(title));
    await close();
  }
  const input=document.createElement('textarea');document.body.append(input);input.focus();
  let before=reloads;const f5=await press('F5',{},input);
  check('F5 in composer runs same reload once',reloads===before+1&&f5.defaultPrevented);
  before=reloads;const repeat=await press('F5',{repeat:true},input);
  check('held F5 is consumed without repeat',reloads===before&&repeat.defaultPrevented);
  for(const opts of [{ctrlKey:true},{shiftKey:true},{altKey:true},{metaKey:true}])check('modified F5 untouched '+JSON.stringify(opts),!(await press('F5',opts,input)).defaultPrevented);
  const modal=document.createElement('div');modal.setAttribute('role','dialog');modal.setAttribute('aria-modal','true');document.body.append(modal);
  before=reloads;
  for(const key of ['F1','F2','F3','F4','F5'])check('modal blocks '+key,(await press(key,{},input)).defaultPrevented&&reloads===before&&!document.querySelector('.modal'));
  const filterBefore=document.querySelector('.filters .on')?.textContent;
  check('modal blocks Alt+Right and native navigation',(await press('ArrowRight',{altKey:true},input)).defaultPrevented&&document.querySelector('.filters .on')?.textContent===filterBefore);
  modal.remove();
  const active=()=>document.querySelector('.filters .on')?.textContent.replace(/\d/g,'').trim();
  for(const [key,want] of [['ArrowRight','Agents'],['ArrowRight','People'],['ArrowRight','All'],['ArrowLeft','People'],['ArrowLeft','Agents'],['ArrowLeft','All']]){
    const ev=await press(key,{altKey:true},input);check('Alt+'+key+' wraps to '+want,ev.defaultPrevented&&active()===want);
  }
  for(const opts of [{},{ctrlKey:true},{altKey:true,ctrlKey:true}])check('editing arrows untouched '+JSON.stringify(opts),!(await press('ArrowLeft',opts,input)).defaultPrevented);
  input.remove();
  const row=document.querySelector('.crow');await click(row);await tick();
  const text=document.querySelector('.composer textarea');check('draft fixture opens real conversation',!!text);
  if(text){
    const timeline=document.querySelector('.timeline');
    Object.defineProperties(timeline,{scrollHeight:{configurable:true,value:2000},clientHeight:{configurable:true,value:400}});
    await act(async()=>{timeline.scrollTop=650;timeline.dispatchEvent(new window.Event('scroll',{bubbles:true}));});
    await act(async()=>{Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype,'value').set.call(text,'unsaved latest draft');text.dispatchEvent(new window.Event('input',{bubbles:true}));});
    await click(refresh());
    check('refresh snapshots latest draft before debounce',sessionStorage.getItem('hubchat.refresh-state')?.includes('unsaved latest draft'));
    await act(async()=>root.unmount());root=createRoot(document.getElementById('root'));await mount();await tick();
    check('open chat and latest draft survive remount',document.querySelector('.composer textarea')?.value==='unsaved latest draft');
    check('reading scroll position survives remount',document.querySelector('.timeline')?.scrollTop===650);
  }
  const css=fs.readFileSync(path.join(repo,'src/styles/app-desktop.css'),'utf8');
  check('Refresh has both no-drag properties',/\.titlebar \.tb-refresh\s*\{[^}]*-webkit-app-region:\s*no-drag;[^}]*\bapp-region:\s*no-drag;/s.test(css));
}finally{await act(async()=>root.unmount());dom.window.close();}
const failed=checks.filter(c=>!c.ok);
fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({passed:failed.length===0,base:base??null,checks},null,2));
await new Promise(r=>process.stdout.write(JSON.stringify({checks:checks.length,failed:failed.length})+'\n',r));
process.exit(failed.length?1:0);
