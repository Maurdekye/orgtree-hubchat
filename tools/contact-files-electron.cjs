const fs=require('node:fs'),path=require('node:path');const{app,BrowserWindow}=require('electron');const out=process.env.HUBCHAT_RENDERER_OUT;
app.disableHardwareAcceleration();app.setPath('userData',path.join(out,'user-data'));
app.whenReady().then(async()=>{
 const win=new BrowserWindow({show:false,width:1000,height:690,webPreferences:{backgroundThrottling:false}});
 win.webContents.session.webRequest.onBeforeRequest({urls:['http://*/*','https://*/*']},(_,cb)=>cb({cancel:true}));
 try{
  await win.loadFile(path.join(out,'capture.html'));
  const results=await win.webContents.executeJavaScript(`(async()=>{
   const frames=[...document.querySelectorAll('iframe')],deadline=performance.now()+20000;
   while(frames.some(f=>!f.contentWindow.contactFilesResult)){
    const failed=frames.find(f=>f.contentWindow.contactFilesError);if(failed)throw Error(failed.src+': '+failed.contentWindow.contactFilesError);
    if(performance.now()>deadline)throw Error('No fixture verdict: '+JSON.stringify(frames.map(f=>({src:f.src,body:f.contentDocument.body.innerText.slice(0,600)}))));
    await new Promise(r=>setTimeout(r,20));
   }
   await Promise.all(frames.map(f=>f.contentDocument.fonts.ready));await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));await new Promise(r=>setTimeout(r,120));
   return frames.map(f=>f.contentWindow.contactFilesResult);
  })()`);
  const red=process.env.HUBCHAT_PROOF_PHASE==='red';
  const targets=['rows expose keyboard activation','in-progress and failed rows show transfer state','image click opens existing viewer','saved file click uses bound native attachment opener','saved menu exposes platform save action','remote file offers Download','download action queues existing transfer without opening','failed file offers retry','menu dismissal keeps contact panel open','expired file has no download or open action','outgoing content URI uses native bound opener','remote click opens only after download completes','show in chat returns to conversation','show in chat loads older pages and highlights exact message'];
  // Folder reveal is an expected red only on desktop; no accidental OS calls on Android is a control.
  const passed=results.length===2&&results.every(r=>r.checks.length===17&&(red?JSON.stringify(r.checks.filter(c=>!c.ok).map(c=>c.name).sort())===JSON.stringify([...targets,...(r.platform==='desktop'?['folder reveal is desktop only']:[])].sort()):r.checks.every(c=>c.ok)));
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({passed,phase:red?'matched red':'green',results},null,2));
  if(!red)fs.writeFileSync(path.join(out,'contact-files.png'),(await win.webContents.capturePage()).toPNG());
  console.log(JSON.stringify({passed,results}));app.exit(passed?0:1);
 }catch(e){console.error(e.stack);app.exit(2);}
});
