const fs=require('node:fs'),path=require('node:path');
const {app,BrowserWindow}=require('electron');
const out=process.env.HUBCHAT_NEWCHAT_OUT;
app.disableHardwareAcceleration();app.setPath('userData',path.join(out,'user-data'));
app.whenReady().then(async()=>{
 const win=new BrowserWindow({show:false,width:930,height:650,webPreferences:{backgroundThrottling:false}});
 win.webContents.on('console-message',(_e,_level,message)=>console.log('page:',message));
 win.webContents.session.webRequest.onBeforeRequest({urls:['http://*/*','https://*/*']},(_,cb)=>cb({cancel:true}));
 try {
  await win.loadFile(path.join(out,'capture.html'));
  const results=await win.webContents.executeJavaScript(`(async()=>{
    const frames=[...document.querySelectorAll('iframe')];
    const deadline=performance.now()+12000;
    while(frames.some(f=>!f.contentWindow.firstSendResult)){
      const failed=frames.find(f=>f.contentWindow.firstSendError);
      if(failed)throw Error(failed.src+': '+failed.contentWindow.firstSendError);
      if(performance.now()>deadline)throw Error('Focused fixture did not execute: '+JSON.stringify(frames.map(f=>({src:f.src,ready:f.contentDocument.readyState,progress:f.contentWindow.firstSendProgress,error:f.contentWindow.firstSendError,body:f.contentDocument.body.innerText.slice(0,400)}))));
      await new Promise(r=>setTimeout(r,20));
    }
    await Promise.all(frames.map(f=>f.contentDocument.fonts.ready));
    await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
    await new Promise(r=>setTimeout(r,120));
    return frames.map(f=>f.contentWindow.firstSendResult);
  })()`);
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify(results,null,2));
  const targets=['first synced bubble is visible without inbound','new-chat details disappear after first send','actual sent state stays truthful','loaded UUID anchor survives retimed range reload','new-chat details stay absent after retimed reload'].sort();
  const red=process.env.HUBCHAT_PROOF_PHASE==='red';
  const passed=results.length===2&&results.every(r=>r.checks.length===9&&(red?JSON.stringify(r.checks.filter(c=>!c.ok).map(c=>c.name).sort())===JSON.stringify(targets):r.checks.every(c=>c.ok)));
  if(!red)fs.writeFileSync(path.join(out,'newchat.png'),(await win.webContents.capturePage()).toPNG());
  console.log(JSON.stringify({passed,phase:red?'matched red':'green',results}));app.exit(passed?0:1);
 } catch(e){console.error(e.stack);app.exit(2);}
});
