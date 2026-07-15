// Print "windowid x y w h" for the first on-screen window owned by the
// process name given as argv[1] (e.g. "xilem-grid").
import CoreGraphics
import Foundation

let target = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "xilem-grid"
let opts: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
guard let list = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] else {
    exit(2)
}
for w in list {
    guard let owner = w[kCGWindowOwnerName as String] as? String, owner == target,
        let bounds = w[kCGWindowBounds as String] as? [String: CGFloat],
        let num = w[kCGWindowNumber as String] as? Int,
        let layer = w[kCGWindowLayer as String] as? Int, layer >= 0, layer <= 3
    else { continue }
    print("\(num) \(Int(bounds["X"]!)) \(Int(bounds["Y"]!)) \(Int(bounds["Width"]!)) \(Int(bounds["Height"]!))")
    exit(0)
}
exit(1)
