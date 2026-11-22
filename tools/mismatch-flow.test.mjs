// Headless real-module tests for Linux store-mismatch UI flow:
// - M1: cold start sets onboarding=true; "Use B here" clears onboarding so App
//   routes straight to the signed-in view without restart.
// - Setup invitations held while in mismatch state (with notice, no overlay),
//   released after mismatch resolves via either Use B or Restore A; outside
//   mismatch they open immediately.
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

/** Tiny synchronous React element & hook harness for exercising real components. */
function makeReactHarness() {
  const states = [];
  const refs = [];
  const effects = [];
  let stateIdx = 0;
  let refIdx = 0;

  const React = {
    Fragment: Symbol.for("react.fragment"),
    createElement(type, props, ...children) {
      return { type, props: { ...(props || {}), children } };
    },
    useState(init) {
      const i = stateIdx++;
      if (!(i in states)) states[i] = typeof init === "function" ? init() : init;
      return [
        states[i],
        (v) => {
          states[i] = typeof v === "function" ? v(states[i]) : v;
        },
      ];
    },
    useRef(init) {
      const i = refIdx++;
      if (!(i in refs)) refs[i] = { current: init };
      return refs[i];
    },
    useEffect(fn) {
      effects.push(fn);
    },
    useCallback(fn) {
      return fn;
    },
    useSyncExternalStore(_sub, getSnap) {
      return getSnap();
    },
  };

  function render(fn) {
    stateIdx = 0;
    refIdx = 0;
    effects.length = 0;
    return fn();
  }

  function flushEffects() {
    for (const fn of effects.splice(0)) fn();
  }

  function findAll(node, pred, out = []) {
    if (!node || typeof node !== "object") return out;
    if (Array.isArray(node)) {
      for (const x of node) findAll(x, pred, out);
      return out;
    }
    if (pred(node)) out.push(node);
    const ch = node.props?.children;
    if (ch) findAll(ch, pred, out);
    return out;
  }

  return { React, render, flushEffects, findAll };
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

function loadStoreAndSetup(initialState) {
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
      throw new Error("not stubbed");
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
  return {
    h,
    api,
    store,
    setup,
    toasts,
    usedFound,
    baseMocks,
    setState(s) {
      curState = s;
    },
  };
}

test("M1: cold start sets onboarding=true and Use B here clears onboarding so signed-in Desktop renders without restart", async () => {
  const env = loadStoreAndSetup(makeState());
  await env.store.startStore();
  assert.equal(env.store.getSnap().onboarding, true, "cold launch without active identity sets onboarding=true");
  assert.equal(env.store.getSnap().state.key_lost.found, "maya.222222");

  const Confirm = function Confirm(p) { return p; };
  const Frame = function Frame(p) { return p; };
  const onboardingMod = loadTs("src/components/Onboarding.tsx", {
    ...env.baseMocks,
    "../lib/store": env.store,
    "../lib/setup": env.setup,
    "../lib/toast": { toast: (m) => env.toasts.push(m) },
    "../lib/native": env.baseMocks["./native"],
    "../lib/visibility": env.baseMocks["./visibility"],
    "../lib/recovery": { copyWords: async () => {}, downloadWords: async () => {} },
    "../lib/recoveryFile": { RECOVERY_FILE_ERROR: "err", RECOVERY_FILE_LIMIT: 4096, recoveryFileWords: (t) => t },
    "../lib/icons": { Icon: () => null, Logo: () => null },
    "../lib/peers": { splitAddr: (a) => [a, ""] },
    "../lib/chatlink": { chatLinkTarget: () => null },
    "./HubAdder": { HubAdder: () => null },
    "./HubReview": { HubReview: () => null, useHubReview: () => ({ urls: [], blocked: false }) },
    "./LinkDevice": { formatCode: (s) => s },
    "./Guide": { openGuide: () => {} },
    "./ui": { Confirm, NoteCard: () => null, QR: () => null, useNow: () => 0, usePlatform: () => "desktop", useWeb: () => false },
    "../assets/desktop-footer-qr.png": "",
    "../assets/desktop-footer-qr.json": { x: 0, y: 0, w: 1, h: 1 },
  }, { React: env.h.React });

  // Render LostKey -> click "Use maya.222222 here" -> re-render -> click Confirm.onOk.
  const tree1 = env.h.render(() => onboardingMod.LostKey({ lost: env.store.getSnap().state.key_lost, onPick: () => {} }));
  const useBtn = env.h.findAll(tree1, (n) => n.props?.t === "Use maya.222222 here")[0];
  assert.ok(useBtn, "Use <found> here option must render on LostKey");
  useBtn.props.onClick();

  const tree2 = env.h.render(() => onboardingMod.LostKey({ lost: env.store.getSnap().state.key_lost, onPick: () => {} }));
  const confirmEl = env.h.findAll(tree2, (n) => n.type === Confirm)[0];
  assert.ok(confirmEl, "Confirm dialog must open for Use B here");
  await confirmEl.props.onOk();

  // Wait one microtask tick for async useFound() chain to finish.
  await new Promise((r) => setTimeout(r, 10));

  assert.deepEqual(env.usedFound, ["maya.222222"]);
  const snap = env.store.getSnap();
  assert.equal(snap.state.me?.address, "maya.222222");
  assert.equal(snap.state.key_lost, null);
  assert.equal(snap.onboarding, false, "Use B here must clear onboarding flag so App routes to Desktop without restart");

  // Confirm App.tsx's exact route condition selects signed-in Desktop (not LostKey, not Onboarding).
  const lost = snap.state.me ? null : snap.state.key_lost;
  assert.equal(lost, null);
  assert.equal(snap.onboarding || !snap.state.me, false, "App.tsx route must select Desktop, not Onboarding");
});

test("setup invitation is held while in mismatch state (both resolution paths) and opens immediately outside mismatch", async () => {
  const INVITE = "hubchat://setup?v=1&hub=http%3A%2F%2F127.0.0.1%3A7371&org=o.1&orgname=Org&pc=pc&code=ABCD-1234&net=wifi&hubname=pc";

  // Path 1: Resolve mismatch via Use B here.
  {
    const env = loadStoreAndSetup(makeState());
    await env.store.startStore();
    env.setup.startSetup(INVITE);

    const activeDuringMismatch = env.h.render(() => env.setup.useSetup());
    assert.equal(activeDuringMismatch, null, "setup overlay must not open while in mismatch state");
    assert.ok(
      env.toasts.includes("Finish choosing an identity first"),
      "user must get notice instead of silent drop",
    );

    // Resolve mismatch via Use B here (me=B, key_lost=null, onboarding=false).
    await env.api.useFoundIdentity("maya.222222");
    env.store.setOnboarding(false);
    await env.store.refreshAll();
    assert.equal(typeof env.setup.syncSetupGate, "function", "setup.ts must expose syncSetupGate for App");
    env.setup.syncSetupGate(false, true);

    const activeAfterB = env.h.render(() => env.setup.useSetup());
    assert.equal(activeAfterB?.input, INVITE, "held setup invitation must open after Use B resolves mismatch");
  }

  // Path 2: Resolve mismatch via Restore A.
  {
    const env = loadStoreAndSetup(makeState());
    await env.store.startStore();
    env.setup.startSetup(INVITE);
    assert.equal(env.h.render(() => env.setup.useSetup()), null, "held during mismatch");

    env.setState(makeState({
      me: { id: "alex", address: "alex.111111", name: "Alex", about: "" },
      key_lost: null,
    }));
    env.store.setOnboarding(false);
    await env.store.refreshAll();
    env.setup.syncSetupGate(false, true);

    const activeAfterA = env.h.render(() => env.setup.useSetup());
    assert.equal(activeAfterA?.input, INVITE, "held setup invitation must open after Restore A resolves mismatch");
  }

  // Outside mismatch state: opens immediately with no mismatch toast.
  {
    const env = loadStoreAndSetup(makeState({ key_lost: null }));
    await env.store.startStore();
    env.setup.startSetup(INVITE);
    const activeNormal = env.h.render(() => env.setup.useSetup());
    assert.equal(activeNormal?.input, INVITE, "outside mismatch state setup invitation opens immediately");
    assert.equal(env.toasts.length, 0, "no mismatch toast outside mismatch state");
  }
});

test("mismatch restore confirm names A only ('Restore <A>') and native refusal returns to mismatch screen with error", async () => {
  const NATIVE_ERR = "This device's chats belong to alex.111111. Use alex.111111's recovery words, link or key file, or choose Use maya.222222 here.";
  const env = loadStoreAndSetup(makeState());
  env.api.restoreWords = async () => { throw new Error(NATIVE_ERR); };
  await env.store.startStore();

  const Confirm = function Confirm(p) { return p; };
  const onboardingMod = loadTs("src/components/Onboarding.tsx", {
    ...env.baseMocks,
    "../lib/store": env.store,
    "../lib/setup": env.setup,
    "../lib/toast": { toast: (m) => env.toasts.push(m) },
    "../lib/native": env.baseMocks["./native"],
    "../lib/visibility": env.baseMocks["./visibility"],
    "../lib/recovery": { copyWords: async () => {}, downloadWords: async () => {} },
    "../lib/recoveryFile": { RECOVERY_FILE_ERROR: "err", RECOVERY_FILE_LIMIT: 4096, recoveryFileWords: (t) => t },
    "../lib/icons": { Icon: () => null, Logo: () => null },
    "../lib/peers": { splitAddr: (a) => [a, ""] },
    "../lib/chatlink": { chatLinkTarget: () => null },
    "./HubAdder": { HubAdder: () => null },
    "./HubReview": { HubReview: () => null, useHubReview: () => ({ urls: [], blocked: false }) },
    "./LinkDevice": { formatCode: (s) => s },
    "./Guide": { openGuide: () => {} },
    "./ui": { Confirm, NoteCard: () => null, QR: () => null, useNow: () => 0, usePlatform: () => "desktop", useWeb: () => false },
    "../assets/desktop-footer-qr.png": "",
    "../assets/desktop-footer-qr.json": { x: 0, y: 0, w: 1, h: 1 },
  }, { React: env.h.React });

  const refused = [];
  const props = {
    start: "restore",
    home: () => {},
    onMismatchRefused: (msg) => refused.push(msg),
  };

  // Render restore step, type 24 words, click Continue.
  const t1 = env.h.render(() => onboardingMod.Onboarding(props));
  const ta = env.h.findAll(t1, (n) => n.type === "textarea" && n.props?.id === "ob-words")[0];
  assert.ok(ta, "recovery words textarea must render");
  ta.props.onChange({ target: { value: Array(24).fill("amber").join(" ") } });

  const t2 = env.h.render(() => onboardingMod.Onboarding(props));
  const frame = env.h.findAll(t2, (n) => typeof n.type === "function" && n.type.name === "Frame")[0];
  const contAction = frame.props.actions.find((a) => a.label === "Continue");
  assert.ok(contAction, "Continue action must exist");
  contAction.onClick();

  const t3 = env.h.render(() => onboardingMod.Onboarding(props));
  const confirmEl = env.h.findAll(t3, (n) => n.type === Confirm)[0];
  assert.ok(confirmEl, "mismatch confirm dialog must open before restoreWords");
  assert.equal(confirmEl.props.okLabel, "Restore alex.111111", "confirm button must name A only ('Restore <A>')");
  assert.ok(String(confirmEl.props.title).includes("Restore alex.111111"), "confirm title must name A only ('Restore <A>')");

  // Approve confirm -> native refuses C -> onMismatchRefused receives native error so App returns to LostKey.
  confirmEl.props.onOk();
  await new Promise((r) => setTimeout(r, 10));
  assert.deepEqual(refused, [NATIVE_ERR], "native refusal must route back to mismatch screen with error");
});
