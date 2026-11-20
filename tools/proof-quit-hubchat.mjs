// Proof against the browser mock (no Tauri): desktop Settings has an in-app
// "Quit Hubchat" (docs/tray-menu.md), for a desktop without a tray.
// - it is the last entry of the Settings nav, under a separator, after About
// - it is a plain button, not one of the section tabs
// - a click asks the shell to quit, with no question (the mock toasts)
//
//   npx vite --port 14733 --strictPort      (in another shell)
//   node tools/proof-quit-hubchat.mjs http://127.0.0.1:14733 <shots-dir>
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { launch } from './cdp.mjs';

const [base, shots] = process.argv.slice(2);
if (!base || !shots) { console.error('usage: node tools/proof-quit-hubchat.mjs <base-url> <shots-dir>'); process.exit(2); }
mkdirSync(shots, { recursive: true });
const results = [];
const check = (name, ok, detail = '') => { results.push({ name, ok }); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };

const b = await launch({ width: 1180, height: 760 });
// the nav's children in order: buttons by their text, a separator as "---"
const nav = () => b.eval(`[...document.querySelectorAll('.set-nav button, .set-nav [role=separator]')].map((x) => x.getAttribute('role') === 'separator' ? '---' : x.textContent.trim())`);

try {
  await b.nav(base + '/', 1500);
  // a cold dev server takes a while to hand out the app
  for (let i = 0; i < 80 && !(await b.eval(`!!document.querySelector('button[aria-label="Settings"]')`)); i++) await b.sleep(250);
  await b.click('button[aria-label="Settings"]', 400);
  const names = await nav();
  const n = names.length;
  check('Quit Hubchat is the last entry of the Settings nav, under a separator, after About',
    n > 3 && names[n - 1] === 'Quit Hubchat' && names[n - 2] === '---' && names[n - 3] === 'About', names.join(' | '));
  check('it is a button outside the section tabs', await b.eval(`(() => { const q = [...document.querySelectorAll('.set-nav button')].find((x) => x.textContent.trim() === 'Quit Hubchat'); return !!q && q.getAttribute('role') !== 'tab' && !q.closest('[role=tablist]'); })()`));
  await b.shot(join(shots, 'quit-hubchat.png'), '.modal.settings');
  await b.eval(`[...document.querySelectorAll('.set-nav button')].find((x) => x.textContent.trim() === 'Quit Hubchat')?.click()`);
  await b.sleep(300);
  const body = await b.eval(`document.body.innerText`);
  check('a click asks the shell to quit (the mock toasts)', /\[Quits Hubchat\]/.test(body));
  check('with no question first', !(await b.eval(`!![...document.querySelectorAll('.modal')].some((m) => !m.classList.contains('settings'))`)));
  const errors = b.errors.filter((e) => !/Download the React DevTools/.test(e));
  check('no console errors or exceptions', errors.length === 0, errors.join(' | '));
} finally {
  await b.close();
}
const failed = results.filter((r) => !r.ok).length;
console.log(`${results.length - failed}/${results.length} passed`);
process.exit(failed ? 1 : 0);
