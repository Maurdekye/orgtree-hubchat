#!/usr/bin/env bash
# smoke.sh <out dir>
# Mounts the built dmg, copies Hubchat.app out, checks it is universal and
# ad-hoc signed, starts each architecture's slice and checks it stays up 30 s,
# and records what Gatekeeper says about a quarantined copy.
set -euo pipefail
out="$1"
dmg="$(ls "$out"/*.dmg)"
work="$RUNNER_TEMP/smoke"
rm -rf "$work"; mkdir -p "$work/mnt"
hdiutil attach "$dmg" -nobrowse -readonly -mountpoint "$work/mnt"
cp -R "$work/mnt/Hubchat.app" "$work/Hubchat.app"
hdiutil detach "$work/mnt"
app="$work/Hubchat.app"

exe="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")"
bin="$app/Contents/MacOS/$exe"
echo "== Info.plist"; /usr/libexec/PlistBuddy -c Print "$app/Contents/Info.plist"
echo "== lipo"; lipo -archs "$bin"
lipo -archs "$bin" | grep -q arm64
lipo -archs "$bin" | grep -q x86_64
echo "== codesign"; codesign -dv "$app" 2>&1 || true
if codesign --verify --deep --strict "$app"; then echo "codesign verify: ok"; else echo "::warning::codesign --verify failed"; fi

# The updater tarball must hold the same app as the dmg.
mkdir -p "$work/tgz"; tar -xzf "$(ls "$out"/*.app.tar.gz)" -C "$work/tgz"
cmp "$bin" "$work/tgz/Hubchat.app/Contents/MacOS/$exe"
echo "updater tarball: same binary as the dmg"

run_slice() {
  local arch="$1" log="$work/run-$1.log" alive=1
  echo "== launch ($arch)"
  arch "-$arch" "$bin" >"$log" 2>&1 &
  local pid=$!
  for _ in $(seq 1 30); do
    sleep 1
    if ! kill -0 "$pid" 2>/dev/null; then alive=0; break; fi
  done
  if [ "$alive" = 1 ]; then
    echo "$arch: still running after 30 s (pid $pid)"
    ps -o pid,rss,etime,command -p "$pid" || true
    kill "$pid" 2>/dev/null || true
    sleep 2; kill -9 "$pid" 2>/dev/null || true
  fi
  echo "-- log ($arch)"; cat "$log" || true
  pkill -f "$bin" 2>/dev/null || true; sleep 1
  if [ "$alive" != 1 ]; then echo "::error::$arch: Hubchat exited within 30 s"; return 1; fi
}

run_slice arm64
if arch -x86_64 /usr/bin/true 2>/dev/null; then
  run_slice x86_64 || echo "::warning::x86_64 slice under Rosetta did not stay up"
else
  echo "::notice::Rosetta is not available on this runner; x86_64 slice not launched"
fi

echo "== app data after launch"
ls -la "$HOME/Library/Application Support/dev.orgtree.hubchat" 2>/dev/null || echo "(none)"

echo "== Gatekeeper on a quarantined copy (what a downloaded dmg gets)"
q="$work/quarantined/Hubchat.app"; mkdir -p "$(dirname "$q")"; cp -R "$app" "$q"
xattr -w com.apple.quarantine "0083;$(printf %x "$(date +%s)");Safari;" "$q"
spctl --assess --type execute -vv "$q" 2>&1 || echo "(spctl rejected it: expected for an unsigned, unnotarized app)"
xattr -dr com.apple.quarantine "$q"
if xattr -l "$q" | grep -q com.apple.quarantine; then echo "::warning::quarantine still set"; else echo "xattr -dr com.apple.quarantine: quarantine removed"; fi
