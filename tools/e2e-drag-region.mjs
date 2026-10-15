// The title bar is a native caption (user 2026-10-14 22:31Z: Hubchat
// 1.2.0-alpha.5's window could not be dragged by its title bar). Checked in
// the real WebView2 of a desktop test build, without a mouse: agents run in a
// non-interactive window station, where Windows cannot move a window at all
// (GetCursorPos fails with error 1459 and a posted WM_NCLBUTTONDOWN starts no
// move loop, measured in rig round 446), so the drag itself is the user's check.
//   - computed style: .titlebar app-region drag, .window-controls no-drag
//   - Windows' hit test (WM_NCHITTEST, tools/e2e-drag-region.ps1) as Windows
//     makes it: the window under the point (from the main window down, each
//     time the topmost child that takes the point) answers HTCAPTION at the
//     bar's empty part, the name, the logo and the version; at the three window
//     buttons and in the window body it doesn't, and no window that takes the
//     point does. WebView2 makes a drag region out of child windows of its own,
//     on the app's UI thread, that answer HTCAPTION wherever they are asked, so
//     only windows that take the point count (rig round 447).
// The red control (the base build) shows no caption anywhere.
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

// only this build's own data, by exact name
if (!ID.endsWith('.test') || ID === 'dev.orgtree.hubchat.test') throw new Error('not the e2e identifier: ' + ID);
const credExists = () => /Target:/.test(sh('cmdkey', [`/list:${CRED}`]));
const wipe = () => {
  if (credExists()) sh('cmdkey', [`/delete:${CRED}`]);
  for (const p of [DATA, WEBVIEW]) { if (!p.endsWith(`\\${ID}`)) throw new Error('not the e2e folder: ' + p); rmSync(p, { recursive: true, force: true }); }
};

let app = null;
let b = null;
const report = { styles: null, hittest: null };
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
    return { titlebar: st('.titlebar'), controls: st('.titlebar .window-controls') };
  })()`);
  const is = (v, want) => v.webkit === want || v.plain === want;
  check('the title bar is app-region drag', is(report.styles.titlebar, 'drag'), J(report.styles.titlebar));
  check('the window buttons are app-region no-drag', is(report.styles.controls, 'no-drag'), J(report.styles.controls));

  // points on the bar and below it, in page coordinates
  const pts = await b.eval(`(() => {
    const c = (sel, i = 0) => { const e = document.querySelectorAll(sel)[i]; const r = e.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; };
    const body = document.querySelector('.win-body').getBoundingClientRect();
    return { dpr: devicePixelRatio, points: [
      { name: 'bar: empty part', drag: true, ...c('.titlebar .tb-fill') },
      { name: 'bar: the name', drag: true, ...c('.titlebar .tb-brand [role=heading]') },
      { name: 'bar: the logo', drag: true, ...c('.titlebar .tb-brand svg') },
      { name: 'bar: the version', drag: true, ...c('.titlebar .ver') },
      { name: 'button: minimize', drag: false, ...c('.titlebar .window-control', 0) },
      { name: 'button: maximize', drag: false, ...c('.titlebar .window-control', 1) },
      { name: 'button: close', drag: false, ...c('.titlebar .window-control', 2) },
      { name: 'window body', drag: false, x: body.left + Math.min(200, body.width / 2), y: body.top + Math.min(120, body.height / 2) },
    ] };
  })()`);
  const ptsFile = join(OUT, 'points.json');
  writeFileSync(ptsFile, J(pts, null, 2));
  const ansFile = join(OUT, 'hittest.json');
  const ps = spawnSync('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', join(import.meta.dirname, 'e2e-drag-region.ps1'),
    '-TargetPid', String(app.pid), '-Points', ptsFile, '-Out', ansFile], { encoding: 'utf8', timeout: 60000 });
  if (!existsSync(ansFile)) throw new Error('no hit test answers: ' + (ps.stderr || ps.stdout || '').slice(0, 800));
  report.hittest = JSON.parse(readFileSync(ansFile, 'utf8'));
  if (report.hittest.error) throw new Error('hit test: ' + report.hittest.error);
  for (const [i, p] of report.hittest.points.entries()) {
    const want = pts.points[i].drag;
    const caption = p.answers.filter((a) => a.ht === HTCAPTION);
    const all = `under the point ${p.hit.cls} ${p.hit.hwnd}=${p.hit.ht}; taking it: ` + p.answers.map((a) => `${a.cls}=${a.ht}`).join(' ');
    check(`${p.name}: ${want ? 'Windows sees a caption' : 'no caption'}`,
      want ? p.hit.ht === HTCAPTION : p.hit.ht !== HTCAPTION && caption.length === 0, all);
  }
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
