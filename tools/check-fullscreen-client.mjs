// The Android fullscreen client (src-tauri/gen/android/.../VideoFullscreen.kt) forwards every call wry's chrome
// client overrides, by delegation, so the file chooser and permission prompts work exactly as before (inline video,
// Hubchat 1.2.7; coordinator 2026-10-30: "forward every one of wry's overrides explicitly, with a unit or
// compile-time check that none is missed"). Read against the wry that Cargo.lock pins, from Cargo's registry (or the
// copy an Android build generated). Checks, as PASS/FAIL lines:
//   - the client overrides exactly wry's methods, with the same parameter types;
//   - each one but the fullscreen pair is the single call inner.<same method>(<same parameters, in order>);
//   - the fullscreen pair is the client's own (no call into wry's, which refuses fullscreen);
//   - MainActivity installs it once wry's client is set.
//   node tools/check-fullscreen-client.mjs [repo]   (default: the current folder)
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';

const repo = resolve(process.argv[2] || '.');
const results = [];
const check = (name, ok, detail = '') => { results.push(!!ok); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${ok || !detail ? '' : ' — ' + detail}`); };
const app = join(repo, 'src-tauri/gen/android/app/src/main/java/dev/orgtree/hubchat');
const OURS = join(app, 'VideoFullscreen.kt'), MAIN = join(app, 'MainActivity.kt');

const lock = readFileSync(join(repo, 'Cargo.lock'), 'utf8');
const version = /\[\[package\]\]\s*name = "wry"\s*version = "([^"]+)"/.exec(lock)?.[1];
const registry = join(process.env.CARGO_HOME || join(homedir(), '.cargo'), 'registry', 'src');
const candidates = [
  ...(version && existsSync(registry) ? readdirSync(registry).map((d) => join(registry, d, `wry-${version}`, 'src/android/kotlin/RustWebChromeClient.kt')) : []),
  join(app, 'generated/RustWebChromeClient.kt'),
];
const WRY = candidates.find(existsSync);
check(`wry's chrome client is found (wry ${version} in Cargo.lock)`, !!WRY, candidates.join(', '));
console.log(`INFO wry's client: ${WRY ?? 'none'}`);

/** The class's own `override fun name(params)[: R] [= body | { body }]` (its members, two spaces in; not those of an
 *  object inside it): name, parameter names and types, body. */
function overrides(src) {
  const out = [];
  const re = /^ {2}override fun (\w+)\s*\(([^)]*)\)\s*(?::\s*([\w.<>?, ]+?))?\s*(=|\{)/gm;
  for (let m; (m = re.exec(src));) {
    const params = m[2].split(/,(?![^<]*>)/).map((p) => p.trim()).filter(Boolean).map((p) => {
      const [n, ...t] = p.split(':');
      return { name: n.trim(), type: t.join(':').replace(/\s+/g, '') };
    });
    let body;
    if (m[4] === '=') {
      // a single expression: up to the next declaration
      const rest = src.slice(re.lastIndex);
      const end = rest.search(/\n\s*(override |fun |private |\/\/|\}\s*$)/);
      body = (end < 0 ? rest : rest.slice(0, end)).trim();
    } else {
      let depth = 1, i = re.lastIndex;
      while (depth && i < src.length) { if (src[i] === '{') depth++; else if (src[i] === '}') depth--; i++; }
      body = src.slice(re.lastIndex, i - 1).trim();
    }
    out.push({ name: m[1], params, sig: m[1] + '(' + params.map((p) => p.type).join(',') + ')', body });
  }
  return out;
}

const ours = existsSync(OURS) ? overrides(readFileSync(OURS, 'utf8')) : [];
check('the fullscreen client exists', existsSync(OURS), OURS);
const theirs = WRY ? overrides(readFileSync(WRY, 'utf8')) : [];
console.log(`INFO wry overrides ${theirs.length}: ${theirs.map((o) => o.sig).join(' ')}`);
const sigs = (list) => list.map((o) => o.sig).sort();
const missing = sigs(theirs).filter((s) => !sigs(ours).includes(s)), extra = sigs(ours).filter((s) => !sigs(theirs).includes(s));
check("it overrides exactly wry's methods, with the same parameter types", theirs.length > 0 && !missing.length && !extra.length,
  `missing ${JSON.stringify(missing)}, extra ${JSON.stringify(extra)}`);

const FULLSCREEN = ['onShowCustomView', 'onHideCustomView'];
const forwarded = ours.filter((o) => !FULLSCREEN.includes(o.name));
const getter = (n) => n.startsWith('get') ? n[3].toLowerCase() + n.slice(4) : null;
const delegates = (o) => {
  const call = `inner.${o.name}(${o.params.map((p) => p.name).join(', ')})`;
  const body = o.body.replace(/\s+/g, ' ').trim();
  return body === call || (!o.params.length && getter(o.name) && body === `inner.${getter(o.name)}`);
};
const copied = forwarded.filter((o) => !delegates(o));
check("each of the others is one call to wry's client with the same parameters, in order", forwarded.length === theirs.length - FULLSCREEN.length && !copied.length,
  JSON.stringify(copied.map((o) => ({ [o.name]: o.body }))));
const own = ours.filter((o) => FULLSCREEN.includes(o.name));
check("the fullscreen pair is the client's own, never wry's refusal", own.length === 2 && own.every((o) => !/\binner\b/.test(o.body)),
  JSON.stringify(own.map((o) => o.name)));

const main = existsSync(MAIN) ? readFileSync(MAIN, 'utf8') : '';
const hook = /override fun onWebViewCreate\(webView: android\.webkit\.WebView\) \{\s*super\.onWebViewCreate\(webView\)([\s\S]*?)\n  \}\n/.exec(main)?.[1] ?? '';
check("MainActivity installs it once wry's client is set (posted from onWebViewCreate)", /webView\.post \{ FullscreenChromeClient\.install\(this, webView\) \}/.test(hook), hook.slice(0, 300));

const passed = results.filter(Boolean).length;
console.log(`${passed}/${results.length} passed`);
process.exitCode = passed === results.length ? 0 : 1;
