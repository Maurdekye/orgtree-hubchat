// Loaded only by the contained scratch proof (tools/proof-dm-long-press.mjs) after it installs its DOM.
import { useToasts } from "../src/lib/toast";
export { api } from "../src/api";
export { Chats } from "../src/components/Android";
export { ChatRows } from "../src/components/ChatList";
export { CtxMenu } from "../src/components/CtxMenu";
export { closeMenu } from "../src/lib/ctxmenu";
export { PlatformCtx } from "../src/components/ui";
export * as store from "../src/lib/store";
export { act, createElement } from "react";
export { createRoot } from "react-dom/client";

/** The toasts on screen, as text, for the proof to read. */
export function ToastProbe() {
  const list = useToasts();
  return <div id="toasts">{list.map((t) => t.text).join("|")}</div>;
}
