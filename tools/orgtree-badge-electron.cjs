// Offscreen Electron for tools/proof-orgtree-badge.mjs: each view loads the
// mounted fixture, collects its checks, and (green) captures it. Muted, every
// permission (clipboard included) denied, http(s) blocked, input only to these
// offscreen windows. Records the Windows session of this process and of each
// renderer.
const fs=require('node:fs'),path=require('node:path'),{execFileSync}=require('node:child_process');const{app,BrowserWindow,session}=require('electron');
const out=process.env.HUBCHAT_RENDERER_OUT,red=process.env.HUBCHAT_PROOF_PHASE==='red';
app.commandLine.appendSwitch('mute-audio');app.disableHardwareAcceleration();app.setPath('userData',path.join(out,'user-data'));
app.on('window-all-closed',()=>{});
const sessionOf=pid=>{try{return Number(execFileSync('powershell',['-NoProfile','-NonInteractive','-Command',`(Get-Process -Id ${pid}).SessionId`],{encoding:'utf8',windowsHide:true}).trim());}catch{return null;}};
const views=[['desktop','local'],['desktop','remote'],['desktop','fail'],['android','local']];
// What the source before the link fails, view by view (and nothing else).
const ask="asks which orgs this PC's Orgtree hosts",link=["a local org's badge is a button","its tooltip says Open in Orgtree","it is announced with the org's name","it keeps the chip's size and colours","clicking starts Orgtree on that org"];
const expectedRed={'desktop/local':[ask,...link,'the badge stays a link'],'desktop/remote':[ask],'desktop/fail':[ask,...link,'a failed start says so once'],'android/local':[]};
app.whenReady().then(async()=>{
 session.defaultSession.setPermissionRequestHandler((_w,_p,cb)=>cb(false));session.defaultSession.setPermissionCheckHandler(()=>false);
 session.defaultSession.webRequest.onBeforeRequest({urls:['http://*/*','https://*/*']},(_,cb)=>cb({cancel:true}));
 const results=[];let win;
 try{
  for(const [platform,view] of views){
   win=new BrowserWindow({show:false,width:platform==='desktop'?1000:390,height:690,webPreferences:{backgroundThrottling:false,offscreen:true}});
   win.webContents.setAudioMuted(true);
   await win.loadFile(path.join(out,platform+'.html'),{query:{platform,view}});
   const r=await win.webContents.executeJavaScript(`(async()=>{
    const deadline=performance.now()+20000;
    while(!window.orgtreeBadgeResult){if(window.orgtreeBadgeError)throw Error(window.orgtreeBadgeError);if(performance.now()>deadline)throw Error('No fixture verdict');await new Promise(r=>setTimeout(r,20));}
    await document.fonts.ready;await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));await new Promise(r=>setTimeout(r,120));
    const b=document.querySelector('${platform==='desktop'?'.info-hero':'.hero'} .chip.k-org'),rect=b?.getBoundingClientRect();
    return {...window.orgtreeBadgeResult,badge:b?{tag:b.tagName,x:rect.x+rect.width/2,y:rect.y+rect.height/2}:null};
   })()`);
   r.session={main:sessionOf(process.pid),renderer:sessionOf(win.webContents.getOSProcessId())};
   const key=platform+'/'+view,failed=r.checks.filter(c=>!c.ok).map(c=>c.name).sort();
   r.verdict=red?JSON.stringify(failed)===JSON.stringify([...expectedRed[key]].sort()):failed.length===0;
   if(!red){
    // the local badge with the pointer on it, as a reader sees it before clicking
    if(view==='local'&&r.badge?.tag==='BUTTON'){win.webContents.sendInputEvent({type:'mouseMove',x:Math.round(r.badge.x),y:Math.round(r.badge.y)});await new Promise(r=>setTimeout(r,300));
     r.hover=await win.webContents.executeJavaScript(`(()=>{const b=document.querySelector('.chip.k-org:hover');return b?{text:b.textContent,decoration:getComputedStyle(b).textDecorationLine}:null})()`);}
    win.webContents.invalidate();await new Promise(r=>setTimeout(r,250));
    fs.writeFileSync(path.join(out,platform+'-'+view+'.png'),(await win.webContents.capturePage()).toPNG());
   }
   results.push(r);win.destroy();win=null;
  }
  const passed=results.length===views.length&&results.every(r=>r.verdict&&r.session.renderer===0&&r.session.main===0)&&(red||results.find(r=>r.platform==='desktop'&&r.view==='local')?.hover!=null);
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({passed,phase:red?'matched red':'green',results},null,2));
  console.log(JSON.stringify({passed,results:results.map(r=>({platform:r.platform,view:r.view,verdict:r.verdict,session:r.session,calls:r.calls,hover:r.hover,failed:r.checks.filter(c=>!c.ok).map(c=>c.name)}))},null,1));
  app.exit(passed?0:1);
 }catch(e){if(win&&!win.isDestroyed())win.destroy();console.error(e.stack);app.exit(2);}
});
