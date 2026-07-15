#!/bin/bash
# Grid self-test driver (SPEC-7). Shared-desktop safe: before every input
# burst the frontmost window at the target point must be iced-grid, else the
# step is retried after re-activating (other agents/apps share this desktop).
set -u
cd "$(dirname "$0")"

H=./uihelper
OWNER=iced-grid

activate() {
  osascript -e "tell application \"System Events\" to set frontmost of process \"$OWNER\" to true" >/dev/null 2>&1
  sleep 0.4
}

# guarded <x> <y> <cmd...>  — activate + verify top window at point, then run
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

hx() { echo $((X + $1)); }   # in-window x -> global
hy() { echo $((Y + $1)); }   # in-window y -> global

HEADER_Y=$(hy 98)
ROW0_Y=$(hy 126)

case "${1:-all}" in
  filter)
    guarded "$(hx 160)" "$(hy 57)" click "$(hx 160)" "$(hy 57)"
    guarded "$(hx 160)" "$(hy 57)" type "bris"
    ;;
  clear)
    guarded "$(hx 160)" "$(hy 57)" click "$(hx 160)" "$(hy 57)"
    guarded "$(hx 160)" "$(hy 57)" key delete 8
    ;;
  sortname)   guarded "$(hx 200)" "$HEADER_Y" click "$(hx 200)" "$HEADER_Y" ;;
  sortid)     guarded "$(hx 45)"  "$HEADER_Y" click "$(hx 45)"  "$HEADER_Y" ;;
  sortvalue)  guarded "$(hx 495)" "$HEADER_Y" click "$(hx 495)" "$HEADER_Y" ;;
  row)        # $2 = visible row index
    y=$((ROW0_Y + 26 * ${2:-0}))
    guarded "$(hx 400)" "$y" click "$(hx 400)" "$y"
    ;;
  shiftrow)
    y=$((ROW0_Y + 26 * ${2:-0}))
    guarded "$(hx 400)" "$y" click "$(hx 400)" "$y" shift
    ;;
  resize)     # drag divider after id column (x = 10+70+3 = 83) right by 60
    guarded "$(hx 83)" "$HEADER_Y" drag "$(hx 83)" "$HEADER_Y" "$(hx 143)" "$HEADER_Y"
    ;;
  scroll)     # $2 = dy per event, $3 = count
    guarded "$(hx 400)" "$(hy 350)" scroll "$(hx 400)" "$(hy 350)" "${2:--120}" "${3:-100}"
    ;;
  shot)
    screencapture -x -o -l "$WID" "${2:-shot.png}"
    ;;
esac
