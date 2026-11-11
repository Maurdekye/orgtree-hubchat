// Build Hubchat Web: the core (crates/hubchat-web) to WebAssembly, its
// JavaScript bindings, then the page.
//
//   node tools/web-build.mjs [--wasm <a built hubchat_web.wasm>] [--core <folder>] [--out <folder>]
//
// 1. cargo build -p hubchat-web --target wasm32-unknown-unknown --profile web --locked
//    (skipped with --wasm: the file is taken as built);
// 2. wasm-bindgen --target web into --core (default web/core);
// 3. vite build --config vite.web.config.ts into --out (default dist-web);
// 4. the size budget (spec R1.8): what a first visit downloads, compressed,
//    is at most 3 MB; over it the build fails.
//
// It needs: the Rust target wasm32-unknown-unknown; a clang that makes wasm32
// objects, because SQLite is C (on PATH, or CC_wasm32_unknown_unknown and
// AR_wasm32_unknown_unknown); and the wasm-bindgen CLI of exactly the version
// Cargo.lock pins for the wasm-bindgen crate (on PATH, or WASM_BINDGEN).
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const arg = (name) => { const i = process.argv.indexOf('--' + name); return i > 0 ? process.argv[i + 1] : null; };
const BUDGET = 3_000_000;

function run(what, cmd, args, env = {}) {
  console.log(`[web-build] ${what}`);
  const r = spawnSync(cmd, args, { cwd: root, stdio: 'inherit', env: { ...process.env, ...env }, shell: false });
  if (r.error) { console.error(`[web-build] ${what}: ${r.error.message}`); process.exit(1); }
  if (r.status !== 0) { console.error(`[web-build] ${what} failed (exit ${r.status})`); process.exit(r.status || 1); }
}

// 1. the core
let wasm = arg('wasm');
if (!wasm) {
  run('cargo build (hubchat-web, wasm32, profile web)', 'cargo', ['build', '-p', 'hubchat-web', '--target', 'wasm32-unknown-unknown', '--profile', 'web', '--locked']);
  wasm = join(process.env.CARGO_TARGET_DIR || join(root, 'target'), 'wasm32-unknown-unknown', 'web', 'hubchat_web.wasm');
}
if (!existsSync(wasm)) { console.error(`[web-build] no core at ${wasm}`); process.exit(1); }

// 2. its bindings: the CLI and the crate must be the same version
const lock = readFileSync(join(root, 'Cargo.lock'), 'utf8').replace(/\r\n/g, '\n');
const pinned = /\nname = "wasm-bindgen"\nversion = "([^"]+)"/.exec(lock)?.[1];
const bindgen = process.env.WASM_BINDGEN || 'wasm-bindgen';
const has = spawnSync(bindgen, ['--version'], { encoding: 'utf8' });
const cli = /wasm-bindgen (\S+)/.exec(has.stdout || '')?.[1];
if (!pinned || cli !== pinned) {
  console.error(`[web-build] the wasm-bindgen CLI is ${cli || 'missing'}, Cargo.lock pins ${pinned}. Install it: cargo install wasm-bindgen-cli --version ${pinned} --locked`);
  process.exit(1);
}
const core = resolve(arg('core') || join(root, 'web', 'core'));
mkdirSync(core, { recursive: true });
run(`wasm-bindgen ${cli}`, bindgen, ['--target', 'web', '--out-dir', core, '--out-name', 'hubchat_web', wasm]);

// 3. the page
const out = resolve(arg('out') || join(root, 'dist-web'));
run('vite build', process.execPath, [join(root, 'node_modules', 'vite', 'bin', 'vite.js'), 'build', '--config', 'vite.web.config.ts'], { HUBCHAT_WEB_CORE: core, HUBCHAT_WEB_OUT: out });

// 4. the budget
const files = [];
(function walk(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p);
    else if (/\.(wasm|js|css|html)$/.test(name)) files.push(p);
  }
})(out);
let total = 0;
for (const f of files.sort()) {
  const raw = readFileSync(f);
  const packed = gzipSync(raw, { level: 9 }).length;
  total += packed;
  console.log(`[web-build] ${String(raw.length).padStart(9)} B  ${String(packed).padStart(9)} B gzip  ${f.slice(out.length + 1).replace(/\\/g, '/')}`);
}
console.log(`[web-build] first load, compressed: ${total} B of ${BUDGET} B allowed (${(100 * total / BUDGET).toFixed(1)} %)`);
if (total > BUDGET) { console.error('[web-build] over the size budget (spec R1.8)'); process.exit(1); }
console.log(`[web-build] built ${out}`);
