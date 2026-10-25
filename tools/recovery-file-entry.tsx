// Mounted recovery UI; all identities, files and API calls are synthetic.
import { createRoot } from "react-dom/client";
import { Onboarding } from "../src/components/Onboarding";
import { PlatformCtx } from "../src/components/ui";
import { api } from "../src/api";
import { mockApi } from "../src/lib/mock";

declare global { interface Window { runRecoveryProof: () => Promise<unknown>; recoveryFixture: { text: string; words: string }; } }
window.runRecoveryProof = async () => {
  localStorage.clear(); sessionStorage.clear();
  const platform = new URLSearchParams(location.search).get("platform") === "android" ? "android" : "desktop";
  const checks: { name: string; ok: boolean }[] = [], calls: string[] = [];
  const check = (name: string, ok: unknown) => checks.push({ name, ok: !!ok });
  const pause = () => new Promise<void>(r => setTimeout(r, 35));
  let rejectRestore = true;
  Object.assign(api, mockApi, {
    checkId: async () => ({ ok: false, error: null, max_len: 24 }),
    restoreWords: async (s: string) => { calls.push(s); if (rejectRestore) throw Error("These words do not check out"); return "recovery-proof.synthetic"; },
    state: async () => ({ ...await mockApi.state(), me: null, hubs: [], platform }),
    onEvent: async () => () => {},
  });
  const root = createRoot(document.getElementById("root")!);
  const mount = async () => { root.render(<PlatformCtx.Provider value={platform}><Onboarding key={Math.random()} start="restore" /></PlatformCtx.Provider>); await pause(); };
  await mount();
  const words = () => document.querySelector<HTMLTextAreaElement>("#ob-words")!;
  const input = () => document.querySelector<HTMLInputElement>('input[type="file"]');
  const button = (s: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find(b => b.textContent?.trim() === s);
  const type = async (s: string) => { Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(words(), s); words().dispatchEvent(new Event("input", { bubbles: true })); await pause(); };
  const choose = async (file?: File) => { const el = input()!; Object.defineProperty(el, "files", { configurable: true, value: file ? [file] : [] }); el.dispatchEvent(new Event("change", { bubbles: true })); await pause(); };
  const file = (text: string) => new File([text], "synthetic-recovery.txt", { type: "text/plain" });
  const { text, words: expected } = window.recoveryFixture;
  check("file input exists", !!input());
  const picker = button("Choose recovery words file");
  check("named file chooser exists", !!picker);
  await type("manually typed words");
  check("typing stays available", words().value === "manually typed words");
  button("Continue")?.click(); await pause();
  check("Continue uses existing validation", calls.length === 1 && calls[0] === "manually typed words" && !!document.querySelector(".wstat.bad"));
  if (!input()) return { platform, checks }; // Cheap red control: feature absent.
  const rect = picker!.getBoundingClientRect();
  check("chooser has visible in-window geometry", rect.width > 0 && rect.height > 0 && rect.left >= 0 && rect.right <= innerWidth && rect.top >= 0 && rect.bottom <= innerHeight);
  let clicks = 0; input()!.addEventListener("click", e => { e.preventDefault(); clicks++; });
  picker!.click(); await pause(); check("chooser activates system file input", clicks === 1);
  await choose(file(text));
  check("native exporter file fills only ordered words", words().value === expected);
  check("selection does not recover automatically", calls.length === 1);
  check("file input selection clears immediately", input()!.value === "");
  await choose(file("\ufeff" + text.replace(/\r\n/g, "\n")));
  check("BOM and LF exports are accepted", words().value === expected);
  await choose(file(expected.toUpperCase().replace(/ /g, "\n")));
  check("plain words with whitespace are accepted", words().value === expected);
  await choose(); check("cancel keeps current words", words().value === expected);
  await choose(file(text.replace(/ 2\./, " 1.")));
  check("out-of-order numbered file is refused", !!document.querySelector(".wstat.bad"));
  await choose(file(text.replace(/24\. [a-z]+\r?\n/, "")));
  check("truncated export is refused", !!document.querySelector(".wstat.bad"));
  await choose(file("PRIVATE-FILE-CANARY unrelated file"));
  check("wrong file has a fixed private error", !!document.querySelector(".wstat.bad") && !document.body.textContent?.includes("PRIVATE-FILE-CANARY") && !document.body.textContent?.includes("synthetic-recovery.txt"));
  let read = false;
  const huge = file("x"); Object.defineProperty(huge, "size", { value: 65537 }); Object.defineProperty(huge, "text", { value: async () => { read = true; return text; } });
  await choose(huge); check("oversized file is refused before reading", !read && !!document.querySelector(".wstat.bad"));
  const unreadable = file(""); Object.defineProperty(unreadable, "text", { value: async () => { throw Error("PRIVATE-READ-ERROR"); } });
  await choose(unreadable); check("read failure is clear and private", !!document.querySelector(".wstat.bad") && !document.body.textContent?.includes("PRIVATE-READ-ERROR"));
  let resolve!: (s: string) => void;
  const delayed = () => { const f = file(""); Object.defineProperty(f, "text", { value: () => new Promise<string>(r => { resolve = r; }) }); return f; };
  await choose(delayed());
  check("Continue waits for local read", !!document.querySelector<HTMLButtonElement>("button.primary")?.disabled);
  await type("newer manual edit"); resolve(text); await pause();
  check("late file cannot overwrite manual edit", words().value === "newer manual edit");
  await choose(delayed()); const older = resolve;
  await choose(file(expected)); older("wrong stale data"); await pause();
  check("latest selection wins over stale read", words().value === expected && !document.querySelector(".wstat.bad"));
  await choose(delayed()); const leaving = resolve;
  const back = document.querySelector<HTMLButtonElement>('[aria-label="Back"]') ?? button("Back"); back?.click(); await pause();
  const method = [...document.querySelectorAll<HTMLElement>(".method")].find(e => e.textContent?.includes("Recovery words"));
  method?.click(); await pause(); leaving(text); await pause();
  check("Back clears words and invalidates pending read", !!words() && words().value === "");
  await choose(file(expected));
  check("import enables normal Continue", !document.querySelector<HTMLButtonElement>("button.primary")?.disabled);
  rejectRestore = false; button("Continue")?.click(); await pause(); await pause();
  check("imported words reach same recovery API", calls.length === 2 && calls[1] === expected);
  check("success clears detached words field", !words());
  await mount(); await choose(delayed()); const unmounted = resolve;
  root.render(<div>Left recovery</div>); await pause(); unmounted(text); await pause(); await mount();
  check("unmount does not retain pending words", words().value === "");
  await choose(file(text)); // Representative capture: imported, waiting for Continue.
  return { platform, checks };
};
