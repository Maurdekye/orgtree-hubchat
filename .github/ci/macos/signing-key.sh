#!/usr/bin/env bash
# Puts a throwaway updater signing key into the job environment: `tauri build`
# needs one because createUpdaterArtifacts is on. Its signatures do not match
# the pubkey in tauri.conf.json; for a real release, release.yml's sign job
# re-signs the .app.tar.gz with the real key outside this job.
set -euo pipefail
dir="$RUNNER_TEMP/updater-key"
mkdir -p "$dir"
npx tauri signer generate --ci -w "$dir/test.key" -p "" >/dev/null
key="$(cat "$dir/test.key")"
echo "::add-mask::$key"
{
  echo "TAURI_SIGNING_PRIVATE_KEY<<HC_EOF_KEY"
  echo "$key"
  echo "HC_EOF_KEY"
  echo "TAURI_SIGNING_PRIVATE_KEY_PASSWORD="
} >> "$GITHUB_ENV"
echo "updater signing: throwaway key"
