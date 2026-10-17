// Synthetic hidden Electron canvas proof. No Tauri, hub, identity or shell writes.
// Run inside the shared build slot and kernel Job, with outputs under C:.
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
const repo=process.cwd(), out=process.env.HUBCHAT_AVATAR_OUT;
if(!process.env.HUBCHAT_JOB_CONTAINED || !/^C:[\\/]orgtree-build[\\/]hubchat-jumplist-sol[\\/]/i.test(out||''))throw Error('Requires own C output and Job');
const deps=process.env.HUBCHAT_DEPENDENCY_ROOT;
const require=createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT,'package.json'));
fs.mkdirSync(out,{recursive:true});
await require('esbuild').build({entryPoints:[repo+'/tools/shortcut-avatars-entry.tsx'],outfile:out+'/entry.js',bundle:true,
  platform:'browser',format:'iife',jsx:'automatic',nodePaths:[deps],define:{'process.env.NODE_ENV':'"test"'}});
fs.writeFileSync(out+'/index.html','<!doctype html><html class="theme-dark"><head><link rel="stylesheet" href="entry.css"></head><body><div id="root"></div><script src="entry.js"></script></body></html>');
fs.writeFileSync(out+'/electron.cjs',`
const fs=require('node:fs'),path=require('node:path');
const {app,BrowserWindow}=require(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT+'/electron');
app.disableHardwareAcceleration();
app.setPath('userData',path.join(__dirname,'user-data'));
app.whenReady().then(async()=>{
 const win=new BrowserWindow({show:false,width:500,height:400,webPreferences:{backgroundThrottling:false}});
 win.webContents.session.webRequest.onBeforeRequest({urls:['http://*/*','https://*/*']},(_,cb)=>cb({cancel:true}));
 try {
  await win.loadFile(path.join(__dirname,'index.html'));
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
   return {checks,png};
  })()`);
  fs.writeFileSync(path.join(__dirname,'avatar.png'),Buffer.from(result.png.split(',')[1],'base64'));
  fs.writeFileSync(path.join(__dirname,'result.json'),JSON.stringify(result.checks,null,2));
  console.log('Avatar renderer checks '+result.checks.length+'/'+result.checks.length);
  win.destroy();app.exit(0);
 }catch(e){console.error(e.stack);win.destroy();app.exit(1);}
});
`);
const env={...process.env};delete env.ELECTRON_RUN_AS_NODE;
const r=spawnSync(require('electron'),[out+'/electron.cjs'],{env,encoding:'utf8',windowsHide:true,timeout:90000,maxBuffer:2e6});
console.log(r.stdout,r.stderr);
if(r.status!==0)throw Error('Avatar renderer failed '+JSON.stringify({status:r.status,signal:r.signal,error:r.error?.message}));
