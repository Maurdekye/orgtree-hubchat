// Try a built Hubchat Web (`npm run build:web`) on this machine:
//
//   node tools/web-serve.mjs [--dist dist-web] [--port 8787] [--host 127.0.0.1] [--default-hubs https://hub.example]
//
// It listens on this machine only (a browser treats http://localhost as
// secure). A phone needs HTTPS in front of it, and the hub must name the
// page's https address among its web app addresses: docs/hubchat-web.md,
// "Trying it on a phone". This is a development server; the container's own
// server (spec section 9) replaces it.
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { webServer } from './web-static.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const arg = (name, fallback) => { const i = process.argv.indexOf('--' + name); return i > 0 && process.argv[i + 1] ? process.argv[i + 1] : fallback; };
const dist = resolve(arg('dist', join(here, '..', 'dist-web')));
const port = Number(arg('port', 8787)), host = arg('host', '127.0.0.1');
if (!existsSync(join(dist, 'index.html'))) { console.error(`no built page in ${dist}: run npm run build:web first`); process.exit(2); }

const served = [];
// --default-hubs https://hub.example,https://other.example: offered to someone adding their first hub
const hubs = String(arg('default-hubs', '')).split(',').map((h) => h.trim()).filter(Boolean);
const server = webServer(dist, served, hubs.length ? { name: 'Hubchat Web', default_hubs: hubs } : null);
server.on('error', (e) => { console.error(`can't listen on ${host}:${port}: ${e.message}`); process.exit(1); });
server.listen(port, host, () => {
  console.log(`Hubchat Web (${dist}) on http://${host === '0.0.0.0' || host === '::' ? 'localhost' : host}:${port}`);
  console.log('For a phone, put HTTPS in front, for example with Tailscale:');
  console.log(`  tailscale serve --bg --https=443 http://127.0.0.1:${port}`);
  console.log("and add that https address to the hub's web app addresses (HUB_WEB_ORIGINS).");
  console.log('Ctrl+C stops it.');
});
// one line per request, so a first visit's downloads can be read off
setInterval(() => { for (const s of served.splice(0)) console.log(`${s.status} ${String(s.sent).padStart(8)} B  ${s.path}`); }, 500);
