// Recovery words: copy or download (user 19:08Z). Both count as "saved",
// which stops the reminder.
import { save } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { copyText, errText } from "./native";
import { refreshState } from "./store";
import { toast } from "./toast";

export async function copyWords(words: string[]) {
  await copyText(words.join(" "), "Recovery words copied");
  try { await api.recoverySaved(); await refreshState(); } catch { /* copying still worked */ }
}

/** Desktop: a save dialog. Android: straight to Downloads. */
export async function downloadWords(platform: string, id: string): Promise<boolean> {
  try {
    let dest: string | null = null;
    if (platform !== "android") {
      const picked = await save({ defaultPath: `hubchat-recovery-${id}.txt`, filters: [{ name: "Text", extensions: ["txt"] }] });
      if (!picked) return false;
      dest = String(picked);
    }
    const where = await api.saveRecovery(dest);
    await refreshState();
    toast(platform === "android" ? `Saved to ${where}` : "Recovery words saved");
    return true;
  } catch (e) {
    toast("Couldn't save: " + errText(e));
    return false;
  }
}
