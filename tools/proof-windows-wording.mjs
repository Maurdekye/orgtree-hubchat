// Windows-specific wording & Hubchat Web hides (docket orgtree-hubchat-windows-specific-wording-on-non-w).
// Real SettingsModal, Onboarding and orgtreeRefs mounted in an isolated JSDOM with a stubbed api.
// Run inside joblimit.ps1 with HUBCHAT_DEPENDENCY_ROOT, HUBCHAT_DOM_DEPENDENCY_ROOT, HUBCHAT_RENDERER_OUT.
for (const k of ["HUBCHAT_DEPENDENCY_ROOT", "HUBCHAT_DOM_DEPENDENCY_ROOT", "HUBCHAT_RENDERER_OUT"]) {
  if (!process.env[k]) throw Error("Missing required env var " + k);
}
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const repo = process.cwd();
const deps = createRequire(path.join(process.env.HUBCHAT_DEPENDENCY_ROOT, "package.json"));
const orgdeps = createRequire(path.join(process.env.HUBCHAT_DOM_DEPENDENCY_ROOT, "package.json"));
const esbuild = orgdeps("esbuild");
const out = process.env.HUBCHAT_RENDERER_OUT;
fs.mkdirSync(out, { recursive: true });

await esbuild.build({
  entryPoints: [path.join(repo, "tools/windows-wording-entry.tsx")],
  outfile: path.join(out, "entry.mjs"),
  bundle: true,
  format: "esm",
  platform: "browser",
  loader: { ".md": "text", ".png": "dataurl", ".jpg": "dataurl", ".css": "empty" },
  jsx: "automatic",
  plugins: [
    {
      name: "raw-query",
      setup(b) {
        b.onResolve({ filter: /\?raw$/ }, (args) => ({
          path: path.resolve(args.resolveDir, args.path.replace(/\?raw$/, "")),
          namespace: "raw-file",
        }));
        b.onLoad({ filter: /.*/, namespace: "raw-file" }, (args) => ({
          contents: fs.readFileSync(args.path, "utf8"),
          loader: "text",
        }));
      },
    },
    {
      name: "md-spy",
      setup(b) {
        const mdPath = path.resolve(repo, "src/lib/md.ts");
        b.onLoad({ filter: /[\\/]src[\\/]lib[\\/]md\.ts$/ }, (args) => {
          if (path.resolve(args.path) !== mdPath) return null;
          const src = fs.readFileSync(args.path, "utf8");
          const patched = src.replace(
            "export function md(",
            "export function md(...args) { globalThis.__mdCalls = (globalThis.__mdCalls || 0) + 1; return _realMd(...args); }\nfunction _realMd(",
          );
          if (patched === src) throw new Error("md-spy could not patch export function md(");
          return { contents: patched, loader: "ts" };
        });
      },
    },
    {
      name: "existing-dependencies",
      setup(b) {
        b.onResolve({ filter: /^[^./][^:]*/ }, (a) => ({
          path: deps.resolve(a.path === "qrcode" ? "qrcode/lib/browser.js" : a.path),
        }));
      },
    },
  ],
});
esbuild.stop();

const { JSDOM } = orgdeps("jsdom");
const dom = new JSDOM('<div id="root"></div>', { url: "http://scratch.invalid" });
for (const key of ["window", "document", "navigator", "history", "HTMLElement", "HTMLTextAreaElement", "HTMLInputElement", "localStorage", "location", "getComputedStyle", "Event", "MouseEvent", "KeyboardEvent", "MutationObserver"]) {
  Object.defineProperty(globalThis, key, {
    value: key === "getComputedStyle" ? dom.window[key].bind(dom.window) : dom.window[key],
    configurable: true,
  });
}
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
globalThis.requestAnimationFrame = (f) => setTimeout(f, 0);
globalThis.cancelAnimationFrame = clearTimeout;
dom.window.HTMLElement.prototype.scrollIntoView = function () {};

const checks = [];
const check = (name, ok, detail = "") => {
  ok = !!ok;
  checks.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"} ${name}${!ok && detail ? " :: " + detail : ""}`);
};
const wait = (ms = 20) => new Promise((r) => setTimeout(r, ms));

async function mountEnv(tag, { platform = "desktop", shell = "tauri", os = "windows", device_name = "" }) {
  const m = await import(pathToFileURL(path.join(out, "entry.mjs")).href + "?" + tag);
  const linkStarts = [];
  Object.assign(m.api, {
    state: async () => ({
      me: { id: "alex", address: "alex.123456", name: "Alex", about: "", picture: null },
      recovery_saved: true,
      read_receipts: true,
      status: "online",
      language: "",
      notifications: { enabled: true, preview: true, sound: true },
      stay_connected: null,
      push: null,
      file_background_hint: null,
      device_name,
      device_maker: "",
      hubs: [],
      platform,
      shell,
      os,
      key_lost: null,
      key_restored: false,
    }),
    chats: async () => [],
    directory: async () => [],
    transfers: async () => ({ rows: [], next: null }),
    onEvent: async () => () => {},
    onSettingsChanged: async () => () => {},
    onLink: async () => () => {},
    onPendingLink: async () => () => {},
    takePendingLink: async () => null,
    exitOnClose: async () => false,
    setExitOnClose: async (v) => v,
    devices: async () => ({ this_device: "d1", devices: [] }),
    logInfo: async () => ({ active: false, verbose: false, pinned: false, files: 0, bytes: 0, max_bytes: 1000, retention_days: 7, fallback: false }),
    checkId: async () => ({ ok: true, error: null, max_len: 24 }),
    probeHub: async (url) => ({ result: "connected", url, name: "Scratch Hub", max_attachment_bytes: 1000000, version: "2.4.0", features: [] }),
    linkStart: async (hub, name, code) => {
      linkStarts.push({ hub, name, code });
      return { code: "ABCD-EFGH-IJKL-MNOP", qr: "hubchat://link?code=ABCD" };
    },
    linkCancel: async () => {},
  });
  await m.act(async () => { await m.store.startStore(); });
  const host = document.createElement("div");
  document.body.appendChild(host);
  const root = m.createRoot(host);
  const web = shell === "web";
  const render = async (node) => {
    await m.act(async () => {
      root.render(
        m.createElement(m.WebCtx.Provider, { value: web },
          m.createElement(m.PlatformCtx.Provider, { value: platform },
            m.createElement(m.OsCtx.Provider, { value: os ?? m.detectOs(platform) }, node))),
      );
    });
    await m.act(async () => { await wait(30); });
  };
  const done = async () => {
    await m.act(async () => { root.unmount(); });
    host.remove();
  };
  return { m, host, render, done, linkStarts };
}

// ---- H-01: General tab Start with Windows vs Start at login
{
  const win = await mountEnv("h01-win", { platform: "desktop", shell: "tauri", os: "windows" });
  await win.render(win.m.createElement(win.m.SettingsModal, { tab: "general", setTab: () => {}, onClose: () => {} }));
  const txt = win.host.textContent || "";
  check("H-01a Windows keeps Start with Windows byte-identical",
    txt.includes("Start with Windows")
      && txt.includes("Hubchat starts hidden in the tray when you sign in to Windows, so messages arrive before you open it."),
    txt.slice(0, 300));
  await win.done();
}
{
  const lin = await mountEnv("h01-linux", { platform: "desktop", shell: "tauri", os: "linux" });
  await lin.render(lin.m.createElement(lin.m.SettingsModal, { tab: "general", setTab: () => {}, onClose: () => {} }));
  const txt = lin.host.textContent || "";
  check("H-01b Linux shows Start at login and omits Windows",
    txt.includes("Start at login")
      && txt.includes("Hubchat starts hidden in the tray when you sign in, so messages arrive before you open it.")
      && !txt.includes("Start with Windows")
      && !txt.includes("sign in to Windows"),
    txt.slice(0, 300));
  await lin.done();
}
{
  const mac = await mountEnv("h01-macos", { platform: "desktop", shell: "tauri", os: "macos" });
  await mac.render(mac.m.createElement(mac.m.SettingsModal, { tab: "general", setTab: () => {}, onClose: () => {} }));
  const txt = mac.host.textContent || "";
  check("H-01c macOS shows Start at login and omits Windows",
    txt.includes("Start at login") && !txt.includes("Windows"),
    txt.slice(0, 300));
  await mac.done();
}

// ---- H-03: Hubchat Web hides desktop-only controls in General & hides Developer tab
{
  const web = await mountEnv("h03-web", { platform: "desktop", shell: "web", os: "linux" });
  await web.render(web.m.createElement(web.m.SettingsModal, { tab: "general", setTab: () => {}, onClose: () => {} }));
  const tabs = [...web.host.querySelectorAll('[role="tab"]')].map((e) => e.textContent?.trim());
  const body = web.host.querySelector("#set-panel")?.textContent || "";
  check("H-03a Hubchat Web General hides Start with Windows / Start at login and Exit on close, keeps Enter key",
    !body.includes("Start with Windows") && !body.includes("Start at login") && !body.includes("Exit on close") && body.includes("Enter key"),
    body);
  check("H-03b Hubchat Web hides Developer tab",
    !tabs.includes("Developer") && tabs.includes("General"),
    JSON.stringify(tabs));
  await web.render(web.m.createElement(web.m.SettingsModal, { tab: "developer", setTab: () => {}, onClose: () => {} }));
  const fallbackBody = web.host.querySelector("#set-panel")?.textContent || "";
  check("H-03c Hubchat Web safely falls back when tab='developer' is passed",
    !fallbackBody.includes("Verbose logging") && !fallbackBody.includes("Open logs folder"),
    fallbackBody.slice(0, 200));
  await web.done();
}
{
  const nat = await mountEnv("h03-native", { platform: "desktop", shell: "tauri", os: "windows" });
  await nat.render(nat.m.createElement(nat.m.SettingsModal, { tab: "general", setTab: () => {}, onClose: () => {} }));
  const tabs = [...nat.host.querySelectorAll('[role="tab"]')].map((e) => e.textContent?.trim());
  check("H-03d Native desktop keeps Developer tab", tabs.includes("Developer"), JSON.stringify(tabs));
  await nat.done();
}

// ---- B-01: device word ("PC" on Windows, "device" on Linux/macOS) across Settings, Onboarding, useDeviceWord, orgtreeRefs
{
  const win = await mountEnv("b01-win", { platform: "desktop", shell: "tauri", os: "windows" });
  await win.render(win.m.createElement("div", null,
    win.m.createElement(win.m.DeviceWordProbe),
    win.m.createElement(win.m.SettingsModal, { tab: "devices", setTab: () => {}, onClose: () => {} })));
  const word = win.host.querySelector("#device-word")?.textContent;
  const txt = win.host.textContent || "";
  check("B-01a Windows keeps PC wording byte-identical",
    word === "PC" && txt.includes("This PC") && txt.includes("This PC asks before it switches.")
      && txt.includes("Bring your identity to a new phone or PC:"),
    `word=${word}`);
  await win.done();
}
{
  const lin = await mountEnv("b01-linux", { platform: "desktop", shell: "tauri", os: "linux" });
  await lin.render(lin.m.createElement("div", null,
    lin.m.createElement(lin.m.DeviceWordProbe),
    lin.m.createElement(lin.m.SettingsModal, { tab: "devices", setTab: () => {}, onClose: () => {} })));
  const word = lin.host.querySelector("#device-word")?.textContent;
  const txt = lin.host.textContent || "";
  check("B-01b Linux uses 'device' instead of 'PC' in Devices settings and useDeviceWord",
    word === "device" && txt.includes("This device") && txt.includes("This device asks before it switches.")
      && txt.includes("Bring your identity to a new phone or device:") && !/\bPC\b/.test(txt),
    `word=${word} text=${txt.slice(0, 300)}`);
  await lin.done();
}
{
  const linOb = await mountEnv("b01-ob-linux", { platform: "desktop", shell: "tauri", os: "linux" });
  await linOb.render(linOb.m.createElement(linOb.m.Onboarding, { start: "method" }));
  const txt = linOb.host.textContent || "";
  check("B-01c Linux Onboarding uses 'device' instead of 'PC'",
    txt.includes("Bring your identity to this device") && txt.includes("approve this device there") && !/\bPC\b/.test(txt),
    txt.slice(0, 300));
  await linOb.done();
}
{
  const m = await import(pathToFileURL(path.join(out, "entry.mjs")).href + "?b01-oref");
  const ref = m.parseOref("@doc:beta/notes");
  const ctxWin = m.orefContext({ peer: "alpha.1", peerKind: "org", peerName: "Alpha", outgoing: false, desktop: true, links: true, orgs: [], windows: true });
  const ctxNix = m.orefContext({ peer: "alpha.1", peerKind: "org", peerName: "Alpha", outgoing: false, desktop: true, links: true, orgs: [], windows: false });
  const cWin = m.tokenChip(ref, ctxWin);
  const cNix = m.tokenChip(ref, ctxNix);
  check("B-01d orgtreeRefs keeps PC on Windows and uses device off Windows",
    cWin.title === "Orgtree on this PC didn't list its orgs" && cNix.title === "Orgtree on this device didn't list its orgs",
    `win=${cWin.title} nix=${cNix.title}`);
  const ctxWinOther = m.orefContext({ peer: "alpha.1", peerKind: "org", peerName: "Alpha", outgoing: false, desktop: true, links: false, orgs: [], windows: true });
  const ctxNixOther = m.orefContext({ peer: "alpha.1", peerKind: "org", peerName: "Alpha", outgoing: false, desktop: true, links: false, orgs: [], windows: false });
  check("B-01e orgtreeRefs notHere keeps PC on Windows and uses device off Windows",
    m.tokenChip(ref, ctxWinOther).title === "Opens in Orgtree on the PC that runs beta"
      && m.tokenChip(ref, ctxNixOther).title === "Opens in Orgtree on the device that runs beta",
    `win=${m.tokenChip(ref, ctxWinOther).title} nix=${m.tokenChip(ref, ctxNixOther).title}`);
}

// ---- H-02: Default device name when device_name is empty (through Onboarding Link through a hub -> api.linkStart)
async function probeDefaultName(tag, envOpts) {
  const env = await mountEnv(tag, { ...envOpts, device_name: "" });
  await env.render(env.m.createElement(env.m.Onboarding, { start: "method" }));
  const linkBtn = [...env.host.querySelectorAll("button")].find((b) => b.textContent?.includes("Link through a hub"));
  await env.m.act(async () => { linkBtn?.click(); await wait(30); });
  const input = env.host.querySelector('input[type="text"], input');
  if (input) {
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
    await env.m.act(async () => {
      setter.call(input, "http://hub.local:7370");
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.dispatchEvent(new Event("change", { bubbles: true }));
      await wait(20);
    });
    const checkBtn = [...env.host.querySelectorAll("button")].find((b) => /Check/i.test(b.textContent || "") && !b.disabled);
    await env.m.act(async () => { checkBtn?.click(); await wait(50); });
    const useBtn = [...env.host.querySelectorAll("button")].find((b) => /Use Scratch Hub/i.test(b.textContent || "") && !b.disabled);
    await env.m.act(async () => { useBtn?.click(); await wait(60); });
  }
  const got = env.linkStarts[0]?.name ?? null;
  await env.done();
  return got;
}
{
  const winName = await probeDefaultName("h02-win", { platform: "desktop", shell: "tauri", os: "windows" });
  const linName = await probeDefaultName("h02-lin", { platform: "desktop", shell: "tauri", os: "linux" });
  const macName = await probeDefaultName("h02-mac", { platform: "desktop", shell: "tauri", os: "macos" });
  const webName = await probeDefaultName("h02-web", { platform: "desktop", shell: "web", os: "linux" });
  check("H-02 default device name matches platform (Windows PC / Linux device / Mac / Web browser)",
    winName === "Windows PC" && linName === "Linux device" && macName === "Mac" && webName === "Web browser",
    JSON.stringify({ winName, linName, macName, webName }));
}

// ---- Review Item 2: MessageView draft-typing regression (32 rows, 4 draft edits -> 0 extra md() renders)
{
  const env = await mountEnv("r02-msg-rerender", { platform: "desktop", shell: "tauri", os: "windows" });
  globalThis.__mdCalls = 0;
  const noop = () => {};
  const rows = Array.from({ length: 32 }, (_, i) => env.m.createElement(env.m.MessageView, {
    key: i,
    m: {
      id: "fixture-" + i,
      peer: "fixture",
      body: "Message body " + i,
      attachments: [],
      outgoing: false,
      ts: 1700000000000 + i * 1000,
      status: "read",
    },
    first: i === 0,
    peerName: "Contact",
    peerKind: "person",
    quoted: null,
    quotes: [],
    card: null,
    picks: [],
    notes: [],
    hover: false,
    selected: false,
    highlighted: false,
    onReply: noop,
    onSelect: noop,
    onPick: noop,
    onInfo: noop,
    onDelete: noop,
    onJump: noop,
    onOpenAddr: noop,
    onContactInfo: noop,
  }));
  await env.render(env.m.createElement("div", null, rows));
  const initialCalls = globalThis.__mdCalls;
  for (const draft of ["a", "ab", "abc", "abcd"]) {
    await env.m.act(async () => { env.m.store.noteDraft("fixture", draft); });
  }
  const extraCalls = globalThis.__mdCalls - initialCalls;
  check("R-02 MessageView 32 rows trigger 0 extra Markdown renders across 4 draft edits",
    initialCalls >= 32 && extraCalls === 0,
    `initial=${initialCalls} extra=${extraCalls}`);
  await env.done();
}

// ---- Review Item 7a: Compact Web Devices uses "This browser" (never "This phone") while keeping compact layout
{
  const cw = await mountEnv("r07-compact-web-devices", { platform: "android", shell: "web", os: "linux", device_name: "" });
  await cw.render(cw.m.createElement(cw.m.SettingsSection, { tab: "devices" }));
  const txt = cw.host.textContent || "";
  check("R-07a Compact Web Devices uses This browser (never This phone)",
    txt.includes("This browser")
      && txt.includes("This browser asks before it switches.")
      && !txt.includes("This phone"),
    txt.slice(0, 350));
  await cw.done();
}

// ---- Review Item 7b: SetupFlow uses device-word noun ("this phone" only on phones, "this device" on Linux desktop)
{
  const setupProbe = async (tag, envOpts, net) => {
    const env = await mountEnv(tag, envOpts);
    Object.assign(env.m.api, {
      parseSetup: async () => ({
        link: { org: "acme.1", orgname: "Acme", pc: "HostBox", hub: "http://host:7371", hubname: "HostHub", net, ts: "u@example.test" },
        error: null,
      }),
      appInstalled: async () => true,
      setupCheck: async () => ({ reachable: false, error: "unreachable" }),
    });
    await env.render(env.m.createElement(env.m.SetupFlow, { req: { n: 1, input: "hubchat://setup?x=1" } }));
    await env.m.act(async () => { await wait(50); });
    const txt = env.host.textContent || "";
    await env.done();
    return txt;
  };
  const linWifi = await setupProbe("r07-setup-lin-wifi", { platform: "desktop", shell: "tauri", os: "linux" }, "wifi");
  const linTs = await setupProbe("r07-setup-lin-ts", { platform: "desktop", shell: "tauri", os: "linux" }, "tailscale");
  const andWifi = await setupProbe("r07-setup-and-wifi", { platform: "android", shell: "tauri", os: "android" }, "wifi");
  check("R-07b SetupFlow uses 'this device' on Linux desktop and keeps 'this phone' on Android",
    linWifi.includes("Is this device on the same Wi-Fi as your other device?")
      && !linWifi.includes("this phone")
      && linTs.includes("Is Tailscale on and signed in on this device?")
      && !linTs.includes("this phone")
      && andWifi.includes("Is this phone on the same Wi-Fi as your other device?"),
    JSON.stringify({ linWifi: linWifi.slice(0, 200), linTs: linTs.slice(0, 200), andWifi: andWifi.slice(0, 200) }));
}

const passed = checks.filter((c) => c.ok).length;
const failed = checks.filter((c) => !c.ok).length;
console.log(`SUMMARY pass=${passed} fail=${failed} total=${checks.length}`);
dom.window.close();
process.exit(failed === 0 ? 0 : 1);
