// Writes Hubchat Web's service worker (sw.js) into a built page: the template
// web/sw.template.js with the build's version, a hash of its files, and their
// list. tools/web-build.mjs calls it after Vite; tools/web-e2e.mjs calls it to
// act out a newer build.
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
/** Not kept by the worker: itself, the server's settings (kept fresh), source maps. */
const skip = (rel) => rel === 'sw.js' || rel === 'config.json' || rel.endsWith('.map');

/** Write `out`/sw.js for the files now in `out`. Returns { version, build, files }. */
export function writeServiceWorker(out, version = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version) {
  const files = [];
  (function walk(dir, prefix) {
    for (const name of readdirSync(dir).sort()) {
      const p = join(dir, name);
      if (statSync(p).isDirectory()) walk(p, prefix + name + '/');
      else if (!skip(prefix + name)) files.push(prefix + name);
    }
  })(out, '');
  const hash = createHash('sha256');
  hash.update(version);
  for (const f of files) { hash.update('\0' + f + '\0'); hash.update(readFileSync(join(out, f))); }
  const build = hash.digest('hex').slice(0, 16);
  const text = readFileSync(join(root, 'web', 'sw.template.js'), 'utf8')
    .replace('"__VERSION__"', JSON.stringify(version))
    .replace('"__BUILD__"', JSON.stringify(build))
    .replace('__FILES__', JSON.stringify(files));
  writeFileSync(join(out, 'sw.js'), text);
  return { version, build, files };
}
