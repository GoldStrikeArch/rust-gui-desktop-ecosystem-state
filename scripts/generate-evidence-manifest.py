#!/usr/bin/env python3
"""Generate checksums and reconciliation metadata for retained evidence.

The original experiments did not record source hashes or commands at launch
time. Input hashes and reproduction commands emitted here describe the current
reconciled tree and tooling, not fabricated historical provenance.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import io
from datetime import datetime
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MEASUREMENTS = ROOT / "measurements"
OUTPUT = MEASUREMENTS / "artifact-manifest.tsv"
MACHINE = "Apple M4 Pro; 24 GiB; macOS 26.5.2 (25F84); rustc/cargo 1.96.1"
RUN_DATES = {
    "iter1": "2026-07-07",
    "iter2": "2026-07-07",
    "iter3": "2026-07-09",
    "iter4": "2026-07-10",
}
EXPECTED_ROUND_COUNTS = {"iter1": 7, "iter2": 14, "iter3": 14, "iter4": 21}
IGNORED_INPUT_SUFFIXES = {".log", ".md"}
IGNORED_INPUT_NAMES = {"Cargo.lock", "screenshot.png"}
GENERATED_IMAGE_MARKERS = (
    "screenshot",
    "snapshot",
    "shot-",
    "fetch-",
    "grid-window",
    "peek-window",
    "launch-idle",
    "verify-",
)

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument(
    "--check",
    action="store_true",
    help="fail without writing if the retained manifest is out of date",
)
args = parser.parse_args()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def required(path: Path) -> Path:
    if not path.is_file():
        raise SystemExit(f"required evidence is missing: {relative(path)}")
    return path


def app_input_hash(app_name: str) -> str:
    app_dir = ROOT / "apps" / app_name
    digest = hashlib.sha256()
    roots = [app_dir]
    if app_name.endswith("-babel"):
        roots.append(ROOT / "apps/babel-assets")
    if app_name.endswith("-peek"):
        roots.append(ROOT / "apps/peek-assets")
    files = []
    for tree in roots:
        files.extend(
            path
            for path in tree.rglob("*")
            if path.is_file()
            and "target" not in path.parts
            and path.name not in IGNORED_INPUT_NAMES
            and path.suffix.lower() not in IGNORED_INPUT_SUFFIXES
            and path.suffix.lower() != ".ppm"
            and not (
                path.suffix.lower() in {".png", ".jpg", ".jpeg"}
                and any(marker in path.name.lower() for marker in GENERATED_IMAGE_MARKERS)
            )
        )
    for path in sorted(files):
        identity = path.relative_to(ROOT).as_posix().encode()
        digest.update(len(identity).to_bytes(4, "big"))
        digest.update(identity)
        digest.update(bytes.fromhex(sha256(path)))
    return digest.hexdigest()


def relative(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


IDENTITIES: dict[str, tuple[str, str]] = {}


def app_identity(app_name: str) -> tuple[str, str]:
    if app_name in IDENTITIES:
        return IDENTITIES[app_name]
    app_dir = ROOT / "apps" / app_name
    lock = app_dir / "Cargo.lock"
    identity = app_input_hash(app_name), sha256(required(lock))
    IDENTITIES[app_name] = identity
    return identity


def assert_app_set(records: list[dict[str, str]], expected: set[str], label: str) -> None:
    apps = [record["app"] for record in records]
    if len(apps) != len(set(apps)):
        raise SystemExit(f"{label}: duplicate app rows")
    if set(apps) != expected:
        missing = sorted(expected - set(apps))
        extra = sorted(set(apps) - expected)
        raise SystemExit(f"{label}: app-set mismatch; missing={missing}, extra={extra}")


fieldnames = [
    "record_type",
    "round",
    "app",
    "run_date_utc",
    "reproduction_command",
    "machine",
    "current_input_hash_sha256",
    "lockfile_sha256",
    "artifact",
    "artifact_sha256",
    "verification_level",
    "notes",
]
rows: list[dict[str, str]] = []
round_records: dict[str, list[dict[str, str]]] = {}

for round_name in ("iter1", "iter2", "iter3", "iter4"):
    result = required(MEASUREMENTS / f"results-{round_name}.csv")
    result_hash = sha256(result)
    with result.open(newline="") as handle:
        records = list(csv.DictReader(handle))
    expected_count = EXPECTED_ROUND_COUNTS[round_name]
    apps = [record["app"] for record in records]
    if len(records) != expected_count or len(set(apps)) != expected_count:
        raise SystemExit(
            f"{relative(result)}: expected {expected_count} unique app rows, got {len(records)}"
        )
    for record in records:
        if record["clean_build_secs"] == "BUILD_FAILED":
            raise SystemExit(f"{relative(result)}: {record['app']} records a failed build")
        if record["process_survived_8s"] != "yes":
            raise SystemExit(f"{relative(result)}: {record['app']} did not survive eight seconds")
    round_records[round_name] = records
    for record in records:
            app = record["app"]
            input_hash, lock_hash = app_identity(app)
            level = "compiled"
            if record["process_survived_8s"] == "yes":
                level += ";process-survived-8s"
            rows.append(
                {
                    "record_type": "build-benchmark",
                    "round": round_name,
                    "app": app,
                    "run_date_utc": RUN_DATES[round_name],
                    "reproduction_command": f"./measure.sh --round {round_name}",
                    "machine": MACHINE,
                    "current_input_hash_sha256": input_hash,
                    "lockfile_sha256": lock_hash,
                    "artifact": relative(result),
                    "artifact_sha256": result_hash,
                    "verification_level": level,
                    "notes": (
                        "Reproduction command and input hash describe the reconciled current "
                        "tooling/tree, not retained run-time provenance."
                    ),
                }
            )

all_apps = {record["app"] for records in round_records.values() for record in records}
if len(all_apps) != 56:
    raise SystemExit(f"canonical round union: expected 56 apps, got {len(all_apps)}")

runtime = required(MEASUREMENTS / "runtime.csv")
with runtime.open(newline="") as handle:
    runtime_records = list(csv.DictReader(handle))
expected_runtime_apps = {
    record["app"] for record in round_records["iter2"] if record["app"].endswith("-dash")
}
assert_app_set(runtime_records, expected_runtime_apps, relative(runtime))
for record in runtime_records:
        app = record["app"]
        input_hash, lock_hash = app_identity(app)
        rows.append(
            {
                "record_type": "runtime-summary",
                "round": "iter2",
                "app": app,
                "run_date_utc": "2026-07-07",
                "reproduction_command": "./scripts/runtime-sample.sh",
                "machine": MACHINE,
                "current_input_hash_sha256": input_hash,
                "lockfile_sha256": lock_hash,
                "artifact": relative(runtime),
                "artifact_sha256": sha256(runtime),
                "verification_level": "process-sampled;raw-samples-not-retained",
                "notes": "Original per-second samples and run-time source hashes were not retained.",
            }
        )

verifications = (
    (
        MEASUREMENTS / "verification-iter3/current/results.tsv",
        "iter3",
        "./scripts/verify-iter3.sh --duration 8",
    ),
    (
        MEASUREMENTS / "verification-all/current/results.tsv",
        "all",
        "./scripts/verify-windows.sh --duration 8",
    ),
)
full_verification_records: list[dict[str, str]] = []
for verification, verification_round, verification_command in verifications:
    required(verification)
    with verification.open(newline="") as handle:
        verification_records = list(csv.DictReader(handle, delimiter="\t"))
    expected_apps = (
        {record["app"] for record in round_records["iter3"]}
        if verification_round == "iter3"
        else all_apps
    )
    assert_app_set(verification_records, expected_apps, relative(verification))
    if verification_round == "all":
        full_verification_records = verification_records
    for record in verification_records:
        if record["process_survived_8s"] != "true":
            raise SystemExit(f"{relative(verification)}: {record['app']} did not survive")
        if record["visible_window_observed"] != "true":
            raise SystemExit(f"{relative(verification)}: {record['app']} had no visible window")
        if record["exit_before_cleanup"] != "false":
            raise SystemExit(f"{relative(verification)}: {record['app']} exited early")
        app = record["app"]
        input_hash, lock_hash = app_identity(app)
        log = required(ROOT / record["log"])
        levels = []
        if record["process_survived_8s"] == "true":
            levels.append("process-survived-8s")
        if record["visible_window_observed"] == "true":
            levels.append("window-observed")
        if record["selftest_output_observed"] == "true":
            levels.append("self-test-output")
        rows.append(
            {
                "record_type": "launch-verification",
                "round": verification_round,
                "app": app,
                "run_date_utc": record["timestamp_utc"],
                "reproduction_command": verification_command,
                "machine": MACHINE,
                "current_input_hash_sha256": input_hash,
                "lockfile_sha256": lock_hash,
                "artifact": relative(log),
                "artifact_sha256": sha256(log),
                "verification_level": ";".join(levels),
                "notes": (
                    f"window_id={record['window_id']}; title={record['window_title']}; "
                    "input hash is current reconciliation metadata"
                ),
            }
        )

binary_inventory = required(MEASUREMENTS / "verification-all/current/binary-inventory.tsv")
inventory_hash = sha256(binary_inventory)
with binary_inventory.open(newline="") as handle:
    inventory_records = list(csv.DictReader(handle, delimiter="\t"))
assert_app_set(inventory_records, all_apps, relative(binary_inventory))
audit_start = min(
    datetime.fromisoformat(record["timestamp_utc"].replace("Z", "+00:00"))
    for record in full_verification_records
)
latest_binary_mtime = max(
    datetime.fromisoformat(record["executable_mtime_utc"].replace("Z", "+00:00"))
    for record in inventory_records
)
if latest_binary_mtime >= audit_start:
    raise SystemExit(
        f"{relative(binary_inventory)}: an executable mtime is not earlier than the audit start"
    )
for record in inventory_records:
    if len(record["executable_sha256"]) != 64:
        raise SystemExit(f"{relative(binary_inventory)}: invalid hash for {record['app']}")
    app = record["app"]
    input_hash, lock_hash = app_identity(app)
    rows.append(
        {
            "record_type": "launch-binary-snapshot",
            "round": "all",
            "app": app,
            "run_date_utc": "2026-07-10",
            "reproduction_command": "./scripts/record-binary-inventory.py measurements/verification-all/current/results.tsv",
            "machine": MACHINE,
            "current_input_hash_sha256": input_hash,
            "lockfile_sha256": lock_hash,
            "artifact": relative(binary_inventory),
            "artifact_sha256": inventory_hash,
            "verification_level": "post-run-executable-hash-snapshot",
            "notes": (
                f"executable={record['executable']}; bytes={record['executable_bytes']}; "
                f"mtime={record['executable_mtime_utc']}; sha256={record['executable_sha256']}; "
                "all recorded mtimes predate the audit; not a claim that the final "
                "reconciled source tree produced this binary"
            ),
        }
    )

audit_reruns = (
    (
        "tauri-grid",
        MEASUREMENTS / "verification-iter4-rerun/tauri-grid-20260710.log",
        "GRID_SELFTEST=1 apps/tauri-grid/target/release/tauri-grid",
        "self-test;synthetic-input;fresh-audit-rerun",
    ),
    (
        "tauri-fetch",
        MEASUREMENTS / "verification-iter4-rerun/tauri-fetch-20260710.log",
        "FETCH_SELFTEST=1 apps/tauri-fetch/target/release/tauri-fetch",
        "self-test;network-observed;fresh-audit-rerun",
    ),
)
for app, log, command, level in audit_reruns:
    required(log)
    log_text = log.read_text(errors="replace")
    if "SELFTEST FAIL" in log_text or "SELFTEST TIMEOUT" in log_text:
        raise SystemExit(f"{relative(log)} contains a failed or timed-out self-test")
    expected_done = (
        "SELFTEST DONE pass=14 fail=0"
        if app == "tauri-grid"
        else "SELFTEST DONE pass=10 fail=0"
    )
    if expected_done not in log_text:
        raise SystemExit(f"{relative(log)} lacks {expected_done!r}")
    input_hash, lock_hash = app_identity(app)
    rows.append(
        {
            "record_type": "capability-audit-rerun",
            "round": "iter4",
            "app": app,
            "run_date_utc": "2026-07-10",
            "reproduction_command": command,
            "machine": MACHINE,
            "current_input_hash_sha256": input_hash,
            "lockfile_sha256": lock_hash,
            "artifact": relative(log),
            "artifact_sha256": sha256(log),
            "verification_level": level,
            "notes": "Fresh reconciled-tree audit rerun; not the original collection or timing environment.",
        }
    )

corpus = required(ROOT / "apps/babel-assets/corpus.txt")
screenshots = sorted((ROOT / "apps").glob("*-babel/screenshot.png"))
expected_babel_apps = {
    record["app"] for record in round_records["iter3"] if record["app"].endswith("-babel")
}
if {screenshot.parent.name for screenshot in screenshots} != expected_babel_apps:
    raise SystemExit("Babel screenshot set does not match the seven canonical Babel apps")
for screenshot in screenshots:
    app = screenshot.parent.name
    input_hash, lock_hash = app_identity(app)
    rows.append(
        {
            "record_type": "babel-gallery",
            "round": "iter3",
            "app": app,
            "run_date_utc": "2026-07-09",
            "reproduction_command": "per-app screenshot hook; see FRICTION.md",
            "machine": MACHINE,
            "current_input_hash_sha256": input_hash,
            "lockfile_sha256": lock_hash,
            "artifact": relative(screenshot),
            "artifact_sha256": sha256(screenshot),
            "verification_level": "retained-screenshot",
            "notes": f"corpus_sha256={sha256(corpus)}; raw OCR/caret traces were not all retained",
        }
    )

for app, recorded_identity in IDENTITIES.items():
    lock = ROOT / "apps" / app / "Cargo.lock"
    current_identity = app_input_hash(app), sha256(required(lock))
    if current_identity != recorded_identity:
        raise SystemExit(f"inputs changed while generating the manifest: {app}")

buffer = io.StringIO(newline="")
writer = csv.DictWriter(buffer, fieldnames=fieldnames, delimiter="\t", lineterminator="\n")
writer.writeheader()
writer.writerows(rows)
content = buffer.getvalue()

if args.check:
    if not OUTPUT.exists() or OUTPUT.read_text() != content:
        print(f"drift: {relative(OUTPUT)}")
        raise SystemExit(1)
    print(f"manifest matches retained evidence ({len(rows)} rows)")
else:
    temporary_output = OUTPUT.with_name(f".{OUTPUT.name}.tmp")
    temporary_output.write_text(content)
    temporary_output.replace(OUTPUT)
    print(f"wrote {relative(OUTPUT)} ({len(rows)} rows)")
