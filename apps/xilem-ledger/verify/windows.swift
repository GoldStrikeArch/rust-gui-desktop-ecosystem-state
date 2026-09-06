// Like scripts/window-count.swift but without the `layer == 0` filter, so it
// also finds the window while LEDGER_TOPMOST=1 raises it above other research
// apps that were covering it during scripted runs.
// Usage: swift verify/windows.swift <PID>   ->  windowid<TAB>x y w h<TAB>layer<TAB>title
import CoreGraphics
import Foundation

guard CommandLine.arguments.count == 2, let pid = Int32(CommandLine.arguments[1]) else {
    fputs("usage: windows PID\n", stderr); exit(2)
}
let opts: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
guard let wins = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] else { exit(1) }
for w in wins {
    guard let owner = w[kCGWindowOwnerPID as String] as? Int32, owner == pid,
          let num = w[kCGWindowNumber as String] as? UInt32,
          let layer = w[kCGWindowLayer as String] as? Int,
          let bd = w[kCGWindowBounds as String] as? [String: Any],
          let b = CGRect(dictionaryRepresentation: bd as CFDictionary),
          b.width > 1, b.height > 1 else { continue }
    let title = (w[kCGWindowName as String] as? String) ?? ""
    print("\(num)\t\(Int(b.minX)) \(Int(b.minY)) \(Int(b.width)) \(Int(b.height))\t\(layer)\t\(title)")
}
