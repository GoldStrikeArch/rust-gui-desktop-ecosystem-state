#!/bin/bash
# Usage: batch.sh <shotname> <drive-args...>  — window-relative coords as WR:x:y tokens
set -e
cd "$(dirname "$0")"
INFO=$(swift wininfo.swift xilem-grid)
[ -z "$INFO" ] && { echo "ABORT: window not found"; exit 3; }
read -r WID WX WY WW WH <<< "$INFO"
TOP=$(swift whatsat.swift $((WX+WW/2)) $((WY+WH/2)))
case "$TOP" in *xilem-grid*) ;; *) echo "ABORT: occluded: $TOP"; exit 4;; esac
SHOT=$1; shift
ARGS=()
for a in "$@"; do
  case "$a" in
    WR:*:*) x=${a#WR:}; y=${x#*:}; x=${x%%:*}; ARGS+=("$((WX+x))" "$((WY+y))");;
    *) ARGS+=("$a");;
  esac
done
swift drive.swift "${ARGS[@]}"
sleep 0.6
screencapture -o -x -l "$WID" "$SHOT"
echo "batch done: $SHOT (win $WX,$WY ${WW}x${WH})"
