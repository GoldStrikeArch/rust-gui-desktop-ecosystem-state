#!/usr/bin/env python3
"""Record hashes for the executables named by a window-verification TSV."""

from __future__ import annotations

import argparse
import csv
import hashlib
import io
import tomllib
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("results", type=Path, help="window-verification results.tsv")
parser.add_argument("--output", type=Path, help="default: binary-inventory.tsv beside results")
args = parser.parse_args()

results = args.results if args.results.is_absolute() else ROOT / args.results
output = args.output or results.with_name("binary-inventory.tsv")
if not output.is_absolute():
    output = ROOT / output

with results.open(newline="") as handle:
    records = list(csv.DictReader(handle, delimiter="\t"))

rows: list[dict[str, str | int]] = []
for record in records:
    app = record["app"]
    app_dir = ROOT / "apps" / app
    with (app_dir / "Cargo.toml").open("rb") as handle:
        package = tomllib.load(handle)["package"]["name"]
    executable = app_dir / "target" / "release" / package
    if not executable.is_file():
        raise SystemExit(f"missing executable for {app}: {executable}")
    stat = executable.stat()
    executable_hash = sha256(executable)
    recorded_hash = record.get("executable_sha256", "")
    if recorded_hash and recorded_hash != executable_hash:
        raise SystemExit(
            f"executable changed during verification for {app}: "
            f"launched {recorded_hash}, now {executable_hash}"
        )
    rows.append(
        {
            "app": app,
            "package": package,
            "executable": executable.relative_to(ROOT).as_posix(),
            "executable_bytes": stat.st_size,
            "executable_mtime_utc": datetime.fromtimestamp(
                stat.st_mtime, timezone.utc
            ).isoformat().replace("+00:00", "Z"),
            "executable_sha256": executable_hash,
        }
    )

fieldnames = [
    "app",
    "package",
    "executable",
    "executable_bytes",
    "executable_mtime_utc",
    "executable_sha256",
]
buffer = io.StringIO(newline="")
writer = csv.DictWriter(buffer, fieldnames=fieldnames, delimiter="\t", lineterminator="\n")
writer.writeheader()
writer.writerows(rows)
temporary_output = output.with_name(f".{output.name}.tmp")
temporary_output.write_text(buffer.getvalue())
temporary_output.replace(output)
try:
    display_output = output.relative_to(ROOT)
except ValueError:
    display_output = output
print(f"wrote {display_output} ({len(rows)} rows)")
