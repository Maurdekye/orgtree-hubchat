#!/usr/bin/env bash
# Compare a CI-built APK with a published one; informational, never fails the job.
#   compare.sh <ci.apk> <published.apk>
set -uo pipefail
ci="$1"; pub="$2"
bt="$ANDROID_HOME/build-tools/$(ls "$ANDROID_HOME/build-tools" | sort -V | tail -1)"
tmp="$(mktemp -d)"
for x in ci pub; do
  f="${!x}"
  # name, uncompressed size, method, CRC-32 (the v2 signature block is outside the entries)
  unzip -v "$f" | awk '$7 ~ /^[0-9a-f]{8}$/ && NF>=8 {print $8, $1, $2, $7}' | sort > "$tmp/$x.entries"
  "$bt/aapt2" dump badging "$f" | grep -E "^(package|sdkVersion|targetSdkVersion|native-code|application-label|launchable-activity):" > "$tmp/$x.badging"
  "$bt/apksigner" verify -v --print-certs "$f" 2>&1 | grep -E "Verified using|Signer #1 certificate DN|Number of signers" > "$tmp/$x.sign"
  "$bt/zipalign" -c -P 16 4 "$f" && echo "zipalign -P 16: OK" > "$tmp/$x.align" || echo "zipalign -P 16: FAIL" > "$tmp/$x.align"
  echo "size $(stat -c %s "$f")  entries $(wc -l < "$tmp/$x.entries")" > "$tmp/$x.summary"
  mkdir -p "$tmp/$x.lib"; unzip -q -o "$f" 'lib/*' -d "$tmp/$x.lib" 2>/dev/null
  for so in "$tmp/$x.lib"/lib/*/*.so; do
    [ -f "$so" ] || continue
    echo "${so#$tmp/$x.lib/} $(stat -c %s "$so") $(file -b "$so" | cut -d, -f1-2) align=$(readelf -lW "$so" | awk '$1=="LOAD"{print $NF}' | sort -u | tr '\n' ' ')"
  done > "$tmp/$x.so"
done
for part in summary badging sign align so; do
  echo "===== $part (--- published, +++ CI)"
  diff -u "$tmp/pub.$part" "$tmp/ci.$part" && { echo "(identical)"; cat "$tmp/ci.$part"; }
done
echo "===== entries: only in published / only in CI / size differences"
join -v1 <(cut -d' ' -f1 "$tmp/pub.entries") <(cut -d' ' -f1 "$tmp/ci.entries") | sed 's/^/only-published: /'
join -v2 <(cut -d' ' -f1 "$tmp/pub.entries") <(cut -d' ' -f1 "$tmp/ci.entries") | sed 's/^/only-ci: /'
join "$tmp/pub.entries" "$tmp/ci.entries" | awk '$2!=$5 || $3!=$6 || $4!=$7 {print "differs: " $1 "  published " $2 " " $3 " crc " $4 "  ci " $5 " " $6 " crc " $7; d++}
  END {print "byte-identical entries (same size, method, CRC-32): " NR-d " of " NR " common"}'
rm -rf "$tmp"
exit 0
