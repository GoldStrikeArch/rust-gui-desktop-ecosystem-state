import ApplicationServices
import Foundation

guard CommandLine.arguments.count >= 2, let pid = Int32(CommandLine.arguments[1]) else {
    fputs("usage: axdump PID [maxdepth]\n", stderr); exit(2)
}
let maxDepth = CommandLine.arguments.count >= 3 ? Int(CommandLine.arguments[2])! : 12
let app = AXUIElementCreateApplication(pid)
// Ask the app to turn on its accessibility implementation (Chromium/Electron
// convention; harmless elsewhere).
AXUIElementSetAttributeValue(app, "AXManualAccessibility" as CFString, kCFBooleanTrue)
AXUIElementSetAttributeValue(app, "AXEnhancedUserInterface" as CFString, kCFBooleanTrue)

func str(_ e: AXUIElement, _ attr: String) -> String? {
    var v: CFTypeRef?
    guard AXUIElementCopyAttributeValue(e, attr as CFString, &v) == .success else { return nil }
    if let s = v as? String { return s }
    if let n = v as? NSNumber { return n.stringValue }
    if let b = v as? Bool { return b ? "true" : "false" }
    if let v = v { return String(describing: v) }
    return nil
}

func children(_ e: AXUIElement) -> [AXUIElement] {
    var v: CFTypeRef?
    guard AXUIElementCopyAttributeValue(e, kAXChildrenAttribute as CFString, &v) == .success,
          let arr = v as? [AXUIElement] else { return [] }
    return arr
}

var count = 0
func dump(_ e: AXUIElement, _ depth: Int) {
    if depth > maxDepth { return }
    count += 1
    let pad = String(repeating: "  ", count: depth)
    let role = str(e, kAXRoleAttribute as String) ?? "?"
    var line = "\(pad)\(role)"
    for a in [kAXSubroleAttribute as String, kAXTitleAttribute as String,
              kAXDescriptionAttribute as String, kAXValueAttribute as String,
              kAXHelpAttribute as String, kAXPlaceholderValueAttribute as String] {
        if let s = str(e, a), !s.isEmpty {
            line += "  \(a.replacingOccurrences(of: "AX", with: ""))=\(s.prefix(70).replacingOccurrences(of: "\n", with: " "))"
        }
    }
    print(line)
    for c in children(e) { dump(c, depth + 1) }
}

// Wait a moment: some adapters build the tree lazily on the first request.
_ = children(app)
Thread.sleep(forTimeInterval: 1.0)
dump(app, 0)
fputs("nodes: \(count)\n", stderr)
