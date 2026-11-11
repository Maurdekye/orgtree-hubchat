// Updates and working offline (spec R6.1): the service worker (web/sw.template.js)
// keeps the app's files, and a newer build waits in it until the person taps
// Update in the apps' own update banner (src/lib/updates.ts asks here).

let registration: ServiceWorkerRegistration | null = null;
/** Settles when the worker is registered, or cannot be. */
let registering: Promise<void> | null = null;

/** Start keeping the app's files. Without a service worker (the development
 *  server, an old browser) the app works as before. */
export function keepAppFiles(): Promise<void> {
  registering ??= (async () => {
    if (!("serviceWorker" in navigator)) return;
    try {
      registration = await navigator.serviceWorker.register(new URL("sw.js", document.baseURI), { updateViaCache: "none" });
    } catch { registration = null; }
  })();
  return registering;
}

/** A newer build that is being fetched has settled: fetched, or failed. */
function settled(r: ServiceWorkerRegistration): Promise<void> {
  const w = r.installing;
  if (!w) return Promise.resolve();
  return new Promise((done) => {
    const check = () => { if (w.state !== "installing") { w.removeEventListener("statechange", check); done(); } };
    w.addEventListener("statechange", check);
    setTimeout(done, 120_000);
    check();
  });
}

/** What the waiting build calls itself. */
function nameOf(w: ServiceWorker): Promise<string> {
  return new Promise((done) => {
    const ch = new MessageChannel();
    ch.port1.onmessage = (e: MessageEvent<{ version?: string; build?: string }>) => done(`${e.data?.version ?? ""}+${e.data?.build ?? ""}`);
    setTimeout(() => done("+"), 3000);
    w.postMessage("version", [ch.port2]);
  });
}

/** Load the waiting build: it takes over, then the page reloads into it. */
function load(r: ServiceWorkerRegistration): Promise<void> {
  return new Promise((_, fail) => {
    const w = r.waiting;
    if (!w) { fail("the new version is gone; reload the page"); return; }
    navigator.serviceWorker.addEventListener("controllerchange", () => location.reload(), { once: true });
    setTimeout(() => fail("the new version didn't start; reload the page"), 20_000);
    w.postMessage("update");
  });
}

/** Ask the server for a newer build. Null: this is the newest, or the server can't be reached. */
export async function newerBuild(): Promise<{ version: string; install: () => Promise<void> } | null> {
  // the app's first check comes as it starts: wait for the registration it began
  await registering;
  const r = registration;
  if (!r) return null;
  await r.update().catch(() => {});
  await settled(r);
  // a first visit has nothing waiting: its worker starts at once
  if (!r.waiting || !navigator.serviceWorker.controller) return null;
  return { version: await nameOf(r.waiting), install: () => load(r) };
}
