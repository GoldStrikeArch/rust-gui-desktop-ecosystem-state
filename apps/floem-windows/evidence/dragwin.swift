// Verification helper: drag from (x1,y1) to (x2,y2) with real CGEvents, used
// to move a window by its title bar for the position-persistence check.
//   swiftc -O -o /tmp/dragwin dragwin.swift && /tmp/dragwin X1 Y1 X2 Y2
import CoreGraphics
import Foundation
let a = CommandLine.arguments
guard a.count == 5, let x1 = Double(a[1]), let y1 = Double(a[2]),
      let x2 = Double(a[3]), let y2 = Double(a[4]) else {
    fputs("usage: dragwin X1 Y1 X2 Y2\n", stderr); exit(2)
}
func post(_ t: CGEventType, _ p: CGPoint) {
    CGEvent(mouseEventSource: nil, mouseType: t, mouseCursorPosition: p, mouseButton: .left)?
        .post(tap: .cghidEventTap)
}
post(.mouseMoved, CGPoint(x: x1, y: y1)); usleep(60_000)
post(.leftMouseDown, CGPoint(x: x1, y: y1)); usleep(120_000)
for i in 1...20 {
    let f = Double(i) / 20.0
    post(.leftMouseDragged, CGPoint(x: x1 + (x2 - x1) * f, y: y1 + (y2 - y1) * f))
    usleep(20_000)
}
usleep(120_000)
post(.leftMouseUp, CGPoint(x: x2, y: y2))
print("dragged \(x1),\(y1) -> \(x2),\(y2)")
