// Verification helper for a SHARED desktop: posts a synthetic left click to a
// specific process (CGEventPostToPid) instead of to whatever window happens to
// be topmost. Sibling research apps run concurrently and overlap this app's
// window, so tools/synth/synth's screen-point clicks land on their windows.
// The event still enters the target app through NSApplication -sendEvent:, so
// AppKit's modal-session filtering (the thing SPEC-9 measures) still applies.
//   swiftc -O -o /tmp/clickpid clickpid.swift
//   /tmp/clickpid <PID> <X> <Y> [COUNT]
import CoreGraphics
import Foundation

let a = CommandLine.arguments
guard a.count >= 4, let pid = pid_t(a[1]), let x = Double(a[2]), let y = Double(a[3]) else {
    fputs("usage: clickpid PID X Y [COUNT]\n", stderr); exit(2)
}
let count = a.count > 4 ? Int(a[4])! : 1
let p = CGPoint(x: x, y: y)
for i in 1...count {
    guard let move = CGEvent(mouseEventSource: nil, mouseType: .mouseMoved,
                             mouseCursorPosition: p, mouseButton: .left),
          let down = CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown,
                             mouseCursorPosition: p, mouseButton: .left),
          let up = CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp,
                           mouseCursorPosition: p, mouseButton: .left) else { exit(1) }
    down.setIntegerValueField(.mouseEventClickState, value: Int64(i))
    up.setIntegerValueField(.mouseEventClickState, value: Int64(i))
    move.postToPid(pid); usleep(20_000)
    down.postToPid(pid); usleep(40_000)
    up.postToPid(pid); usleep(90_000)
}
print("clicked \(count)x at \(x),\(y) -> pid \(pid)")
