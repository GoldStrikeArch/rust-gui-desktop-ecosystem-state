import AppKit
let pid = pid_t(CommandLine.arguments[1])!
guard let app = NSRunningApplication(processIdentifier: pid) else { exit(1) }
app.activate(options: [.activateIgnoringOtherApps])
usleep(400_000)
print("activated \(pid), frontmost=\(app.isActive)")
