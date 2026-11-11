// Hubchat Web's start: take the core for this tab (spec R1.3), then start
// the same UI the apps use, over it. Until the core is here a plain screen
// says what is happening, and when it cannot come, why.
import type { ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import "../styles/tokens.css";
import "../styles/base.css";
import "../styles/app.css";
import "./web.css";
import { installApi } from "../api";
import { boot } from "../boot";
import { Logo } from "../lib/icons";
import { applyTheme } from "../lib/theme";
import { isApplePhoneOrPad, isInstalled, keepScale, storageRefused, unsupported } from "./device";
import { Install } from "./Install";
import { followKeyboard } from "./viewport";
import { openCore, type Core } from "./webApi";
import { sentence } from "./wire";

const el = document.getElementById("root") as HTMLElement;
let screen: Root | null = null;

function render(what: ReactNode): void {
  screen ??= createRoot(el);
  screen.render(what);
}

/** The screen before the app: the logo, what is happening, and one thing to do about it. */
function show(text: string, o: { title?: string; bad?: boolean; action?: { label: string; run: () => void } } = {}): void {
  render(
    <div className="boot gate" role="status">
      <Logo size={56} />
      {o.title ? <b>{o.title}</b> : null}
      <span className={o.bad ? "err" : undefined}>{text}</span>
      {o.action ? <button type="button" className="btn primary" onClick={o.action.run}>{o.action.label}</button> : null}
    </div>,
  );
}

async function start(takeOver = false): Promise<void> {
  applyTheme();
  const why = unsupported();
  if (why) { show(why.text, { title: why.title }); return; }
  if (isApplePhoneOrPad()) {
    // R3.6: in a tab, an iPhone or iPad only says how to put Hubchat on the Home Screen
    if (!isInstalled()) { render(<Install />); return; }
    keepScale();
    followKeyboard();
  }
  show("Starting…");
  if (await storageRefused()) {
    show("This window gives Hubchat no storage for your key and messages. Open it in an ordinary window.", { title: "Hubchat Web doesn't work in private browsing" });
    return;
  }
  let core: Core | null;
  try {
    core = await openCore(takeOver);
  } catch (e) {
    show("Hubchat couldn't start its core: " + sentence(e), { bad: true, action: { label: "Reload", run: () => location.reload() } });
    return;
  }
  if (!core) {
    show("It works in one tab at a time, so your messages stay in one place.", {
      title: "Hubchat is open in another tab or window",
      action: { label: "Use it here", run: () => void start(true) },
    });
    return;
  }
  // another tab took it: this one starts over, and finds the core taken
  void core.taken.then(() => location.reload());
  screen?.unmount();
  screen = null;
  installApi(core.api);
  await boot();
}

void start();
