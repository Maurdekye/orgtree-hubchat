// The lost key on Windows (user 2026-10-10 08:05Z: Windows lost every saved
// sign-in after a crash, and Hubchat on the PC started over without a word),
// with a Hubchat Test build: identifier dev.orgtree.hubchat.test, so its own
// data folder and its own Credential Manager entry. The real Hubchat's entry
// and folders are never touched: the test stops unless the app says it is the
// test build, deletes only the test's credential and folders, and checks the
// real entry, the hubchat:// handler and the Run key are as they were. The
// app starts hidden (tray only) and is driven through WebView2's debug port;
// it is stopped by its process id only (the real Hubchat is hubchat.exe too).
//   0. a throwaway identity X, only for its recovery words; then a fresh profile
//   1. identity Y with a made-up hub: its key goes to Credential Manager and to
//      Hubchat's DPAPI backup; a backup left from X is replaced at the next start
//   2. Credential Manager loses Y's entry: Hubchat restores it from the backup
//      by itself, says so once, and the entry is back
//   3. both copies gone: the lost-key screen names Y, not the welcome screen
//   4. Y's recovery words: the same identity, with its hub ("Welcome back")
//   5. both gone again, and X's words: another identity; Y's local data goes first
//   6. both gone again: Start over removes the local data, then the welcome screen
//   7. the key log tells each step, and never holds the key
//   node tools/e2e-desktop-lostkey.mjs <Hubchat Test exe> <shots dir>
import { spawn, spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { attach } from './cdp.mjs';

const [EXE, shots] = process.argv.slice(2);
if (!EXE || !shots) { console.error('usage: node tools/e2e-desktop-lostkey.mjs <Hubchat Test exe> <shots dir>'); process.exit(2); }
mkdirSync(shots, { recursive: true });
const ID = 'dev.orgtree.hubchat.test';
const DATA = join(process.env.APPDATA, ID);
const WEBVIEW = join(process.env.LOCALAPPDATA, ID);
const TARGET = `identity:${DATA}.dev.orgtree.hubchat`;
const BACKUP = join(DATA, 'identity-backup.dpapi');
const LOG = join(DATA, 'identity-log.txt');
// X's sealed backup, kept aside to play a backup left from another identity
const X_BACKUP = join(tmpdir(), 'hubchat-test-x-backup.dpapi');
const FAKE_HUB = 'http://127.0.0.1:7499';
const PORT = 9231;
const J = JSON.stringify;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };
const info = (name, detail) => console.log(`INFO ${name} — ${detail}`);
const until = async (f, ms) => { const t = Date.now(); while (Date.now() - t < ms) { if (await f()) return true; await sleep(300); } return false; };
const sh = (cmd, args) => spawnSync(cmd, args, { encoding: 'utf8' }).stdout || '';

// Credential Manager, by exact target name only, and only the test's
if (!TARGET.startsWith('identity:') || !TARGET.includes(`\\${ID}.dev.orgtree.hubchat`)) throw new Error('not the test target: ' + TARGET);
const credExists = () => /Target:/.test(sh('cmdkey', [`/list:${TARGET}`]));
const credDelete = () => { if (credExists()) sh('cmdkey', [`/delete:${TARGET}`]); return !credExists(); };
const realEntries = () => sh('cmdkey', ['/list']).split('\n').filter((l) => /Target:.*identity:.*\\dev\.orgtree\.hubchat\.dev\.orgtree\.hubchat/i.test(l)).length;
const reg = (key, value) => sh('reg', ['query', key, ...(value ? ['/v', value] : ['/ve'])]).trim();
const wipeProfile = () => {
  for (const p of [DATA, WEBVIEW]) { if (!p.endsWith(`\\${ID}`)) throw new Error('not a test folder: ' + p); rmSync(p, { recursive: true, force: true }); }
};
const logLines = () => (existsSync(LOG) ? readFileSync(LOG, 'utf8').trim().split('\n') : []);

// a fresh profile counts as the first start of this version, which shows the
// window despite --hidden (the update-restart fix): note this version as run
const VERSION = JSON.parse(readFileSync(join(import.meta.dirname, '..', 'src-tauri', 'tauri.conf.json'), 'utf8')).version;
const quiet = () => { mkdirSync(DATA, { recursive: true }); writeFileSync(join(DATA, 'last-run-version'), VERSION); };

let app = null;
let b = null;
const start = async () => {
  quiet();
  app = spawn(EXE, ['--hidden'], { stdio: 'ignore', env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` } });
  b = null;
  for (let i = 0; i < 60 && !b; i++) { try { b = await attach(PORT); } catch { await sleep(500); } }
  if (!b) throw new Error('could not attach to Hubchat Test');
  // the debug port answers before the app's page has loaded: wait for Tauri's bridge
  if (!await until(() => b.eval(`typeof window.__TAURI_INTERNALS__?.invoke === 'function'`).catch(() => false), 20000)) { await stop(); throw new Error("Hubchat Test's page never loaded"); }
  const id = await b.eval(`window.__TAURI_INTERNALS__.invoke('plugin:app|identifier')`);
  if (id !== ID) { await stop(); throw new Error('this is not the test build: ' + id); }
  await until(() => b.eval(`window.__TAURI_INTERNALS__.invoke('hc_state').then(() => true)`).catch(() => false), 20000);
  await sleep(1500);
};
const stop = async () => {
  try { await b?.close(); } catch {}
  b = null;
  if (app && app.exitCode === null) {
    spawnSync('taskkill', ['/pid', String(app.pid), '/T', '/F']);
    await until(() => app.exitCode !== null, 10000);
  }
  app = null;
  await until(async () => { try { await fetch(`http://127.0.0.1:${PORT}/json/version`); return false; } catch { return true; } }, 10000);
};
const inv = (cmd, args = {}) => b.eval(`window.__TAURI_INTERNALS__.invoke(${J(cmd)}, ${J(args)})`);
const state = () => inv('hc_state');
const text = async () => (await b.eval(`document.body.innerText`)).replace(/\s+/g, ' ');
const has = (s, ms = 10000) => until(async () => (await text().catch(() => '')).includes(s), ms);
const click = (sel, t) => b.eval(`(() => { const e = [...document.querySelectorAll(${J(sel)})].find((e) => e.innerText.replace(/\\s+/g, ' ').includes(${J(t)})); if (e) e.click(); return !!e; })()`);
const typeWords = (w) => b.eval(`(() => { const a = document.querySelector('textarea'); const set = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set; set.call(a, ${J(w)}); a.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
const shot = async (name) => { try { await b.shot(join(shots, name)); } catch { /* a hidden window may not paint */ } };
const loseBoth = () => { credDelete(); rmSync(BACKUP, { force: true }); return !credExists() && !existsSync(BACKUP); };

const before = { real: realEntries(), scheme: reg('HKCU\\Software\\Classes\\hubchat\\shell\\open\\command'), run: reg('HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run', 'Hubchat Test') };
try {
  info('the real Hubchat entry before', `${before.real} entry`);
  // 0. a throwaway identity, for another identity's words
  credDelete(); wipeProfile();
  await start();
  check('0. the app says it is the test build', true, ID);
  const addrX = await inv('hc_create_identity', { id: 'otherkey', name: 'Other Key' });
  const wordsX = (await inv('hc_recovery_words')).join(' ');
  await stop();
  copyFileSync(BACKUP, X_BACKUP);
  credDelete(); wipeProfile();

  // 1. identity Y, with a made-up hub
  await start();
  const addrY = await inv('hc_create_identity', { id: 'losttest', name: 'Lost Test' });
  await inv('hc_add_hub', { input: FAKE_HUB });
  const wordsY = (await inv('hc_recovery_words')).join(' ');
  check('1. Y is made and its key is in Credential Manager', !!addrY && credExists(), addrY);
  check("1. ...and in Hubchat's DPAPI backup", existsSync(BACKUP), BACKUP);
  await stop();

  // 1b. a backup left from X (as when Y's couldn't be written): replaced at start
  copyFileSync(X_BACKUP, BACKUP);
  await start();
  check("1b. with X's backup in place, Hubchat starts as Y", (await state()).me?.address === addrY);
  await stop();
  check("1b. ...and X's backup was replaced", !readFileSync(BACKUP).equals(readFileSync(X_BACKUP)));

  // 2. Credential Manager loses it: restored from the backup, said once
  check("2. Y's Credential Manager entry is deleted (the test's only)", credDelete());
  await start();
  let s = await state();
  check('2. Hubchat starts as Y all the same', s.me?.address === addrY, J(s.me));
  check('2. ...the entry is back in Credential Manager', credExists());
  check('2. ...and it says so once', s.key_restored === true && await has("Windows had lost Hubchat's saved key."), (await text()).slice(0, 200));
  await shot('lk-2-restored.png');
  await b.eval(`[...document.querySelectorAll('[aria-label="Key notice"] button')].find((x) => x.innerText.trim() === 'OK').click(), true`);
  check('2. OK, and the notice is gone', await until(async () => (await state()).key_restored === false, 5000));
  await stop();

  // 3. both copies gone: the lost-key screen, naming Y
  check('3. both copies are gone (test entry and test backup)', loseBoth());
  await start();
  s = await state();
  check('3. no identity, but Hubchat knows it lost Y\'s key', !s.me && s.key_lost?.address === addrY && s.key_lost?.unreadable === false, J(s.key_lost));
  check('3. the lost-key screen shows, not the welcome', await has('Hubchat lost its key on this PC') && (await text()).includes(addrY) && !(await text()).includes('Create a new identity'), (await text()).slice(0, 240));
  await shot('lk-3-lost.png');

  // 4. Y's own words: the same identity, with its hub
  await click('.method', 'Recovery words');
  await until(() => b.eval(`!!document.querySelector('textarea')`), 5000);
  await typeWords(wordsY); await sleep(200);
  await click('.ob-foot button', 'Continue');
  check('4. Y\'s words bring Y back', await until(async () => (await state()).me?.address === addrY, 15000));
  s = await state();
  check('4. ...with its hub: the local data was kept', s.hubs.some((h) => h.url === FAKE_HUB), J(s.hubs.map((h) => h.url)));
  check('4. ...straight into the app (no "Add your hubs")', await until(async () => !(await text()).includes('Add your hubs') && !(await text()).includes('lost its key'), 8000), (await text()).slice(0, 160));
  check('4. the key is kept again in both places', credExists() && existsSync(BACKUP));
  await stop();

  // 5. both gone again, and X's words: Y's local data goes first
  loseBoth();
  await start();
  await has('Hubchat lost its key on this PC');
  await click('.method', 'Recovery words');
  await until(() => b.eval(`!!document.querySelector('textarea')`), 5000);
  await typeWords(wordsX); await sleep(200);
  await click('.ob-foot button', 'Continue');
  check("5. X's words bring X", await until(async () => (await state()).me?.address === addrX, 15000));
  s = await state();
  check("5. ...and nothing of Y's: its hub is gone", !s.hubs.some((h) => h.url === FAKE_HUB), J(s.hubs.map((h) => h.url)));
  check('5. ...so Hubchat asks for hubs', await has('Add your hubs', 8000));
  await stop();

  // 6. both gone again: Start over
  loseBoth();
  await start();
  check('6. the lost-key screen, now naming X', await has('Hubchat lost its key on this PC') && (await text()).includes(addrX));
  await click('.method', 'Start over with a new identity');
  check('6. Start over asks first', await has('Start over with a new identity?'));
  await click('button', 'Start over');
  check('6. ...then the welcome screen', await has('Create a new identity', 8000));
  s = await state();
  check('6. no identity, nothing lost any more, no key anywhere', !s.me && !s.key_lost && !credExists() && !existsSync(BACKUP), J({ me: s.me, lost: s.key_lost }));
  await stop();

  // 7. the key log
  const lines = logLines();
  const want = ['key saved: a new identity', 'key found in Credential Manager; the backup held another key; a new one was written',
    "no key in Credential Manager; key restored from Hubchat's backup, put back into Credential Manager",
    'no key in Credential Manager; no backup', 'key saved: an identity brought to this device',
    "another identity's local data was removed before taking this one", 'started over: the local data of the lost identity was removed'];
  for (const w of want) check(`7. the key log says "${w}"`, lines.some((l) => l.endsWith(' ' + w)));
  check('7. the key log holds no key and no address', !lines.some((l) => [addrX, addrY, ...wordsY.split(' ').slice(0, 3)].some((x) => l.includes(x))));
  writeFileSync(join(shots, 'identity-log.txt'), lines.join('\n') + '\n');
} catch (e) {
  check('no error', false, String(e && e.stack || e));
} finally {
  await stop();
  credDelete();
  wipeProfile();
  rmSync(X_BACKUP, { force: true });
  const after = { real: realEntries(), scheme: reg('HKCU\\Software\\Classes\\hubchat\\shell\\open\\command'), run: reg('HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run', 'Hubchat Test') };
  check("the real Hubchat's Credential Manager entry is as it was", after.real === before.real, `${before.real} -> ${after.real}`);
  check('the hubchat:// handler and the Run key are as they were', after.scheme === before.scheme && after.run === before.run);
  check('the test profile is gone (entry and folders)', !credExists() && !existsSync(DATA) && !existsSync(WEBVIEW));
  const pass = results.filter(Boolean).length;
  console.log(`${pass}/${results.length} passed`);
  process.exit(pass === results.length ? 0 : 1);
}
