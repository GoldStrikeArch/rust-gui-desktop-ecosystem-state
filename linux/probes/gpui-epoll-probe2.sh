#!/usr/bin/env bash
# gpui-epoll-probe2.sh — working replacement for the failed first epoll probe
# (that one used awk strtonum, absent in Debian mawk, and grepped the fdinfo
# DIRECTORY instead of the per-fd files; it enumerated nothing).
#
# Question under test: is gpui-app's X11 socket fd registered in any epoll
# set that the process actually waits on? (The "event loop is deaf"
# hypothesis for the gpui black-window defect.)
#
# Usage (host, from repo root):
#   docker run --rm -v "$PWD":/work -v rcn-cargo-target:/cargo-target \
#     -v rcn-cargo-home:/cargo-home rcn-linux-check:latest \
#     bash /work/linux/probes/gpui-epoll-probe2.sh
#
# Runtime-only apt installs (image/Dockerfile deliberately NOT modified):
#   strace gawk xdotool x11-apps iproute2 x11-xserver-utils
#   (iproute2 provides ss; x11-xserver-utils provides xrefresh)
#   — documented in gpui-epoll-probe2-apt.txt
#
# Outputs (all under linux-results/):
#   gpui-epoll-probe2-run.txt        combined run log + verdict data
#   gpui-epoll-probe2-apt.txt        runtime apt install log
#   gpui-epoll-probe2-app.txt        gpui-app stdout/stderr
#   gpui-epoll-probe2-fds.txt        ls -l /proc/PID/fd
#   gpui-epoll-probe2-connect.txt    launch-phase strace of socket/connect
#                                    (definitive fd -> X11/dbus map)
#   gpui-epoll-probe2-ss.txt         ss -xp + /proc/net/unix (X-socket map)
#   gpui-epoll-probe2-fdinfo.txt     fdinfo of EVERY eventpoll fd (tfd lists)
#   gpui-epoll-probe2-threads.txt    per-thread comm/wchan before strace
#   gpui-epoll-probe2-strace.txt     6s strace -f during stimulus window
#   gpui-epoll-probe2-xwininfo.txt   window tree + state
#   gpui-epoll-probe2-screen.png     root-window screenshot (black check)

set -u
OUT=/work/linux-results
mkdir -p "$OUT"
LOG="$OUT/gpui-epoll-probe2-run.txt"
: > "$LOG"
log() { printf '%s\n' "$*" | tee -a "$LOG"; }

log "== gpui-epoll-probe2 =="
log "date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
log "kernel: $(uname -a)"

log ""
log "-- runtime tool install (not in image; see gpui-epoll-probe2-apt.txt) --"
{ apt-get update && apt-get install -y --no-install-recommends strace gawk xdotool x11-apps iproute2 x11-xserver-utils; } \
  > "$OUT/gpui-epoll-probe2-apt.txt" 2>&1
log "apt exit=$?  strace=$(command -v strace || echo MISSING)  gawk=$(command -v gawk || echo MISSING)  xdotool=$(command -v xdotool || echo MISSING)  xrefresh=$(command -v xrefresh || echo MISSING)  ss=$(command -v ss || echo MISSING)"

# --- 1. Xvfb + dbus + launch app ---------------------------------------
export DISPLAY=:99
Xvfb :99 -screen 0 1280x800x24 > /dev/null 2>&1 &
XVFB_PID=$!
sleep 1
xdpyinfo -display :99 > /dev/null 2>&1 || { log "XVFB_FAILED"; exit 2; }
eval "$(dbus-launch --sh-syntax 2>/dev/null)" || true
log "DISPLAY=$DISPLAY  Xvfb pid=$XVFB_PID  dbus=${DBUS_SESSION_BUS_ADDRESS:-<none>}"

BIN=/cargo-target/gpui-app/release/gpui-app
[ -x "$BIN" ] || { log "NO_BINARY $BIN"; exit 1; }
# Launch under a short-lived strace tracing socket()+connect() so every fd
# that connects to /tmp/.X11-unix/X99 (or the dbus socket) is mapped at
# creation time. Detach gotchas (both verified in this container):
#   - `timeout N strace ... CMD` signals the whole process GROUP -> the app
#     itself gets SIGTERM and dies;
#   - `kill -TERM <strace>` is IGNORED when strace spawned the command
#     (strace ignores TERM so ^C-style signals reach the tracee), so strace
#     stays attached and the later diagnostic attach fails with EPERM.
# SIGKILLing strace works: the kernel auto-detaches its tracees, the app
# keeps running (TracerPid returns to 0), and a later attach succeeds.
strace -f -tt -e trace=socket,connect \
  -o "$OUT/gpui-epoll-probe2-connect.txt" \
  "$BIN" > "$OUT/gpui-epoll-probe2-app.txt" 2>&1 &
LAUNCH_STRACE_PID=$!
sleep 2
PID=$(pgrep -x gpui-app | head -1)
[ -n "${PID:-}" ] || { log "APP_PID_NOT_FOUND"; tee -a "$LOG" < "$OUT/gpui-epoll-probe2-app.txt"; exit 1; }
log "gpui-app PID=$PID  (launch strace pid=$LAUNCH_STRACE_PID; waiting to 8s for steady state)"
sleep 6
kill -KILL "$LAUNCH_STRACE_PID" 2>/dev/null   # kernel detaches tracees; app survives
sleep 1
kill -0 "$PID" 2>/dev/null || { log "APP_DIED_EARLY"; tee -a "$LOG" < "$OUT/gpui-epoll-probe2-app.txt"; exit 1; }
for i in 1 2 3 4 5; do
  TRACER=$(gawk '/^TracerPid:/ {print $2}' "/proc/$PID/status" 2>/dev/null)
  [ "${TRACER:-0}" = "0" ] && break
  sleep 1
done
log "TracerPid after launch-strace kill: ${TRACER:-?} (must be 0 for the later attach)"
log "-- launch-phase connect() calls (verbatim) --"
grep -E 'connect\(' "$OUT/gpui-epoll-probe2-connect.txt" | tee -a "$LOG"
XFDS_CONNECT=$(grep -E 'connect\([0-9]+, \{sa_family=AF_UNIX, sun_path=@?"/tmp/\.X11-unix/X99"' \
  "$OUT/gpui-epoll-probe2-connect.txt" | grep -Eo 'connect\([0-9]+' | grep -Eo '[0-9]+' | sort -un | tr '\n' ' ')
log "fds that connect()ed to X99 at launch: ${XFDS_CONNECT:-<none>}"

# --- 2. fd enumeration ---------------------------------------------------
ls -l "/proc/$PID/fd" > "$OUT/gpui-epoll-probe2-fds.txt" 2>&1

EPFDS=""; SOCKMAP=""   # SOCKMAP entries: fd:inode
for f in /proc/$PID/fd/*; do
  fd=${f##*/}
  tgt=$(readlink "$f") || continue
  case "$tgt" in
    'anon_inode:[eventpoll]') EPFDS="$EPFDS $fd" ;;
    socket:\[*\]) inode=${tgt#socket:\[}; inode=${inode%\]}; SOCKMAP="$SOCKMAP $fd:$inode" ;;
  esac
done
log ""
log "eventpoll fds:$EPFDS"
log "socket fds (fd:inode):$SOCKMAP"

# --- map sockets to the X11 connection via ss -xp -----------------------
{ echo "== ss -xp =="; ss -xp; echo; echo "== /proc/net/unix =="; cat /proc/net/unix; } \
  > "$OUT/gpui-epoll-probe2-ss.txt" 2>&1

# X-server-side ESTAB rows carry path /tmp/.X11-unix/X99; their peer inode
# (field 8) is the CLIENT-side inode, i.e. gpui-app's end of the connection.
XCLIENT_INODES=$(gawk '$2=="ESTAB" && $5 ~ /\.X11-unix\/X99$/ { print $8 }' \
  "$OUT/gpui-epoll-probe2-ss.txt" | sort -u)
log "X-client-side socket inodes (peers of Xvfb :99):"
log "$XCLIENT_INODES"

XFDS=""
for pair in $SOCKMAP; do
  fd=${pair%%:*}; inode=${pair##*:}
  for xi in $XCLIENT_INODES; do
    [ "$inode" = "$xi" ] && XFDS="$XFDS $fd"
  done
done
log "=> gpui-app X11-socket fds (via ss peer map):$XFDS"
if [ -z "$XFDS" ] && [ -n "${XFDS_CONNECT:-}" ]; then
  XFDS=" ${XFDS_CONNECT% }"
  log "   (ss map empty — falling back to launch connect() trace)"
fi
log "=> X fds used for verdict:${XFDS:-<none>}"
log "   cross-check, connect() trace said:${XFDS_CONNECT:+ }${XFDS_CONNECT:-<none>}"
[ -n "$XFDS" ] || log "WARNING: no X socket fd identified — verdict impossible"

# --- 3. fdinfo of EVERY eventpoll fd (tfd: lines = registrations) -------
: > "$OUT/gpui-epoll-probe2-fdinfo.txt"
for ep in $EPFDS; do
  {
    echo "===== /proc/$PID/fdinfo/$ep (eventpoll) ====="
    cat "/proc/$PID/fdinfo/$ep"
    echo
  } >> "$OUT/gpui-epoll-probe2-fdinfo.txt" 2>&1
done
log ""
log "-- fdinfo tfd registrations per eventpoll fd (full dumps in gpui-epoll-probe2-fdinfo.txt) --"
grep -E '^(=====|tfd:)' "$OUT/gpui-epoll-probe2-fdinfo.txt" | tee -a "$LOG"

# --- 4. VERDICT DATA: X fd in any epoll set? -----------------------------
log ""
log "-- membership matrix (X fd vs eventpoll tfd lists) --"
for ep in $EPFDS; do
  TFDS=$(gawk '$1=="tfd:" { print $2 }' "/proc/$PID/fdinfo/$ep" | sort -n | tr '\n' ' ')
  log "epoll fd $ep registers tfds: ${TFDS:-<none>}"
  for x in $XFDS; do
    hit=no
    for t in $TFDS; do [ "$t" = "$x" ] && hit=YES; done
    log "  X fd $x in epoll $ep: $hit"
  done
done

# --- per-thread wait states BEFORE strace perturbs anything --------------
{
  echo "== threads of $PID: tid comm state wchan =="
  for t in /proc/$PID/task/*; do
    tid=${t##*/}
    comm=$(cat "$t/comm" 2>/dev/null)
    state=$(gawk '/^State:/ {print $2}' "$t/status" 2>/dev/null)
    wchan=$(cat "$t/wchan" 2>/dev/null)
    printf '%s\t%s\t%s\t%s\n' "$tid" "$comm" "$state" "$wchan"
  done
} > "$OUT/gpui-epoll-probe2-threads.txt" 2>&1
log ""
log "-- thread wait channels --"
tee -a "$LOG" < "$OUT/gpui-epoll-probe2-threads.txt"

# --- 5. behavioral cross-check: strace during X-event stimulus ----------
log ""
log "-- strace window (6s) with X stimulus --"
WID=$(xdotool search --name 'Tasks' 2>/dev/null | head -1)
log "app window id: ${WID:-<not found>}"
timeout 6 strace -f -tt -p "$PID" \
  -e trace=epoll_wait,epoll_pwait,epoll_pwait2,epoll_ctl,ppoll,poll,pselect6,select,recvmsg,recvfrom,read,readv,write,writev,sendmsg,sendto \
  -o "$OUT/gpui-epoll-probe2-strace.txt" 2> "$OUT/gpui-epoll-probe2-strace-err.txt" &
STRACE_JOB=$!
sleep 1
# stimulus: Expose + pointer + key + resize (each generates X events)
xrefresh -display :99 2>/dev/null || true
if [ -n "${WID:-}" ]; then
  xdotool windowfocus "$WID" 2>/dev/null || true
  xdotool mousemove 640 400 click 1 2>/dev/null || true
  xdotool key --window "$WID" a 2>/dev/null || true
  xdotool windowsize "$WID" 600 700 2>/dev/null || true
fi
xrefresh -display :99 2>/dev/null || true
wait "$STRACE_JOB" 2>/dev/null
STRACE_RC=$?
log "strace exit=$STRACE_RC (124=timeout=expected)  stderr: $(head -3 "$OUT/gpui-epoll-probe2-strace-err.txt" | tr '\n' ' ')"
log "strace lines: $(wc -l < "$OUT/gpui-epoll-probe2-strace.txt")"

log ""
log "-- strace analysis --"
log "epoll fds actually waited on (epoll_wait/epoll_pwait first arg, count):"
grep -Eo 'epoll_p?wait2?\([0-9]+' "$OUT/gpui-epoll-probe2-strace.txt" \
  | grep -Eo '[0-9]+$' | sort | uniq -c | tee -a "$LOG"
for x in $XFDS; do
  log "reads/recvs on X fd $x: $(grep -Ec "(read|readv|recvmsg|recvfrom)\($x," "$OUT/gpui-epoll-probe2-strace.txt")"
  log "writes/sends on X fd $x: $(grep -Ec "(write|writev|sendmsg|sendto)\($x," "$OUT/gpui-epoll-probe2-strace.txt")"
  log "poll/ppoll mentioning X fd $x: $(grep -E 'p?poll\(' "$OUT/gpui-epoll-probe2-strace.txt" | grep -c "fd=$x")"
  log "epoll_ctl on X fd $x during window: $(grep -Ec "epoll_ctl\([0-9]+, EPOLL_CTL_[A-Z]+, $x," "$OUT/gpui-epoll-probe2-strace.txt")"
done
log "sample epoll_wait lines:"
grep -E 'epoll_p?wait' "$OUT/gpui-epoll-probe2-strace.txt" | head -5 | tee -a "$LOG"
log "sample X-fd read lines (if any):"
for x in $XFDS; do
  grep -E "(read|readv|recvmsg|recvfrom)\($x," "$OUT/gpui-epoll-probe2-strace.txt" | head -3 | tee -a "$LOG"
done

log ""
log "-- X protocol forensics on the traced payloads --"
# Outbound: X11 PutImage requests are opcode 0x48 ('H'); with BIG-REQUESTS the
# 28-byte header is H,format,len=0,extlen,drawable,gc,w,h,x,y,leftpad,DEPTH,pad.
for x in $XFDS; do
  log "PutImage submissions on X fd $x (writev iov starting opcode 'H'=0x48): $(grep -cE "writev\($x, \[\{iov_base=\"H" "$OUT/gpui-epoll-probe2-strace.txt")"
  log "sample PutImage header+pixels (byte 21 of header = depth):"
  grep -E "writev\($x, \[\{iov_base=\"H" "$OUT/gpui-epoll-probe2-strace.txt" | head -1 | tee -a "$LOG"
done
# Inbound: X11 error packets start with byte 0x00; byte1 = error code
# (0x08 = "\10" = BadMatch), byte 10 = failing major opcode ('H' = 72 = PutImage).
for x in $XFDS; do
  log "X ERROR packets received on fd $x: $(grep -E "recvmsg\($x," "$OUT/gpui-epoll-probe2-strace.txt" | grep -cF 'iov_base="\0')"
  # strace renders byte 0x08 as \10, or \010 when the next char is a digit
  log "  of which BadMatch (code 8): $(grep -E "recvmsg\($x," "$OUT/gpui-epoll-probe2-strace.txt" | grep -cF -e 'iov_base="\0\10' -e 'iov_base="\0\010')"
  log "  sample error packets (byte1=\\10 BadMatch ... byte10=H i.e. PutImage):"
  grep -E "recvmsg\($x," "$OUT/gpui-epoll-probe2-strace.txt" | grep -F 'iov_base="\0' | head -2 | tee -a "$LOG"
done

# --- re-dump fdinfo after stimulus (did registrations change?) ----------
log ""
log "-- fdinfo tfd lists AFTER stimulus --"
for ep in $EPFDS; do
  TFDS=$(gawk '$1=="tfd:" { print $2 }' "/proc/$PID/fdinfo/$ep" 2>/dev/null | sort -n | tr '\n' ' ')
  log "epoll fd $ep tfds now: ${TFDS:-<none>}"
done

# --- 6. screenshot + window state ---------------------------------------
xwininfo -display :99 -root -tree > "$OUT/gpui-epoll-probe2-xwininfo.txt" 2>&1
[ -n "${WID:-}" ] && xwininfo -display :99 -id "$WID" >> "$OUT/gpui-epoll-probe2-xwininfo.txt" 2>&1
import -display :99 -window root "$OUT/gpui-epoll-probe2-screen.png" 2>>"$LOG" || log "SCREENSHOT_FAIL"
log ""
log "-- screenshot pixel stats (mean/stddev; 0 0 = pure black) --"
identify -format 'mean=%[fx:mean] stddev=%[fx:standard_deviation]\n' \
  "$OUT/gpui-epoll-probe2-screen.png" 2>/dev/null | tee -a "$LOG"

log ""
log "app still alive: $(kill -0 "$PID" 2>/dev/null && echo yes || echo no)"
log "-- gpui-app output (verbatim) --"
tee -a "$LOG" < "$OUT/gpui-epoll-probe2-app.txt"

kill "$PID" 2>/dev/null; sleep 1; kill -9 "$PID" 2>/dev/null
kill "$XVFB_PID" 2>/dev/null
log ""
log "== probe complete =="
