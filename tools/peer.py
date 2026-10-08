"""A scripted chat partner for end-to-end tests: registers on a hub, sends one
message to the app's address, waits for the app's reply, receipts it as
delivered and read, and reports what happened as one JSON line.

    python -I tools/peer.py <hub-url> <app-address> [timeout-s]
"""
import hashlib
import json
import secrets
import sys
import time
import urllib.request
import uuid

hub, target = sys.argv[1].rstrip("/"), sys.argv[2]
deadline = time.time() + (float(sys.argv[3]) if len(sys.argv) > 3 else 60)
secret = secrets.token_hex(16)
slug = "peer." + hashlib.sha256(secret.encode()).hexdigest()[:6]
H = {"X-Org-Auth": f"{slug}:{secret}", "Content-Type": "application/json"}


def post(path, body, timeout=40):
    req = urllib.request.Request(hub + path, json.dumps(body).encode(), H, method="POST")
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read())


out = {"peer": slug}
post("/api/register", {"slug": slug, "kind": "chat", "username": "peer", "org_name": "Test Peer"})
mid = uuid.uuid4().hex
post("/api/send", {"id": mid, "to": target, "body": "hello from **peer**"})
out["sent"] = mid
while time.time() < deadline:
    p = post("/api/poll?wait=5", {})
    for r in p["receipts"]:
        if r["id"] == mid:
            out["our_receipt"] = r["state"]
    got = [m for m in p["messages"] if m["from"] == target]
    if got:
        ids = [m["id"] for m in p["messages"]]
        post("/api/ack", {"ids": ids})
        now = time.strftime("%Y-%m-%dT%H:%M:%S.000Z", time.gmtime())
        post("/api/receipts", {"receipts": [{"id": i, "state": s, "at": now} for i in ids for s in ("delivered", "read")]})
        out["reply"] = got[0]["body"]
        out["reply_id"] = got[0]["id"]
        break
# a last look for our own message's receipt (the app marks it read when shown)
end = time.time() + 15
while time.time() < end and out.get("our_receipt") != "read":
    for r in post("/api/poll?wait=3", {})["receipts"]:
        if r["id"] == mid:
            out["our_receipt"] = r["state"]
print(json.dumps(out), flush=True)
