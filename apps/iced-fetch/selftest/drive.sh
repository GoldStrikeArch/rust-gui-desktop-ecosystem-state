#!/bin/bash
# Fetcher self-test driver (SPEC-8). Shared-desktop safe: the frontmost
# window at each target point must be iced-fetch before input is posted.
set -u
cd "$(dirname "$0")"

H=./uihelper
OWNER=iced-fetch

activate() {
  osascript -e "tell application \"System Events\" to set frontmost of process \"$OWNER\" to true" >/dev/null 2>&1
  sleep 0.4
}

guarded() {
  local x=$1 y=$2; shift 2
  for _ in 1 2 3 4 5 6 7 8; do
    activate
    if [ "$($H topat "$x" "$y")" = "$OWNER" ]; then
      "$H" "$@"
      return 0
    fi
    sleep 0.6
  done
  echo "GUARD-FAIL: $OWNER not frontmost at $x,$y (skipping: $*)" >&2
  return 1
}

read -r WID X Y W HGT <<<"$($H bounds $OWNER)"
echo "window id=$WID at $X,$Y ${W}x$HGT" >&2

hx() { echo $((X + $1)); }
hy() { echo $((Y + $1)); }

INPUT_X=$(hx 350); INPUT_Y=$(hy 99)
DL_X=$(hx 61);     DL_Y=$(hy 468)      # Download button / progress row
CANCEL_X=$(hx 640); CANCEL_Y=$(hy 468) # Cancel button (right of the bar)
FLAKY_X=$(hx 60);  FLAKY_Y=$(hy 554)   # Call /flaky button

case "${1:-}" in
  focus)    guarded "$INPUT_X" "$INPUT_Y" click "$INPUT_X" "$INPUT_Y" ;;
  type)     guarded "$INPUT_X" "$INPUT_Y" type "$2" ;;
  clear)    guarded "$INPUT_X" "$INPUT_Y" key delete "${2:-12}" ;;
  download) guarded "$DL_X" "$DL_Y" click "$DL_X" "$DL_Y" ;;
  cancel)   guarded "$CANCEL_X" "$CANCEL_Y" click "$CANCEL_X" "$CANCEL_Y" ;;
  flaky)    guarded "$FLAKY_X" "$FLAKY_Y" click "$FLAKY_X" "$FLAKY_Y" ;;
  clickat)  guarded "$(hx $2)" "$(hy $3)" click "$(hx $2)" "$(hy $3)" ;;
  shot)     screencapture -x -o -l "$WID" "${2:-shot.png}" ;;
esac
