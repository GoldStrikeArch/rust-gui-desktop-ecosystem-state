#!/usr/bin/env python3
"""Synchronize repeated benchmark tables/charts from canonical CSV files.

Run without arguments to update marked blocks, or with ``--check`` in CI to
fail if a report/dashboard copy has drifted. Prose and interpretations remain
hand-authored; numeric rows inside BEGIN/END GENERATED markers do not.
"""

from __future__ import annotations

import argparse
import csv
import re
from pathlib import Path


parser = argparse.ArgumentParser()
parser.add_argument("--check", action="store_true", help="fail instead of writing if generated blocks drift")
parser.add_argument("--cohort", help="cohort directory holding the canonical CSVs (defaults to measurements/)")
args = parser.parse_args()


ROOT = Path(__file__).resolve().parents[1]
MEASUREMENTS = Path(args.cohort).expanduser().resolve() if args.cohort else ROOT / "measurements"
MIB = 1024 * 1024
VERSIONS = {
    "iced": "0.14.0",
    "egui": "0.35.0",
    "xilem": "0.4.0",
    "tauri": "2.11.5",
    "dioxus": "0.7.9",
    "slint": "1.17.1",
    "gpui": "0.2.2",
    "freya": "0.4.0",
    "vizia": "0.4.0",
    # floem's crates.io release is stale; the cohort pins a git rev of main.
    "floem": "git-778bb5f2",
}


def read_csv(name: str) -> list[dict[str, str]]:
    with (MEASUREMENTS / name).open(newline="") as handle:
        return list(csv.DictReader(handle))


def framework(app: str) -> str:
    return app.rsplit("-", 1)[0]


def mib(value: str) -> str:
    return f"{int(value) / MIB:.1f}"


def generated_block(text: str, name: str, body: str) -> str:
    begin = f"<!-- BEGIN GENERATED: {name} -->"
    end = f"<!-- END GENERATED: {name} -->"
    pattern = re.compile(
        r"^(?P<indent>[ \t]*)" + re.escape(begin) + r"\n.*?^(?P=indent)" + re.escape(end) + r"$",
        re.DOTALL | re.MULTILINE,
    )
    updated, count = pattern.subn(
        lambda match: f"{match.group('indent')}{begin}\n{body}\n{match.group('indent')}{end}",
        text,
    )
    if count != 1:
        raise RuntimeError(f"expected one generated block {name!r}, found {count}")
    return updated


iter1_rows = {framework(row["app"]): row for row in read_csv("results-iter1.csv")}
iter2_rows = {row["app"]: row for row in read_csv("results-iter2.csv")}
iter4_rows = read_csv("results-iter4.csv")
runtime_rows = {framework(row["app"]): row for row in read_csv("runtime.csv")}


def report10_table() -> str:
    lines = [
        "| App | Clean build | Incremental | Binary (raw MiB) | Binary (stripped MiB) | Unique crate names | AccessKit in tree | LoC (Rust) | LoC (other UI) | Process survived 8 s |",
        "|---|---:|---:|---:|---:|---:|---|---:|---:|---|",
    ]
    for name in ("iced", "egui", "xilem", "tauri", "dioxus", "slint", "gpui", "freya", "vizia", "floem"):
        row = iter1_rows[name]
        incr = row["incremental_secs"] + ("²" if name == "tauri" else "")
        accesskit = "**yes**" if int(row["accesskit_in_deps"]) else "no"
        if name in {"tauri", "dioxus"}:
            accesskit += "³"
        elif name == "gpui":
            accesskit += "⁴"
        rust_loc = row["loc_rust"] + ("¹" if name == "egui" else "")
        ui_loc = row["loc_other_ui"] + (" (.slint)" if name == "slint" else "")
        lines.append(
            f"| {name} | {row['clean_build_secs']} s | {incr} s | {mib(row['binary_bytes'])} | "
            f"{mib(row['binary_stripped_bytes'])} | {row['deps_unique']} | {accesskit} | "
            f"{rust_loc} | {ui_loc} | {row['process_survived_8s']} |"
        )
    return "\n".join(lines)


def report00_table() -> str:
    lines = [
        "| App (version) | Clean build | Incremental | Binary (stripped MiB) | Unique crate names | AccessKit in tree | Recorded source LoC¹ | SPEC-1 result |",
        "|---|---:|---:|---:|---:|---|---:|---|",
    ]
    access = {
        "iced": "no",
        "egui": "**yes**",
        "xilem": "**yes**",
        "tauri": "no (browser-derived a11y)",
        "dioxus": "no (browser-derived a11y)",
        "slint": "**yes**",
        "gpui": "no (merged upstream, unreleased)",
        "freya": "**yes**",
        "vizia": "**yes**",
        "floem": "no (not integrated)",
    }
    for name in ("iced", "egui", "xilem", "tauri", "dioxus", "slint", "gpui", "freya", "vizia", "floem"):
        row = iter1_rows[name]
        recorded_loc = int(row["loc_rust"]) + int(row["loc_other_ui"])
        result = "approximated²" if name == "gpui" else "source-complete"
        lines.append(
            f"| {name} {VERSIONS[name]} | {row['clean_build_secs']} s | {row['incremental_secs']} s | "
            f"{mib(row['binary_stripped_bytes'])} | {row['deps_unique']} | {access[name]} | "
            f"{recorded_loc} | {result} |"
        )
    return "\n".join(lines)


def report11_table() -> str:
    lines = [
        "| App | Clean | Incr | Binary (stripped MiB) | Unique names | | App | Clean | Incr | Binary (stripped MiB) | Unique names |",
        "|---|---:|---:|---:|---:|---|---|---:|---:|---:|---:|",
    ]
    for name in ("iced", "egui", "gpui", "tauri", "xilem", "slint", "dioxus", "freya", "vizia", "floem"):
        dash = iter2_rows[f"{name}-dash"]
        board = iter2_rows[f"{name}-board"]
        lines.append(
            f"| {name}-dash | {dash['clean_build_secs']} s | {dash['incremental_secs']} s | "
            f"{mib(dash['binary_stripped_bytes'])} | {dash['deps_unique']} | | {name}-board | "
            f"{board['clean_build_secs']} s | {board['incremental_secs']} s | "
            f"{mib(board['binary_stripped_bytes'])} | {board['deps_unique']} |"
        )
    return "\n".join(lines)


def iter4_canonical_table() -> str:
    lines = [
        "| App | Clean | Incremental | Binary (stripped MiB) | Unique crate names | AccessKit in tree | LoC (Rust) | LoC (other UI) | Process survived 8 s |",
        "|---|---:|---:|---:|---:|---|---:|---:|---|",
    ]
    for row in iter4_rows:
        accesskit = "yes" if int(row["accesskit_in_deps"]) else "no"
        lines.append(
            f"| {row['app']} | {row['clean_build_secs']} s | {row['incremental_secs']} s | "
            f"{mib(row['binary_stripped_bytes'])} | {row['deps_unique']} | {accesskit} | "
            f"{row['loc_rust']} | {row['loc_other_ui']} | {row['process_survived_8s']} |"
        )
    return "\n".join(lines)


def chart_rows(values: dict[str, float], display, indent: str = "      ") -> str:
    maximum = max(values.values())
    rows = []
    for name, value in sorted(values.items(), key=lambda item: (item[1], item[0])):
        width = round(value / maximum * 100)
        rows.append(
            f'{indent}<div class="brow"><span class="blabel">{name}</span>'
            f'<div class="btrack"><div class="bar" style="width:{width}%"></div></div>'
            f'<span class="bval">{display(value)}</span></div>'
        )
    return "\n".join(rows)


files: dict[Path, str] = {}

path = ROOT / "report/10-empirical-results.md"
files[path] = generated_block(path.read_text(), "iter1-headline", report10_table())

path = ROOT / "report/11-interactive-results.md"
files[path] = generated_block(path.read_text(), "iter2-builds", report11_table())

path = ROOT / "report/00-ecosystem-map.md"
files[path] = generated_block(path.read_text(), "iter1-ecosystem", report00_table())

path = ROOT / "report/data/iter4-rows.md"
files[path] = generated_block(path.read_text(), "iter4-canonical-builds", iter4_canonical_table())

path = ROOT / "dashboard.html"
dashboard = path.read_text()
dashboard = generated_block(
    dashboard,
    "dashboard-iter1-clean",
    chart_rows({name: float(row["clean_build_secs"]) for name, row in iter1_rows.items()}, lambda v: f"{v:.0f} s"),
)
dashboard = generated_block(
    dashboard,
    "dashboard-iter1-binary",
    chart_rows({name: int(row["binary_stripped_bytes"]) / MIB for name, row in iter1_rows.items()}, lambda v: f"{v:.1f}"),
)
dashboard = generated_block(
    dashboard,
    "dashboard-iter1-deps",
    chart_rows({name: float(row["deps_unique"]) for name, row in iter1_rows.items()}, lambda v: f"{v:.0f}"),
)
recorded_locs = {
    name: float(int(row["loc_rust"]) + int(row["loc_other_ui"]))
    for name, row in iter1_rows.items()
}
dashboard = generated_block(dashboard, "dashboard-iter1-loc", chart_rows(recorded_locs, lambda v: f"{v:.0f}"))

iter2_locs: dict[str, float] = {}
for name in VERSIONS:
    rows = (iter2_rows[f"{name}-dash"], iter2_rows[f"{name}-board"])
    iter2_locs[name] = float(sum(int(row["loc_rust"]) + int(row["loc_other_ui"]) for row in rows))
dashboard = generated_block(dashboard, "dashboard-iter2-loc", chart_rows(iter2_locs, lambda v: f"{v:.0f}"))
dashboard = generated_block(
    dashboard,
    "dashboard-runtime-cpu",
    chart_rows({name: float(row["avg_cpu_pct"]) for name, row in runtime_rows.items()}, lambda v: f"{v:.1f}%"),
)
dashboard = generated_block(
    dashboard,
    "dashboard-runtime-rss",
    chart_rows({name: float(row["max_rss_mib"]) for name, row in runtime_rows.items()}, lambda v: f"{v:.0f}"),
)
files[path] = dashboard

drifted = [path for path, expected in files.items() if path.read_text() != expected]
if args.check:
    if drifted:
        for path in drifted:
            print(f"drift: {path.relative_to(ROOT)}")
        raise SystemExit(1)
    print("benchmark copies match canonical CSVs")
else:
    for path in drifted:
        path.write_text(files[path])
        print(f"updated {path.relative_to(ROOT)}")
    if not drifted:
        print("benchmark copies already synchronized")
