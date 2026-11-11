// Writes and checks build-info-web.json, the record of a release's Hubchat
// Web files (spec R6.4):
//
//   node build-info.mjs <out> <version> <commit> <program> <image id> <built page>   write it
//   node build-info.mjs --check <folder> <version>                                    check it
//
// { version, commit,
//   files:   { "<asset name>": "<sha256>" }     the static archive and the image file
//   page:    { "<path>": "<sha256>" }           every file of the built page
//   program: { sha256, bytes }                  /hubchat-web inside the image
//   image:   { name, id }                       hubchat-web:<version>, as `docker load` gives it
// }
// --check: the two asset files are in <folder>, hash as recorded, and the
// record is for <version>. Used when the release is staged, and again
// before the image is pushed.
import crypto from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const sha256 = (file) => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex')
const fail = (message) => { console.log(`::error::${message}`); process.exit(1) }
export const assetNames = (version) => [`hubchat-web-${version}-static.tar.gz`, `hubchat-web-${version}-docker-amd64.tar.gz`]

/** Problems with the Hubchat Web files of a release folder; empty when they are right. */
export function problems(folder, version) {
  const found = []
  const infoFile = path.join(folder, 'build-info-web.json')
  if (!fs.existsSync(infoFile)) return ['build-info-web.json is missing']
  let info
  try { info = JSON.parse(fs.readFileSync(infoFile, 'utf8')) } catch (e) { return [`build-info-web.json is not JSON: ${e.message}`] }
  if (info.version !== version) found.push(`build-info-web.json is for ${info.version}, not ${version}`)
  if (!/^[0-9a-f]{40}$/.test(info.commit ?? '')) found.push('build-info-web.json names no commit')
  for (const name of assetNames(version)) {
    const file = path.join(folder, name)
    if (!fs.existsSync(file)) found.push(`${name} is missing`)
    else if (info.files?.[name] !== sha256(file)) found.push(`${name} is not the file build-info-web.json records`)
  }
  if (!/^[0-9a-f]{64}$/.test(info.program?.sha256 ?? '')) found.push('build-info-web.json records no program')
  if (info.image?.name !== `hubchat-web:${version}`) found.push(`build-info-web.json's image is ${info.image?.name}, not hubchat-web:${version}`)
  if (!info.page?.['index.html'] || !info.page?.['sw.js']) found.push("build-info-web.json's page has no index.html or sw.js")
  return found
}

// run as a program (assemble.mjs imports `problems` instead)
if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  if (process.argv[2] === '--check') {
    const [folder, version] = process.argv.slice(3)
    if (!folder || !version) fail('usage: node build-info.mjs --check <folder> <version>')
    const found = problems(folder, version)
    for (const p of found) console.log(`::error::${p}`)
    if (found.length) process.exit(1)
    console.log(`Hubchat Web ${version}: the static archive and the image are the files build-info-web.json records`)
  } else {
    const [out, version, commit, program, imageId, dist] = process.argv.slice(2)
    if (!out || !version || !commit || !program || !imageId || !dist) fail('usage: node build-info.mjs <out> <version> <commit> <program> <image id> <built page>')
    const page = {}
    const walk = (dir, rel) => {
      for (const name of fs.readdirSync(dir).sort()) {
        const full = path.join(dir, name)
        if (fs.statSync(full).isDirectory()) walk(full, rel + name + '/')
        else page[rel + name] = sha256(full)
      }
    }
    walk(dist, '')
    const info = {
      version, commit,
      files: Object.fromEntries(assetNames(version).map((name) => [name, sha256(path.join(out, name))])),
      page,
      program: { sha256: sha256(program), bytes: fs.statSync(program).size },
      image: { name: `hubchat-web:${version}`, id: imageId.trim() },
    }
    fs.writeFileSync(path.join(out, 'build-info-web.json'), JSON.stringify(info, null, 2) + '\n')
    const found = problems(out, version)
    if (found.length) fail(found.join('; '))
    console.log(`build-info-web.json: ${Object.keys(page).length} page files, program ${info.program.sha256.slice(0, 12)}..., image ${info.image.id.slice(0, 19)}...`)
  }
}
