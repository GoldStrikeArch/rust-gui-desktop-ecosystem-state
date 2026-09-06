#!/bin/bash
# Scripted verification run for apps/xilem-ledger (SPEC-10, macOS).
#
# Two kinds of evidence:
#   A. synthetic-input — real CGEvent clicks/keys (tools/synth/synth) plus
#      AppleScript keystrokes against the running app.
#   B. self-test/demo  — LEDGER_DEMO=N replays the same interactions through
#      injected masonry TextEvents and stops at step N, so the screenshot is
#      deterministic. Needed because sibling research apps in this corpus
#      float always-on-top windows over the ledger window, which makes
#      synthetic clicks on the middle columns unreliable.
#
# The window is always captured by CGWindowID (screencapture -l), so occlusion
# by those sibling windows does not affect the images.
set -u
cd "$(dirname "$0")/.."
ROOT=../..
SYNTH=$ROOT/tools/synth/synth
OUT=evidence
LOG=$OUT/log.txt
mkdir -p $OUT
: > "$LOG"
say() { echo "$@" | tee -a "$LOG"; }

PID=""; WID=""
launch() { # launch [env assignments...]
  pkill -f 'release/xilem-ledger'; sleep 1
  say "\$ $* LEDGER_TOPMOST=1 LEDGER_POS=560,40 ./target/release/xilem-ledger > run.log"
  (env "$@" LEDGER_TOPMOST=1 LEDGER_POS=560,40 ./target/release/xilem-ledger > run.log 2> run.err &)
  sleep 5
  PID=$(pgrep -n xilem-ledger)
  WID=$(swift verify/windows.swift "$PID" | awk '{print $1}')
  say "  PID=$PID CGWindowID=$WID  ($(swift verify/windows.swift "$PID"))"
}
act() { osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $PID) to true" >/dev/null 2>&1; sleep 0.35; }
shot() { say "\$ screencapture -x -o -l \$WID $OUT/$1"; screencapture -x -o -l "$WID" "$OUT/$1"; sips -Z 1200 "$OUT/$1" >/dev/null; }
click() { act; say "\$ synth click $1 $2 1"; $SYNTH click "$1" "$2" 1 >/dev/null; sleep 0.6; }
key() { act; say "\$ synth key $1"; $SYNTH key "$1" >/dev/null; sleep 0.45; }

# Screen coordinates derived from the window frame (560,33 820x592) and the
# fixed column layout in src/main.rs.
X_DATE=626; X_LOCALE=636; Y_TOOLBAR=91; ROW0_Y=161

say "===== A. synthetic input ====="
launch LEDGER_DEMO=  # empty -> parsed as None
say ""; say "-- A1. initial state: en-US, Amount column aligned on the decimal point"
shot 01-initial.png

say ""; say "-- A2. Tab-order walk (real Tab key presses; the app logs"
say "        RenderRoot::focused_widget() after every winit event)"
click $X_DATE $ROW0_Y
: > run.log
for i in $(seq 1 10); do key 48; done
say "--- app stdout ---"
cat run.log | tee -a "$LOG"
cp run.log $OUT/tab-order-walk.txt

say ""; say "-- A3. Locale toggle by click -> whole table re-parses and re-formats"
click $X_LOCALE $Y_TOOLBAR
shot 04-fr-fr-locale-toggle.png
click $X_LOCALE $Y_TOOLBAR

say ""; say "-- A4. accessibility dump (AccessKit tree via System Events)"
say "\$ osascript -e 'tell application \"System Events\" to get entire contents of window 1 of process \"xilem-ledger\"'"
act
osascript -e 'tell application "System Events" to get entire contents of window 1 of process "xilem-ledger"' \
  > $OUT/ax-dump.txt 2>&1
sleep 1
# second query: AccessKit only builds the tree after the first request
osascript -e 'tell application "System Events" to get entire contents of window 1 of process "xilem-ledger"' \
  >> $OUT/ax-dump.txt 2>&1
say "  wrote $OUT/ax-dump.txt ($(wc -c < $OUT/ax-dump.txt) bytes)"

say ""; say "===== B. scripted demo (injected TextEvents, deterministic stop) ====="
say ""; say "-- B1. LEDGER_DEMO=2: 'a' selected then 1234.5 typed, not yet committed"
launch LEDGER_DEMO=2; sleep 2; shot 02-typed-before-tab.png
say ""; say "-- B2. LEDGER_DEMO=3: Tab -> blur commits, normalises, formats"
launch LEDGER_DEMO=3; sleep 2; shot 03-en-us-1234.50.png
say ""; say "-- B3. LEDGER_DEMO=4: same value under fr-FR"
launch LEDGER_DEMO=4; sleep 2; shot 04b-fr-fr-1-234-50.png
say ""; say "-- B4. LEDGER_DEMO=7: description cleared -> inline error + Save disabled"
launch LEDGER_DEMO=7; sleep 2; shot 06-validation-error.png
say ""; say "-- B5. LEDGER_DEMO=9: category type-ahead list filtered by 'S'"
launch LEDGER_DEMO=9; sleep 2; shot 07-category-typeahead.png

say ""; say "===== C. tabular-figures comparison ====="
say "-- C1. LEDGER_MONO=1: same table with FontStack=Monospace instead of the"
say "        system UI face (both request tnum/lnum via StyleProperty::FontFeatures)"
launch LEDGER_MONO=1; sleep 2; shot 09-mono-figures.png

say ""; say "===== D. self-test ====="
pkill -f 'release/xilem-ledger'; sleep 1
say "\$ LEDGER_SELFTEST=1 ./target/release/xilem-ledger"
(LEDGER_SELFTEST=1 ./target/release/xilem-ledger > $OUT/selftest-log.txt 2>&1 &)
sleep 12
cat $OUT/selftest-log.txt | tee -a "$LOG"
pkill -f 'release/xilem-ledger'
say ""; say "done."
