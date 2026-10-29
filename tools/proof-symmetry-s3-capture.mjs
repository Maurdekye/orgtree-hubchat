// Captures for the Hubchat symmetry S3 fixes (tools/symmetry-s3-entry.tsx): the same checks as proof-symmetry-s3.mjs run in
// REAL Chromium (an offscreen Electron window, Session 0 only: it asserts the session), plus a screenshot of each state the
// fixes change. Run inside a kernel Job with HUBCHAT_JOB_CONTAINED=1 and a new output folder under C:/orgtree-build
// (HUBCHAT_RENDERER_OUT); HUBCHAT_DEPENDENCY_ROOT / HUBCHAT_DOM_DEPENDENCY_ROOT as for the DOM proof.
// HUBCHAT_PROOF_PHASE=red expects exactly the "#<audit number>" checks to fail (the source before the fixes).
import fs from 'node:fs'; import path from 'node:path'; import { createRequire } from 'node:module'; import { spawnSync } from 'node:child_process';
const repo = process.cwd(), out = path.resolve(process.env.HUBCHAT_RENDERER_OUT || '');
if (process.env.HUBCHAT_JOB_CONTAINED !== '1' || !/^C:[\\/]orgtree-build[\\/]/i.test(out)) throw Error('Assigned C output and kernel Job required');
if (fs.existsSync(out)) throw Error('The output folder must be new: ' + out);
const deps = createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT, 'package.json')), dom = createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT, 'package.json'));
fs.mkdirSync(out, { recursive: true });
fs.writeFileSync(out + '/result.json', JSON.stringify({ passed: false, status: 'not executed' }));
try {
  await dom('esbuild').build({ entryPoints: [repo + '/tools/symmetry-s3-entry.tsx'], outfile: out + '/entry.js', bundle: true, platform: 'browser', format: 'iife', jsx: 'automatic',
    loader: { '.md': 'text', '.svg': 'text', '.png': 'dataurl' }, plugins: [{ name: 'existing-dependencies', setup(b) { b.onResolve({ filter: /^[^./][^:]*/ }, (a) => ({ path: deps.resolve(a.path === 'qrcode' ? 'qrcode/lib/browser.js' : a.path) })); } }] });
} finally { dom('esbuild').stop(); }
for (const platform of ['desktop', 'android']) {
  const names = ['tokens.css', 'base.css', 'app.css', ...(platform === 'desktop' ? ['desktop.css', 'app-desktop.css'] : ['mobile.css', 'app-mobile.css'])];
  fs.writeFileSync(out + '/' + platform + '.css', names.map((n) => fs.readFileSync(repo + '/src/styles/' + n, 'utf8')).join('\n'));
  fs.writeFileSync(out + '/' + platform + '.html', `<!doctype html><html class="theme-dark" data-theme="dark" data-platform="${platform}"><head><link rel="stylesheet" href="${platform}.css"></head><body><div id="root" style="height:100vh"></div><script>window.addEventListener('error',e=>{window.symmetryS3Error=e.message});</script><script src="entry.js"></script></body></html>`);
}
const env = { ...process.env }; delete env.ELECTRON_RUN_AS_NODE; delete env.NODE_OPTIONS;
const p = spawnSync(dom('electron'), [repo + '/tools/symmetry-s3-electron.cjs'], { env, encoding: 'utf8', windowsHide: true, timeout: 170000, maxBuffer: 4e6 });
fs.writeFileSync(out + '/electron.log', (p.stdout || '') + (p.stderr || '') + (p.error ? String(p.error) : ''));
console.log(JSON.stringify({ status: p.status, signal: p.signal, error: p.error?.message || null }));
console.log(((p.stdout || '') + (p.stderr || '')).slice(-6000));
const verdict = JSON.parse(fs.readFileSync(out + '/result.json', 'utf8'));
process.exitCode = p.status === 0 && verdict.passed ? 0 : p.status || 2;
