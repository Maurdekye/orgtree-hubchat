// A hub with a certificate from its own certificate authority (CA), on the emulator, in
// Hubchat Test (hubchat_core::tls; Android hands its certificate store to the core through
// TrustedRoots.kt):
//   1. the CA is not on the device: "Add a hub" with the hub's https address says that this
//      device does not trust the hub's certificate, and the hub sees no finished TLS handshake;
//   2. the CA is installed as a USER certificate, as Settings > Security > "Install a
//      certificate" does, with Hubchat still running: the same check connects, no restart;
//   3. a hub with a certificate from another CA is still refused.
// The hub is a stand-in on this PC that speaks HTTPS with the throwaway certificates of
// crates/hubchat-core/tests/tls and answers /healthz; the emulator reaches it by adb reverse,
// as https://127.0.0.1:<port> (the certificate is for that IP address).
// The CA is installed by writing the file Android's own installer writes
// (/data/misc/user/0/cacerts-added/<subject hash>.0), which needs `adb root`: an emulator
// image WITHOUT Google Play. It is removed at the end. The app's data is cleared at the start.
// EMULATORS ONLY: the script stops on anything else, so it can never clear a person's Hubchat
// or put a test CA on a real phone. Every adb call names its device (ANDROID_SERIAL).
//   needs: ANDROID_SERIAL (an emulator), adb on PATH or in ADB; a build of this commit installed:
//   Hubchat Test (dev.orgtree.hubchat.test), or with HUBCHAT_PKG=dev.orgtree.hubchat a debug or
//   CI build under the app's own name, on an emulator that is thrown away afterwards
//   node tools/e2e-android-trust.mjs <shots-dir>
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:https';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { attach } from './cdp.mjs';

const [shots] = process.argv.slice(2);
const SERIAL = process.env.ANDROID_SERIAL;
if (!SERIAL || !shots) {
  console.error('usage: ANDROID_SERIAL=<device> node tools/e2e-android-trust.mjs <shots-dir>');
  process.exit(2);
}
mkdirSync(shots, { recursive: true });
const ADB = process.env.ADB || 'adb';
const PKG = process.env.HUBCHAT_PKG || 'dev.orgtree.hubchat.test';
const TLS = join(dirname(fileURLToPath(import.meta.url)), '..', 'crates', 'hubchat-core', 'tests', 'tls');
// where Android keeps a user-installed CA: the old-style hash of its subject (`openssl x509 -subject_hash_old`)
const CA_ON_DEVICE = '/data/misc/user/0/cacerts-added/88d238ef.0';
const HUB_PORT = 7397, OTHER_PORT = 7398;
const HUB = `https://127.0.0.1:${HUB_PORT}`, OTHER = `https://127.0.0.1:${OTHER_PORT}`;
const UNTRUSTED = "this device does not trust the hub's certificate";
const J = JSON.stringify;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const adb = (...a) => spawnSync(ADB, ['-s', SERIAL, ...a], { encoding: 'utf8', maxBuffer: 64 << 20, env: { ...process.env, MSYS_NO_PATHCONV: '1' } }).stdout || '';
const shot = (name) => writeFileSync(join(shots, name), spawnSync(ADB, ['-s', SERIAL, 'exec-out', 'screencap', '-p'], { maxBuffer: 64 << 20 }).stdout);
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };
const until = async (f, ms) => { const t = Date.now(); while (Date.now() - t < ms) { if (await f()) return true; await sleep(500); } return false; };

/** A stand-in hub over HTTPS with the certificate `name`; counts the TLS handshakes it finished. */
const standIn = (name, port) => new Promise((resolve, reject) => {
  const seen = { handshakes: 0 };
  const server = createServer({ cert: readFileSync(join(TLS, name + '.pem')), key: readFileSync(join(TLS, name + '-key.pem')) }, (req, res) => {
    const body = req.url.startsWith('/healthz')
      ? J({ ok: true, name: 'trust-hub', orgs: 1, version: '2.5.1', features: ['sync'], max_attachment_bytes: 1000000 })
      : J({ detail: 'Not Found' });
    res.writeHead(req.url.startsWith('/healthz') ? 200 : 404, { 'content-type': 'application/json', 'content-length': Buffer.byteLength(body), connection: 'close' });
    res.end(body);
  });
  server.on('secureConnection', () => { seen.handshakes += 1; });
  server.on('tlsClientError', () => {}); // a client that refuses the certificate
  server.on('error', reject);
  server.listen(port, '127.0.0.1', () => resolve({ server, seen }));
});

let b;
const attachApp = async () => {
  let pid = '';
  for (let i = 0; i < 40 && !(pid = adb('shell', 'pidof', PKG).trim()); i++) await sleep(500);
  await until(() => adb('shell', 'cat', '/proc/net/unix').includes(`webview_devtools_remote_${pid}`), 30000);
  adb('forward', 'tcp:9338', `localabstract:webview_devtools_remote_${pid}`);
  try { await b?.close(); } catch {}
  b = null;
  for (let i = 0; i < 40 && !b; i++) { try { b = await attach(9338); } catch { await sleep(500); } }
  if (!b) throw new Error('could not attach to ' + PKG);
};
const top = () => (adb('shell', 'dumpsys', 'activity', 'activities').match(/topResumedActivity=\S+ \S+ (\S+?)\//) || [])[1] || '';
const launch = async () => {
  for (let i = 0; i < 6 && top() !== PKG; i++) {
    adb('shell', 'monkey', '-p', PKG, '-c', 'android.intent.category.LAUNCHER', '1');
    await until(() => top() === PKG, 5000);
  }
  await attachApp();
  await sleep(1500);
};
const inv = (cmd, args = {}) => b.eval(`window.__TAURI_INTERNALS__.invoke(${J(cmd)}, ${J(args)})`);
const waitFor = (expr, ms) => until(() => b.eval(expr).catch(() => false), ms);
const clickText = (sel, text) => b.eval(`(() => { const x = [...document.querySelectorAll(${J(sel)})].find((x) => x.innerText.includes(${J(text)})); if (x) x.click(); return !!x; })()`);
const card = () => b.eval(`(document.querySelector('.probe-card.ok, .probe-card.bad') || document.querySelector('.probe-card') || {}).innerText || ''`).then((t) => t.replace(/\s+/g, ' ').trim());
/** Type `address` in "Add a hub" and tap Check; the card that comes, as its words. */
const checkAddress = async (address) => {
  await b.eval(`(() => { const e = document.querySelector('#hub-in'); const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set; set.call(e, ''); e.dispatchEvent(new Event('input', { bubbles: true })); })()`);
  await b.type('#hub-in', address, 300);
  await clickText('button', 'Check');
  // the card while it asks has neither class
  await waitFor(`!!document.querySelector('.probe-card.ok, .probe-card.bad')`, 40000);
  return card();
};
const caOnDevice = () => adb('shell', 'ls', CA_ON_DEVICE).includes('88d238ef.0');
const removeCa = () => adb('shell', 'rm', '-f', CA_ON_DEVICE);

let hub, other;
/** past the emulator check: from here on the device is touched, and put back at the end */
let changed = false;
try {
  hub = await standIn('hub', HUB_PORT);
  other = await standIn('other-hub', OTHER_PORT);
  const emulator = /^emulator-\d+$/.test(SERIAL) && adb('shell', 'getprop', 'ro.kernel.qemu').trim() === '1';
  check('the device is an emulator', emulator, `${SERIAL}, ro.kernel.qemu=${adb('shell', 'getprop', 'ro.kernel.qemu').trim()}`);
  if (!emulator) throw new Error('emulators only: nothing was changed on this device');
  check(`${PKG} is installed`, adb('shell', 'pm', 'path', PKG).includes('package:'));
  adb('root');
  check('this emulator image allows adb root (no Google Play)', await until(() => adb('shell', 'id', '-u').trim() === '0', 20000), adb('shell', 'id').trim());
  changed = true;
  removeCa();
  check('the test CA is not on the device to begin with', !caOnDevice());
  adb('reverse', `tcp:${HUB_PORT}`, `tcp:${HUB_PORT}`);
  adb('reverse', `tcp:${OTHER_PORT}`, `tcp:${OTHER_PORT}`);
  adb('shell', 'pm', 'clear', PKG);
  adb('shell', 'pm', 'grant', PKG, 'android.permission.POST_NOTIFICATIONS');
  await launch();
  await inv('hc_create_identity', { id: 'trust', name: 'Trust Tester' });
  await inv('hc_recovery_saved');
  await b.eval('location.reload()').catch(() => {});
  await sleep(2500);
  await attachApp();
  // Settings > Hubs > Add a hub
  await waitFor(`!!document.querySelector('button[aria-label="Settings"]')`, 20000);
  await b.click('button[aria-label="Settings"]', 900);
  await clickText('.li', 'Hubs');
  await waitFor(`[...document.querySelectorAll('button')].some((x) => x.innerText.includes('Add a hub'))`, 10000);
  await clickText('button', 'Add a hub');
  check('"Add a hub" is on the screen', await waitFor(`!!document.querySelector('#hub-in')`, 20000), await b.eval(`document.body.innerText.slice(0, 200)`));

  // 1. the CA is not on the device
  const before = await checkAddress(HUB);
  check(`1. without the CA the hub is refused, and the card says why ("${UNTRUSTED}")`, before.includes("Can't reach that hub") && before.includes(UNTRUSTED), before);
  check('1. the hub saw no finished TLS handshake', hub.seen.handshakes === 0, J(hub.seen));
  const p1 = await inv('hc_probe_hub', { input: HUB });
  check('1. the check itself answers unreachable', p1.result === 'unreachable' && p1.error.includes(UNTRUSTED), J(p1));
  shot('trust-1-refused.png');

  // 2. the CA installed as a user certificate, Hubchat still running
  const pidBefore = adb('shell', 'pidof', PKG).trim();
  adb('shell', 'mkdir', '-p', dirname(CA_ON_DEVICE).replace(/\\/g, '/'));
  adb('push', join(TLS, 'ca.pem'), CA_ON_DEVICE);
  adb('shell', 'chown', '-R', 'system:system', '/data/misc/user/0/cacerts-added');
  adb('shell', 'chmod', '755', '/data/misc/user/0/cacerts-added');
  adb('shell', 'chmod', '644', CA_ON_DEVICE);
  adb('shell', 'restorecon', '-R', '/data/misc/user/0/cacerts-added');
  check('2. the test CA is now a user certificate of the device', caOnDevice(), adb('shell', 'ls', '-l', '/data/misc/user/0/cacerts-added').trim());
  await sleep(6000); // the core looks at the device's roots again at most every 5 s
  const after = await checkAddress(HUB);
  check('2. with the CA installed the same address connects', after.includes('Connected to trust-hub'), after);
  check('2. the hub finished a TLS handshake', hub.seen.handshakes >= 1, J(hub.seen));
  check('2. Hubchat was not restarted in between', adb('shell', 'pidof', PKG).trim() === pidBefore, `${pidBefore} -> ${adb('shell', 'pidof', PKG).trim()}`);
  const p2 = await inv('hc_probe_hub', { input: HUB });
  check('2. the check itself answers connected', p2.result === 'connected' && p2.name === 'trust-hub', J(p2));
  shot('trust-2-connected.png');

  // 3. another CA's certificate is still refused
  const p3 = await inv('hc_probe_hub', { input: OTHER });
  check('3. a hub with a certificate from another CA is still refused', p3.result === 'unreachable' && p3.error.includes(UNTRUSTED) && other.seen.handshakes === 0, J(p3) + ' ' + J(other.seen));
} catch (e) {
  check('the run finished', false, String(e?.stack || e));
} finally {
  try { await b?.close(); } catch {}
  if (changed) {
    removeCa();
    check('the test CA is removed from the device again', !caOnDevice());
    adb('reverse', '--remove', `tcp:${HUB_PORT}`);
    adb('reverse', '--remove', `tcp:${OTHER_PORT}`);
    adb('forward', '--remove', 'tcp:9338');
    adb('shell', 'pm', 'clear', PKG);
  }
  hub?.server.close();
  other?.server.close();
}
const failed = results.filter((ok) => !ok).length;
console.log(`${results.length - failed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);
