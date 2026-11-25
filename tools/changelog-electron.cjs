// Offscreen Electron for tools/proof-changelog.mjs: each layout loads the mounted fixture (tools/changelog-entry.tsx)
// and (green) captures the open Changelog tab in the dark and the light theme. No input, media or network: http(s) is
// blocked and every permission denied. Muted, no clipboard. Records the Windows session of this process and each renderer.
const fs=require('node:fs'),path=require('node:path'),{execFileSync}=require('node:child_process');const{app,BrowserWindow,session}=require('electron');
const out=process.env.HUBCHAT_RENDERER_OUT,red=process.env.HUBCHAT_PROOF_PHASE==='red',versions=process.env.HUBCHAT_CHANGELOG_VERSIONS||'[]';
app.commandLine.appendSwitch('mute-audio');app.disableHardwareAcceleration();app.setPath('userData',path.join(out,'user-data'));
app.on('window-all-closed',()=>{});
const sessionOf=pid=>{try{return Number(execFileSync('powershell',['-NoProfile','-NonInteractive','-Command',`(Get-Process -Id ${pid}).SessionId`],{encoding:'utf8',windowsHide:true}).trim());}catch{return null;}};
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
// before the changelog there is no tab: everything about it fails; the start, Settings and network checks pass
// (and on desktop, in this 690 px window, the section list was clipped: Quit Hubchat out of reach, and About with a 13th)
const changelogRed=['a Changelog tab between Help and About','the tab opens under its own title','every version, newest first',
 'the test build says its notes come with its release','no download sections','sanitized: no script, frame or live link'];
const expectedRed={desktop:[...changelogRed,'every section reachable'].sort(),android:[...changelogRed].sort()};
app.whenReady().then(async()=>{
 const s=session.defaultSession;
 s.setPermissionRequestHandler((_w,_p,cb)=>cb(false));s.setPermissionCheckHandler(()=>false);
 s.webRequest.onBeforeRequest({urls:['http://*/*','https://*/*']},(_,cb)=>cb({cancel:true}));
 const results=[];let win;
 try{
  for(const platform of ['desktop','android']){
   win=new BrowserWindow({show:true,width:platform==='desktop'?1000:390,height:690,webPreferences:{backgroundThrottling:false,offscreen:true}});
   win.webContents.setAudioMuted(true);
   await win.loadFile(path.join(out,platform+'.html'),{query:{platform,versions}});
   const js=code=>win.webContents.executeJavaScript(code);
   await js(`(async()=>{const d=performance.now()+25000;while(!window.changelogResult){if(window.changelogError)throw Error(window.changelogError);if(performance.now()>d)throw Error('No fixture');await new Promise(r=>setTimeout(r,20));}})()`);
   const shot=async name=>{win.webContents.invalidate();await sleep(350);fs.writeFileSync(path.join(out,name+'.png'),(await win.webContents.capturePage()).toPNG());};
   if(!red){await shot(platform+'-changelog-dark');await js('window.captureLight()');await shot(platform+'-changelog-light');if(platform==='desktop'){await js("document.querySelector('.set-nav').scrollTop=1e6");await shot('desktop-nav-end');}}
   const r={platform,checks:(await js('window.changelogResult')).checks};
   r.session={main:sessionOf(process.pid),renderer:sessionOf(win.webContents.getOSProcessId())};
   r.failed=r.checks.filter(c=>!c.ok).map(c=>c.name).sort();
   r.verdict=r.checks.length===10&&(red?JSON.stringify(r.failed)===JSON.stringify(expectedRed[platform]):r.failed.length===0);
   results.push(r);win.destroy();win=null;
  }
  const passed=results.length===2&&results.every(r=>r.verdict&&r.session.renderer===0&&r.session.main===0);
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({passed,phase:red?'matched red':'green',results},null,2));
  console.log(JSON.stringify({passed,results:results.map(r=>({platform:r.platform,verdict:r.verdict,session:r.session,failed:r.failed}))},null,1));
  app.exit(passed?0:1);
 }catch(e){if(win&&!win.isDestroyed())win.destroy();console.error(e.stack);app.exit(2);}
});
