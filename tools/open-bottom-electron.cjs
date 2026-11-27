// Offscreen Electron for tools/proof-open-bottom.mjs: each layout loads the mounted fixture (tools/open-bottom-entry.tsx),
// runs its steps and captures the opened chat. No OS input, media or network: http(s) is blocked and every permission
// denied. Muted. Records the Windows session of this process and of each renderer. Red (the source before the fix)
// expects exactly the failures below and captures before-*.png; green captures after-*.png.
const fs=require('node:fs'),path=require('node:path'),{execFileSync}=require('node:child_process');const{app,BrowserWindow,session}=require('electron');
const out=process.env.HUBCHAT_RENDERER_OUT,red=process.env.HUBCHAT_PROOF_PHASE==='red';
app.commandLine.appendSwitch('mute-audio');app.disableHardwareAcceleration();app.setPath('userData',path.join(out,'user-data'));
app.on('window-all-closed',()=>{});
const sessionOf=pid=>{try{return Number(execFileSync('powershell',['-NoProfile','-NonInteractive','-Command',`(Get-Process -Id ${pid}).SessionId`],{encoding:'utf8',windowsHide:true}).trim());}catch{return null;}};
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const expectedRed=JSON.parse(process.env.HUBCHAT_OPEN_RED||'null')||['content that grows after the pin keeps the chat at its bottom'];
const COUNT=3;
// The natural opens depend on when each picture decodes: the source before the fix lands above the bottom on SOME
// opens only (measured: 319 px on the first desktop open in one run, 0 in another), so red may or may not fail it.
const NATURAL='every open of a chat that ends in pictures lands at its bottom';
app.whenReady().then(async()=>{
 const s=session.defaultSession;
 s.setPermissionRequestHandler((_w,_p,cb)=>cb(false));s.setPermissionCheckHandler(()=>false);
 s.webRequest.onBeforeRequest({urls:['http://*/*','https://*/*']},(_,cb)=>cb({cancel:true}));
 const results=[];let win;
 try{
  for(const platform of ['desktop','android']){
   win=new BrowserWindow({show:true,width:platform==='desktop'?1000:390,height:720,webPreferences:{backgroundThrottling:false,offscreen:true}});
   win.webContents.setAudioMuted(true);
   await win.loadFile(path.join(out,platform+'.html'),{query:{platform}});
   const js=code=>win.webContents.executeJavaScript(code);
   await js(`(async()=>{const d=performance.now()+25000;while(!window.openFixture){if(window.openFixtureError)throw Error(window.openFixtureError);if(performance.now()>d)throw Error('No fixture');await new Promise(r=>setTimeout(r,20));}})()`);
   const r={platform,actions:[]};
   r.actions.push({step:'opens',state:await js(`window.openFixture.step("opens")`)});
   win.webContents.invalidate();await sleep(400);
   fs.writeFileSync(path.join(out,`${red?'before':'after'}-${platform}-opened.png`),(await win.webContents.capturePage()).toPNG());
   r.actions.push({step:'late-growth',state:await js(`window.openFixture.step("late-growth")`)});
   r.actions.push({step:'user-up',state:await js(`window.openFixture.step("user-up")`)});
   r.checks=await js('window.openFixture.checks');
   r.session={main:sessionOf(process.pid),renderer:sessionOf(win.webContents.getOSProcessId())};
   r.failed=r.checks.filter(c=>!c.ok).map(c=>c.name).sort();
   r.verdict=r.checks.length===COUNT&&(red?expectedRed.every(n=>r.failed.includes(n))&&r.failed.every(n=>expectedRed.includes(n)||n===NATURAL):r.failed.length===0);
   results.push(r);win.destroy();win=null;
  }
  const passed=results.length===2&&results.every(r=>r.verdict&&r.session.renderer===0&&r.session.main===0);
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({passed,phase:red?'matched red':'green',results},null,2));
  console.log(JSON.stringify({passed,results:results.map(r=>({platform:r.platform,verdict:r.verdict,session:r.session,checks:r.checks.length,failed:r.failed,actions:r.actions}))},null,1));
  app.exit(passed?0:1);
 }catch(e){if(win&&!win.isDestroyed())win.destroy();console.error(e.stack);app.exit(2);}
});
