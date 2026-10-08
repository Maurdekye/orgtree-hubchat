// Router: onboarding until an identity exists (and its last step is done),
// then the platform's layout.
import React, { useEffect } from "react";
import { Android } from "./components/Android";
import { Desktop } from "./components/Desktop";
import { Onboarding } from "./components/Onboarding";
import { PlatformCtx, Toasts } from "./components/ui";
import { TitleBar } from "./components/TitleBar";
import { Logo } from "./lib/icons";
import { startStore, useSnap } from "./lib/store";

export default function App() {
  const snap = useSnap();
  useEffect(() => { void startStore(); }, []);
  const platform = snap.state?.platform ?? "desktop";

  // Desktop windows are frameless: our own title bar sits on top of everything.
  const framed = (content: React.ReactNode) =>
    platform === "desktop" ? <div className="win"><TitleBar /><div className="win-body">{content}</div></div> : content;
  if (!snap.ready || !snap.state) {
    return framed(
      <div className="boot">
        <Logo size={56} />
        {snap.error ? <div className="err">Hubchat couldn't start its core: {snap.error}</div> : <span>Starting…</span>}
      </div>
    );
  }
  let body;
  if (snap.onboarding || !snap.state.me) body = <><Onboarding /><Toasts /></>;
  // keyed by address: after switching identity the layout starts afresh
  else body = platform === "android" ? <Android key={snap.state.me.address} /> : <Desktop key={snap.state.me.address} />;
  return <PlatformCtx.Provider value={platform}>{framed(body)}</PlatformCtx.Provider>;
}
