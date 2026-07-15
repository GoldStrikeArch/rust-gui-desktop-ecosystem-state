#!/usr/bin/env bash
# Serial, reproducible measurement pass over one experiment round.
# Usage: ./measure.sh [--round iter1|iter2|iter3|iter4|all] [--output PATH] [app-dir ...]
# Defaults: --round iter1, output a timestamped file under measurements/reruns/.
#   iter1 = *-app; iter2 = *-dash + *-board; iter3 = *-tray + *-babel
#   iter4 = *-grid + *-fetch + *-peek
# Run apps SERIALLY (never in parallel) so timings are contention-free.
# This measurement path is macOS-only: it uses BSD stat, Mach-O strip flags,
# and macOS executable/process assumptions. The Linux round has separate tools.
set -eu

ROOT="$(cd "$(dirname "$0")" && pwd)"
CALLER_PWD="$(pwd -P)"
OUT="$ROOT/measurements"
mkdir -p "$OUT"
round="iter1"
csv_arg=""
apps=()
while [ "$#" -gt 0 ]; do
  case "$1" in
    --round)
      [ "$#" -ge 2 ] || { echo "--round requires a value" >&2; exit 2; }
      round="$2"; shift 2 ;;
    --output)
      [ "$#" -ge 2 ] || { echo "--output requires a path" >&2; exit 2; }
      csv_arg="$2"; shift 2 ;;
    -h|--help)
      sed -n '2,6p' "$0"; exit 0 ;;
    --)
      shift; apps+=("$@"); break ;;
    -*)
      echo "unknown option: $1" >&2; exit 2 ;;
    *)
      apps+=("$1"); shift ;;
  esac
done

case "$round" in
  iter1|iter2|iter3|iter4|all) ;;
  *) echo "invalid round '$round' (expected iter1, iter2, iter3, iter4, or all)" >&2; exit 2 ;;
esac

if [ "$(uname -s)" != "Darwin" ]; then
  echo "measure.sh supports macOS only (BSD stat, Mach-O strip, and macOS launch assumptions)." >&2
  echo "Use the separate Linux measurement workflow documented in README.md on other hosts." >&2
  exit 2
fi

using_default_apps=no
if [ ${#apps[@]} -eq 0 ]; then
  using_default_apps=yes
  case "$round" in
    iter1) apps=("$ROOT"/apps/*-app); expected_apps=7 ;;
    iter2) apps=("$ROOT"/apps/*-dash "$ROOT"/apps/*-board); expected_apps=14 ;;
    iter3) apps=("$ROOT"/apps/*-tray "$ROOT"/apps/*-babel); expected_apps=14 ;;
    iter4) apps=("$ROOT"/apps/*-grid "$ROOT"/apps/*-fetch "$ROOT"/apps/*-peek); expected_apps=21 ;;
    all) apps=("$ROOT"/apps/*-app "$ROOT"/apps/*-dash "$ROOT"/apps/*-board "$ROOT"/apps/*-tray "$ROOT"/apps/*-babel "$ROOT"/apps/*-grid "$ROOT"/apps/*-fetch "$ROOT"/apps/*-peek); expected_apps=56 ;;
  esac
fi

# Resolve every custom relative path before the loop changes directories.
normalized_apps=()
for app in "${apps[@]}"; do
  case "$app" in
    /*) app_path="$app" ;;
    *) app_path="$CALLER_PWD/$app" ;;
  esac
  if [ -d "$app_path" ]; then
    app_path="$(cd "$app_path" && pwd -P)"
  fi
  normalized_apps+=("$app_path")
done
apps=("${normalized_apps[@]}")
if [ "$using_default_apps" = yes ] && [ "${#apps[@]}" -ne "$expected_apps" ]; then
  echo "expected $expected_apps apps for $round, found ${#apps[@]}" >&2
  exit 1
fi

if [ -n "$csv_arg" ]; then
  case "$csv_arg" in
    /*) CSV="$csv_arg" ;;
    *) CSV="$ROOT/$csv_arg" ;;
  esac
else
  CSV="$OUT/reruns/$(date -u +%Y%m%dT%H%M%SZ)/results-$round.csv"
fi
csv_dir="$(dirname "$CSV")"
csv_base="$(basename "$CSV")"
mkdir -p "$csv_dir"
CSV_TMP="$(mktemp "$csv_dir/.${csv_base}.tmp.XXXXXX")"
cleanup_csv() {
  if [ -n "${CSV_TMP:-}" ]; then
    rm -f "$CSV_TMP"
  fi
}
trap cleanup_csv EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
echo "app,crate_root,clean_build_secs,incremental_secs,binary_bytes,binary_stripped_bytes,deps_unique,accesskit_in_deps,loc_rust,loc_other_ui,process_survived_8s" > "$CSV_TMP"
measurement_failed=no

for app in "${apps[@]}"; do
  name="$(basename "$app")"
  # Tauri-style layouts may nest the crate in src-tauri/
  if [ -f "$app/Cargo.toml" ]; then
    crate="$app"
  elif [ -f "$app/src-tauri/Cargo.toml" ]; then
    crate="$app/src-tauri"
  else
    echo "!! $name: no Cargo.toml found, skipping"
    measurement_failed=yes
    continue
  fi
  crate_rel=${crate#"$ROOT"/}
  echo "=== $name (crate: $crate) ==="
  if ! cd "$crate"; then
    echo "!! $name: could not enter crate directory $crate"
    measurement_failed=yes
    continue
  fi

  # Resolve the package-named benchmark binary and its exact source/output
  # paths. This excludes auxiliary binaries such as slint-fetch's probe.
  metadata_log="$OUT/$name-metadata.log"
  if ! metadata=$(cargo metadata --locked --no-deps --format-version 1 2> "$metadata_log"); then
    echo "!! $name: cargo metadata FAILED (see $metadata_log)"
    echo "$name,$crate_rel,METADATA_FAILED,,,,,,,," >> "$CSV_TMP"
    measurement_failed=yes
    continue
  fi
  metadata_value() {
    printf '%s' "$metadata" | plutil -extract "$1" raw -o - - 2>> "$metadata_log"
  }
  if ! package_name=$(metadata_value packages.0.name) ||
     ! target_dir=$(metadata_value target_directory); then
    echo "!! $name: could not read package metadata (see $metadata_log)"
    echo "$name,$crate_rel,METADATA_FAILED,,,,,,,," >> "$CSV_TMP"
    measurement_failed=yes
    continue
  fi
  target_index=0
  main_rs=""
  while target_name=$(metadata_value "packages.0.targets.$target_index.name"); do
    target_kind=$(metadata_value "packages.0.targets.$target_index.kind.0" || true)
    if [ "$target_name" = "$package_name" ] && [ "$target_kind" = "bin" ]; then
      main_rs=$(metadata_value "packages.0.targets.$target_index.src_path" || true)
      break
    fi
    target_index=$((target_index + 1))
  done
  if [ -z "$main_rs" ]; then
    echo "!! $name: no package-named binary target '$package_name'"
    echo "$name,$crate_rel,TARGET_FAILED,,,,,,,," >> "$CSV_TMP"
    measurement_failed=yes
    continue
  fi
  bin="$target_dir/release/$package_name"

  # --- clean release build (deps already in ~/.cargo cache; network warm) ---
  cargo clean >/dev/null 2>&1
  if ! /usr/bin/time -p cargo build --locked --release --bin "$package_name" > "$OUT/$name-build.log" 2>&1; then
    echo "!! $name: build FAILED (see $OUT/$name-build.log)"
    echo "$name,$crate_rel,BUILD_FAILED,,,,,,,," >> "$CSV_TMP"
    measurement_failed=yes
    continue
  fi
  clean_raw=$(awk '$1 == "real" { value=$2 } END { print value }' "$OUT/$name-build.log")
  clean_secs=$(awk -v value="${clean_raw:-0}" 'BEGIN { printf "%.0f", value }')

  # --- incremental rebuild after touching main.rs ---
  touch "$main_rs"
  if /usr/bin/time -p cargo build --locked --release --bin "$package_name" > /dev/null 2> "$OUT/$name-incremental.log"; then
    incr_raw=$(awk '$1 == "real" { value=$2 } END { print value }' "$OUT/$name-incremental.log")
    incr_secs=$(awk -v value="${incr_raw:-0}" 'BEGIN { printf "%.0f", value }')
  else
    incr_raw=$(awk '$1 == "real" { value=$2 } END { print value }' "$OUT/$name-incremental.log")
    incr_secs="INCREMENTAL_FAILED"
    measurement_failed=yes
    echo "!! $name: incremental build FAILED after ${incr_raw:-unknown}s (see $OUT/$name-incremental.log)"
  fi

  # --- binary size (the package-named benchmark target, never a helper bin) ---
  if [ -x "$bin" ]; then
    size=$(stat -f %z "$bin")
    cp "$bin" "$OUT/$name-bin-stripped"
    strip -Sx "$OUT/$name-bin-stripped" 2>/dev/null
    size_stripped=$(stat -f %z "$OUT/$name-bin-stripped")
    rm -f "$OUT/$name-bin-stripped"
  else
    size=""; size_stripped=""
    measurement_failed=yes
    echo "!! $name: expected benchmark binary not found at $bin"
  fi

  # --- dependency stats ---
  if cargo tree --locked --package "$package_name" --edges normal --prefix none \
      > "$OUT/$name-deps-tree.txt" 2> "$OUT/$name-deps-tree-error.log"; then
    sed 's/ (\*)$//' "$OUT/$name-deps-tree.txt" | sort -u > "$OUT/$name-deps-flat.txt"
    deps=$(awk '{print $1}' "$OUT/$name-deps-flat.txt" | sort -u | wc -l | tr -d ' ')
    ak=$(grep -c '^accesskit' "$OUT/$name-deps-flat.txt" || true)
  else
    echo "!! $name: cargo tree FAILED (see $OUT/$name-deps-tree-error.log)"
    : > "$OUT/$name-deps-flat.txt"
    deps=""
    ak=""
    measurement_failed=yes
  fi

  # --- lines of code (Rust + UI files: slint/html/js/css), excluding target ---
  loc_rust=$(find "$app" -name '*.rs' -not -path '*/target/*' -exec cat {} + | wc -l | tr -d ' ')
  loc_other=$(find "$app" \( -name '*.slint' -o -name '*.html' -o -name '*.js' -o -name '*.css' \) -not -path '*/target/*' -exec cat {} + 2>/dev/null | wc -l | tr -d ' ')

  # --- launch check: run 8s, verify still alive, kill ---
  process_survived=no
  if [ -x "$bin" ]; then
    "$bin" > "$OUT/$name-run.log" 2>&1 &
    pid=$!
    sleep 8
    if kill -0 "$pid" 2>/dev/null; then process_survived=yes; fi
    # `|| true`: under set -e, kill on an already-exited pid must not abort the run
    kill "$pid" 2>/dev/null || true; sleep 1; kill -9 "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  if [ "$process_survived" != yes ]; then
    measurement_failed=yes
    echo "!! $name: process did not survive eight seconds"
  fi

  echo "$name,$crate_rel,$clean_secs,$incr_secs,$size,$size_stripped,$deps,$ak,$loc_rust,$loc_other,$process_survived" >> "$CSV_TMP"
  echo "    clean=${clean_raw}s incr=${incr_raw}s bin=${size}B deps=$deps accesskit=$ak process_survived_8s=$process_survived"
done

if [ "$measurement_failed" = yes ]; then
  echo >&2
  echo "Measurement pass had failures; canonical CSV left unchanged: $CSV" >&2
  exit 1
fi

mv -f "$CSV_TMP" "$CSV"
CSV_TMP=""
trap - EXIT HUP INT TERM
echo
echo "Results: $CSV"
