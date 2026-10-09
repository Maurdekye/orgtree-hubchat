#!/usr/bin/env bash
# Launch smoke for one Linux build of Hubchat, on the runner only:
#   smoke.sh <label> <command...>
# Starts the app on a virtual X display with its own D-Bus session, and
# passes when it is still running after SMOKE_SECONDS (default 30).
set -euo pipefail
label="$1"; shift
secs="${SMOKE_SECONDS:-30}"
log="$RUNNER_TEMP/smoke-$label.log"

# a fresh profile each run: the app's data and config folders
export HOME="$RUNNER_TEMP/home-$label"
mkdir -p "$HOME"

echo "::group::smoke $label: $*"
setsid dbus-run-session -- xvfb-run -a -s "-screen 0 1280x800x24" "$@" >"$log" 2>&1 &
pid=$!
alive=1
for _ in $(seq 1 "$secs"); do
  sleep 1
  if ! kill -0 "$pid" 2>/dev/null; then alive=0; break; fi
done
# everything the run started (its own process group)
ps -o pid,stat,etime,args -g "$pid" || true
data="$HOME/.local/share/dev.orgtree.hubchat"
ls -la "$data" 2>/dev/null || echo "(no data folder at $data)"
if [ "$alive" = 1 ]; then
  kill -TERM -- "-$pid" 2>/dev/null || true
  sleep 3
  kill -KILL -- "-$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
fi
echo "--- app output ($log) ---"
cat "$log" || true
echo "::endgroup::"
if [ "$alive" != 1 ]; then
  echo "::error::smoke $label: the app exited within ${secs}s"
  exit 1
fi
if [ ! -d "$data" ]; then
  echo "::error::smoke $label: the app never created its data folder ($data)"
  exit 1
fi
echo "smoke $label: still running after ${secs}s, data folder present"
