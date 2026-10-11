// The tray menu (desktop; src-tauri/src/tray.rs). The updater runs here, so
// the tray's update rows show what lib/updates.ts reports and ask it to check
// or install; a setting changed in the tray ("hc-settings") is read again,
// so Settings shows it. Started at boot, before any screen: the tray works
// during onboarding too.
import { api } from "../api";
import { refreshState } from "./store";
import { checkForUpdate, installUpdate, setAutoChecks, subscribeUpdates, trayView } from "./updates";

let started = false;
export function startTray(): void {
  if (started) return;
  started = true;
  let last = "";
  const report = (force = false) => {
    const view = trayView();
    const s = JSON.stringify(view);
    if (s === last && !force) return;
    last = s;
    api.trayUpdates(view).catch(() => { last = ""; });
  };
  subscribeUpdates(() => report());
  report();
  void api.onTray((a) => {
    if (a.k === "check") checkForUpdate().catch(() => {});
    else if (a.k === "install") installUpdate().catch(() => {});
    else setAutoChecks(a.on);
    // the tray ticked its box itself: always answer with the real state
    report(true);
  });
  void api.onSettingsChanged(() => void refreshState());
}
