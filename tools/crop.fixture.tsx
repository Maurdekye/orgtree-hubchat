import { createRoot } from "react-dom/client";
import { api, installApi } from "../src/api";
import { mockApi } from "../src/lib/mock";
import { startStore, refreshAll } from "../src/lib/store";
import { SettingsModal, SettingsSection } from "../src/components/Settings";
import { PlatformCtx } from "../src/components/ui";
import { setThemePref } from "../src/lib/theme";

const W = window as any;
installApi(mockApi);
const nativeState = await api.state();
let picture = true, revision = 1, platform: "desktop" | "android" = "desktop";
const canvas = document.createElement("canvas"); canvas.width = 400; canvas.height = 240;
const context = canvas.getContext("2d")!; context.fillStyle = "#d99863"; context.fillRect(0, 0, 400, 240); context.fillStyle = '#277a9c'; context.fillRect(0, 0, 100, 240); context.fillStyle = '#dfb94f'; context.fillRect(200, 80, 200, 160);
const png = canvas.toDataURL("image/png").split(",")[1];
const meta = () => ({ revision: String(revision), sha256: picture ? "a".repeat(64) : null, width: 400, height: 240, bytes: 96 });
const saved: any[] = [], previews: string[] = [], errors: string[] = [];
let chooser = 0;
Object.assign(api, {
  state: async () => ({ ...nativeState, platform, me: { ...nativeState.me, name: "Synthetic Account", about: "Local fixture only", picture: meta() } }),
  picture: async () => ({ ...meta(), data: png }),
  picturePreview: async (data: string) => { previews.push(data); if (W.failPreview) throw Error("Synthetic preview failure"); return { data: png, width: 400, height: 240 }; },
  setPicture: async (owner: string, data: string | null, crop: unknown) => {
    saved.push({ owner, data, crop }); if (W.failSave) throw Error("Synthetic save failure"); picture = data !== null; revision++; return { ...meta(), data: picture ? png : null };
  },
});
HTMLInputElement.prototype.click = function () { if (this.type === "file") chooser++; };
W.addEventListener("error", (e: ErrorEvent) => errors.push(e.message));
await startStore();
const root = createRoot(document.getElementById("root")!);
W.proof = { saved, previews, errors, chooser: () => chooser,
  mount: async (mode: "desktop" | "android", theme: "light" | "dark", hasPicture = true) => {
    root.render(null); platform = mode; picture = hasPicture; revision++;
    document.documentElement.dataset.platform = mode;
    setThemePref(theme); await refreshAll();
    root.render(<PlatformCtx.Provider value={mode}>{mode === "desktop"
      ? <SettingsModal tab="profile" setTab={() => {}} onClose={() => {}} />
      : <main className="scr"><header className="bar"><h2>Account</h2></header><div className="scr-body"><SettingsSection tab="profile" /></div></main>}
    </PlatformCtx.Provider>);
  },
  select: (size = 3) => {
    const input = document.getElementById("picture-file") as HTMLInputElement;
    Object.defineProperty(input, "files", { configurable: true, value: [{ size, arrayBuffer: async () => Uint8Array.from([7, 8, 9]).buffer }] });
    input.dispatchEvent(new Event("change", { bubbles: true }));
  },
  dispose: () => root.unmount(),
};
await W.proof.mount("desktop", "dark");
