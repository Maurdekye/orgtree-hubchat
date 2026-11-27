// Offscreen Electron for tools/proof-math.mjs: each layout loads the mounted fixture (tools/math-entry.tsx), runs its
// checks and captures the chat. No OS input, media or network: http(s) is blocked and every permission denied
// (clipboard included); the copy check uses the page's own ClipboardEvent. Muted. Records the Windows session of this
// process and of each renderer. The red phase (the source before math) captures the same pictures: the "before".
const fs=require('node:fs'),path=require('node:path'),{execFileSync}=require('node:child_process');const{app,BrowserWindow,session}=require('electron');
const out=process.env.HUBCHAT_RENDERER_OUT,red=process.env.HUBCHAT_PROOF_PHASE==='red';
app.commandLine.appendSwitch('mute-audio');app.disableHardwareAcceleration();app.setPath('userData',path.join(out,'user-data'));
app.on('window-all-closed',()=>{});
const sessionOf=pid=>{try{return Number(execFileSync('powershell',['-NoProfile','-NonInteractive','-Command',`(Get-Process -Id ${pid}).SessionId`],{encoding:'utf8',windowsHide:true}).trim());}catch{return null;}};
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const layouts=[['desktop','dark'],['desktop','light'],['android','dark'],['android','light']];
// what the source before math fails, and nothing else: amounts, code, broken LaTeX and the preview were text already.
const expectedRed=['inline math typesets: $...$ and \\(...\\)','display math typesets: $$...$$ and \\[...\\] on their own lines',
 'a long display formula scrolls inside its bubble, never past it','a formula keeps its colour; a typed tag stays text',
 "KaTeX's fonts load from the app itself",'copying a selected formula copies its TeX source'].sort();
const COUNT=10;
app.whenReady().then(async()=>{
 const s=session.defaultSession;
 s.setPermissionRequestHandler((_w,_p,cb)=>cb(false));s.setPermissionCheckHandler(()=>false);
 s.webRequest.onBeforeRequest({urls:['http://*/*','https://*/*']},(_,cb)=>cb({cancel:true}));
 const results=[];let win;
 try{
  for(const [platform,theme] of layouts){
   win=new BrowserWindow({show:true,width:platform==='desktop'?1000:390,height:820,webPreferences:{backgroundThrottling:false,offscreen:true}});
   win.webContents.setAudioMuted(true);
   await win.loadFile(path.join(out,platform+'.html'),{query:{platform,theme}});
   const js=code=>win.webContents.executeJavaScript(code);
   await js(`(async()=>{const d=performance.now()+25000;while(!window.mathFixture){if(window.mathFixtureError)throw Error(window.mathFixtureError);if(performance.now()>d)throw Error('No fixture');await new Promise(r=>setTimeout(r,20));}})()`);
   const r={platform,theme,actions:[]};
   r.actions.push({step:'render',state:await js(`window.mathFixture.step("render")`)});
   win.webContents.invalidate();await sleep(400);
   fs.writeFileSync(path.join(out,`${red?'before':'after'}-${platform}-${theme}.png`),(await win.webContents.capturePage()).toPNG());
   r.actions.push({step:'copy',state:await js(`window.mathFixture.step("copy")`)});
   r.checks=await js('window.mathFixture.checks');
   r.session={main:sessionOf(process.pid),renderer:sessionOf(win.webContents.getOSProcessId())};
   r.failed=r.checks.filter(c=>!c.ok).map(c=>c.name).sort();
   r.verdict=r.checks.length===COUNT&&(red?JSON.stringify(r.failed)===JSON.stringify(expectedRed):r.failed.length===0);
   results.push(r);win.destroy();win=null;
  }
  const passed=results.length===layouts.length&&results.every(r=>r.verdict&&r.session.renderer===0&&r.session.main===0);
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({passed,phase:red?'matched red':'green',results},null,2));
  console.log(JSON.stringify({passed,results:results.map(r=>({platform:r.platform,theme:r.theme,verdict:r.verdict,session:r.session,checks:r.checks.length,failed:r.failed}))},null,1));
  app.exit(passed?0:1);
 }catch(e){if(win&&!win.isDestroyed())win.destroy();console.error(e.stack);app.exit(2);}
});
