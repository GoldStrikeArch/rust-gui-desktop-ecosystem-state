`allwin.swift` — same query as `scripts/window-count.swift` but prints the
CGWindowLayer too and does **not** filter to layer 0. Needed here because
`WindowLevel::AlwaysOnTop` (this app's modal dialog, and everything under
`WINDOWS_TOP=1`) puts a window at layer 3, where the repo script cannot see it.

    swift verify/allwin.swift <PID>
    # id  layer  x  y  w  h  title

Env vars the app understands (all optional; all documented in FRICTION.md):
  WINDOWS_SELFTEST=1   run the scripted self-test, print SELFTEST DONE, exit 0
  WINDOWS_LOG=1        print STATE / BLOCKED / VETOED / KEY lines to stdout
  WINDOWS_STATE=PATH   persistence file (default: next to the binary)
  WINDOWS_POS=x,y[,w,h]  initial main-window rect when no state file exists
  WINDOWS_TOP=1        inspector/preferences at WindowLevel::AlwaysOnTop
  WINDOWS_SHEET=1      pass the parent window to rfd (macOS sheet experiment)
