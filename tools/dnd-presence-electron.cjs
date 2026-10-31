// Offscreen Electron for tools/proof-dnd-presence.mjs: each view loads the mounted fixture
// (tools/dnd-presence-entry.tsx), runs its steps and (green) captures. No input, no media, no network: http(s) is
// blocked and every permission denied. Muted (--mute-audio and per window), no clipboard. Records the Windows session
// of this process and of each renderer.
const fs=require('node:fs'),path=require('node:path'),{execFileSync}=require('node:child_process');const{app,BrowserWindow,session}=require('electron');
const out=process.env.HUBCHAT_RENDERER_OUT,red=process.env.HUBCHAT_PROOF_PHASE==='red';
app.commandLine.appendSwitch('mute-audio');app.disableHardwareAcceleration();app.setPath('userData',path.join(out,'user-data'));
app.on('window-all-closed',()=>{});
const sessionOf=pid=>{try{return Number(execFileSync('powershell',['-NoProfile','-NonInteractive','-Command',`(Get-Process -Id ${pid}).SessionId`],{encoding:'utf8',windowsHide:true}).trim());}catch{return null;}};
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
// What each view does: fixture steps and captures (green only).
const views={
 'desktop/presence':[['step','list'],['shot','desktop-list'],['step','header'],['shot','desktop-header'],['step','picker'],['shot','desktop-picker'],['step','picker-close'],['step','mention'],['step','info-org'],['shot','desktop-info-org'],['step','directory'],['shot','desktop-directory']],
 'android/presence':[['step','list'],['shot','android-list'],['step','header'],['shot','android-header'],['step','picker'],['shot','android-picker'],['step','picker-close'],['step','mention'],['step','info-org'],['shot','android-info-org'],['step','directory'],['shot','android-directory']],
};
// What the source before Do not disturb fails, view by view (and nothing else): every Do not disturb check (before it,
// an org on Do not disturb is busy, which the hub also says); online, busy and offline pass as they were
const dndChecks=['the chat list: an org on Do not disturb wears the red dot, titled Do not disturb',"the open chat's header says Do not disturb, dot and words",
 'the hub picker: the hub where it is on Do not disturb says so, dot and words','a mention of an org on Do not disturb: its hover leads with Do not disturb',
 'Contact info of an org on Do not disturb: the red dot and the words Do not disturb','Contact info of an org on Do not disturb says what it means, after what online means',
 'the Directory: Online now, then Busy now, then Do not disturb, then the people seen lately','the Directory: the row on Do not disturb wears the red dot and says do not disturb'];
const expectedRed={'desktop/presence':dndChecks,'android/presence':dndChecks};
app.whenReady().then(async()=>{
 const s=session.defaultSession;
 s.setPermissionRequestHandler((_w,_p,cb)=>cb(false));s.setPermissionCheckHandler(()=>false);
 s.webRequest.onBeforeRequest({urls:['http://*/*','https://*/*']},(_,cb)=>cb({cancel:true}));
 const results=[];let win;
 try{
  for(const [key,script] of Object.entries(views)){
   const [platform,view]=key.split('/');
   // shown (offscreen: no window appears), so the page is visible and its observers run
   win=new BrowserWindow({show:true,width:platform==='desktop'?1000:390,height:690,webPreferences:{backgroundThrottling:false,offscreen:true}});
   win.webContents.setAudioMuted(true);
   await win.loadFile(path.join(out,platform+'.html'),{query:{platform,view}});
   const js=code=>win.webContents.executeJavaScript(code);
   await js(`(async()=>{const d=performance.now()+25000;while(!window.dndFixture){if(window.dndFixtureError)throw Error(window.dndFixtureError);if(performance.now()>d)throw Error('No fixture');await new Promise(r=>setTimeout(r,20));}})()`);
   const r={platform,view,visibility:await js('document.visibilityState'),actions:[]};
   for(const [act,arg] of script){
    if(act==='step')r.actions.push({step:arg,state:await js(`window.dndFixture.step(${JSON.stringify(arg)})`)});
    else if(act==='shot'&&!red){win.webContents.invalidate();await sleep(350);fs.writeFileSync(path.join(out,arg+'.png'),(await win.webContents.capturePage()).toPNG());}
   }
   r.checks=await js('window.dndFixture.checks');
   r.session={main:sessionOf(process.pid),renderer:sessionOf(win.webContents.getOSProcessId())};
   const failed=r.checks.filter(c=>!c.ok).map(c=>c.name).sort();r.failed=failed;
   r.verdict=red?JSON.stringify(failed)===JSON.stringify([...expectedRed[key]].sort()):failed.length===0;
   results.push(r);win.destroy();win=null;
  }
  const passed=results.length===Object.keys(views).length&&results.every(r=>r.verdict&&r.session.renderer===0&&r.session.main===0);
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({passed,phase:red?'matched red':'green',results},null,2));
  console.log(JSON.stringify({passed,results:results.map(r=>({platform:r.platform,view:r.view,verdict:r.verdict,session:r.session,visibility:r.visibility,failed:r.failed}))},null,1));
  app.exit(passed?0:1);
 }catch(e){if(win&&!win.isDestroyed())win.destroy();console.error(e.stack);app.exit(2);}
});
