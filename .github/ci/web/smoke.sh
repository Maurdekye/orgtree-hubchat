#!/usr/bin/env bash
# Smoke-tests Hubchat Web's server, as the built program or as the image
# (spec R6.1 to R6.3; docs/hubchat-web-server.md):
#
#   smoke.sh binary <path to hubchat-web> <version> <log-dir>
#   smoke.sh image  <image>               <version> <log-dir>
#
# Checks: `version` prints exactly `hubchat-web <version>` (a build without
# the real page prints more and fails here); every path of the table with its
# type, caching and safety headers; compressed answers; /config.json follows
# the settings; a bad setting refuses to start; no cookie; nothing but GET
# and HEAD. As an image also: it runs as its non-root user on a read-only
# file system with every capability dropped, becomes healthy by its own
# HEALTHCHECK, and `docker stop` (SIGTERM) ends it with status 0.
#
# Needs curl and node (JSON is compared with node, not jq). Everything on
# loopback; containers are named hubchat-web-smoke-<random> and removed on
# exit; nothing else on the host is touched.
set -euo pipefail
mode=$1 target=$2 version=$3 logs=$4
mkdir -p "$logs"
id="hubchat-web-smoke-$(od -An -N4 -tx1 /dev/urandom | tr -d ' \n')"
pids=() names=()
fail() { echo "::error::$*"; exit 1; }
cleanup() {
  for p in "${pids[@]:-}"; do if [ -n "$p" ]; then kill "$p" 2>/dev/null || true; fi; done
  for n in "${names[@]:-}"; do if [ -n "$n" ]; then docker logs "$n" > "$logs/$n.log" 2>&1 || true; docker rm -f "$n" >/dev/null 2>&1 || true; fi; done
  return 0
}
trap cleanup EXIT
free_port() { node -e 'const s=require("net").createServer();s.listen(0,"127.0.0.1",()=>{console.log(s.address().port);s.close()})'; }

# program <arg>...: run the program once, with the settings in the environment array E
program() {
  if [ "$mode" = image ]; then
    local e=(); for kv in "${E[@]:-}"; do [ -n "$kv" ] && e+=(-e "$kv"); done
    docker run --rm --read-only --cap-drop ALL "${e[@]}" "$target" "$@"
  else
    env "${E[@]:-NO_SETTING=1}" "$target" "$@"
  fi
}
# start: serve with the settings in E; sets $base. In binary mode the port is ours, in image mode docker's.
start() {
  local n="$id-$((${#names[@]} + ${#pids[@]}))"
  if [ "$mode" = image ]; then
    local e=(); for kv in "${E[@]:-}"; do [ -n "$kv" ] && e+=(-e "$kv"); done
    docker run -d --name "$n" --read-only --cap-drop ALL --security-opt no-new-privileges -p 127.0.0.1::8080 \
      --health-interval 2s --health-start-period 0s "${e[@]}" "$target" >/dev/null
    names+=("$n"); last=$n
    local port; port="$(docker port "$n" 8080/tcp | head -n1)"; port=${port##*:}
    base="http://127.0.0.1:$port"
  else
    local port; port="$(free_port)"
    env "${E[@]:-NO_SETTING=1}" HUBCHAT_WEB_BIND=127.0.0.1 HUBCHAT_WEB_PORT="$port" "$target" > "$logs/$n.out" 2> "$logs/$n.err" &
    pids+=($!); last=$!
    base="http://127.0.0.1:$port"
  fi
  for _ in $(seq 100); do curl -fsS -o /dev/null "$base/healthz" 2>/dev/null && return 0; sleep 0.2; done
  fail "the server did not answer on $base within 20 s"
}
# head_of <path> [curl args]: status line and headers, lower-cased names, no CR
head_of() { local p=$1; shift; curl -sS -o /dev/null -D - "$@" "$base$p" | tr -d '\r' | sed -E 's/^([A-Za-z-]+):/\L\1:/'; }
# expect <headers> <name> <value>
expect() { local got; got="$(printf '%s\n' "$1" | sed -n "s/^$2: //p" | head -n1)"; [ "$got" = "$3" ] || fail "$4: $2 is '$got', not '$3'"; }
status_of() { printf '%s\n' "$1" | head -n1 | cut -d' ' -f2; }
same_json() { node -e 'const a=JSON.parse(process.argv[1]),b=JSON.parse(process.argv[2]);const s=(v)=>JSON.stringify(v,Object.keys(v).sort());process.exit(s(a)===s(b)?0:1)' "$1" "$2" || fail "$3: got $1, expected $2"; }

CSP="default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; media-src blob:; connect-src https: http://localhost:* http://127.0.0.1:*; worker-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'"
safety() { # every answer carries these, and never a cookie
  expect "$1" content-security-policy "$CSP" "$2"
  expect "$1" x-content-type-options nosniff "$2"
  expect "$1" referrer-policy no-referrer "$2"
  expect "$1" cross-origin-opener-policy same-origin "$2"
  expect "$1" permissions-policy "camera=(self), microphone=(), geolocation=()" "$2"
  if printf '%s\n' "$1" | grep -qi '^set-cookie:'; then fail "$2: a cookie"; fi
  if printf '%s\n' "$1" | grep -qi '^strict-transport-security:'; then fail "$2: HSTS over plain HTTP"; fi
  if printf '%s\n' "$1" | grep -qi '^access-control-allow-origin:'; then fail "$2: shared with other origins"; fi
}

# 1. what it says it is
E=()
got="$(program version)"
echo "version: $got"
[ "$got" = "hubchat-web $version" ] || fail "it says '$got', not 'hubchat-web $version' (a build without the real page says so here)"
if [ "$mode" = image ]; then
  echo "platform: $(docker image inspect -f '{{.Os}}/{{.Architecture}}' "$target")"
  user="$(docker image inspect -f '{{.Config.User}}' "$target")"
  [ -n "$user" ] && [ "$user" != root ] && [ "$user" != 0 ] && [ "${user%%:*}" != 0 ] || fail "the image runs as '$user': it must not be root"
fi
program files > "$logs/files.txt"
grep -q ' /index\.html$' "$logs/files.txt" || fail "no /index.html inside"
grep -q ' /sw\.js$' "$logs/files.txt" || fail "no /sw.js inside"
grep -q ' /manifest\.webmanifest$' "$logs/files.txt" || fail "no /manifest.webmanifest inside"
wasm="$(awk '$4 ~ /^\/assets\/.*\.wasm$/ {print $4; exit}' "$logs/files.txt")"
script="$(awk '$4 ~ /^\/assets\/.*\.js$/ {print $4; exit}' "$logs/files.txt")"
[ -n "$wasm" ] || fail "no .wasm under /assets/ inside: this is not Hubchat Web"
[ -n "$script" ] || fail "no script under /assets/ inside"
echo "files: $(wc -l < "$logs/files.txt" | tr -d ' ') inside"

# 2. the defaults
start
h="$(head_of /)"; [ "$(status_of "$h")" = 200 ] || fail "/ is $(status_of "$h")"
expect "$h" content-type "text/html; charset=utf-8" /; expect "$h" cache-control no-cache /; safety "$h" /
h="$(head_of /index.html)"; [ "$(status_of "$h")" = 200 ] || fail "/index.html is $(status_of "$h")"
for p in /sw.js /manifest.webmanifest; do
  h="$(head_of $p)"; [ "$(status_of "$h")" = 200 ] || fail "$p is $(status_of "$h")"
  expect "$h" cache-control no-cache $p; safety "$h" $p
  tag="$(printf '%s\n' "$h" | sed -n 's/^etag: //p')"; [ -n "$tag" ] || fail "$p: no ETag"
  h="$(head_of $p -H "If-None-Match: $tag")"; [ "$(status_of "$h")" = 304 ] || fail "$p asked again with its ETag is $(status_of "$h"), not 304"
done
expect "$(head_of /sw.js)" content-type "text/javascript; charset=utf-8" /sw.js
expect "$(head_of /manifest.webmanifest)" content-type application/manifest+json /manifest.webmanifest
h="$(head_of "$wasm")"; [ "$(status_of "$h")" = 200 ] || fail "$wasm is $(status_of "$h")"
expect "$h" content-type application/wasm "$wasm"; expect "$h" cache-control "public, max-age=31536000, immutable" "$wasm"; safety "$h" "$wasm"
raw="$(printf '%s\n' "$h" | sed -n 's/^content-length: //p')"
for coding in br gzip; do
  h="$(head_of "$wasm" -H "Accept-Encoding: $coding")"
  expect "$h" content-encoding $coding "$wasm ($coding)"; expect "$h" vary Accept-Encoding "$wasm ($coding)"
  packed="$(printf '%s\n' "$h" | sed -n 's/^content-length: //p')"
  [ "$packed" -lt "$raw" ] || fail "$wasm as $coding is $packed B, not under $raw B"
  echo "wasm: $raw B, $packed B as $coding"
done
curl -fsS --compressed "$base$script" | head -c 200 > "$logs/script-head.txt" || fail "$script did not come back through curl's own decoding"
expect "$(head_of "$script")" content-type "text/javascript; charset=utf-8" "$script"

h="$(head_of /healthz)"; expect "$h" content-type application/json /healthz; safety "$h" /healthz
same_json "$(curl -fsS "$base/healthz")" "{\"ok\":true,\"app\":\"hubchat-web\",\"version\":\"$version\"}" /healthz
h="$(head_of /config.json)"; expect "$h" content-type application/json /config.json; expect "$h" cache-control no-cache /config.json; safety "$h" /config.json
same_json "$(curl -fsS "$base/config.json")" "{\"name\":\"\",\"suggested_hubs\":[],\"hub_suggestions\":true,\"version\":\"$version\"}" "/config.json with nothing set"

for p in /nope /assets/ /api/hubs /ui /.env "/assets/..%2f..%2findex.html"; do
  h="$(head_of "$p" --path-as-is)"; [ "$(status_of "$h")" = 404 ] || fail "$p is $(status_of "$h"), not 404"; safety "$h" "$p"
done
for m in POST PUT DELETE PATCH OPTIONS; do
  for p in / /config.json /healthz; do
    h="$(head_of $p -X $m)"; [ "$(status_of "$h")" = 405 ] || fail "$m $p is $(status_of "$h"), not 405"
    expect "$h" allow "GET, HEAD" "$m $p"; safety "$h" "$m $p"
  done
done
h="$(curl -sS -I "$base/" | tr -d '\r')"; [ "$(status_of "$h")" = 200 ] || fail "HEAD / is $(status_of "$h")"
echo "paths: ok"

if [ "$mode" = image ]; then
  health=""
  for _ in $(seq 60); do health="$(docker inspect -f '{{.State.Health.Status}}' "$last")"; [ "$health" = healthy ] && break; sleep 1; done
  [ "$health" = healthy ] || fail "the container is '$health', not healthy, after 60 s"
  echo "health: $health"
  docker stop -t 20 "$last" >/dev/null
  code="$(docker inspect -f '{{.State.ExitCode}}' "$last")"
  [ "$code" = 0 ] || fail "after SIGTERM it exited with status $code, not 0"
  echo "stop: exit 0"
else
  E=(HUBCHAT_WEB_BIND=127.0.0.1 "HUBCHAT_WEB_PORT=${base##*:}"); program healthcheck || fail "healthcheck does not pass against the running server"
  echo "healthcheck: ok"
fi

# 3. the settings reach /config.json
E=("HUBCHAT_WEB_NAME=Smoke test" "HUBCHAT_WEB_SUGGEST_HUBS=Hub.Example, https://two.example:8443/,hub.example")
start
same_json "$(curl -fsS "$base/config.json")" "{\"name\":\"Smoke test\",\"suggested_hubs\":[\"https://hub.example\",\"https://two.example:8443\"],\"hub_suggestions\":true,\"version\":\"$version\"}" "/config.json with a name and a list"
E=(HUBCHAT_WEB_HUB_SUGGESTIONS=false HUBCHAT_WEB_SUGGEST_HUBS=hub.example)
start
same_json "$(curl -fsS "$base/config.json")" "{\"name\":\"\",\"suggested_hubs\":[],\"hub_suggestions\":false,\"version\":\"$version\"}" "/config.json with suggestions off"
echo "settings: ok"

# 4. a bad setting refuses to start, and says which
refuses() { # <setting=value> <what it must say>
  E=("$1"); local out code=0
  out="$(program 2>&1)" || code=$?
  [ "$code" = 2 ] || fail "$1: exit $code, not 2 ($out)"
  case "$out" in *"$2"*) ;; *) fail "$1: it says '$out', not '$2'";; esac
}
refuses HUBCHAT_WEB_PORT=eighty 'HUBCHAT_WEB_PORT="eighty"'
refuses HUBCHAT_WEB_HUB_SUGGESTIONS=nope 'HUBCHAT_WEB_HUB_SUGGESTIONS="nope": write true or false.'
refuses HUBCHAT_WEB_SUGGEST_HUBS=http://far.example '"http://far.example" is not a hub address'
refuses HUBCHAT_WEB_LOG=everything 'HUBCHAT_WEB_LOG="everything"'
refuses HUBCHAT_WEB_TLS_CERT=/cert.pem 'HUBCHAT_WEB_TLS_CERT is set but HUBCHAT_WEB_TLS_KEY is not'
echo "bad settings: refused"
echo "smoke passed: $mode $target $version"
