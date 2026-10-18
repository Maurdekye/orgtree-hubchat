// Run in the shared build slot under the existing Windows Job runner.
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
const repo = process.cwd(), out = process.env.HUBCHAT_MESSAGE_INFO_OUT;
if (!process.env.HUBCHAT_JOB_CONTAINED || !/^C:[\\/]orgtree-build[\\/]hubchat-jumplist-sol[\\/]/i.test(out || '')) throw Error('Own C output and kernel Job required');
const req = createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT, 'package.json'));
fs.mkdirSync(out, { recursive: true });
await req('esbuild').build({ entryPoints: [repo + '/tools/message-info-entry.tsx'], outfile: out + '/entry.js', bundle: true,
  platform: 'browser', format: 'iife', jsx: 'automatic', nodePaths: [process.env.HUBCHAT_DEPENDENCY_ROOT], loader: { '.md': 'text' },
  define: { 'process.env.NODE_ENV': '"test"' } });
const common = ['tokens.css', 'base.css', 'app.css'];
for (const platform of ['desktop', 'android']) {
  const names = [...common, ...(platform === 'desktop' ? ['desktop.css', 'app-desktop.css'] : ['mobile.css', 'app-mobile.css'])];
  fs.writeFileSync(out + '/' + platform + '.css', names.map(n => fs.readFileSync(repo + '/src/styles/' + n, 'utf8')).join('\n'));
  fs.writeFileSync(out + '/' + platform + '.html', `<!doctype html><html class="theme-dark" data-platform="${platform}"><head><link rel="stylesheet" href="${platform}.css"></head><body style="padding:20px;overflow:auto"><div id="root"></div><script src="entry.js"></script></body></html>`);
}
fs.writeFileSync(out + '/capture.html', '<!doctype html><html><body style="margin:0;background:#181818;color:#e8e8e8;font:14px sans-serif;display:flex"><section><h3 style="margin:18px">Desktop</h3><iframe style="width:460px;height:510px;border:0" src="desktop.html?platform=desktop"></iframe></section><section><h3 style="margin:18px">Android layout</h3><iframe style="width:450px;height:510px;border:0" src="android.html?platform=android"></iframe></section></body></html>');
const env = { ...process.env }; delete env.ELECTRON_RUN_AS_NODE;
const r = spawnSync(req('electron'), [repo + '/tools/message-info-electron.cjs'], { env, encoding: 'utf8', windowsHide: true, timeout: 45000, maxBuffer: 2e6 });
console.log(r.stdout, r.stderr);
console.log(JSON.stringify({ status: r.status, signal: r.signal, error: r.error?.message || null }));
process.exitCode = r.status === 0 ? 0 : r.status === 1 ? 1 : 2;
