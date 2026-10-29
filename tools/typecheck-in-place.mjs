// Typechecks the Hubchat worktree in cwd (tsc --noEmit over src) with the dependencies read in place (no copy, no link).
// Run inside a kernel Job: HUBCHAT_DEPENDENCY_ROOT=<a checkout with node_modules> HUBCHAT_TSC_OUT=<new folder under C:/orgtree-build> node tools/typecheck-in-place.mjs
import fs from 'node:fs'; import path from 'node:path'; import {spawnSync} from 'node:child_process';
const root = process.cwd(), depsRoot = process.env.HUBCHAT_DEPENDENCY_ROOT, out = process.env.HUBCHAT_TSC_OUT;
fs.mkdirSync(out, {recursive: true});
const config = {extends: root + '/tsconfig.json', references: [], compilerOptions: {
  typeRoots: [depsRoot + '/node_modules/@types'], types: [depsRoot + '/node_modules/vite/client'],
  paths: {'*': [depsRoot + '/node_modules/*'], react: [depsRoot + '/node_modules/@types/react'], 'react/*': [depsRoot + '/node_modules/@types/react/*'],
    'react-dom': [depsRoot + '/node_modules/@types/react-dom'], 'react-dom/*': [depsRoot + '/node_modules/@types/react-dom/*'], qrcode: [depsRoot + '/node_modules/@types/qrcode']}}};
fs.writeFileSync(out + '/typecheck.json', JSON.stringify(config));
const t = spawnSync(process.execPath, [depsRoot + '/node_modules/typescript/bin/tsc', '-p', out + '/typecheck.json', '--noEmit'], {cwd: root, encoding: 'utf8', windowsHide: true, timeout: 200000});
fs.writeFileSync(out + '/typecheck.log', t.stdout + t.stderr);
console.log('tsc exit', t.status, (t.stdout + t.stderr).slice(-1500));
process.exit(t.status ?? 2);
