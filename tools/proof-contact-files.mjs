// Focused mounted desktop/Android check; no real data, opener or network.
import fs from 'node:fs';import path from 'node:path';import{createRequire}from'node:module';import{spawnSync}from'node:child_process';
const repo=process.cwd(),out=path.resolve(process.env.HUBCHAT_RENDERER_OUT||'');
if(process.env.HUBCHAT_JOB_CONTAINED!=='1'||!/^C:[\\/]orgtree-build[\\/]/i.test(out))throw Error('Assigned C output and kernel Job required');
const deps=createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT,'package.json')),dom=createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT,'package.json'));
fs.mkdirSync(out,{recursive:true});
fs.writeFileSync(out+'/result.json',JSON.stringify({passed:false,status:'not executed'}));
try{await dom('esbuild').build({entryPoints:[repo+'/tools/contact-files-entry.tsx'],outfile:out+'/entry.js',bundle:true,platform:'browser',format:'iife',jsx:'automatic',loader:{'.md':'text','.svg':'text','.png':'dataurl'},plugins:[{name:'existing-dependencies',setup(b){b.onResolve({filter:/^[^./][^:]*/},a=>({path:deps.resolve(a.path==='qrcode'?'qrcode/lib/browser.js':a.path)}));}}]});}finally{dom('esbuild').stop();}
for(const platform of ['desktop','android']){
 const names=['tokens.css','base.css','app.css',...(platform==='desktop'?['desktop.css','app-desktop.css']:['mobile.css','app-mobile.css'])];
 fs.writeFileSync(out+'/'+platform+'.css',names.map(n=>fs.readFileSync(repo+'/src/styles/'+n,'utf8')).join('\n'));
 fs.writeFileSync(out+'/'+platform+'.html',`<!doctype html><html class="theme-dark" data-platform="${platform}"><head><link rel="stylesheet" href="${platform}.css"></head><body><div id="root" style="height:100vh"></div><script>window.addEventListener('error',e=>window.contactFilesError=e.message);</script><script src="entry.js"></script></body></html>`);
}
fs.writeFileSync(out+'/capture.html','<!doctype html><html><body style="margin:0;background:#181818;color:#eee;font:14px sans-serif;display:flex"><section style="width:640px;overflow:hidden"><h3 style="margin:8px">Desktop (conversation and Contact info)</h3><iframe style="width:1000px;height:620px;border:0;margin-left:-360px" src="desktop.html?platform=desktop"></iframe></section><section><h3 style="margin:8px">Android</h3><iframe style="width:350px;height:620px;border:0" src="android.html?platform=android"></iframe></section></body></html>');
const env={...process.env};delete env.ELECTRON_RUN_AS_NODE;
const p=spawnSync(dom('electron'),[repo+'/tools/contact-files-electron.cjs'],{env,encoding:'utf8',windowsHide:true,timeout:45000,maxBuffer:2e6});
fs.writeFileSync(out+'/electron.log',(p.stdout||'')+(p.stderr||'')+(p.error?String(p.error):''));console.log(JSON.stringify({status:p.status,signal:p.signal,error:p.error?.message||null}));console.log((p.stdout+p.stderr).slice(-5000));const verdict=JSON.parse(fs.readFileSync(out+'/result.json','utf8'));process.exitCode=p.status===0&&verdict.passed?0:p.status||2;
