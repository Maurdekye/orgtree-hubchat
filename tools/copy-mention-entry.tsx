// Loaded only by the contained scratch proof (tools/proof-copy-mention.mjs) after it installs its DOM.
import { useToasts } from "../src/lib/toast";
export { api } from "../src/api";
export { MessageView } from "../src/components/MessageView";
export { Conversation } from "../src/components/Conversation";
export { ChatRows, RailRows } from "../src/components/ChatList";
export { ContactRow } from "../src/components/Directory";
export { ConvMenu } from "../src/components/Android";
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
