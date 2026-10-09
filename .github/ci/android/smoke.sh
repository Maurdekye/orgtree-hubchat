#!/usr/bin/env bash
# Launch smoke on the runner: boot an x86_64 Android emulator whose image
# translates arm64 code, install the APK, start it, and check it stays up.
#   smoke.sh <apk>
set -euo pipefail
apk="$1"; pkg=dev.orgtree.hubchat; img="${SMOKE_IMAGE:-system-images;android-30;google_apis;x86_64}"
sdkm="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"; avdm="$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager"
# KVM for the emulator (GitHub's Ubuntu runners expose /dev/kvm).
echo 'KERNEL=="kvm", GROUP="kvm", MODE="0666", OPTIONS+="static_node=kvm"' | sudo tee /etc/udev/rules.d/99-kvm4all.rules >/dev/null
sudo udevadm control --reload-rules && sudo udevadm trigger --name-match=kvm
(yes 2>/dev/null || true) | "$sdkm" --licenses >/dev/null 2>&1 || true
"$sdkm" --install emulator platform-tools "$img" | tail -3
# avdmanager and the emulator must agree on where AVDs live.
export ANDROID_AVD_HOME="$HOME/.android/avd"; mkdir -p "$ANDROID_AVD_HOME"
echo no | "$avdm" create avd -n smoke -k "$img" --force >/dev/null
export PATH="$ANDROID_HOME/emulator:$ANDROID_HOME/platform-tools:$PATH"
emulator -list-avds
emulator -accel-check || true
log="${RUNNER_TEMP:-/tmp}/smoke"; mkdir -p "$log"
emulator -avd smoke -no-window -no-audio -no-boot-anim -no-snapshot -no-metrics -accel on \
  -gpu swiftshader_indirect -camera-back none -camera-front none > "$log/emulator.log" 2>&1 &
emu=$!
cleanup() {
  rc=$?
  [ "$rc" = 0 ] || { echo "== emulator.log (tail)"; tail -60 "$log/emulator.log"; }
  adb emu kill >/dev/null 2>&1 || true; kill "$emu" 2>/dev/null || true
}
trap cleanup EXIT
sleep 10
kill -0 "$emu" 2>/dev/null || { echo "::error::emulator exited at start"; exit 1; }
timeout 300 adb wait-for-device
for i in $(seq 1 120); do
  [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && break; sleep 5
done
[ "$(adb shell getprop sys.boot_completed | tr -d '\r')" = 1 ] || { echo "::error::emulator did not boot"; exit 1; }
echo "abilist: $(adb shell getprop ro.product.cpu.abilist | tr -d '\r')"
adb shell input keyevent 82 || true
adb install -r -g "$apk"
adb logcat -c
adb shell am start -W -n "$pkg/.MainActivity"
sleep 30
pid="$(adb shell pidof "$pkg" | tr -d '\r' || true)"
adb logcat -d > "$log/logcat.txt" || true
adb shell screencap -p /sdcard/smoke.png && adb pull /sdcard/smoke.png "$log/smoke.png" >/dev/null || true
if grep -E "FATAL EXCEPTION|Fatal signal|$pkg.*(crash|died)" "$log/logcat.txt" | head -20; then
  echo "::error::crash in logcat (see above)"; exit 1
fi
[ -n "$pid" ] || { echo "::error::$pkg is not running 30 s after launch"; grep -i hubchat "$log/logcat.txt" | tail -40; exit 1; }
echo "smoke OK: $pkg running as pid $pid 30 s after launch"
grep -iE "hubchat|RustStdoutStderr|chromium" "$log/logcat.txt" | tail -20 || true
