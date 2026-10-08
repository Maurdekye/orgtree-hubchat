// Makes src/assets/desktop-footer-qr.png: a crop of the desktop sidebar
// footer ("You @net:… [copy] [QR]") from the shipped UI, shown on Android's
// "I already use Hubchat" screen so people know which button to click on the
// PC. Also writes src/assets/desktop-footer-qr.json: where the QR button sits
// in the crop (percent), for the highlight ring drawn over it in CSS.
//
// RE-RUN THIS WHENEVER THE DESKTOP SIDEBAR FOOTER CHANGES (its layout, the
// QR button, icons, fonts or colours), and commit both files.
//
// usage: start the browser mock first,  npx vite --port 1430 --strictPort
//        then                           node tools/make-footer-crop.mjs [url]
import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { launch } from './cdp.mjs';

const url = process.argv[2] || 'http://localhost:1430/';
const assets = join(dirname(fileURLToPath(import.meta.url)), '..', 'src', 'assets');
const b = await launch({ width: 1280, height: 800, dpr: 2 });
try {
  await b.nav(url, 1500);
  // the mock's default (desktop, signed in, dark theme); no hover, no focus ring
  await b.eval(`document.activeElement && document.activeElement.blur()`);
  const where = await b.eval(`(() => {
    const f = document.querySelector('.side-foot'), q = document.querySelector('.side-foot .side-qr');
    if (!f || !q) return null;
    const a = f.getBoundingClientRect(), r = q.getBoundingClientRect();
    const pct = (v, t) => Math.round(v / t * 10000) / 100;
    return { width: Math.round(a.width), height: Math.round(a.height), x: pct(r.left - a.left, a.width), y: pct(r.top - a.top, a.height), w: pct(r.width, a.width), h: pct(r.height, a.height) };
  })()`);
  if (!where) throw new Error('no .side-foot with a .side-qr button at ' + url);
  await b.shot(join(assets, 'desktop-footer-qr.png'), '.side-foot', 0);
  writeFileSync(join(assets, 'desktop-footer-qr.json'), JSON.stringify(where, null, 2) + '\n');
  console.log('wrote desktop-footer-qr.png/.json', where, b.errors.length ? '\n' + b.errors.join('\n') : '');
} finally {
  await b.close();
}
