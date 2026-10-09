// Android background modes (design D6) on the real phone, Hubchat Test,
// through the scratch v2 hub over Tailscale (<the PC's tailnet name>:7397, HUBCHAT_PC).
//
//   node tools/e2e-d6.mjs            the quick part:
//     1. default "Stay connected": the foreground service and its ongoing
//        notification run;
//     2. switched off: both go, a periodic job is scheduled;
//     3. "Show message text" off: a message arriving while the app shows the
//        chat list notifies "New message", titled with the sender's name;
//     4. switched back on: the service and its notification return;
//     5. switched off again, the app is sent away and its process killed, a
//        peer writes: the script prints what to watch for.
//   node tools/e2e-d6.mjs verify <peer> <text>
//     the periodic check (Android runs it within about 15 minutes; a forced
//     run before that is skipped by WorkManager) has posted the notification
//     for <peer> with <text>, from a process that was gone; then cleans up.
import { spawnSync } from 'node:child_process';
import { attach } from './cdp.mjs';

const ADB = process.env.ADB || 'adb'; // adb on PATH, or its full path in ADB
const PKG = 'dev.orgtree.hubchat.test';
const HUB = `http://${process.env.HUBCHAT_PC || 'home-pc'}:7397`;
const HUB_LOCAL = 'http://127.0.0.1:7397';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// dumpsys jobscheduler runs past spawnSync's default 1 MB
const adb = (...a) => spawnSync(ADB, a, { encoding: 'utf8', maxBuffer: 64 << 20, env: { ...process.env, MSYS_NO_PATHCONV: '1' } }).stdout;
const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''}`); };
const waitUntil = async (f, ms) => { const t = Date.now(); while (Date.now() - t < ms) { if (await f()) return true; await sleep(500); } return false; };

const services = () => adb('shell', 'dumpsys', 'activity', 'services', PKG).includes(`${PKG}/dev.orgtree.hubchat.ConnectionService`);
/** The live notification records only (the dump goes on with archives). */
const notifs = () => {
  const d = adb('shell', 'dumpsys', 'notification', '--noredact');
  const from = d.indexOf('Notification List:');
  const rest = from < 0 ? d : d.slice(from);
  const end = rest.search(/\n {2}\S/); // the next section at the list's own depth
  return end < 0 ? rest : rest.slice(0, end);
};
/** Our live notifications: [{id, tag, title, text}] */
const ours = () => {
  const out = [];
  for (const block of notifs().split(/\n\s*NotificationRecord\(/).slice(1)) {
    if (!block.includes(`pkg=${PKG}`)) continue;
    const id = (/ id=(\d+)/.exec(block) || [])[1];
    const tag = (/ tag=(\S+)/.exec(block) || [])[1];
    const title = (/android\.title=String \(([^)]*)\)/.exec(block) || [])[1];
    const text = (/android\.text=String \(([^)]*)\)/.exec(block) || [])[1];
    out.push({ id, tag, title, text });
  }
  return out;
};
/** The ongoing "connected" notification (id 1) is up. */
const ongoingUp = () => adb('shell', 'cmd', 'notification', 'list').split('\n').some((l) => l.startsWith(`0|${PKG}|1|`));
/** WorkManager's job: { ns, id } (Android 14+ puts it in a namespace). */
const job = () => {
  const d = adb('shell', 'dumpsys', 'jobscheduler');
  const m = new RegExp(`JOB (?:(\\S+):)?u0a\\d+/(\\d+): \\S+ \\S*${PKG.replace(/\./g, '\\.')}/androidx\\.work\\.impl\\.background\\.systemjob\\.SystemJobService`).exec(d);
  return m ? { ns: m[1] || null, id: m[2] } : null;
};
const runJob = (j) => adb('shell', 'cmd', 'jobscheduler', 'run', '-f', ...(j.ns ? ['-n', j.ns] : []), PKG, j.id);
/** Send the app away and wait until Android lets it be killed (cached). */
const killInBackground = async () => {
  adb('shell', 'input', 'keyevent', 'KEYCODE_HOME');
  return waitUntil(() => { adb('shell', 'am', 'kill', PKG); return adb('shell', 'pidof', PKG).trim() === ''; }, 30000);
};

// a peer on the hub, by plain HTTP (v1 routes)
const peerSlug = `dsixpeer.${Math.random().toString(16).slice(2, 8)}`;
const peerAuth = { 'X-Org-Auth': `${peerSlug}:peer-secret-${peerSlug}-0123456789`, 'Content-Type': 'application/json' };
const peerPost = async (path, body) => {
  const r = await fetch(HUB_LOCAL + path, { method: 'POST', headers: peerAuth, body: JSON.stringify(body) });
  if (!r.ok) throw new Error(`${path} ${r.status} ${await r.text()}`);
  return r.json();
};

let app;
const mode = process.argv[2] || 'quick';
let keepPeer = false;
try {
  if (mode === 'verify') {
    const [peer, text] = process.argv.slice(3);
    const n = ours().find((x) => x.tag === peer);
    check('the periodic check notified the message from a process that was gone', n?.text === text, JSON.stringify(n));
    check("titled with the sender's name", n?.title === 'Pat Peer', JSON.stringify(n));
    check('no ongoing notification in this mode', !ongoingUp());
    // clean up: back to the default, the peer off the hub
    adb('shell', 'monkey', '-p', PKG, '-c', 'android.intent.category.LAUNCHER', '1');
    await sleep(4000);
    adb('forward', 'tcp:9334', `localabstract:webview_devtools_remote_${adb('shell', 'pidof', PKG).trim()}`);
    app = await attach(9334);
    await sleep(1500);
    await app.eval(`window.__TAURI_INTERNALS__.invoke('hc_set_stay_connected', { on: true })`);
    check('back to Stay connected', await waitUntil(() => services(), 15000));
    const auth = { 'X-Org-Auth': `${peer}:peer-secret-${peer}-0123456789`, 'Content-Type': 'application/json' };
    await fetch(HUB_LOCAL + '/api/unregister', { method: 'POST', headers: auth, body: JSON.stringify({ slug: peer }) }).catch(() => {});
    keepPeer = true;
  } else {
    // a fresh test app
    adb('reverse', '--remove-all');
    adb('shell', 'pm', 'clear', PKG);
    adb('shell', 'pm', 'grant', PKG, 'android.permission.POST_NOTIFICATIONS');
    adb('shell', 'monkey', '-p', PKG, '-c', 'android.intent.category.LAUNCHER', '1');
    await sleep(4000);
    adb('forward', 'tcp:9334', `localabstract:webview_devtools_remote_${adb('shell', 'pidof', PKG).trim()}`);
    app = await attach(9334);
    await sleep(1500);
    const inv = (cmd, args = {}) => app.eval(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);
    const me = await inv('hc_create_identity', { id: 'dsix', name: 'Dee Six' });
    await inv('hc_add_hub', { input: HUB });
    const up = await waitUntil(async () => (await inv('hc_state')).hubs.some((h) => h.state === 'connected'), 30000);
    check('phone is set up and connected over Tailscale', up, me);
    await peerPost('/api/register', { slug: peerSlug, kind: 'person', org_name: 'Pat Peer', username: 'pat', blurb: '' });
    // the phone learns the peer's name from the roster
    await waitUntil(async () => (await inv('hc_directory')).some((c) => c.address === peerSlug), 70000);

    // 1. the default
    let st = await inv('hc_state');
    check('Stay connected is the default', st.stay_connected === true, String(st.stay_connected));
    check('the foreground service runs', services());
    check('the ongoing notification shows', await waitUntil(() => ongoingUp(), 20000), JSON.stringify(ours()));

    // 2. off
    await inv('hc_set_stay_connected', { on: false });
    st = await inv('hc_state');
    check('switched off', st.stay_connected === false);
    check('the service stopped', await waitUntil(() => !services(), 10000));
    check('the ongoing notification went', await waitUntil(() => !ongoingUp(), 10000), JSON.stringify(ours()));
    await waitUntil(() => job() !== null, 15000);
    check('a periodic job is scheduled', !!job(), JSON.stringify(job()));

    // 3. the text hidden, the app open on its chat list
    await inv('hc_set_notifications', { enabled: true, preview: false, sound: false });
    await peerPost('/api/send', { from: peerSlug, to: me, body: 'secret text', id: 'd6a-' + Date.now() });
    const hidden = await waitUntil(() => ours().some((n) => n.tag === peerSlug), 30000);
    const n1 = ours().find((n) => n.tag === peerSlug);
    check("text off: 'New message', titled with the sender's name", hidden && n1?.text === 'New message' && n1?.title === 'Pat Peer' && !/secret text/.test(notifs()), JSON.stringify(n1));
    await inv('hc_set_notifications', { enabled: true, preview: true, sound: false });

    // 4. back on
    await inv('hc_set_stay_connected', { on: true });
    check('switched back on: the service runs again', await waitUntil(() => services(), 15000));
    check('...with its ongoing notification', await waitUntil(() => ongoingUp(), 20000));
    const ongoing = ours().find((n) => n.id === '1');
    check('...saying who is connected', /Connected as dsix\./.test(ongoing?.text || ''), JSON.stringify(ongoing));
    check('the periodic job is gone', await waitUntil(() => job() === null, 10000), JSON.stringify(job()));

    // 5. off again; away; killed; a peer writes
    await inv('hc_set_stay_connected', { on: false });
    await waitUntil(() => job() !== null, 15000);
    check('no script errors', app.errors.length === 0, app.errors.join(' | '));
    await app.close(); app = null;
    check('the process is gone', await killInBackground());
    const text = 'hello from the 15-minute check ' + Date.now();
    await peerPost('/api/send', { from: peerSlug, to: me, body: text, id: 'd6c-' + Date.now() });
    await sleep(5000);
    check('nothing arrives while the process is gone', !ours().some((n) => n.tag === peerSlug && n.text === text) && adb('shell', 'pidof', PKG).trim() === '');
    keepPeer = true;
    console.log(`WATCH ${peerSlug} ${JSON.stringify(text)} armed at ${new Date().toISOString()}`);
  }
} catch (e) {
  check('script ran to the end', false, String(e && e.stack || e));
} finally {
  try { await app?.close(); } catch { /* ignore */ }
  if (!keepPeer) { try { await peerPost('/api/unregister', { slug: peerSlug }); } catch { /* ignore */ } }
  const failed = results.filter((r) => !r).length;
  console.log(`${results.length - failed}/${results.length} passed`);
  process.exit(failed ? 1 : 0);
}
