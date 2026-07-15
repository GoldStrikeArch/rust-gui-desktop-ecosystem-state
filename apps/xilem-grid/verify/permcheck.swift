import CoreGraphics
import Foundation
import ApplicationServices
let target = CGPoint(x: 500, y: 300)
let ev = CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: target, mouseButton: .left)
ev?.post(tap: .cghidEventTap)
usleep(150_000)
let loc = CGEvent(source: nil)!.location
print("posted to (500,300); now at (\(Int(loc.x)),\(Int(loc.y)))")
print(AXIsProcessTrusted() ? "AX trusted" : "AX NOT trusted")
