// Checks a just-published Hubchat release the way installed apps will see it:
// - every public asset downloads and matches SHA256SUMS.txt, and none is missing;
// - latest.json names this version, and each platform entry points at an asset of
//   this release whose .sig is exactly the entry's signature and binds this version;
// - Hubchat-android.apk is the versioned APK byte for byte;
// - GitHub's "latest" release is this one (installed apps read latest/download/latest.json).
//   TAG=v<x.y.z> GITHUB_REPOSITORY=owner/repo GH_TOKEN=... node verify-published.mjs
import crypto from 'node:crypto'

const { TAG: tag, GITHUB_REPOSITORY: repo, GH_TOKEN: token } = process.env
const version = /^v(\d+\.\d+\.\d+)$/.exec(tag ?? '')?.[1] ?? fail(`not a version tag: ${tag}`)
const api = async path => {
  const response = await fetch(`https://api.github.com/repos/${repo}/${path}`, {
    headers: { Accept: 'application/vnd.github+json', Authorization: `Bearer ${token}`, 'User-Agent': 'hubchat-verify' } })
  if (!response.ok) fail(`GET ${path}: HTTP ${response.status}`)
  return response.json()
}
const problems = []
const check = (ok, message) => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${message}`); if (!ok) problems.push(message) }

const release = await api(`releases/tags/${encodeURIComponent(tag)}`)
check(!release.draft && !release.prerelease, `release ${tag} is public and not a prerelease`)

const bytes = {}
for (const asset of release.assets) {
  const response = await fetch(asset.browser_download_url)
  if (!response.ok) { check(false, `${asset.name} downloads (HTTP ${response.status})`); continue }
  bytes[asset.name] = Buffer.from(await response.arrayBuffer())
  check(bytes[asset.name].length === asset.size, `${asset.name}: ${asset.size} bytes`)
}
const sha256 = name => crypto.createHash('sha256').update(bytes[name]).digest('hex')
for (const name of ['latest.json', 'SHA256SUMS.txt', `Hubchat_${version}_arm64.apk`, 'Hubchat-android.apk']) {
  check(name in bytes, `asset ${name} is present`)
}
if ('SHA256SUMS.txt' in bytes) {
  const listed = new Map(bytes['SHA256SUMS.txt'].toString('utf8').trim().split('\n').map(line => line.split(/\s+\*?/).reverse()))
  for (const name of Object.keys(bytes).filter(n => n !== 'SHA256SUMS.txt')) {
    check(listed.get(name) === sha256(name), `${name} matches SHA256SUMS.txt`)
  }
  for (const name of listed.keys()) check(name in bytes, `${name} (in SHA256SUMS.txt) is published`)
}
if (`Hubchat_${version}_arm64.apk` in bytes && 'Hubchat-android.apk' in bytes) {
  check(sha256(`Hubchat_${version}_arm64.apk`) === sha256('Hubchat-android.apk'), 'Hubchat-android.apk is the versioned APK')
}
if ('latest.json' in bytes) {
  const latest = JSON.parse(bytes['latest.json'].toString('utf8'))
  check(latest.version === version, `latest.json version ${latest.version}`)
  const prefix = `https://github.com/${repo}/releases/download/${tag}/`
  for (const [platform, entry] of Object.entries(latest.platforms ?? {})) {
    const name = entry.url?.startsWith(prefix) ? decodeURIComponent(entry.url.slice(prefix.length)) : null
    check(name !== null && name in bytes, `latest.json ${platform} points at an asset of ${tag} (${entry.url})`)
    if (!name || !(name in bytes)) continue
    const sig = bytes[`${name}.sig`]?.toString('utf8').trim()
    check(sig !== undefined && entry.signature === sig, `latest.json ${platform} signature is ${name}.sig`)
    const comment = sig ? Buffer.from(sig, 'base64').toString('utf8') : ''
    check(comment.includes(`version:${version}`), `${name}.sig binds version ${version} (requireSignedVersion)`)
  }
  check('windows-x86_64' in (latest.platforms ?? {}), 'latest.json has windows-x86_64')
}
const latestRelease = await api('releases/latest')
check(latestRelease.tag_name === tag, `"latest" is ${tag} (it is ${latestRelease.tag_name})`)

if (problems.length) fail(`${problems.length} problem(s) with the published ${tag}; see docs/ci-release.md, "Rolling back"`)
console.log(`${tag} is published correctly`)

function fail(message) {
  console.log(`::error::${message}`)
  process.exit(1)
}
