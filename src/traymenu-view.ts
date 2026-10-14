/** Plain DOM tray surface: no main-app boot, store or network. Native handles
 * supply the live tree and validate every action again before dispatching. */
export interface MenuRow {
  id: string; label: string; enabled: boolean; separator: boolean;
  checked: boolean | null; children: MenuRow[];
}
export interface MenuSnapshot {
  generation: number; revision: number; rows: MenuRow[]; dark: boolean;
  submenuLeft: boolean; maxWidth: number; maxHeight: number;
}
export type MenuInvoke = (command: string, args: Record<string, unknown>) => Promise<unknown>;
interface Pane { rows: MenuRow[]; el: HTMLElement; selected: number; parent?: string }

export function installMenu(host: HTMLElement, invoke: MenuInvoke) {
  const doc = host.ownerDocument;
  let view: MenuSnapshot | undefined, panes: Pane[] = [], busy = false;
  let prefix = "", prefixAt = 0, rootX = 12, menuWidth = 312;
  let frameWidth = 600, frameHeight = 480;
  const run = (command: string, args: Record<string, unknown> = {}) =>
    invoke(command, { generation: view?.generation, ...args });
  const dismiss = () => { if (!busy) { busy = true; void run("hc_tray_popup_close").catch(() => {}); } };
  const buttons = (pane: Pane) => Array.from(pane.el.querySelectorAll<HTMLButtonElement>(":scope > button"));
  function select(pane: Pane, index: number, focus = true) {
    pane.selected = index;
    for (const el of buttons(pane)) {
      const active = Number(el.dataset.index) === index;
      el.classList.toggle("selected", active);
      el.tabIndex = active ? 0 : -1;
      if (active && focus) { el.focus({ preventScroll: true }); el.scrollIntoView?.({ block: "nearest" }); }
    }
  }
  const first = (rows: MenuRow[]) => rows.findIndex(r => !r.separator && r.enabled);
  function trim(depth: number) {
    for (const p of panes.splice(depth)) p.el.remove();
    for (const p of panes) for (const b of buttons(p)) b.setAttribute("aria-expanded", "false");
    for (let i = 1; i < panes.length; i++) {
      const b = buttons(panes[i - 1]).find(b => b.dataset.id === panes[i].parent);
      b?.setAttribute("aria-expanded", "true");
    }
  }
  function openChild(depth: number, index: number, focus: boolean) {
    const pane = panes[depth], row = pane.rows[index];
    trim(depth + 1);
    if (!row?.enabled || !row.children.length) return;
    const child = makePane(row.children, depth + 1, row.id);
    const button = buttons(pane).find(b => b.dataset.id === row.id)!;
    button.setAttribute("aria-expanded", "true");
    const x = Number.parseFloat(pane.el.style.left), y = Number.parseFloat(pane.el.style.top);
    const width = child.el.getBoundingClientRect().width || menuWidth;
    const height = child.el.getBoundingClientRect().height || 128;
    const candidate = view?.submenuLeft ? x - width + 1 : x + menuWidth - 1;
    child.el.style.left = `${Math.max(12, Math.min(frameWidth - width - 12, candidate))}px`;
    child.el.style.top = `${Math.max(8, Math.min(frameHeight - height - 8, y + button.offsetTop))}px`;
    select(child, focus ? first(child.rows) : -1, focus);
  }
  function activate(depth: number, index: number) {
    const row = panes[depth]?.rows[index];
    if (busy || !row?.enabled || row.separator) return;
    if (row.children.length) { openChild(depth, index, true); return; }
    busy = true;
    void run("hc_tray_popup_action", { id: row.id, checked: row.checked === null ? null : !row.checked }).catch(() => {});
  }
  function makePane(rows: MenuRow[], depth: number, parent?: string): Pane {
    const el = doc.createElement("div"); el.className = "tray-pane"; el.setAttribute("role", "menu");
    el.setAttribute("aria-label", parent ? "Submenu" : "Hubchat");
    el.style.width = `${menuWidth}px`; el.style.maxHeight = `${frameHeight - 16}px`;
    const pane: Pane = { rows, el, selected: -1, parent }; panes.push(pane); host.append(el);
    rows.forEach((row, index) => {
      if (row.separator) { const hr = doc.createElement("div"); hr.className = "tray-separator"; hr.setAttribute("role", "separator"); el.append(hr); return; }
      const b = doc.createElement("button"); b.type = "button"; b.tabIndex = -1;
      b.dataset.id = row.id; b.dataset.index = String(index);
      b.setAttribute("role", row.checked === null ? "menuitem" : "menuitemcheckbox");
      b.setAttribute("aria-disabled", String(!row.enabled));
      if (row.checked !== null) b.setAttribute("aria-checked", String(row.checked));
      if (row.children.length) { b.setAttribute("aria-haspopup", "menu"); b.setAttribute("aria-expanded", "false"); }
      const tick = doc.createElement("span"); tick.className = "tray-tick"; tick.setAttribute("aria-hidden", "true");
      if (row.checked) tick.innerHTML = '<svg viewBox="0 0 16 16"><path d="m3 8 3 3 7-7"/></svg>';
      const label = doc.createElement("span"); label.className = "tray-label"; label.textContent = row.label;
      const arrow = doc.createElement("span"); arrow.className = "tray-arrow"; arrow.setAttribute("aria-hidden", "true");
      if (row.children.length) arrow.innerHTML = '<svg viewBox="0 0 16 16"><path d="m6 4 4 4-4 4"/></svg>';
      b.append(tick, label, arrow); el.append(b);
      b.addEventListener("pointerenter", () => {
        if (!row.enabled) return;
        select(pane, index, false);
        if (panes[depth + 1]?.parent !== row.id) openChild(depth, index, false);
      });
      b.addEventListener("click", () => activate(depth, index));
    });
    return pane;
  }
  function update(next: MenuSnapshot) {
    if (view && (next.generation < view.generation || (next.generation === view.generation && next.revision <= view.revision))) return;
    const sameOpen = next.generation === view?.generation;
    const trail = sameOpen ? panes.map(p => ({ id: p.rows[p.selected]?.id, parent: p.parent })) : [];
    const hadFocus = host.contains(doc.activeElement);
    view = next; busy = false; prefix = "";
    doc.documentElement.dataset.menuTheme = next.dark ? "dark" : "light";
    host.replaceChildren(); panes = [];
    frameWidth = Math.min(next.maxWidth, 312 * 2 + 24);
    menuWidth = Math.min(312, Math.max(1, next.maxWidth - 24));
    const height = (rows: MenuRow[]) => rows.reduce((h, r) => h + (r.separator ? 17 : 28), 16);
    frameHeight = Math.min(next.maxHeight, Math.max(height(next.rows), ...next.rows.map(r => height(r.children))) + 16);
    host.style.width = `${frameWidth}px`; host.style.height = `${frameHeight}px`;
    rootX = next.submenuLeft ? frameWidth - menuWidth - 12 : 12;
    const root = makePane(next.rows, 0);
    root.el.style.left = `${rootX}px`; root.el.style.top = "8px";
    select(root, -1, false);
    for (let depth = 0; depth < trail.length; depth++) {
      const pane = panes[depth]; if (!pane) break;
      const at = pane.rows.findIndex(r => r.id === trail[depth].id && r.enabled);
      if (at >= 0) select(pane, at, false);
      if (trail[depth + 1]?.parent === trail[depth].id && at >= 0) openChild(depth, at, false);
    }
    if (sameOpen && hadFocus) select(panes[panes.length - 1], panes[panes.length - 1].selected);
    void run("hc_tray_popup_ready", { revision: next.revision, width: frameWidth, height: frameHeight, rootX }).catch(() => {});
  }
  function key(event: KeyboardEvent) {
    if (!view || busy) return;
    const pane = panes[panes.length - 1]; if (!pane) return;
    const depth = panes.length - 1;
    const move = (delta: number) => {
      for (let n = 1; n <= pane.rows.length; n++) {
        const start = pane.selected < 0 ? (delta > 0 ? -1 : 0) : pane.selected;
        const at = (start + delta * n + pane.rows.length) % pane.rows.length;
        if (pane.rows[at].enabled && !pane.rows[at].separator) { select(pane, at); return; }
      }
    };
    switch (event.key) {
      case "Escape": dismiss(); break;
      case "Tab": dismiss(); break;
      case "ArrowDown": move(1); break;
      case "ArrowUp": move(-1); break;
      case "Home": select(pane, first(pane.rows)); break;
      case "End": select(pane, pane.rows.reduce((last, r, i) => r.enabled && !r.separator ? i : last, -1)); break;
      case "ArrowRight": openChild(depth, pane.selected, true); break;
      case "ArrowLeft": if (depth) { trim(depth); select(panes[depth - 1], panes[depth - 1].selected); } break;
      case "Enter": case " ": activate(depth, pane.selected); break;
      default: {
        if (event.key.length !== 1 || event.ctrlKey || event.metaKey || event.altKey) return;
        const now = Date.now(); prefix = now - prefixAt < 650 ? prefix + event.key.toLocaleLowerCase() : event.key.toLocaleLowerCase(); prefixAt = now;
        const match = pane.rows.findIndex(r => r.enabled && !r.separator && r.label.toLocaleLowerCase().startsWith(prefix));
        if (match >= 0) select(pane, match); break;
      }
    }
    event.preventDefault(); event.stopPropagation();
  }
  const outside = (event: PointerEvent) => { if (!(event.target as Element).closest(".tray-pane")) dismiss(); };
  const context = (event: Event) => event.preventDefault();
  doc.addEventListener("keydown", key); doc.addEventListener("pointerdown", outside); doc.addEventListener("contextmenu", context);
  return { update, dispose() { doc.removeEventListener("keydown", key); doc.removeEventListener("pointerdown", outside); doc.removeEventListener("contextmenu", context); host.replaceChildren(); } };
}
