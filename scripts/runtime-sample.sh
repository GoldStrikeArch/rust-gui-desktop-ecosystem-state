#!/usr/bin/env bash
# Runtime CPU/RSS sampling for the SPEC-2 dash apps (repaint-model comparison).
# Usage: ./scripts/runtime-sample.sh [app-dir ...]   (defaults to apps/*-dash)
# For each app: launch release binary, 5 s warmup, then sample %cpu and rss
# every 1 s for 30 s. For webview apps, also attributes NEW WebKit helper
# processes (WebContent/Networking/GPU) that appear after launch.
# Output: a timestamped runtime-<UTC>.csv plus sibling raw samples by default.
# Set RUNTIME_OUTPUT explicitly for a different immutable run-specific path.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/measurements"
mkdir -p "$OUT"
if [[ $(uname -s) != Darwin ]]; then
  echo "runtime-sample.sh currently supports the macOS dashboard experiment only" >&2
  exit 2
fi
CSV="${RUNTIME_OUTPUT:-$OUT/runtime-$(date -u +%Y%m%dT%H%M%SZ).csv}"
[[ "$CSV" == /* ]] || CSV="$ROOT/$CSV"
SAMPLES="${CSV%.csv}-samples.csv"
mkdir -p "$(dirname "$CSV")"
CSV_TMP=$(mktemp "${CSV}.tmp.XXXXXX")
SAMPLES_TMP=$(mktemp "${SAMPLES}.tmp.XXXXXX")
current_pid=""
cleanup() {
  if [[ -n "$current_pid" ]] && kill -0 "$current_pid" 2>/dev/null; then
    kill "$current_pid" 2>/dev/null || true
  fi
  [[ -z "$CSV_TMP" ]] || rm -f "$CSV_TMP"
  [[ -z "$SAMPLES_TMP" ]] || rm -f "$SAMPLES_TMP"
}
trap cleanup EXIT
trap 'exit 130' HUP INT TERM
echo "app,avg_cpu_pct,peak_cpu_pct,max_rss_mib,helper_procs" > "$CSV_TMP"
echo "app,sample_index,main_pid,helper_pids,cpu_pct,rss_kib,rss_mib" > "$SAMPLES_TMP"

apps=("$@")
using_defaults=0
if [ ${#apps[@]} -eq 0 ]; then
  apps=("$ROOT"/apps/*-dash)
  using_defaults=1
fi
normalized_apps=()
for app in "${apps[@]}"; do
  [[ "$app" == /* ]] || app="$ROOT/$app"
  normalized_apps+=("$app")
done
apps=("${normalized_apps[@]}")
if ((using_defaults && ${#apps[@]} != 7)); then
  echo "expected seven dashboard apps, found ${#apps[@]}" >&2
  exit 1
fi

webkit_pids() { pgrep -f 'com.apple.WebKit' 2>/dev/null | sort -n; }

measurement_failed=0
for app in "${apps[@]}"; do
  name="$(basename "$app")"
  crate="$app"
  [ -f "$app/src-tauri/Cargo.toml" ] && crate="$app/src-tauri"
  if [ ! -f "$crate/Cargo.toml" ]; then
    echo "!! $name: no Cargo.toml found"
    echo "$name,NO_MANIFEST,,," >> "$CSV_TMP"
    measurement_failed=1
    continue
  fi
  package=$(sed -n 's/^name = "\([^"]*\)"/\1/p' "$crate/Cargo.toml" | head -1)
  if [ -z "$package" ]; then
    echo "!! $name: Cargo package name not found"
    echo "$name,NO_PACKAGE,,," >> "$CSV_TMP"
    measurement_failed=1
    continue
  fi
  bin="$crate/target/release/$package"
  if [ ! -x "$bin" ]; then
    echo "!! $name: no release binary (build first)"
    echo "$name,NO_BINARY,,," >> "$CSV_TMP"
    measurement_failed=1
    continue
  fi
  echo "=== $name ==="
  before_wk=$(webkit_pids)
  "$bin" > "$OUT/$name-runtime.log" 2>&1 &
  main=$!
  current_pid=$main
  sleep 5
  if ! kill -0 "$main" 2>/dev/null; then
    echo "!! $name: died during warmup"
    echo "$name,DIED,,," >> "$CSV_TMP"
    current_pid=""
    measurement_failed=1
    continue
  fi
  # helper processes that appeared after launch (webview content/networking/gpu)
  after_wk=$(webkit_pids)
  helpers=$(comm -13 <(echo "$before_wk") <(echo "$after_wk") | tr '\n' ' ')
  nhelp=$(echo $helpers | wc -w | tr -d ' ')

  sum_cpu=0; peak=0; max_rss=0; n=0
  for i in $(seq 1 30); do
    kill -0 "$main" 2>/dev/null || break
    cpu=0; rss=0
    for pid in $main $helpers; do
      line=$(ps -o %cpu=,rss= -p "$pid" 2>/dev/null | head -1)
      [ -z "$line" ] && continue
      c=$(echo "$line" | awk '{print $1}'); r=$(echo "$line" | awk '{print $2}')
      cpu=$(echo "$cpu $c" | awk '{print $1+$2}')
      rss=$((rss + r))
    done
    rss_mib=$(echo "$rss" | awk '{printf "%.3f", $1/1024}')
    helper_csv=$(echo "$helpers" | awk '{$1=$1; gsub(/ /,";"); print}')
    echo "$name,$i,$main,$helper_csv,$cpu,$rss,$rss_mib" >> "$SAMPLES_TMP"
    sum_cpu=$(echo "$sum_cpu $cpu" | awk '{print $1+$2}')
    peak=$(echo "$peak $cpu" | awk '{if ($2>$1) print $2; else print $1}')
    [ "$rss" -gt "$max_rss" ] && max_rss=$rss
    n=$((n+1))
    sleep 1
  done
  kill "$main" 2>/dev/null || true
  sleep 1
  kill -9 "$main" 2>/dev/null || true
  wait "$main" 2>/dev/null || true
  current_pid=""

  if [ "$n" -ne 30 ]; then
    echo "$name,DIED,,," >> "$CSV_TMP"
    measurement_failed=1
    continue
  fi
  avg=$(echo "$sum_cpu $n" | awk '{printf "%.1f", $1/$2}')
  rss_mib=$(echo "$max_rss" | awk '{printf "%.0f", $1/1024}')
  echo "$name,$avg,$peak,$rss_mib,$nhelp" >> "$CSV_TMP"
  echo "    avg_cpu=${avg}% peak=${peak}% max_rss=${rss_mib}MiB helpers=$nhelp"
done

if ((measurement_failed)); then
  echo "runtime sampling failed; no result CSV was published" >&2
  exit 1
fi
mv "$CSV_TMP" "$CSV"
CSV_TMP=""
mv "$SAMPLES_TMP" "$SAMPLES"
SAMPLES_TMP=""
echo; echo "Results: $CSV"
echo "Raw samples: $SAMPLES"
echo "note: %cpu is per-core (can exceed 100); webview helper attribution is"
echo "best-effort (new com.apple.WebKit processes during the window)."
cleanup
trap - EXIT HUP INT TERM
