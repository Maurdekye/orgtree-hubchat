const fs=require('node:fs'),path=require('node:path');const{app,BrowserWindow}=require('electron');const out=process.env.HUBCHAT_RENDERER_OUT;
app.disableHardwareAcceleration();app.setPath('userData',path.join(out,'user-data'));
app.on('window-all-closed',()=>{}); // The next layout still needs its window.
app.whenReady().then(async()=>{
 const red=process.env.HUBCHAT_PROOF_PHASE==='red',results=[],pictures=[];
 const targets=['rows expose keyboard activation','in-progress and failed rows show transfer state','image click opens existing viewer','saved file click uses bound native attachment opener','saved menu exposes platform save action','remote file offers Download','download action queues existing transfer without opening','failed file offers retry','menu dismissal keeps contact panel open','expired file has no download or open action','outgoing content URI uses native bound opener','remote click opens only after download completes','show in chat returns to conversation','show in chat loads older pages and highlights exact message'];
 let win;
 try{
  // Top-level pages paint reliably after scrolling; hidden iframes can retain
  // an earlier route even after their DOM assertions have settled.
  for(const platform of ['desktop','android']){
   win=new BrowserWindow({show:false,width:platform==='desktop'?1000:390,height:690,webPreferences:{backgroundThrottling:false,offscreen:true}});
   win.webContents.session.webRequest.onBeforeRequest({urls:['http://*/*','https://*/*']},(_,cb)=>cb({cancel:true}));
   await win.loadFile(path.join(out,platform+'.html'),{query:{platform}});
   const r=await win.webContents.executeJavaScript(`(async()=>{
    const deadline=performance.now()+20000;
    while(!window.contactFilesResult){if(window.contactFilesError)throw Error(window.contactFilesError);if(performance.now()>deadline)throw Error('No fixture verdict');await new Promise(r=>setTimeout(r,20));}
    await document.fonts.ready;await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));await new Promise(r=>setTimeout(r,120));
    const rect=s=>{const e=document.querySelector(s),r=e?.getBoundingClientRect();return r?{x:r.x,y:r.y,width:r.width,height:r.height,display:getComputedStyle(e).display}:null;};
    return {...window.contactFilesResult,capture:{panel:!!document.querySelector('.info,.hero'),menu:!!document.querySelector('[role="menu"]'),width:innerWidth,height:innerHeight,panelRect:rect('.info,.hero'),menuRect:rect('[role="menu"]')}};
   })()`);
   results.push(r);
   if(!red){win.webContents.invalidate();await new Promise(r=>setTimeout(r,250));const png=(await win.webContents.capturePage()).toPNG();fs.writeFileSync(path.join(out,platform+'.png'),png);pictures.push('data:image/png;base64,'+png.toString('base64'));}
   win.destroy();win=null;
  }
  const passed=results.length===2&&results.every(r=>r.checks.length===17&&(red?JSON.stringify(r.checks.filter(c=>!c.ok).map(c=>c.name).sort())===JSON.stringify([...targets,...(r.platform==='desktop'?['folder reveal is desktop only']:[])].sort()):r.checks.every(c=>c.ok)&&r.capture.panel&&r.capture.menu));
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({passed,phase:red?'matched red':'green',results},null,2));
  if(!red){
   win=new BrowserWindow({show:false,webPreferences:{backgroundThrottling:false}});await win.loadURL('about:blank');
   const png=await win.webContents.executeJavaScript(`(async()=>{const images=await Promise.all(${JSON.stringify(pictures)}.map(url=>new Promise(r=>{const i=new Image();i.onload=()=>r(i);i.src=url;})));const c=document.createElement('canvas');c.width=images.reduce((n,i)=>n+i.width,0);c.height=Math.max(...images.map(i=>i.height));const ctx=c.getContext('2d');let x=0;for(const i of images){ctx.drawImage(i,x,0);x+=i.width;}return c.toDataURL('image/png').split(',')[1];})()`);
   fs.writeFileSync(path.join(out,'contact-files.png'),Buffer.from(png,'base64'));win.destroy();win=null;
  }
  console.log(JSON.stringify({passed,results}));app.exit(passed?0:1);
 }catch(e){if(win&&!win.isDestroyed())win.destroy();console.error(e.stack);app.exit(2);}
});
