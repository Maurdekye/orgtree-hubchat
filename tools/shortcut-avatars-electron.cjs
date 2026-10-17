const out=process.env.HUBCHAT_AVATAR_OUT;
const fs=require('node:fs'),path=require('node:path');
const {app,BrowserWindow}=require('electron');
app.disableHardwareAcceleration();
app.setPath('userData',path.join(out,'user-data'));
app.whenReady().then(async()=>{
 const win=new BrowserWindow({show:false,width:500,height:400,webPreferences:{backgroundThrottling:false}});
 win.webContents.session.webRequest.onBeforeRequest({urls:['http://*/*','https://*/*']},(_,cb)=>cb({cancel:true}));
 try {
  await win.loadFile(path.join(out,'index.html'));
  const result=await win.webContents.executeJavaScript(`(async()=>{
   const checks=[]; const check=(name,ok)=>{checks.push({name,ok:!!ok});if(!ok)throw Error(name);};
   const frames=()=>new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
   await document.fonts.ready;
   for(const theme of ['dark','light'])for(const kind of ['person','org'])for(const [name,text] of [['Ada Lovelace','AL'],['Jean-Luc Picard','JP'],['','?'],['123 test','1T'],['Élodie Durand','ÉD']]){
    document.documentElement.className='theme-'+theme;
    mountAvatar(kind,name);await frames();
    const el=document.querySelector('.av'), style=getComputedStyle(el), expected=avatarFallback(kind,name);
    check(theme+'/'+kind+'/'+name+' client letters',el.textContent===text&&expected.text===text);
    check('client kind class',el.classList.contains(expected.cls));
    const png=avatarPng(kind,name),image=new Image();image.src=png;await image.decode();
    const c=document.createElement('canvas');c.width=c.height=128;const ctx=c.getContext('2d');ctx.drawImage(image,0,0);
    check('transparent outside avatar',ctx.getImageData(0,0,1,1).data[3]===0);
    // Compare the actual canvas background with a swatch in the client's color.
    const actual=Array.from(ctx.getImageData(64,15,1,1).data);
    const swatch=document.createElement('canvas').getContext('2d');swatch.fillStyle=style.backgroundColor;swatch.fillRect(0,0,1,1);
    const expectedPixel=Array.from(swatch.getImageData(0,0,1,1).data);
    check('same client background '+theme+'/'+kind,actual.every((v,i)=>Math.abs(v-expectedPixel[i])<=2));
    const data=ctx.getImageData(35,40,58,48).data;
    let letters=false;for(let i=0;i<data.length;i+=4)if(data[i+3]>actual[3]+10||Math.abs(data[i]-actual[0])>10||Math.abs(data[i+1]-actual[1])>10||Math.abs(data[i+2]-actual[2])>10){letters=true;break;}
    check('visible initials',letters);
   }
   document.documentElement.className='theme-dark';mountAvatar('person','Example Person');await frames();
   const png=avatarPng('person','Example Person');
   const taskPngs={};
   for(const theme of ['dark','light'])for(const view of ['settings','directory','newchat']){
    document.documentElement.className='theme-'+theme;mountTaskIcon(view);await frames();
    const svg=document.querySelector('svg'),style=getComputedStyle(document.querySelector('button'));
    check(theme+'/'+view+' uses exact client path',!!svg.querySelector('path').getAttribute('d'));
    const png=taskIconPng(view);taskPngs[view]=png;
    const image=new Image();image.src=png;await image.decode();
    const c=document.createElement('canvas');c.width=c.height=128;const ctx=c.getContext('2d');ctx.drawImage(image,0,0);
    const pixels=ctx.getImageData(0,0,128,128).data;
    const tile=new DOMParser().parseFromString(appIcon,'image/svg+xml').querySelector('rect');
    const sample=Array.from(ctx.getImageData(3,64,1,1).data);
    check(theme+'/'+view+' app icon background tile',sample[0]===31&&sample[1]===31&&sample[2]===31&&sample[3]===255);
    let visible=0;for(let i=3;i<pixels.length;i+=4)if(pixels[i]>0)visible++;
    check(theme+'/'+view+' visible transparent icon',visible>100&&visible<128*128);
    // Independent SVG rasterization of the mounted client icon must match PNG.
    const copy=svg.cloneNode(true);copy.setAttribute('xmlns','http://www.w3.org/2000/svg');copy.setAttribute('width','128');copy.setAttribute('height','128');copy.setAttribute('fill',style.color);
    const background=tile.cloneNode(true);background.setAttribute('width','24');background.setAttribute('height','24');background.setAttribute('rx',String(Number(tile.getAttribute('rx'))*24/256));
    copy.querySelector('path').setAttribute('transform','translate(4 4) scale(0.6666666666666666)');copy.prepend(background);
    const expected=new Image();expected.src='data:image/svg+xml;base64,'+btoa(new XMLSerializer().serializeToString(copy));await expected.decode();
    ctx.clearRect(0,0,128,128);ctx.drawImage(expected,0,0);
    const reference=ctx.getImageData(0,0,128,128).data;
    check(theme+'/'+view+' matches client SVG pixels',pixels.every((v,i)=>Math.abs(v-reference[i])<=2));
   }
   check('fixed tasks have three distinct glyphs',new Set(Object.values(taskPngs)).size===3);
   for(const platform of ['desktop','android']){
    await mountDirectory(platform);await frames();
    const rows=Array.from(document.querySelectorAll('.dir-row'));
    check(platform+' populated directory fixture',rows.length>0);
    check(platform+' no redundant row control',rows.every(r=>!r.querySelector('.btn,button')));
    const row=rows[0];row.focus();
    check(platform+' row keeps keyboard focus',document.activeElement===row&&row.tabIndex===0&&row.getAttribute('role')==='button');
    const before=opened.length;row.click();
    check(platform+' click opens one chat',opened.length===before+1);
    const address=opened[opened.length-1];
    for(const key of ['Enter',' ']){
     const before=opened.length;const event=new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true});row.dispatchEvent(event);
     check(platform+' '+key+' opens same chat once',opened.length===before+1&&opened[opened.length-1]===address&&event.defaultPrevented);
    }
    const beforeTap=opened.length;
    row.dispatchEvent(new PointerEvent('pointerdown',{pointerType:'touch',bubbles:true}));
    row.dispatchEvent(new PointerEvent('pointerup',{pointerType:'touch',bubbles:true}));row.click();
    check(platform+' touch click opens same chat once',opened.length===beforeTap+1&&opened[opened.length-1]===address);
   }
   return {checks,png,taskPngs};
  })()`);
  fs.writeFileSync(path.join(out,'avatar.png'),Buffer.from(result.png.split(',')[1],'base64'));
  for(const [view,png]of Object.entries(result.taskPngs))fs.writeFileSync(path.join(out,view+'.png'),Buffer.from(png.split(',')[1],'base64'));
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify(result.checks,null,2));
  console.log('Avatar renderer checks '+result.checks.length+'/'+result.checks.length);
  win.destroy();app.exit(0);
 }catch(e){console.error(e.stack);win.destroy();app.exit(1);}
});
