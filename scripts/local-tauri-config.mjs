// Local Windows packaging: frontendDist tries URL before Directory. A drive
// path such as C:/... is a valid URL, so use an unambiguous verbatim path.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

export function windowsFrontendDirectory(directory) {
  if (!/^[a-z]:[\\/]/i.test(directory)) throw new Error('Expected an absolute Windows drive directory');
  const resolved = path.win32.normalize(directory);
  const result = path.win32.toNamespacedPath(resolved);
  let isUrl = false;
  try { new URL(result); isUrl = true; } catch {}
  if (isUrl || !result.startsWith('\\\\?\\')) throw new Error('Frontend directory is URL-ambiguous');
  return result;
}

export function checkFrontend(directory) {
  const real = fs.realpathSync.native(directory);
  if (real.toLowerCase() !== path.resolve(directory).toLowerCase()) throw new Error('Frontend directory must not be linked');
  const html = fs.readFileSync(path.join(directory, 'index.html'), 'utf8');
  if (!html.includes('<div id="root"></div>')) throw new Error('Missing application root');
  const assets = [...html.matchAll(/(?:src|href)="(\/assets\/[^"?#]+)"/g)].map(m => m[1]);
  if (!assets.some(a => a.endsWith('.js')) || !assets.some(a => a.endsWith('.css'))) throw new Error('Missing built frontend entry assets');
  for (const asset of assets) {
    const target = path.resolve(directory, '.' + asset);
    if (!target.startsWith(path.resolve(directory) + path.sep) || !fs.statSync(target).isFile()) throw new Error('Missing or invalid frontend asset');
  }
  return assets;
}

export function localConfig(directory) {
  checkFrontend(directory);
  return {build: {frontendDist: windowsFrontendDirectory(directory), beforeBuildCommand: ''}, bundle: {createUpdaterArtifacts: false}};
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [directory, output] = process.argv.slice(2);
  if (!directory || !output) throw new Error('Usage: node scripts/local-tauri-config.mjs <built-frontend-directory> <output-config.json>');
  fs.writeFileSync(output, JSON.stringify(localConfig(directory), null, 2) + '\n');
  console.log('Local frontend configuration written; actual Tauri parser and rendered startup must also be checked.');
}
