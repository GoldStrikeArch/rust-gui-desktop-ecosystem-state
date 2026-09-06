// Deterministic input for a CROWDED desktop (several agents drive synthetic
// input on the same display concurrently, so `tools/synth/synth click` — which
// posts to the HID tap — lands on whichever app happens to be on top).
// CGEventPostToPid delivers the event straight to one process' event queue, so
// the target app hit-tests it against its OWN windows and z-order stops
// mattering.
//
//   swiftc -O -o postclick postclick.swift
//   ./postclick <pid> click <x> <y> [count]
//   ./postclick <pid> key <keycode> [cmd] [shift]
import CoreGraphics
import Foundation

let a = CommandLine.arguments
func usage() -> Never { fputs("usage: postclick PID click X Y [COUNT] | postclick PID key CODE [cmd] [shift]\n", stderr); exit(2) }
guard a.count >= 3, let pid = Int32(a[1]) else { usage() }

switch a[2] {
case "click":
    guard a.count >= 5, let x = Double(a[3]), let y = Double(a[4]) else { usage() }
    let count = a.count >= 6 ? (Int(a[5]) ?? 1) : 1
    let p = CGPoint(x: x, y: y)
    let mv = CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: p, mouseButton: .left)!
    mv.postToPid(pid); usleep(120_000)
    for i in 1...count {
        let d = CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown, mouseCursorPosition: p, mouseButton: .left)!
        d.setIntegerValueField(.mouseEventClickState, value: Int64(i)); d.postToPid(pid); usleep(50_000)
        let u = CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp, mouseCursorPosition: p, mouseButton: .left)!
        u.setIntegerValueField(.mouseEventClickState, value: Int64(i)); u.postToPid(pid)
        if i < count { usleep(120_000) }
    }
    print("posted \(count) click(s) at \(Int(x)),\(Int(y)) to pid \(pid)")
case "key":
    guard a.count >= 4, let code = UInt16(a[3]) else { usage() }
    var flags = CGEventFlags()
    if a.contains("cmd") { flags.insert(.maskCommand) }
    if a.contains("shift") { flags.insert(.maskShift) }
    let d = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true)!
    let u = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false)!
    d.flags = flags; u.flags = flags
    d.postToPid(pid); usleep(30_000); u.postToPid(pid)
    print("posted key \(code) flags=\(flags.rawValue) to pid \(pid)")
default: usage()
}
