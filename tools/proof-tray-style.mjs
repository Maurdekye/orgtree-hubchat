// Whole invocation (bundling/browser included) belongs under the machine build
// slot and a kernel Job. No Hubchat app, installed data or sign-ins are loaded.
// node tools/proof-tray-style.mjs tests|visual <owned-C-output> <real-deps> [reference-dir]
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { launch } from './cdp.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const [mode, output, deps, refs] = process.argv.slice(2);
if (!['tests', 'visual'].includes(mode) || !output || !deps) throw Error('tests|visual, output, dependencies, [references] required');
fs.mkdirSync(output, { recursive: true });
const require = createRequire(path.resolve(deps, '../package.json'));
const esbuild = require('esbuild');
if (mode === 'tests') {
  const outfile = path.join(output, 'tray-style-tests.mjs');
  await esbuild.build({ entryPoints: [root + '/tools/tray-style.test.ts'], outfile,
    bundle: true, platform: 'node', format: 'esm', target: 'node22', nodePaths: [deps],
    external: ['node:*'], plugins: [{ name: 'real-jsdom', setup(build) {
      build.onResolve({ filter: /^jsdom$/ }, () => ({ path: pathToFileURL(require.resolve('jsdom')).href, external: true }));
    } }] });
  esbuild.stop();
  const p = spawnSync(process.execPath, ['--test', '--test-reporter=tap', outfile],
    { encoding: 'utf8', timeout: 45000, windowsHide: true, maxBuffer: 4 * 1024 * 1024 });
  const log = (p.stdout || '') + (p.stderr || ''); fs.writeFileSync(path.join(output, 'tests.log'), log);
  console.log(log); process.exitCode = p.status || (p.error ? 1 : 0);
} else {
  if (!refs) throw Error('Supplied Orgtree reference directory required');
  const build = await esbuild.build({ stdin: { contents:
    'import {installMenu} from "./src/traymenu-view";import{fixture}from"./tools/tray-style-fixture";' +
    'window.calls=[];window.menu=installMenu(document.body,async(command,args)=>window.calls.push({command,args}));' +
    'window.fixture=fixture;window.menu.update(fixture());', resolveDir: root, loader: 'ts' },
    bundle: true, write: false, platform: 'browser', format: 'iife', target: 'chrome130', nodePaths: [deps] });
  esbuild.stop();
  const js = build.outputFiles[0].contents;
  const css = fs.readFileSync(root + '/src/styles/traymenu.css');
  const server = createServer((req, res) => {
    if (req.url === '/') { res.setHeader('content-type', 'text/html'); return res.end('<!doctype html><meta charset="utf-8"><link rel="stylesheet" href="/menu.css"><script defer src="/menu.js"></script>'); }
    if (req.url === '/menu.js') { res.setHeader('content-type', 'text/javascript'); return res.end(js); }
    if (req.url === '/menu.css') { res.setHeader('content-type', 'text/css'); return res.end(css); }
    if (req.url === '/favicon.ico') { res.writeHead(204); return res.end(); }
    if (req.url === '/comparison') {
      res.setHeader('content-type', 'text/html');
      return res.end('<!doctype html><meta charset="utf-8"><style>body{background:#d8dce1;font:14px Segoe UI;margin:24px}main{display:flex;gap:32px;align-items:start}figure{margin:0}figcaption{margin-bottom:12px}img{display:block;max-width:600px}</style><main><figure><figcaption>Orgtree user reference · LIGHT</figcaption><img src="/reference.png"></figure><figure><figcaption>Hubchat synthetic popup · LIGHT</figcaption><img src="/main-light.png"><p>Dark variant is inferred; no dark context-menu reference was supplied.</figure></main>');
    }
    const files = { '/reference.png': path.join(refs, '3-main-only.png'), '/main-light.png': path.join(output, 'main-light.png') };
    if (files[req.url]) { res.setHeader('content-type', 'image/png'); return res.end(fs.readFileSync(files[req.url])); }
    res.writeHead(404); res.end();
  });
  await new Promise(r => server.listen(0, '127.0.0.1', r));
  let b; const measurements = [];
  try {
    b = await launch({ width: 900, height: 650 });
    const base = `http://127.0.0.1:${server.address().port}`;
    await b.nav(base + '/');
    for (const dark of [false, true]) for (const submenuLeft of [true, false]) {
      const theme = dark ? 'dark-inferred' : 'light';
      const side = submenuLeft ? 'left' : 'right';
      await b.eval(`{const v=fixture(${dark},${submenuLeft});v.generation=${1 + Number(dark) * 2 + Number(!submenuLeft)};menu.update(v)}`);
      await b.shot(path.join(output, `main-${theme}-${side}.png`));
      if (!dark && submenuLeft) await b.shot(path.join(output, 'main-light.png'), '.tray-pane', 8);
      await b.mouse('[data-id="notifications"]', 'hover', 30);
      measurements.push(await b.eval(`Array.from(document.querySelectorAll('.tray-pane')).map(p=>({theme:${JSON.stringify(theme)},side:${JSON.stringify(side)},box:p.getBoundingClientRect().toJSON(),background:getComputedStyle(p).backgroundColor,rowHeight:p.querySelector('button').getBoundingClientRect().height,rows:p.querySelectorAll('button').length,overflow:p.scrollWidth>p.clientWidth}))`));
      await b.shot(path.join(output, `submenu-${theme}-${side}.png`));
    }
    // Actual short work area, not injected DOM geometry; all submenu bounds
    // remain inside it and rows scroll to stay keyboard reachable.
    await b.size(280, 240);
    await b.eval('{const v=fixture();v.generation=10;v.maxWidth=280;v.maxHeight=240;menu.update(v)}');
    await b.key('n', 0, 10);
    await b.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'ArrowRight', code: 'ArrowRight', windowsVirtualKeyCode: 39 });
    const narrow = await b.eval('Array.from(document.querySelectorAll(".tray-pane")).map(p=>p.getBoundingClientRect().toJSON())');
    assert.ok(narrow.every(r => r.x >= 0 && r.y >= 0 && r.right <= 280 && r.bottom <= 240), JSON.stringify(narrow));
    await b.shot(path.join(output, 'narrow-short.png'));
    await b.size(1100, 700); await b.nav(base + '/comparison');
    await b.shot(path.join(output, 'comparison-light.png'));
    assert.equal(b.errors.length, 0, b.errors.join('\n'));
    assert.ok(measurements.flat().every(m => m.rowHeight === 28 && !m.overflow));
    fs.writeFileSync(path.join(output, 'visual.json'), JSON.stringify({ measurements, narrow, errors: b.errors, dark: 'inferred; no supplied dark context-menu reference' }, null, 2));
    console.log(JSON.stringify({ captures: 11, measurements: measurements.flat().length, errors: b.errors.length }));
  } finally { if (b) await b.close(); await new Promise(r => server.close(r)); }
}
