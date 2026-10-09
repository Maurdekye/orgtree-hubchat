// Pictures and files in our own messages sent from another device (user
// 2026-10-09 11:34Z), real apps on both sides: the desktop test build and
// Hubchat Test on the emulator (or a phone) become one identity on a v2
// scratch hub, then
//   1. the phone sends Pat a picture: the PC shows it in the bubble (not a
//      file chip), its preview comes from the hub, and it can be downloaded;
//   2. the PC sends Pat a picture and a text file: the phone shows the
//      picture in the bubble and downloads the file.
// The phone reaches the hub through adb reverse at the same address as the
// PC. Hubchat Test's data on the phone is cleared; run tools/e2e-reset.sh
// first for the desktop test build's. The real Hubchat is never touched.
//
//   needs: HUBCHAT_V2_HUB (a v2 hub on this PC, e.g. http://127.0.0.1:7397),
//   ANDROID_SERIAL, Hubchat Test installed, adb on PATH or in ADB
//   node tools/e2e-own-pictures.mjs <hubchat.exe test build> <shots-dir>
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { deflateSync, crc32 } from 'node:zlib';
import { attach } from './cdp.mjs';

/** A plain RGB PNG, w x h, one colour. */
function pngOf(w, h, rgb) {
  const row = w * 3 + 1;
  const raw = Buffer.alloc(row * h);
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) raw.set(rgb, y * row + 1 + x * 3);
  const chunk = (type, data) => {
    const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type), data]);
    const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(td) >>> 0);
    return Buffer.concat([len, td, crc]);
  };
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 2;
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}

const [exe, shots] = process.argv.slice(2);
const HUB = process.env.HUBCHAT_V2_HUB;
const SERIAL = process.env.ANDROID_SERIAL;
if (!exe || !shots || !HUB || !SERIAL) {
  console.error('usage: HUBCHAT_V2_HUB=http://127.0.0.1:<port> ANDROID_SERIAL=<device> node tools/e2e-own-pictures.mjs <hubchat.exe test build> <shots-dir>');
  process.exit(2);
}
mkdirSync(shots, { recursive: true });
const PORT = new URL(HUB).port;
const ADB = process.env.ADB || 'adb';
const PKG = 'dev.orgtree.hubchat.test';
const J = JSON.stringify;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const adb = (...a) => spawnSync(ADB, ['-s', SERIAL, ...a], { encoding: 'utf8', maxBuffer: 64 << 20, env: { ...process.env, MSYS_NO_PATHCONV: '1' } }).stdout?.trim() || '';
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };
const peerSlug = spawnSync('node', [join(import.meta.dirname, 'peer2.mjs'), HUB, 'whoami'], { encoding: 'utf8' }).stdout.trim().split('\n').pop();
const inv = (b, cmd, args = {}) => b.eval(`window.__TAURI_INTERNALS__.invoke(${J(cmd)}, ${J(args)})`);
const waitFor = async (b, expr, ms) => { const t = Date.now(); while (Date.now() - t < ms) { if (await b.eval(expr).catch(() => false)) return true; await sleep(400); } return false; };
const shot = (b, n) => Promise.race([b.shot(join(shots, n)).catch(() => {}), sleep(8000)]);
const chatOf = (b) => inv(b, 'hc_chat', { peer: peerSlug, beforeAt: null, beforeId: null, fromAt: null, fromId: null, limit: 20 });
// send from a device's own core, as its composer does: pictures are saved
// like a pasted one, other files go by path
const sendFrom = (b, body, files) => b.eval(`(async () => {
  const inv = window.__TAURI_INTERNALS__.invoke;
  const atts = [];
  for (const f of ${J(files)}) {
    const source = f.path || await inv('hc_save_pasted', new Uint8Array(f.bytes), { headers: { 'x-name': f.name } });
    atts.push({ name: f.name, bytes: f.size, source });
  }
  const id = crypto.randomUUID().replace(/-/g, '');
  await inv('hc_send', { msg: { id, peer: ${J(peerSlug)}, body: ${J('')} + ${J(body)}, attachments: atts } });
  return id;
})()`);
const openPat = (b) => b.eval(`(() => { const el = [...document.querySelectorAll('.crow')].find((x) => x.innerText.includes(${J(peerSlug)}) || x.innerText.includes('Pat Peer')); if (!el) return false; el.click(); return true; })()`);
const imgShown = (b, id, w) => waitFor(b, `[...document.querySelectorAll('[data-id=${J(id)}] .att-img img, .msg .att-img img')].some((i) => i.complete && i.naturalWidth === ${w})`, 30000);

const phonePng = pngOf(64, 48, [181, 59, 111]);
const pcPng = pngOf(80, 60, [59, 181, 111]);
const pcPngPath = join(shots, 'from-the-pc.png');
const pcTxtPath = join(shots, 'notes-from-the-pc.txt');
writeFileSync(pcPngPath, pcPng);
writeFileSync(pcTxtPath, 'Notes sent from the PC.\n');

adb('reverse', `tcp:${PORT}`, `tcp:${PORT}`);
adb('shell', 'pm', 'clear', PKG);
adb('shell', 'monkey', '-p', PKG, '-c', 'android.intent.category.LAUNCHER', '1');
await sleep(3500);
adb('forward', 'tcp:9334', `localabstract:webview_devtools_remote_${adb('shell', 'pidof', PKG)}`);
const app = spawn(exe, [], { stdio: 'ignore', env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9333' } });
let pc, phone;
try {
  for (let i = 0; i < 60 && !pc; i++) { try { pc = await attach(9333); } catch { await sleep(500); } }
  for (let i = 0; i < 40 && !phone; i++) { try { phone = await attach(9334); } catch { await sleep(500); } }
  await sleep(2500);
  check('the PC and the phone start without an identity', (await inv(pc, 'hc_state')).me === null && (await inv(phone, 'hc_state')).me === null);

  // one identity: the PC creates it, the phone links through the hub
  const A = await inv(pc, 'hc_create_identity', { id: 'own', name: 'Own Tester' });
  await inv(pc, 'hc_add_hub', { input: HUB });
  await inv(pc, 'hc_recovery_saved');
  await pc.eval('location.reload()').catch(() => {}); await sleep(2500);
  pc = await attach(9333);
  const start = await inv(phone, 'hc_link_start', { hub: HUB, deviceName: 'Test phone' });
  let look = null;
  for (let i = 0; i < 40; i++) { look = await inv(pc, 'hc_link_lookup', { input: start.qr }).catch(() => null); if (look?.device_name) break; await sleep(3000); }
  await inv(pc, 'hc_link_approve', { code: start.code });
  // the identity arrives and waits in memory for the phone to keep its hubs
  // (the review screen's Confirm)
  let kept = null;
  for (let i = 0; i < 60 && !kept; i++) { kept = await inv(phone, 'hc_link_confirm', { hubs: [HUB] }).catch(() => null); if (!kept) await sleep(1000); }
  const ps = await inv(phone, 'hc_state');
  check('the phone is the same identity', ps?.me?.address === A, `${ps?.me?.address} vs ${A}`);
  check('both are connected', await waitFor(phone, `window.__TAURI_INTERNALS__.invoke('hc_state').then((s) => s.hubs.some((h) => h.state === 'connected'))`, 30000)
    && await waitFor(pc, `window.__TAURI_INTERNALS__.invoke('hc_state').then((s) => s.hubs.some((h) => h.state === 'connected'))`, 30000));

  // 1. a picture from the phone
  const fromPhone = await sendFrom(phone, 'A picture from the phone', [{ name: 'from-the-phone.png', size: phonePng.length, bytes: [...phonePng] }]);
  const arrived = await waitFor(pc, `window.__TAURI_INTERNALS__.invoke('hc_message', { id: ${J(fromPhone)} }).then((m) => !!m && m.outgoing && m.attachments.length === 1).catch(() => false)`, 60000);
  check("1. the phone's message reaches the PC as our own", arrived);
  const onPc = await inv(pc, 'hc_message', { id: fromPhone }).catch(() => null);
  check('1. on the PC it has no local original (sent from elsewhere)', !!onPc && !onPc.attachments[0].source, J(onPc?.attachments));
  await pc.eval(`location.hash = ''`).catch(() => {});
  check('1. the PC opens the chat with Pat', await waitFor(pc, `!!document.querySelector('.crow')`, 20000) && await openPat(pc));
  check('1. the PC shows the picture in the bubble', await imgShown(pc, fromPhone, 64));
  check('1. …not as a file chip', await pc.eval(`![...document.querySelectorAll('.msg.out .attcard, .msg.out .att-file')].some((e) => e.innerText.includes('from-the-phone.png'))`));
  await shot(pc, 'own-1-pc-picture-from-phone.png');
  const dl = await inv(pc, 'hc_download', { messageId: fromPhone, localId: onPc.attachments[0].local_id }).catch((e) => 'ERR ' + e);
  const after = await inv(pc, 'hc_message', { id: fromPhone });
  check('1. the PC downloads it', after.attachments[0].state === 'done' && !!after.attachments[0].local_path, J({ dl, att: after.attachments[0] }));

  // 2. a picture and a text file from the PC
  const fromPc = await sendFrom(pc, 'A picture and notes from the PC', [
    { name: 'from-the-pc.png', size: pcPng.length, path: pcPngPath },
    { name: 'notes-from-the-pc.txt', size: 24, path: pcTxtPath },
  ]);
  const onPhone = await waitFor(phone, `window.__TAURI_INTERNALS__.invoke('hc_message', { id: ${J(fromPc)} }).then((m) => !!m && m.outgoing && m.attachments.length === 2).catch(() => false)`, 60000);
  check("2. the PC's message reaches the phone as our own", onPhone);
  check('2. the phone opens the chat with Pat', await waitFor(phone, `!!document.querySelector('.crow')`, 20000) && await openPat(phone));
  check('2. the phone shows the picture in the bubble', await imgShown(phone, fromPc, 80));
  await shot(phone, 'own-2-phone-picture-from-pc.png');
  const m2 = await inv(phone, 'hc_message', { id: fromPc });
  const txt = m2.attachments.find((a) => a.name.endsWith('.txt'));
  const dl2 = await inv(phone, 'hc_download', { messageId: fromPc, localId: txt.local_id }).catch((e) => 'ERR ' + e);
  const after2 = (await inv(phone, 'hc_message', { id: fromPc })).attachments.find((a) => a.name.endsWith('.txt'));
  check('2. the phone downloads the text file', after2.state === 'done', J({ dl2, att: after2 }));
} catch (e) {
  check('no error', false, String(e && e.stack || e));
} finally {
  try { await pc?.close(); } catch {}
  try { await phone?.close(); } catch {}
  app.kill();
  adb('forward', '--remove', 'tcp:9334');
  adb('reverse', '--remove', `tcp:${PORT}`);
  const pass = results.filter(Boolean).length;
  console.log(`${pass}/${results.length} passed`);
  process.exit(pass === results.length ? 0 : 1);
}
