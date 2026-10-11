// Proof against the browser mock (no Tauri): Settings › Developer (desktop)
// holds the verbose logging switch and the logs folder row (docs/logging.md).
// - the tab is in the nav, between General and Help
// - Verbose logging is off at first, says what it adds and what it never does,
//   and switches on and off (the shell would keep it in verbose-logging)
// - Open logs folder says how much the log holds and that it stays on the device
//
//   npx vite --port 14732 --strictPort      (in another shell)
//   node tools/proof-developer-tab.mjs http://127.0.0.1:14732 <shots-dir>
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { launch } from './cdp.mjs';

const [base, shots] = process.argv.slice(2);
if (!base || !shots) { console.error('usage: node tools/proof-developer-tab.mjs <base-url> <shots-dir>'); process.exit(2); }
mkdirSync(shots, { recursive: true });
const results = [];
const check = (name, ok, detail = '') => { results.push({ name, ok }); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };

const b = await launch({ width: 1180, height: 760 });
const tabs = () => b.eval(`[...document.querySelectorAll('.set-nav button')].map((x) => x.textContent.trim())`);
const tab = (name) => b.eval(`(() => { const e = [...document.querySelectorAll('.set-nav button')].find((x) => x.textContent.trim() === ${JSON.stringify(name)}); if (!e) return false; e.click(); return true; })()`);
const rowText = (sel) => b.eval(`(() => { const s = document.querySelector(${JSON.stringify(sel)}); return s ? s.closest('.set-row').innerText : null; })()`);
const checked = (label) => b.eval(`document.querySelector('[aria-label=${JSON.stringify(label)}]')?.getAttribute('aria-checked')`);

try {
  await b.nav(base + '/', 1500);
  await b.click('button[aria-label="Settings"]', 400);
  const names = await tabs();
  const i = names.indexOf('Developer');
  check('Developer is in the Settings nav between General and Help', i > 0 && names[i - 1] === 'General' && names[i + 1] === 'Help', names.join(' | '));
  check('the tab opens', await tab('Developer'));
  await b.sleep(400);
  check('the page is titled Developer', (await b.eval(`document.querySelector('.set-main .modal-h h3')?.textContent`)) === 'Developer');
  let t = await rowText('[aria-label="Verbose logging"]');
  check('Verbose logging is off at first', (await checked('Verbose logging')) === 'false', t);
  check('it says what it adds and what it never does', /step-by-step lines/.test(t) && /Never message text, addresses or keys/.test(t) && /stays after a restart/.test(t), t);
  await b.shot(join(shots, 'developer-off.png'), '.modal.settings');
  await b.click('[aria-label="Verbose logging"]', 300);
  check('it switches on', (await checked('Verbose logging')) === 'true');
  await b.shot(join(shots, 'developer-on.png'), '.modal.settings');
  await b.click('[aria-label="Verbose logging"]', 300);
  check('and off again', (await checked('Verbose logging')) === 'false');
  const folder = await b.eval(`[...document.querySelectorAll('.set-row')].find((r) => /Open logs folder/.test(r.innerText))?.innerText ?? null`);
  check('Open logs folder says how much the log holds, its limits, and that it never leaves the device',
    /3 files, 1\.2 MB/.test(folder ?? '') && /up to 64 MB, kept 30 days/.test(folder ?? '') && /never sent anywhere/.test(folder ?? ''), folder);
  await b.eval(`[...document.querySelectorAll('.set-row button')].find((x) => /Open/.test(x.innerText)).click()`);
  await b.sleep(300);
  check('Open asks for the folder (the mock toasts)', /Opens the logs folder/.test(await b.eval(`document.body.innerText`)));
  const errors = b.errors.filter((e) => !/Download the React DevTools/.test(e));
  check('no console errors or exceptions', errors.length === 0, errors.join(' | '));
} finally {
  await b.close();
}
const failed = results.filter((r) => !r.ok).length;
console.log(`${results.length - failed}/${results.length} passed`);
process.exit(failed ? 1 : 0);
