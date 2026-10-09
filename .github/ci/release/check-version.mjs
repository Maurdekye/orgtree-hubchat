// Checks a Hubchat release before anything is built, from the checked-out source:
// the tag is a plain v<major>.<minor>.<patch>, every place the version is written
// says the same, and (for a real release) both notes files exist. Writes mode, tag,
// version and sha to GITHUB_OUTPUT for the later jobs.
//   MODE=release|test TAG=<pushed tag or empty> node check-version.mjs
// In test mode the tag is v<package.json version> and missing notes are warnings.
import fs from 'node:fs'
import { execFileSync } from 'node:child_process'

const mode = process.env.MODE
if (mode !== 'release' && mode !== 'test') fail(`MODE must be release or test, got "${mode}"`)
const read = file => fs.readFileSync(file, 'utf8')
const json = file => JSON.parse(read(file))

const tag = process.env.TAG || (mode === 'test' ? `v${json('package.json').version}` : '')
const version = /^v(\d+\.\d+\.\d+)$/.exec(tag)?.[1]
if (!version) fail(`a release tag looks like v1.2.3, got "${tag}"`)

// Every place a version bump touches (hubchat-opus's list, 2026-10-09). The browser
// mock in native.ts is matched by shape, so a refactor there only warns.
const lock = json('package-lock.json')
const surfaces = [
  ['Cargo.toml [workspace.package] version', /\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/m.exec(read('Cargo.toml'))?.[1]],
  ['Cargo.lock package hubchat', /^name = "hubchat"\r?\nversion = "([^"]+)"/m.exec(read('Cargo.lock'))?.[1]],
  ['Cargo.lock package hubchat-core', /^name = "hubchat-core"\r?\nversion = "([^"]+)"/m.exec(read('Cargo.lock'))?.[1]],
  ['package.json version', json('package.json').version],
  ['package-lock.json version', lock.version],
  ['package-lock.json packages[""].version', lock.packages?.['']?.version],
  ['src-tauri/tauri.conf.json version', json('src-tauri/tauri.conf.json').version],
]
const mock = /if \(!isTauri\) return "([^"]+)"/.exec(read('src/lib/native.ts'))?.[1]
let bad = 0
for (const [where, found] of surfaces) {
  const ok = found === version
  if (!ok) bad += 1
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${where}: ${found ?? '(not found)'}`)
}
if (mock === undefined) console.log(`::warning::src/lib/native.ts: the browser mock's version line was not found; check it says ${version}`)
else {
  if (mock !== version) bad += 1
  console.log(`${mock === version ? 'ok  ' : 'FAIL'} src/lib/native.ts browser mock: ${mock}`)
}
if (bad) fail(`${bad} version surface(s) differ from ${tag}; bump them all in one commit and tag that commit`)

for (const notes of [`docs/release-notes-${version}.md`, `docs/update-notes-${version}.txt`]) {
  const present = fs.existsSync(notes) && read(notes).trim().length > 0
  if (present) console.log(`ok   ${notes}`)
  else if (mode === 'release') fail(`${notes} is missing or empty: the draft's body and latest.json's notes come from it`)
  else console.log(`::warning::${notes} is missing (fine for a test run)`)
}

const sha = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim()
console.log(`${mode} ${tag} = ${sha}`)
fs.appendFileSync(process.env.GITHUB_OUTPUT, `mode=${mode}\ntag=${tag}\nversion=${version}\nsha=${sha}\n`)

function fail(message) {
  console.log(`::error::${message}`)
  process.exit(1)
}
