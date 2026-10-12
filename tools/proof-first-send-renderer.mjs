// Isolated DOM/IPC-shape fixture proof. Run ONLY through verify-first-send-renderer.ps1.
for(const k of ['HUBCHAT_DEPENDENCY_ROOT','HUBCHAT_DOM_DEPENDENCY_ROOT','HUBCHAT_RENDERER_OUT','HUBCHAT_RENDERER_EVIDENCE','HUBCHAT_JOB_CONTAINED']) if(!process.env[k]) throw Error('Use the contained verifier; missing '+k);
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {pathToFileURL} from 'node:url';
const repo=process.cwd();
const deps=createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT,'package.json'));
const orgdeps=createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT,'package.json'));
const esbuild=orgdeps('esbuild');
const out=process.env.HUBCHAT_RENDERER_OUT;fs.mkdirSync(out,{recursive:true});
await esbuild.build({entryPoints:[path.join(repo,'tools/first-send-renderer-entry.tsx')],outfile:path.join(out,'entry.mjs'),bundle:true,format:'esm',platform:'browser',loader:{'.md':'text'},jsx:'automatic',plugins:[{name:'existing-dependencies',setup(b){b.onResolve({filter:/^[^./][^:]*/},a=>({path:deps.resolve(a.path==='qrcode'?'qrcode/lib/browser.js':a.path)}));}}]});
esbuild.stop();
const {JSDOM}=orgdeps('jsdom');
const dom=new JSDOM('<div id="root"></div>',{url:'http://scratch.invalid'});
for(const key of ['window','document','navigator','HTMLElement','HTMLTextAreaElement','localStorage','location','getComputedStyle','Event','MouseEvent','MutationObserver']) Object.defineProperty(globalThis,key,{value:key==='getComputedStyle'?dom.window[key].bind(dom.window):dom.window[key],configurable:true});
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
globalThis.requestAnimationFrame=f=>setTimeout(f,0);
globalThis.cancelAnimationFrame=clearTimeout;
const {api,store,Conversation,act,createElement,createRoot}=await import(pathToFileURL(path.join(out,'entry.mjs')));
const fixture=process.env.HUBCHAT_FIRST_SEND_FIXTURE?JSON.parse(fs.readFileSync(process.env.HUBCHAT_FIRST_SEND_FIXTURE,'utf8')):null;
const peer='pat.abcdef',rows=[];let sendCalls=0,listener; const readRows=()=>fixture&&rows.length?fixture.visible:rows;
Object.assign(api,{state:async()=>({me:{address:'scratch.123456'},hubs:[],platform:'desktop'}),chats:async()=>rows.length?[{peer,last:rows.at(-1),unread:0}]:[],directory:async()=>[],transfers:async()=>({rows:[],next:null}),onEvent:async f=>{listener=f;return()=>{}},draft:async()=>'',setDraft:async()=>{},setupStatus:async()=>null,sendRoute:async()=>({pinned:null,automatic:null,next:null,hubs:[]}),loadOlder:async()=>({added:0,more:[],unreachable:[]}),chat:async(p,o={})=>readRows().filter(m=>m.peer===p&&(!o.before||m.created_at<o.before.created_at)&&(!o.from||m.created_at>=o.from.created_at)),send:async n=>{sendCalls++;if(fixture){rows.push({...fixture.persisted[0]});return;}rows.push({...n,outgoing:true,created_at:new Date().toISOString(),state:'queued',hub:null,attachments:[],error:null,seen:true,sent_at:null,received_at:null,fetched_at:null,delivered_at:null,read_at:null,kind:null});}});
await act(async()=>{await store.startStore();});
const root=createRoot(document.getElementById('root'));
await act(async()=>{root.render(createElement(Conversation,{peer,onBack:()=>{},onInfo:()=>{},onOpenAddr:()=>{},onContact:()=>{}}));});
await act(async()=>{await new Promise(r=>setTimeout(r,20));});
const checks=[];const check=(name,ok)=>{checks.push({name,ok});console.log(`${ok?'PASS':'FAIL'} ${name}`);};
check('empty desktop conversation',document.querySelectorAll('.msg').length===0&&!!document.querySelector('.chat-start'));
const ta=document.querySelector('textarea');
await act(async()=>{Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(ta,'first outgoing');ta.dispatchEvent(new Event('input',{bubbles:true}));});
await act(async()=>{document.querySelector('.sendbtn').click();await new Promise(r=>setTimeout(r,20));});
check('composer clears after accepted command',ta.value==='');
check('outbox has exactly one accepted send',sendCalls===1&&rows.length===1);
check('first queued bubble visible without native event or inbound',document.getElementById('m-'+rows[0]?.id)?.textContent.includes('first outgoing'));
await act(async()=>{listener({type:'chat',peer});await new Promise(r=>setTimeout(r,20));});
check('empty state removed',!document.querySelector('.chat-start'));
check('queued delivery remains truthful',!!document.querySelector('[aria-label="Waiting"]')||document.querySelector('.tick')?.getAttribute('aria-label')?.includes('Queued'));
await act(async()=>{root.unmount();});
fs.writeFileSync(process.env.HUBCHAT_RENDERER_EVIDENCE,JSON.stringify({checks},null,2));
dom.window.close();process.exit(process.env.HUBCHAT_PROOF_PHASE==='red' ? (checks.filter(c=>!c.ok).length===3?0:1) : (checks.every(c=>c.ok)?0:1));





