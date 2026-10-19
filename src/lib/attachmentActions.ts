// Shared by chat attachments and Contact info's file list.
import { api, type Attachment, type Message } from "../api";
import type { MenuEntry } from "./ctxmenu";
import { openImage, previewable } from "./images";
import { errText, openAttachment } from "./native";
import { attView } from "./peers";
import { toast } from "./toast";

export const localAttachment = (m: Message, a: Attachment) =>
  !!(a.local_path && attView(a, m).open) || !!(m.outgoing && a.source && !a.download && attView(a, m).st === "local");

/** The same file operations the chat card/viewer offers, using native bound
 *  message/attachment IDs (also resolves Android retained content URIs). */
export function attachmentMenu(m: Message, a: Attachment, platform: "desktop" | "android"): MenuEntry[] {
  const v = attView(a, m), out: MenuEntry[] = [];
  const run = (p: Promise<unknown>) => void p.catch(e => toast(errText(e)));
  if (previewable(a) && v.st !== "expired") out.push({ label: "View", icon: "image", run: () => openImage(m, a) });
  if (v.download) out.push({ label: "Download", icon: "download", run: () => run(api.download(m.id, a.local_id)) });
  if (v.retry === "download") out.push({ label: "Retry download", icon: "refresh", run: () => run(api.download(m.id, a.local_id)) });
  if (v.retry === "send") out.push({ label: "Retry upload", icon: "refresh", run: () => run(api.retry(m.id)) });
  if (localAttachment(m, a)) {
    out.push({ label: "Open", icon: "open_in_new", run: () => run(openAttachment(m.id, a.local_id, false)) });
    if (platform === "desktop") out.push({ label: "Show in folder", icon: "folder", run: () => run(openAttachment(m.id, a.local_id, true)) });
  }
  if (v.cancel) out.push({ label: m.outgoing && !a.download ? "Cancel upload" : "Cancel download", icon: "close", run: () => run(api.cancelTransfer(a.local_id)) });
  if (m.outgoing && m.state === "failed" && !a.download) out.push({ label: "Remove file", icon: "close", run: () => run(api.removeAttachment(m.id, a.local_id)) });
  return out;
}
