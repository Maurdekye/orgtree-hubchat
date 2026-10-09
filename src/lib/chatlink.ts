// Chat links (user 23:53Z): the profile's QR code is
// hubchat://chat?to=<address>, so a phone's camera opens Hubchat with it and
// Hubchat opens (or starts) the chat with that address. Distinct from the
// link-a-device URLs (hubchat://link?...).

export const chatLink = (address: string) => "hubchat://chat?to=" + encodeURIComponent(address);

/** A chat link's address; "" for a chat link without a usable one; null
 *  when it isn't a chat link at all. */
export function chatLinkTarget(link: string): string | null {
  const t = link.trim();
  if (!/^hubchat:\/\/chat(\/|\?|#|$)/i.test(t)) return null;
  let to = "";
  try { to = new URL(t).searchParams.get("to") || ""; } catch { return ""; }
  to = to.trim().replace(/^@?net:/i, "");
  return /^[a-z0-9][a-z0-9._-]{0,127}$/i.test(to) && to.includes(".") ? to : "";
}
