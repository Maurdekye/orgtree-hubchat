#!/usr/bin/env bash
# Build a debug Android APK without Windows symlink rights.
# `tauri android build` compiles the Rust library and then tries to SYMLINK it into
# jniLibs, which Windows refuses without Developer Mode. This script lets tauri compile,
# copies the .so itself, and runs Gradle with the Rust step skipped.
# Usage: scripts/android-build.sh [debug|release]   (arm64 only for now)
set -u
cd "$(dirname "$0")/.."
PROFILE="${1:-debug}"
[ -f <toolchain>/env.sh ] && source <toolchain>/env.sh
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
FLAG=""; [ "$PROFILE" = debug ] && FLAG="--debug"
npx tauri android build $FLAG --target aarch64 --apk >/dev/null 2>&1 || true   # fails at the symlink step on purpose
SO="$(cygpath -u "${CARGO_TARGET_DIR:-src-tauri/target}")/aarch64-linux-android/$PROFILE/libhubchat_lib.so"
[ -f "$SO" ] || { echo "Rust build failed; run: npx tauri android build $FLAG --target aarch64 --apk"; exit 1; }
mkdir -p src-tauri/gen/android/app/src/main/jniLibs/arm64-v8a
cp -f "$SO" src-tauri/gen/android/app/src/main/jniLibs/arm64-v8a/
CAP="$(tr '[:lower:]' '[:upper:]' <<< "${PROFILE:0:1}")${PROFILE:1}"
cd src-tauri/gen/android && ./gradlew.bat --console=plain -q "assembleArm64$CAP" -x "rustBuildArm64$CAP" || exit 1
ls -la app/build/outputs/apk/arm64/$PROFILE/*.apk
