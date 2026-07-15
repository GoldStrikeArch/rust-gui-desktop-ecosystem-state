#!/usr/bin/env bash
# In-container helper: build + run + screenshot one app under Xvfb.
# Usage (inside container): /work/linux/run-app.sh <app-name> [extra env pairs...]
#   e.g. run-app.sh iced-app  |  run-app.sh tauri-app WEBKIT_DISABLE_DMABUF_RENDERER=1
# Outputs (bind-mounted): /work/linux-results/<app>-{build,run}.log, <app>.png
# Target dirs go to /cargo-target/<app> (named volume) so macOS target/ is untouched.
set -u

APP="$1"; shift || true
SRC="/work/apps/$APP"
OUT="/work/linux-results"
mkdir -p "$OUT"
export CARGO_TARGET_DIR="/cargo-target/$APP"
export CARGO_HOME="${CARGO_HOME:-/cargo-home}"

# extra env pairs (e.g. WEBKIT_DISABLE_DMABUF_RENDERER=1)
for kv in "$@"; do export "$kv"; done

# per-display Xvfb (unique per app to allow parallel agents)
DNUM=$(( (RANDOM % 500) + 100 ))
export DISPLAY=":$DNUM"
Xvfb "$DISPLAY" -screen 0 1280x800x24 > /dev/null 2>&1 &
XVFB_PID=$!
sleep 1
xdpyinfo -display "$DISPLAY" > /dev/null 2>&1 || { echo "XVFB_FAILED"; exit 2; }

# dbus session (menus/tray/notifications plumbing)
eval "$(dbus-launch --sh-syntax 2>/dev/null)" || true

cd "$SRC" || { echo "NO_SRC $SRC"; exit 2; }

echo "== build $APP ($(date -u +%H:%M:%SZ)) ==" | tee "$OUT/$APP-build.log"
if ! cargo build --release >> "$OUT/$APP-build.log" 2>&1; then
  echo "COMPILE_FAIL"
  tail -30 "$OUT/$APP-build.log"
  kill $XVFB_PID 2>/dev/null || true
  exit 1
fi
echo "COMPILE_OK"

BIN="$CARGO_TARGET_DIR/release/$APP"
[ -x "$BIN" ] || BIN=$(find "$CARGO_TARGET_DIR/release" -maxdepth 1 -type f -executable ! -name '*.d' ! -name '*.so' | head -1)
[ -n "$BIN" ] || { echo "NO_BINARY"; kill $XVFB_PID 2>/dev/null || true; exit 1; }

echo "== run $APP (DISPLAY=$DISPLAY) ==" > "$OUT/$APP-run.log"
env | grep -E 'WEBKIT|WGPU|ICED|SLINT|WINIT|RUST_LOG' >> "$OUT/$APP-run.log" 2>/dev/null || true
"$BIN" >> "$OUT/$APP-run.log" 2>&1 &
PID=$!
sleep 10
ALIVE=no
kill -0 "$PID" 2>/dev/null && ALIVE=yes
# screenshot of the whole root window (window manager-less; app fills its own window)
import -display "$DISPLAY" -window root "$OUT/$APP.png" 2>>"$OUT/$APP-run.log" || echo "SCREENSHOT_FAIL" >> "$OUT/$APP-run.log"
kill "$PID" 2>/dev/null || true; sleep 1; kill -9 "$PID" 2>/dev/null || true
kill $XVFB_PID 2>/dev/null || true

echo "RUN_ALIVE_10S=$ALIVE"
echo "--- last run log lines ---"
tail -25 "$OUT/$APP-run.log"
