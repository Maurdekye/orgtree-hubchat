import React from "react";
import ReactDOM from "react-dom/client";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/app.css";
import desktopCss from "./styles/desktop.css?inline";
import desktopApp from "./styles/app-desktop.css?inline";
import mobileCss from "./styles/mobile.css?inline";
import mobileApp from "./styles/app-mobile.css?inline";
import { api, installApi, isTauri } from "./api";
import { applyTheme } from "./lib/theme";
import App from "./App";

/** The desktop and Android sheets share class names, so only one is loaded. */
function platformStyles(platform: "desktop" | "android") {
  const el = document.createElement("style");
  el.dataset.platform = platform;
  el.textContent = platform === "android" ? mobileCss + "\n" + mobileApp : desktopCss + "\n" + desktopApp;
  document.head.appendChild(el);
  document.documentElement.dataset.platform = platform;
}

async function boot() {
  applyTheme();
  if (!isTauri) {
    const { mockApi } = await import("./lib/mock");
    installApi(mockApi);
  }
  let platform: "desktop" | "android" = "desktop";
  try { platform = (await api.state()).platform; } catch { /* App shows the error */ }
  platformStyles(platform);
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

void boot();
