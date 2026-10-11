import {createServer} from 'vite';
import {launch} from './cdp.mjs';
import {mkdirSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
const out='D:/orgtree-work/rig/agy-astra/hubchat-file-background';mkdirSync(out,{recursive:true});
const server=await createServer({server:{host:'127.0.0.1',port:0}});await server.listen();
const b=await launch();const checks=[];const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const check=(name,ok)=>{checks.push({name,ok});console.log(`${ok?'PASS':'FAIL'} ${name}`);if(!ok)throw Error(name);};
try {
 await b.size(360,800);await b.nav(server.resolvedUrls.local[0]+'?platform=android');
 await b.eval(`(async()=>{const {api}=await import('/src/api.ts');const s=await import('/src/lib/store.ts');window.api=api;window.s=s;window.st=await api.state();api.state=async()=>({...window.st,file_background_hint:window.hint??null});})()`);
 check('no banner without paused background work',await b.eval(`!document.querySelector('[aria-label="File background notice"]')`));
 for(const theme of ['light','dark']){
  await b.eval(`(async()=>{window.hint='Android could not keep files running in the background. Keep Hubchat open to continue.';const t=await import('/src/lib/theme.ts');t.setThemePref('${theme}');await window.s.refreshState();})()`);await sleep(200);
  check('plain instructions fit phone in '+theme,await b.eval(`(()=>{const e=document.querySelector('[aria-label="File background notice"]');const r=e?.getBoundingClientRect();return e?.textContent.includes('Keep Hubchat open')&&r.width>0&&r.left>=0&&r.right<=innerWidth&&document.documentElement.scrollWidth<=innerWidth;})()`));
  await b.shot(join(out,theme+'.png'));
 }
 await b.eval(`window.hint=null;window.s.refreshState()`);await sleep(100);
 check('coverage recovery removes banner',await b.eval(`!document.querySelector('[aria-label="File background notice"]')`));
 writeFileSync(join(out,'result.json'),JSON.stringify({checks},null,2));
}finally{await b.close();await server.close();}
