// Turns a signed Hubchat release folder into its final asset set, in the formats
// the hand-made 0.1.x releases used (hubchat-opus's make-release.mjs):
//   latest.json    the updater feed: version, notes, pub_date, and one platform
//                  entry per updater-*.json fragment ({"<key>": {asset, signature}})
//   SHA256SUMS.txt "<sha256>  <file>" for every other file, sorted, LF
// The fragments are consumed. Run from the checked-out release source:
//   MODE=release|test VERSION=<x.y.z> node assemble.mjs <dist>
import fs from 'node:fs'
import path from 'node:path'
import crypto from 'node:crypto'

const dist = process.argv[2]
const { MODE: mode, VERSION: version, GITHUB_REPOSITORY: repo = 'Maurdekye/orgtree-hubchat' } = process.env
if (!dist || !version || !['release', 'test'].includes(mode)) fail('usage: MODE=release|test VERSION=x.y.z node assemble.mjs <dist>')
const file = name => path.join(dist, name)
const sha256 = name => crypto.createHash('sha256').update(fs.readFileSync(file(name))).digest('hex')

// The update text shown in the app (docs/update-notes-<v>.txt; required for a real release).
const notesFile = `docs/update-notes-${version}.txt`
const notes = fs.existsSync(notesFile) ? fs.readFileSync(notesFile, 'utf8').trim() : ''
if (!notes && mode === 'release') fail(`${notesFile} is missing or empty`)

const platforms = {}
for (const name of fs.readdirSync(dist).filter(n => /^updater-.+\.json$/.test(n)).sort()) {
  for (const [key, entry] of Object.entries(JSON.parse(fs.readFileSync(file(name), 'utf8')))) {
    if (platforms[key]) fail(`two fragments define platform ${key}`)
    if (!entry?.asset || !fs.existsSync(file(entry.asset))) fail(`${name}: ${key} names a missing asset ${entry?.asset}`)
    const sig = fs.readFileSync(file(entry.asset + '.sig'), 'utf8').trim()
    if (entry.signature?.trim() !== sig) fail(`${name}: ${key}'s signature is not ${entry.asset}.sig`)
    platforms[key] = { signature: sig, url: `https://github.com/${repo}/releases/download/v${version}/${entry.asset}` }
  }
  fs.rmSync(file(name))
}
if (!Object.keys(platforms).length) fail('no updater-*.json fragments: latest.json would be empty')
for (const required of ['windows-x86_64']) if (!platforms[required]) fail(`latest.json has no ${required} entry`)

// The fixed-name APK (phone linking downloads releases/latest/download/Hubchat-android.apk)
// must be the versioned APK byte for byte.
const apk = `Hubchat_${version}_arm64.apk`
if (!fs.existsSync(file(apk)) || !fs.existsSync(file('Hubchat-android.apk'))) fail(`missing ${apk} or Hubchat-android.apk`)
if (sha256(apk) !== sha256('Hubchat-android.apk')) fail('Hubchat-android.apk differs from ' + apk)

const latest = { version, notes, pub_date: new Date().toISOString().replace(/\.\d+Z$/, 'Z'), platforms }
fs.writeFileSync(file('latest.json'), JSON.stringify(latest, null, 2) + '\n')

const names = fs.readdirSync(dist).filter(n => n !== 'SHA256SUMS.txt').sort()
const sums = names.map(n => `${sha256(n)}  ${n}`).join('\n') + '\n'
fs.writeFileSync(file('SHA256SUMS.txt'), sums)
console.log(`latest.json platforms: ${Object.keys(platforms).join(', ')}`)
for (const n of [...names, 'SHA256SUMS.txt']) console.log(`${String(fs.statSync(file(n)).size).padStart(12)}  ${n}`)
fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY ?? '/dev/null',
  `### Hubchat ${version} (${mode})\n\n| file | bytes | sha256 |\n|---|---:|---|\n`
  + names.map(n => `| ${n} | ${fs.statSync(file(n)).size} | \`${sha256(n)}\` |`).join('\n') + '\n')

function fail(message) {
  console.log(`::error::${message}`)
  process.exit(1)
}
