#!/usr/bin/env bash
# The Hubchat Web image holds one file, the program. This checks that the
# program inside <image> is the one <build-info-web.json> records, by
# SHA-256 (so it does not depend on how the local docker names an image).
#   program-of.sh <image> <build-info-web.json>
set -euo pipefail
image=$1 info=$2
want="$(node -e 'console.log(JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).program.sha256)' "$info")"
id="hubchat-web-check-$(od -An -N4 -tx1 /dev/urandom | tr -d ' \n')"
tmp="$(mktemp -d)"
cleanup() { docker rm -f "$id" >/dev/null 2>&1 || true; rm -rf "$tmp"; return 0; }
trap cleanup EXIT
MSYS_NO_PATHCONV=1 docker create --name "$id" "$image" >/dev/null
( cd "$tmp" && MSYS_NO_PATHCONV=1 docker cp "$id:/hubchat-web" hubchat-web )
got="$(sha256sum "$tmp/hubchat-web" | cut -d' ' -f1)"
[ "$got" = "$want" ] || { echo "::error::$image holds a program $got, not the recorded $want"; exit 1; }
echo "$image holds the recorded program ($want)"
