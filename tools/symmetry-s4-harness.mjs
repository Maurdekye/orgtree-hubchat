// Shared set-up for the symmetry S4 mounted proofs: bundle an entry with esbuild (dependencies read in place),
// mount it in jsdom, hand back a check recorder. Synthetic API only, never a live hub. Run under the repository
// Job limiter (HUBCHAT_JOB_CONTAINED=1, an assigned C:\orgtree-build output); no dependency copies.
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {pathToFileURL} from 'node:url';

export async function boot(entryName) {
  const repo = process.cwd(), out = process.env.HUBCHAT_RENDERER_OUT;
  if (process.env.HUBCHAT_JOB_CONTAINED !== '1' || !/^C:[\\/]orgtree-build[\\/]/i.test(out || '')) throw Error('Assigned C output and Job required');
  const hub = createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT, 'package.json'));
  const deps = createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT, 'package.json'));
  fs.mkdirSync(out, {recursive: true});
  const esbuild = deps('esbuild');
  try {
    await esbuild.build({entryPoints: [repo + '/tools/' + entryName], outfile: out + '/entry.mjs', bundle: true,
      format: 'esm', platform: 'browser', jsx: 'automatic', loader: {'.md': 'text', '.svg': 'text', '.png': 'dataurl', '.css': 'empty'},
      plugins: [{name: 'existing-dependencies', setup(b) {
        b.onResolve({filter: /^[^./][^:]*/}, a => ({path: hub.resolve(a.path === 'qrcode' ? 'qrcode/lib/browser.js' : a.path)}));
      }}]});
  } finally { await esbuild.stop(); }
  const {JSDOM} = deps('jsdom');
  const dom = new JSDOM('<div id="root"></div>', {url: 'http://scratch.invalid', pretendToBeVisual: true});
  for (const k of ['window', 'document', 'navigator', 'HTMLElement', 'HTMLInputElement', 'HTMLTextAreaElement', 'Node', 'localStorage', 'sessionStorage', 'location', 'Event', 'MouseEvent', 'KeyboardEvent', 'ClipboardEvent', 'DataTransfer', 'File', 'FileList', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame'])
    if (dom.window[k] !== undefined) Object.defineProperty(globalThis, k, {value: dom.window[k], configurable: true});
  for (const [k, v] of Object.entries({innerWidth: 1000, innerHeight: 700})) Object.defineProperty(globalThis, k, {value: v, configurable: true, writable: true});
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  const mod = await import(pathToFileURL(out + '/entry.mjs'));
  const {act, createElement: h, createRoot} = mod;
  const root = createRoot(document.getElementById('root')), checks = [];
  const check = (name, ok) => { checks.push({name, ok: !!ok}); console.log((ok ? 'PASS ' : 'FAIL ') + name); };
  const settle = async (n = 4) => { for (let i = 0; i < n; i++) await act(async () => { await Promise.resolve(); }); };
  const click = async el => { if (!el || el.disabled) return false; await act(async () => { el.click(); await Promise.resolve(); }); await settle(); return true; };
  const render = async node => { await act(async () => { root.render(node); }); await settle(); };
  const finish = async scope => {
    await act(async () => root.unmount()); dom.window.close();
    fs.writeFileSync(out + '/result.json', JSON.stringify({passed: checks.every(c => c.ok), checks, scope}, null, 2));
    process.exit(checks.every(c => c.ok) ? 0 : 1);
  };
  return {mod, h, act, root, check, checks, settle, click, render, finish, document: dom.window.document, window: dom.window};
}
