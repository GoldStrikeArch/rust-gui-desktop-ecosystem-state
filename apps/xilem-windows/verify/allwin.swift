import CoreGraphics
import Foundation
let pid = Int32(CommandLine.arguments[1])!
let opts: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
guard let ws = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] else { exit(1) }
for w in ws {
    guard let p = w[kCGWindowOwnerPID as String] as? Int32, p == pid,
          let n = w[kCGWindowNumber as String] as? UInt32,
          let l = w[kCGWindowLayer as String] as? Int,
          let bd = w[kCGWindowBounds as String] as? [String: Any],
          let b = CGRect(dictionaryRepresentation: bd as CFDictionary),
          b.width > 1, b.height > 1 else { continue }
    let t = (w[kCGWindowName as String] as? String) ?? ""
    print("\(n)\t\(l)\t\(Int(b.origin.x))\t\(Int(b.origin.y))\t\(Int(b.width))\t\(Int(b.height))\t\(t)")
}
