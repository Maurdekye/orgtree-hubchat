// Chat links (user 23:53Z): the profile's QR code is
// hubchat://chat?to=<address>, so a phone's camera opens Hubchat with it and
// Hubchat opens (or starts) the chat with that address. Distinct from the
// link-a-device URLs (hubchat://link?...).

export const chatLink = (address: string) => "hubchat://chat?to=" + encodeURIComponent(address);

/** A chat link's address; "" for a chat link without a usable one; null
 *  when it isn't a chat link at all. */
export function chatLinkTarget(link: string, currentOwner?: string): string | null {
  const t = link.trim();
  if (!/^hubchat:\/\/chat(\/|\?|#|$)/i.test(t)) return null;
  let to = "";
  try {
    const url = new URL(t);
    const owner = url.searchParams.get("owner");
    if (owner !== null && currentOwner !== undefined && owner !== currentOwner) return "";
    to = url.searchParams.get("to") || "";
  } catch { return ""; }
  to = to.trim().replace(/^@?(?:net|org):/i, "");
  return /^[a-z0-9][a-z0-9._-]{0,127}$/i.test(to) && to.includes(".") ? to : "";
}

/** Native launcher view activations never enter the device-linking flow. */
export function shortcutView(link: string): "settings" | "directory" | "newchat" | null {
  try {
    const u = new URL(link);
    if (u.protocol !== "hubchat:" || u.hostname !== "view" || (u.pathname !== "" && u.pathname !== "/")) return null;
    const view = u.searchParams.get("name");
    return view === "settings" || view === "directory" || view === "newchat" ? view : null;
  } catch { return null; }
}
