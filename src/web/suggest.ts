// The hubs to offer someone adding their first hub (spec R6.2a; user 2026-11-11: "if there is a
// running mailhub at the same address as the hubchat server then suggest that", "allow an env var
// that lets the server specify manually any specific mailhub domains to auto-suggest", "setting it
// false disables it"). The server says what it wants in /config.json:
//
//   hub_suggestions: false     nothing is suggested, nothing is probed
//   suggested_hubs: [..]       exactly these, in this order (the server's own address is not added)
//   neither                    the address this page is served from, then https:// plus its host
//
// Only addresses: the Add hub step checks each and shows the cards.
import type { SuggestedHubs } from "../lib/native";

/** At most this many listed hubs are offered. */
const LIMIT = 8;

/** A listed entry as an address: `https://host[:port]`, or a bare `host[:port]` taken as https. */
function listedAddress(entry: unknown): string | null {
  if (typeof entry !== "string") return null;
  const t = entry.trim().replace(/\/+$/, "");
  if (!t) return null;
  return /^[a-z][a-z0-9+.-]*:\/\//i.test(t) ? t : "https://" + t;
}

/** What the server says in /config.json. A server without one (plain files): nothing set. */
async function serverConfig(): Promise<{ name?: unknown; suggested_hubs?: unknown; hub_suggestions?: unknown }> {
  try {
    const r = await fetch(new URL("config.json", document.baseURI), { cache: "no-cache" });
    if (r.ok) {
      const c: unknown = await r.json();
      if (c && typeof c === "object") return c;
    }
  } catch { /* a server without one */ }
  return {};
}

/** The label the server's operator gave it (HUBCHAT_WEB_NAME), for the welcome screen. Empty: none. */
export async function serverName(): Promise<string> {
  const { name } = await serverConfig();
  return typeof name === "string" ? name.trim().slice(0, 48) : "";
}

export async function suggestedHubs(): Promise<SuggestedHubs> {
  const config = await serverConfig();
  if (config.hub_suggestions === false) return { listed: false, addresses: [] };
  const listed: string[] = [];
  if (Array.isArray(config.suggested_hubs)) {
    for (const entry of config.suggested_hubs) {
      const a = listedAddress(entry);
      if (a && !listed.includes(a)) listed.push(a);
    }
  }
  if (listed.length) return { listed: true, addresses: listed.slice(0, LIMIT) };
  // nothing listed: a hub behind the same address as the page, or on the same machine over https
  const own = [location.origin];
  const host = "https://" + location.hostname;
  if (host !== location.origin) own.push(host);
  return { listed: false, addresses: own };
}
