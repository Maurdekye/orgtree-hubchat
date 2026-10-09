#!/usr/bin/env bash
# Sign an unsigned APK with a THROWAWAY key made here (never leaves the job),
# using the same apksigner command release.yml's sign job runs with the real
# keystore: v2 only, as the published Hubchat APKs are.
#   sign-test.sh <unsigned.apk> <signed.apk>
set -euo pipefail
in="$1"; out="$2"
apksigner="$ANDROID_HOME/build-tools/${APKSIGNER_BUILD_TOOLS:-35.0.0}/apksigner"
dir="${RUNNER_TEMP:?}/android-signing"; mkdir -p "$dir"; chmod 700 "$dir"
store="$dir/throwaway.p12"; pass="$(openssl rand -hex 24)"
echo "::add-mask::$pass"
rm -f "$store"
keytool -genkeypair -noprompt -storetype PKCS12 -keystore "$store" \
  -storepass "$pass" -keypass "$pass" -alias hubchat \
  -keyalg RSA -keysize 4096 -validity 10000 \
  -dname "CN=Hubchat TEST (throwaway CI key), O=Orgtree"
KS_PASS="$pass" "$apksigner" sign \
  --ks "$store" --ks-type PKCS12 --ks-key-alias hubchat \
  --ks-pass env:KS_PASS --key-pass env:KS_PASS \
  --v1-signing-enabled false --v2-signing-enabled true \
  --v3-signing-enabled false --v4-signing-enabled false \
  --out "$out" "$in"
rm -rf "$dir"
