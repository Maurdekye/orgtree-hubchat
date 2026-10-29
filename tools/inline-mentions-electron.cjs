// Offscreen Electron for tools/proof-inline-mentions.mjs: each view loads the
// mounted fixture, collects its checks, and (green) captures it. Muted, every
// permission (clipboard included) denied, http(s) blocked, input only to these
// offscreen windows. Records the Windows session of this process and of each
// renderer.
const fs=require('node:fs'),path=require('node:path'),{execFileSync}=require('node:child_process');const{app,BrowserWindow,session}=require('electron');
const out=process.env.HUBCHAT_RENDERER_OUT,red=process.env.HUBCHAT_PROOF_PHASE==='red';
app.commandLine.appendSwitch('mute-audio');app.disableHardwareAcceleration();app.setPath('userData',path.join(out,'user-data'));
app.on('window-all-closed',()=>{});
const sessionOf=pid=>{try{return Number(execFileSync('powershell',['-NoProfile','-NonInteractive','-Command',`(Get-Process -Id ${pid}).SessionId`],{encoding:'utf8',windowsHide:true}).trim());}catch{return null;}};
const views=[['desktop','marks'],['desktop','menu'],['desktop','live'],['android','marks'],['android','tap'],['android','open']];
// What the source before the marks fails, view by view (and nothing else).
const expectedRed={
 'desktop/marks':["an accessible person's mention shows their DM-list letters and colours","a picture mention shows the DM list's picture","an org's mention shows its DM-list letters, colours and shape","an agent session's mention shows the DM list's terminal icon","a mention of yourself shows your own avatar","a sent message's mention shows its mark too","inaccessible mentions show a small grey dot and no picture","their tooltip says they aren't on your hubs","an inaccessible mention's text is muted, without a pointer","an accessible mention's tooltip reads as Orgtree's: presence and type, name, description, address, action"],
 'desktop/menu':["clicking an inaccessible mention opens nothing and says nothing","right-click on an inaccessible mention offers Copy address, not Open chat"],
 'desktop/live':["enabling a hub turns a mention into its avatar, live, without re-rendering the message","a contact leaving the directory turns its mention into the dot, live"],
 'android/marks':["an accessible mention shows its DM-list avatar","an inaccessible mention shows the grey dot","its tooltip says they aren't on your hubs"],
 'android/tap':["tapping an inaccessible mention says why once, and stays in the chat"],
 'android/open':[],
};
// The views a reader should see, and what each picture is of.
const shots={'desktop/marks':'desktop-marks','desktop/menu':'desktop-menu-off','desktop/live':'desktop-live','android/marks':'android-marks','android/tap':'android-tap'};
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
    const deadline=performance.now()+25000;
    while(!window.inlineMentionsResult){if(window.inlineMentionsError)throw Error(window.inlineMentionsError);if(performance.now()>deadline)throw Error('No fixture verdict');await new Promise(r=>setTimeout(r,20));}
    await document.fonts.ready;await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));await new Promise(r=>setTimeout(r,120));
    return window.inlineMentionsResult;
   })()`);
   r.session={main:sessionOf(process.pid),renderer:sessionOf(win.webContents.getOSProcessId())};
   const key=platform+'/'+view,failed=r.checks.filter(c=>!c.ok).map(c=>c.name).sort();
   r.verdict=red?JSON.stringify(failed)===JSON.stringify([...expectedRed[key]].sort()):failed.length===0;
   if(!red&&shots[key]){
    win.webContents.invalidate();await new Promise(r=>setTimeout(r,250));
    fs.writeFileSync(path.join(out,shots[key]+'.png'),(await win.webContents.capturePage()).toPNG());
    // the received message up close (200% zoom), where the marks are
    if(platform==='desktop'&&view!=='menu'){
     win.webContents.setZoomFactor(2);await new Promise(r=>setTimeout(r,400));win.webContents.invalidate();await new Promise(r=>setTimeout(r,250));
     const b=await win.webContents.executeJavaScript(`(()=>{const e=document.querySelector('.msg.in .bubble');if(!e)return null;e.scrollIntoView({block:'center'});const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,zoom:devicePixelRatio}})()`);
     await new Promise(r=>setTimeout(r,250));
     if(b){const s=b.zoom||2,pad=8,x=Math.max(0,Math.floor((b.x-pad)*s)),y=Math.max(0,Math.floor((b.y-pad)*s));
      const img=await win.webContents.capturePage();const size=img.getSize();
      fs.writeFileSync(path.join(out,shots[key]+'-zoom.png'),img.crop({x:Math.min(x,size.width-1),y:Math.min(y,size.height-1),width:Math.min(Math.ceil((b.width+pad*2)*s),size.width-x),height:Math.min(Math.ceil((b.height+pad*2)*s),size.height-y)}).toPNG());
      r.zoom={bubble:b,image:size};}
    }
   }
   results.push(r);win.destroy();win=null;
  }
  const passed=results.length===views.length&&results.every(r=>r.verdict&&r.session.renderer===0&&r.session.main===0);
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({passed,phase:red?'matched red':'green',results},null,2));
  console.log(JSON.stringify({passed,results:results.map(r=>({platform:r.platform,view:r.view,verdict:r.verdict,session:r.session,calls:r.calls,failed:r.checks.filter(c=>!c.ok).map(c=>c.name)}))},null,1));
  app.exit(passed?0:1);
 }catch(e){if(win&&!win.isDestroyed())win.destroy();console.error(e.stack);app.exit(2);}
});
