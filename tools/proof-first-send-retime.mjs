// Focused synthetic renderer test and one combined desktop/Android capture.
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
const repo=process.cwd(),out=process.env.HUBCHAT_NEWCHAT_OUT;
if(process.env.HUBCHAT_JOB_CONTAINED!=='1'||!/^C:[\\/]orgtree-build[\\/]hubchat-dm-sol[\\/]/i.test(out||''))throw Error('Own output and kernel Job required');
const deps=createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT,'package.json'));
const dom=createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT,'package.json'));
fs.mkdirSync(out,{recursive:true});
const fixture=JSON.parse(fs.readFileSync(process.env.HUBCHAT_RETIMED_SEND_FIXTURE,'utf8'));
try {await dom('esbuild').build({entryPoints:[repo+'/tools/first-send-retime-entry.tsx'],outfile:out+'/entry.js',bundle:true,platform:'browser',format:'iife',jsx:'automatic',loader:{'.md':'text'},plugins:[{name:'existing-dependencies',setup(b){b.onResolve({filter:/^[^./][^:]*/},a=>({path:deps.resolve(a.path==='qrcode'?'qrcode/lib/browser.js':a.path)}));}}]});}finally{dom('esbuild').stop();}
for(const platform of ['desktop','android']){
 const names=['tokens.css','base.css','app.css',...(platform==='desktop'?['desktop.css','app-desktop.css']:['mobile.css','app-mobile.css'])];
 fs.writeFileSync(out+'/'+platform+'.css',names.map(n=>fs.readFileSync(repo+'/src/styles/'+n,'utf8')).join('\n'));
 fs.writeFileSync(out+'/'+platform+'.html',`<!doctype html><html class="theme-dark" data-platform="${platform}"><head><link rel="stylesheet" href="${platform}.css"></head><body><div id="root" style="height:100vh;display:flex;flex-direction:column"></div><script>window.addEventListener('error',e=>window.firstSendError=e.message);window.firstSendFixture=${JSON.stringify(fixture).replaceAll('<','\\u003c')};</script><script src="entry.js"></script></body></html>`);
}
fs.writeFileSync(out+'/capture.html','<!doctype html><html><body style="margin:0;background:#181818;color:#eee;font:14px sans-serif;display:flex"><section><h3 style="margin:12px">Desktop</h3><iframe style="width:470px;height:570px;border:0" src="desktop.html?platform=desktop"></iframe></section><section><h3 style="margin:12px">Android layout</h3><iframe style="width:430px;height:570px;border:0" src="android.html?platform=android"></iframe></section></body></html>');
const env={...process.env};delete env.ELECTRON_RUN_AS_NODE;
const p=spawnSync(dom('electron'),[repo+'/tools/first-send-retime-electron.cjs'],{env,encoding:'utf8',windowsHide:true,timeout:45000,maxBuffer:2e6});
fs.writeFileSync(out+'/electron.log',(p.stdout||'')+(p.stderr||'')+(p.error?String(p.error):''));
console.log(JSON.stringify({status:p.status,signal:p.signal,error:p.error?.message||null}));console.log((p.stdout+p.stderr).slice(-3000));
process.exitCode=p.status??2;
