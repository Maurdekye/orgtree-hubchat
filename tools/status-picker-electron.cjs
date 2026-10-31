// Offscreen Electron for tools/proof-status-picker.mjs: each view loads the mounted fixture
// (tools/status-picker-entry.tsx), runs its steps and (green) captures. No input, no media, no network: http(s) is
// blocked and every permission denied. Muted (--mute-audio and per window), no clipboard. Records the Windows session
// of this process and of each renderer.
const fs=require('node:fs'),path=require('node:path'),{execFileSync}=require('node:child_process');const{app,BrowserWindow,session}=require('electron');
const out=process.env.HUBCHAT_RENDERER_OUT,red=process.env.HUBCHAT_PROOF_PHASE==='red';
app.commandLine.appendSwitch('mute-audio');app.disableHardwareAcceleration();app.setPath('userData',path.join(out,'user-data'));
app.on('window-all-closed',()=>{});
const sessionOf=pid=>{try{return Number(execFileSync('powershell',['-NoProfile','-NonInteractive','-Command',`(Get-Process -Id ${pid}).SessionId`],{encoding:'utf8',windowsHide:true}).trim());}catch{return null;}};
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
// What each view does: fixture steps and captures (green only).
const script=p=>[['step','me'],['shot',p+'-status-me'],['step','open'],['shot',p+'-status-menu'],['step','pick-dnd'],['shot',p+'-status-dnd'],
 ['step','reopen'],['shot',p+'-status-menu-dnd'],['step','close'],['step','notifications'],['shot',p+'-status-notifications'],['step','turn-off'],
 ['step','profile'],['step','person-dnd'],['shot',p+'-status-person-dnd'],['step','person-close'],['step','offline']];
const views={'desktop/status':script('desktop'),'android/status':script('android')};
// What the source before the picker fails: every check (nothing of it exists yet)
const names=p=>["your own avatar wears the dot others see: Automatic shows online",
 "the picker: Automatic, Online, Busy and Do not disturb, each with what it does and the dot others see; Automatic checked",
 "each choice is a radio item, as in the hub picker","the picker ends with Profile",
 p==='desktop'?"desktop: a popover under your avatar, headed Your status":"Android: a bottom sheet with its grip, headed Your status",
 "Do not disturb picked: the core is told, the picker closes and your dot turns red","reopened: Do not disturb is the one checked",
 "Settings › Notifications on Do not disturb: nothing shows or sounds on any device, messages still arrive","…with a way to turn it off",
 "Turn it off: Automatic again, and the note goes","Profile in the picker opens your profile, as your avatar did",
 "Contact info of a person on Do not disturb: the red dot, and what it means for a person",
 "your dot follows your status (Busy), and says offline while none of your hubs is connected"];
const expectedRed={'desktop/status':names('desktop'),'android/status':names('android')};
app.whenReady().then(async()=>{
 const s=session.defaultSession;
 s.setPermissionRequestHandler((_w,_p,cb)=>cb(false));s.setPermissionCheckHandler(()=>false);
 s.webRequest.onBeforeRequest({urls:['http://*/*','https://*/*']},(_,cb)=>cb({cancel:true}));
 const results=[];let win;
 try{
  for(const [key,steps] of Object.entries(views)){
   const [platform,view]=key.split('/');
   // shown (offscreen: no window appears), so the page is visible and its observers run
   win=new BrowserWindow({show:true,width:platform==='desktop'?1100:390,height:platform==='desktop'?720:800,webPreferences:{backgroundThrottling:false,offscreen:true}});
   win.webContents.setAudioMuted(true);
   await win.loadFile(path.join(out,platform+'.html'),{query:{platform,view}});
   const js=code=>win.webContents.executeJavaScript(code);
   await js(`(async()=>{const d=performance.now()+25000;while(!window.statusFixture){if(window.statusFixtureError)throw Error(window.statusFixtureError);if(performance.now()>d)throw Error('No fixture');await new Promise(r=>setTimeout(r,20));}})()`);
   const r={platform,view,visibility:await js('document.visibilityState'),actions:[]};
   for(const [act,arg] of steps){
    if(act==='step')r.actions.push({step:arg,state:await js(`window.statusFixture.step(${JSON.stringify(arg)})`)});
    else if(act==='shot'&&!red){win.webContents.invalidate();await sleep(350);fs.writeFileSync(path.join(out,arg+'.png'),(await win.webContents.capturePage()).toPNG());}
   }
   r.checks=await js('window.statusFixture.checks');
   r.session={main:sessionOf(process.pid),renderer:sessionOf(win.webContents.getOSProcessId())};
   const failed=r.checks.filter(c=>!c.ok).map(c=>c.name).sort();r.failed=failed;
   r.verdict=red?JSON.stringify(failed)===JSON.stringify([...expectedRed[key]].sort()):failed.length===0&&r.checks.length===expectedRed[key].length;
   results.push(r);win.destroy();win=null;
  }
  const passed=results.length===Object.keys(views).length&&results.every(r=>r.verdict&&r.session.renderer===0&&r.session.main===0);
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({passed,phase:red?'matched red':'green',results},null,2));
  console.log(JSON.stringify({passed,results:results.map(r=>({platform:r.platform,view:r.view,verdict:r.verdict,session:r.session,visibility:r.visibility,checks:r.checks.length,failed:r.failed}))},null,1));
  app.exit(passed?0:1);
 }catch(e){if(win&&!win.isDestroyed())win.destroy();console.error(e.stack);app.exit(2);}
});
