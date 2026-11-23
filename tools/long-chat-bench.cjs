// Offscreen Electron for tools/proof-long-chat.mjs: loads the long-chat fixture
// (tools/long-chat-entry.tsx) once per history length and records what each
// costs. No input, no media, no network (http(s) blocked, every permission
// denied), muted, no clipboard. Records the Windows session of each process.
const fs = require('node:fs'), path = require('node:path'), { execFileSync } = require('node:child_process');
const { app, BrowserWindow, session } = require('electron');
const out = process.env.HUBCHAT_RENDERER_OUT;
const sizes = (process.env.HUBCHAT_LONG_CHAT_SIZES || '100,1000,5000,20000').split(',').map(Number);
const label = process.env.HUBCHAT_LONG_CHAT_LABEL || 'run';
app.commandLine.appendSwitch('mute-audio'); app.disableHardwareAcceleration(); app.setPath('userData', path.join(out, 'user-data-' + label));
app.on('window-all-closed', () => {});
const sessionOf = pid => { try { return Number(execFileSync('powershell', ['-NoProfile', '-NonInteractive', '-Command', `(Get-Process -Id ${pid}).SessionId`], { encoding: 'utf8', windowsHide: true }).trim()); } catch { return null; } };
app.whenReady().then(async () => {
  const s = session.defaultSession;
  s.setPermissionRequestHandler((_w, _p, cb) => cb(false)); s.setPermissionCheckHandler(() => false);
  s.webRequest.onBeforeRequest({ urls: ['http://*/*', 'https://*/*', 'ws://*/*', 'wss://*/*'] }, (_, cb) => cb({ cancel: true }));
  const results = []; let code = 0;
  try {
    for (const n of sizes) {
      const win = new BrowserWindow({ show: true, width: 1000, height: 720, webPreferences: { backgroundThrottling: false, offscreen: true } });
      win.webContents.setAudioMuted(true);
      await win.loadFile(path.join(out, 'long-chat.html'));
      const js = c => win.webContents.executeJavaScript(c);
      await js(`(async()=>{const d=performance.now()+20000;while(!window.longChat){if(window.longChatError)throw Error(window.longChatError);if(performance.now()>d)throw Error('No fixture');await new Promise(r=>setTimeout(r,20));}})()`);
      const r = await js(`window.longChat.run(${n})`);
      r.session = { main: sessionOf(process.pid), renderer: sessionOf(win.webContents.getOSProcessId()) };
      console.log(JSON.stringify(r));
      results.push(r); win.destroy();
    }
  } catch (e) { console.error(e); code = 1; }
  const failed = results.flatMap(r => (r.checks || []).filter(c => !c.ok).map(c => `${r.n}: ${c.name}`));
  const passed = !code && results.length === sizes.length && !failed.length && results.every(r => r.session.main === 0 && r.session.renderer === 0);
  fs.writeFileSync(path.join(out, `long-chat-${label}.json`), JSON.stringify({ label, sizes, passed, failed, results }, null, 2));
  console.log(passed ? 'LONG-CHAT PASS' : 'LONG-CHAT FAIL ' + JSON.stringify(failed));
  if (!code && !passed) code = 2;
  app.exit(code);
});
