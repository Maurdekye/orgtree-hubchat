// Headless real-module tests for Linux store-mismatch UI flow and lost-key routing:
// - Hubless recovery regression: restoring an identity with zero hubs (both Linux
//   mismatch and ordinary lost-key) keeps <Onboarding> mounted and routes to
//   "Add your hubs", never remounting to "Create a new identity" (as at f954531).
// - Use B here invokes useFoundIdentity(B) without altering onboarding (f954531).
// - Setup invitations held while in mismatch state (with notice, no overlay),
//   released after mismatch resolves via Restore A; outside mismatch they open
//   immediately.
// - Recovery/link/key-file confirm dialog in mismatch state names A only
//   ("Restore <A>"), and native refusal shows the error and stays on the
//   mismatch screen.
import test from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { existsSync, readFileSync } from "node:fs";
import vm from "node:vm";
import path from "node:path";

const require = createRequire(path.resolve(process.env.HUBCHAT_DEPENDENCY_ROOT || ".", "package.json"));
const ts = require("typescript");

function loadTs(relPath, mocks, extras = {}) {
  const full = path.resolve(relPath);
  const src = readFileSync(full, "utf8");
  const isTsx = relPath.endsWith(".tsx");
  const js = ts.transpileModule(src, {
    fileName: full,
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
      jsx: isTsx ? ts.JsxEmit.React : undefined,
    },
  }).outputText;
  const mod = { exports: {} };
  const ctx = {
    require: (id) => {
      if (id in mocks) return mocks[id];
      const base = path.dirname(full);
      for (const ext of ["", ".ts", ".tsx", ".js"]) {
        const p = path.resolve(base, id + ext);
        if (existsSync(p)) {
          return loadTs(path.relative(process.cwd(), p), mocks, extras);
        }
      }
      throw new Error(`unexpected dependency in ${relPath}: ${id}`);
    },
    module: mod,
    exports: mod.exports,
    console,
    Error,
    Promise,
    Set,
    Map,
    Date,
    Math,
    Object,
    Array,
    RegExp,
    JSON,
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    performance: { now: () => 0 },
    document: {
      visibilityState: "visible",
      hasFocus: () => true,
      addEventListener: () => {},
      removeEventListener: () => {},
    },
    window: {
      addEventListener: () => {},
      removeEventListener: () => {},
    },
    navigator: { userAgent: "Mozilla/5.0 (X11; Linux aarch64)" },
    ...extras,
  };
  vm.runInNewContext(js, ctx);
  return mod.exports;
}

/** Synchronous keyed React reconciler & hook harness that models React's real
 *  key-based mount/unmount lifecycle so changing `key` on a child component
 *  resets that component's hooks (catching remount bugs like lostPick -> null). */
function makeReactHarness() {
  const Fragment = Symbol.for("react.fragment");
  let activeFiber = null;
  const pendingEffects = [];

  function depsChanged(prev, next) {
    if (!prev || !next) return true;
    if (prev.length !== next.length) return true;
    for (let i = 0; i < prev.length; i++) {
      if (!Object.is(prev[i], next[i])) return true;
    }
    return false;
  }

  const React = {
    Fragment,
    createElement(type, props, ...children) {
      const p = props ? { ...props } : {};
      const key = p.key !== undefined ? p.key : null;
      delete p.key;
      const flat = [];
      const push = (c) => {
        if (Array.isArray(c)) {
          for (const x of c) push(x);
        } else if (c !== null && c !== undefined && c !== false && c !== true) {
          flat.push(c);
        }
      };
      push(children);
      p.children = flat.length === 1 ? flat[0] : flat;
      return { type, key, props: p };
    },
    useState(init) {
      const f = activeFiber;
      const i = f.hookIdx++;
      if (!(i in f.states)) f.states[i] = typeof init === "function" ? init() : init;
      return [
        f.states[i],
        (v) => {
          f.states[i] = typeof v === "function" ? v(f.states[i]) : v;
        },
      ];
    },
    useRef(init) {
      const f = activeFiber;
      const i = f.hookIdx++;
      if (!(i in f.refs)) f.refs[i] = { current: init };
      return f.refs[i];
    },
    useEffect(fn, deps) {
      const f = activeFiber;
      const i = f.effectIdx++;
      const prev = f.effects[i];
      if (!prev || depsChanged(prev.deps, deps)) {
        pendingEffects.push({ fiber: f, idx: i, fn, deps });
      }
    },
    useCallback(fn) {
      return fn;
    },
    useMemo(fn) {
      return fn();
    },
    useSyncExternalStore(_sub, getSnap) {
      return getSnap();
    },
    createContext(def) {
      return {
        _default: def,
        Provider: ({ children }) => children,
      };
    },
    useContext(ctx) {
      return ctx._default;
    },
  };

  function makeFiber(type, key) {
    return {
      type,
      key,
      states: [],
      refs: [],
      effects: [],
      children: new Map(),
      hookIdx: 0,
      effectIdx: 0,
    };
  }

  function cleanupFiber(fiber) {
    for (const ch of fiber.children.values()) cleanupFiber(ch);
    for (const ef of fiber.effects) {
      if (ef && typeof ef.cleanup === "function") ef.cleanup();
    }
    fiber.children.clear();
  }

  function renderTree(rootFiber, element, expand = () => true) {
    function visit(node, parentFiber, slotKey) {
      if (node === null || node === undefined || typeof node !== "object") return node;
      if (Array.isArray(node)) {
        return node.map((child, idx) => visit(child, parentFiber, `${slotKey}[${idx}]`));
      }
      if (node.type === Fragment) {
        return {
          ...node,
          props: {
            ...node.props,
            children: visit(node.props?.children, parentFiber, `${slotKey}:frag`),
          },
        };
      }
      if (typeof node.type === "function") {
        if (!expand(node.type)) return node;
        const childId = `${slotKey}:${node.type.name || "anon"}:${node.key ?? ""}`;
        let fiber = parentFiber.children.get(childId);
        if (!fiber || fiber.type !== node.type || fiber.key !== (node.key ?? null)) {
          if (fiber) cleanupFiber(fiber);
          fiber = makeFiber(node.type, node.key ?? null);
          parentFiber.children.set(childId, fiber);
        }
        parentFiber._seen.add(childId);
        fiber.hookIdx = 0;
        fiber.effectIdx = 0;
        fiber._seen = new Set();
        const prev = activeFiber;
        activeFiber = fiber;
        let rendered;
        try {
          rendered = node.type(node.props || {});
        } finally {
          activeFiber = prev;
        }
        const resolved = visit(rendered, fiber, "0");
        for (const [k, oldChild] of fiber.children.entries()) {
          if (!fiber._seen.has(k)) {
            cleanupFiber(oldChild);
            fiber.children.delete(k);
          }
        }
        return { type: node.type, key: node.key, props: node.props, rendered: resolved };
      }
      return {
        ...node,
        props: {
          ...node.props,
          children: visit(node.props?.children, parentFiber, `${slotKey}:${String(node.type)}`),
        },
      };
    }

    rootFiber._seen = new Set();
    const out = visit(element, rootFiber, "root");
    for (const [k, oldChild] of rootFiber.children.entries()) {
      if (!rootFiber._seen.has(k)) {
        cleanupFiber(oldChild);
        rootFiber.children.delete(k);
      }
    }
    return out;
  }

  function flushEffects() {
    let ran = 0;
    while (pendingEffects.length) {
      const item = pendingEffects.shift();
      const prev = item.fiber.effects[item.idx];
      if (prev && typeof prev.cleanup === "function") prev.cleanup();
      const cleanup = item.fn();
      item.fiber.effects[item.idx] = { deps: item.deps, cleanup };
      ran++;
    }
    return ran;
  }

  /** Render + flush effects until hooks/effects settle (up to 8 passes). */
  function renderSettled(rootFiber, makeEl, expand) {
    let tree = renderTree(rootFiber, makeEl(), expand);
    for (let i = 0; i < 8; i++) {
      const ran = flushEffects();
      tree = renderTree(rootFiber, makeEl(), expand);
      if (!ran && pendingEffects.length === 0) break;
    }
    return tree;
  }

  function findAll(node, pred, out = []) {
    if (!node || typeof node !== "object") return out;
    if (Array.isArray(node)) {
      for (const x of node) findAll(x, pred, out);
      return out;
    }
    if (pred(node)) out.push(node);
    if ("rendered" in node) {
      findAll(node.rendered, pred, out);
      return out;
    }
    const ch = node.props?.children;
    if (ch) findAll(ch, pred, out);
    return out;
  }

  function textOf(node) {
    if (node === null || node === undefined || typeof node === "boolean") return "";
    if (typeof node === "string" || typeof node === "number") return String(node);
    if (Array.isArray(node)) return node.map(textOf).join(" ");
    if (typeof node !== "object") return "";
    if ("rendered" in node) return textOf(node.rendered);
    return textOf(node.props?.children);
  }

  /** Simple single-component render helper (used by focused component tests). */
  const simpleRoot = makeFiber("simple", null);
  const SimpleWrap = (p) => p.fn();
  function render(fn) {
    return renderTree(simpleRoot, React.createElement(SimpleWrap, { fn }), (t) => t === SimpleWrap).rendered;
  }

  return { React, makeFiber, renderTree, renderSettled, flushEffects, render, findAll, textOf };
}

function makeState(overrides = {}) {
  return {
    me: null,
    key_lost: { address: "alex.111111", unreadable: false, found: "maya.222222" },
    key_restored: false,
    key_storage: "file",
    recovery_saved: false,
    read_receipts: true,
    status: "auto",
    notify: { enabled: true, preview: true, sound: false },
    device_name: "Steam Frame",
    platform: "desktop",
    shell: "desktop",
    version: "1.3.12",
    hubs: [],
    ...overrides,
  };
}

function loadAppStack(initialState) {
  let curState = initialState;
  const toasts = [];
  const usedFound = [];
  const api = {
    state: async () => curState,
    chats: async () => [],
    directory: async () => [],
    transfers: async () => ({ rows: [], next: null }),
    onEvent: async () => () => {},
    retryKey: async () => {},
    startOver: async () => {},
    useFoundIdentity: async (expected) => {
      usedFound.push(expected);
      curState = makeState({
        me: { id: "maya", address: expected, name: "", about: "" },
        key_lost: null,
      });
    },
    checkId: async () => ({ ok: true, error: null, max_len: 24 }),
    restoreWords: async () => {
      curState = makeState({
        me: { id: "alex", address: "alex.111111", name: "Alex", about: "" },
        key_lost: null,
        hubs: [],
      });
      return "alex.111111";
    },
    keyFileImport: async () => {
      throw new Error("not stubbed");
    },
    restoreQr: async () => {
      throw new Error("not stubbed");
    },
    linkConfirm: async () => {
      throw new Error("not stubbed");
    },
  };
  const h = makeReactHarness();
  const baseMocks = {
    react: h.React,
    "../api": { api, isWeb: false },
    "./api": { api, isWeb: false },
    "./answers": { forgetDraftAnswers: () => {} },
    "./native": {
      errText: (e) => (e instanceof Error ? e.message : String(e)),
      openLink: async () => {},
      scanQr: async () => null,
      baseName: (p) => p,
      pickKeyFile: async () => null,
      serverName: async () => "",
      suggestedHubs: async () => ({ listed: false, addresses: [] }),
    },
    "./toast": { toast: (m) => toasts.push(m) },
    "./updates": { checkForUpdate: async () => null, installUpdate: async () => {} },
    "./visibility": { startTake: () => {}, usePendingLink: () => {} },
  };
  const store = loadTs("src/lib/store.ts", baseMocks);
  const setup = loadTs("src/lib/setup.ts", { ...baseMocks, "./store": store });

  const Confirm = function Confirm(p) { return p; };
  const Desktop = function Desktop() { return null; };
  const Android = function Android() { return null; };
  const SetupFlow = function SetupFlow(p) { return p; };
  const Toasts = function Toasts() { return null; };
  const CtxMenu = function CtxMenu() { return null; };
  const TitleBar = function TitleBar() { return null; };
  const PlatformCtx = h.React.createContext("desktop");
  const WebCtx = h.React.createContext(false);
  const OsCtx = h.React.createContext("linux");
  const detectOs = (p) => (p === "android" ? "android" : "linux");

  const onboardingMod = loadTs("src/components/Onboarding.tsx", {
    ...baseMocks,
    "../lib/store": store,
    "../lib/setup": setup,
    "../lib/toast": { toast: (m) => toasts.push(m) },
    "../lib/native": baseMocks["./native"],
    "../lib/visibility": baseMocks["./visibility"],
    "../lib/recovery": { copyWords: async () => {}, downloadWords: async () => {} },
    "../lib/recoveryFile": { RECOVERY_FILE_ERROR: "err", RECOVERY_FILE_LIMIT: 4096, recoveryFileWords: (t) => t },
    "../lib/icons": { Icon: () => null, Logo: () => null },
    "../lib/peers": { splitAddr: (a) => [a, ""] },
    "../lib/chatlink": { chatLinkTarget: () => null },
    "./HubAdder": { HubAdder: () => null },
    "./HubReview": { HubReview: () => null, useHubReview: () => ({ urls: [], blocked: false }) },
    "./LinkDevice": { formatCode: (s) => s },
    "./Guide": { openGuide: () => {} },
    "./ui": {
      Confirm,
      NoteCard: () => null,
      QR: () => null,
      defaultDeviceName: () => "Linux device",
      useDeviceWord: () => "device",
      useIsWindowsDesktop: () => false,
      useNow: () => 0,
      useOs: () => "linux",
      usePlatform: () => "desktop",
      useWeb: () => false,
    },
    "../assets/desktop-footer-qr.png": "",
    "../assets/desktop-footer-qr.json": { x: 0, y: 0, w: 1, h: 1 },
  }, { React: h.React });

  const appMod = loadTs("src/App.tsx", {
    ...baseMocks,
    "./components/Android": { Android },
    "./components/CtxMenu": { CtxMenu },
    "./components/Block": { BlockConfirm: () => null },
    "./components/Desktop": { Desktop },
    "./components/Onboarding": onboardingMod,
    "./components/SetupFlow": { SetupFlow },
    "./components/ui": { OsCtx, PlatformCtx, Toasts, WebCtx, detectOs },
    "./components/TitleBar": { TitleBar },
    "./lib/ctxmenu": { onContextMenu: () => {} },
    "./lib/icons": { Logo: () => null },
    "./lib/setup": setup,
    "./lib/store": store,
    "./lib/toast": { toast: (m) => toasts.push(m) },
    "./lib/shortcutIcons": { useShortcutTaskIcons: () => {} },
  }, { React: h.React });

  return {
    h,
    api,
    store,
    setup,
    toasts,
    usedFound,
    Confirm,
    Desktop,
    SetupFlow,
    onboardingMod,
    App: appMod.default,
    setState(s) {
      curState = s;
    },
  };
}

for (const [label, keyLost] of [
  ["Linux mismatch", { address: "alex.111111", unreadable: false, found: "maya.222222" }],
  ["ordinary lost-key", { address: "alex.111111", unreadable: false, found: null }],
]) {
  test(`hubless recovery (${label}) routes to 'Add your hubs' and never remounts to 'Create a new identity'`, async () => {
    const env = loadAppStack(makeState({ key_lost: keyLost, hubs: [] }));
    const root = env.h.makeFiber("root", null);
    const expand = (fn) =>
      fn === env.App
      || fn === env.onboardingMod.LostKey
      || fn === env.onboardingMod.Onboarding
      || fn.name === "Frame"
      || fn.name === "Opt"
      || fn.name === "Provider";

    // Initial render starts store; await store hydration then settle effects.
    env.h.renderSettled(root, () => env.h.React.createElement(env.App, null), expand);
    await new Promise((r) => setTimeout(r, 10));
    const tLost = env.h.renderSettled(root, () => env.h.React.createElement(env.App, null), expand);
    assert.ok(
      env.h.findAll(tLost, (n) => n.type === env.onboardingMod.LostKey).length === 1,
      "App must render LostKey initially",
    );

    // Click "Recovery words" on LostKey -> App sets lostPick="restore" -> mounts <Onboarding key="restore">.
    const restoreOpt = env.h.findAll(tLost, (n) => n.props?.t === "Recovery words")[0];
    assert.ok(restoreOpt, "LostKey must offer 'Recovery words'");
    restoreOpt.props.onClick();

    const tRestore = env.h.renderSettled(root, () => env.h.React.createElement(env.App, null), expand);
    const ta = env.h.findAll(tRestore, (n) => n.type === "textarea" && n.props?.id === "ob-words")[0];
    assert.ok(ta, "Onboarding restore step must render recovery words textarea");
    ta.props.onChange({ target: { value: Array(24).fill("amber").join(" ") } });

    const tWords = env.h.renderSettled(root, () => env.h.React.createElement(env.App, null), expand);
    const frame = env.h.findAll(tWords, (n) => typeof n.type === "function" && n.type.name === "Frame")[0];
    const contAction = frame.props.actions.find((a) => a.label === "Continue");
    assert.ok(contAction, "Continue action must exist on restore step");
    contAction.onClick();

    // In mismatch state, confirm dialog appears first; in ordinary lost-key, restoreWords runs directly.
    if (keyLost.found) {
      const tConfirm = env.h.renderSettled(root, () => env.h.React.createElement(env.App, null), expand);
      const confirmEl = env.h.findAll(tConfirm, (n) => n.type === env.Confirm)[0];
      assert.ok(confirmEl, "mismatch restore confirm dialog must appear");
      confirmEl.props.onOk();
    }

    await new Promise((r) => setTimeout(r, 15));
    const tAfter = env.h.renderSettled(root, () => env.h.React.createElement(env.App, null), expand);

    // State now has restored identity A with zero hubs and onboarding still true.
    assert.equal(env.store.getSnap().state.me?.address, "alex.111111");
    assert.equal(env.store.getSnap().state.hubs.length, 0);
    assert.equal(env.store.getSnap().onboarding, true);

    const renderedText = env.h.textOf(tAfter);
    assert.ok(
      renderedText.includes("Add your hubs"),
      `expected 'Add your hubs' after hubless recovery, got: ${renderedText}`,
    );
    assert.ok(
      !renderedText.includes("Create a new identity"),
      `must never remount to 'Create a new identity' after hubless recovery, got: ${renderedText}`,
    );
  });
}

test("Use B here invokes useFoundIdentity(B) and leaves onboarding unchanged (f954531 behaviour)", async () => {
  const env = loadAppStack(makeState());
  await env.store.startStore();
  assert.equal(env.store.getSnap().onboarding, true);

  const tree1 = env.h.render(() => env.onboardingMod.LostKey({ lost: env.store.getSnap().state.key_lost, onPick: () => {} }));
  const useBtn = env.h.findAll(tree1, (n) => n.props?.t === "Use maya.222222 here")[0];
  assert.ok(useBtn, "Use <found> here option must render on LostKey");
  useBtn.props.onClick();

  const tree2 = env.h.render(() => env.onboardingMod.LostKey({ lost: env.store.getSnap().state.key_lost, onPick: () => {} }));
  const confirmEl = env.h.findAll(tree2, (n) => n.type === env.Confirm)[0];
  assert.ok(confirmEl, "Confirm dialog must open for Use B here");
  await confirmEl.props.onOk();
  await new Promise((r) => setTimeout(r, 10));

  assert.deepEqual(env.usedFound, ["maya.222222"]);
  const snap = env.store.getSnap();
  assert.equal(snap.state.me?.address, "maya.222222");
  assert.equal(snap.state.key_lost, null);
  assert.equal(snap.onboarding, true, "reverted M1: onboarding stays unchanged as at f954531");
});

test("setup invitation is held while in mismatch state, opens after Restore A completes onboarding, and opens immediately outside mismatch", async () => {
  const INVITE = "hubchat://setup?v=1&hub=http%3A%2F%2F127.0.0.1%3A7371&org=o.1&orgname=Org&pc=pc&code=ABCD-1234&net=wifi&hubname=pc";

  // Held during mismatch, released once Restore A finishes onboarding (with hub).
  {
    const env = loadAppStack(makeState());
    await env.store.startStore();
    env.setup.startSetup(INVITE);

    const activeDuringMismatch = env.h.render(() => env.setup.useSetup());
    assert.equal(activeDuringMismatch, null, "setup overlay must not open while in mismatch state");
    assert.ok(
      env.toasts.includes("Finish choosing an identity first"),
      "user must get notice instead of silent drop",
    );

    env.setState(makeState({
      me: { id: "alex", address: "alex.111111", name: "Alex", about: "" },
      key_lost: null,
      hubs: [{ url: "http://127.0.0.1:7371", name: "pc", state: "connected", features: [] }],
    }));
    env.store.setOnboarding(false);
    await env.store.refreshAll();
    env.setup.syncSetupGate(false, true);

    const activeAfterA = env.h.render(() => env.setup.useSetup());
    assert.equal(activeAfterA?.input, INVITE, "held setup invitation must open after Restore A resolves mismatch");
  }

  // Outside mismatch state: opens immediately with no mismatch toast.
  {
    const env = loadAppStack(makeState({ key_lost: null }));
    await env.store.startStore();
    env.setup.startSetup(INVITE);
    const activeNormal = env.h.render(() => env.setup.useSetup());
    assert.equal(activeNormal?.input, INVITE, "outside mismatch state setup invitation opens immediately");
    assert.equal(env.toasts.length, 0, "no mismatch toast outside mismatch state");
  }
});

test("mismatch restore confirm names A only ('Restore <A>') and native refusal returns to mismatch screen with error", async () => {
  const NATIVE_ERR = "This device's chats belong to alex.111111. Use alex.111111's recovery words, link or key file, or choose Use maya.222222 here.";
  const env = loadAppStack(makeState());
  env.api.restoreWords = async () => { throw new Error(NATIVE_ERR); };
  await env.store.startStore();

  const refused = [];
  const props = {
    start: "restore",
    home: () => {},
    onMismatchRefused: (msg) => refused.push(msg),
  };

  const t1 = env.h.render(() => env.onboardingMod.Onboarding(props));
  const ta = env.h.findAll(t1, (n) => n.type === "textarea" && n.props?.id === "ob-words")[0];
  assert.ok(ta, "recovery words textarea must render");
  ta.props.onChange({ target: { value: Array(24).fill("amber").join(" ") } });

  const t2 = env.h.render(() => env.onboardingMod.Onboarding(props));
  const frame = env.h.findAll(t2, (n) => typeof n.type === "function" && n.type.name === "Frame")[0];
  const contAction = frame.props.actions.find((a) => a.label === "Continue");
  assert.ok(contAction, "Continue action must exist");
  contAction.onClick();

  const t3 = env.h.render(() => env.onboardingMod.Onboarding(props));
  const confirmEl = env.h.findAll(t3, (n) => n.type === env.Confirm)[0];
  assert.ok(confirmEl, "mismatch confirm dialog must open before restoreWords");
  assert.equal(confirmEl.props.okLabel, "Restore alex.111111", "confirm button must name A only ('Restore <A>')");
  assert.ok(String(confirmEl.props.title).includes("Restore alex.111111"), "confirm title must name A only ('Restore <A>')");

  confirmEl.props.onOk();
  await new Promise((r) => setTimeout(r, 10));
  assert.deepEqual(refused, [NATIVE_ERR], "native refusal must route back to mismatch screen with error");
});
