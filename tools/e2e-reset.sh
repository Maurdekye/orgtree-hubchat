#!/usr/bin/env bash
# Remove the dev Hubchat identity and data of THIS Windows user so the e2e
# smoke starts at onboarding. Never run this on a machine where Hubchat is
# used for real.
powershell -c "Get-Process hubchat -ErrorAction SilentlyContinue | Stop-Process -Force" >/dev/null 2>&1
powershell -c "cmdkey /delete:'identity:$APPDATA\dev.orgtree.hubchat.dev.orgtree.hubchat'" >/dev/null 2>&1
sleep 1
rm -rf "$APPDATA/dev.orgtree.hubchat"
