#!/usr/bin/env python3
"""Focused non-build tests for fresh-cohort artifact validation."""

from __future__ import annotations

import csv
import shlex
import tempfile
import tomllib
import unittest
from pathlib import Path

from cohort import (
    validate_runtime_samples,
    validate_skia_cache_registrations,
    validate_windows_arm,
)


FIELDS = (
    "app", "sample_index", "main_pid", "helper_pids", "cpu_pct", "rss_kib", "rss_mib",
)
ROOT = Path(__file__).resolve().parents[1]


def valid_rows(apps: tuple[str, ...]) -> list[dict[str, str]]:
    return [
        {
            "app": app,
            "sample_index": str(index),
            "main_pid": "1234",
            "helper_pids": "2345;3456" if app == apps[0] else "",
            "cpu_pct": "12.5",
            "rss_kib": "102400",
            "rss_mib": "100.000",
        }
        for app in apps
        for index in range(1, 31)
    ]


class RuntimeSamplesValidationTests(unittest.TestCase):
    apps = ("freya-dash", "vizia-dash")

    def validate(self, rows: list[dict[str, str]], fieldnames=FIELDS) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            artifact = Path(temporary) / "runtime-samples.csv"
            with artifact.open("w", newline="") as handle:
                writer = csv.DictWriter(
                    handle, fieldnames=fieldnames, extrasaction="ignore", lineterminator="\n"
                )
                writer.writeheader()
                writer.writerows(rows)
            validate_runtime_samples(artifact, set(self.apps))

    def test_accepts_exact_thirty_indexed_numeric_samples_per_app(self) -> None:
        self.validate(valid_rows(self.apps))

    def test_rejects_duplicate_or_missing_sample_index(self) -> None:
        rows = valid_rows(self.apps)
        rows[29]["sample_index"] = "29"
        with self.assertRaisesRegex(SystemExit, "duplicate sample index"):
            self.validate(rows)

    def test_rejects_non_numeric_telemetry_and_helper_pids(self) -> None:
        for field, value in (("cpu_pct", "NaN"), ("rss_kib", "many"), ("helper_pids", "12;bad")):
            with self.subTest(field=field):
                rows = valid_rows(self.apps)
                rows[0][field] = value
                with self.assertRaises(SystemExit):
                    self.validate(rows)

    def test_rejects_schema_drift(self) -> None:
        with self.assertRaisesRegex(SystemExit, "schema mismatch"):
            self.validate(valid_rows(self.apps), fieldnames=FIELDS[:-1])


class ToolchainIdentityContractTests(unittest.TestCase):
    def test_root_pin_is_bound_into_every_manifest_app_identity(self) -> None:
        with (ROOT / "rust-toolchain.toml").open("rb") as handle:
            toolchain = tomllib.load(handle)["toolchain"]
        self.assertEqual(toolchain, {"channel": "1.96.1", "profile": "minimal"})

        generator = (ROOT / "scripts/generate-evidence-manifest.py").read_text()
        self.assertIn('COHORT_BUILD_INPUTS = (ROOT / "rust-toolchain.toml",)', generator)
        self.assertIn(
            "files: list[Path] = [required(path) for path in COHORT_BUILD_INPUTS]",
            generator,
        )


class SkiaCacheValidationTests(unittest.TestCase):
    @staticmethod
    def metadata(command: str, caches: dict[str, Path] | None = None) -> dict:
        artifacts: dict[str, dict[str, str]] = {"iter4": {"command": command}}
        for key, path in (caches or {}).items():
            artifacts[key] = {"path": str(path)}
        return {"artifacts": artifacts}

    def test_no_explicit_framework_cache_requires_no_registration(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            cohort = Path(temporary)
            self.assertEqual(
                validate_skia_cache_registrations(cohort, self.metadata("cargo build")),
                {},
            )

    def test_rejects_unsafe_global_skia_cache_variable(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            cohort = Path(temporary)
            metadata = self.metadata("SKIA_BINARIES_URL=file:///tmp/global.tar.gz ./measure.sh")
            with self.assertRaisesRegex(SystemExit, "must not record global"):
                validate_skia_cache_registrations(cohort, metadata)

    def test_requires_each_used_framework_cache_registration(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            cohort = Path(temporary)
            command = "FREYA_SKIA_BINARIES_URL=file:///tmp/freya.tar.gz ./measure.sh"
            with self.assertRaisesRegex(SystemExit, "freya-skia-cache.*not registered"):
                validate_skia_cache_registrations(cohort, self.metadata(command))

    def test_accepts_two_distinct_registered_archives_and_shell_quoted_urls(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            cohort = Path(temporary)
            cache_dir = cohort / "cache with spaces"
            cache_dir.mkdir()
            freya = cache_dir / "freya.tar.gz"
            vizia = cache_dir / "vizia.tar.gz"
            freya.write_bytes(b"freya")
            vizia.write_bytes(b"vizia")
            command = " ".join(
                (
                    f"FREYA_SKIA_BINARIES_URL={shlex.quote(freya.as_uri())}",
                    f"VIZIA_SKIA_BINARIES_URL={shlex.quote(vizia.as_uri())}",
                    "./measure.sh --round iter4",
                )
            )
            result = validate_skia_cache_registrations(
                cohort,
                self.metadata(
                    command,
                    {"freya-skia-cache": freya, "vizia-skia-cache": vizia},
                ),
            )
            self.assertEqual(
                result,
                {
                    "freya-skia-cache": freya.resolve(),
                    "vizia-skia-cache": vizia.resolve(),
                },
            )

    def test_rejects_file_url_that_does_not_match_registered_archive(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            cohort = Path(temporary)
            registered = cohort / "registered.tar.gz"
            other = cohort / "other.tar.gz"
            registered.write_bytes(b"registered")
            other.write_bytes(b"other")
            command = f"VIZIA_SKIA_BINARIES_URL={other.as_uri()} ./measure.sh"
            metadata = self.metadata(command, {"vizia-skia-cache": registered})
            with self.assertRaisesRegex(SystemExit, "not registered vizia-skia-cache"):
                validate_skia_cache_registrations(cohort, metadata)


class WindowsArmValidationTests(unittest.TestCase):
    apps = ("iced-app", "freya-peek")
    packaging_apps = ("iced-app",)

    @staticmethod
    def write_csv(path: Path, fieldnames: tuple[str, ...], rows: list[dict[str, str]]) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("w", newline="") as handle:
            writer = csv.DictWriter(handle, fieldnames=fieldnames, lineterminator="\n")
            writer.writeheader()
            writer.writerows(rows)

    def build_cohort(
        self,
        cohort: Path,
        *,
        results_apps: tuple[str, ...] | None = None,
        environment: bool = True,
        packaging_rows: list[dict[str, str]] | None = None,
    ) -> None:
        self.write_csv(
            cohort / "windows/results.csv",
            ("app", "compile_ok", "run_alive_10s_default", "notes"),
            [
                # compile_ok=no is deliberately valid: failures are findings.
                {"app": app, "compile_ok": "no", "run_alive_10s_default": "no", "notes": ""}
                for app in (results_apps if results_apps is not None else self.apps)
            ],
        )
        if environment:
            environment_path = cohort / "windows/environment.txt"
            environment_path.parent.mkdir(parents=True, exist_ok=True)
            environment_path.write_text("machine=stub; rustc/cargo 1.96.1\n")
        if packaging_rows is None:
            packaging_rows = [
                {"app": "iced-app", "tool": "cargo-bundle", "format": "msi", "status": "failed"},
                {"app": "iced-app", "tool": "dx", "format": "msi", "status": "passed"},
            ]
        self.write_csv(
            cohort / "windows/packaging/results.csv",
            ("app", "tool", "format", "status"),
            packaging_rows,
        )

    def validate(self, cohort: Path) -> None:
        validate_windows_arm(cohort, set(self.apps), set(self.packaging_apps))

    def test_accepts_full_coverage_including_recorded_failures(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            cohort = Path(temporary)
            self.build_cohort(cohort)
            self.validate(cohort)

    def test_rejects_missing_app_rows(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            cohort = Path(temporary)
            self.build_cohort(cohort, results_apps=self.apps[:1])
            with self.assertRaisesRegex(SystemExit, "all 80 apps"):
                self.validate(cohort)

    def test_rejects_missing_environment_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            cohort = Path(temporary)
            self.build_cohort(cohort, environment=False)
            with self.assertRaisesRegex(SystemExit, "environment evidence"):
                self.validate(cohort)

    def test_rejects_packaging_coverage_gaps_and_duplicates(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            cohort = Path(temporary)
            self.build_cohort(
                cohort,
                packaging_rows=[
                    {"app": "egui-app", "tool": "dx", "format": "msi", "status": "passed"},
                ],
            )
            with self.assertRaisesRegex(SystemExit, "all ten todo apps"):
                self.validate(cohort)
        with tempfile.TemporaryDirectory() as temporary:
            cohort = Path(temporary)
            duplicate = {"app": "iced-app", "tool": "dx", "format": "msi", "status": "passed"}
            self.build_cohort(cohort, packaging_rows=[duplicate, dict(duplicate)])
            with self.assertRaisesRegex(SystemExit, "duplicate app/tool/format"):
                self.validate(cohort)


class SkiaCacheProtocolContractTests(unittest.TestCase):
    def test_measurement_wrapper_and_manifest_count_contract(self) -> None:
        measure = (ROOT / "measure.sh").read_text()
        self.assertIn('export SKIA_BINARIES_URL="$FREYA_SKIA_BINARIES_URL"', measure)
        self.assertIn('export SKIA_BINARIES_URL="$VIZIA_SKIA_BINARIES_URL"', measure)
        self.assertIn("FREYA_SKIA_BINARIES_URL=$freya_skia_quoted", measure)
        self.assertIn("VIZIA_SKIA_BINARIES_URL=$vizia_skia_quoted", measure)

        generator = (ROOT / "scripts/generate-evidence-manifest.py").read_text()
        # 80 build-benchmark + 10 runtime + 100 launch-verification
        # + 80 binary-snapshot + 2 audit-reruns + 10 babel-gallery
        self.assertIn("FRESH_MANIFEST_BASE_ROWS = 282", generator)
        self.assertIn("expected_rows = FRESH_MANIFEST_BASE_ROWS + len(SKIA_CACHES)", generator)
        self.assertIn('record_type="build-cache"', generator)
        self.assertIn('"artifact_sha256": artifact_sha256(artifact)', generator)
        # Windows arm: cohort-gated 80 reality + 33 packaging rows.
        self.assertIn("WINDOWS_REALITY_ROWS = 80", generator)
        self.assertIn("WINDOWS_PACKAGING_ROWS = 33", generator)
        self.assertIn("expected_rows += WINDOWS_REALITY_ROWS + WINDOWS_PACKAGING_ROWS", generator)


if __name__ == "__main__":
    unittest.main()
