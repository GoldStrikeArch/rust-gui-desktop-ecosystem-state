// Synthetic-input driver (CGEvent) for scripted UI verification.
// All coordinates are GLOBAL screen points (caller adds the window origin).
// Commands (repeatable, processed left to right):
//   click X Y [shift|cmd]      primary click, optional modifier held
//   drag X1 Y1 X2 Y2           press, move in steps, release
//   move X Y                   move pointer
//   scroll X Y DY N            move pointer to X,Y then post N pixel-scroll
//                              events of DY each (negative = scroll down)
//   type STRING                type text into the focused widget
//   key CODE                   press+release a virtual keycode (51=delete)
//   sleep MS                   pause
import CoreGraphics
import Foundation

func post(_ ev: CGEvent?) {
    ev?.post(tap: .cghidEventTap)
    usleep(20_000)
}

func mouse(_ type: CGEventType, _ p: CGPoint, flags: CGEventFlags = []) {
    let ev = CGEvent(
        mouseEventSource: nil, mouseType: type, mouseCursorPosition: p,
        mouseButton: .left)
    ev?.flags = flags
    post(ev)
}

var args = Array(CommandLine.arguments.dropFirst())
func next() -> String { args.removeFirst() }
func nextD() -> Double { Double(next())! }

while !args.isEmpty {
    let cmd = next()
    switch cmd {
    case "click":
        let p = CGPoint(x: nextD(), y: nextD())
        var flags: CGEventFlags = []
        if let m = args.first, m == "shift" || m == "cmd" {
            flags = m == "shift" ? .maskShift : .maskCommand
            args.removeFirst()
        }
        mouse(.mouseMoved, p)
        usleep(60_000)
        mouse(.leftMouseDown, p, flags: flags)
        usleep(60_000)
        mouse(.leftMouseUp, p, flags: flags)
    case "drag":
        let a = CGPoint(x: nextD(), y: nextD())
        let b = CGPoint(x: nextD(), y: nextD())
        mouse(.mouseMoved, a)
        usleep(80_000)
        mouse(.leftMouseDown, a)
        usleep(80_000)
        let steps = 12
        for i in 1...steps {
            let t = Double(i) / Double(steps)
            let p = CGPoint(x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t)
            mouse(.leftMouseDragged, p)
            usleep(25_000)
        }
        usleep(80_000)
        mouse(.leftMouseUp, b)
    case "move":
        mouse(.mouseMoved, CGPoint(x: nextD(), y: nextD()))
    case "scroll":
        let p = CGPoint(x: nextD(), y: nextD())
        let dy = Int32(nextD())
        let n = Int(nextD())
        mouse(.mouseMoved, p)
        usleep(60_000)
        for _ in 0..<n {
            let ev = CGEvent(
                scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 1,
                wheel1: dy, wheel2: 0, wheel3: 0)
            post(ev)
            usleep(8_000)
        }
    case "type":
        let text = next()
        for ch in text.unicodeScalars {
            let down = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: true)
            var u = [UniChar(ch.value)]
            down?.keyboardSetUnicodeString(stringLength: 1, unicodeString: &u)
            post(down)
            let up = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: false)
            up?.keyboardSetUnicodeString(stringLength: 1, unicodeString: &u)
            post(up)
            usleep(90_000)
        }
    case "key":
        let code = CGKeyCode(UInt16(next())!)
        post(CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true))
        post(CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false))
        usleep(90_000)
    case "sleep":
        usleep(UInt32(nextD() * 1000))
    default:
        FileHandle.standardError.write("unknown cmd \(cmd)\n".data(using: .utf8)!)
        exit(2)
    }
}
