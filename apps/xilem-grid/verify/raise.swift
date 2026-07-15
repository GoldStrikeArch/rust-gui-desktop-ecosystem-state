// Raise + activate our own app's window (AX API, needs accessibility trust).
import AppKit
import ApplicationServices
let pid = pid_t(CommandLine.arguments[1])!
let axApp = AXUIElementCreateApplication(pid)
var windows: CFTypeRef?
guard AXUIElementCopyAttributeValue(axApp, kAXWindowsAttribute as CFString, &windows) == .success,
      let list = windows as? [AXUIElement], let win = list.first else {
    print("no AX window"); exit(1)
}
AXUIElementPerformAction(win, kAXRaiseAction as CFString)
NSRunningApplication(processIdentifier: pid)?.activate(options: [.activateAllWindows])
usleep(400_000)
let active = NSRunningApplication(processIdentifier: pid)?.isActive ?? false
print("raised; active=\(active)")
