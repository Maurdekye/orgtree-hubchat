#!/usr/bin/env bash
# Signs a staged Hubchat release folder in place. The only code that ever holds
# the real keys, so it runs nothing from the project beyond the pinned Tauri CLI.
#   sign.sh <dist> [--throwaway-keys]
#
# 1. Android: apksigner signs the UNSIGNED APK (from build-android's second
#    artifact) v2-only, as every published APK is, into Hubchat_<v>_arm64.apk
#    and its fixed-name twin Hubchat-android.apk.
# 2. Updater: every asset named in an updater-*.json fragment is signed with the
#    Tauri updater key; <asset>.sig and the fragment's signature are replaced.
#    This runs after step 1, so an APK named by a fragment is signed as shipped.
#
# Keys come from the environment (the `release` environment's secrets):
#   TAURI_SIGNING_PRIVATE_KEY, TAURI_SIGNING_PRIVATE_KEY_PASSWORD (may be empty),
#   ANDROID_KEYSTORE_B64 (base64 PKCS#12), ANDROID_KEYSTORE_PASSWORD,
#   ANDROID_KEY_ALIAS, ANDROID_KEY_PASSWORD.
# --throwaway-keys makes fresh keys here instead, so test runs exercise this code.
set -euo pipefail
dist="$1"
mode="${2:-}"
: "${VERSION:?VERSION is required}"
case "$mode" in
  ''|--throwaway-keys) ;;
  *) echo "::error::unknown option $mode"; exit 1 ;;
esac
keys="$RUNNER_TEMP/release-keys"
mkdir -p "$keys"
trap 'rm -rf "$keys"' EXIT

# Real keys become files and leave the environment before anything else runs,
# npm included: from here on the tools only ever get them as files.
if [ -z "$mode" ]; then
  missing=""
  for name in TAURI_SIGNING_PRIVATE_KEY ANDROID_KEYSTORE_B64 ANDROID_KEYSTORE_PASSWORD ANDROID_KEY_ALIAS ANDROID_KEY_PASSWORD; do
    [ -n "${!name:-}" ] || missing="$missing $name"
  done
  if [ -n "$missing" ]; then
    echo "::error::the release environment lacks:$missing (docs/ci-release.md, 'Secrets')"
    exit 1
  fi
  printf '%s' "$ANDROID_KEYSTORE_B64" | base64 -d > "$keys/android.p12"
  printf '%s' "$TAURI_SIGNING_PRIVATE_KEY" > "$keys/updater.key"
fi
password="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"   # empty for Hubchat's key today
keystore_password="${ANDROID_KEYSTORE_PASSWORD:-}"
key_password="${ANDROID_KEY_PASSWORD:-}"
alias="${ANDROID_KEY_ALIAS:-}"
unset TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD ANDROID_KEYSTORE_B64 \
  ANDROID_KEYSTORE_PASSWORD ANDROID_KEY_PASSWORD ANDROID_KEY_ALIAS

# The Tauri CLI at the exact version the lockfile pins, installed without running
# any package scripts.
npm ci --ignore-scripts --no-audit --no-fund >/dev/null
tauri() { node node_modules/@tauri-apps/cli/tauri.js "$@"; }
tauri --version

if [ "$mode" = "--throwaway-keys" ]; then
  tauri signer generate --ci -w "$keys/updater.key" -p "" >/dev/null
  password=""
  keytool -genkeypair -keystore "$keys/android.p12" -storetype PKCS12 -alias throwaway \
    -keyalg RSA -keysize 2048 -validity 2 -dname "CN=Hubchat CI throwaway" \
    -storepass throwaway -keypass throwaway >/dev/null 2>&1
  keystore_password=throwaway key_password=throwaway alias=throwaway
  echo "Signing with THROWAWAY keys (test run): nothing signed here can update or install over a real Hubchat."
fi

# 1. Android
build_tools="$ANDROID_HOME/build-tools/35.0.0"
[ -x "$build_tools/apksigner" ] || "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" "build-tools;35.0.0" >/dev/null
unsigned="$dist/Hubchat_${VERSION}_arm64-unsigned.apk"
apk="$dist/Hubchat_${VERSION}_arm64.apk"
[ -f "$unsigned" ] || { echo "::error::missing $unsigned (build-android's hubchat-android-unsigned artifact)"; exit 1; }
"$build_tools/zipalign" -c -P 16 4 "$unsigned"
rm -f "$apk"
KS_PASS="$keystore_password" KEY_PASS="$key_password" "$build_tools/apksigner" sign \
  --ks "$keys/android.p12" --ks-type PKCS12 --ks-key-alias "$alias" \
  --ks-pass env:KS_PASS --key-pass env:KEY_PASS \
  --v1-signing-enabled false --v2-signing-enabled true --v3-signing-enabled false --v4-signing-enabled false \
  --out "$apk" "$unsigned"
verify="$("$build_tools/apksigner" verify --verbose --print-certs "$apk")"
grep -E '^(Verified using|Signer #1 certificate (DN|SHA-256))' <<< "$verify"
for want in 'v1 scheme (JAR signing): false' 'v2 scheme (APK Signature Scheme v2): true' 'v3 scheme (APK Signature Scheme v3): false'; do
  grep -qF "Verified using $want" <<< "$verify" || { echo "::error::$apk is not signed like the published APKs (want: $want)"; exit 1; }
done
"$build_tools/zipalign" -c -P 16 4 "$apk"
cp "$apk" "$dist/Hubchat-android.apk"
rm "$unsigned"

# 2. Updater signatures
shopt -s nullglob
fragments=("$dist"/updater-*.json)
[ ${#fragments[@]} -gt 0 ] || { echo "::error::no updater-*.json fragments in $dist"; exit 1; }
assets="$(node -e '
  const fs = require("fs"); const names = new Set()
  for (const f of process.argv.slice(1)) for (const v of Object.values(JSON.parse(fs.readFileSync(f, "utf8")))) names.add(v.asset)
  console.log([...names].join("\n"))' "${fragments[@]}")"
for asset in $assets; do
  [ -f "$dist/$asset" ] || { echo "::error::a fragment names $asset, which is not in the release"; exit 1; }
  rm -f "$dist/$asset.sig"
  # --app-version: tauri.conf.json sets requireSignedVersion, so the updater refuses
  # a signature that does not name the version (tauri build adds it by itself).
  tauri signer sign -f "$keys/updater.key" -p "$password" --app-version "$VERSION" "$dist/$asset" >/dev/null
  [ -s "$dist/$asset.sig" ] || { echo "::error::tauri signer wrote no $asset.sig"; exit 1; }
  decoded="$(base64 -d "$dist/$asset.sig")"
  grep -qF "version:$VERSION" <<< "$decoded" || { echo "::error::$asset.sig does not bind version $VERSION"; exit 1; }
  echo "signed $asset"
done
node -e '
  const fs = require("fs"), path = require("path")
  for (const f of process.argv.slice(1)) {
    const fragment = JSON.parse(fs.readFileSync(f, "utf8"))
    for (const v of Object.values(fragment)) v.signature = fs.readFileSync(path.join(path.dirname(f), v.asset + ".sig"), "utf8").trim()
    fs.writeFileSync(f, JSON.stringify(fragment, null, 2) + "\n")
  }' "${fragments[@]}"
ls -la "$dist"
