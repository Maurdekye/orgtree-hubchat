// Mounted desktop/Android check of Settings › Changelog (tools/changelog-entry.tsx, driven by
// tools/changelog-electron.cjs): the real App on synthetic state; no hub, real data, media or network.
// Run inside a kernel Job with HUBCHAT_JOB_CONTAINED=1 and a new output folder under C:/orgtree-build
// (HUBCHAT_RENDERER_OUT). HUBCHAT_PROOF_PHASE=red expects exactly the failures of the source before the changelog.
//   node tools/proof-changelog.mjs [repo]   (default: the current folder)
import fs from 'node:fs';import path from 'node:path';import{createRequire}from'node:module';import{spawnSync}from'node:child_process';
const repo=path.resolve(process.argv[2]||process.cwd()),out=path.resolve(process.env.HUBCHAT_RENDERER_OUT||'');
if(process.env.HUBCHAT_JOB_CONTAINED!=='1'||!/^C:[\\/]orgtree-build[\\/]/i.test(out))throw Error('Assigned C output and kernel Job required');
if(fs.existsSync(out))throw Error('The output folder must be new: '+out);
const deps=createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT,'package.json')),dom=createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT,'package.json'));
fs.mkdirSync(out,{recursive:true});
fs.writeFileSync(out+'/result.json',JSON.stringify({passed:false,status:'not executed'}));
try{await dom('esbuild').build({entryPoints:[repo+'/tools/changelog-entry.tsx'],outfile:out+'/entry.js',bundle:true,platform:'browser',format:'iife',jsx:'automatic',loader:{'.md':'text','.svg':'text','.png':'dataurl'},plugins:[{name:'existing-dependencies',setup(b){b.onResolve({filter:/^[^./][^:]*/},a=>({path:deps.resolve(a.path==='qrcode'?'qrcode/lib/browser.js':a.path)}));}}]});}finally{dom('esbuild').stop();}
for(const platform of ['desktop','android']){
 const names=['tokens.css','base.css','app.css',...(platform==='desktop'?['desktop.css','app-desktop.css']:['mobile.css','app-mobile.css'])];
 fs.writeFileSync(out+'/'+platform+'.css',names.map(n=>fs.readFileSync(repo+'/src/styles/'+n,'utf8')).join('\n'));
 const own=fs.existsSync(out+'/entry.css')?'<link rel="stylesheet" href="entry.css">':'';
 fs.writeFileSync(out+'/'+platform+'.html',`<!doctype html><html class="theme-dark" data-platform="${platform}"><head><link rel="stylesheet" href="${platform}.css">${own}</head><body><div id="root" style="height:100vh"></div><script>window.addEventListener('error',e=>window.changelogError=e.message);</script><script src="entry.js"></script></body></html>`);
}
// the versions the repo has notes for, newest first: what the tab must list
const num=v=>v.split('.').map(Number),cmp=(a,b)=>{const x=num(a),y=num(b);for(let i=0;i<3;i++)if(x[i]!==y[i])return y[i]-x[i];return 0;};
const versions=fs.readdirSync(repo+'/docs').map(f=>/^release-notes-(\d+\.\d+\.\d+)\.md$/.exec(f)?.[1]).filter(Boolean).sort(cmp);
const env={...process.env,HUBCHAT_CHANGELOG_VERSIONS:JSON.stringify(versions)};delete env.ELECTRON_RUN_AS_NODE;delete env.NODE_OPTIONS;
const p=spawnSync(dom('electron'),[repo+'/tools/changelog-electron.cjs'],{env,encoding:'utf8',windowsHide:true,timeout:180000,maxBuffer:4e6});
fs.writeFileSync(out+'/electron.log',(p.stdout||'')+(p.stderr||'')+(p.error?String(p.error):''));
fs.rmSync(path.join(out,'user-data'),{recursive:true,force:true});
console.log(JSON.stringify({status:p.status,signal:p.signal,error:p.error?.message||null}));console.log((p.stdout+p.stderr).slice(-6000));
const verdict=JSON.parse(fs.readFileSync(out+'/result.json','utf8'));process.exitCode=p.status===0&&verdict.passed?0:p.status||2;
