#!/usr/bin/env bash
# Packs a built Hubchat Web page into the release's three files (spec R6.4;
# docs/ci-release.md, "Hubchat Web"):
#
#   package.sh <built page folder> <version> <commit> <out folder>
#
#   hubchat-web-<v>-static.tar.gz        the page's files, for any static web server
#   hubchat-web-<v>-docker-amd64.tar.gz  the image, for `docker load` (hubchat-web:<v>)
#   build-info-web.json                  version, commit, each file's SHA-256, the
#                                        program's SHA-256 and the image's id
#
# Steps: the static archive (reproducible: sorted, no times, no owners); the
# server program, built static (musl) with the page inside it, in the official
# Rust image named below; smoke.sh on the program (on Linux, where it can run)
# and on the image; docker save. It needs docker, node, curl, tar, gzip,
# sha256sum. It pushes nothing. Containers and volumes it makes are named
# hubchat-web-pack-<random> and removed on exit; the image hubchat-web:<v> is
# left for the caller.
set -euo pipefail
# The Rust that builds the server. Alpine: its own target is musl, so the
# program is static and runs in an image that holds nothing else.
RUST_IMAGE="${HUBCHAT_WEB_RUST_IMAGE:-rust:1.89-alpine}"
dist=$(cd "$1" && pwd) version=$2 commit=$3
mkdir -p "$4"; out=$(cd "$4" && pwd)
here=$(cd "$(dirname "$0")" && pwd); repo=$(cd "$here/../../.." && pwd)
id="hubchat-web-pack-$(od -An -N4 -tx1 /dev/urandom | tr -d ' \n')"
work="$out/.work"; rm -rf "$work"; mkdir -p "$work/ctx" "$work/logs"
fail() { echo "::error::$*"; exit 1; }
cleanup() { docker rm -f "$id" >/dev/null 2>&1 || true; docker volume rm "$id-target" >/dev/null 2>&1 || true; return 0; }
trap cleanup EXIT
# Git Bash on Windows rewrites /paths in docker's arguments; a no-op elsewhere
dk() { MSYS_NO_PATHCONV=1 docker "$@"; }
win() { if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else printf '%s' "$1"; fi; }

[ -f "$dist/index.html" ] && [ -f "$dist/sw.js" ] || fail "$dist is not a built Hubchat Web (run tools/web-build.mjs first)"
case "$version" in [0-9]*.[0-9]*.[0-9]*) ;; *) fail "not a version: $version";; esac

echo "== the static archive"
static="hubchat-web-$version-static.tar.gz"
# the same bytes for the same page, whoever packs it and when
tar --sort=name --mtime='1970-01-01 00:00:00Z' --owner=0 --group=0 --numeric-owner --format=ustar -C "$dist" -cf - . | gzip -n -9 > "$out/$static"
tar -tzf "$out/$static" | grep -qx './index.html' || fail "$static has no index.html"
echo "$static: $(wc -c < "$out/$static" | tr -d ' ') B, $(tar -tzf "$out/$static" | grep -vc '/$') files"

echo "== the program (static, in $RUST_IMAGE)"
dk run --rm --name "$id" \
  -v "$(win "$repo"):/src:ro" -v "$(win "$dist"):/dist:ro" -v "$(win "$work/ctx"):/out" -v "$id-target:/target" \
  -e CARGO_TARGET_DIR=/target -e HUBCHAT_WEB_DIST=/dist -e CARGO_TERM_COLOR=never -e "CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}" \
  -w /src "$RUST_IMAGE" sh -ec '
    apk add --no-cache musl-dev >/dev/null
    rustc --version
    cargo build --release --locked -p hubchat-web-server
    cp /target/release/hubchat-web /out/hubchat-web
    chmod 0555 /out/hubchat-web'
program="$work/ctx/hubchat-web"
[ -s "$program" ] || fail "no program came out of the build"
echo "hubchat-web: $(wc -c < "$program" | tr -d ' ') B"
if [ "$(uname -s)" = Linux ] && [ "$(uname -m)" = x86_64 ]; then
  bash "$here/smoke.sh" binary "$program" "$version" "$work/logs/binary"
else
  echo "smoke on the program: skipped, this is not x86_64 Linux (the image is smoked below)"
fi

echo "== the image"
dk build -f "$(win "$repo/crates/hubchat-web-server/Dockerfile")" -t "hubchat-web:$version" \
  --build-arg "HUBCHAT_WEB_IMAGE_VERSION=$version" --build-arg "HUBCHAT_WEB_IMAGE_REVISION=$commit" \
  --provenance=false "$(win "$work/ctx")"
bash "$here/smoke.sh" image "hubchat-web:$version" "$version" "$work/logs/image"
# the program in the image is the one built above
dk create --name "$id" "hubchat-web:$version" >/dev/null
dk cp "$id:/hubchat-web" "$(win "$work/from-image")"
dk rm "$id" >/dev/null
cmp -s "$program" "$work/from-image" || fail "the image does not hold the program that was built"
image="hubchat-web-$version-docker-amd64.tar.gz"
dk save "hubchat-web:$version" | gzip -n > "$out/$image"
echo "$image: $(wc -c < "$out/$image" | tr -d ' ') B"

echo "== build-info-web.json"
node "$here/build-info.mjs" "$out" "$version" "$commit" "$program" "$(dk image inspect -f '{{.Id}}' "hubchat-web:$version")" "$dist"
rm -rf "$work"
( cd "$out" && sha256sum "$static" "$image" build-info-web.json )
echo "packed: Hubchat Web $version ($commit)"
