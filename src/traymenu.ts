import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { installMenu, type MenuSnapshot } from "./traymenu-view";
import { installRecent, type RecentSnapshot } from "./trayrecent-view";
import "./styles/traymenu.css";

let kind = "", surface: ReturnType<typeof installMenu> | ReturnType<typeof installRecent> | undefined;
let generation = -1, revision = -1;
function update(view: MenuSnapshot & { kind?: string }) {
  if (view.generation < generation || (view.generation === generation && view.revision <= revision)) return;
  generation = view.generation; revision = view.revision;
  const next = view.kind || "menu";
  if (kind !== next) {
    surface?.dispose(); kind = next;
    surface = next === "recent" ? installRecent(document.body, invoke) : installMenu(document.body, invoke);
  }
  if (next === "recent") (surface as ReturnType<typeof installRecent>).update(view as RecentSnapshot);
  else (surface as ReturnType<typeof installMenu>).update(view);
}
// Subscribe first: an open/state update during the initial request cannot be lost.
void listen<MenuSnapshot>("hc-tray-popup", event => update(event.payload)).then(async () => {
  update(await invoke<MenuSnapshot>("hc_tray_popup_snapshot"));
}).catch(() => { /* A cancelled pending open remains hidden. */ });
