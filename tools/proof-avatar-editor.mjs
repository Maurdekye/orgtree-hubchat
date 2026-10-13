import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { launch } from './cdp.mjs';
import { withBuildLock } from 'file:///D:/orgtree-work/worktrees/hubchat-dm-sol-usage-export/tools/rig/lib.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const out = process.env.AVATAR_EDITOR_OUT;
if (!out?.startsWith('C:\\orgtree-build\\hubchat-dm-sol\\')) throw Error('Owned C output required');
const depsRoot = 'E:/Libraries/Desktop/orgtree/.worktrees/orgtree-hubchat';
const deps = createRequire(depsRoot + '/package.json'), orgDeps = createRequire('E:/Libraries/Desktop/orgtree/package.json');
const esbuild = orgDeps('esbuild'); fs.mkdirSync(out, { recursive: true });
if (!process.argv.includes('--inside')) {
  const r = withBuildLock('Hubchat avatar focused renderer/typecheck proof', () => {
    const p = spawnSync(process.execPath, [fileURLToPath(import.meta.url), '--inside'], { cwd: root, env: process.env, windowsHide: true, encoding: 'utf8', timeout: 120000, maxBuffer: 4 * 1024 * 1024 });
    fs.writeFileSync(path.join(out, 'run.log'), p.stdout + p.stderr); console.log((p.stdout + p.stderr).slice(-3500)); return p.status ?? 1;
  });
  console.log(JSON.stringify(r)); process.exit(r.result);
}
await esbuild.build({ entryPoints: [root + '/tools/avatar-editor.fixture.tsx'], outfile: out + '/renderer.js', bundle: true, format: 'esm', platform: 'browser',
  target: 'es2022', jsx: 'automatic', loader: { '.md': 'text' }, plugins: [{ name: 'existing-dependencies', setup(b) {
    b.onResolve({ filter: /^[^./][^:]*/ }, a => ({ path: deps.resolve(a.path === 'qrcode' ? 'qrcode/lib/browser.js' : a.path) }));
  } }] }); await esbuild.stop();
const sourceSheets = names => names.map(n => fs.readFileSync(path.join(root, 'src/styles', n + '.css'), 'utf8')).join('\n');
const shared = sourceSheets(['tokens', 'base', 'app']);
const sheets = { desktop: shared + sourceSheets(['desktop', 'app-desktop']), android: shared + sourceSheets(['mobile', 'app-mobile']) };
const old = spawnSync(process.execPath, [root + '/tools/proof-pictures-renderer.mjs'], { cwd: root, windowsHide: true, encoding: 'utf8', timeout: 30000,
  env: { ...process.env, HUBCHAT_DEPENDENCY_ROOT: depsRoot, HUBCHAT_DOM_DEPENDENCY_ROOT: 'E:/Libraries/Desktop/orgtree', HUBCHAT_RENDERER_OUT: out + '/legacy', HUBCHAT_RENDERER_EVIDENCE: out + '/legacy.json', HUBCHAT_JOB_CONTAINED: '1' } });
fs.writeFileSync(out + '/legacy.log', old.stdout + old.stderr);
const config = { extends: root + '/tsconfig.json', references: [], compilerOptions: {
  typeRoots: [depsRoot + '/node_modules/@types'], types: [depsRoot + '/node_modules/vite/client'],
  paths: { '*': [depsRoot + '/node_modules/*'], react: [depsRoot + '/node_modules/@types/react'], 'react/*': [depsRoot + '/node_modules/@types/react/*'],
    'react-dom': [depsRoot + '/node_modules/@types/react-dom'], 'react-dom/*': [depsRoot + '/node_modules/@types/react-dom/*'] },
} };
fs.writeFileSync(out + '/typecheck.json', JSON.stringify(config));
const t = spawnSync(process.execPath, [depsRoot + '/node_modules/typescript/bin/tsc', '-p', out + '/typecheck.json', '--noEmit'], { cwd: root, encoding: 'utf8', timeout: 30000, windowsHide: true });
fs.writeFileSync(out + '/typecheck.log', t.stdout + t.stderr);
const checks = [], check = (name, ok, detail) => { checks.push({ name, ok: !!ok, detail }); console.log(JSON.stringify({ name, ok: !!ok, detail })); };
check('source typecheck', t.status === 0, (t.stdout + t.stderr).slice(-500));
check('retained picture renderer controls', old.status === 0, (old.stdout + old.stderr).slice(-500));
const server = http.createServer((r, s) => {
  if (r.url === '/renderer.js') { s.setHeader('Content-Type', 'text/javascript'); s.end(fs.readFileSync(out + '/renderer.js')); }
  else if (r.url?.startsWith('/styles.css')) { s.setHeader('Content-Type', 'text/css'); s.end(sheets[r.url.includes('android') ? 'android' : 'desktop']); }
  else if (r.url === '/') s.end('<!doctype html><html><head><link id="styles" rel="stylesheet" href="/styles.css?desktop"><link rel="icon" href="data:,"></head><body><div id="root"></div><script type="module" src="/renderer.js"></script></body></html>');
  else { s.writeHead(404); s.end('synthetic fixture only'); }
});
await new Promise(r => server.listen(0, '127.0.0.1', r));
let browser;
try {
  browser = await launch({ width: 1100, height: 860 });
  await browser.nav('http://127.0.0.1:' + server.address().port + '/', 500);
  for (const platform of ['desktop', 'android']) for (const theme of ['dark', 'light']) {
    const label = platform + '-' + theme;
    await browser.size(platform === 'android' ? 390 : 1100, 860);
    await browser.eval(`document.getElementById('styles').href='/styles.css?${platform}'; proof.mount('${platform}','${theme}',true)`); await browser.sleep(180);
    check(label + ' direct avatar button/pencil/X idle and no entry controls', await browser.eval(`!!document.querySelector('button.profile-picture-button .profile-picture-pencil') && !!document.querySelector('button.profile-picture-remove') && document.querySelector('#picture-file').hidden && !document.querySelector('input[aria-label="Crop size"]') && ![...document.querySelectorAll('label')].some(e=>e.textContent==='Profile picture')`));
    const badge = await browser.eval(`(()=>{const p=document.querySelector('.profile-picture-pencil'),x=document.querySelector('.profile-picture-remove'),a=document.querySelector('.profile-picture-button');const pr=p.getBoundingClientRect(),xr=x.getBoundingClientRect(),ar=a.getBoundingClientRect();return {visible:!!pr.width&&getComputedStyle(p).visibility==='visible',bottom:pr.top>ar.top+ar.height/2,top:xr.top<ar.top+ar.height/2,aria:a.getAttribute('aria-label'),cursor:getComputedStyle(a).cursor}})()`);
    check(label + ' always visible bottom pencil/top X', badge.visible && badge.bottom && badge.top && badge.cursor === 'pointer' && badge.aria === 'Change profile picture', badge);
    await browser.shot(out + '/' + label + '.png');
    const count = await browser.eval('proof.chooser()');
    await browser.mouse('.profile-picture-button');
    check(label + ' avatar pointer opens chooser exactly once', await browser.eval('proof.chooser()') === count + 1);
    await browser.eval(`document.querySelector('.profile-picture-button').focus()`); await browser.key('Enter');
    check(label + ' Enter opens chooser', await browser.eval('proof.chooser()') === count + 2);
    await browser.send('Input.dispatchKeyEvent', { type: 'keyDown', key: ' ', code: 'Space', windowsVirtualKeyCode: 32 });
    await browser.send('Input.dispatchKeyEvent', { type: 'keyUp', key: ' ', code: 'Space', windowsVirtualKeyCode: 32 }); await browser.sleep(80);
    check(label + ' Space opens chooser', await browser.eval('proof.chooser()') === count + 3);
    await browser.eval('proof.select()'); await browser.sleep(100);
    check(label + ' existing preview/crop/save flow', !!await browser.count('img[alt="Selected profile picture crop"]') && !!await browser.count('input[aria-label="Crop size"]'));
    if (platform === 'desktop' && theme === 'dark') await browser.shot(out + '/desktop-crop.png');
    await browser.eval(`[...document.querySelectorAll('button')].find(b=>b.textContent==='Save picture').click()`); await browser.sleep(100);
    check(label + ' original bytes and crop sent to existing save API', await browser.eval(`proof.saved.at(-1).data==='BwgJ' && proof.saved.at(-1).crop.size===1 && !document.querySelector('img[alt="Selected profile picture crop"]')`));
    const beforeRemove = await browser.eval('proof.chooser()');
    await browser.mouse('.profile-picture-remove');
    check(label + ' X removes via API without chooser and disappears', await browser.eval(`proof.saved.at(-1).data===null && !document.querySelector('.profile-picture-remove') && !!document.querySelector('.profile-picture-pencil') && proof.chooser()===${beforeRemove}`));
    await browser.shot(out + '/' + label + '-empty.png');
  }
  await browser.eval(`proof.mount('desktop','dark',false)`); await browser.sleep(100);
  await browser.mouse('.profile-picture-button'); await browser.eval('proof.select(5*1024*1024+1)'); await browser.sleep(80);
  check('oversize preserves existing limit/error without preview', await browser.eval(`document.querySelector('[role="alert"]')?.textContent==='Choose an image up to 5 MiB.'`));
  await browser.eval('window.failPreview=true; proof.select()'); await browser.sleep(80);
  check('preview errors remain visible', await browser.eval(`document.querySelector('[role="alert"]')?.textContent==='Synthetic preview failure'`));
  await browser.eval('window.failPreview=false; proof.select()'); await browser.sleep(80);
  await browser.eval(`window.failSave=true; [...document.querySelectorAll('button')].find(b=>b.textContent==='Save picture').click()`); await browser.sleep(80);
  check('save errors keep crop for retry', await browser.eval(`document.querySelector('[role="alert"]')?.textContent==='Synthetic save failure' && !!document.querySelector('img[alt="Selected profile picture crop"]')`));
  check('no renderer errors', browser.errors.length === 0, browser.errors);
} catch (e) { check('proof completes', false, String(e)); }
finally { if (browser) { try { await browser.eval('proof.dispose()'); } catch {} await browser.close(); } await new Promise(r => server.close(r)); }
fs.writeFileSync(out + '/result.json', JSON.stringify({ scope: 'actual Settings/Avatar/PictureEditor with synthetic native API and chooser; Chrome desktop/Android layout, no Tauri/device acceptance', checks, passed: checks.filter(c => c.ok).length, total: checks.length }, null, 2));
process.exitCode = checks.every(c => c.ok) ? 0 : 1;
