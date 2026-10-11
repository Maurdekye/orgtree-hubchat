// Proof against the browser mock with deferred update responses (?defer=1,
// lib/updates.ts updateSeam): every desktop install, wherever it starts, goes
// through one guarded state (lead-astra review 2026-10-11 23:26Z).
// 1. An update is known and a re-check (the tray's) is still running: the
//    banner can't start an install, and the finished check opens no door to
//    one either.
// 2. An install is running (the tray's): the banner shows it and starts no
//    second one; when it fails, the banner can be used again (not stuck).
//
//   npx vite --port 14731 --strictPort      (in another shell)
//   node tools/proof-update-races.mjs http://127.0.0.1:14731 <shots-dir>
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { launch } from './cdp.mjs';

const [base, shots] = process.argv.slice(2);
if (!base || !shots) { console.error('usage: node tools/proof-update-races.mjs <base-url> <shots-dir>'); process.exit(2); }
mkdirSync(shots, { recursive: true });
const results = [];
const check = (name, ok, detail = '') => { results.push({ name, ok }); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };

const b = await launch({ width: 1180, height: 760 });
const seam = (expr) => b.eval(`(() => { const s = window.hcUpdateSeam; return ${expr}; })()`);
const settle = async (kind, value) => {
  const ok = await b.eval(`(() => { const f = window.hcUpdateSeam?.${kind}.shift(); if (!f) return false; f(${JSON.stringify(value)}); return true; })()`);
  if (!ok) throw new Error(`nothing waiting in ${kind}`);
  await b.sleep(300);
};
// what the tray asks for, as lib/tray.ts does it
const tray = (fn) => b.eval(`import('/src/lib/updates.ts').then((m) => { m.${fn}().catch(() => {}); return true; })`);
const banner = () => b.eval(`(() => { const e = document.querySelector('.banner.upd .btn.primary'); return e ? { text: e.innerText.trim(), disabled: e.disabled } : null; })()`);
const clickBanner = () => b.eval(`(() => { const e = document.querySelector('.banner.upd .btn.primary'); if (!e) return false; e.click(); return true; })()`);
const show = (x) => JSON.stringify(x);

try {
  // ----------------------------------- 1. a re-check while an update is known
  await b.nav(base + '/?defer=1', 1500);
  check('the background check waits for the page', (await seam('s.checks.length')) === 1);
  await settle('checks', '1');
  let bn = await banner();
  check('the found update is offered on the banner', bn?.text === 'Restart to update' && bn.disabled === false, show(bn));
  await tray('checkForUpdate');
  await b.sleep(200);
  bn = await banner();
  check('during a re-check the banner can\'t install', bn?.disabled === true, show(bn));
  await clickBanner();
  await b.sleep(200);
  check('a banner click during the re-check starts no install', (await seam('s.started')) === 0, 'started=' + (await seam('s.started')));
  await settle('checks', '1');
  bn = await banner();
  const started1 = await seam('s.started');
  check('the finished re-check leaves no install running and offers the update again', started1 === 0 && bn?.text === 'Restart to update' && bn.disabled === false,
    `started=${started1} banner=${show(bn)}`);
  await b.shot(join(shots, 'race1-after-recheck.png'));

  // ------------------------------------------ 2. an install the tray started
  await b.nav(base + '/?defer=1', 1500);
  await settle('checks', '1');
  await tray('installUpdate');
  await b.sleep(200);
  bn = await banner();
  check('the tray\'s install shows on the banner, which waits', (await seam('s.started')) === 1 && bn?.text === 'Updating…' && bn.disabled === true, show(bn));
  await clickBanner();
  await tray('installUpdate');
  await b.sleep(200);
  check('a second request starts no second install', (await seam('s.started')) === 1, 'started=' + (await seam('s.started')));
  await settle('installs', false);
  bn = await banner();
  check('after the install fails the banner can be used again', bn?.text === 'Restart to update' && bn.disabled === false, show(bn));
  await b.shot(join(shots, 'race2-after-failure.png'));
  const errors = b.errors.filter((e) => !/Download the React DevTools/.test(e));
  check('no console errors or exceptions', errors.length === 0, errors.join(' | '));
} finally {
  await b.close();
}
const failed = results.filter((r) => !r.ok).length;
console.log(`${results.length - failed}/${results.length} passed`);
process.exit(failed ? 1 : 0);
