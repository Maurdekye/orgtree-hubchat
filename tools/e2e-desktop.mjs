// End-to-end smoke of the real app (Windows, or Android with E2E_ATTACH) against a scratch hub:
// onboarding (new identity, add hub, recovery words) -> a scripted peer
// writes -> the message shows up -> we reply from the composer -> the peer
// receives it and receipts it -> the tick climbs.
//
//   node tools/e2e-desktop.mjs <hubchat.exe> <shots-dir>
//
// Needs: a debug build, python with the orgtree mailhub (MAILHUB_DIR),
// and NO existing Hubchat identity for this user (it creates one).
import { spawn, spawnSync } from 'node:child_process';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { attach } from './cdp.mjs';

const [exe, shots] = process.argv.slice(2);
const MAILHUB = process.env.MAILHUB_DIR || '<orgtree>/engine/mailhub';
const HUB_PORT = 7399;
// E2E_ATTACH=<port>: drive an already-running app (e.g. Android via adb forward) instead of launching one.
const CDP_PORT = Number(process.env.E2E_ATTACH || 9333);
const HUB = `http://127.0.0.1:${HUB_PORT}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, ok, detail = '') => { results.push({ name, ok, detail }); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };

const hub = spawn('python', ['-m', 'mailhub.serve'], {
  cwd: MAILHUB, stdio: 'ignore',
  env: { ...process.env, HUB_DATA: mkdtempSync(join(tmpdir(), 'hc-hub-')), HUB_PORT: String(HUB_PORT), HUB_BIND: '127.0.0.1', HUB_NAME: 'e2ehub' },
});
const app = process.env.E2E_ATTACH ? null : spawn(exe, [], { stdio: 'ignore', env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP_PORT}` } });
let b;
try {
  for (let i = 0; i < 50; i++) { try { await fetch(HUB + '/healthz'); break; } catch { await sleep(200); } }
  console.log('hub up; attaching');
  for (let i = 0; i < 60 && !b; i++) { try { b = await attach(CDP_PORT); } catch { await sleep(500); } }
  if (!b) throw new Error('could not attach to the app WebView');
  // Screenshots are evidence, not steps: never let one hang the run.
  const rawShot = b.shot.bind(b);
  b.shot = (p) => Promise.race([rawShot(p).catch(() => {}), sleep(8000)]).then(() => console.log('  shot', p.split(/[\/]/).pop()));
  await sleep(2500);
  const inv = (cmd, args = {}) => b.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);
  const clickText = async (sel, text, wait = 500) => {
    const ok = await b.eval(`(() => { const e = [...document.querySelectorAll(${JSON.stringify(sel)})].find((x) => x.innerText.includes(${JSON.stringify(text)}) && !x.disabled); if (!e) return false; e.click(); return true; })()`);
    if (!ok) throw new Error(`no ${sel} with "${text}"`);
    await sleep(wait);
  };
  const waitFor = async (expr, ms = 15000) => { const t = Date.now(); while (Date.now() - t < ms) { if (await b.eval(expr)) return true; await sleep(300); } return false; };

  const s0 = await inv('hc_state');
  check('starts at onboarding (no identity)', s0.me === null, JSON.stringify(s0.me));
  await b.shot(join(shots, 'real-onboarding.png'));
  await clickText('.fork-opt', 'Create a new identity');
  await b.type('#ob-id', 'e2e', 200); await b.type('#ob-name', 'E2E Tester', 200);
  await clickText('.ob-foot button', 'Continue', 1500);
  const s1 = await inv('hc_state');
  check('identity created', !!s1.me && s1.me.address.startsWith('e2e.'), s1.me?.address);
  await b.shot(join(shots, 'real-address.png'));
  await clickText('.ob-foot button', 'Continue', 600);
  await b.type('#hub-in', `127.0.0.1:${HUB_PORT}`, 200);
  await clickText('button', 'Check', 2000);
  check('probe says connected', await b.eval(`!!document.querySelector('.probe-card') && document.querySelector('.probe-card').innerText.includes('e2ehub')`));
  await b.shot(join(shots, 'real-hub-probe.png'));
  await clickText('.probe-card button', 'Add', 1500);
  await clickText('.ob-foot button', 'Continue', 1200);
  const words = await b.eval(`document.querySelectorAll('.words li, .word, .wgrid > *').length`);
  check('recovery words shown', words >= 24, `${words} word cells`);
  await b.shot(join(shots, 'real-words.png'));
  await clickText('button', 'saved', 1500);
  const hubOk = await waitFor(`window.__TAURI_INTERNALS__.invoke('hc_state').then((s) => s.hubs.some((h) => h.state === 'connected'))`, 15000);
  check('hub connected', hubOk);
  await b.shot(join(shots, 'real-main-empty.png'));

  const me = (await inv('hc_state')).me.address;
  const peer = spawn('python', ['-I', 'tools/peer.py', HUB, me, '40'], { stdio: ['ignore', 'pipe', 'inherit'] });
  let peerOut = ''; peer.stdout.on('data', (d) => { peerOut += d; });
  const peerDone = new Promise((r) => peer.on('exit', r));
  const gotRow = await waitFor(`[...document.querySelectorAll('.crow')].some((r) => r.innerText.includes('hello from') || r.innerText.includes('Test Peer'))`, 20000);
  check('incoming message appears in the chat list', gotRow);
  await b.shot(join(shots, 'real-incoming.png'));
  await clickText('.crow', 'peer', 1200).catch(() => clickText('.crow', 'Test Peer', 1200));
  check('message body rendered with markdown', await b.eval(`[...document.querySelectorAll('.msg.in')].some((m) => m.querySelector('strong')?.innerText === 'peer')`));
  await b.type('.comp-box textarea', 'reply from the app', 200);
  if (process.env.E2E_ATTACH) await b.click('button[aria-label="Send"]', 800); else await b.key('Enter', 0, 800);
  const outs = await b.eval(`[...document.querySelectorAll('.msg.out')].map((m) => m.innerText.slice(0, 60))`);
  check('reply shows in the conversation', outs.some((t) => t.includes('reply from the app')), JSON.stringify(outs));
  await b.shot(join(shots, 'real-after-send.png'));
  await peerDone;
  const p = JSON.parse(peerOut.trim().split('\n').pop() || '{}');
  check('peer received the reply', p.reply === 'reply from the app', JSON.stringify(p));
  const tickRead = await waitFor(`(() => { const m = [...document.querySelectorAll('.msg.out')].pop(); return !!m && /read/.test(m.querySelector('.tick')?.className || ''); })()`, 20000);
  if (!process.env.E2E_ATTACH) check('chat list row shows the read tick too', await waitFor(`(() => { const r = [...document.querySelectorAll('.crow')].find((x) => x.innerText.includes('reply from the app')); return !!r && /read/.test(r.querySelector('.tick')?.className || ''); })()`, 5000));
  check('reply tick climbs to read (green)', tickRead, await b.eval(`[...document.querySelectorAll('.msg.out')].pop()?.querySelector('.tick')?.className`));
  check("peer's message receipted (delivered or read)", ['delivered', 'read'].includes(p.our_receipt), p.our_receipt);
  await b.shot(join(shots, 'real-conversation.png'));
  check('no script errors', b.errors.length === 0, b.errors.join(' | '));
} catch (e) {
  check('script ran to the end', false, String(e));
  try { await b?.shot(join(shots, 'real-failure.png')); } catch { /* app gone */ }
} finally {
  try { await b?.close(); } catch { /* ignore */ }
  if (app) spawnSync('taskkill', ['/PID', String(app.pid), '/T', '/F'], { stdio: 'ignore' });
  hub.kill();
  const failed = results.filter((r) => !r.ok).length;
  console.log(`${results.length - failed}/${results.length} passed`);
  process.exit(failed ? 1 : 0);
}
