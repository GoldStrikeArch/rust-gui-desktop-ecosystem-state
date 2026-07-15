#!/usr/bin/env bash
# In-container runner for the iteration-5 fault-line probes.
# Usage (host):
#   docker run --rm -v "$PWD":/work -v rcn-cargo-target:/cargo-target \
#     -v rcn-cargo-home:/cargo-home rcn-linux-check:latest \
#     bash /work/linux/probes/run-probes.sh
# Outputs: linux-results/probes.log (combined) + per-probe build/run logs.
set -u

OUT=/work/linux-results
mkdir -p "$OUT"
cd /work/linux/probes || { echo "NO_PROBES_DIR"; exit 2; }
export CARGO_TARGET_DIR=/cargo-target/probes
export CARGO_HOME="${CARGO_HOME:-/cargo-home}"

# Xvfb + dbus session, mirroring linux/run-app.sh
DNUM=$(( (RANDOM % 300) + 600 ))
export DISPLAY=":$DNUM"
Xvfb "$DISPLAY" -screen 0 1280x800x24 >/dev/null 2>&1 &
XVFB_PID=$!
sleep 1
xdpyinfo -display "$DISPLAY" >/dev/null 2>&1 || { echo "XVFB_FAILED"; exit 2; }
eval "$(dbus-launch --sh-syntax 2>/dev/null)" || true

LOG="$OUT/probes.log"
: > "$LOG"
log() { printf '%s\n' "$*" | tee -a "$LOG"; }

log "== probe environment =="
log "date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
log "rustc: $(rustc --version)"
log "DISPLAY=$DISPLAY (Xvfb 1280x800, no WM, no compositor, no tray host)"
log "dbus session: ${DBUS_SESSION_BUS_ADDRESS:-<none>}"
log "xdotool: $(command -v xdotool || echo 'NOT INSTALLED')"

log ""
log "===== dbus check: StatusNotifier host present on session bus? ====="
dbus-send --session --print-reply --dest=org.freedesktop.DBus \
  /org/freedesktop/DBus org.freedesktop.DBus.NameHasOwner \
  string:org.kde.StatusNotifierWatcher 2>&1 | tee -a "$LOG"

log ""
log "===== probe-muda-winit: build (EXPECTED compile error) ====="
cargo build --release --bin probe-muda-winit >"$OUT/probe-muda-winit-build.log" 2>&1
MUDA_RC=$?
log "cargo build exit=$MUDA_RC (full log: linux-results/probe-muda-winit-build.log)"
log "--- rustc errors verbatim ---"
grep -B1 -A22 '^error\[' "$OUT/probe-muda-winit-build.log" | head -80 | tee -a "$LOG"
if [ $MUDA_RC -eq 0 ]; then
  log "(unexpected compile success — running it)"
  "$CARGO_TARGET_DIR/release/probe-muda-winit" >"$OUT/probe-muda-winit-run.log" 2>&1
  log "run exit=$?"
  tee -a "$LOG" < "$OUT/probe-muda-winit-run.log"
fi

build_probe() {
  local P="$1"
  log ""
  log "===== $P: build ====="
  if cargo build --release --bin "$P" >"$OUT/$P-build.log" 2>&1; then
    log "COMPILE_OK"
    return 0
  fi
  log "COMPILE_FAIL (full log: linux-results/$P-build.log)"
  tail -25 "$OUT/$P-build.log" | tee -a "$LOG"
  return 1
}

run_probe() {
  local P="$1"
  log ""
  log "===== $P: run ====="
  "$CARGO_TARGET_DIR/release/$P" >"$OUT/$P-run.log" 2>&1 &
  local PID=$!
  if [ "$P" = "probe-hotkey-x11" ]; then
    sleep 1
    if command -v xdotool >/dev/null 2>&1; then
      log "(firing synthetic Ctrl+Shift+K via xdotool)"
      xdotool key --clearmodifiers ctrl+shift+k 2>&1 | tee -a "$LOG"
    else
      log "(xdotool not installed — registration result alone is the claim test)"
    fi
  fi
  local waited=0
  while kill -0 "$PID" 2>/dev/null && [ "$waited" -lt 15 ]; do
    sleep 1; waited=$((waited + 1))
  done
  if kill -0 "$PID" 2>/dev/null; then
    kill -9 "$PID" 2>/dev/null
    log "exit: KILLED_AFTER_15S (hung)"
  else
    wait "$PID"
    local RC=$?
    if [ "$RC" -ge 128 ]; then
      log "exit=$RC (terminated by signal $((RC - 128)))"
    else
      log "exit=$RC"
    fi
  fi
  log "--- $P output verbatim ---"
  tee -a "$LOG" < "$OUT/$P-run.log"
}

for P in probe-tray-nogtk probe-tray-gtk probe-hotkey-x11; do
  if build_probe "$P"; then
    run_probe "$P"
  fi
done

kill "$XVFB_PID" 2>/dev/null || true
log ""
log "== probes done =="
