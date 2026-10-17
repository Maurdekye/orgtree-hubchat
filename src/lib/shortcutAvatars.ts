import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "../api";
import { avatarPng } from "./avatarFallback";
import { displayName, kindOf } from "./peers";
import { useSnap } from "./store";

/** Windows only. Native admission checks the owner and current open-DM name;
 * old renderer completions cannot populate another identity's menu. */
export function useShortcutAvatars() {
  const snap = useSnap();
  useEffect(() => {
    const owner = snap.state?.me?.address;
    if (!isTauri || !navigator.userAgent.includes("Windows") || !owner) return;
    let live = true;
    let running = false, again = false;
    const sent = new Map<string, string>();
    const sync = async () => {
      if (running) { again = true; return; }
      running = true;
      do {
        again = false;
        await document.fonts.ready;
        for (const row of snap.chats) {
          if (!live) break;
          const contact = snap.byAddr.get(row.peer);
          if (contact?.picture?.sha256 || kindOf(contact) === "chat") continue;
          const name = displayName(contact, row.peer);
          const png = avatarPng(kindOf(contact), name);
          if (sent.get(row.peer) === png) continue;
          try {
            await invoke("hc_shortcut_avatar", { owner, peer: row.peer, name, kind: kindOf(contact), png });
            sent.set(row.peer, png);
          } catch { /* A closed DM or changed identity is no longer eligible. */ }
        }
      } while (live && again);
      running = false;
    };
    void sync();
    // Includes explicit and system theme changes; computed CSS is authoritative.
    const observer = new MutationObserver(() => { void sync(); });
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class", "data-theme"] });
    return () => { live = false; observer.disconnect(); };
  }, [snap.state?.me?.address, snap.chats, snap.byAddr]);
}
