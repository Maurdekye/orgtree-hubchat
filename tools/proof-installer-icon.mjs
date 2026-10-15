// Contained NSIS packaging proof, run only inside the rig's shared lock/Job.
// Uses the real Tauri bundler and exact source template, but a never-run C#
// stand-in payload. No cargo/app/frontend build, install, registry write or publish.
// node tools/proof-installer-icon.mjs <new-own-C-output> <existing-tauri.js>
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { windowsFrontendDirectory } from '../scripts/local-tauri-config.mjs';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const [output, cli] = process.argv.slice(2);
if (process.platform !== 'win32' || !output || !cli) throw new Error('Windows: supply new own C output and existing tauri.js');
const out = path.resolve(output);
if (!out.toLowerCase().startsWith('c:\\orgtree-build\\') || fs.existsSync(out)) {
  throw new Error('Output must be a NEW directory below the caller\'s granted C:/orgtree-build scope');
}
fs.mkdirSync(out, { recursive: true });
for (const name of ['tmp', 'captured', 'frontend', 'target/debug']) fs.mkdirSync(path.join(out, name), { recursive: true });
const env = { ...process.env, CARGO_TARGET_DIR: path.join(out, 'target'), CARGO_BUILD_JOBS: '2',
  TEMP: path.join(out, 'tmp'), TMP: path.join(out, 'tmp') };
function run(exe, args) {
  console.log(JSON.stringify({ exe, args }));
  const result = spawnSync(exe, args, { cwd: repo, env, stdio: 'inherit', windowsHide: true });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${path.basename(exe)} exited ${result.status}`);
}
// Custom signing callback only exports resource files for inspection. It never
// signs, executes or installs them. Tauri invokes it at uninstaller finalization.
const capture = path.join(out, 'capture.ps1');
fs.writeFileSync(capture, `param([Parameter(Mandatory)][string]$Binary)
$ErrorActionPreference = 'Stop'
$destination = Join-Path $PSScriptRoot ('captured/' + [IO.Path]::GetFileName($Binary))
Copy-Item -LiteralPath $Binary -Destination $destination -Force
Write-Output ('RESOURCE_CAPTURE ' + $destination)
`);
fs.writeFileSync(path.join(out, 'payload.cs'), 'class Payload { static void Main() {} }\n');
fs.writeFileSync(path.join(out, 'frontend/index.html'), '<!doctype html><title>Contained icon proof; no app runtime</title>\n');
const csc = path.join(process.env.WINDIR, 'Microsoft.NET/Framework64/v4.0.30319/csc.exe');
run(csc, ['/nologo', '/target:winexe', '/out:' + path.join(out, 'target/debug/hubchat.exe'),
  '/win32icon:' + path.join(repo, 'src-tauri/icons/icon.ico'), path.join(out, 'payload.cs')]);
const override = {
  productName: 'Hubchat Icon Test', identifier: 'dev.orgtree.hubchat.icon.test',
  build: { beforeBuildCommand: '', beforeBundleCommand: '', frontendDist: windowsFrontendDirectory(path.join(out, 'frontend')) },
  bundle: { createUpdaterArtifacts: false, windows: {
    webviewInstallMode: { type: 'skip' },
    signCommand: { cmd: 'powershell.exe', args: ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', capture, '%1'] },
  } },
};
const overridePath = path.join(out, 'proof.conf.json');
fs.writeFileSync(overridePath, JSON.stringify(override, null, 2) + '\n');
// Keep installer/uninstallerIcon absent in the override: these must come from
// the checked-in config so the same runner meaningfully fails on the base.
run(process.execPath, [path.resolve(cli), 'bundle', '--debug', '--bundles', 'nsis',
  '--config', overridePath, '--ci', '--no-binary-patching']);
console.log('Contained bundles: ' + path.join(out, 'target/debug/bundle/nsis'));
console.log('Uninstaller export: ' + path.join(out, 'captured'));
console.log('Do not install or click Next. Inspect PE resources and Welcome-window icon only.');
