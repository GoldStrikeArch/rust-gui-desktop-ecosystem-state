#!/bin/zsh
# VERIFICATION harness — samples CPU% of the tauri-peek process tree,
# including the out-of-process WebKit helpers (WebContent / GPU / Networking)
# that actually do the camera capture + compositing for the getUserMedia path.
# Helpers are XPC services (ppid=1), so they are attributed to the app via
# `launchctl procinfo`'s "responsible pid".
# usage: cpu_sample.sh <app_pid> <n_samples> <interval_s> <label>
set -u
APP_PID=$1; N=${2:-10}; INT=${3:-1}; LABEL=${4:-sample}

# Collect candidate pids: the app, its children, and WebKit helpers whose
# responsible pid is the app.
candidates() {
  echo $APP_PID
  ps -Ao pid,ppid | awk -v p=$APP_PID '$2==p {print $1}'
  for wk in $(ps -Ao pid,comm | grep -i 'com.apple.WebKit' | awk '{print $1}'); do
    rp=$(launchctl procinfo $wk 2>/dev/null | awk '/responsible pid/ {print $NF}')
    [ "$rp" = "$APP_PID" ] && echo $wk
  done
}

PIDS=$(candidates | sort -u | tr '\n' ',' | sed 's/,$//')
echo "[cpu_sample] label=$LABEL app_pid=$APP_PID pids=$PIDS"
ps -o pid,comm -p "$PIDS" 2>/dev/null | sed 's/^/[cpu_sample] proc: /'

TOTALS=()
for i in $(seq 1 $N); do
  # re-resolve pid set each time in case helpers (re)spawn
  PIDS=$(candidates | sort -u | tr '\n' ',' | sed 's/,$//')
  LINE=$(ps -o pid=,pcpu=,rss=,comm= -p "$PIDS" 2>/dev/null)
  TOT=$(echo "$LINE" | awk '{s+=$2} END {printf "%.1f", s}')
  RSS=$(echo "$LINE" | awk '{s+=$3} END {printf "%.1f", s/1024}')
  TOTALS+=($TOT)
  echo "[cpu_sample] $LABEL sample=$i total_cpu_pct=$TOT total_rss_mib=$RSS"
  echo "$LINE" | awk '{printf "[cpu_sample]   pid=%s cpu=%s%% rss_mib=%.1f %s\n", $1, $2, $3/1024, $4}'
  sleep $INT
done
echo -n "[cpu_sample] $LABEL avg_total_cpu_pct="
printf '%s\n' "${TOTALS[@]}" | awk '{s+=$1; n++} END {printf "%.1f\n", s/n}'
