import CoreGraphics
import Foundation

guard CommandLine.arguments.count == 2,
      let requestedPID = Int32(CommandLine.arguments[1]) else {
    fputs("usage: window-count PID\n", stderr)
    exit(2)
}

let options: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
guard let windows = CGWindowListCopyWindowInfo(options, kCGNullWindowID)
        as? [[String: Any]] else {
    exit(1)
}

for window in windows {
    guard let ownerPID = window[kCGWindowOwnerPID as String] as? Int32,
          ownerPID == requestedPID,
          let layer = window[kCGWindowLayer as String] as? Int,
          layer == 0,
          let number = window[kCGWindowNumber as String] as? UInt32,
          let boundsDictionary = window[kCGWindowBounds as String]
              as? [String: Any],
          let bounds = CGRect(dictionaryRepresentation:
              boundsDictionary as CFDictionary),
          bounds.width > 1,
          bounds.height > 1 else {
        continue
    }

    let rawTitle = (window[kCGWindowName as String] as? String) ?? ""
    let title = rawTitle
        .replacingOccurrences(of: "\t", with: " ")
        .replacingOccurrences(of: "\n", with: " ")
    print("\(number)\t\(title)")
}
