#!/usr/bin/env bash
# Serial macOS launch verifier for the SPEC-4 (tray) and SPEC-5 (Babel) apps.
# It distinguishes build success, eight-second process survival, visible-window
# observation, and optional app-provided self-test output. It does not claim
# real end-to-end interaction unless a retained app harness says so.
set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
ROUND=all
DURATION=10
BUILD=0
OUTPUT=""

usage() {
  cat <<'EOF'
Usage: scripts/verify-iter3.sh [options]

  --capability tray|babel|all   Apps to verify (default: all)
  --duration SECONDS           Per-app observation time, minimum 8 (default: 10)
  --build                      Build each release executable before launching
  --output PATH                TSV destination (logs use PATH without .tsv)
  -h, --help                   Show this help

The verifier is macOS-specific because visible windows are read from
CGWindowList. Apps launch strictly one at a time. Available BABEL_SELFTEST or
TRAY_SELFTEST hooks are enabled; stdout/stderr is retained for every app.
EOF
}

while (($#)); do
  case "$1" in
    --capability)
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
  tray|babel|all) ;;
  *) echo "--capability must be tray, babel, or all" >&2; exit 2 ;;
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
  OUTPUT="$ROOT/measurements/verification-iter3/$STAMP/results.tsv"
fi
LOG_DIR=${OUTPUT%.tsv}-logs
mkdir -p "$(dirname "$OUTPUT")" "$LOG_DIR"

TMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/gui-window-check.XXXXXX")
trap 'rm -rf "$TMP_DIR"; exit 130' HUP INT TERM
/usr/bin/swiftc "$ROOT/scripts/window-count.swift" -o "$TMP_DIR/window-count"

printf 'timestamp_utc\tapp\tcapability\tbuild_status\tpid\tprocess_survived_8s\tvisible_window_observed\twindow_id\twindow_title\tselftest_hook\tselftest_output_observed\texit_before_cleanup\tlog\n' > "$OUTPUT"

apps=()
if [[ "$ROUND" == tray || "$ROUND" == all ]]; then
  apps+=(iced-tray egui-tray gpui-tray tauri-tray xilem-tray slint-tray dioxus-tray)
fi
if [[ "$ROUND" == babel || "$ROUND" == all ]]; then
  apps+=(iced-babel egui-babel gpui-babel tauri-babel xilem-babel slint-babel dioxus-babel)
fi

for app in "${apps[@]}"; do
  app_dir="$ROOT/apps/$app"
  capability=${app##*-}
  package=$(sed -n 's/^name = "\([^"]*\)"/\1/p' "$app_dir/Cargo.toml" | head -1)
  executable="$app_dir/target/release/$package"
  log="$LOG_DIR/$app.log"
  build_status=not_requested

  if ((BUILD)); then
    if cargo build --release --manifest-path "$app_dir/Cargo.toml" >"$LOG_DIR/$app-build.log" 2>&1; then
      build_status=passed
    else
      build_status=failed
    fi
  elif [[ ! -x "$executable" ]]; then
    build_status=missing_executable
  fi

  if [[ "$build_status" == failed || "$build_status" == missing_executable ]]; then
    printf '%s\t%s\t%s\t%s\t\tfalse\tfalse\t\t\tnone\tfalse\ttrue\t%s\n' \
      "$(date -u +%FT%TZ)" "$app" "$capability" "$build_status" "$log" >> "$OUTPUT"
    continue
  fi

  hook=none
  env_args=()
  case "$app" in
    iced-babel|tauri-babel)
      hook=BABEL_SELFTEST
      env_args+=(BABEL_SELFTEST=1)
      ;;
    dioxus-tray|tauri-tray)
      hook=TRAY_SELFTEST
      env_args+=(TRAY_SELFTEST=1)
      ;;
  esac

  : > "$log"
  if ((${#env_args[@]})); then
    env "${env_args[@]}" "$executable" >"$log" 2>&1 &
  else
    "$executable" >"$log" 2>&1 &
  fi
  pid=$!
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
  fi
  wait "$pid" 2>/dev/null || true

  selftest_output=false
  if [[ "$hook" == BABEL_SELFTEST ]] &&
     grep -Eiq 'self.?test|grapheme probe|corpus rendered' "$log"; then
    selftest_output=true
  elif [[ "$hook" == TRAY_SELFTEST ]] && grep -Eiq 'self.?test' "$log"; then
    selftest_output=true
  fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$(date -u +%FT%TZ)" "$app" "$capability" "$build_status" "$pid" \
    "$survived" "$visible" "$window_id" "$window_title" "$hook" \
    "$selftest_output" "$exited" "$log" >> "$OUTPUT"
done

echo "wrote $OUTPUT"
rm -rf "$TMP_DIR"
trap - HUP INT TERM
