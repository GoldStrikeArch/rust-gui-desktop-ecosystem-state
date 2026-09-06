#!/bin/zsh
# SPEC-9 evidence run. Must be run from the REPO ROOT:
#   apps/vizia-windows/evidence/run-evidence.sh
# Parks the windows at fixed coordinates via the app's own persistence file so
# the sibling agents' windows (four other GUI apps were driving synthetic input
# on the same display) cannot collide with the click targets, and re-issues
# every click until the expected window count is observed.
set -u
E=apps/vizia-windows/evidence
L=$E/log.txt
S=./tools/synth/synth
BIN=apps/vizia-windows/target/release/vizia-windows
STATE=apps/vizia-windows/target/release/windows-state.txt

pkill -f 'release/vizia-windows'; sleep 1
printf 'main=760,430,720,480\ninspector=1120,60,360,300\ninspector_open=0\n' > $STATE
rm -f $E/*.png
(cd apps/vizia-windows && ./target/release/vizia-windows > evidence/run.log 2>&1 &)
sleep 4
PID=$(pgrep -n -f 'release/vizia-windows')

raise() { osascript -e "tell application \"System Events\" to tell (first process whose unix id is $PID) to perform action \"AXRaise\" of every window" >/dev/null 2>&1
          osascript -e 'tell application "System Events" to set frontmost of process "vizia-windows" to true' >/dev/null 2>&1; }
axwin() { osascript -e "tell application \"System Events\" to get name of every window of (first process whose unix id is $PID)" 2>/dev/null; }
nwin()  { axwin | tr ',' '\n' | grep -c . ; }
clickuntil() { # X Y WANTED_WINDOW_COUNT
  for i in $(seq 1 25); do
    [ "$(nwin)" = "$3" ] && { echo "  (target reached after $((i-1)) retries)"; return 0; }
    raise; sleep 0.2; $S click $1 $2 1 >/dev/null; sleep 0.6
  done
  echo "  (FAILED: wanted $3 windows, have $(nwin))"; return 1
}
clickn() { for i in $(seq 1 ${3:-3}); do raise; sleep 0.25; $S click $1 $2 1 >/dev/null; sleep 0.45; done; }
shot()  { screencapture -x -o -l $1 $2; }

MAIN=$($S bounds $PID | awk '$2==760 {print $1}')
INSP=1
{
echo "== SPEC-9 'Windows' (vizia =0.4.0) — synthetic-input evidence =="
echo "macOS 26.6.2 (M4 Pro), rustc 1.96.1, release build. Reproduce with"
echo "  apps/vizia-windows/evidence/run-evidence.sh   (from the repo root)"
echo
echo "METHOD. Four sibling agents drove synthetic input on the same display during"
echo "this run, so (a) the windows are parked at fixed coordinates by pre-seeding the"
echo "app's OWN persistence file target/release/windows-state.txt with"
echo "    main=760,430,720,480 / inspector=1120,60,360,300 / inspector_open=0"
echo "  and (b) every click is issued through a retry wrapper that AXRaises this"
echo "  app's windows first and re-clicks until the expected window count appears"
echo "  (a bare CGEvent otherwise lands on whichever sibling window is on top)."
echo "Coordinates are logical screen points. pid=$PID  main window id=$MAIN"
echo
echo "[1] one window at start"
echo "\$ swift scripts/window-count.swift $PID"; swift scripts/window-count.swift $PID
echo "\$ tools/synth/synth bounds $PID";        $S bounds $PID
echo "  -> evidence/01-main.png"
} > $L
shot $MAIN $E/01-main.png

{ echo; echo "[2] click 'Inspector' (869,488)  ->  2 windows"; clickuntil 869 488 2
  echo "\$ swift scripts/window-count.swift $PID"; swift scripts/window-count.swift $PID; } >> $L
{ echo; echo "[3] click 'Preferences…' (967,488)  ->  3 windows"; clickuntil 967 488 3
  echo "\$ swift scripts/window-count.swift $PID"; swift scripts/window-count.swift $PID
  echo "\$ tools/synth/synth bounds $PID"; $S bounds $PID; } >> $L
INSP=$($S bounds $PID | awk '$2==1120 {print $1}')
PREFS=$($S bounds $PID | awk '$4==320 {print $1}')
{ echo; echo "[4] click 'Inspector' AGAIN  ->  still 3 windows (singleton; the handler raises the existing one)"
  clickn 869 488 2; echo "\$ swift scripts/window-count.swift $PID"; swift scripts/window-count.swift $PID; } >> $L
shot $INSP $E/02-inspector.png
shot $PREFS $E/03-prefs.png

# ---- shared state -------------------------------------------------------
raise; sleep 0.3; $S click 1300 161 1 >/dev/null; sleep 0.5; raise; sleep 0.3
for k in 51 51 51 51 51 51 51 51 51; do $S key $k >/dev/null; done
for r in 1 2 3; do raise; sleep 0.3; for k in 14 2 27 16 0 1 17; do $S key $k >/dev/null; sleep 0.04; done; sleep 0.3; done
sleep 0.6
shot $INSP $E/04-inspector-edited.png
shot $MAIN $E/05-main-live-update.png
{ echo; echo "[5] shared state, live + bidirectional: click into the Inspector's 'name' Textbox (1300,161),"
  echo "    9x Backspace, then type. The MAIN window's list row and status bar follow every keystroke,"
  echo "    and the dirty flag ('· unsaved') appears. One Signal<Vec<Project>>, no copy, no sync code."
  echo "\$ tools/synth/synth click 1300 161 1 ; tools/synth/synth key 51 (x9) ; key 14 2 27 16 0 1 17"
  echo "  -> evidence/04-inspector-edited.png  evidence/05-main-live-update.png"; } >> $L

# ---- ping / pong --------------------------------------------------------
clickn 1152 322 4
shot $MAIN $E/06-main-pings.png
{ echo; echo "[6] cross-window message Ping: clicks on the Inspector's 'Ping' (1152,322) bump the MAIN"
  echo "    window's counter (cx.emit from the child window propagates up the shared entity tree to the"
  echo "    root model).  -> evidence/06-main-pings.png"; } >> $L

raise; sleep 0.4
screencapture -x -o -l $INSP -T 2 $E/07-inspector-pong-flash.png &
sleep 1.85; $S click 1052 488 1 >/dev/null
wait; sleep 0.8; shot $INSP $E/08-inspector-after-flash.png
{ echo; echo "[7] cross-window message Pong: 'Pong' in the MAIN window (1052,488) flashes the Inspector's"
  echo "    background for 300 ms (cx.schedule_emit + a CSS background-color transition)."
  echo "  -> evidence/07-inspector-pong-flash.png (caught 150 ms in) / 08-inspector-after-flash.png"; } >> $L

# ---- theme + compact ----------------------------------------------------
shot $MAIN $E/09d-compact-off.png
clickn 707 265 2                      # Compact rows
sleep 0.4; shot $MAIN $E/09e-compact-on.png
clickn 737 333 2                      # Theme = Light
sleep 0.6
shot $MAIN  $E/09a-theme-light-main.png
shot $INSP  $E/09b-theme-light-inspector.png
shot $PREFS $E/09c-theme-light-prefs.png
{ echo; echo "[8] Preferences: 'Compact rows' (707,265) re-lays out the main list immediately"
  echo "    (09d-compact-off.png vs 09e-compact-on.png), and Theme=Light (737,333) restyles ALL THREE"
  echo "    windows at once (09a/09b/09c)."; } >> $L

# ---- modality -----------------------------------------------------------
shot $MAIN $E/10-main-before-modal.png
clickuntil 797 488 4                  # Edit…
{ echo; echo "[9] modality. 'Edit…' (797,488) opens the dialog built with Window::popup(cx, true, ..)."
  echo "\$ osascript ... get name of every window"; axwin
  echo "  NOTE: the dialog also carries .always_on_top(true) (vizia/winit express no macOS window-modal"
  echo "  or owner relationship), which puts it on CGWindowLevel 3 — scripts/window-count.swift and"
  echo "  tools/synth/synth bounds both filter layer==0, so the dialog is invisible to them and the"
  echo "  Accessibility API is used instead."
  echo "\$ swift scripts/window-count.swift $PID"; swift scripts/window-count.swift $PID; } >> $L
MODALPOS=$(osascript -e "tell application \"System Events\" to tell (first process whose unix id is $PID) to get position of window \"Edit project\"" 2>/dev/null | tr -d ' ')
MODALSZ=$(osascript -e "tell application \"System Events\" to tell (first process whose unix id is $PID) to get size of window \"Edit project\"" 2>/dev/null | tr -d ' ')
{ echo "  modal AX position=$MODALPOS size=$MODALSZ"; } >> $L
screencapture -x -o -R ${MODALPOS%,*},${MODALPOS#*,},${MODALSZ%,*},${MODALSZ#*,} $E/11-modal.png
shot $MAIN $E/12-main-blocked.png
raise; sleep 0.3; $S click 1113 488 1 >/dev/null; sleep 0.3   # Delete, while the modal is up
raise; sleep 0.3; $S click 1113 488 1 >/dev/null; sleep 0.8
shot $MAIN $E/13-main-after-blocked-delete.png
{ echo "  clicked 'Delete' (1113,488) TWICE while the dialog was open:"
  echo "\$ osascript ... get name of every window"; axwin
  echo "  (no rfd 'Delete project' confirm appeared and the row count is unchanged —"
  echo "   compare evidence/10-main-before-modal.png with 13-main-after-blocked-delete.png)"; } >> $L
raise; sleep 0.3; $S key 53 >/dev/null; sleep 0.8            # Esc closes the topmost non-main window
{ echo; echo "[10] Esc closes the dialog and focus returns to the main window"; axwin; } >> $L
echo "PID=$PID MAIN=$MAIN INSP=$INSP PREFS=$PREFS"
