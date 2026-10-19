// Synthetic popup only. Run under a build-slot permit and kernel Job.
// node tools/proof-tray-scroll.mjs <owned-C-output> <existing-dependencies>
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
const [out,deps]=process.argv.slice(2);
if(process.env.HUBCHAT_JOB_CONTAINED!=='1'||!out||!deps)throw Error('Kernel Job, output and dependencies required');
fs.mkdirSync(out,{recursive:true});
const req=createRequire(path.resolve(deps,'../package.json'));
const source=`import {installRecent} from './src/trayrecent-view';
import {recentFixture} from './tools/tray-recent-fixture';
window.calls=[];window.popup=installRecent(document.body,async(command,args)=>window.calls.push({command,args}));
window.fixture=()=>{const v=recentFixture();v.recent=Array.from({length:16},(_,i)=>({...v.recent[i%6],peer:'peer'+i,name:'Recent chat '+String(i+1).padStart(2,'0'),typing:false,pictureKey:null}));return v;};
popup.update(fixture());`;
const bundle=await req('esbuild').build({stdin:{contents:source,resolveDir:process.cwd(),loader:'ts'},write:false,bundle:true,platform:'browser',format:'iife',nodePaths:[deps]});
await req('esbuild').stop();
fs.writeFileSync(out+'/menu.js',bundle.outputFiles[0].contents);
fs.copyFileSync('src/styles/traymenu.css',out+'/menu.css');
fs.writeFileSync(out+'/index.html','<!doctype html><meta charset="utf-8"><link rel="stylesheet" href="menu.css"><script defer src="menu.js"></script>');
const env={...process.env,HUBCHAT_TRAY_SCROLL_OUT:out};delete env.ELECTRON_RUN_AS_NODE;
const start=Date.now();
const p=spawnSync(req('electron'),[path.resolve('tools/tray-scroll-electron.cjs')],{env,encoding:'utf8',windowsHide:true,timeout:45000});
fs.writeFileSync(out+'/electron.log',(p.stdout||'')+(p.stderr||''));
fs.writeFileSync(out+'/electron-receipt.json',JSON.stringify({status:p.status,signal:p.signal,error:p.error?.message??null,ms:Date.now()-start},null,2));
console.log(p.stdout,p.stderr);process.exitCode=p.status??2;
