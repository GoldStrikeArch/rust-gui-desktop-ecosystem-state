import CoreGraphics
import Foundation
let opts: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
let list = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as! [[String: Any]]
for w in list {
    let owner = w[kCGWindowOwnerName as String] as? String ?? "?"
    let layer = w[kCGWindowLayer as String] as? Int ?? -99
    let b = w[kCGWindowBounds as String] as? [String: CGFloat] ?? [:]
    if layer >= -1 && layer <= 5 {
        print("\(owner) layer=\(layer) x=\(Int(b["X"] ?? -1)) y=\(Int(b["Y"] ?? -1)) w=\(Int(b["Width"] ?? -1)) h=\(Int(b["Height"] ?? -1))")
    }
}
