#!/usr/bin/env bash
# Reproducible macOS packaging audit for the nine SPEC-1 todo applications.
# The runner is serial and publishes evidence only inside an initialized cohort.
set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
COHORT=""
ARTIFACT_ROOT=""
SKIP_BUILD=0
REPLACE=0
STARTED_AT=$(date -u +%FT%TZ)

usage() {
  cat <<'EOF'
Usage: scripts/package-macos.sh --cohort DIR [options]

  --artifact-dir DIR   App/DMG destination (default: dist/cohorts/<cohort-id>)
  --skip-build         Audit already-built cargo-bundle/Tauri bundle outputs
  --replace            Replace this cohort's prior packaging result/artifacts
  -h, --help           Show this help

Requires macOS, rustc 1.96.1, cargo-bundle 0.11.0, and tauri-cli 2.11.4.
The eight non-Tauri apps use cargo-bundle; Tauri uses its native bundler.
Every final DMG is made from the ad-hoc-signed app with bounded hdiutil retries.
EOF
}

while (($#)); do
  case "$1" in
    --cohort) COHORT=${2:-}; shift 2 ;;
    --artifact-dir) ARTIFACT_ROOT=${2:-}; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --replace) REPLACE=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

[[ $(uname -s) == Darwin ]] || { echo "package-macos.sh requires macOS" >&2; exit 2; }
[[ -n "$COHORT" ]] || { echo "--cohort is required" >&2; exit 2; }
[[ "$COHORT" == /* ]] || COHORT="$ROOT/$COHORT"
[[ -f "$COHORT/cohort.json" ]] || {
  echo "--cohort must name a directory initialized by scripts/cohort.py" >&2
  exit 2
}

export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-1.96.1}"
case "$(rustc --version)" in
  "rustc 1.96.1 "*) ;;
  *) echo "package-macos.sh requires rustc 1.96.1" >&2; exit 2 ;;
esac
[[ $(cargo bundle --version 2>/dev/null) == "cargo-bundle v0.11.0" ]] || {
  echo "cargo-bundle 0.11.0 is required" >&2
  exit 2
}
[[ $(cargo tauri --version 2>/dev/null) == "tauri-cli 2.11.4" ]] || {
  echo "tauri-cli 2.11.4 is required" >&2
  exit 2
}
for tool in codesign hdiutil spctl ditto /usr/libexec/PlistBuddy; do
  [[ -x "$tool" ]] || command -v "$tool" >/dev/null 2>&1 || {
    echo "required packaging tool is unavailable: $tool" >&2
    exit 2
  }
done

COHORT_ID=$(basename "$COHORT")
if [[ -z "$ARTIFACT_ROOT" ]]; then
  ARTIFACT_ROOT="$ROOT/dist/cohorts/$COHORT_ID"
elif [[ "$ARTIFACT_ROOT" != /* ]]; then
  ARTIFACT_ROOT="$ROOT/$ARTIFACT_ROOT"
fi
PACKAGING_DIR="$COHORT/packaging"
LOG_DIR="$PACKAGING_DIR/logs"
CSV="$PACKAGING_DIR/results.csv"

if [[ -e "$CSV" || -e "$ARTIFACT_ROOT" ]]; then
  if ((REPLACE)); then
    case "$ARTIFACT_ROOT" in
      "$ROOT"/dist/cohorts/*) rm -rf "$ARTIFACT_ROOT" ;;
      *) echo "refusing --replace outside dist/cohorts/: $ARTIFACT_ROOT" >&2; exit 2 ;;
    esac
  else
    echo "packaging output already exists; use a new cohort or --replace" >&2
    exit 2
  fi
fi
mkdir -p "$LOG_DIR" "$ARTIFACT_ROOT"
CSV_TMP=$(mktemp "$PACKAGING_DIR/.results.csv.tmp.XXXXXX")
TMP_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/gui-packaging.XXXXXX")

cleanup() {
  [[ -z "${CSV_TMP:-}" ]] || rm -f "$CSV_TMP"
  if [[ -n "${TMP_ROOT:-}" && -d "$TMP_ROOT" ]]; then
    rm -rf "$TMP_ROOT"
  fi
}
trap cleanup EXIT
trap 'exit 130' HUP INT TERM

csv_row() {
  python3 - "$@" <<'PY'
import csv
import sys
csv.writer(sys.stdout, lineterminator="\n").writerow(sys.argv[1:])
PY
}

csv_row \
  app tool bundle_status bundle_secs app_bytes dmg_bytes plist_ok \
  preseal_launch_8s codesign_ok postseal_launch_8s builtin_dmg_status \
  hdiutil_attempts hdiutil_verify mounted_bundle_codesign spctl_result \
  app_artifact dmg_artifact log > "$CSV_TMP"

apps=(
  iced-app egui-app gpui-app tauri-app xilem-app slint-app dioxus-app
  freya-app vizia-app floem-app
)

product_name() {
  case "$1" in
    iced-app) echo "Iced Tasks" ;;
    egui-app) echo "Egui Tasks" ;;
    gpui-app) echo "Gpui Tasks" ;;
    tauri-app) echo "tauri-app" ;;
    xilem-app) echo "Xilem Tasks" ;;
    slint-app) echo "Slint Tasks" ;;
    dioxus-app) echo "Dioxus Tasks" ;;
    freya-app) echo "Freya Tasks" ;;
    vizia-app) echo "Vizia Tasks" ;;
    floem-app) echo "Floem Tasks" ;;
  esac
}

launch_bundle() {
  local bundle=$1 log=$2 executable pid alive=no
  executable=$(find "$bundle/Contents/MacOS" -maxdepth 1 -type f -perm -111 | head -1)
  [[ -n "$executable" ]] || { echo "no bundle executable" >> "$log"; echo no; return; }
  "$executable" >> "$log" 2>&1 &
  pid=$!
  sleep 8
  if kill -0 "$pid" 2>/dev/null; then alive=yes; fi
  kill "$pid" 2>/dev/null || true
  sleep 1
  kill -9 "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  echo "$alive"
}

failures=0
for app in "${apps[@]}"; do
  echo "=== package $app ==="
  app_dir="$ROOT/apps/$app"
  product=$(product_name "$app")
  expected_identifier="org.rcn.${app%-app}app"
  log="$LOG_DIR/$app.log"
  : > "$log"
  tool=cargo-bundle
  bundle_status=failed
  bundle_secs=""
  app_bytes=""
  dmg_bytes=""
  plist_ok=no
  preseal_launch=no
  codesign_ok=no
  postseal_launch=no
  builtin_dmg=not_attempted
  hdiutil_attempts=0
  hdiutil_verify=no
  mounted_codesign=no
  spctl_result=not_run
  app_artifact=""
  dmg_artifact=""
  source_app=""

  if [[ ! -f "$app_dir/Cargo.toml" ]]; then
    echo "missing manifest: $app_dir/Cargo.toml" >> "$log"
  elif [[ "$app" == tauri-app ]]; then
    tool=tauri-cli
    if ((SKIP_BUILD)); then
      builtin_dmg=reused
    else
      (
        cd "$app_dir" || exit
        cargo build --locked --release || exit
        /usr/bin/time -p cargo tauri build --ci --bundles app,dmg --no-sign
      ) >> "$log" 2>&1
      build_exit=$?
      bundle_secs=$(awk '$1 == "real" { value=$2 } END { print value }' "$log")
      if ((build_exit == 0)); then builtin_dmg=passed; else builtin_dmg=failed; fi
    fi
    source_app=$(find "$app_dir/target/release/bundle/macos" -maxdepth 1 -name '*.app' -type d 2>/dev/null | head -1)
  else
    if ((SKIP_BUILD)); then
      builtin_dmg=reused
    else
      (
        cd "$app_dir" || exit
        cargo build --locked --release || exit
        /usr/bin/time -p cargo bundle --release --format osx
      ) >> "$log" 2>&1
      build_exit=$?
      bundle_secs=$(awk '$1 == "real" { value=$2 } END { print value }' "$log")
      if ((build_exit == 0)); then
        (
          cd "$app_dir" || exit
          cargo bundle --release --format dmg
        ) >> "$log" 2>&1
        if (($? == 0)); then builtin_dmg=passed; else builtin_dmg=failed; fi
      fi
    fi
    source_app=$(find "$app_dir/target/release/bundle/osx" -maxdepth 1 -name '*.app' -type d 2>/dev/null | head -1)
  fi

  final_dir="$ARTIFACT_ROOT/${app%-app}"
  final_app="$final_dir/$product.app"
  final_dmg="$final_dir/$product.dmg"
  mkdir -p "$final_dir"
  if [[ -n "$source_app" && -d "$source_app" ]]; then
    preseal_launch=$(launch_bundle "$source_app" "$log")
    if ditto "$source_app" "$final_app" >> "$log" 2>&1; then
      plist="$final_app/Contents/Info.plist"
      identifier=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist" 2>> "$log" || true)
      executable_name=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$plist" 2>> "$log" || true)
      if [[ "$identifier" == "$expected_identifier" && -n "$executable_name" &&
            -x "$final_app/Contents/MacOS/$executable_name" ]]; then
        plist_ok=yes
      else
        echo "plist mismatch: identifier=$identifier expected=$expected_identifier executable=$executable_name" >> "$log"
      fi
      if codesign --force --deep --sign - "$final_app" >> "$log" 2>&1 &&
         codesign --verify --deep --strict --verbose=2 "$final_app" >> "$log" 2>&1; then
        codesign_ok=yes
      fi
      postseal_launch=$(launch_bundle "$final_app" "$log")

      for attempt in 1 2 3; do
        hdiutil_attempts=$attempt
        rm -f "$final_dmg"
        if hdiutil create -volname "$product" -srcfolder "$final_app" \
          -ov -format UDZO "$final_dmg" >> "$log" 2>&1; then
          break
        fi
      done
      if [[ -f "$final_dmg" ]] && hdiutil verify "$final_dmg" >> "$log" 2>&1; then
        hdiutil_verify=yes
      fi

      mountpoint="$TMP_ROOT/mount-${app%-app}"
      mkdir -p "$mountpoint"
      if [[ "$hdiutil_verify" == yes ]] &&
         hdiutil attach -nobrowse -readonly -mountpoint "$mountpoint" "$final_dmg" >> "$log" 2>&1; then
        mounted_app=$(find "$mountpoint" -maxdepth 1 -name '*.app' -type d | head -1)
        if [[ -n "$mounted_app" ]] &&
           codesign --verify --deep --strict --verbose=2 "$mounted_app" >> "$log" 2>&1; then
          mounted_codesign=yes
        fi
        hdiutil detach "$mountpoint" >> "$log" 2>&1 || true
      fi

      if spctl --assess --type execute --verbose=2 "$final_app" >> "$log" 2>&1; then
        spctl_result=accepted_unexpected
      else
        spctl_result=rejected_expected_without_developer_id
      fi
      app_bytes=$(( $(du -sk "$final_app" | awk '{print $1}') * 1024 ))
      [[ -f "$final_dmg" ]] && dmg_bytes=$(stat -f %z "$final_dmg")
      app_artifact=${final_app#"$ROOT"/}
      dmg_artifact=${final_dmg#"$ROOT"/}
    fi
  else
    echo "no built app bundle found" >> "$log"
  fi

  if [[ "$plist_ok" == yes && "$preseal_launch" == yes && "$codesign_ok" == yes &&
        "$postseal_launch" == yes && "$hdiutil_verify" == yes && "$mounted_codesign" == yes ]]; then
    bundle_status=passed
  else
    failures=$((failures + 1))
  fi
  log_record=${log#"$ROOT"/}
  csv_row \
    "$app" "$tool" "$bundle_status" "$bundle_secs" "$app_bytes" "$dmg_bytes" \
    "$plist_ok" "$preseal_launch" "$codesign_ok" "$postseal_launch" "$builtin_dmg" \
    "$hdiutil_attempts" "$hdiutil_verify" "$mounted_codesign" "$spctl_result" \
    "$app_artifact" "$dmg_artifact" "$log_record" >> "$CSV_TMP"
done

mv "$CSV_TMP" "$CSV"
CSV_TMP=""
python3 "$ROOT/scripts/cohort.py" record \
  --cohort "$COHORT" \
  --key packaging \
  --artifact "$CSV" \
  --started "$STARTED_AT" \
  --command "RUSTUP_TOOLCHAIN=$RUSTUP_TOOLCHAIN ./scripts/package-macos.sh --cohort ${COHORT#"$ROOT"/}"

echo "wrote $CSV"
if ((failures)); then
  echo "packaging audit failed for $failures app(s); results retained in the in-progress cohort" >&2
  exit 1
fi
