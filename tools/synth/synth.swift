import CoreGraphics
import Foundation

// usage: synth bounds PID | synth click X Y COUNT [GAP_MS] | synth key KEYCODE [shift]
let args = CommandLine.arguments
func usage() -> Never { fputs("usage: synth bounds PID | click X Y COUNT [GAP_MS] | key KEYCODE [shift]\n", stderr); exit(2) }
guard args.count >= 2 else { usage() }

switch args[1] {
case "bounds":
    guard args.count == 3, let pid = Int32(args[2]) else { usage() }
    let opts: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
    guard let windows = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] else { exit(1) }
    for w in windows {
        guard let owner = w[kCGWindowOwnerPID as String] as? Int32, owner == pid,
              let layer = w[kCGWindowLayer as String] as? Int, layer == 0,
              let num = w[kCGWindowNumber as String] as? UInt32,
              let bd = w[kCGWindowBounds as String] as? [String: Any],
              let b = CGRect(dictionaryRepresentation: bd as CFDictionary), b.width > 1, b.height > 1 else { continue }
        print("\(num) \(Int(b.origin.x)) \(Int(b.origin.y)) \(Int(b.width)) \(Int(b.height))")
    }
case "click":
    guard args.count >= 5, let x = Double(args[2]), let y = Double(args[3]), let count = Int(args[4]) else { usage() }
    let gap = args.count >= 6 ? (Double(args[5]) ?? 80) : 80
    let p = CGPoint(x: x, y: y)
    let move = CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: p, mouseButton: .left)!
    move.post(tap: .cghidEventTap); usleep(120_000)
    for i in 1...count {
        let down = CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown, mouseCursorPosition: p, mouseButton: .left)!
        down.setIntegerValueField(.mouseEventClickState, value: Int64(i))
        down.post(tap: .cghidEventTap); usleep(40_000)
        let up = CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp, mouseCursorPosition: p, mouseButton: .left)!
        up.setIntegerValueField(.mouseEventClickState, value: Int64(i))
        up.post(tap: .cghidEventTap)
        if i < count { usleep(UInt32(gap * 1000)) }
    }
    print("clicked \(count)x at \(Int(x)),\(Int(y)) gap=\(Int(gap))ms")
case "key":
    guard args.count >= 3, let code = UInt16(args[2]) else { usage() }
    let shift = args.count >= 4 && args[3] == "shift"
    let d = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true)!
    let u = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false)!
    if shift { d.flags = .maskShift; u.flags = .maskShift }
    d.post(tap: .cghidEventTap); usleep(30_000); u.post(tap: .cghidEventTap)
default: usage()
}
