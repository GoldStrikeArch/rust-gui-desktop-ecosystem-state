#!/usr/bin/env python3
"""Launch dioxus-peek in autotest mode, sample process-tree CPU/RSS, screenshot.

WebKit XPC helpers reparent to launchd (ppid 1), so helpers are attributed to
this run by "new since baseline snapshot" — per-process rows are kept in the
CSV so any contamination from concurrently launched webview apps is auditable.
"""
import subprocess, sys, time, os, csv, re

APP_DIR = "/Users/mpl4/Desktop/workspace/self_learning/rust/gui-ecosystem-research/apps/dioxus-peek"
BIN = APP_DIR + "/target/release/dioxus-peek"
RUN = sys.argv[1] if len(sys.argv) > 1 else "run1"
LOG = f"{APP_DIR}/launch-{RUN}.log"
CSV = f"{APP_DIR}/cpu-samples-{RUN}.csv"
SHOT_TIMES = {12: "rust-cam", 26: "js-cam", 44: "gallery"}  # seconds -> label
DURATION = 80
SCRATCH = os.path.dirname(os.path.abspath(__file__))

def find_window_id(pid):
    """CGWindowID of the app's own window — captures ONLY our window even when
    other (private) windows overlap it."""
    try:
        out = subprocess.run(["swift", f"{SCRATCH}/findwin.swift", str(pid)],
                             capture_output=True, text=True, timeout=30).stdout
        for line in out.splitlines():
            num, width, name = line.split("\t", 2)
            if "Peek" in name and float(width) > 300:
                return num
    except Exception as e:
        print("findwin failed:", e, flush=True)
    return None

def ps_snapshot():
    out = subprocess.run(["ps", "-axo", "pid=,ppid=,%cpu=,rss=,comm="],
                         capture_output=True, text=True).stdout
    rows = []
    for line in out.splitlines():
        parts = line.split(None, 4)
        if len(parts) == 5:
            try:
                rows.append((int(parts[0]), int(parts[1]), float(parts[2]),
                             int(parts[3]), parts[4]))
            except ValueError:
                pass
    return rows

def descendants(rows, root):
    kids = {}
    for pid, ppid, *_ in rows:
        kids.setdefault(ppid, []).append(pid)
    seen, stack = set(), [root]
    while stack:
        p = stack.pop()
        if p in seen:
            continue
        seen.add(p)
        stack.extend(kids.get(p, []))
    return seen

webkit_re = re.compile(r"com\.apple\.WebKit\.(WebContent|GPU|Networking)")
baseline_webkit = {pid for pid, _, _, _, comm in ps_snapshot() if webkit_re.search(comm)}

env = dict(os.environ, PEEK_AUTOTEST="1")
logf = open(LOG, "w")
t0 = time.time()
proc = subprocess.Popen([BIN], stdout=logf, stderr=subprocess.STDOUT, env=env, cwd=APP_DIR)
print(f"spawned pid={proc.pid} t0={t0}", flush=True)

w = csv.writer(open(CSV, "w"))
w.writerow(["elapsed_s", "scope", "pid", "comm", "cpu_pct", "rss_kib"])
shots_done = set()
win_id = None
while time.time() - t0 < DURATION:
    el = round(time.time() - t0, 1)
    rows = ps_snapshot()
    mine = descendants(rows, proc.pid)
    total_cpu, total_rss = 0.0, 0
    for pid, ppid, cpu, rss, comm in rows:
        is_mine = pid in mine
        is_helper = pid not in baseline_webkit and webkit_re.search(comm)
        if is_mine or is_helper:
            scope = "app" if is_mine else "webkit-new"
            w.writerow([el, scope, pid, comm.split("/")[-1], cpu, rss])
            total_cpu += cpu
            total_rss += rss
    w.writerow([el, "TOTAL", "", "", round(total_cpu, 1), total_rss])
    if win_id is None and el >= 5:
        win_id = find_window_id(proc.pid)
        print(f"window id: {win_id}", flush=True)
    for t, label in SHOT_TIMES.items():
        if t not in shots_done and el >= t:
            shots_done.add(t)
            if win_id is not None:
                subprocess.run(["screencapture", "-x", "-o", f"-l{win_id}",
                                f"{APP_DIR}/shot-{RUN}-{label}.png"])
                print(f"shot {label} at {el}s", flush=True)
    if proc.poll() is not None:
        print(f"app exited early code={proc.returncode} at {el}s", flush=True)
        break
    time.sleep(1.0)

proc.terminate()
try:
    proc.wait(timeout=5)
except subprocess.TimeoutExpired:
    proc.kill()
print("done", flush=True)
