import CoreGraphics
import Foundation

let pid = Int32(CommandLine.arguments[1])!
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
for w in list {
    if let owner = w[kCGWindowOwnerPID as String] as? Int32, owner == pid,
       let num = w[kCGWindowNumber as String] as? Int {
        let name = (w[kCGWindowName as String] as? String) ?? "?"
        let bounds = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
        print("\(num)\t\(bounds["Width"] ?? 0)\t\(name)")
    }
}
