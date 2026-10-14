import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { installMenu, type MenuSnapshot } from "./traymenu-view";
import "./styles/traymenu.css";

const menu = installMenu(document.body, invoke);
// Subscribe first: an open/state update during the initial request cannot be lost.
void listen<MenuSnapshot>("hc-tray-popup", event => menu.update(event.payload)).then(async () => {
  menu.update(await invoke<MenuSnapshot>("hc_tray_popup_snapshot"));
}).catch(() => { /* A cancelled pending open remains hidden. */ });
