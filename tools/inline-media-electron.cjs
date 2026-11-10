// Offscreen Electron for tools/proof-inline-media.mjs: each view loads the mounted fixture
// (tools/inline-media-entry.tsx), runs its steps, does the clicks and keys between them as native input
// to its own offscreen window, and (green) captures. A stand-in answers the players' hcmedia URLs as the
// hub answers a range: a 206 to the end of the file, streamed only as the player reads it, and counts
// what each player read. (The shell's bounded 1 MiB pieces are proven by hubchat-core's tests and by
// tools/e2e-media.mjs in the real WebView2: Electron's custom protocol takes a short 206 for the end of
// the file.) Muted (--mute-audio and per window), every permission denied but fullscreen, http(s)
// blocked, no clipboard. Records the Windows session of this process and of each renderer.
const fs=require('node:fs'),path=require('node:path'),{execFileSync}=require('node:child_process');const{app,BrowserWindow,session,protocol}=require('electron');
const out=process.env.HUBCHAT_RENDERER_OUT,red=process.env.HUBCHAT_PROOF_PHASE==='red';
protocol.registerSchemesAsPrivileged([{scheme:'hcmedia',privileges:{standard:true,secure:true,stream:true,supportFetchAPI:true}}]);
app.commandLine.appendSwitch('mute-audio');app.disableHardwareAcceleration();app.setPath('userData',path.join(out,'user-data'));
app.on('window-all-closed',()=>{});
const sessionOf=pid=>{try{return Number(execFileSync('powershell',['-NoProfile','-NonInteractive','-Command',`(Get-Process -Id ${pid}).SessionId`],{encoding:'utf8',windowsHide:true}).trim());}catch{return null;}};
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const manifest=JSON.parse(fs.readFileSync(path.join(out,'media','manifest.json'),'utf8'));
// What each view does: fixture steps, native clicks on its targets, keys, and captures (green only).
const views={
 'desktop/video':[['step','video-load'],['shot','desktop-video-paused','vid'],['click','video'],['step','video-playing'],['shot','desktop-video-playing','vid'],['fullscreen'],['step','video-fullscreen'],['shot','desktop-video-fullscreen'],['key','Escape'],['step','video-escaped'],['shot','desktop-video-escaped','vid']],
 'desktop/stop':[['step','video-load'],['click','video'],['step','stop-scroll'],['click','video'],['step','stop-hidden'],['click','video'],['step','stop-chat']],
 'desktop/audio':[['step','audio-load'],['shot','desktop-audio-open','aud'],['click','play-audio'],['step','audio-playing'],['shot','desktop-audio-playing','aud'],['step','audio-chat']],
 'desktop/limits':[['step','limits'],['shot','desktop-limits']],
 'android/video':[['step','video-load'],['shot','android-video-paused'],['click','video'],['step','video-playing'],['shot','android-video-playing']],
 'android/audio':[['step','audio-load'],['click','play-audio'],['step','audio-playing'],['shot','android-audio-playing']],
};
const mediaChecks=['a received video shows its player on load: native controls, preload metadata','its first frame shows, paused, at the start',"it doesn't play on its own",'its file row stays below, with Download','the player read the start of the video, and only part of it, before Play'];
// What the source before the players fails, view by view (and nothing else).
const expectedRed={
 'desktop/video':[...mediaChecks,'a click on it plays it','it fills the screen',"and keeps playing there (the card's offscreen release leaves a fullscreen player alone)"],
 'desktop/stop':[...mediaChecks,'scrolled away, a playing video lets go of its file',"scrolled back, it shows its first frame again, paused: it never plays on its own",'hidden (the window minimised or in the tray), a playing video lets go','shown again, it is paused at its first frame','another chat opened, the playing video is gone and let go','back in the chat, the video is paused at its first frame'],
 'desktop/audio':['received audio shows its player on load like a video: visible native controls, preload metadata, no Play audio button','the audio player read only the start of its file on load, like a video','a click on its play control plays it','another chat opened, the audio stops','back in the chat, its player is open again, paused: it never plays on its own'],
 'desktop/limits':["past 256 MiB a video has no player, Orgtree's note, and its file row",'a WebM video shows its first frame too','a broken video says so, and keeps its file row',"a format this platform can't play says so, and keeps its file row"],
 'android/video':[...mediaChecks,'a click on it plays it'],
 'android/audio':['received audio shows its player on load like a video: visible native controls, preload metadata, no Play audio button','the audio player read only the start of its file on load, like a video','a click on its play control plays it'],
};
let log=[];
function answer(req){
 const u=new URL(req.url),lid=decodeURIComponent(u.pathname.split('/').pop()||''),f=manifest[lid];
 if(!f)return new Response(null,{status:404});
 const data=fs.readFileSync(path.join(out,'media',f.file)),total=data.length,range=req.headers.get('range');
 const m=/^bytes=(\d*)-(\d*)/i.exec(range||'bytes=0-');let start=0,end=total-1;
 if(m&&m[1]!==''){start=+m[1];if(m[2]!=='')end=Math.min(+m[2],total-1);}else if(m&&m[2]!==''){start=Math.max(0,total-+m[2]);}
 if(start>=total){log.push({lid,range,status:416,at:Date.now()});return new Response(null,{status:416,headers:{'Content-Range':`bytes */${total}`}});}
 const row={lid,range,status:206,start,end,read:0,at:Date.now()};log.push(row);let pos=start;
 const body=new ReadableStream({pull(c){if(pos>end){c.close();return;}const n=Math.min(65536,end-pos+1);c.enqueue(new Uint8Array(data.subarray(pos,pos+n)));pos+=n;row.read+=n;},cancel(){row.cancelled=true;}},{highWaterMark:0});
 return new Response(body,{status:206,headers:{'Content-Type':f.type,'Content-Range':`bytes ${start}-${end}/${total}`,'Content-Length':String(end-start+1),'Accept-Ranges':'bytes','Cache-Control':'no-store'}});
}
const served=lid=>log.filter(e=>e.lid===lid&&e.status===206);
app.whenReady().then(async()=>{
 const s=session.defaultSession;
 s.setPermissionRequestHandler((_w,p,cb)=>cb(p==='fullscreen'));s.setPermissionCheckHandler((_w,p)=>p==='fullscreen');
 s.webRequest.onBeforeRequest({urls:['http://*/*','https://*/*']},(_,cb)=>cb({cancel:true}));
 s.protocol.handle('hcmedia',answer);
 const results=[];let win;
 try{
  for(const [key,script] of Object.entries(views)){
   const [platform,view]=key.split('/');log=[];
   // shown (offscreen: no window appears), so the page is visible and its observers run
   win=new BrowserWindow({show:true,width:platform==='desktop'?1000:390,height:690,webPreferences:{backgroundThrottling:false,offscreen:true}});
   win.webContents.setAudioMuted(true);
   await win.loadFile(path.join(out,platform+'.html'),{query:{platform,view}});
   const js=(code,gesture=false)=>win.webContents.executeJavaScript(code,gesture);
   await js(`(async()=>{const d=performance.now()+25000;while(!window.mediaFixture){if(window.mediaFixtureError)throw Error(window.mediaFixtureError);if(performance.now()>d)throw Error('No fixture');await new Promise(r=>setTimeout(r,20));}})()`);
   const r={platform,view,visibility:await js('document.visibilityState'),actions:[],harness:[]};
   const hcheck=(name,ok,detail)=>r.harness.push({name,ok:!!ok,...(ok?{}:{detail})});
   const click=async what=>{const t=await js(`window.mediaFixture.target(${JSON.stringify(what)})`);if(!t){r.actions.push({click:what,target:null});return;}
    win.webContents.sendInputEvent({type:'mouseMove',x:t.x,y:t.y});await sleep(120);
    win.webContents.sendInputEvent({type:'mouseDown',x:t.x,y:t.y,button:'left',clickCount:1});await sleep(60);win.webContents.sendInputEvent({type:'mouseUp',x:t.x,y:t.y,button:'left',clickCount:1});
    r.actions.push({click:what,target:t});await sleep(300);};
   for(const [act,arg,crop] of script){
    if(act==='step'){
     r.actions.push({step:arg,state:await js(`window.mediaFixture.step(${JSON.stringify(arg)})`)});
     if(arg==='video-load'){const got=served('vid'),bytes=got.reduce((n,e)=>n+e.read,0);
      hcheck('the player read the start of the video, and only part of it, before Play',got.length>0&&got[0].start===0&&bytes>0&&bytes<manifest.vid.bytes,{bytes,total:manifest.vid.bytes,requests:got.map(e=>[e.range,e.read])});}
     if(arg==='audio-load')hcheck('the audio player read only the start of its file on load, like a video',served('aud').length>0&&served('aud')[0].start===0,served('aud'));
    }else if(act==='click')await click(arg);
    else if(act==='key'){win.webContents.sendInputEvent({type:'keyDown',keyCode:arg});win.webContents.sendInputEvent({type:'keyUp',keyCode:arg});r.actions.push({key:arg});}
    else if(act==='fullscreen'){
     // what the video's own fullscreen button does: the element's request, with a user gesture (the button's
     // place in Chromium's control row changes with the player's width, so it isn't clicked by position)
     const on=async()=>js(`!!document.fullscreenElement`);
     await js(`(document.querySelector('.att-media[data-att="vid"] video')?.requestFullscreen()??Promise.reject()).then(()=>true,()=>false)`,true);
     let ok=false;for(let i=0;i<30&&!(ok=await on());i++)await sleep(100);
     r.actions.push({fullscreen:ok});
    }else if(act==='shot'&&!red){
     win.webContents.invalidate();await sleep(350);
     const img=await win.webContents.capturePage();fs.writeFileSync(path.join(out,arg+'.png'),img.toPNG());
     if(crop){const b=await js(`(()=>{const e=document.querySelector('.att-media[data-att="${crop}"]')?.closest('.msg');if(!e)return null;const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,z:devicePixelRatio}})()`);
      if(b){const z=b.z||1,size=img.getSize(),x=Math.max(0,Math.floor((b.x-8)*z)),y=Math.max(0,Math.floor((b.y-8)*z));
       fs.writeFileSync(path.join(out,arg+'-card.png'),img.crop({x:Math.min(x,size.width-1),y:Math.min(y,size.height-1),width:Math.max(1,Math.min(Math.ceil((b.width+16)*z),size.width-x)),height:Math.max(1,Math.min(Math.ceil((b.height+16)*z),size.height-y))}).toPNG());}}
    }
   }
   const f=await js('({checks:window.mediaFixture.checks,calls:window.mediaFixture.calls})');r.checks=[...f.checks,...r.harness];r.calls=f.calls;r.requests=log.map(({lid,range,status,start,end,read,cancelled})=>({lid,range,status,start,end,read,cancelled:!!cancelled}));
   r.session={main:sessionOf(process.pid),renderer:sessionOf(win.webContents.getOSProcessId())};
   const failed=r.checks.filter(c=>!c.ok).map(c=>c.name).sort();r.failed=failed;
   r.verdict=red?JSON.stringify(failed)===JSON.stringify([...expectedRed[key]].sort()):failed.length===0;
   results.push(r);win.destroy();win=null;
  }
  const passed=results.length===Object.keys(views).length&&results.every(r=>r.verdict&&r.session.renderer===0&&r.session.main===0);
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({passed,phase:red?'matched red':'green',results},null,2));
  console.log(JSON.stringify({passed,results:results.map(r=>({platform:r.platform,view:r.view,verdict:r.verdict,session:r.session,visibility:r.visibility,failed:r.failed,actions:r.actions.filter(a=>a.fullscreen||a.target===null)}))},null,1));
  app.exit(passed?0:1);
 }catch(e){if(win&&!win.isDestroyed())win.destroy();console.error(e.stack);app.exit(2);}
});
