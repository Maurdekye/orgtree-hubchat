// The title bar is a native caption (user 2026-10-14 22:31Z: Hubchat
// 1.2.0-alpha.5's window could not be dragged by its title bar). A right-click
// on it opens the window's system menu, as Orgtree's title bar does (user
// 2026-10-15 11:30Z), and the version label shows its tooltip again.
// Checked in the real WebView2 of a desktop test build, without a mouse:
// agents run in a non-interactive window station, where Windows cannot move a
// window at all (GetCursorPos fails with error 1459 and a posted
// WM_NCLBUTTONDOWN starts no move loop, measured in rig round 446), so the drag
// itself is the user's check.
//   - computed style: .titlebar app-region drag, .window-controls no-drag, and
//     the version label no-drag, so it gets hover and shows its tooltip
//   - Windows' hit test (WM_NCHITTEST, tools/e2e-drag-region.ps1) as Windows
//     makes it: the window under the point (from the main window down, each
//     time the topmost child that takes the point) answers HTCAPTION at the
//     bar's empty part, the name, the logo and just above the version; at the
//     version, the three window buttons and in the window body it doesn't, and
//     no window that takes the point does. WebView2 makes a drag region out of
//     child windows of its own, on the app's UI thread, that answer HTCAPTION
//     wherever they are asked, so only windows that take the point count (rig
//     round 447).
//   - clicks posted as real input would deliver them (e2e-drag-region.ps1
//     -Mode clicks): Alt+Space (SC_KEYMENU) opens the system menu, which shows
//     the rig can see and recognise one; then a right-click at the bar's empty
//     part, name and logo opens the window's system menu too; a double-click on
//     the caption maximises the window and a second one restores it.
// The red control (the caption without the version label's fix) has the
// version label inside the caption; everything else is the same.
//
//   needs: the e2e build, from the worktree:
//     npx tauri build --debug --no-bundle --config src-tauri/tauri.test.conf.json
//       --config tools/e2e-drag-region.conf.json
//   node tools/e2e-drag-region.mjs <hubchat.exe e2e build> <out dir>
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { attach } from './cdp.mjs';

const [exeArg, outArg] = process.argv.slice(2);
if (!exeArg || !outArg) {
  console.error('usage: node tools/e2e-drag-region.mjs <hubchat.exe e2e build> <out dir>');
  process.exit(2);
}
const EXE = resolve(exeArg);
const OUT = resolve(outArg);
mkdirSync(OUT, { recursive: true });
const CONF = JSON.parse(readFileSync(join(import.meta.dirname, 'e2e-drag-region.conf.json'), 'utf8'));
const ID = CONF.identifier;
const DATA = join(process.env.APPDATA, ID);
const WEBVIEW = join(process.env.LOCALAPPDATA, ID);
const CRED = `identity:${DATA}.dev.orgtree.hubchat`;
const CDP = 9244;
const HTCAPTION = 2;
const J = JSON.stringify;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };
const sh = (cmd, args) => spawnSync(cmd, args, { encoding: 'utf8' }).stdout || '';
const until = async (f, ms, every = 100) => { const t = Date.now(); while (Date.now() - t < ms) { if (await f().catch(() => false)) return true; await sleep(every); } return false; };
const native = (mode, ptsFile, outFile, pid) => {
  const ps = spawnSync('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', join(import.meta.dirname, 'e2e-drag-region.ps1'),
    '-TargetPid', String(pid), '-Points', ptsFile, '-Out', outFile, '-Mode', mode], { encoding: 'utf8', timeout: 90000 });
  if (!existsSync(outFile)) throw new Error(`no ${mode} answers: ` + (ps.stderr || ps.stdout || String(ps.error || '')).slice(0, 800));
  const r = JSON.parse(readFileSync(outFile, 'utf8'));
  if (r.error) throw new Error(`${mode}: ` + r.error);
  return r;
};

// only this build's own data, by exact name
if (!ID.endsWith('.test') || ID === 'dev.orgtree.hubchat.test') throw new Error('not the e2e identifier: ' + ID);
const credExists = () => /Target:/.test(sh('cmdkey', [`/list:${CRED}`]));
const wipe = () => {
  if (credExists()) sh('cmdkey', [`/delete:${CRED}`]);
  for (const p of [DATA, WEBVIEW]) { if (!p.endsWith(`\\${ID}`)) throw new Error('not the e2e folder: ' + p); rmSync(p, { recursive: true, force: true }); }
};

let app = null;
let b = null;
const report = { styles: null, hittest: null, clicks: null };
try {
  wipe();
  app = spawn(EXE, [], { stdio: 'ignore', env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP}` } });
  for (let i = 0; i < 60 && !b; i++) { try { b = await attach(CDP); } catch { await sleep(500); } }
  if (!b) throw new Error('could not attach to the e2e build');
  if (!await until(() => b.eval(`typeof window.__TAURI_INTERNALS__?.invoke === 'function'`), 20000, 50)) throw new Error("the e2e build's page never loaded: " + J(b.target));
  const id = await b.eval(`window.__TAURI_INTERNALS__.invoke('plugin:app|identifier')`);
  if (id !== ID) throw new Error('this is not the e2e build: ' + id);
  if (!await until(() => b.eval(`!!document.querySelector('.titlebar .window-control.close') && !!document.querySelector('.titlebar .ver')`), 20000)) throw new Error('no title bar');
  await sleep(500);

  // the page's own view of the regions
  report.styles = await b.eval(`(() => {
    const st = (sel) => { const s = getComputedStyle(document.querySelector(sel)); return { webkit: s.getPropertyValue('-webkit-app-region'), plain: s.getPropertyValue('app-region') }; };
    return { titlebar: st('.titlebar'), controls: st('.titlebar .window-controls'), version: st('.titlebar .ver'),
      versionTitle: document.querySelector('.titlebar .ver').title };
  })()`);
  const is = (v, want) => v.webkit === want || v.plain === want;
  check('the title bar is app-region drag', is(report.styles.titlebar, 'drag'), J(report.styles.titlebar));
  check('the window buttons are app-region no-drag', is(report.styles.controls, 'no-drag'), J(report.styles.controls));
  check('the version label is app-region no-drag, so it gets hover for its tooltip', is(report.styles.version, 'no-drag'),
    `${J(report.styles.version)}, tooltip ${J(report.styles.versionTitle)}`);

  // points on the bar and below it, in page coordinates; act: what the clicks pass does there
  const pts = await b.eval(`(() => {
    const box = (sel, i = 0) => document.querySelectorAll(sel)[i].getBoundingClientRect();
    const c = (sel, i = 0) => { const r = box(sel, i); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; };
    const bar = box('.titlebar'), ver = box('.titlebar .ver');
    const body = document.querySelector('.win-body').getBoundingClientRect();
    return { dpr: devicePixelRatio, points: [
      { name: 'bar: empty part', drag: true, act: 'right double', ...c('.titlebar .tb-fill') },
      { name: 'bar: the name', drag: true, act: 'right', ...c('.titlebar .tb-brand [role=heading]') },
      { name: 'bar: the logo', drag: true, act: 'right', ...c('.titlebar .tb-brand svg') },
      { name: 'bar: just above the version', drag: true, x: ver.left + ver.width / 2, y: bar.top + Math.max(2, (ver.top - bar.top) / 2) },
      { name: 'bar: the version (no-drag, for its tooltip)', drag: false, ...c('.titlebar .ver') },
      { name: 'button: minimize', drag: false, ...c('.titlebar .window-control', 0) },
      { name: 'button: maximize', drag: false, ...c('.titlebar .window-control', 1) },
      { name: 'button: close', drag: false, ...c('.titlebar .window-control', 2) },
      { name: 'window body', drag: false, x: body.left + Math.min(200, body.width / 2), y: body.top + Math.min(120, body.height / 2) },
    ] };
  })()`);
  const ptsFile = join(OUT, 'points.json');
  writeFileSync(ptsFile, J(pts, null, 2));
  report.hittest = native('hittest', ptsFile, join(OUT, 'hittest.json'), app.pid);
  for (const [i, p] of report.hittest.points.entries()) {
    const want = pts.points[i].drag;
    const caption = p.answers.filter((a) => a.ht === HTCAPTION);
    const all = `under the point ${p.hit.cls} ${p.hit.hwnd}=${p.hit.ht}; taking it: ` + p.answers.map((a) => `${a.cls}=${a.ht}`).join(' ');
    check(`${p.name}: ${want ? 'Windows sees a caption' : 'no caption'}`,
      want ? p.hit.ht === HTCAPTION : p.hit.ht !== HTCAPTION && caption.length === 0, all);
  }

  // right-clicks, the keyboard's system menu and the double-click, as Windows would deliver them
  report.clicks = native('clicks', ptsFile, join(OUT, 'clicks.json'), app.pid);
  const k = report.clicks;
  // the menu seen: the thread's flags, its owner, each menu window with its items
  const menu = (s) => `system menu: ${s.system}; flags ${s.flags}, owner ${s.menuOwner}${s.ownerCls ? ' ' + s.ownerCls : ''}; `
    + (s.menus.map((m) => `menu ${m.hwnd} at ${J(m.rect)} with ${m.count} items ${m.items.join(' ')}`).join('; ') || 'no menu window');
  check('control: Alt+Space opens the system menu, so this rig can see and recognise one', k.control.opened && k.control.seen.system && k.control.dismissed,
    `${k.control.opened ? `opened after ${k.control.ms} ms (${menu(k.control.seen)})` : 'no menu in 1.5 s'}, closed again: ${k.control.dismissed}; main window style ${k.style}`);
  for (const r of k.right) {
    const how = `${r.press} then ${r.release.msg} to ${r.release.cls} ${r.release.to} (window under the point ${r.hit.cls}=${r.hit.ht}, capture after the press ${r.captureAfterPress})`;
    check(`${r.name}: a right-click opens the window's system menu`, !r.menuBefore && r.opened && r.seen.system && r.dismissed,
      `${how}; ${r.menuBefore ? 'a menu was still open before the press; ' : ''}${r.opened ? `a menu opened ${r.openAfterPress ? 'on the press' : `${r.ms} ms after the release`} (${menu(r.seen)}), closed again: ${r.dismissed}` : 'no menu in 1.5 s'}${k.control.opened ? '' : '; the control saw no menu either'}`);
  }
  const d = k.double;
  check('a double-click on the caption maximizes the window, and a second one restores it', !!d && d.maximized && d.restored,
    d ? `${d.name}: ` + d.clicks.map((c, i) => `${i ? 'second' : 'first'} on ${c.hit.cls}=${c.hit.ht}: zoomed ${c.zoomed} after ${c.ms} ms ${J(c.rect)}`).join('; ') + `; before ${J(d.rectBefore)}` : 'no double-click point');
} catch (e) {
  check('ran to the end', false, String(e));
} finally {
  try { await b?.eval(`window.__TAURI_INTERNALS__.invoke('plugin:process|exit', { code: 0 })`); } catch { /* the page goes with it */ }
  try { await b?.close(); } catch {}
  if (app && !await until(async () => app.exitCode !== null, 10000)) {
    spawnSync('taskkill', ['/pid', String(app.pid), '/T', '/F']);
    await until(async () => app.exitCode !== null, 10000);
  }
  await until(async () => { try { await fetch(`http://127.0.0.1:${CDP}/json/version`); return false; } catch { return true; } }, 10000);
  try { wipe(); } catch (e) { console.log('INFO cleanup — ' + String(e)); }
  check('cleanup: the e2e identity and folders are gone', !credExists() && !existsSync(DATA) && !existsSync(WEBVIEW));
}
const passed = results.filter(Boolean).length;
console.log(`${passed}/${results.length} passed`);
writeFileSync(join(OUT, 'result.json'), J({ passed, total: results.length, ...report }, null, 2));
process.exit(passed === results.length ? 0 : 1);
