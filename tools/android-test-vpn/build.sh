#!/usr/bin/env bash
# The test VPN app (README.md), built with the Android SDK's own tools (no
# Gradle). A throwaway signing key is made in the output folder on the first
# build; it signs nothing else.
#   source <toolchain env.sh>; bash tools/android-test-vpn/build.sh <out dir>
# Output: <out dir>/hubchat-test-vpn.apk
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$1"
SDK="$(cygpath -u "${ANDROID_HOME:?set ANDROID_HOME first, for example by sourcing the toolchain env.sh}" 2>/dev/null || echo "$ANDROID_HOME")"
BT="$SDK/build-tools/36.0.0"
JAR="$SDK/platforms/android-35/android.jar"
rm -rf "$OUT/classes" "$OUT/dex"
mkdir -p "$OUT/classes" "$OUT/dex"
javac --release 11 -classpath "$JAR" -d "$OUT/classes" "$HERE"/src/dev/orgtree/hubchat/vpntest/*.java
"$BT/d8.bat" --release --lib "$JAR" --output "$OUT/dex" $(find "$OUT/classes" -name '*.class')
"$BT/aapt2.exe" link --manifest "$HERE/AndroidManifest.xml" -I "$JAR" -o "$OUT/base.apk"
python - "$OUT" <<'EOF'
import shutil, sys, zipfile
out = sys.argv[1]
shutil.copy(out + "/base.apk", out + "/unsigned.apk")
with zipfile.ZipFile(out + "/unsigned.apk", "a", zipfile.ZIP_DEFLATED) as z:
    z.write(out + "/dex/classes.dex", "classes.dex")
EOF
"$BT/zipalign.exe" -f -p 4 "$OUT/unsigned.apk" "$OUT/aligned.apk"
[ -f "$OUT/test.keystore" ] || keytool -genkeypair -keystore "$OUT/test.keystore" -storepass testvpn -keypass testvpn \
  -alias testvpn -keyalg RSA -keysize 2048 -validity 3650 -dname "CN=Hubchat test VPN" >/dev/null
"$BT/apksigner.bat" sign --ks "$OUT/test.keystore" --ks-pass pass:testvpn --out "$OUT/hubchat-test-vpn.apk" "$OUT/aligned.apk"
ls -la "$OUT/hubchat-test-vpn.apk"
