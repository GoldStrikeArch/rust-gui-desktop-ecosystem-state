#!/bin/bash
# SPEC-9 verification driver for apps/floem-windows. Run from anywhere:
#     bash apps/floem-windows/evidence/drive.sh
#
# Shared-desktop notes (this is a research machine running ~10 sibling GUI
# apps at once, all driving synthetic input):
#  * every interaction re-reads the live window rect (`synth bounds`) — a
#    sibling's drag moved this window mid-run the first time;
#  * every interaction first single-clicks this window's TITLE BAR, which
#    activates the app and raises the window so the next click is not eaten by
#    an overlapping sibling window;
#  * screenshots are window-scoped (`screencapture -o -l <windowid>`), which
#    works even when the window is occluded.
set -u
cd "$(dirname "$0")/../../.." || exit 1
APP=apps/floem-windows/target/release/floem-windows
EV=apps/floem-windows/evidence
SYNTH=tools/synth/synth
LOG=/tmp/fw-drive.log
RAISE=/tmp/fw-raise            # touch it -> the app raises itself (see main.rs)
raise() { touch $RAISE; sleep 0.35; }

wid()   { swift scripts/window-count.swift "$PID" | grep -F "$1" | head -1 | cut -f1; }
rect()  { $SYNTH bounds "$PID" | awk -v w="$1" '$1==w {print $2, $3, $4, $5}'; }
count() { swift scripts/window-count.swift "$PID"; }
shot()  { screencapture -x -o -l "$1" "$2" && sips -Z 1200 "$2" >/dev/null; }
# hit <windowid> <dx> <dy> [n] — click at (dx,dy) inside that window's frame,
# after raising it by clicking its title bar.
hit() {
  local r x y; raise
  r=$(rect "$1"); x=$(echo "$r"|cut -d' ' -f1); y=$(echo "$r"|cut -d' ' -f2)
  $SYNTH click $((x+$2)) $((y+$3)) "${4:-1}" >/dev/null; sleep 1.2
}
deletes() { grep -c 'toolbar: Delete pressed' $LOG; }
# hit_until <windowid> <dx> <dy> <log-pattern> — retry the click (the desktop
# is contended; a sibling window occasionally eats one) until the app logs it.
hit_until() {
  local before after
  before=$(grep -c "$4" $LOG)
  for _ in 1 2 3 4 5; do
    hit "$1" "$2" "$3"
    after=$(grep -c "$4" $LOG)
    [ "$after" -gt "$before" ] && { echo "    (click landed after retry)"; return 0; }
    sleep 0.6
  done
  echo "    !! click on ($2,$3) never reached the app"; return 1
}

rm -f apps/floem-windows/target/release/windows-layout.txt
rm -f $RAISE
WINDOWS_RAISE_FILE=$RAISE WINDOWS_POS=60,60 $APP > $LOG 2>&1 &
PID=$!; sleep 3
echo "=== pid $PID"
MAIN=$(wid 'Windows (floem)')
echo "--- [1] window-count after launch (expect 1)"; count; echo "main rect: $(rect $MAIN)"
shot $MAIN $EV/01-main.png

echo "--- [2] click Inspector -> window-count (expect 2)"
hit_until $MAIN 106 58 'inspector: opened'; count
INSP=$(wid 'Inspector'); shot $INSP $EV/02-inspector.png

echo "--- [3] click Preferences -> window-count (expect 3)"
hit_until $MAIN 200 58 'prefs: opened'; count
PREF=$(wid 'Preferences'); shot $PREF $EV/03-prefs.png

echo "--- [4] modality: open the Edit dialog (macOS sheet), click Delete on the parent"
hit_until $MAIN 37 58 'modal: opened'; sleep 1.0
echo "window-count with the sheet up:"; count
shot $MAIN $EV/04-sheet.png
echo "Delete presses before: $(deletes)"
R=$(rect $MAIN); MX=$(echo "$R"|cut -d' ' -f1); MY=$(echo "$R"|cut -d' ' -f2)
raise; $SYNTH click $((MX+681)) $((MY+58)) 3 >/dev/null; sleep 1.0
/tmp/clickpid $PID $((MX+681)) $((MY+58)) 3 >/dev/null; sleep 1.0
echo "Delete presses WHILE THE SHEET IS UP: $(deletes)"
shot $MAIN $EV/05-sheet-delete-clicks.png

echo "--- [4b] control: Esc closes the sheet, the SAME click must now fire"
raise; $SYNTH key 53 >/dev/null; sleep 1.2; count
hit_until $MAIN 681 58 'toolbar: Delete pressed'; sleep 1.5
echo "Delete presses AFTER the sheet closed: $(deletes)"
R=$(rect $MAIN); MX=$(echo "$R"|cut -d' ' -f1); MY=$(echo "$R"|cut -d' ' -f2)
screencapture -x -R $((MX-20)),$((MY-20)),780,340 $EV/06-native-confirm.png
sips -Z 1200 $EV/06-native-confirm.png >/dev/null
osascript -e 'tell application "System Events" to keystroke return' >/dev/null 2>&1
sleep 1.2; echo "rows now: $(grep -c 'delete: ' $LOG) delete outcomes logged"; tail -2 $LOG

echo "--- [5] shared state: type into the Inspector Name field, screenshot main"
hit_until $INSP 130 66 'never-matches' >/dev/null; for k in 6 7 6 5; do $SYNTH key $k >/dev/null; sleep 0.1; done; sleep 1.0
shot $MAIN $EV/07-shared-main.png; shot $INSP $EV/07-shared-inspector.png

echo "--- [6] Ping in the inspector increments the main window's counter"
hit_until $INSP 40 260 'ping:'; sleep 0.8
shot $MAIN $EV/08-ping-main.png
grep -E 'ping:' $LOG | tail -2

echo "--- [7] Pong from main flashes the inspector"
raise; R=$(rect $MAIN); MX=$(echo "$R"|cut -d' ' -f1); MY=$(echo "$R"|cut -d' ' -f2)
$SYNTH click $((MX+283)) $((MY+58)) 1 >/dev/null; sleep 0.12
shot $INSP $EV/09-pong-flash.png
grep -E 'pong:' $LOG | tail -2

echo "--- [8] bounds before quit"; $SYNTH bounds $PID; count
cp $LOG $EV/drive-run.log
echo "=== driver finished; app still running as pid $PID"
