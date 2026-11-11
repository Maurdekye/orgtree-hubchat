// The hubs to offer someone adding their first hub (user 2026-11-11: "if there is a running mailhub
// at the same address as the hubchat server then suggest that"): the ones the server names in its
// config.json (`default_hubs`), then the page's own address and its host. Only addresses: the
// Add hub step checks each, and shows the first that answers as a suggestion to accept.

/** At most this many addresses are tried. */
const LIMIT = 4;

export async function suggestedHubs(): Promise<string[]> {
  const out: string[] = [];
  const add = (a: unknown) => {
    if (typeof a !== "string") return;
    const t = a.trim().replace(/\/+$/, "");
    if (t && !out.includes(t)) out.push(t);
  };
  try {
    const r = await fetch(new URL("config.json", document.baseURI), { cache: "no-cache" });
    if (r.ok) {
      const c = (await r.json()) as { default_hubs?: unknown };
      if (Array.isArray(c.default_hubs)) c.default_hubs.forEach(add);
    }
  } catch { /* a server without one */ }
  // a hub behind the same address as the page, or on the same machine over https
  add(location.origin);
  add("https://" + location.hostname);
  return out.slice(0, LIMIT);
}
