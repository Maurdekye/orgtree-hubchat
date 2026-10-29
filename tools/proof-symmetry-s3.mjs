// Hubchat symmetry fixes S3 (docket symmetry-fixes-s3-hubchat-labels-hovers-and-keyb): the mounted real App on the
// synthetic API, in a DOM with no window (tools/symmetry-s3-entry.tsx), one process per platform. Run ONLY through
// verify-symmetry-s3.ps1 (kernel Job, new output folder under C:/orgtree-build).
// Phase green: every check passes. Phase red (the source before the fixes): exactly the checks whose name starts
// with "#<audit number>" fail and every other check, the controls, passes, so the controls really ran.
for (const k of ['HUBCHAT_DEPENDENCY_ROOT', 'HUBCHAT_DOM_DEPENDENCY_ROOT', 'HUBCHAT_RENDERER_OUT', 'HUBCHAT_RENDERER_EVIDENCE', 'HUBCHAT_JOB_CONTAINED']) {
  if (!process.env[k]) throw Error('Use the contained verifier; missing ' + k);
}
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';

const repo = process.cwd();
const phase = process.env.HUBCHAT_PROOF_PHASE || 'green';
const out = process.env.HUBCHAT_RENDERER_OUT;
const platform = process.argv[2];
fs.mkdirSync(out, { recursive: true });

if (!platform) {
  // Parent: build once, run each platform in its own process, judge the union.
  const orgdeps = createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT, 'package.json'));
  const deps = createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT, 'package.json'));
  const esbuild = orgdeps('esbuild');
  try {
    await esbuild.build({
      entryPoints: [path.join(repo, 'tools/symmetry-s3-entry.tsx')], outfile: path.join(out, 'entry.mjs'), bundle: true, format: 'esm',
      platform: 'browser', jsx: 'automatic', loader: { '.md': 'text', '.css': 'empty', '.svg': 'text', '.png': 'dataurl' },
      plugins: [{ name: 'existing-dependencies', setup(b) { b.onResolve({ filter: /^[^./][^:]*/ }, (a) => ({ path: deps.resolve(a.path === 'qrcode' ? 'qrcode/lib/browser.js' : a.path) })); } }],
    });
  } finally { esbuild.stop(); }
  const results = [];
  for (const p of ['desktop', 'android']) {
    const child = spawnSync(process.execPath, [process.argv[1], p], { env: process.env, cwd: repo, encoding: 'utf8', windowsHide: true, timeout: 80000, maxBuffer: 8e6 });
    process.stdout.write(child.stdout || ''); process.stderr.write(child.stderr || '');
    const file = path.join(out, p + '.json');
    if (!fs.existsSync(file)) { console.log('FAIL ' + p + ' produced no verdict (exit ' + child.status + ', ' + (child.error?.message || child.signal || '') + ')'); results.push({ platform: p, checks: [{ name: p + ' ran', ok: false }], notes: {} }); continue; }
    results.push(JSON.parse(fs.readFileSync(file, 'utf8')));
  }
  // Audit #5 is a check, not a change: every offline indicator is the same grey ring (--faint), nowhere red.
  const css = (n) => fs.readFileSync(path.join(repo, 'src/styles', n), 'utf8').split(/\r?\n/);
  const offline = [['base.css', '.pres.p-offline {'], ['base.css', '.ptext.p-offline::before {'], ['base.css', '.hdot.offline {'],
    ['desktop.css', '.hubpill.offline .dot, .hubpill.down .dot {'], ['desktop.css', '.hubst.offline .dot {'], ['mobile.css', '.hubst.offline .dot {']];
  const lines = offline.map(([f, s]) => css(f).find((l) => l.startsWith(s)) || '');
  const grey = lines.every((l) => l.includes('var(--faint)') && !/--bad|--danger|--err|red|#[0-9a-f]{3,6}\b/i.test(l));
  results.push({ platform: 'css', checks: [{ name: 'offline is the grey ring (--faint) on the avatar, presence text, hub picker and hub dots (audit #5, check only)', ok: grey }], notes: { rules: String(lines.filter(Boolean).length) } });
  const all = results.flatMap((r) => r.checks.map((c) => ({ ...c, platform: r.platform })));
  for (const c of all) console.log(`${c.ok ? 'PASS' : 'FAIL'} [${c.platform}] ${c.name}`);
  const labels = results.map((r) => r.notes.systemLabel).filter(Boolean);
  const sameLabel = labels.length === 2 && labels[0] === labels[1];
  console.log(`${sameLabel || phase === 'red' ? 'INFO' : 'FAIL'} system theme labels: ${JSON.stringify(labels)}`);
  const failed = all.filter((c) => !c.ok).map((c) => c.name).sort();
  const tagged = all.filter((c) => /^#\d+ /.test(c.name)).map((c) => c.name).sort();
  const names = [...new Set(failed)], expectedRed = [...new Set(tagged)];
  const ok = phase === 'red' ? JSON.stringify(names) === JSON.stringify(expectedRed) : failed.length === 0 && sameLabel;
  fs.writeFileSync(process.env.HUBCHAT_RENDERER_EVIDENCE, JSON.stringify({
    phase, passed: ok, total: all.length, failed,
    unexpectedPass: phase === 'red' ? expectedRed.filter((n) => !failed.includes(n)) : [],
    unexpectedFail: phase === 'red' ? failed.filter((n) => !expectedRed.includes(n)) : [], labels,
  }, null, 2));
  console.log(JSON.stringify({ phase, checks: all.length, failed: failed.length, ok }));
  process.exit(ok ? 0 : 1);
}

// Child: one platform in a DOM.
const orgdeps = createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT, 'package.json'));
const { JSDOM } = orgdeps('jsdom');
const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://scratch.invalid/?platform=' + platform, pretendToBeVisual: true });
const w = dom.window;
for (const k of Object.getOwnPropertyNames(w)) if (!(k in globalThis)) Object.defineProperty(globalThis, k, { value: w[k], configurable: true, writable: true });
for (const k of ['window', 'document', 'navigator', 'history', 'location', 'localStorage', 'sessionStorage', 'Event', 'CustomEvent', 'MouseEvent', 'KeyboardEvent', 'FocusEvent', 'InputEvent', 'MutationObserver', 'Node', 'Element', 'HTMLElement']) {
  Object.defineProperty(globalThis, k, { value: w[k], configurable: true, writable: true });
}
globalThis.getComputedStyle = w.getComputedStyle.bind(w);
globalThis.requestAnimationFrame = (f) => setTimeout(f, 0);
globalThis.cancelAnimationFrame = clearTimeout;
for (const k of ['ResizeObserver', 'IntersectionObserver']) if (!w[k]) globalThis[k] = w[k] = class { observe() {} unobserve() {} disconnect() {} };
if (!w.matchMedia) globalThis.matchMedia = w.matchMedia = () => ({ matches: false, media: '', addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} });
w.HTMLElement.prototype.scrollIntoView = function () {};
w.HTMLCanvasElement.prototype.getContext = () => null;
const mod = await import(pathToFileURL(path.join(out, 'entry.mjs')));
let result;
try { result = await Promise.race([mod.run(platform), new Promise((_, rej) => setTimeout(() => rej(Error('no verdict in 60 s')), 60000))]); }
catch (e) { console.log('FAIL ' + platform + ' threw: ' + (e?.stack || e)); result = { platform, checks: [{ name: platform + ' ran to the end', ok: false }], notes: {} }; }
fs.writeFileSync(path.join(out, platform + '.json'), JSON.stringify(result, null, 2));
w.close();
process.exit(0);
