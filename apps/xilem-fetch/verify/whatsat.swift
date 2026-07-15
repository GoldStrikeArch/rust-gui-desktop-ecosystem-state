import CoreGraphics
import Foundation
let px = Double(CommandLine.arguments[1])!, py = Double(CommandLine.arguments[2])!
let opts: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
let list = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as! [[String: Any]]
for w in list {
    guard let layer = w[kCGWindowLayer as String] as? Int, layer >= 0, layer <= 3,
        let b = w[kCGWindowBounds as String] as? [String: CGFloat] else { continue }
    let r = CGRect(x: b["X"]!, y: b["Y"]!, width: b["Width"]!, height: b["Height"]!)
    if r.contains(CGPoint(x: px, y: py)) {
        print("TOP at point: \(w[kCGWindowOwnerName as String] ?? "?") pid=\(w[kCGWindowOwnerPID as String] ?? "?") bounds=\(r)")
        break
    }
}
