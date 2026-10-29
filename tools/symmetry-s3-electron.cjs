// Offscreen Electron for tools/proof-symmetry-s3-capture.mjs: each platform loads the mounted fixture
// (tools/symmetry-s3-entry.tsx), runs every check in real Chromium and takes a screenshot whenever the fixture asks.
// Muted, every permission (clipboard included) denied, http(s) blocked, input only to these offscreen windows.
// Records the Windows session of this process and of each renderer (the proof needs Session 0).
const fs = require('node:fs'), path = require('node:path'), { execFileSync } = require('node:child_process');
const { app, BrowserWindow, session } = require('electron');
const out = process.env.HUBCHAT_RENDERER_OUT, red = process.env.HUBCHAT_PROOF_PHASE === 'red';
app.commandLine.appendSwitch('mute-audio'); app.disableHardwareAcceleration(); app.setPath('userData', path.join(out, 'user-data'));
app.on('window-all-closed', () => {});
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const sessionOf = (pid) => { try { return Number(execFileSync('powershell', ['-NoProfile', '-NonInteractive', '-Command', `(Get-Process -Id ${pid}).SessionId`], { encoding: 'utf8', windowsHide: true }).trim()); } catch { return null; } };
app.whenReady().then(async () => {
  session.defaultSession.setPermissionRequestHandler((_w, _p, cb) => cb(false)); session.defaultSession.setPermissionCheckHandler(() => false);
  session.defaultSession.webRequest.onBeforeRequest({ urls: ['http://*/*', 'https://*/*'] }, (_, cb) => cb({ cancel: true }));
  const results = [], shots = [];
  let win;
  try {
    for (const platform of ['desktop', 'android']) {
      win = new BrowserWindow({ show: false, width: platform === 'desktop' ? 1000 : 390, height: 690, webPreferences: { backgroundThrottling: false, offscreen: true } });
      win.webContents.setAudioMuted(true);
      await win.loadFile(path.join(out, platform + '.html'));
      // a real (trusted) non-modifier key press first, before the app mounts, so a script focus() after it is drawn as
      // keyboard focus (:focus-visible); a synthetic click() does not count as pointer use
      // (through this window's own DevTools connection: a trusted key event on a hidden offscreen window)
      const cdp = win.webContents.debugger; cdp.attach('1.3');
      await cdp.sendCommand('Emulation.setFocusEmulationEnabled', { enabled: true });
      await cdp.sendCommand('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
      await cdp.sendCommand('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
      await sleep(150);
      await win.webContents.executeJavaScript(`window.symmetryS3Run(${JSON.stringify(platform)}).catch((e) => { window.symmetryS3Error = String((e && e.stack) || e); }); 0`);
      const deadline = Date.now() + 60000;
      let result = null;
      while (!result) {
        if (Date.now() > deadline) throw Error(platform + ': no verdict in 60 s');
        const st = await win.webContents.executeJavaScript('({ shot: window.symmetryShot || null, result: window.symmetryS3Result || null, error: window.symmetryS3Error || null })');
        if (st.error) throw Error(st.error);
        if (st.shot) {
          await win.webContents.executeJavaScript('document.fonts.ready.then(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))))');
          await sleep(250); win.webContents.invalidate(); await sleep(250);
          fs.writeFileSync(path.join(out, st.shot + '.png'), (await win.webContents.capturePage()).toPNG());
          shots.push(st.shot);
          await win.webContents.executeJavaScript('window.symmetryShot = null; window.symmetryShotDone(); 0');
        } else if (st.result) result = st.result;
        else await sleep(40);
      }
      result.session = { main: sessionOf(process.pid), renderer: sessionOf(win.webContents.getOSProcessId()) };
      results.push(result); win.destroy(); win = null;
    }
    const all = results.flatMap((r) => r.checks.map((c) => ({ ...c, platform: r.platform })));
    const failed = [...new Set(all.filter((c) => !c.ok).map((c) => c.name))].sort();
    const tagged = [...new Set(all.filter((c) => /^#\d+ /.test(c.name)).map((c) => c.name))].sort();
    const focus = Object.fromEntries(results.flatMap((r) => Object.entries(r.notes).filter(([k]) => k.startsWith('focus-visible:'))));
    const labels = results.map((r) => r.notes.systemLabel).filter(Boolean);
    const sessionsOk = results.every((r) => r.session.renderer === 0 && r.session.main === 0);
    const checksOk = red ? JSON.stringify(failed) === JSON.stringify(tagged) : failed.length === 0 && labels.length === 2 && labels[0] === labels[1];
    // in green every focused element must really be drawn as keyboard focus, or the screenshots show no ring
    const focusOk = red || Object.values(focus).every((v) => v === 'true');
    const passed = checksOk && sessionsOk && focusOk && shots.length >= 10;
    fs.writeFileSync(path.join(out, 'result.json'), JSON.stringify({ passed, phase: red ? 'matched red' : 'green', checks: all.length, failed, unexpectedPass: red ? tagged.filter((n) => !failed.includes(n)) : [], unexpectedFail: red ? failed.filter((n) => !tagged.includes(n)) : [], focus, labels, shots, sessions: results.map((r) => [r.platform, r.session]) }, null, 2));
    console.log(JSON.stringify({ passed, checks: all.length, failed: failed.length, focus, labels, shots: shots.length, sessions: results.map((r) => r.session) }, null, 1));
    app.exit(passed ? 0 : 1);
  } catch (e) { if (win && !win.isDestroyed()) win.destroy(); console.error(e.stack); app.exit(2); }
});
