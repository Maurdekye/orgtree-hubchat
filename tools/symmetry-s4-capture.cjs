// Electron driver for the symmetry S4 captures (tools/proof-symmetry-s4-capture.mjs builds the pages). Offscreen, Session 0
// only, audio muted, every permission refused, no network, the clipboard stubbed; input only by script into its own pages.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const { app, BrowserWindow, session, clipboard } = require('electron');
const out = process.env.HUBCHAT_RENDERER_OUT;
const sid = (pid) => Number(execFileSync('powershell.exe', ['-NoProfile', '-Command', `(Get-Process -Id ${Number(pid)}).SessionId`], { encoding: 'utf8', windowsHide: true }).trim());
assert.equal(sid(process.pid), 0, 'Session 0 required before creating a window');
app.disableHardwareAcceleration();
app.commandLine.appendSwitch('mute-audio');
app.setPath('userData', path.join(out, 'user-data'));
let clipboardCalls = 0;
for (const k of Object.keys(clipboard)) if (typeof clipboard[k] === 'function') clipboard[k] = () => { clipboardCalls++; return k === 'availableFormats' ? [] : k === 'has' ? false : ''; };
app.on('window-all-closed', () => {});

const OFF = ["document.querySelector('.set-state')?.textContent", 'off'], ON = ["document.querySelector('.set-state')?.textContent", 'on'];
const SCENES = {
  desktop: [
    { scene: 'newchat', shots: [{ name: 'newchat-rows', expect: [/Design · Android/, /office/, /lab/] }, { name: 'newchat-menu', act: 'menu', expect: [/Message/, /Contact info/, /Copy address/] }] },
    { scene: 'files-loading', shots: [{ name: 'files-loading', act: 'scrollFiles', expect: [/Loading files/] }] },
    { scene: 'files-error', shots: [{ name: 'files-error', act: 'scrollFiles', expect: [/disk is busy/, /Retry/] }] },
    { scene: 'files-none', shots: [{ name: 'files-none', act: 'scrollFiles', expect: [/No files yet/] }] },
    { scene: 'files-list', shots: [{ name: 'files-list', act: 'scrollFiles', expect: [/report\.pdf/, /notes\.txt/] }] },
    { scene: 'transfers', shots: [{ name: 'transfers-history', expect: [/Received from Maya Lin/, /Sent to Maya Lin/, /Received from Kim Okafor/] }] },
    { scene: 'transfers-error', shots: [{ name: 'transfers-page-error', expect: [/could not be read/, /Retry/, /Received from Maya Lin/, /Load more/] }, { name: 'transfers-after-retry', act: 'retry', expect: [/older-1/, /older-2/] }] },
    { scene: 'settings', shots: [{ name: 'settings-tabs-focus', act: 'focusTab', expect: [/Privacy/, /Send read receipts/], fact: OFF }, { name: 'settings-toggled-on', act: 'toggle', expect: [/Send read receipts/], fact: ON }] },
    { scene: 'composer', shots: [{ name: 'composer-default', expect: [/Enter\s*send/, /Shift\+Enter\s*new line/] }, { name: 'composer-pasted-pdf', act: 'paste', expect: [/report\.pdf/] }, { name: 'composer-newline-mode', act: 'setNewline', expect: [/Ctrl\+Enter\s*send/, /Enter\s*new line/] }] },
    { scene: 'mention', shots: [{ name: 'mention-menu', act: 'menu', expect: [/Open chat/, /Contact info/, /Copy address/] }] },
  ],
  android: [
    { scene: 'newchat', shots: [{ name: 'newchat-rows', expect: [/Design · Android/, /office/, /lab/] }] },
    { scene: 'files-loading', shots: [{ name: 'files-loading', act: 'scrollFiles', expect: [/Loading files/] }] },
    { scene: 'files-error', shots: [{ name: 'files-error', act: 'scrollFiles', expect: [/disk is busy/, /Retry/] }] },
    { scene: 'files-none', shots: [{ name: 'files-none', act: 'scrollFiles', expect: [/No files yet/] }] },
    { scene: 'transfers', shots: [{ name: 'transfers-history', expect: [/Received from Maya Lin/, /Sent to Maya Lin/] }] },
    { scene: 'transfers-error', shots: [{ name: 'transfers-page-error', expect: [/could not be read/, /Retry/, /Load more/] }, { name: 'transfers-after-retry', act: 'retry', expect: [/older-1/] }] },
    { scene: 'settings', shots: [{ name: 'privacy-off', expect: [/Send read receipts/], fact: OFF }, { name: 'privacy-on', act: 'toggle', expect: [/Send read receipts/], fact: ON }] },
  ],
};

const wait = (ms) => new Promise((r) => setTimeout(r, ms));
app.whenReady().then(async () => {
  const results = [], errors = [];
  try {
    for (const platform of ['desktop', 'android']) {
      for (const sc of SCENES[platform]) {
        const win = new BrowserWindow({ width: platform === 'desktop' ? 1000 : 390, height: platform === 'desktop' ? 700 : 760, show: false, frame: false, webPreferences: { offscreen: true, backgroundThrottling: false, nodeIntegration: false, contextIsolation: true, sandbox: true } });
        const wc = win.webContents;
        wc.setAudioMuted(true);
        wc.session.setPermissionCheckHandler(() => false);
        wc.session.setPermissionRequestHandler((_w, _p, cb) => cb(false));
        wc.session.webRequest.onBeforeRequest({ urls: ['http://*/*', 'https://*/*', 'ws://*/*', 'wss://*/*'] }, (_d, cb) => cb({ cancel: true }));
        wc.on('console-message', (_e, level, message) => { if (level >= 3) errors.push(`${platform}/${sc.scene}: ${message}`); });
        for (const k of ['copy', 'cut', 'paste', 'pasteAndMatchStyle']) wc[k] = () => { throw Error('Clipboard denied'); };
        await win.loadFile(path.join(out, platform + '.html'), { query: { platform, scene: sc.scene } });
        const deadline = Date.now() + 15000;
        for (;;) {
          const s = await wc.executeJavaScript('({ready: !!window.sceneReady, error: window.sceneError || null})');
          if (s.error) throw Error(`${platform}/${sc.scene}: ${s.error}`);
          if (s.ready) break;
          if (Date.now() > deadline) throw Error(`${platform}/${sc.scene}: scene never became ready`);
          await wait(40);
        }
        for (const shot of sc.shots) {
          if (shot.act) await wc.executeJavaScript(`window.scene.${shot.act}()`);
          await wc.executeJavaScript('new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 180))))');
          const text = await wc.executeJavaScript('document.body.innerText');
          const png = (await wc.capturePage()).toPNG();
          fs.writeFileSync(path.join(out, `${platform}-${shot.name}.png`), png);
          const missing = shot.expect.filter((re) => !re.test(text)).map(String);
          if (shot.fact) { const got = await wc.executeJavaScript(shot.fact[0]); if (got !== shot.fact[1]) missing.push(`${shot.fact[0]} was ${JSON.stringify(got)}, wanted ${JSON.stringify(shot.fact[1])}`); }
          results.push({ platform, scene: sc.scene, shot: shot.name, png: `${platform}-${shot.name}.png`, ok: missing.length === 0, missing, text: text.replace(/\s+/g, ' ').slice(0, 400) });
        }
        win.destroy();
      }
    }
    const passed = results.every((r) => r.ok) && clipboardCalls === 0;
    fs.writeFileSync(path.join(out, 'capture.json'), JSON.stringify({ passed, sessionId: sid(process.pid), audioMuted: true, clipboardCalls, errors, results }, null, 2));
    console.log(JSON.stringify({ passed, shots: results.length, failed: results.filter((r) => !r.ok).map((r) => r.platform + '/' + r.shot), errors: errors.length }));
    app.exit(passed ? 0 : 1);
  } catch (e) {
    console.error(e.stack || e);
    fs.writeFileSync(path.join(out, 'capture-failure.json'), JSON.stringify({ error: String(e.stack || e), errors, results }, null, 2));
    app.exit(2);
  }
});
