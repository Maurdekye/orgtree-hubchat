// Tonight's update on the real apps (run after e2e-link-tailnet.mjs, which
// leaves the phone test build and the desktop test build as one identity A on
// the scratch hub): a scripted partner (Pat) sends through the hub.
//   1. a picture from Pat shows as a picture on the phone and the PC (fetched
//      for the preview, not downloaded);
//   2. the phone's notification goes once the chat is read on the PC;
//   3. while the PC is in use the phone keeps quiet; after, it notifies
//      (needs a hub with the "active" feature);
//   4. a hubchat://chat link opens the chat on the phone; an address no hub
//      lists opens with the message box off;
//   5. a long chat opens with the newest page and loads older ones;
//   6. a picture pasted into the PC's message box is attached and sent.
//
//   node tools/e2e-update.mjs <hubchat.exe test build> <shots-dir> <hub-url-for-the-peer>
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
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

const [exe, shots, hub] = process.argv.slice(2);
mkdirSync(shots, { recursive: true });
const ADB = process.env.ADB || 'adb'; // adb on PATH, or its full path in ADB
const PKG = 'dev.orgtree.hubchat.test';
const PEER = fileURLToPath(new URL('./peer2.mjs', import.meta.url));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const env = { ...process.env, MSYS_NO_PATHCONV: '1' };
const adb = (...a) => spawnSync(ADB, a, { encoding: 'utf8', env, maxBuffer: 64 << 20 }).stdout.trim();
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };
const peer = (...a) => JSON.parse(spawnSync('node', [PEER, hub, ...a], { encoding: 'utf8' }).stdout.trim().split('\n').pop() || '{}');
const peerSlug = spawnSync('node', [PEER, hub, 'whoami'], { encoding: 'utf8' }).stdout.trim();
/** This test build's message notifications on the phone, by chat. */
const notes = () => {
  const d = adb('shell', 'dumpsys', 'notification', '--noredact');
  return d.split('NotificationRecord(').filter((b) => b.includes('pkg=' + PKG) && /\bid=2\b/.test(b)).map((b) => (/tag=(\S+)/.exec(b) || [])[1]);
};
const waitFor = async (b, expr, ms) => { const t = Date.now(); while (Date.now() - t < ms) { if (await b.eval(expr).catch(() => false)) return true; await sleep(400); } return false; };
const until = async (f, ms) => { const t = Date.now(); while (Date.now() - t < ms) { if (await f()) return true; await sleep(700); } return false; };
const inv = (b, cmd, args = {}) => b.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);
const shot = (b, n) => Promise.race([b.shot(join(shots, n)).catch(() => {}), sleep(8000)]);
const openChat = (b, slug) => b.eval(`(() => { const el = [...document.querySelectorAll('.crow')].find((x) => x.innerText.includes('Pat Peer') || x.innerText.includes(${JSON.stringify(slug)})); if (!el) return false; el.click(); return true; })()`);
const toList = async (b) => { for (let i = 0; i < 4 && !(await b.eval(`!!document.querySelector('.crow')`).catch(() => false)); i++) { await b.eval('history.back()').catch(() => {}); await sleep(600); } };

// a picture to send: a real PNG
const png = join(shots, 'peer-screenshot.png');
writeFileSync(png, pngOf(64, 48, [59, 111, 181]));

adb('shell', 'monkey', '-p', PKG, '-c', 'android.intent.category.LAUNCHER', '1');
await sleep(3500);
adb('forward', 'tcp:9334', `localabstract:webview_devtools_remote_${adb('shell', 'pidof', PKG)}`);
const app = spawn(exe, [], { stdio: 'ignore', env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9333' } });
let pc, phone;
try {
  for (let i = 0; i < 60 && !pc; i++) { try { pc = await attach(9333); } catch { await sleep(500); } }
  phone = await attach(9334);
  const A = (await inv(pc, 'hc_state')).me?.address;
  const Aphone = (await inv(phone, 'hc_state')).me?.address;
  check('phone and PC are one identity', !!A && A === Aphone, A + ' / ' + Aphone);
  // the hub's features come with its first answer
  await waitFor(pc, `window.__TAURI_INTERNALS__.invoke('hc_state').then((s) => s.hubs.some((h) => h.state === 'connected' && h.features.length > 0))`, 30000);
  const hubs = await inv(pc, 'hc_state').then((s) => s.hubs);
  const active = hubs.some((h) => (h.features || []).includes('active'));
  console.log('peer', peerSlug, 'hub has active:', active);

  // 1. a picture from Pat
  peer('send', A, 'the new onboarding', png);
  await sleep(2500);
  await toList(phone);
  check('phone: Pat\'s chat is there', await waitFor(phone, `[...document.querySelectorAll('.crow')].some((x) => x.innerText.includes('Pat Peer'))`, 20000));
  await openChat(phone, peerSlug);
  check('phone: the picture shows in the bubble', await waitFor(phone, `[...document.querySelectorAll('.timeline .att-img img')].some((i) => i.complete && i.naturalWidth === 64)`, 20000));
  await shot(phone, 'upd-phone-picture.png');
  await phone.eval(`document.querySelector('.timeline .att-img img').click()`);
  check('phone: tapping it opens the viewer', await waitFor(phone, `!!document.querySelector('.imgview img')`, 5000));
  await shot(phone, 'upd-phone-viewer.png');
  await phone.eval('history.back()'); await sleep(600);
  check('phone: back closes the viewer, the chat stays', !(await phone.eval(`!!document.querySelector('.imgview')`)) && await phone.eval(`!!document.querySelector('.scr .composer')`));
  const pcMsgs = await inv(pc, 'hc_chat', { peer: peerSlug, beforeAt: null, beforeId: null, fromAt: null, fromId: null, limit: 50 });
  const picMsg = pcMsgs.find((m) => m.attachments.length);
  check('PC: has the picture message', !!picMsg);
  if (picMsg) {
    const bytes = await pc.eval(`window.__TAURI_INTERNALS__.invoke('hc_attachment_preview', { messageId: ${JSON.stringify(picMsg.id)}, localId: ${JSON.stringify(picMsg.attachments[0].local_id)} }).then((b) => (b.byteLength ?? b.length))`);
    check('PC: its preview comes through the core', bytes > 50, String(bytes));
    const after = (await inv(pc, 'hc_message', { id: picMsg.id })).attachments[0];
    check('PC: showing it downloaded nothing', after.state !== 'done' && !after.local_path, JSON.stringify(after));
  }

  // 2. the phone's notification goes once the chat is read on the PC
  await toList(phone);
  adb('shell', 'input', 'keyevent', '3'); // the phone to the home screen: it notifies
  await sleep(2000);
  await inv(pc, 'hc_set_active', { on: false });
  peer('send', A, 'are you there? ' + Date.now());
  check('phone: notifies for Pat', await until(() => notes().includes(peerSlug), 20000), JSON.stringify(notes()));
  await inv(pc, 'hc_mark_read', { peer: peerSlug });
  check('phone: the notification goes when the PC reads the chat', await until(() => !notes().includes(peerSlug), 20000), JSON.stringify(notes()));

  // 3. while the PC is in use, the phone keeps quiet
  if (active) {
    await inv(pc, 'hc_set_active', { on: true });
    await sleep(1500);
    const before = (await inv(pc, 'hc_chat', { peer: peerSlug, beforeAt: null, beforeId: null, fromAt: null, fromId: null, limit: 5 })).length;
    peer('send', A, 'while you are at the PC ' + Date.now());
    await sleep(8000);
    const arrived = (await inv(phone, 'hc_chat', { peer: peerSlug, beforeAt: null, beforeId: null, fromAt: null, fromId: null, limit: 5 })).length;
    check('phone: no notification while the PC is in use', !notes().includes(peerSlug), JSON.stringify(notes()));
    check('phone: the message still arrived', arrived >= Math.min(5, before), arrived + ' messages');
    await inv(pc, 'hc_set_active', { on: false });
    await sleep(1500);
    peer('send', A, 'now that you left the PC ' + Date.now());
    check('phone: notifies again once the PC is put down', await until(() => notes().includes(peerSlug), 20000), JSON.stringify(notes()));
    await inv(pc, 'hc_mark_read', { peer: peerSlug });
    await until(() => !notes().includes(peerSlug), 15000);
  } else {
    check('the hub has the "active" feature (skipping the quiet test)', false);
  }

  // 4. chat links open the chat on the phone
  adb('shell', 'am', 'start', '-a', 'android.intent.action.VIEW', '-d', `hubchat://chat?to=${peerSlug}`, PKG);
  check('phone: a chat link opens that chat', await waitFor(phone, `/Pat Peer/.test(document.querySelector('.scr .appbar .who')?.innerText || '')`, 15000), await phone.eval(`document.querySelector('.scr .appbar')?.innerText || ''`));
  adb('shell', 'am', 'start', '-a', 'android.intent.action.VIEW', '-d', 'hubchat://chat?to=nobody.4a1b2c', PKG);
  check('phone: an address no hub lists opens with the box off', await waitFor(phone, `/isn't on any of the mail hubs you're connected to/.test(document.querySelector('.scr .comp-off')?.innerText || '')`, 15000));
  await shot(phone, 'upd-phone-nobody.png');

  // 5. a long chat: the newest page, then older ones
  for (let i = 1; i <= 60; i++) peer('send', A, 'log line ' + i);
  await sleep(4000);
  await toList(phone);
  await openChat(phone, peerSlug);
  await sleep(1500);
  const n0 = await phone.eval(`document.querySelectorAll('.timeline .msg').length`);
  check('phone: a long chat opens with the newest 50', n0 === 50, String(n0));
  await phone.eval(`(() => { const t = document.querySelector('.timeline'); t.scrollTop = 0; t.dispatchEvent(new Event('scroll')); })()`);
  check('phone: older ones load as the top comes near', await waitFor(phone, `document.querySelectorAll('.timeline .msg').length > 50`, 10000), String(await phone.eval(`document.querySelectorAll('.timeline .msg').length`)));

  // 6. a picture pasted into the PC's message box
  await openChat(pc, peerSlug);
  await sleep(800);
  const pasted = await pc.eval(`(async () => {
    const c = document.createElement('canvas'); c.width = 120; c.height = 80; const g = c.getContext('2d'); g.fillStyle = '#2e7d32'; g.fillRect(0, 0, 120, 80);
    const blob = await new Promise((r) => c.toBlob(r, 'image/png'));
    const dt = new DataTransfer(); dt.items.add(new File([blob], 'image.png', { type: 'image/png' }));
    const ta = document.querySelector('.composer textarea'); ta.focus();
    return !ta.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));
  })()`);
  check('PC: a pasted picture is taken', pasted);
  check('PC: …attached as pasted-….png, shown as a thumbnail', await waitFor(pc, `[...document.querySelectorAll('.attrow .attthumb img')].some((i) => /^pasted-\\d{8}-\\d{6}\\.png$/.test(i.alt) && i.complete && i.naturalWidth === 120)`, 8000), await pc.eval(`[...document.querySelectorAll('.attrow > *')].map((c) => c.title || c.innerText).join(' | ')`));
  await pc.eval(`document.querySelector('.sendbtn').click()`);
  check('PC: sent, it shows as a picture', await waitFor(pc, `[...document.querySelectorAll('.msg.out .att-img img')].some((i) => /^pasted-/.test(i.alt) && i.complete && i.naturalWidth === 120)`, 20000));
  await shot(pc, 'upd-pc-pasted.png');
  check('no script errors on the phone', phone.errors.length === 0, phone.errors.join(' | '));
  check('no script errors on the PC', pc.errors.length === 0, pc.errors.join(' | '));
} catch (e) {
  check('script ran to the end', false, e.stack || String(e));
} finally {
  try { await pc?.close(); await phone?.close(); } catch { /* ignore */ }
  spawnSync('taskkill', ['/PID', String(app.pid), '/T', '/F'], { stdio: 'ignore' });
  const failed = results.filter((r) => !r).length;
  console.log(`${results.length - failed}/${results.length} passed`);
  process.exit(failed ? 1 : 0);
}
