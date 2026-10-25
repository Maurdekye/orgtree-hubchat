const fs=require('node:fs'),path=require('node:path');
const {app,BrowserWindow,clipboard}=require('electron'),out=process.env.HUBCHAT_RENDERER_OUT;
// Neither this host nor its renderer may touch the real clipboard.
for(const key of ['readText','readHTML','readRTF','readBookmark','readFindText','availableFormats'])clipboard[key]=()=>key==='availableFormats'?[]:'';
for(const key of ['write','writeText','writeHTML','writeRTF','writeBookmark','writeImage','writeFindText','clear'])clipboard[key]=()=>{};
app.disableHardwareAcceleration();app.setPath('userData',path.join(out,'user-data'));app.on('window-all-closed',()=>{});
app.whenReady().then(async()=>{
 let win;const results=[],red=process.env.HUBCHAT_PROOF_PHASE==='red';
 try{
  for(const platform of ['desktop','android']){
   win=new BrowserWindow({show:false,width:platform==='desktop'?1000:390,height:690,webPreferences:{backgroundThrottling:false,offscreen:true,contextIsolation:true,nodeIntegration:false}});
   win.webContents.session.setPermissionRequestHandler((_,__,cb)=>cb(false));win.webContents.session.setPermissionCheckHandler(()=>false);
   win.webContents.session.webRequest.onBeforeRequest({urls:['http://*/*','https://*/*']},(_,cb)=>cb({cancel:true}));win.webContents.setWindowOpenHandler(()=>({action:'deny'}));
   await win.loadFile(path.join(out,platform+'.html'),{query:{platform}});
   win.webContents.debugger.attach('1.3');
   const evaluate=async expression=>{const r=await win.webContents.debugger.sendCommand('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});if(r.exceptionDetails)throw Error('Renderer proof exception');return r.result.value;};
   await evaluate(`Object.defineProperty(navigator,'clipboard',{value:{readText:async()=>'',writeText:async()=>{throw Error('Clipboard blocked in proof')}}});true`);
   results.push(await evaluate('window.runRecoveryProof()'));
   if(!red){await evaluate(`(async()=>{await document.fonts.ready;await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));})()`);win.webContents.invalidate();await new Promise(r=>setTimeout(r,250));fs.writeFileSync(out+'/'+platform+'.png',(await win.webContents.capturePage()).toPNG());}
   win.destroy();win=null;
  }
  const passed=results.length===2&&results.every(r=>red?r.checks.length===4&&r.checks.filter(c=>!c.ok).map(c=>c.name).join('|')==='file input exists|named file chooser exists':r.checks.length>=24&&r.checks.every(c=>c.ok));
  fs.writeFileSync(out+'/result.json',JSON.stringify({passed,phase:red?'matched red':'green',isolation:{session:0,clipboard:'stubbed and denied',network:'blocked',input:'CDP to owned windows'},results},null,2));app.exit(passed?0:1);
 }catch(e){fs.writeFileSync(out+'/host-error.txt',String(e.stack));if(win&&!win.isDestroyed())win.destroy();app.exit(2);}
});
