// Synthetic renderer fixture: actual MessageInfoBody and platform styles, no core.
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";
import type { Message } from "../src/api";
import { MessageInfoBody } from "../src/components/MessageInfo";
import { PlatformCtx, type Platform } from "../src/components/ui";
import { dayLabel, ms, timeSec } from "../src/lib/format";

const platform = new URLSearchParams(location.search).get("platform") as Platform;
const root = createRoot(document.getElementById("root")!);
const t = "2026-10-18T10:00:00Z", fetched = "2026-10-18T10:00:01Z";
const base: Message = {
  id: "synthetic", peer: "sample.123456", outgoing: true, hub: null, body: "",
  kind: null, reply_to: null, sent_at: t, received_at: t, created_at: t,
  state: "sent", fetched_at: null, delivered_at: null, read_at: null,
  error: null, seen: false, attachments: [],
};
const checks: { name: string; ok: boolean; detail?: unknown }[] = [];
function check(name: string, ok: boolean, detail?: unknown) { checks.push({ name, ok, detail }); }
const mount = (patch: Partial<Message>) => {
  const m = Object.freeze({ ...base, ...patch });
  flushSync(() => root.render(<PlatformCtx.Provider value={platform}><MessageInfoBody m={m} /></PlatformCtx.Provider>));
  return [...document.querySelectorAll<HTMLLIElement>(".ladder li")];
};
const stamp = (at: string) => timeSec(ms(at)) + " · " + dayLabel(ms(at));

const cases: [string, Partial<Message>, boolean, string][] = [
  ["pending", {}, false, "—"],
  ["fetched only", { fetched_at: fetched }, true, stamp(fetched)],
  ["delivered implies fetched", { delivered_at: t }, true, "time not recorded"],
  ["read alone implies fetched", { read_at: t }, true, "time not recorded"],
  ["delivered preserves fetched time", { fetched_at: fetched, delivered_at: t }, true, stamp(fetched)],
  ["read preserves fetched time", { fetched_at: fetched, read_at: t }, true, stamp(fetched)],
];
for (const [name, patch, done, at] of cases) {
  const steps = mount(patch), row = steps[2];
  check(name + ": all five steps retained", steps.length === 5);
  check(name + ": fetched completion", row.classList.contains("done") === done);
  check(name + ": fetched checkmark", !!row.querySelector("svg") === done);
  check(name + ": fetched time", row.querySelector(".lt")?.textContent === at, row.querySelector(".lt")?.textContent);
  check(name + ": read remains actual", steps[4].classList.contains("done") === !!patch.read_at);
  check(name + ": delivered remains actual", steps[3].classList.contains("done") === !!patch.delivered_at);
}
mount({ outgoing: false, delivered_at: t });
check("incoming keeps Received layout", !document.querySelector(".ladder") && document.body.textContent!.includes("Received"));
mount({ fetched_at: fetched, delivered_at: t });
for (const [i, row] of [...document.querySelectorAll(".ladder li.done")].entries()) {
  const icon = row.querySelector<SVGElement>("svg")!;
  const marker = icon.parentElement!;
  const a = marker.getBoundingClientRect(), b = icon.getBoundingClientRect(), style = getComputedStyle(icon);
  const dx = b.x + b.width / 2 - (a.x + a.width / 2), dy = b.y + b.height / 2 - (a.y + a.height / 2);
  check("marker " + i + ": icon centered", Math.abs(dx) < .1 && Math.abs(dy) < .1, { dx, dy });
  check("marker " + i + ": no link-screen margin", [style.marginTop, style.marginRight, style.marginBottom, style.marginLeft].every(v => v === "0px"));
  check("marker " + i + ": icon inherits marker contrast", style.color === getComputedStyle(marker).color);
}
mount({ delivered_at: t });
Object.assign(window, { messageInfoResult: { platform, checks, failed: checks.filter(c => !c.ok).length } });
