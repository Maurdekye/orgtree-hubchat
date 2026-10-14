// Run bundling/browser under the shared lock and kernel Job. Synthetic only.
// DOM paint samples exclude native WebView startup/event delivery/show latency.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {createServer} from 'node:http';
import {launch} from './cdp.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const [output,deps]=process.argv.slice(2);
if(!output||!deps)throw Error('owned output, dependencies required');
fs.mkdirSync(output,{recursive:true});
const require=createRequire(path.resolve(deps,'../package.json')),esbuild=require('esbuild');
const build=await esbuild.build({stdin:{contents:
 'import{installMenu}from"./src/traymenu-view";import{fixture}from"./tools/tray-style-fixture";'+
 'const host=document.createElement("main");host.style.position="absolute";document.body.append(host);'+
 'let menu,serial=0;window.samples=[];'+
 'const invoke=async(command,args)=>{if(command==="hc_tray_popup_ready"){host.style.left=args.originX+"px";host.style.top=args.originY+"px";host.style.visibility="visible";}};'+
 'window.openSample=async(cold,x,y)=>{host.style.visibility="hidden";if(cold){menu?.dispose();menu=undefined;}'+
 'const start=performance.now();menu??=installMenu(host,invoke);const v={...fixture(false,x>450),generation:++serial,cursorX:x,cursorY:y,maxWidth:900,maxHeight:650};menu.update(v);'+
 'await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));const box=host.querySelector(".tray-pane").getBoundingClientRect().toJSON();'+
 'const sample={cold,x,y,rendererPaintMs:performance.now()-start,box};window.samples.push(sample);return sample;};',resolveDir:root,loader:'ts'},
 bundle:true,write:false,format:'iife',platform:'browser',nodePaths:[deps]});
esbuild.stop();
const js=build.outputFiles[0].contents,css=fs.readFileSync(root+'/src/styles/traymenu.css');
const server=createServer((req,res)=>{
 if(req.url==='/'){res.setHeader('content-type','text/html');res.end('<!doctype html><meta charset="utf-8"><style>body{background:#888}</style><link rel="stylesheet" href="/menu.css"><script defer src="/menu.js"></script>');return;}
 if(req.url==='/menu.js'){res.setHeader('content-type','text/javascript');res.end(js);return;}
 if(req.url==='/menu.css'){res.setHeader('content-type','text/css');res.end(css);return;}
 res.writeHead(204);res.end();
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
let b;const samples=[],scaledCorners=[];
try{
 b=await launch({width:900,height:650});await b.nav(`http://127.0.0.1:${server.address().port}/`);
 for(const cold of [true,false])for(let i=0;i<5;i++)samples.push(await b.eval(`openSample(${cold},${100+i*30},100)`));
 for(const [name,x,y]of[['middle',350,120],['bottom-right',890,640],['top-left',0,0]]){
   const s=await b.eval(`openSample(false,${x},${y})`);
   assert.ok(s.box.x>=0&&s.box.y>=0&&s.box.right<=900&&s.box.bottom<=650,JSON.stringify(s));
   if(name==='middle'){assert.equal(s.box.x,x);assert.equal(s.box.y,y);}
   await b.shot(path.join(output,name+'.png'));samples.push(s);
 }
 // Browser DSF emulation exercises real DOM layout at 125% and 150%.
 // Physical monitor origins below are coordinate composition, NOT a native
 // monitor or GetCursorPos measurement. Native acceptance remains separate.
 for(const scale of [1,1.25,1.5]){
   await b.size(900,650,scale);
   assert.equal(await b.eval('devicePixelRatio'),scale);
   for(const x of [700,800,868])for(const y of [480,560,618]){
     const s=await b.eval(`openSample(false,${x},${y})`);
     assert.equal(s.box.right,x,`right corner at DSF ${scale}: ${JSON.stringify(s)}`);
     assert.equal(s.box.bottom,y,`bottom corner at DSF ${scale}: ${JSON.stringify(s)}`);
     for(const work of [{x:0,y:0},{x:-1920,y:-200}]){
       const cursorPhysical={x:work.x+x*scale,y:work.y+y*scale};
       const cornerPhysical={x:work.x+s.box.right*scale,y:work.y+s.box.bottom*scale};
       assert.deepEqual(cornerPhysical,cursorPhysical);
       scaledCorners.push({scale,work,cursorPhysical,cornerPhysical,box:s.box});
     }
   }
   await b.shot(path.join(output,`corner-dsf-${scale}.png`));
 }
 assert.equal(b.errors.length,0,b.errors.join('\n'));
 fs.writeFileSync(output+'/result.json',JSON.stringify({samples,scaledCorners,errors:b.errors,
  scalingScope:'CDP deviceScaleFactor emulation; physical-origin composition, NOT native monitor/GetCursorPos',
  latencyScope:'renderer cold mount / warm update through two animation frames; NOT native click-to-visible'},null,2));
 console.log(JSON.stringify({samples:samples.length,scaledCorners:scaledCorners.length,captures:6,errors:b.errors.length}));
}finally{if(b)await b.close();await new Promise(r=>server.close(r));}
