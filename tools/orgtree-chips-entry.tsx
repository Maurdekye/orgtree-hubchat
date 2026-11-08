// Loaded only by the contained scratch proof (tools/proof-orgtree-chips.mjs) after installing its DOM.
import { useToasts } from "../src/lib/toast";
export { api } from "../src/api";
export { Conversation } from "../src/components/Conversation";
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
