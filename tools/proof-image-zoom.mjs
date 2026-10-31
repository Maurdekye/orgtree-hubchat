// The image viewer's zoom (USER 2026-10-31 15:03:04Z md50218422de7: "when i zoom in on an image and close the
// image, the app stays zoomed in"). Mounted viewer in jsdom, mock core only. Run in the rig's kernel Job:
//   HUBCHAT_JOB_CONTAINED=1 HUBCHAT_RENDERER_OUT=C:\orgtree-build\...\out HUBCHAT_DOM_DEPENDENCY_ROOT=<dir with jsdom+esbuild>
//   HUBCHAT_DEPENDENCY_ROOT=<dir with node_modules for react> node tools/proof-image-zoom.mjs
// jsdom has no pinch-zoom of its own, so what it proves is the viewer's part: the overlay takes the touches
// (touch-action: none), a pinch moves the PICTURE (transform) and never the page, the end of a gesture is not a tap
// that closes the viewer, and opening and closing pin the viewport meta to scale 1 for a moment, then restore it.
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {pathToFileURL} from 'node:url';
const repo=process.cwd(),out=path.resolve(process.env.HUBCHAT_RENDERER_OUT||'');
if(!process.env.HUBCHAT_JOB_CONTAINED||!/^C:[\\/]orgtree-build[\\/]/i.test(out))throw Error('Requires contained C output');
const orgdeps=createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT,'package.json'));
const deps=process.env.HUBCHAT_DEPENDENCY_ROOT,esbuild=orgdeps('esbuild');
fs.mkdirSync(out,{recursive:true});
try{await esbuild.build({entryPoints:[path.join(repo,'tools/image-zoom-entry.tsx')],outfile:path.join(out,'entry.mjs'),bundle:true,
  format:'esm',platform:'browser',jsx:'automatic',loader:{'.md':'text','.css':'empty'},nodePaths:[path.join(deps,'node_modules')]});}finally{esbuild.stop();}
const {JSDOM}=orgdeps('jsdom');
const META='width=device-width, initial-scale=1.0, viewport-fit=cover';
const dom=new JSDOM(`<head><meta name="viewport" content="${META}"></head><div id="root"></div>`,{url:'http://scratch.invalid/?platform=android',pretendToBeVisual:true});
for(const k of ['window','document','navigator','history','HTMLElement','Event','MouseEvent','Node','MutationObserver'])
  try{Object.defineProperty(globalThis,k,{value:dom.window[k],configurable:true,writable:true})}catch{}
globalThis.requestAnimationFrame=f=>setTimeout(f,0);globalThis.cancelAnimationFrame=clearTimeout;
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
globalThis.URL.createObjectURL=()=>'blob:scratch/x';
let captured=0;
const P=dom.window.HTMLElement.prototype;
P.setPointerCapture=function(){captured++};
Object.defineProperty(P,'offsetWidth',{configurable:true,get(){return this.tagName==='IMG'?300:0}});
Object.defineProperty(P,'offsetHeight',{configurable:true,get(){return this.tagName==='IMG'?200:0}});
P.getBoundingClientRect=function(){return this.tagName==='IMG'?{left:100,top:100,width:300,height:200,right:400,bottom:300,x:100,y:100}:{left:0,top:0,width:0,height:0,right:0,bottom:0,x:0,y:0}};
const {ImageViewer,openImage,closeImage,api,PlatformCtx,act,createElement:h,createRoot,StrictMode}=await import(pathToFileURL(path.join(out,'entry.mjs')));
api.attachmentPreview=async()=>new ArrayBuffer(8);
api.message=async()=>null;
const m={id:'m1',peer:'p',outgoing:false,hub:null,body:'',kind:null,reply_to:null,sent_at:null,received_at:null,created_at:'2026-10-31T00:00:00Z',state:'delivered',attachments:[]};
const a={local_id:'a1',hub_id:null,name:'pic.png',bytes:8,source:null,local_path:null,state:'done',error:null};
const checks=[],check=(name,ok)=>{checks.push({name,ok:!!ok});console.log(`${ok?'PASS':'FAIL'} ${name}`)};
const tick=()=>act(async()=>{await new Promise(r=>setTimeout(r,30))});
const meta=()=>document.querySelector('meta[name=viewport]');
const pointer=(type,el,id,x,y)=>{const e=new dom.window.MouseEvent(type,{bubbles:true,cancelable:true,clientX:x,clientY:y});Object.defineProperty(e,'pointerId',{value:id});return act(async()=>{el.dispatchEvent(e)})};
const scaleOf=()=>{const t=document.querySelector('.imgview img')?.style.transform||'';const s=/scale\(([\d.]+)\)/.exec(t);return s?+s[1]:null};
const css=fs.readFileSync(path.join(repo,'src/styles/images.css'),'utf8');
let root;
for(const platform of ['desktop','android']){
  const metaLog=[];const mo=new dom.window.MutationObserver(()=>metaLog.push(meta().getAttribute('content')));
  mo.observe(meta(),{attributes:true});
  root=createRoot(document.getElementById('root'));
  await act(async()=>root.render(h(StrictMode,null,h(PlatformCtx.Provider,{value:platform},h(ImageViewer)))));
  await act(async()=>openImage(m,a));await tick();await tick();
  const ov=document.querySelector('.imgview'),img=document.querySelector('.imgview img');
  check(platform+': the viewer is open',!!ov&&!!img);
  check(platform+': the controls stay above a zoomed picture (z-index on the close icon and caption)',/\.iv-close\s*\{[^}]*z-index:\s*2/.test(css)&&/\.iv-cap\s*\{[^}]*z-index:\s*2/.test(css));
  check(platform+': the overlay takes the touches (touch-action: none in images.css)',/\.imgview\s*\{[^}]*touch-action:\s*none/.test(css));
  check(platform+': opening pins the page scale to 1 for a moment',metaLog.some(c=>/maximum-scale=1/.test(c)));
  await tick();await tick();
  check(platform+': ...and the viewport meta is back as it was',meta().getAttribute('content')===META);
  if(ov){
    await pointer('pointerdown',ov,1,200,200);await pointer('pointerdown',ov,2,240,200);
    await pointer('pointermove',ov,1,160,200);await pointer('pointermove',ov,2,280,200);
    check(platform+': a pinch zooms the PICTURE (scale '+scaleOf()+')',scaleOf()>2.5);
    check(platform+': no pointer capture (it would retarget the picture own clicks to the overlay)',captured===0);
    await pointer('pointerup',ov,1,160,200);await pointer('pointerup',ov,2,280,200);
    await act(async()=>ov.dispatchEvent(new dom.window.MouseEvent('click',{bubbles:true})));await tick();
    check(platform+': the end of a pinch does not close the viewer',!!document.querySelector('.imgview'));
    check(platform+': the page itself is untouched by the pinch',meta().getAttribute('content')===META);
  }
  metaLog.length=0;
  const ov2=document.querySelector('.imgview');
  if(ov2)await act(async()=>ov2.dispatchEvent(new dom.window.MouseEvent('click',{bubbles:true})));
  await tick();
  if(platform==='android')await act(async()=>{history.back();await new Promise(r=>setTimeout(r,60))});
  await act(async()=>closeImage());await tick();await tick();await tick();
  check(platform+': a tap closes the viewer',!document.querySelector('.imgview'));
  check(platform+': closing pins the page scale to 1 ('+metaLog.length+' meta writes)',metaLog.some(c=>/minimum-scale=1, maximum-scale=1/.test(c)));
  check(platform+': ...and leaves the viewport meta as it was',meta().getAttribute('content')===META);
  mo.disconnect();await act(async()=>root.unmount());
}
const bad=checks.filter(c=>!c.ok);
console.log(`${checks.length-bad.length}/${checks.length} checks passed`);
process.exit(bad.length?1:0);
