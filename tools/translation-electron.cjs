// Offscreen Electron for tools/proof-translation.mjs: each view loads the mounted fixture (tools/translation-entry.tsx),
// runs its steps and (green) captures. No input, no media, no network: http(s) is blocked and every permission denied.
// Muted (--mute-audio and per window), no clipboard. Records the Windows session of
// this process and of each renderer.
const fs=require('node:fs'),path=require('node:path'),{execFileSync}=require('node:child_process');const{app,BrowserWindow,session}=require('electron');
const out=process.env.HUBCHAT_RENDERER_OUT,red=process.env.HUBCHAT_PROOF_PHASE==='red';
app.commandLine.appendSwitch('mute-audio');app.disableHardwareAcceleration();app.setPath('userData',path.join(out,'user-data'));
app.on('window-all-closed',()=>{});
const sessionOf=pid=>{try{return Number(execFileSync('powershell',['-NoProfile','-NonInteractive','-Command',`(Get-Process -Id ${pid}).SessionId`],{encoding:'utf8',windowsHide:true}).trim());}catch{return null;}};
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
// What each view does: fixture steps and captures (green only). The menu and Message info are desktop's.
const views={
 'desktop/translation':[['step','chat'],['shot','desktop-tr-chat'],['step','original'],['shot','desktop-tr-original'],['step','translation'],
  ['step','contact'],['shot','desktop-tr-contact'],['step','contact-close'],
  ['step','menu'],['step','info'],['shot','desktop-tr-info'],['step','info-close'],['step','profile'],['shot','desktop-tr-profile'],['step','pick'],
  ['step','hub-off'],['step','no-hub'],['shot','desktop-tr-profile-no-hub'],['step','no-hub-end']],
 'android/translation':[['step','chat'],['shot','android-tr-chat'],['step','original'],['shot','android-tr-original'],['step','translation'],
  ['step','contact'],['shot','android-tr-contact'],['step','contact-close'],
  ['step','profile'],['shot','android-tr-profile'],['step','pick'],['step','hub-off'],['step','no-hub'],['shot','android-tr-profile-no-hub'],['step','no-hub-end']],
};
const common=["the chat list shows the last message's translation",
 "Contact info shows the language they read in, in its own name",
 "a translated message shows its translation, with one grey line: Translated from Hebrew · Show original",
 "a message that wasn't translated says why, and shows its original",
 "no line when none was needed, while it is on its way, or on our own message (its own text shows)",
 "a reply's quote shows the reader's translation",
 "Show original: the original in place, the line now Original · Show translation",
 "Show translation: back to the translation"];
const profile=["Settings › Profile: Translate messages into, Off then the languages in their own names; Off chosen",
 "its caption: the hub translates, text leaves it; a connected hub that can't is named",
 "picked and saved with the Profile's Save: the language alone is told",
 "a hub with the feature that doesn't translate: says so",
 "no connected hub can translate: the row is disabled and names them"];
const desktopOnly=["desktop menu: Show original right after Copy text",
 "the menu's Show original shows the original, and the menu then offers Show translation",
 "Message info shows both versions and the languages"];
const allChecks={'desktop/translation':[...common,...desktopOnly,...profile],'android/translation':[...common,...profile]};
// Red (round 2, user 2026-11-01 16:13Z): the source before a person's language reached Contact info fails only that check
const lang="Contact info shows the language they read in, in its own name";
const expectedRed={'desktop/translation':[lang],'android/translation':[lang]};
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
   await js(`(async()=>{const d=performance.now()+25000;while(!window.trFixture){if(window.trFixtureError)throw Error(window.trFixtureError);if(performance.now()>d)throw Error('No fixture');await new Promise(r=>setTimeout(r,20));}})()`);
   const r={platform,view,visibility:await js('document.visibilityState'),actions:[]};
   for(const [act,arg] of steps){
    if(act==='step')r.actions.push({step:arg,state:await js(`window.trFixture.step(${JSON.stringify(arg)})`)});
    else if(act==='shot'&&!red){win.webContents.invalidate();await sleep(350);fs.writeFileSync(path.join(out,arg+'.png'),(await win.webContents.capturePage()).toPNG());}
   }
   r.checks=await js('window.trFixture.checks');
   r.session={main:sessionOf(process.pid),renderer:sessionOf(win.webContents.getOSProcessId())};
   const failed=r.checks.filter(c=>!c.ok).map(c=>c.name).sort();r.failed=failed;
   r.verdict=red?JSON.stringify(failed)===JSON.stringify([...expectedRed[key]].sort()):failed.length===0&&r.checks.length===allChecks[key].length;
   results.push(r);win.destroy();win=null;
  }
  const passed=results.length===Object.keys(views).length&&results.every(r=>r.verdict&&r.session.renderer===0&&r.session.main===0);
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({passed,phase:red?'matched red':'green',results},null,2));
  console.log(JSON.stringify({passed,results:results.map(r=>({platform:r.platform,view:r.view,verdict:r.verdict,session:r.session,visibility:r.visibility,checks:r.checks.length,failed:r.failed}))},null,1));
  app.exit(passed?0:1);
 }catch(e){if(win&&!win.isDestroyed())win.destroy();console.error(e.stack);app.exit(2);}
});
