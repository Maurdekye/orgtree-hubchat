// Serve a built Hubchat Web (tools/web-build.mjs) the way its server is
// specified to (spec R3.4): every answer carries the Content Security
// Policy, text is compressed, files with a hash in their name are kept by
// the browser for good and everything else is asked for again each time.
// The end-to-end test (web-e2e.mjs) and `npm run serve:web` (web-serve.mjs)
// use it; the container's own server will replace it.
import { existsSync, readFileSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { extname, join, normalize, resolve, sep } from 'node:path';
import { gzipSync } from 'node:zlib';

/** What the page may load and call: itself, and hubs over https (or on this machine). */
export const CSP = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; media-src blob:; "
  + "connect-src https: http://localhost:* http://127.0.0.1:*; worker-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'";

const TYPES = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.wasm': 'application/wasm',
  '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.ico': 'image/x-icon', '.webmanifest': 'application/manifest+json' };

/** An HTTP server for the built page in `dist` (not listening yet). `served` gets one entry per answer:
 *  { path, status, raw, sent }, the file's size and what went over the wire. */
export function webServer(dist, served = [], config = null) {
  const root = resolve(dist);
  const packed = new Map();
  return createServer((req, res) => {
    const head = { 'content-security-policy': CSP, 'x-content-type-options': 'nosniff', 'referrer-policy': 'no-referrer' };
    const refuse = (status, path) => { served.push({ path, status, raw: 0, sent: 0 }); res.writeHead(status, { ...head, 'content-type': 'text/plain; charset=utf-8' }); res.end(status === 404 ? 'not found' : 'refused'); };
    if (req.method !== 'GET' && req.method !== 'HEAD') { refuse(405, String(req.url)); return; }
    let rel;
    try { rel = decodeURIComponent(new URL(req.url, 'http://x').pathname); } catch { refuse(400, String(req.url)); return; }
    if (rel.endsWith('/')) rel += 'index.html';
    // what the container answers from its settings (spec R6.1); here, from `config`
    if (rel === '/config.json' && config) {
      const body = Buffer.from(JSON.stringify(config));
      served.push({ path: rel, status: 200, raw: body.length, sent: body.length });
      res.writeHead(200, { ...head, 'cache-control': 'no-cache', 'content-type': 'application/json', 'content-length': body.length });
      res.end(req.method === 'HEAD' ? undefined : body);
      return;
    }
    const file = normalize(join(root, rel));
    if (!file.startsWith(root + sep) || !existsSync(file) || !statSync(file).isFile()) { refuse(404, rel); return; }
    const raw = statSync(file).size;
    // Vite puts a hash of the content in the name of everything under assets/
    head['cache-control'] = rel.startsWith('/assets/') ? 'public, max-age=31536000, immutable' : 'no-cache';
    head['content-type'] = TYPES[extname(file).toLowerCase()] || 'application/octet-stream';
    let body = readFileSync(file);
    if (/\.(html|js|css|wasm|json|svg|webmanifest)$/.test(file) && /\bgzip\b/.test(req.headers['accept-encoding'] || '')) {
      // by the file as it is now: a page rebuilt while the server runs is served new
      const key = file + '|' + raw + '|' + statSync(file).mtimeMs;
      if (!packed.has(key)) packed.set(key, gzipSync(body, { level: 9 }));
      body = packed.get(key);
      head['content-encoding'] = 'gzip';
      head.vary = 'Accept-Encoding';
    }
    served.push({ path: rel, status: 200, raw, sent: body.length });
    res.writeHead(200, { ...head, 'content-length': body.length });
    res.end(req.method === 'HEAD' ? undefined : body);
  });
}
