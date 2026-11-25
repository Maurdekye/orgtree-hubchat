// Offscreen Electron for tools/proof-copy-image.mjs: each layout loads the mounted fixture (tools/copy-image-entry.tsx),
// runs its steps and (green) captures both menus. No OS input, media or network: http(s) is blocked and every
// permission denied (clipboard included); the page's clipboard is the fixture's stub. Muted. Records the Windows
// session of this process and of each renderer.
const fs=require('node:fs'),path=require('node:path'),{execFileSync}=require('node:child_process');const{app,BrowserWindow,session}=require('electron');
const out=process.env.HUBCHAT_RENDERER_OUT,red=process.env.HUBCHAT_PROOF_PHASE==='red';
app.commandLine.appendSwitch('mute-audio');app.disableHardwareAcceleration();app.setPath('userData',path.join(out,'user-data'));
app.on('window-all-closed',()=>{});
const sessionOf=pid=>{try{return Number(execFileSync('powershell',['-NoProfile','-NonInteractive','-Command',`(Get-Process -Id ${pid}).SessionId`],{encoding:'utf8',windowsHide:true}).trim());}catch{return null;}};
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const script=platform=>[['step','chat-menu'],['shot',platform+'-chat-menu'],['step','chat-copy'],['step','viewer-menu'],['shot',platform+'-viewer-menu'],['step','viewer-copy'],['step','dismiss'],['step','escape-back']];
// what the source before Copy image fails, and nothing else. Desktop already had a picture menu (on top); Android had
// none, and a long-press on a picture selected its message. A plain click on the backdrop closed the viewer before too.
const common=["the chat picture's menu offers Copy image, after View",'Copy image puts the picture on the clipboard as PNG','it says Image copied',
 "the zoomed picture has a menu: the chat's picture actions without View","the zoomed picture's menu is on top of the viewer",
 "the viewer's Copy image copies the picture too","dismissing the picture's menu leaves the viewer open",
 "the first Escape or Back closes the picture's menu and keeps the viewer"];
const expectedRed={desktop:[...common].sort(),android:[...common,"the chat picture's menu is on top","Android: a long-press on a picture doesn't select its message"].sort()};
const counts={desktop:11,android:13};
app.whenReady().then(async()=>{
 const s=session.defaultSession;
 s.setPermissionRequestHandler((_w,_p,cb)=>cb(false));s.setPermissionCheckHandler(()=>false);
 s.webRequest.onBeforeRequest({urls:['http://*/*','https://*/*']},(_,cb)=>cb({cancel:true}));
 const results=[];let win;
 try{
  for(const platform of ['desktop','android']){
   win=new BrowserWindow({show:true,width:platform==='desktop'?1000:390,height:690,webPreferences:{backgroundThrottling:false,offscreen:true}});
   win.webContents.setAudioMuted(true);
   await win.loadFile(path.join(out,platform+'.html'),{query:{platform}});
   const js=code=>win.webContents.executeJavaScript(code);
   await js(`(async()=>{const d=performance.now()+25000;while(!window.copyFixture){if(window.copyFixtureError)throw Error(window.copyFixtureError);if(performance.now()>d)throw Error('No fixture');await new Promise(r=>setTimeout(r,20));}})()`);
   const r={platform,actions:[]};
   for(const [act,arg] of script(platform)){
    if(act==='step')r.actions.push({step:arg,state:await js(`window.copyFixture.step(${JSON.stringify(arg)})`)});
    else if(!red){win.webContents.invalidate();await sleep(350);fs.writeFileSync(path.join(out,arg+'.png'),(await win.webContents.capturePage()).toPNG());}
   }
   r.checks=await js('window.copyFixture.checks');
   r.session={main:sessionOf(process.pid),renderer:sessionOf(win.webContents.getOSProcessId())};
   r.failed=r.checks.filter(c=>!c.ok).map(c=>c.name).sort();
   r.verdict=r.checks.length===counts[platform]&&(red?JSON.stringify(r.failed)===JSON.stringify(expectedRed[platform]):r.failed.length===0);
   results.push(r);win.destroy();win=null;
  }
  const passed=results.length===2&&results.every(r=>r.verdict&&r.session.renderer===0&&r.session.main===0);
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({passed,phase:red?'matched red':'green',results},null,2));
  console.log(JSON.stringify({passed,results:results.map(r=>({platform:r.platform,verdict:r.verdict,session:r.session,checks:r.checks.length,failed:r.failed}))},null,1));
  app.exit(passed?0:1);
 }catch(e){if(win&&!win.isDestroyed())win.destroy();console.error(e.stack);app.exit(2);}
});
