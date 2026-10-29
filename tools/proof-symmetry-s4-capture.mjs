// Builds the symmetry S4 capture pages (esbuild, dependencies read in place) and runs the Electron driver, silent,
// Session 0, synthetic API. Run under the repository Job limiter: HUBCHAT_JOB_CONTAINED=1 and an assigned C:\orgtree-build output.
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
const repo = process.cwd(), out = path.resolve(process.env.HUBCHAT_RENDERER_OUT || '');
if (process.env.HUBCHAT_JOB_CONTAINED !== '1' || !/^C:[\\/]orgtree-build[\\/]/i.test(out)) throw Error('Assigned C output and Job required');
const hub = createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT, 'package.json'));
const dom = createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT, 'package.json'));
fs.mkdirSync(out, {recursive: true});
const esbuild = dom('esbuild');
try {
  await esbuild.build({entryPoints: [repo + '/tools/symmetry-s4-capture-entry.tsx'], outfile: out + '/entry.js', bundle: true, format: 'iife', platform: 'browser',
    jsx: 'automatic', splitting: false, loader: {'.md': 'text', '.svg': 'text', '.png': 'dataurl'},
    plugins: [{name: 'existing-dependencies', setup(b) {
      b.onResolve({filter: /^[^./][^:]*/}, a => ({path: hub.resolve(a.path === 'qrcode' ? 'qrcode/lib/browser.js' : a.path)}));
    }}]});
} finally { await esbuild.stop(); }
for (const platform of ['desktop', 'android']) {
  const names = ['tokens.css', 'base.css', 'app.css', ...(platform === 'desktop' ? ['desktop.css', 'app-desktop.css'] : ['mobile.css', 'app-mobile.css'])];
  fs.writeFileSync(out + '/' + platform + '.css', names.map(n => fs.readFileSync(repo + '/src/styles/' + n, 'utf8')).join('\n'));
  fs.writeFileSync(out + '/' + platform + '.html', `<!doctype html><html class="theme-dark" data-platform="${platform}"><head><meta charset="utf-8"><link rel="stylesheet" href="${platform}.css"></head><body><div id="root" style="height:100vh"></div><script>window.addEventListener('error',e=>{window.sceneError=e.message});</script><script src="entry.js"></script></body></html>`);
}
const env = {...process.env}; delete env.ELECTRON_RUN_AS_NODE;
const p = spawnSync(dom('electron'), [repo + '/tools/symmetry-s4-capture.cjs'], {env, encoding: 'utf8', windowsHide: true, timeout: 150000, maxBuffer: 4e6});
fs.writeFileSync(out + '/electron.log', (p.stdout || '') + (p.stderr || '') + (p.error ? String(p.error) : ''));
console.log(JSON.stringify({status: p.status, signal: p.signal, error: p.error?.message || null}));
console.log(((p.stdout || '') + (p.stderr || '')).slice(-3000));
process.exitCode = p.status === 0 ? 0 : p.status || 2;
