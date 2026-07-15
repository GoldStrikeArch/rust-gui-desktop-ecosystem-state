#!/usr/bin/env bash
# Serial macOS visible-window verifier for every experiment round.
# This records build availability, eight-second process survival, and a
# CoreGraphics layer-0 window. It does not claim capability correctness.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
ROUND=all
DURATION=8
BUILD=0
OUTPUT=""
TMP_OUTPUT=""
TMP_INVENTORY=""
STAGED_LOG_DIR=""
current_pid=""

usage() {
  cat <<'EOF'
Usage: scripts/verify-windows.sh [options]

  --round iter1|iter2|iter3|iter4|all
                              Apps to verify (default: all)
  --duration SECONDS          Per-app observation time, minimum 8 (default: 8)
  --build                     Build each release executable before launching
  --output PATH               TSV destination (logs use PATH without .tsv)
  -h, --help                  Show this help

The verifier is macOS-specific because it reads visible windows through
CGWindowList. Apps are launched strictly one at a time. No capability self-test
hooks are enabled; this script records launch and visible-window evidence only.
EOF
}

while (($#)); do
  case "$1" in
    --round)
      ROUND=${2:-}
      shift 2
      ;;
    --duration)
      DURATION=${2:-}
      shift 2
      ;;
    --build)
      BUILD=1
      shift
      ;;
    --output)
      OUTPUT=${2:-}
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$ROUND" in
  iter1|iter2|iter3|iter4|all) ;;
  *) echo "--round must be iter1, iter2, iter3, iter4, or all" >&2; exit 2 ;;
esac
case "$DURATION" in
  ''|*[!0-9]*) echo "--duration must be an integer" >&2; exit 2 ;;
esac
if ((DURATION < 8)); then
  echo "--duration must be at least 8 seconds" >&2
  exit 2
fi
if [[ $(uname -s) != Darwin ]]; then
  echo "visible-window verification currently requires macOS" >&2
  exit 2
fi

STAMP=$(date -u +%Y%m%dT%H%M%SZ)
if [[ -z "$OUTPUT" ]]; then
  OUTPUT="$ROOT/measurements/verification-all/$STAMP/results.tsv"
elif [[ "$OUTPUT" != /* ]]; then
  OUTPUT="$ROOT/$OUTPUT"
fi
LOG_DIR=${OUTPUT%.tsv}-logs
INVENTORY="$(dirname "$OUTPUT")/binary-inventory.tsv"
mkdir -p "$(dirname "$OUTPUT")"
TMP_OUTPUT=$(mktemp "${OUTPUT}.tmp.XXXXXX")
TMP_INVENTORY=$(mktemp "${INVENTORY}.tmp.XXXXXX")
STAGED_LOG_DIR=$(mktemp -d "$(dirname "$OUTPUT")/.window-logs.tmp.XXXXXX")

TMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/gui-window-check.XXXXXX")
cleanup() {
  if [[ -n "$current_pid" ]] && kill -0 "$current_pid" 2>/dev/null; then
    kill "$current_pid" 2>/dev/null || true
  fi
  rm -rf "$TMP_DIR"
  [[ -z "$STAGED_LOG_DIR" ]] || rm -rf "$STAGED_LOG_DIR"
  [[ -z "$TMP_OUTPUT" ]] || rm -f "$TMP_OUTPUT"
  [[ -z "$TMP_INVENTORY" ]] || rm -f "$TMP_INVENTORY"
}
trap cleanup EXIT
trap 'exit 130' HUP INT TERM
/usr/bin/swiftc "$ROOT/scripts/window-count.swift" -o "$TMP_DIR/window-count"

printf 'timestamp_utc\tapp\tcapability\tbuild_status\tpid\tprocess_survived_8s\tvisible_window_observed\twindow_id\twindow_title\tselftest_hook\tselftest_output_observed\texit_before_cleanup\texecutable_bytes\texecutable_mtime_utc\texecutable_sha256\tlog\n' > "$TMP_OUTPUT"

suffixes=()
case "$ROUND" in
  iter1) suffixes=(app); expected_apps=7 ;;
  iter2) suffixes=(dash board); expected_apps=14 ;;
  iter3) suffixes=(tray babel); expected_apps=14 ;;
  iter4) suffixes=(grid fetch peek); expected_apps=21 ;;
  all) suffixes=(app dash board tray babel grid fetch peek); expected_apps=56 ;;
esac

apps=()
for suffix in "${suffixes[@]}"; do
  for app_dir in "$ROOT"/apps/*-"$suffix"; do
    [[ -d "$app_dir" ]] && apps+=("$(basename "$app_dir")")
  done
done
if ((${#apps[@]} != expected_apps)); then
  echo "expected $expected_apps apps for $ROUND, found ${#apps[@]}" >&2
  exit 1
fi

failures=0
for app in "${apps[@]}"; do
  app_dir="$ROOT/apps/$app"
  capability=${app##*-}
  package=$(sed -n 's/^name = "\([^"]*\)"/\1/p' "$app_dir/Cargo.toml" | head -1)
  executable="$app_dir/target/release/$package"
  log="$STAGED_LOG_DIR/$app.log"
  final_log="$LOG_DIR/$app.log"
  log_record=${final_log#"$ROOT"/}
  build_status=not_requested
  : > "$log"

  if ((BUILD)); then
    if cargo build --release --locked --bin "$package" --manifest-path "$app_dir/Cargo.toml" >"$STAGED_LOG_DIR/$app-build.log" 2>&1; then
      build_status=passed
    else
      build_status=failed
    fi
  elif [[ ! -x "$executable" ]]; then
    build_status=missing_executable
  fi

  if [[ "$build_status" == failed || "$build_status" == missing_executable ]]; then
    printf '%s\t%s\t%s\t%s\t\tfalse\tfalse\t\t\tnone\tfalse\ttrue\t\t\t\t%s\n' \
      "$(date -u +%FT%TZ)" "$app" "$capability" "$build_status" "$log_record" >> "$TMP_OUTPUT"
    failures=$((failures + 1))
    continue
  fi

  executable_bytes=$(stat -f %z "$executable")
  executable_mtime_epoch=$(stat -f %m "$executable")
  executable_mtime_utc=$(date -u -r "$executable_mtime_epoch" +%FT%TZ)
  executable_sha256=$(shasum -a 256 "$executable" | awk '{print $1}')
  "$executable" >"$log" 2>&1 &
  current_pid=$!
  pid=$current_pid
  visible=false
  window_id=""
  window_title=""
  survived=false
  exited=false

  for ((second = 1; second <= DURATION; second++)); do
    sleep 1
    if ! kill -0 "$pid" 2>/dev/null; then
      exited=true
      break
    fi
    if ((second >= 8)); then
      survived=true
    fi
    if [[ "$visible" == false ]]; then
      window=$($TMP_DIR/window-count "$pid" | head -1)
      if [[ -n "$window" ]]; then
        visible=true
        window_id=${window%%$'\t'*}
        if [[ "$window" == *$'\t'* ]]; then
          window_title=${window#*$'\t'}
        fi
      fi
    fi
  done

  if [[ "$exited" == false ]]; then
    kill "$pid" 2>/dev/null || true
    sleep 1
    kill -9 "$pid" 2>/dev/null || true
  fi
  wait "$pid" 2>/dev/null || true
  current_pid=""

  if [[ "$survived" != true || "$visible" != true || "$exited" != false ]]; then
    failures=$((failures + 1))
  fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\tnone\tfalse\t%s\t%s\t%s\t%s\t%s\n' \
    "$(date -u +%FT%TZ)" "$app" "$capability" "$build_status" "$pid" \
    "$survived" "$visible" "$window_id" "$window_title" "$exited" \
    "$executable_bytes" "$executable_mtime_utc" "$executable_sha256" "$log_record" >> "$TMP_OUTPUT"
  echo "$app survived=$survived window=$visible title=$window_title"
done

if ((failures)); then
  echo "verification failed for $failures app(s); current retained evidence was not replaced" >&2
  exit 1
fi

"$ROOT/scripts/record-binary-inventory.py" "$TMP_OUTPUT" --output "$TMP_INVENTORY"
rm -rf "$LOG_DIR"
mv "$STAGED_LOG_DIR" "$LOG_DIR"
STAGED_LOG_DIR=""
mv "$TMP_OUTPUT" "$OUTPUT"
TMP_OUTPUT=""
mv "$TMP_INVENTORY" "$INVENTORY"
TMP_INVENTORY=""
echo "wrote $OUTPUT"
cleanup
trap - EXIT HUP INT TERM
