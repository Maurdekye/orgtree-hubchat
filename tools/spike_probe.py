"""Spike probe: send messages to the phone's Hubchat address through a hub and
measure how long each takes to come back as a `delivered` receipt (which the
phone posts after it has received, acked and notified).

    python tools/spike_probe.py <hub> <to-address> [count] [interval-s] [label]

Prints one line per message: sent time, delivered latency (or TIMEOUT).
"""
import hashlib
import json
import secrets
import sys
import time
import urllib.request
import uuid

hub = sys.argv[1].rstrip("/")
to = sys.argv[2]
count = int(sys.argv[3]) if len(sys.argv) > 3 else 3
interval = float(sys.argv[4]) if len(sys.argv) > 4 else 5
label = sys.argv[5] if len(sys.argv) > 5 else "probe"

secret = secrets.token_hex(32)
slug = "probe." + hashlib.sha256(secret.encode()).hexdigest()[:6]
auth = {"X-Org-Auth": f"{slug}:{secret}", "Content-Type": "application/json"}


def post(path, body, timeout=80):
    req = urllib.request.Request(hub + path, json.dumps(body).encode(), auth, method="POST")
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read())


post("/api/register", {"slug": slug, "kind": "chat", "username": "probe"})
pending = {}
results = []
next_send = time.time()
sent = 0
deadline = None
while True:
    now = time.time()
    if sent < count and now >= next_send:
        mid = uuid.uuid4().hex
        post("/api/send", {"id": mid, "to": to, "body": f"{label} #{sent + 1} at {time.strftime('%H:%M:%S')}"})
        pending[mid] = now
        sent += 1
        next_send = now + interval
        if sent == count:
            deadline = now + 180
    for r in post("/api/poll?wait=2", {})["receipts"]:
        if r["id"] in pending and r["state"] in ("delivered", "read"):
            t = pending.pop(r["id"])
            results.append(time.time() - t)
            print(f"{time.strftime('%H:%M:%S', time.localtime(t))} delivered after {time.time() - t:.1f} s", flush=True)
    if sent == count and (not pending or time.time() > deadline):
        break
for mid, t in pending.items():
    print(f"{time.strftime('%H:%M:%S', time.localtime(t))} TIMEOUT (no delivered receipt in 180 s)")
print(f"summary {label}: {len(results)}/{count} delivered" + (f", max {max(results):.1f} s" if results else ""))
