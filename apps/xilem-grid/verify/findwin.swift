import CoreGraphics
import Foundation
let target = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "xilem-grid"
for (name, opts) in [("onscreen", CGWindowListOption([.optionOnScreenOnly])), ("all", CGWindowListOption([.optionAll]))] {
    let list = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as! [[String: Any]]
    for w in list where (w[kCGWindowOwnerName as String] as? String) == target {
        let layer = w[kCGWindowLayer as String] as? Int ?? -99
        let b = w[kCGWindowBounds as String] as? [String: CGFloat] ?? [:]
        let alpha = w[kCGWindowAlpha as String] as? Double ?? -1
        let onscreen = w[kCGWindowIsOnscreen as String] as? Bool ?? false
        print("\(name): layer=\(layer) onscreen=\(onscreen) alpha=\(alpha) x=\(Int(b["X"] ?? -1)) y=\(Int(b["Y"] ?? -1)) w=\(Int(b["Width"] ?? -1)) h=\(Int(b["Height"] ?? -1)) id=\(w[kCGWindowNumber as String] as? Int ?? -1)")
    }
}
