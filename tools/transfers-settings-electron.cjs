const fs=require('node:fs'),path=require('node:path');const{app,BrowserWindow}=require('electron');const out=process.env.HUBCHAT_RENDERER_OUT;
app.disableHardwareAcceleration();app.setPath('userData',path.join(out,'user-data'));app.on('window-all-closed',()=>{});
app.whenReady().then(async()=>{
 const red=process.env.HUBCHAT_PROOF_PHASE==='red',results=[],pictures={chats:[],settings:[]};let win;
 const capture=async(platform,phase)=>{await win.webContents.executeJavaScript(`(async()=>{await document.fonts.ready;await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));})()`);win.webContents.invalidate();await new Promise(r=>setTimeout(r,250));const png=(await win.webContents.capturePage()).toPNG();fs.writeFileSync(path.join(out,platform+'-'+phase+'.png'),png);pictures[phase].push('data:image/png;base64,'+png.toString('base64'));};
 const wait=flag=>win.webContents.executeJavaScript(`(async()=>{const end=performance.now()+20000;while(!window.${flag}){if(window.transferSettingsError)throw Error(window.transferSettingsError);if(performance.now()>end)throw Error('Missing ${flag}');await new Promise(r=>setTimeout(r,20));}return window.${flag};})()`);
 try{
  for(const platform of ['desktop','android']){
   win=new BrowserWindow({show:false,width:platform==='desktop'?1000:390,height:690,webPreferences:{backgroundThrottling:false,offscreen:true}});
   win.webContents.session.webRequest.onBeforeRequest({urls:['http://*/*','https://*/*']},(_,cb)=>cb({cancel:true}));
   await win.loadFile(path.join(out,platform+'.html'),{query:{platform}});await wait('captureChats');
   if(!red)await capture(platform,'chats');
   await win.webContents.executeJavaScript('window.continueTransfers()');const r=await wait('transferSettings');results.push(r);
   if(!red)await capture(platform,'settings');win.destroy();win=null;
  }
  const fails=['chat header has no transfers bar','settings offers Transfers','transfers section opens','active transfer and progress retained','finished history retained','history View uses existing opener','active Cancel uses existing API','active row opens its conversation','idle Transfers stays reachable with history'];
  const passed=results.length===2&&results.every(r=>r.checks.length===13&&(red?JSON.stringify(r.checks.filter(c=>!c.ok).map(c=>c.name).sort())===JSON.stringify([...fails,...(r.platform==='desktop'?['desktop rail has no transfers bar']:[])].sort()):r.checks.every(c=>c.ok)));
  if(!red){
   win=new BrowserWindow({show:false});await win.loadURL('about:blank');const png=await win.webContents.executeJavaScript(`(async()=>{const rows=await Promise.all(${JSON.stringify([pictures.chats,pictures.settings])}.map(row=>Promise.all(row.map(url=>new Promise(r=>{const i=new Image();i.onload=()=>r(i);i.src=url;})))));const c=document.createElement('canvas');c.width=Math.max(...rows.map(row=>row.reduce((n,i)=>n+i.width,0)));c.height=rows.reduce((n,row)=>n+Math.max(...row.map(i=>i.height)),0);const ctx=c.getContext('2d');let y=0;for(const row of rows){let x=0;for(const i of row){ctx.drawImage(i,x,y);x+=i.width;}y+=Math.max(...row.map(i=>i.height));}return c.toDataURL('image/png').split(',')[1];})()`);fs.writeFileSync(out+'/transfers-settings.png',Buffer.from(png,'base64'));win.destroy();win=null;
  }
  fs.writeFileSync(out+'/result.json',JSON.stringify({passed,phase:red?'matched red':'green',results},null,2));console.log(JSON.stringify({passed,results}));app.exit(passed?0:1);
 }catch(e){if(win&&!win.isDestroyed())win.destroy();console.error(e.stack);app.exit(2);}
});
