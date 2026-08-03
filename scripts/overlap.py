#!/usr/bin/env python3
"""Cross-framework dependency overlap analysis.

Reads selected measurements/<app>-deps-flat.txt files (one 'crate vX.Y.Z' per
line), reports shared-crate counts, pairwise Jaccard overlap, and version skew.
The published ecosystem comparison is the seven iteration-1 todo apps, so that
is the default; other rounds must be selected explicitly.

Usage: python3 scripts/overlap.py [--round iter1|iter2|iter3|all]
"""
import argparse
import glob
import itertools
import os
import re
from collections import defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MEAS = os.path.join(ROOT, "measurements")

parser = argparse.ArgumentParser()
parser.add_argument(
    "--round",
    choices=("iter1", "iter2", "iter3", "all"),
    default="iter1",
    help="selected app set (default: the seven *-app iteration-1 baselines)",
)
args = parser.parse_args()

suffixes = {
    "iter1": ("-app",),
    "iter2": ("-dash", "-board"),
    "iter3": ("-tray", "-babel"),
    "all": ("-app", "-dash", "-board", "-tray", "-babel"),
}[args.round]

apps = {}  # app name -> {crate name -> set(versions)}
for path in sorted(glob.glob(os.path.join(MEAS, "*-deps-flat.txt"))):
    app = os.path.basename(path).replace("-deps-flat.txt", "")
    if not app.endswith(suffixes):
        continue
    crates = defaultdict(set)
    with open(path) as f:
        for line in f:
            m = re.match(r"^(\S+) v(\S+)", line.strip())
            if m:
                crates[m.group(1)].add(m.group(2))
    apps[app] = dict(crates)

if not apps:
    raise SystemExit("no *-deps-flat.txt files in measurements/ — run measure.sh first")

names = sorted(apps)
print(f"Selected apps ({args.round}): {', '.join(names)}\n")

common_all = sorted(set.intersection(*(set(apps[name]) for name in names)))
skew_all = [
    crate
    for crate in common_all
    if len({version for name in names for version in apps[name][crate]}) > 1
]
print("== All-selected-app summary ==")
print(f"  common crate names: {len(common_all)}")
print(f"  common names with version skew: {len(skew_all)}")
if skew_all:
    print(f"  skewed common names: {', '.join(skew_all)}")
print()

# Crates ranked by how many apps share them
count = defaultdict(list)
for app, crates in apps.items():
    for c in crates:
        count[c].append(app)

print("== Crates shared by >= 3 selected apps (the de-facto common foundation) ==")
for c, users in sorted(count.items(), key=lambda kv: (-len(kv[1]), kv[0])):
    if len(users) >= 3:
        versions = sorted({v for u in users for v in apps[u].get(c, set())})
        skew = " !! version skew: " + ", ".join(versions) if len(versions) > 1 else ""
        print(f"  {c:<28} {len(users)}/{len(names)}  [{', '.join(sorted(users))}]{skew}")

print("\n== Pairwise overlap (Jaccard, by crate name) ==")
for a, b in itertools.combinations(names, 2):
    sa, sb = set(apps[a]), set(apps[b])
    j = len(sa & sb) / len(sa | sb)
    print(f"  {a:<14} vs {b:<14} {j:5.1%}  ({len(sa & sb)} shared)")

print("\n== Per-app totals ==")
for a in names:
    print(f"  {a:<14} {len(apps[a])} unique crate names")

print("\n== Interesting layer crates: who uses what ==")
LAYER = {
    "windowing": ["winit", "tao", "floem-winit"],
    "gpu": ["wgpu", "blade-graphics", "glow", "vello", "vger", "tiny-skia", "femtovg", "skia-safe", "skia-bindings", "freya-skia-safe"],
    "text": ["cosmic-text", "parley", "swash", "rustybuzz", "harfrust", "ab_glyph", "fontdb", "fontique", "skrifa", "epaint"],
    "layout": ["taffy", "morphorm"],
    "a11y": ["accesskit"],
    "webview": ["wry", "webkit2gtk", "webview2-com"],
    "interop": ["raw-window-handle"],
}
for layer, crates in LAYER.items():
    print(f"  [{layer}]")
    for c in crates:
        users = [a for a in names if any(k == c or k.startswith(c + "-") for k in apps[a])]
        if users:
            print(f"    {c:<20} -> {', '.join(users)}")
