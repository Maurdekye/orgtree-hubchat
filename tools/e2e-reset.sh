#!/usr/bin/env bash
# Remove the TEST build's identity and data (identifier dev.orgtree.hubchat.test,
# built with --config src-tauri/tauri.test.conf.json) so an e2e run starts at
# onboarding. The real Hubchat (dev.orgtree.hubchat) is never touched.
powershell -NoProfile -Command 'Get-Process hubchat -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "*cargo-target*" } | Stop-Process -Force' >/dev/null 2>&1
powershell -NoProfile -Command "cmdkey /delete:'identity:$APPDATA\\dev.orgtree.hubchat.test.dev.orgtree.hubchat'" >/dev/null 2>&1
sleep 1
rm -rf "$APPDATA/dev.orgtree.hubchat.test"
