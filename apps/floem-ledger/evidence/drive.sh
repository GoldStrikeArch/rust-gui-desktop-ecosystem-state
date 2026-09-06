#!/bin/bash
# SPEC-10 verification driver for apps/floem-ledger (macOS). Run from anywhere:
#     bash apps/floem-ledger/evidence/drive.sh
#
# Shared-desktop note: ~10 sibling research GUI apps run on this display at the
# same time and drive their own synthetic input, so this app grew ONE
# verification hook — `LEDGER_RAISE_FILE=<path>`: it polls for that file and,
# when the driver touches it, calls floem's own `action::focus_window()` to
# take the front for the next click. (apps/floem-windows needed objc2 for the
# same job because it has several windows; one window needs no objc2.)
# Screenshots are window-scoped: `screencapture -o -l <windowid>` captures an
# occluded window correctly. Keystrokes are `tools/synth/synth key`; note that
# some steps below are wrapped in retry loops because a sibling app regularly
# wins the front between the raise and the key.
set -u
cd "$(dirname "$0")/../../.." || exit 1
APP=apps/floem-ledger/target/release/floem-ledger
EV=apps/floem-ledger/evidence
SYNTH=tools/synth/synth
RAISE=/tmp/lg-raise
LOG=/tmp/lg-drive.log
X0=60; Y0=33                                     # window frame for LEDGER_POS=60,60
raise() { touch $RAISE; sleep 0.35; }
click() { raise; $SYNTH click "$1" "$2" 1 >/dev/null; sleep 0.5; }
key()   { $SYNTH key "$1" >/dev/null; sleep 0.12; }
shot()  { screencapture -x -o -l "$WID" "$1" && sips -Z 1200 "$1" >/dev/null; }
# Screen points of the controls (window content is 820x560 at 60,33):
#   Locale en-US (158,89)  fr-FR (194,89)
#   row N Date (100, 151+43*(N-1))   Description right edge (360, …)
#   row N Qty (530, …)   Unit price right edge (652, …)

pkill -x floem-ledger 2>/dev/null; sleep 1; rm -f $RAISE
LEDGER_RAISE_FILE=$RAISE LEDGER_POS=60,60 $APP > $LOG 2>&1 &
PID=$!; sleep 3
WID=$(swift scripts/window-count.swift $PID | head -1 | cut -f1)
echo "=== pid $PID window $WID"
shot $EV/01-ledger.png

# [1] type 1234.5 into row 1 Unit price, then Tab  -> 1,234.50
click 652 151
for b in 1 2 3; do raise; for i in 1 2 3; do key 51; done; done   # backspace
raise; for k in 18 19 20 21 47 23; do key $k; done                # 1 2 3 4 . 5
shot $EV/02a-typed-raw.png
raise; key 48                                                     # Tab
sleep 0.6; shot $EV/02b-tab-formatted-en-US.png

# [2] Locale toggle -> 1 234,50 everywhere, live
click 194 89
sleep 0.5; shot $EV/03-locale-fr-FR.png
click 158 89                                                      # back to en-US

# [3] decimal alignment: crop the Unit price + Amount columns
raise; screencapture -x -R550,135,235,365 $EV/04-decimal-alignment.png
sips -Z 800 $EV/04-decimal-alignment.png >/dev/null

# [4] Tab-order walk (the app prints a FOCUS line per focus event)
N=$(wc -l < $LOG); click 100 151
for i in $(seq 1 9); do raise; key 48; sleep 0.3; done
M=$(wc -l < $LOG); sed -n "$((N+1)),${M}p" $LOG | grep FOCUS | tee $EV/tab-order.txt

# [5] validation + typing filter
click 360 211                                                     # row 3 Description
for b in 1 2 3; do raise; for i in 1 2 3 4 5; do key 51; done; done
click 530 240                                                     # row 4 Qty
raise; for k in 0 1 2; do key $k; done                            # a s d -> filtered
sleep 0.6; shot $EV/05-validation-and-filter.png
grep FILTER $LOG | tail -3

# [6] accessibility dump
{ osascript -e 'tell application "System Events" to get entire contents of window 1 of process "floem-ledger"'
  osascript -e 'tell application "System Events" to get properties of window 1 of process "floem-ledger"'
  osascript -e 'tell application "System Events" to count UI elements of window 1 of process "floem-ledger"'
} 2>&1 | tee $EV/ax-dump.txt
cp $LOG $EV/drive-run.log
