#!/usr/bin/env bash
# Puts the updater signing key into the job environment for `tauri build`.
# test:    a throwaway key generated here. Its signatures do not match the
#          pubkey in tauri.conf.json, so these bundles cannot update a real
#          install; that is intended.
# release: the real key from repository secrets, read by name; fails closed
#          without the key (the password may be empty).
set -euo pipefail
case "${SIGNING:-}" in
  test)
    dir="$RUNNER_TEMP/updater-key"
    mkdir -p "$dir"
    npx tauri signer generate --ci -w "$dir/test.key" -p "" >/dev/null
    key="$(cat "$dir/test.key")"
    password=""
    ;;
  release)
    # The key is required; an empty password is valid (the real key has none).
    if [ -z "${REAL_KEY:-}" ]; then
      echo "::error::signing=release but TAURI_SIGNING_PRIVATE_KEY is empty"
      exit 1
    fi
    key="$REAL_KEY"
    password="${REAL_PASSWORD:-}"
    ;;
  *)
    echo "::error::signing must be test or release, got '${SIGNING:-}'"
    exit 1
    ;;
esac
echo "::add-mask::$key"
if [ -n "$password" ]; then echo "::add-mask::$password"; fi
{
  echo "TAURI_SIGNING_PRIVATE_KEY<<HC_EOF_KEY"
  echo "$key"
  echo "HC_EOF_KEY"
  echo "TAURI_SIGNING_PRIVATE_KEY_PASSWORD=$password"
} >> "$GITHUB_ENV"
echo "updater signing: $SIGNING"
