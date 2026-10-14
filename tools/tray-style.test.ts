import { test } from "node:test";
import assert from "node:assert/strict";
import { JSDOM } from "jsdom";
import { installMenu, type MenuSnapshot } from "../src/traymenu-view";
import { fixture } from "./tray-style-fixture";

function setup(view = fixture()) {
  const dom = new JSDOM("<!doctype html><body></body>", { pretendToBeVisual: true });
  const calls: { command: string; args: Record<string, unknown> }[] = [];
  const doc = dom.window.document;
  const menu = installMenu(doc.body, async (command, args) => { calls.push({ command, args }); });
  menu.update(view);
  const key = (name: string) => doc.dispatchEvent(new dom.window.KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true }));
  const button = (id: string) => doc.querySelector<HTMLButtonElement>(`[data-id="${id}"]`)!;
  const hover = (id: string) => button(id).dispatchEvent(new dom.window.Event("pointerenter"));
  return { dom, calls, doc, menu, key, button, hover, close: () => { menu.dispose(); dom.window.close(); } };
}

test("menu keeps all native fixture labels, checked and disabled state", () => {
  const t = setup(); try {
    assert.equal(t.doc.querySelectorAll('[role="menu"] > button').length, 11);
    assert.equal(t.button("version").getAttribute("aria-disabled"), "true");
    assert.equal(t.button("autostart").getAttribute("aria-checked"), "true");
    assert.equal(t.button("exit-on-close").getAttribute("aria-checked"), "false");
    t.button("version").click(); assert.equal(t.calls.filter(c => c.command.endsWith("action")).length, 0);
  } finally { t.close(); }
});

test("keyboard skips disabled rows and separators and wraps", () => {
  const t = setup(); try {
    t.key("ArrowDown"); assert.equal(t.doc.activeElement, t.button("open"));
    t.key("ArrowDown"); assert.equal(t.doc.activeElement, t.button("update-auto"));
    t.key("Home"); t.key("ArrowUp"); assert.equal(t.doc.activeElement, t.button("quit"));
    t.key("ArrowDown"); assert.equal(t.doc.activeElement, t.button("open"));
    t.key("End"); assert.equal(t.doc.activeElement, t.button("quit"));
  } finally { t.close(); }
});

test("submenu opens with right arrow, traverses and left returns to its parent", () => {
  const t = setup(); try {
    t.key("n"); assert.equal(t.doc.activeElement, t.button("notifications"));
    t.key("ArrowRight"); assert.equal(t.doc.activeElement, t.button("notify-on"));
    assert.equal(t.button("notifications").getAttribute("aria-expanded"), "true");
    t.key("ArrowDown"); assert.equal(t.doc.activeElement, t.button("notify-text"));
    t.key("ArrowLeft"); assert.equal(t.doc.querySelectorAll('[role="menu"]').length, 1);
    assert.equal(t.doc.activeElement, t.button("notifications"));
  } finally { t.close(); }
});

test("custom check action carries the requested new value exactly once", () => {
  const t = setup(); try {
    t.button("exit-on-close").click(); t.button("exit-on-close").click(); t.key("Enter");
    const actions = t.calls.filter(c => c.command.endsWith("action"));
    assert.equal(actions.length, 1);
    assert.deepEqual(actions[0].args, { generation: 1, id: "exit-on-close", checked: true });
  } finally { t.close(); }
});

test("Escape, Tab and outside clicks dismiss without running an action", () => {
  for (const event of ["Escape", "Tab", "outside"]) {
    const t = setup(); try {
      if (event === "outside") t.doc.body.dispatchEvent(new t.dom.window.Event("pointerdown", { bubbles: true })); else t.key(event);
      assert.equal(t.calls.filter(c => c.command.endsWith("close")).length, 1);
      assert.equal(t.calls.filter(c => c.command.endsWith("action")).length, 0);
    } finally { t.close(); }
  }
});

test("fresh snapshots update open submenu, ticks and availability while preserving focus", () => {
  const t = setup(); try {
    t.key("n"); t.key("ArrowRight");
    const next = fixture(); next.revision = 2;
    next.rows.find(r => r.id === "notifications")!.children[0].checked = false;
    next.rows.find(r => r.id === "update-check")!.enabled = false;
    t.menu.update(next);
    assert.equal(t.button("notify-on").getAttribute("aria-checked"), "false");
    assert.equal(t.button("update-check").getAttribute("aria-disabled"), "true");
    assert.equal(t.doc.activeElement, t.button("notify-on"));
    t.menu.update(fixture()); // stale revision cannot restore ticks
    assert.equal(t.button("notify-on").getAttribute("aria-checked"), "false");
  } finally { t.close(); }
});

test("new opens supersede old generations and reset submenus", () => {
  const t = setup(); try {
    t.hover("notifications"); assert.equal(t.doc.querySelectorAll('[role="menu"]').length, 2);
    const next = fixture(true); next.generation = 2; t.menu.update(next); t.menu.update(fixture());
    assert.equal(t.doc.querySelectorAll('[role="menu"]').length, 1);
    assert.equal(t.doc.documentElement.dataset.menuTheme, "dark");
    t.button("open").click(); assert.equal(t.calls.at(-1)!.args.generation, 2);
  } finally { t.close(); }
});

test("untrusted native labels stay text and trigger no HTML", () => {
  const view = fixture(); view.rows[0].label = '<img src=x onerror="throw 1"> & <script>bad()</script>';
  const t = setup(view); try {
    assert.equal(t.button("open").textContent, view.rows[0].label);
    assert.equal(t.doc.querySelectorAll("img,script").length, 0);
  } finally { t.close(); }
});

test("both submenu directions fit a short/narrow work area", () => {
  for (const left of [true, false]) {
    const view: MenuSnapshot = { ...fixture(false, left), maxWidth: 280, maxHeight: 240 };
    const t = setup(view); try {
      t.hover("notifications");
      for (const el of t.doc.querySelectorAll<HTMLElement>(".tray-pane")) {
        assert.ok(parseFloat(el.style.left) >= 0);
        assert.ok(parseFloat(el.style.top) >= 0);
        assert.ok(parseFloat(el.style.left) + parseFloat(el.style.width) <= 280);
      }
      const ready = t.calls.find(c => c.command.endsWith("ready"))!.args;
      assert.ok(Number(ready.width) <= 280 && Number(ready.height) <= 240);
    } finally { t.close(); }
  }
});
