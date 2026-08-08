#!/usr/bin/env bash
# Build and launch the exact 17-app Linux/Xvfb reality-check cohort serially.
set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
COHORT=""
IMAGE="gui-ecosystem-research:rust-1.96-bookworm"
PLATFORM="linux/arm64"
SKIP_IMAGE_BUILD=0
REPLACE=0
STARTED_AT=$(date -u +%FT%TZ)

usage() {
  cat <<'EOF'
Usage: linux/run-cohort.sh --cohort DIR [options]

  --image NAME          Docker image tag (default: gui-ecosystem-research:rust-1.96-bookworm)
  --platform PLATFORM   Docker platform (default: linux/arm64)
  --skip-image-build    Reuse an already-built image
  --replace             Replace this cohort's Linux artifacts and target volume
  -h, --help            Show this help

The source tree is mounted read-only. Cargo registry state is warm/shared, but
the cohort gets a fresh target volume. Apps and variants run strictly serially.
Default renderer results are retained even when a known workaround is probed.
EOF
}

while (($#)); do
  case "$1" in
    --cohort) COHORT=${2:-}; shift 2 ;;
    --image) IMAGE=${2:-}; shift 2 ;;
    --platform) PLATFORM=${2:-}; shift 2 ;;
    --skip-image-build) SKIP_IMAGE_BUILD=1; shift ;;
    --replace) REPLACE=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

[[ -n "$COHORT" ]] || { echo "--cohort is required" >&2; exit 2; }
[[ "$COHORT" == /* ]] || COHORT="$ROOT/$COHORT"
[[ -f "$COHORT/cohort.json" ]] || {
  echo "--cohort must name a directory initialized by scripts/cohort.py" >&2
  exit 2
}
command -v docker >/dev/null 2>&1 || { echo "docker is required" >&2; exit 2; }

apps=(
  iced-app egui-app gpui-app tauri-app xilem-app slint-app dioxus-app
  freya-app vizia-app floem-app
  iced-tray egui-tray freya-tray vizia-tray floem-tray
  iced-babel gpui-babel freya-babel vizia-babel floem-babel
)
for app in "${apps[@]}"; do
  [[ -f "$ROOT/apps/$app/Cargo.toml" ]] || {
    echo "missing Linux cohort app: apps/$app" >&2
    exit 2
  }
done

COHORT_ID=$(basename "$COHORT")
VOLUME_ID=$(printf '%s' "$COHORT_ID" | tr '[:upper:]' '[:lower:]' | tr -c 'a-z0-9_.-' '-')
TARGET_VOLUME="gui-ecosystem-target-$VOLUME_ID"
CARGO_VOLUME="gui-ecosystem-cargo-home-rust-1.96"
LINUX_DIR="$COHORT/linux"
CSV="$LINUX_DIR/results.csv"

if [[ -e "$LINUX_DIR" ]] || docker volume inspect "$TARGET_VOLUME" >/dev/null 2>&1; then
  if ((REPLACE)); then
    case "$LINUX_DIR" in
      "$COHORT"/linux) rm -rf "$LINUX_DIR" ;;
      *) echo "refusing to replace unexpected Linux output: $LINUX_DIR" >&2; exit 2 ;;
    esac
    docker volume rm "$TARGET_VOLUME" >/dev/null 2>&1 || true
  else
    echo "Linux cohort output/target already exists; use a new cohort or --replace" >&2
    exit 2
  fi
fi
mkdir -p "$LINUX_DIR"

if ((!SKIP_IMAGE_BUILD)); then
  if ! docker build --platform "$PLATFORM" -f "$ROOT/linux/Dockerfile" -t "$IMAGE" "$ROOT/linux" \
    > "$LINUX_DIR/image-build.log" 2>&1; then
    echo "Linux image build failed; see $LINUX_DIR/image-build.log" >&2
    exit 1
  fi
fi
docker image inspect "$IMAGE" --format 'image_id={{.Id}} repo_digests={{join .RepoDigests ","}}' \
  > "$LINUX_DIR/image.txt"
docker run --rm --platform "$PLATFORM" "$IMAGE" sh -c 'cat /env/versions.txt' \
  > "$LINUX_DIR/environment.txt"
{
  echo "platform=$PLATFORM"
  echo "docker=$(docker --version)"
  echo "dockerfile_base=rust:1.96-bookworm@sha256:a339861ae23e9abb272cea45dfafde21760d2ce6577a70f8a926153677902663"
  echo "xvfb=1280x800x24; X11; no window manager, compositor, tray host, or portals"
} >> "$LINUX_DIR/environment.txt"

docker volume create "$TARGET_VOLUME" >/dev/null
docker volume create "$CARGO_VOLUME" >/dev/null

run_variant() {
  local app=$1 variant=$2 skip_build=$3
  shift 3
  echo "=== linux $app [$variant] ==="
  docker run --rm --platform "$PLATFORM" \
    -e LINUX_OUTPUT_DIR=/cohort-output \
    -e RUN_VARIANT="$variant" \
    -e LINUX_SKIP_BUILD="$skip_build" \
    -e CARGO_BUILD_JOBS=2 \
    -v "$ROOT:/work:ro" \
    -v "$LINUX_DIR:/cohort-output" \
    -v "$TARGET_VOLUME:/cargo-target" \
    -v "$CARGO_VOLUME:/cargo-home" \
    "$IMAGE" /work/linux/run-app.sh "$app" "$@" \
    > "$LINUX_DIR/$app-$variant-driver.log" 2>&1
  return $?
}

infrastructure_failures=0
for app in "${apps[@]}"; do
  run_variant "$app" default 0 || true
  default_result="$LINUX_DIR/runs/$app/default/result.tsv"
  if [[ ! -f "$default_result" ]]; then
    echo "missing result from $app default probe" >&2
    infrastructure_failures=$((infrastructure_failures + 1))
    continue
  fi
  default_alive=$(awk -F '\t' 'NR == 2 {print $4}' "$default_result")
  if [[ "$default_alive" != yes ]]; then
    case "$app" in
      iced-app|iced-tray|iced-babel)
        run_variant "$app" tiny-skia 1 ICED_BACKEND=tiny-skia || true
        ;;
      xilem-app)
        run_variant "$app" wgpu-gl 1 WGPU_BACKEND=gl || true
        ;;
    esac
  fi
done

if ((infrastructure_failures)); then
  echo "$infrastructure_failures app probes produced no result; aggregate not published" >&2
  exit 1
fi

python3 - "$ROOT" "$LINUX_DIR" "$CSV" "${apps[@]}" <<'PY'
import csv
import sys
from pathlib import Path

root = Path(sys.argv[1]).resolve()
linux_dir = Path(sys.argv[2]).resolve()
output = Path(sys.argv[3])
apps = sys.argv[4:]

def read_result(path: Path):
    if not path.is_file():
        return None
    with path.open(newline="") as handle:
        return next(csv.DictReader(handle, delimiter="\t"))

def relative(path: Path) -> str:
    return path.resolve().relative_to(root).as_posix()

fieldnames = [
    "app", "compile_ok", "run_alive_10s_default", "renderer_path_default",
    "workaround_variant", "workaround_alive_10s", "screenshot_ok",
    "default_result", "workaround_result", "notes",
]
temporary = output.with_name(".results.csv.tmp")
with temporary.open("w", newline="") as handle:
    writer = csv.DictWriter(handle, fieldnames=fieldnames, lineterminator="\n")
    writer.writeheader()
    for app in apps:
        default_path = linux_dir / "runs" / app / "default" / "result.tsv"
        default = read_result(default_path)
        if default is None:
            raise SystemExit(f"missing default result for {app}")
        variants = sorted((linux_dir / "runs" / app).glob("*/result.tsv"))
        workaround_path = next((path for path in variants if path != default_path), None)
        workaround = read_result(workaround_path) if workaround_path else None
        screenshot_ok = "yes" if (
            default["screenshot_ok"] == "yes"
            or (workaround and workaround["screenshot_ok"] == "yes")
        ) else "no"
        writer.writerow({
            "app": app,
            "compile_ok": "yes" if default["compile_ok"] in {"yes", "reused"} else "no",
            "run_alive_10s_default": default["run_alive_10s"],
            "renderer_path_default": default["renderer_path"],
            "workaround_variant": workaround["variant"] if workaround else "none",
            "workaround_alive_10s": workaround["run_alive_10s"] if workaround else "not_run",
            "screenshot_ok": screenshot_ok,
            "default_result": relative(default_path),
            "workaround_result": relative(workaround_path) if workaround_path else "",
            "notes": (
                f"default_exit={default['exit_code']}; default_env={default['env_overrides'] or 'none'}; "
                + (
                    f"workaround_exit={workaround['exit_code']}; workaround_env={workaround['env_overrides']}"
                    if workaround else "no workaround defined or required"
                )
            ),
        })
temporary.replace(output)
PY

python3 "$ROOT/scripts/cohort.py" record \
  --cohort "$COHORT" \
  --key linux \
  --artifact "$CSV" \
  --started "$STARTED_AT" \
  --command "./linux/run-cohort.sh --cohort ${COHORT#"$ROOT"/} --platform $PLATFORM"

echo "wrote $CSV"
