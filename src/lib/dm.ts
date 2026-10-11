import { api } from "../api";
import { errText } from "./native";
import { refreshChats } from "./store";
import { toast } from "./toast";

/** Navigate only after the core accepted the close; keep drafts and history. */
export async function closeDm(peer: string, closed: () => void): Promise<void> {
  try {
    await api.closeDm(peer);
    closed();
    await refreshChats();
  } catch (e) { toast(errText(e)); }
}

/** Used by every way of opening a conversation, including notification links. */
export async function openDm(peer: string): Promise<void> {
  await api.openDm(peer);
  await refreshChats();
}
