// Focused mounted desktop/Android check of inline video and audio (tools/inline-media-entry.tsx, driven by
// tools/inline-media-electron.cjs): synthetic messages; silent synthetic media made here with ffmpeg (the videos have
// no audio track, the audio is digital silence); no real data or network.
// Run inside a kernel Job with HUBCHAT_JOB_CONTAINED=1 and a new output folder under C:/orgtree-build
// (HUBCHAT_RENDERER_OUT). HUBCHAT_PROOF_PHASE=red expects exactly the failures of the source before the players.
//   node tools/proof-inline-media.mjs [repo]   (default: the current folder)   FFMPEG=<ffmpeg> (default: on PATH)
import fs from 'node:fs';import path from 'node:path';import crypto from 'node:crypto';import{createRequire}from'node:module';import{spawnSync}from'node:child_process';
const repo=path.resolve(process.argv[2]||process.cwd()),out=path.resolve(process.env.HUBCHAT_RENDERER_OUT||'');
if(process.env.HUBCHAT_JOB_CONTAINED!=='1'||!/^C:[\\/]orgtree-build[\\/]/i.test(out))throw Error('Assigned C output and kernel Job required');
if(fs.existsSync(out))throw Error('The output folder must be new: '+out);
const deps=createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT,'package.json')),dom=createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT,'package.json'));
fs.mkdirSync(path.join(out,'media'),{recursive:true});
fs.writeFileSync(out+'/result.json',JSON.stringify({passed:false,status:'not executed'}));
// the media: a numbered test pattern (its first frame is frame 0) of about 8 MB, so a player reads only its start before Play;
// a small WebM; six seconds of silence; and bytes that are no video at all
const ff=process.env.FFMPEG||'ffmpeg';
const make=(name,args)=>{const p=spawnSync(ff,['-hide_banner','-loglevel','error','-y',...args,path.join(out,'media',name)],{encoding:'utf8',timeout:120000,windowsHide:true});if(p.status!==0)throw Error('ffmpeg '+name+': '+(p.stderr||p.error));};
make('clip.mp4',['-f','lavfi','-i','testsrc2=size=640x360:rate=25','-t','25','-an','-c:v','libx264','-pix_fmt','yuv420p','-b:v','2500k','-movflags','+faststart']);
make('clip.webm',['-f','lavfi','-i','testsrc2=size=320x180:rate=25','-t','3','-an','-c:v','libvpx-vp9','-b:v','300k']);
make('notes.mp3',['-f','lavfi','-i','anullsrc=r=44100:cl=mono','-t','6','-c:a','libmp3lame','-b:a','64k']);
fs.writeFileSync(path.join(out,'media','broken.mp4'),Buffer.alloc(200000,'not a video '));
const entry=(file,type)=>{const b=fs.readFileSync(path.join(out,'media',file));return{file,type,bytes:b.length,sha256:crypto.createHash('sha256').update(b).digest('hex')};};
const manifest={vid:entry('clip.mp4','video/mp4'),webm:entry('clip.webm','video/webm'),aud:entry('notes.mp3','audio/mpeg'),bad:entry('broken.mp4','video/mp4'),flac:entry('notes.mp3','audio/flac')};
fs.writeFileSync(path.join(out,'media','manifest.json'),JSON.stringify(manifest,null,1));
try{await dom('esbuild').build({entryPoints:[repo+'/tools/inline-media-entry.tsx'],outfile:out+'/entry.js',bundle:true,platform:'browser',format:'iife',jsx:'automatic',loader:{'.md':'text','.svg':'text','.png':'dataurl'},plugins:[{name:'existing-dependencies',setup(b){b.onResolve({filter:/^[^./][^:]*/},a=>({path:deps.resolve(a.path==='qrcode'?'qrcode/lib/browser.js':a.path)}));}}]});}finally{dom('esbuild').stop();}
for(const platform of ['desktop','android']){
 const names=['tokens.css','base.css','app.css',...(platform==='desktop'?['desktop.css','app-desktop.css']:['mobile.css','app-mobile.css'])];
 fs.writeFileSync(out+'/'+platform+'.css',names.map(n=>fs.readFileSync(repo+'/src/styles/'+n,'utf8')).join('\n'));
 // the components' own stylesheets (images.css, media.css) come bundled as entry.css
 const own=fs.existsSync(out+'/entry.css')?'<link rel="stylesheet" href="entry.css">':'';
 fs.writeFileSync(out+'/'+platform+'.html',`<!doctype html><html class="theme-dark" data-platform="${platform}"><head><link rel="stylesheet" href="${platform}.css">${own}</head><body><div id="root" style="height:100vh"></div><script>window.mediaManifest=${JSON.stringify(manifest)};window.addEventListener('error',e=>window.mediaFixtureError=e.message);</script><script src="entry.js"></script></body></html>`);
}
const env={...process.env};delete env.ELECTRON_RUN_AS_NODE;delete env.NODE_OPTIONS;
const p=spawnSync(dom('electron'),[repo+'/tools/inline-media-electron.cjs'],{env,encoding:'utf8',windowsHide:true,timeout:240000,maxBuffer:4e6});
fs.writeFileSync(out+'/electron.log',(p.stdout||'')+(p.stderr||'')+(p.error?String(p.error):''));
// regenerable bulk goes; the manifest (sizes, sha256), the result and the captures stay
for(const f of ['clip.mp4','clip.webm','notes.mp3','broken.mp4'])fs.rmSync(path.join(out,'media',f),{force:true});
fs.rmSync(path.join(out,'user-data'),{recursive:true,force:true});
console.log(JSON.stringify({status:p.status,signal:p.signal,error:p.error?.message||null}));console.log((p.stdout+p.stderr).slice(-6000));
const verdict=JSON.parse(fs.readFileSync(out+'/result.json','utf8'));process.exitCode=p.status===0&&verdict.passed?0:p.status||2;
