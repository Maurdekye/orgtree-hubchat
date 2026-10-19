import { initials } from "./lib/format";
import type { MenuInvoke, MenuSnapshot } from "./traymenu-view";

export interface RecentRow { peer: string; name: string; kind: string; unread: number; typing: boolean; picture: string | null; pictureKey: string | null }
export interface RecentSnapshot extends MenuSnapshot { kind: "recent"; recent: RecentRow[]; recentAvailable: boolean }

// Same working glyph, period, badge, 28px rows and dark neutrals as Orgtree
// main/traylist.ts. Avatar uses Hubchat's existing kind/initial conventions.
const spinner = '<svg class="recent-spin" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 6v3l4-4-4-4v3c-4.42 0-8 3.58-8 8 0 1.57.46 3.03 1.24 4.26L6.7 14.8c-.45-.83-.7-1.79-.7-2.8 0-3.31 2.69-6 6-6m6.76 1.74L17.3 9.2c.44.84.7 1.79.7 2.8 0 3.31-2.69 6-6 6v-3l-4 4 4 4v-3c4.42 0 8-3.58 8-8 0-1.57-.46-3.03-1.24-4.26"/></svg>';

export function installRecent(host: HTMLElement, invoke: MenuInvoke) {
  const doc = host.ownerDocument;
  let view: RecentSnapshot | undefined, busy = false, selected = -1;
  const images = new Map<string, string | null>();
  const run = (command: string, args: Record<string, unknown> = {}) => invoke(command, { generation: view?.generation, ...args });
  function close() { if (!busy) { busy = true; void run("hc_tray_popup_close").catch(() => {}); } }
  function select(at: number) {
    selected = at;
    host.querySelectorAll<HTMLButtonElement>("button").forEach((b,i) => {
      b.tabIndex = i === at ? 0 : -1; b.classList.toggle("selected", i === at);
      if (i === at) { b.focus({ preventScroll: true }); b.scrollIntoView?.({ block: "nearest" }); }
    });
  }
  function open(row: RecentRow) {
    if (busy) return; busy = true;
    void run("hc_tray_popup_action", { id: row.peer, checked: null }).catch(() => {});
  }
  function update(next: RecentSnapshot) {
    if (view && (next.generation < view.generation || (next.generation === view.generation && next.revision <= view.revision))) return;
    const same = view?.generation === next.generation;
    const focus = host.contains(doc.activeElement), peer = view?.recent[selected]?.peer;
    const scrollTop = same ? host.querySelector(".recent-pane")?.scrollTop ?? 0 : 0;
    if (!same) images.clear();
    view = next; busy = false; selected = -1;
    doc.documentElement.dataset.menuTheme = "recent";
    host.replaceChildren();
    const width = Math.min(next.maxWidth, 224), height = Math.min(next.maxHeight, Math.max(1, Math.min(10, next.recent.length)) * 28 + 36);
    host.style.width = `${width}px`; host.style.height = `${height}px`;
    const pane = doc.createElement("div"); pane.className = "recent-pane";
    pane.setAttribute("role", "menu"); pane.setAttribute("aria-label", "Recent chats");
    pane.style.width = `${Math.max(1,width-24)}px`; pane.style.maxHeight = `${Math.max(1,height-24)}px`;
    pane.style.left = "12px"; pane.style.top = "12px"; host.append(pane);
    if (!next.recent.length) {
      const empty = doc.createElement("div"); empty.className = "recent-empty";
      empty.textContent = next.recentAvailable ? "No recent chats." : "Recent chats unavailable."; pane.append(empty);
    }
    next.recent.forEach(row => {
      const button = doc.createElement("button"); button.type = "button"; button.tabIndex = -1; button.dataset.peer = row.peer;
      button.className = "recent-row" + (row.unread > 0 ? " unread" : ""); button.setAttribute("role", "menuitem");
      button.setAttribute("aria-label", row.name + (row.typing ? ", typing" : row.unread ? `, ${row.unread} unread` : ""));
      const slot = doc.createElement("span"); slot.className = "recent-activity";
      if (row.typing) { slot.innerHTML = spinner; slot.setAttribute("role", "img"); slot.setAttribute("aria-label", "Typing"); }
      else if (row.unread > 0) { const badge = doc.createElement("b"); badge.className = "recent-badge"; badge.textContent = row.unread > 99 ? "99+" : String(row.unread); slot.append(badge); }
      const avatar = doc.createElement("span"); avatar.className = "recent-avatar k-" + (row.kind === "chat" ? "chat" : row.kind === "org" ? "org" : "person");
      avatar.textContent = row.kind === "chat" ? ">_" : initials(row.name);
      const imageKey = row.peer + "\n" + row.pictureKey;
      const putPicture = (data: string | null | undefined, target = avatar) => {
        if (!data || row.kind === "chat" || !/^[A-Za-z0-9+/=]+$/.test(data)) return;
        const img = doc.createElement("img"); img.alt = ""; img.src = "data:image/png;base64," + data;
        img.addEventListener("error", () => { target.replaceChildren(); target.textContent = initials(row.name); }, { once: true });
        target.replaceChildren(img);
      };
      putPicture(row.picture || images.get(imageKey));
      if (!row.picture && row.pictureKey && !images.has(imageKey) && row.kind !== "chat") {
        images.set(imageKey, null); const generation = next.generation;
        void run("hc_tray_recent_picture", { peer: row.peer, key: row.pictureKey }).then(data => {
          if (view?.generation !== generation || typeof data !== "string") return;
          if (!view.recent.some(r => r.peer === row.peer && r.pictureKey === row.pictureKey)) return;
          images.set(imageKey, data);
          while (images.size > 24) images.delete(images.keys().next().value!);
          const current = Array.from(host.querySelectorAll<HTMLButtonElement>("button")).find(b => b.dataset.peer === row.peer);
          const target = current?.querySelector<HTMLElement>(".recent-avatar");
          if (target) putPicture(data, target);
        }).catch(() => {});
      }
      const name = doc.createElement("span"); name.className = "recent-name"; name.textContent = row.name; name.title = row.name;
      button.append(slot, avatar, name); pane.append(button);
      button.addEventListener("click", () => open(row));
    });
    if (same && focus && peer) { const at = next.recent.findIndex(r => r.peer === peer); if (at >= 0) select(at); }
    pane.scrollTop = scrollTop;
    void run("hc_tray_popup_ready", { revision: next.revision, width, height, rootX: width / 2 }).catch(() => {});
  }
  function key(event: KeyboardEvent) {
    if (!view || busy) return;
    const length = view.recent.length;
    switch (event.key) {
      case "Escape": case "Tab": close(); break;
      case "ArrowDown": if (length) select((selected + 1) % length); break;
      case "ArrowUp": if (length) select((selected < 0 ? length - 1 : selected + length - 1) % length); break;
      case "Home": if (length) select(0); break;
      case "End": if (length) select(length - 1); break;
      case "Enter": case " ": if (view.recent[selected]) open(view.recent[selected]); break;
      default: return;
    }
    event.preventDefault(); event.stopPropagation();
  }
  const outside = (e: PointerEvent) => { if (!(e.target as Element).closest(".recent-pane")) close(); };
  doc.addEventListener("keydown", key); doc.addEventListener("pointerdown", outside);
  return { update, dispose() { doc.removeEventListener("keydown",key); doc.removeEventListener("pointerdown",outside); host.replaceChildren(); } };
}
