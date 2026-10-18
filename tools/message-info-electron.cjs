// Hidden isolated Chromium capture, using both real platform stylesheet stacks.
const fs = require('node:fs'), path = require('node:path');
const { app, BrowserWindow } = require('electron');
const out = process.env.HUBCHAT_MESSAGE_INFO_OUT;
app.disableHardwareAcceleration();
app.setPath('userData', path.join(out, 'user-data'));
app.whenReady().then(async () => {
  const win = new BrowserWindow({ show: false, width: 940, height: 600, webPreferences: { backgroundThrottling: false } });
  win.webContents.session.webRequest.onBeforeRequest({ urls: ['http://*/*', 'https://*/*'] }, (_, cb) => cb({ cancel: true }));
  try {
    await win.loadFile(path.join(out, 'capture.html'));
    const result = await win.webContents.executeJavaScript(`(async () => {
      const frames = [...document.querySelectorAll('iframe')];
      await Promise.all(frames.map(f => f.contentWindow.messageInfoResult ? Promise.resolve() : new Promise(r => f.addEventListener('load', r, {once:true}))));
      await Promise.all(frames.map(f => f.contentDocument.fonts.ready));
      await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
      await new Promise(r => setTimeout(r, 120));
      return frames.map(f => f.contentWindow.messageInfoResult);
    })()`);
    fs.writeFileSync(path.join(out, 'result.json'), JSON.stringify(result, null, 2));
    if (result.some(r => !r || !r.checks.length)) throw Error('Fixture did not execute');
    if (process.env.HUBCHAT_PROOF_PHASE !== 'red') fs.writeFileSync(path.join(out, 'message-info.png'), (await win.webContents.capturePage()).toPNG());
    console.log(JSON.stringify(result.map(r => ({ platform: r.platform, total: r.checks.length, failed: r.failed, failures: r.checks.filter(c => !c.ok) }))));
    app.exit(result.some(r => r.failed) ? 1 : 0);
  } catch (e) { console.error(e.stack); app.exit(2); }
});
