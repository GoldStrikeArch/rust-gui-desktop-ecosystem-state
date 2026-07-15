import AppKit
let pid = pid_t(CommandLine.arguments[1])!
guard let app = NSRunningApplication(processIdentifier: pid) else { exit(1) }
print("isHidden=\(app.isHidden) isActive=\(app.isActive)")
if app.isHidden { app.unhide(); usleep(500_000); print("after unhide: isHidden=\(app.isHidden)") }
