#!/usr/bin/env bash
# stage.sh <tauri bundle dir> <out dir>
# Copies the release assets, flat and under their final names, into <out>:
#   Hubchat_<v>_universal.dmg
#   Hubchat_<v>_universal.app.tar.gz (+ .sig)   (Tauri names it Hubchat.app.tar.gz)
#   updater-darwin-aarch64.json, updater-darwin-x86_64.json (both -> the universal tarball)
set -euo pipefail
bundle="$1"; out="$2"
v="$(node -p "require('./src-tauri/tauri.conf.json').version")"
mkdir -p "$out"
ls -la "$bundle/macos" "$bundle/dmg"

dmg="$bundle/dmg/Hubchat_${v}_universal.dmg"
tgz="$bundle/macos/Hubchat.app.tar.gz"
for f in "$dmg" "$tgz" "$tgz.sig"; do
  [ -f "$f" ] || { echo "::error::missing $f"; exit 1; }
done

name="Hubchat_${v}_universal.app.tar.gz"
cp "$dmg" "$out/Hubchat_${v}_universal.dmg"
cp "$tgz" "$out/$name"
cp "$tgz.sig" "$out/$name.sig"
for key in darwin-aarch64 darwin-x86_64; do
  ASSET="$name" KEY="$key" SIG="$out/$name.sig" node "$(dirname "$0")/updater-json.cjs" > "$out/updater-$key.json"
done
ls -la "$out"
(cd "$out" && shasum -a 256 -- *)
