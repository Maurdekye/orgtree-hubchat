import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
const repo=process.cwd(),out=path.resolve(process.env.HUBCHAT_RENDERER_OUT||'');
if(process.env.HUBCHAT_JOB_CONTAINED!=='1'||!/^C:[\\/]orgtree-build[\\/]hubchat-dm-sol[\\/]/i.test(out))throw Error('Assigned C output and kernel Job required');
const deps=createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT,'package.json')),dom=createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT,'package.json'));
fs.mkdirSync(out,{recursive:true});fs.writeFileSync(out+'/result.json',JSON.stringify({passed:false,status:'not executed'}));
try{await dom('esbuild').build({entryPoints:[repo+'/tools/recovery-file-entry.tsx'],outfile:out+'/entry.js',bundle:true,platform:'browser',format:'iife',jsx:'automatic',loader:{'.md':'text','.svg':'text','.png':'dataurl'},plugins:[{name:'existing-dependencies',setup(b){b.onResolve({filter:/^[^./][^:]*/},a=>({path:deps.resolve(a.path==='qrcode'?'qrcode/lib/browser.js':a.path)}));}}]});}finally{dom('esbuild').stop();}
const fixture=process.env.HUBCHAT_PROOF_PHASE==='red'?{text:'',words:''}:JSON.parse(fs.readFileSync(process.env.HUBCHAT_RECOVERY_FIXTURE,'utf8'));
for(const platform of ['desktop','android']){
 const names=['tokens.css','base.css','app.css',...(platform==='desktop'?['desktop.css','app-desktop.css']:['mobile.css','app-mobile.css'])];
 fs.writeFileSync(out+'/'+platform+'.css',names.map(n=>fs.readFileSync(repo+'/src/styles/'+n,'utf8')).join('\n'));
 fs.writeFileSync(out+'/'+platform+'.html',`<!doctype html><html class="theme-dark" data-platform="${platform}"><head><meta charset="utf-8"><link rel="stylesheet" href="${platform}.css"></head><body><div id="root" style="height:100vh"></div><script>window.recoveryFixture=${JSON.stringify(fixture).replace(/</g,'\\u003c')};</script><script src="entry.js"></script></body></html>`);
}
const env={...process.env};delete env.ELECTRON_RUN_AS_NODE;
const p=spawnSync(dom('electron'),[repo+'/tools/recovery-file-electron.cjs'],{env,encoding:'utf8',windowsHide:true,timeout:60000,maxBuffer:2e6});
fs.writeFileSync(out+'/electron.log',(p.stdout||'')+(p.stderr||'')+(p.error?String(p.error):''));
const verdict=JSON.parse(fs.readFileSync(out+'/result.json','utf8'));
console.log(JSON.stringify({status:p.status,signal:p.signal,error:p.error?.message||null,verdict}));
process.exitCode=p.status===0&&verdict.passed?0:p.status||2;
