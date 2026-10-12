// Proof against the browser mock (no Tauri, no tray): Settings shows the
// same settings and updater state as the tray (docs/tray-menu.md).
// - Settings › General: Exit on close is a switch, off at first, with words
//   for each state, and it is kept; Start with Windows still switches.
// - Settings › About shows the updater's shared state: a failed background
//   check stays silent, a failed check someone asked for shows, a found
//   update offers Restart to update, and an install shows "Downloading the
//   update…" and then its failure.
//
//   npx vite --host 127.0.0.1 --port 14731 --strictPort   (in another shell;
//   without --host, "localhost" may answer on IPv6 only)
//   node tools/proof-tray-settings.mjs http://127.0.0.1:14731 <shots-dir>
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { launch } from './cdp.mjs';

const [base, shots] = process.argv.slice(2);
if (!base || !shots) { console.error('usage: node tools/proof-tray-settings.mjs <base-url> <shots-dir>'); process.exit(2); }
mkdirSync(shots, { recursive: true });
const results = [];
const check = (name, ok, detail = '') => { results.push({ name, ok }); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };

const b = await launch({ width: 1180, height: 760 });
const tab = (name) => b.eval(`(() => { const e = [...document.querySelectorAll('.set-nav button')].find((x) => x.textContent.trim() === ${JSON.stringify(name)}); if (!e) return false; e.click(); return true; })()`);
const openSettings = async (name) => {
  await b.click('button[aria-label="Settings"]', 400);
  if (!(await tab(name))) throw new Error('no Settings tab ' + name);
  await b.sleep(300);
};
const rowText = (label) => b.eval(`(() => { const s = document.querySelector('[aria-label=${JSON.stringify(label)}]'); return s ? s.closest('.set-row').innerText : null; })()`);
const checked = (label) => b.eval(`document.querySelector('[aria-label=${JSON.stringify(label)}]')?.getAttribute('aria-checked')`);
const status = () => b.text('.about-upd .help');
const statusBad = () => b.eval(`document.querySelector('.about-upd .help')?.classList.contains('bad') ?? null`);
const button = () => b.eval(`(() => { const e = document.querySelector('.about-upd button'); return e ? { text: e.innerText.trim(), disabled: e.disabled } : null; })()`);

try {
  // ------------------------------------------------ Settings › General
  await b.nav(base + '/', 1500);
  await openSettings('General');
  let t = await rowText('Exit on close');
  check('General has an Exit on close switch, off at first', (await checked('Exit on close')) === 'false', t);
  check('off: closing keeps Hubchat in the tray, and Quit Hubchat is named', /keeps Hubchat running in the tray/.test(t) && /choose Quit Hubchat/.test(t));
  await b.shot(join(shots, 'general-off.png'), '.modal.settings');
  await b.click('[aria-label="Exit on close"]', 300);
  t = await rowText('Exit on close');
  check('on: the switch says so and closing quits', (await checked('Exit on close')) === 'true' && /Closing the window quits Hubchat/.test(t), t);
  check('the mock keeps it where the shell would', (await b.eval(`localStorage.getItem('hubchat.mock.exit_on_close')`)) === '1');
  await b.shot(join(shots, 'general-on.png'), '.modal.settings');
  const before = await checked('Start with Windows');
  await b.click('[aria-label="Start with Windows"]', 300);
  check('Start with Windows still switches (the value in effect comes back)', before === 'false' && (await checked('Start with Windows')) === 'true'
    && (await b.eval(`localStorage.getItem('hubchat.mock.autostart')`)) === '1');
  await b.nav(base + '/', 1500);
  await openSettings('General');
  check('both are kept after a reload', (await checked('Exit on close')) === 'true' && (await checked('Start with Windows')) === 'true');

  // ------------------------------------- Settings › About: failed checks
  await b.nav(base + '/?update=down', 2200); // the background check fails
  await openSettings('About');
  check('a failed background check stays silent', (await status()) === 'Hubchat checks for updates when it starts and every 6 hours.' && (await statusBad()) === false, await status());
  await b.click('.about-upd button', 150);
  check('a check asked for says it is checking, and its button waits', (await status()) === 'Checking…' && (await button()).disabled === true, await status());
  await b.sleep(1100);
  check('a failed check asked for shows its failure', (await status()) === "Couldn't check for updates: the update feed can't be reached" && (await statusBad()) === true, await status());
  await b.shot(join(shots, 'about-check-failed.png'), '.modal.settings');

  // --------------------------------------- Settings › About: an install
  await b.nav(base + '/?update=1&install=fail', 2200); // the background check finds 0.2.0
  await openSettings('About');
  check('a found update is offered', (await status()) === 'Hubchat 0.2.0 is available.' && (await button()).text === 'Restart to update', await status());
  await b.click('.about-upd button', 200);
  check('an install shows it is under way, and its button waits', (await status()) === 'Downloading the update…' && (await button()).disabled === true, await status());
  await b.shot(join(shots, 'about-installing.png'), '.modal.settings');
  await b.sleep(1800);
  check('a failed install says so and can be tried again', (await status()) === "The update didn't install: the download stopped" && (await statusBad()) === true
    && (await button()).text === 'Restart to update' && (await button()).disabled === false, await status());
  await b.shot(join(shots, 'about-install-failed.png'), '.modal.settings');
  const errors = b.errors.filter((e) => !/Download the React DevTools/.test(e));
  check('no console errors or exceptions', errors.length === 0, errors.join(' | '));
} finally {
  await b.close();
}
const failed = results.filter((r) => !r.ok).length;
console.log(`${results.length - failed}/${results.length} passed`);
process.exit(failed ? 1 : 0);
