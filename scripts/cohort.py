#!/usr/bin/env python3
"""Create, resolve, record, validate, and promote immutable study cohorts.

The original seven-framework results live directly in ``measurements/`` and
remain a supported legacy input. New runs live in one directory below
``measurements/reruns/``. The optional ``measurements/active-cohort.txt``
contains a path relative to ``measurements/`` and is changed only by the
validated ``promote`` command.
"""

from __future__ import annotations

import argparse
import csv
import json
import math
import os
import platform
import re
import shlex
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.parse import unquote, urlsplit


ROOT = Path(__file__).resolve().parents[1]
MEASUREMENTS = ROOT / "measurements"
ACTIVE_POINTER = MEASUREMENTS / "active-cohort.txt"
FRAMEWORKS = (
    ("iced", "0.14.0"),
    ("egui", "0.35.0"),
    ("gpui", "0.2.2"),
    ("tauri", "2.11.5"),
    ("xilem", "0.4.0"),
    ("slint", "1.17.1"),
    ("dioxus", "0.7.9"),
    ("freya", "0.4.0"),
    ("vizia", "0.4.0"),
    # floem's crates.io release (0.2.0) is 20 months stale and API-incompatible
    # with current docs; upstream recommends main, so the cohort pins a git rev.
    ("floem", "git-778bb5f2"),
)
ROUND_SUFFIXES = {
    "iter1": ("app",),
    "iter2": ("dash", "board"),
    "iter3": ("tray", "babel"),
    "iter4": ("grid", "fetch", "peek"),
}
SKIA_CACHE_ENV_TO_KEY = {
    "FREYA_SKIA_BINARIES_URL": "freya-skia-cache",
    "VIZIA_SKIA_BINARIES_URL": "vizia-skia-cache",
}
LINUX_APPS = (
    "iced-app",
    "egui-app",
    "gpui-app",
    "tauri-app",
    "xilem-app",
    "slint-app",
    "dioxus-app",
    "freya-app",
    "vizia-app",
    "floem-app",
    "iced-tray",
    "egui-tray",
    "freya-tray",
    "vizia-tray",
    "floem-tray",
    "iced-babel",
    "gpui-babel",
    "freya-babel",
    "vizia-babel",
    "floem-babel",
)


def utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def command_output(*command: str) -> str:
    try:
        return subprocess.run(
            command,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        ).stdout.strip()
    except (FileNotFoundError, subprocess.CalledProcessError):
        return "unavailable"


def resolve_path(value: str | os.PathLike[str], *, relative_to: Path = ROOT) -> Path:
    path = Path(value).expanduser()
    if not path.is_absolute():
        path = relative_to / path
    return path.resolve()


def resolve_cohort(explicit: str | os.PathLike[str] | None = None) -> Path:
    """Resolve an explicit cohort or the active pointer, with legacy fallback."""

    if explicit:
        return resolve_path(explicit)
    if ACTIVE_POINTER.is_file():
        value = ACTIVE_POINTER.read_text().strip()
        if not value or "\n" in value:
            raise SystemExit(f"invalid active cohort pointer: {ACTIVE_POINTER}")
        return resolve_path(value, relative_to=MEASUREMENTS)
    return MEASUREMENTS


def metadata_path(cohort: Path) -> Path:
    return cohort / "cohort.json"


def load_metadata(cohort: Path, *, required: bool = False) -> dict[str, Any]:
    path = metadata_path(cohort)
    if not path.is_file():
        if required:
            raise SystemExit(f"cohort metadata is missing: {path}")
        return {
            "schema_version": 0,
            "cohort_id": "legacy-july-2026",
            "status": "historical",
            "machine": "Apple M4 Pro; 24 GiB; macOS 26.5.2 (25F84); rustc/cargo 1.96.1",
            "artifacts": {},
        }
    try:
        value = json.loads(path.read_text())
    except json.JSONDecodeError as error:
        raise SystemExit(f"invalid cohort metadata {path}: {error}") from error
    if not isinstance(value, dict):
        raise SystemExit(f"cohort metadata must be a JSON object: {path}")
    return value


def atomic_write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w") as handle:
            handle.write(content)
        Path(temporary).replace(path)
    except BaseException:
        Path(temporary).unlink(missing_ok=True)
        raise


def save_metadata(cohort: Path, metadata: dict[str, Any]) -> None:
    atomic_write(metadata_path(cohort), json.dumps(metadata, indent=2, sort_keys=True) + "\n")


def relative_to_root(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return str(path.resolve())


def expected_apps(round_name: str) -> set[str]:
    return {
        f"{framework}-{suffix}"
        for framework, _ in FRAMEWORKS
        for suffix in ROUND_SUFFIXES[round_name]
    }


def measurement_environment_values(
    metadata: dict[str, Any], variable: str
) -> dict[str, set[str]]:
    """Return values assigned to an environment variable by round commands."""

    found: dict[str, set[str]] = {}
    artifacts = metadata.get("artifacts", {})
    if not isinstance(artifacts, dict):
        return found
    assignment = f"{variable}="
    for round_name in ROUND_SUFFIXES:
        entry = artifacts.get(round_name, {})
        if not isinstance(entry, dict):
            continue
        command = str(entry.get("command", ""))
        try:
            tokens = shlex.split(command)
        except ValueError as error:
            raise SystemExit(
                f"cohort {round_name} reproduction command is not valid shell syntax: {error}"
            ) from error
        values = {
            token[len(assignment) :]
            for token in tokens
            if token.startswith(assignment)
        }
        if "" in values:
            raise SystemExit(f"cohort {round_name} records an empty {variable}")
        if values:
            found[round_name] = values
    return found


def _file_url_path(value: str) -> Path | None:
    parsed = urlsplit(value)
    if parsed.scheme != "file":
        return None
    if parsed.netloc not in {"", "localhost"}:
        raise SystemExit(f"local Skia cache URL must use file:///absolute/path: {value}")
    if parsed.query or parsed.fragment:
        raise SystemExit(f"local Skia cache URL must not contain a query or fragment: {value}")
    path = Path(unquote(parsed.path))
    if not path.is_absolute():
        raise SystemExit(f"local Skia cache URL must be absolute: {value}")
    return path.resolve()


def validate_skia_cache_registrations(
    cohort: Path, metadata: dict[str, Any]
) -> dict[str, Path]:
    """Validate explicitly used, framework-specific rust-skia cache archives."""

    generic = measurement_environment_values(metadata, "SKIA_BINARIES_URL")
    if generic:
        raise SystemExit(
            "fresh measurement commands must not record global SKIA_BINARIES_URL; "
            "use FREYA_SKIA_BINARIES_URL and VIZIA_SKIA_BINARIES_URL"
        )

    artifacts = metadata.get("artifacts", {})
    if not isinstance(artifacts, dict):
        artifacts = {}
    cohort = cohort.resolve()
    registered: dict[str, Path] = {}
    for variable, artifact_key in SKIA_CACHE_ENV_TO_KEY.items():
        uses = measurement_environment_values(metadata, variable)
        if not uses:
            continue

        entry = artifacts.get(artifact_key)
        if not isinstance(entry, dict) or not entry.get("path"):
            rounds = ", ".join(sorted(uses))
            raise SystemExit(
                f"measurement command(s) for {rounds} use {variable}, but cohort "
                f"artifact {artifact_key!r} is not registered"
            )
        cache = resolve_path(str(entry["path"]))
        if not cache.is_file():
            raise SystemExit(f"registered {artifact_key} file is missing: {cache}")
        try:
            cache.relative_to(cohort)
        except ValueError as error:
            raise SystemExit(
                f"registered {artifact_key} must be inside its cohort: {cache}"
            ) from error

        for round_name, values in uses.items():
            for value in values:
                local_path = _file_url_path(value)
                if local_path is not None and local_path != cache:
                    raise SystemExit(
                        f"{round_name} {variable} points to {local_path}, not registered "
                        f"{artifact_key} {cache}"
                    )
        registered[artifact_key] = cache

    if len(set(registered.values())) != len(registered):
        raise SystemExit("Freya and Vizia Skia caches must be distinct registered files")
    return registered


def read_rows(path: Path, *, delimiter: str = ",") -> list[dict[str, str]]:
    if not path.is_file():
        raise SystemExit(f"required cohort artifact is missing: {path}")
    with path.open(newline="") as handle:
        records = list(csv.DictReader(handle, delimiter=delimiter))
    if not records or "app" not in records[0]:
        raise SystemExit(f"artifact has no app rows: {path}")
    return records


def read_apps(path: Path, *, delimiter: str = ",") -> set[str]:
    records = read_rows(path, delimiter=delimiter)
    apps = [record["app"] for record in records]
    if len(apps) != len(set(apps)):
        raise SystemExit(f"artifact contains duplicate app rows: {path}")
    return set(apps)


def validate_runtime_samples(path: Path, expected: set[str]) -> list[dict[str, str]]:
    """Validate the fresh dashboard sampler's exact 9 x 30 raw-series contract."""

    records = read_rows(path)
    required_fields = {
        "app", "sample_index", "main_pid", "helper_pids", "cpu_pct", "rss_kib", "rss_mib",
    }
    fields = set(records[0])
    if fields != required_fields:
        raise SystemExit(
            f"{path}: runtime sample schema mismatch; "
            f"missing={sorted(map(str, required_fields - fields))}, "
            f"extra={sorted(map(str, fields - required_fields))}"
        )

    indexes: dict[str, set[int]] = {app: set() for app in expected}
    for line_number, record in enumerate(records, start=2):
        app = record["app"]
        if app not in expected:
            raise SystemExit(f"{path}:{line_number}: unexpected runtime sample app {app!r}")

        try:
            sample_index = int(record["sample_index"])
            main_pid = int(record["main_pid"])
            rss_kib = int(record["rss_kib"])
            cpu_pct = float(record["cpu_pct"])
            rss_mib = float(record["rss_mib"])
        except (TypeError, ValueError) as error:
            raise SystemExit(f"{path}:{line_number}: non-numeric runtime sample field") from error
        if sample_index not in range(1, 31):
            raise SystemExit(f"{path}:{line_number}: sample_index must be in 1..30")
        if sample_index in indexes[app]:
            raise SystemExit(f"{path}:{line_number}: duplicate sample index {app}/{sample_index}")
        if main_pid <= 0 or rss_kib < 0 or cpu_pct < 0 or rss_mib < 0:
            raise SystemExit(f"{path}:{line_number}: invalid negative/zero runtime telemetry")
        if not math.isfinite(cpu_pct) or not math.isfinite(rss_mib):
            raise SystemExit(f"{path}:{line_number}: non-finite runtime telemetry")

        helper_pids = record["helper_pids"]
        if helper_pids is None:
            raise SystemExit(f"{path}:{line_number}: invalid helper_pids field")
        if helper_pids:
            try:
                helpers = [int(value) for value in helper_pids.split(";")]
            except ValueError as error:
                raise SystemExit(f"{path}:{line_number}: invalid helper_pids field") from error
            if not helpers or any(pid <= 0 for pid in helpers):
                raise SystemExit(f"{path}:{line_number}: invalid helper_pids field")
        indexes[app].add(sample_index)

    expected_indexes = set(range(1, 31))
    for app in sorted(expected):
        if indexes[app] != expected_indexes:
            raise SystemExit(
                f"{path}: {app} sample indexes mismatch; "
                f"missing={sorted(expected_indexes - indexes[app])}, "
                f"extra={sorted(indexes[app] - expected_indexes)}"
            )
    if len(records) != len(expected) * 30:
        raise SystemExit(
            f"{path}: expected {len(expected) * 30} runtime sample rows, got {len(records)}"
        )
    return records


def validate_windows_arm(cohort: Path, all_apps: set[str], packaging_expected: set[str]) -> None:
    """Validate the Windows reality-check arm (x64 desktop machine).

    App-set checks only: compile/run failures are recorded findings there,
    exactly as on Linux — validation gates on coverage, not outcomes.
    """

    if read_apps(cohort / "windows/results.csv") != all_apps:
        raise SystemExit("Windows results do not contain all 80 apps")
    if not (cohort / "windows/environment.txt").is_file():
        raise SystemExit("Windows environment evidence is missing")
    packaging_records = read_rows(cohort / "windows/packaging/results.csv")
    # app x tool x format rows: the app column legitimately repeats, so the
    # duplicate-rejecting read_apps() cannot be used here.
    if {record["app"] for record in packaging_records} != packaging_expected:
        raise SystemExit("Windows packaging results do not cover all ten todo apps")
    packaging_rows = {
        (record["app"], record.get("tool", ""), record.get("format", ""))
        for record in packaging_records
    }
    if len(packaging_rows) != len(packaging_records):
        raise SystemExit("Windows packaging results contain duplicate app/tool/format rows")


def validate_complete(cohort: Path) -> None:
    metadata = load_metadata(cohort, required=True)
    skia_caches = validate_skia_cache_registrations(cohort, metadata)
    configured = tuple(
        (entry.get("name"), entry.get("version"))
        for entry in metadata.get("frameworks", [])
        if isinstance(entry, dict)
    )
    if configured != FRAMEWORKS:
        raise SystemExit(
            "cohort framework order/version mismatch; expected "
            + ", ".join(f"{name} {version}" for name, version in FRAMEWORKS)
        )

    all_apps: set[str] = set()
    for round_name in ROUND_SUFFIXES:
        path = cohort / f"results-{round_name}.csv"
        records = read_rows(path)
        apps = {record["app"] for record in records}
        expected = expected_apps(round_name)
        if apps != expected:
            raise SystemExit(
                f"{path}: app-set mismatch; missing={sorted(expected - apps)}, "
                f"extra={sorted(apps - expected)}"
            )
        all_apps.update(apps)
        for record in records:
            if record.get("clean_build_secs") in {
                "BUILD_FAILED", "METADATA_FAILED", "TARGET_FAILED", "",
            }:
                raise SystemExit(f"{path}: failed/incomplete build for {record['app']}")
            if record.get("incremental_secs") in {"INCREMENTAL_FAILED", ""}:
                raise SystemExit(f"{path}: failed/incomplete incremental build for {record['app']}")
            if record.get("process_survived_8s") != "yes":
                raise SystemExit(f"{path}: {record['app']} did not survive eight seconds")
    if len(all_apps) != 80:
        raise SystemExit(f"cohort round union: expected 80 apps, got {len(all_apps)}")

    runtime_expected = {f"{name}-dash" for name, _ in FRAMEWORKS}
    runtime_path = cohort / "runtime.csv"
    runtime_records = read_rows(runtime_path)
    runtime_apps = {record["app"] for record in runtime_records}
    if runtime_apps != runtime_expected:
        raise SystemExit("runtime.csv does not contain the exact ten dashboard apps")
    if any(record.get("avg_cpu_pct") in {"", "DIED"} for record in runtime_records):
        raise SystemExit("runtime.csv contains an incomplete dashboard sample")
    validate_runtime_samples(cohort / "runtime-samples.csv", runtime_expected)

    verification_path = cohort / "verification-all/results.tsv"
    verification_records = read_rows(verification_path, delimiter="\t")
    verification_apps = {record["app"] for record in verification_records}
    if verification_apps != all_apps:
        raise SystemExit("full visible-window verification does not contain all 80 apps")
    for record in verification_records:
        if (
            record.get("process_survived_8s") != "true"
            or record.get("visible_window_observed") != "true"
            or record.get("exit_before_cleanup") != "false"
        ):
            raise SystemExit(f"full verification failed for {record['app']}")
    if read_apps(cohort / "verification-all/binary-inventory.tsv", delimiter="\t") != all_apps:
        raise SystemExit("full binary inventory does not contain all 80 apps")

    iter3_path = cohort / "verification-iter3/results.tsv"
    iter3_records = read_rows(iter3_path, delimiter="\t")
    iter3_apps = {record["app"] for record in iter3_records}
    if iter3_apps != expected_apps("iter3"):
        raise SystemExit("iteration-3 verification does not contain all 20 apps")
    for record in iter3_records:
        if (
            record.get("process_survived_8s") != "true"
            or record.get("visible_window_observed") != "true"
            or record.get("exit_before_cleanup") != "false"
        ):
            raise SystemExit(f"iteration-3 verification failed for {record['app']}")

    packaging_expected = {f"{name}-app" for name, _ in FRAMEWORKS}
    packaging_path = cohort / "packaging/results.csv"
    packaging_records = read_rows(packaging_path)
    if {record["app"] for record in packaging_records} != packaging_expected:
        raise SystemExit("packaging results do not contain all ten todo apps")
    for record in packaging_records:
        if (
            record.get("bundle_status") != "passed"
            or record.get("codesign_ok") != "yes"
            or record.get("hdiutil_verify") != "yes"
            or record.get("mounted_bundle_codesign") != "yes"
        ):
            raise SystemExit(f"packaging verification failed for {record['app']}")

    if read_apps(cohort / "linux/results.csv") != set(LINUX_APPS):
        raise SystemExit("Linux results do not contain the exact 20-app cohort")
    if not (cohort / "linux/environment.txt").is_file():
        raise SystemExit("Linux environment evidence is missing")

    validate_windows_arm(cohort, all_apps, packaging_expected)

    babel_expected = {f"{name}-babel" for name, _ in FRAMEWORKS}
    screenshots = {path.parent.name for path in (ROOT / "apps").glob("*-babel/screenshot.png")}
    if screenshots != babel_expected:
        raise SystemExit("Babel screenshot set does not contain exactly ten framework apps")

    expected_artifact_keys = {
        "iter1", "iter2", "iter3", "iter4", "runtime", "verification-iter3",
        "verification-all", "packaging", "linux", "windows", "windows-packaging",
    } | set(skia_caches)
    artifact_keys = set(metadata.get("artifacts", {}))
    if not expected_artifact_keys <= artifact_keys:
        raise SystemExit(
            f"cohort metadata lacks completed artifacts: {sorted(expected_artifact_keys - artifact_keys)}"
        )
    if not (cohort / "artifact-manifest.tsv").is_file():
        raise SystemExit("generate the cohort artifact manifest before promotion")


def init_command(args: argparse.Namespace) -> None:
    cohort_id = args.id or datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ-ten-framework-macos")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", cohort_id):
        raise SystemExit("--id may contain only letters, digits, dot, underscore, and hyphen")
    cohort = MEASUREMENTS / "reruns" / cohort_id
    if cohort.exists():
        raise SystemExit(f"refusing to reuse existing cohort directory: {cohort}")
    rustc_version = command_output("rustup", "run", "1.96.1", "rustc", "--version")
    cargo_version = command_output("rustup", "run", "1.96.1", "cargo", "--version")
    if not rustc_version.startswith("rustc 1.96.1 ") or not cargo_version.startswith("cargo 1.96.1 "):
        raise SystemExit("the 1.96.1 Rust toolchain must be installed before initializing a cohort")
    cohort.mkdir(parents=True)
    metadata = {
        "schema_version": 1,
        "cohort_id": cohort_id,
        "status": "in-progress",
        "created_at_utc": utc_now(),
        "machine": {
            "architecture": platform.machine(),
            "hardware": command_output("sysctl", "-n", "machdep.cpu.brand_string"),
            "memory_bytes": command_output("sysctl", "-n", "hw.memsize"),
            "os": command_output("sw_vers", "-productName"),
            "os_version": command_output("sw_vers", "-productVersion"),
            "os_build": command_output("sw_vers", "-buildVersion"),
        },
        "rustc": rustc_version,
        "cargo": cargo_version,
        "frameworks": [{"name": name, "version": version} for name, version in FRAMEWORKS],
        "artifacts": {},
    }
    save_metadata(cohort, metadata)
    print(relative_to_root(cohort))


def record_command(args: argparse.Namespace) -> None:
    cohort = resolve_cohort(args.cohort)
    metadata = load_metadata(cohort, required=True)
    if metadata.get("status") != "in-progress":
        raise SystemExit(f"cannot record into cohort with status={metadata.get('status')!r}")
    artifact = resolve_path(args.artifact)
    if not artifact.is_file():
        raise SystemExit(f"cannot record missing cohort artifact: {artifact}")
    try:
        artifact.relative_to(cohort)
    except ValueError as error:
        raise SystemExit(f"artifact must be inside its cohort: {artifact}") from error
    artifacts = metadata.setdefault("artifacts", {})
    artifacts[args.key] = {
        "path": relative_to_root(artifact),
        "started_at_utc": args.started,
        "completed_at_utc": args.completed or utc_now(),
        "command": args.command,
    }
    save_metadata(cohort, metadata)


def promote_command(args: argparse.Namespace) -> None:
    cohort = resolve_cohort(args.cohort)
    try:
        cohort_relative = cohort.relative_to(MEASUREMENTS)
    except ValueError as error:
        raise SystemExit("only a cohort inside measurements/ can be promoted") from error
    if cohort == MEASUREMENTS:
        raise SystemExit("the legacy measurements root cannot be promoted")
    validate_complete(cohort)
    for command in (
        [sys.executable, str(ROOT / "scripts/sync-benchmark-tables.py"), "--check", "--cohort", str(cohort)],
        [sys.executable, str(ROOT / "scripts/generate-evidence-manifest.py"), "--check", "--cohort", str(cohort)],
    ):
        result = subprocess.run(command, cwd=ROOT)
        if result.returncode:
            raise SystemExit(f"promotion check failed: {' '.join(command)}")
    metadata = load_metadata(cohort, required=True)
    metadata["status"] = "complete"
    metadata["completed_at_utc"] = utc_now()
    save_metadata(cohort, metadata)
    atomic_write(ACTIVE_POINTER, cohort_relative.as_posix() + "\n")
    print(f"promoted {metadata.get('cohort_id')} via {relative_to_root(ACTIVE_POINTER)}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="subcommand", required=True)

    init = subparsers.add_parser("init", help="create a new in-progress cohort")
    init.add_argument("--id", help="directory/cohort identifier (default: timestamped)")
    init.set_defaults(function=init_command)

    record = subparsers.add_parser("record", help="record one completed cohort artifact")
    record.add_argument("--cohort", required=True)
    record.add_argument("--key", required=True)
    record.add_argument("--artifact", required=True)
    record.add_argument("--started", required=True)
    record.add_argument("--completed")
    record.add_argument("--command", required=True)
    record.set_defaults(function=record_command)

    promote = subparsers.add_parser(
        "promote", help="validate a complete cohort and atomically make it active"
    )
    promote.add_argument("--cohort", required=True)
    promote.set_defaults(function=promote_command)
    return parser


if __name__ == "__main__":
    arguments = build_parser().parse_args()
    arguments.function(arguments)
