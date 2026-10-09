#!/usr/bin/env bash
# Static checks of a release APK; prints a report and fails on a hard error.
#   check-apk.sh <apk> [expected versionName]
# UNSIGNED=1 skips the signature checks (for the unsigned Gradle output).
set -euo pipefail
apk="$1"; want="${2:-}"
bt="$ANDROID_HOME/build-tools/$(ls "$ANDROID_HOME/build-tools" | sort -V | tail -1)"
echo "== build-tools: $bt"
echo "== size: $(stat -c %s "$apk") bytes  sha256: $(sha256sum "$apk" | cut -d' ' -f1)"
echo "== badging"
# Read once: `aapt2 | grep -q` can die of SIGPIPE under pipefail.
badging="$("$bt/aapt2" dump badging "$apk")"
grep -E "^(package|sdkVersion|targetSdkVersion|native-code|application-label):" <<< "$badging"
if [ -n "$want" ]; then
  grep -q "versionName='$want'" <<< "$badging" \
    || { echo "::error::versionName is not $want"; exit 1; }
fi
# The release app id: a test build's ".test" suffix (HUBCHAT_TEST_BUILD) must never ship.
grep -qE "^package: name='dev\.orgtree\.hubchat'( |$)" <<< "$badging" \
  || { echo "::error::the package name is not dev.orgtree.hubchat"; exit 1; }
echo "== zipalign -c -P 16 -v 4"
"$bt/zipalign" -c -P 16 -v 4 "$apk" | tail -1
if [ "${UNSIGNED:-0}" = 1 ]; then
  echo "== signature: none expected (unsigned)"
  unzip -Z1 "$apk" | grep -E '^META-INF/.*\.(RSA|DSA|EC|SF)$' && { echo "::error::v1 signature files present"; exit 1; } || true
else
  echo "== apksigner (published APKs are v2 only)"
  sig="$("$bt/apksigner" verify -v --print-certs "$apk" | grep -vE "^WARNING")"; echo "$sig"
  for want in "v1 scheme (JAR signing): false" "v2 scheme (APK Signature Scheme v2): true" "v3 scheme (APK Signature Scheme v3): false"; do
    grep -qF "Verified using $want" <<< "$sig" || { echo "::error::expected 'Verified using $want'"; exit 1; }
  done
fi
echo "== entries"
echo "$(unzip -Z1 "$apk" | wc -l) entries"
echo "== native libraries (LOAD alignment must be 0x4000)"
tmp="$(mktemp -d)"; unzip -q -o "$apk" 'lib/*' -d "$tmp"
for so in "$tmp"/lib/*/*.so; do
  echo "-- ${so#$tmp/}: $(file -b "$so" | cut -d, -f1-2)"
  bad=$(readelf -lW "$so" | awk '$1=="LOAD" && $NF!="0x4000"' | wc -l)
  readelf -lW "$so" | awk '$1=="LOAD"{print "   LOAD align " $NF}'
  if [ "$bad" != 0 ]; then
    case "$so" in */libhubchat_lib.so) echo "::error::libhubchat_lib.so is not 16 KB aligned"; exit 1 ;;
      *) echo "::warning::${so#$tmp/} is not 16 KB aligned (third-party library)" ;; esac
  fi
done
rm -rf "$tmp"
