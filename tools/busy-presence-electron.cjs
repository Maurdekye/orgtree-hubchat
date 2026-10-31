// Offscreen Electron for tools/proof-busy-presence.mjs: each view loads the mounted fixture
// (tools/busy-presence-entry.tsx), runs its steps and (green) captures. No input, no media, no network: http(s) is
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
 'desktop/presence':[['step','list'],['shot','desktop-list'],['step','header'],['shot','desktop-header'],['step','picker'],['shot','desktop-picker'],['step','picker-close'],['step','mention'],['step','info-person'],['shot','desktop-info-person'],['step','info-org'],['shot','desktop-info-org'],['step','directory'],['shot','desktop-directory']],
 'android/presence':[['step','list'],['shot','android-list'],['step','header'],['shot','android-header'],['step','picker'],['shot','android-picker'],['step','picker-close'],['step','mention'],['step','info-person'],['shot','android-info-person'],['step','info-org'],['shot','android-info-org'],['step','directory'],['shot','android-directory']],
};
// What the source before busy fails, view by view (and nothing else): everything but online and offline as before
// (before busy, a busy address is online, so the Directory counted it online)
const busyChecks=['the chat list: a busy person wears the busy dot, titled Busy','the chat list: a busy org wears the busy dot, titled Busy',"the open chat's header says busy, dot and word",
 'the hub picker: the hub where they are busy says so, dot and words','a mention of a busy org: its hover leads with Busy',
 'Contact info of a busy person: the busy dot and the word Busy','Contact info of a busy person says what busy means','Contact info of a busy org: the busy dot and the word Busy','Contact info of a busy org says what busy means, after what online means',
 'the Directory: Online now, then Busy now, then the people seen lately','the Directory: the busy rows wear the busy dot and say busy',
 'the Directory counts as online only who is online and not busy'];
const expectedRed={'desktop/presence':busyChecks,'android/presence':busyChecks};
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
   await js(`(async()=>{const d=performance.now()+25000;while(!window.busyFixture){if(window.busyFixtureError)throw Error(window.busyFixtureError);if(performance.now()>d)throw Error('No fixture');await new Promise(r=>setTimeout(r,20));}})()`);
   const r={platform,view,visibility:await js('document.visibilityState'),actions:[]};
   for(const [act,arg] of script){
    if(act==='step')r.actions.push({step:arg,state:await js(`window.busyFixture.step(${JSON.stringify(arg)})`)});
    else if(act==='shot'&&!red){win.webContents.invalidate();await sleep(350);fs.writeFileSync(path.join(out,arg+'.png'),(await win.webContents.capturePage()).toPNG());}
   }
   r.checks=await js('window.busyFixture.checks');
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
