#!/usr/bin/env python3
"""Aggregate per-app Windows result.tsv files into results.csv.

Reads every RUNS/<app>/<variant>/result.tsv written by windows/run-app.ps1.
Per app, the 'default' variant (or 'msmf-manifest' when it is the only variant
present; 'default' wins when both exist) fills the compile/build/binary/alive/
window columns. The first non-default variant with run_alive_10s=yes fills the
workaround columns (else the last attempted variant). With --only, rows for
matching apps are recomputed and merged into the existing output CSV; all
other rows are preserved. Output is sorted by app, LF newlines, stdlib only.
"""

import argparse
import csv
import fnmatch
import sys
from pathlib import Path

FIELDS = [
    "app",
    "compile_ok",
    "clean_build_secs",
    "binary_bytes",
    "pdb_bytes",
    "run_alive_10s_default",
    "visible_window_default",
    "workaround_variant",
    "workaround_alive_10s",
    "default_result",
    "workaround_result",
    "notes",
]


def read_result(path: Path):
    with path.open(newline="", encoding="utf-8") as handle:
        return next(csv.DictReader(handle, delimiter="\t"), None)


def classify(row: dict) -> str:
    if row.get("compile_ok", "no") == "no":
        return "build-failed"
    if row.get("run_alive_10s") == "yes":
        if row.get("visible_window") == "yes":
            return "alive+window"
        return "alive-no-window"
    return "died"


def aggregate_app(app_dir: Path):
    app = app_dir.name
    variants = {}
    for tsv in sorted(app_dir.glob("*/result.tsv")):
        row = read_result(tsv)
        if row is not None:
            variants[tsv.parent.name] = row
    if not variants:
        return None

    if "default" in variants:
        base_name = "default"
    elif "msmf-manifest" in variants:
        base_name = "msmf-manifest"
    else:
        base_name = sorted(variants)[0]
    base = variants[base_name]

    # Sorted order matches attempt order for the harness's ladders
    # (tiny-skia alone; wgpu-dx12 before wgpu-gl).
    workaround_names = [name for name in sorted(variants) if name != base_name]
    workaround_name = next(
        (n for n in workaround_names if variants[n].get("run_alive_10s") == "yes"),
        workaround_names[-1] if workaround_names else None,
    )
    workaround = variants[workaround_name] if workaround_name else None

    notes = " | ".join(
        row.get("notes", "").strip()
        for _, row in sorted(variants.items())
        if row.get("notes", "").strip()
    )

    return {
        "app": app,
        "compile_ok": "yes" if base.get("compile_ok") in ("yes", "reused") else "no",
        "clean_build_secs": base.get("clean_build_secs", ""),
        "binary_bytes": base.get("binary_bytes", ""),
        "pdb_bytes": base.get("pdb_bytes", ""),
        "run_alive_10s_default": base.get("run_alive_10s", "no"),
        "visible_window_default": base.get("visible_window", "no"),
        "workaround_variant": workaround_name if workaround_name else "none",
        "workaround_alive_10s": (
            workaround.get("run_alive_10s", "no") if workaround else "not_run"
        ),
        "default_result": classify(base),
        "workaround_result": classify(workaround) if workaround else "not_run",
        "notes": notes,
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Aggregate windows/run-app.ps1 result.tsv files into a CSV."
    )
    parser.add_argument("--runs", required=True, help="the <cohort>/windows/runs dir")
    parser.add_argument("--output", required=True, help="the results.csv to write")
    parser.add_argument(
        "--only",
        action="append",
        default=[],
        metavar="PATTERN",
        help="fnmatch pattern against app names; repeatable. Only matching "
        "apps are recomputed; existing rows for other apps are preserved.",
    )
    args = parser.parse_args()

    runs = Path(args.runs)
    output = Path(args.output)
    if not runs.is_dir():
        sys.exit(f"runs directory not found: {runs}")

    rows = {}
    if args.only and output.is_file():
        with output.open(newline="", encoding="utf-8") as handle:
            for row in csv.DictReader(handle):
                rows[row["app"]] = {key: row.get(key, "") for key in FIELDS}

    for app_dir in sorted(path for path in runs.iterdir() if path.is_dir()):
        app = app_dir.name
        if args.only and not any(fnmatch.fnmatch(app, pat) for pat in args.only):
            continue
        aggregated = aggregate_app(app_dir)
        if aggregated is None:
            sys.exit(f"no result.tsv under {app_dir}")
        rows[app] = aggregated

    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_name(".results.csv.tmp")
    with temporary.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=FIELDS, lineterminator="\n")
        writer.writeheader()
        for app in sorted(rows):
            writer.writerow(rows[app])
    temporary.replace(output)
    print(f"wrote {output} ({len(rows)} apps)")


if __name__ == "__main__":
    main()
