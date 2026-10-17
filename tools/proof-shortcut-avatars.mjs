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
  platform:'browser',format:'iife',jsx:'automatic',nodePaths:[deps],loader:{'.md':'text','.svg':'text'},define:{'process.env.NODE_ENV':'"test"'}});
fs.writeFileSync(out+'/index.html','<!doctype html><html class="theme-dark"><head><link rel="stylesheet" href="entry.css"></head><body><div id="root"></div><script src="entry.js"></script></body></html>');

const env={...process.env};delete env.ELECTRON_RUN_AS_NODE;
const r=spawnSync(require('electron'),[repo+'/tools/shortcut-avatars-electron.cjs'],{env,encoding:'utf8',windowsHide:true,timeout:90000,maxBuffer:2e6});
console.log(r.stdout,r.stderr);
if(r.status!==0)throw Error('Avatar renderer failed '+JSON.stringify({status:r.status,signal:r.signal,error:r.error?.message}));
