// Long-chat benchmark (docket hubchat-on-the-steam-frame-lag-when-the-open-cha):
// bundles tools/long-chat-entry.tsx with the desktop styles and runs it in
// offscreen Electron (tools/long-chat-bench.cjs) for each history length.
// Run ONLY in Session 0, inside a kernel Job (joblimit.ps1), with:
//   HUBCHAT_DEPENDENCY_ROOT      a Hubchat checkout with node_modules (react, ...)
//   HUBCHAT_DOM_DEPENDENCY_ROOT  a folder whose node_modules has esbuild and electron
//   HUBCHAT_RENDERER_OUT         an empty output folder
//   HUBCHAT_LONG_CHAT_SIZES      optional, default 100,1000,5000,20000
//   HUBCHAT_LONG_CHAT_LABEL      optional, names the result file (base, tip, ...)
for (const k of ['HUBCHAT_DEPENDENCY_ROOT', 'HUBCHAT_DOM_DEPENDENCY_ROOT', 'HUBCHAT_RENDERER_OUT']) {
  if (!process.env[k]) throw Error('missing ' + k);
}
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
const repo = process.cwd();
const deps = createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT, 'package.json'));
const dom = createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT, 'package.json'));
const out = process.env.HUBCHAT_RENDERER_OUT; fs.mkdirSync(out, { recursive: true });
const esbuild = dom('esbuild');
try {
  await esbuild.build({
    entryPoints: [path.join(repo, 'tools/long-chat-entry.tsx')], outfile: path.join(out, 'long-chat.js'), bundle: true, platform: 'browser', format: 'iife', jsx: 'automatic',
    define: { 'process.env.NODE_ENV': '"production"' }, minify: false,
    loader: { '.md': 'text', '.svg': 'text', '.png': 'dataurl', '.woff2': 'file', '.woff': 'file', '.ttf': 'file' },
    plugins: [{ name: 'existing-dependencies', setup(b) {
      // count the rows whose memo misses, by the prop that changed (window.__memoMiss)
      b.onLoad({ filter: /MessageView\.tsx$/ }, a => {
        const src = fs.readFileSync(a.path, 'utf8');
        const hook = 'const __diff = (f: any) => memo(f, (x: any, y: any) => { let same = true; for (const k of new Set([...Object.keys(x), ...Object.keys(y)])) if (x[k] !== y[k]) { same = false; const w = window as any; (w.__memoMiss ??= {})[k] = ((w.__memoMiss[k] ?? 0) + 1); } return same; });\n';
        const out = src.replace('export const MessageView = memo(function MessageView(', hook + 'export const MessageView = __diff(function MessageView(');
        if (out === src) throw Error('MessageView memo not found');
        return { contents: out, loader: 'tsx' };
      });
      b.onResolve({ filter: /\.css\?inline$/ }, () => ({ path: 'empty', namespace: 'empty' }));
      b.onLoad({ filter: /.*/, namespace: 'empty' }, () => ({ contents: 'export default ""', loader: 'js' }));
      // the profiling build: production speed, and Profiler still reports render cost
      // a stylesheet's url(...) (KaTeX's fonts) is relative to it: esbuild resolves those itself
      b.onResolve({ filter: /^[^./][^:]*/ }, a => a.kind === 'url-token' ? undefined : ({ path: deps.resolve(a.path === 'qrcode' ? 'qrcode/lib/browser.js' : a.path === 'react-dom/client' ? 'react-dom/profiling' : a.path) }));
    } }],
  });
} finally { esbuild.stop(); }
const css = ['tokens.css', 'base.css', 'app.css', 'desktop.css', 'app-desktop.css'].map(n => fs.readFileSync(path.join(repo, 'src/styles', n), 'utf8')).join('\n');
fs.writeFileSync(path.join(out, 'styles.css'), css);
// styles a component imports itself are bundled next to the script
const own = fs.existsSync(path.join(out, 'long-chat.css')) ? '<link rel="stylesheet" href="long-chat.css">' : '';
fs.writeFileSync(path.join(out, 'long-chat.html'), `<!doctype html><html class="theme-dark" data-platform="desktop"><head><link rel="stylesheet" href="styles.css">${own}</head>`
  + `<body><div id="root" class="app" style="height:100vh;display:flex"></div><script src="long-chat.js"></script></body></html>`);
const electron = dom('electron');
const env = { ...process.env }; delete env.ELECTRON_RUN_AS_NODE;
const r = spawnSync(electron, [path.join(repo, 'tools/long-chat-bench.cjs')], { stdio: 'inherit', env });
process.exit(r.status ?? 1);
