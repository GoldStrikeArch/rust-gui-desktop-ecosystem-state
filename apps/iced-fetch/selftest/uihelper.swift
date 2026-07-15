// Verification-only input synthesis helper (SPEC-7/SPEC-8 self-tests).
// Scopes all interaction to a window owned by the given process name:
// `bounds` looks the window up via CGWindowList so the driver script can
// compute in-window coordinates and screenshot it (`screencapture -l <id>`).
//
//   uihelper bounds <owner-name>            -> "<windowid> <x> <y> <w> <h>"
//   uihelper click <x> <y> [shift|cmd]
//   uihelper drag <x1> <y1> <x2> <y2>
//   uihelper scroll <x> <y> <dy> <count>    (dy<0 scrolls content down)
//   uihelper type <text>
//
// Global coordinates, origin top-left of the main display (same space as
// CGWindowList bounds). Requires Accessibility permission for the caller.

import CoreGraphics
import Foundation

func fail(_ message: String) -> Never {
    FileHandle.standardError.write((message + "\n").data(using: .utf8)!)
    exit(1)
}

func post(_ event: CGEvent?, _ delayMs: UInt32 = 15) {
    guard let event else { fail("event creation failed") }
    event.post(tap: .cghidEventTap)
    usleep(delayMs * 1000)
}

func mouse(_ type: CGEventType, _ p: CGPoint, flags: CGEventFlags = []) {
    let event = CGEvent(
        mouseEventSource: nil, mouseType: type, mouseCursorPosition: p,
        mouseButton: .left)
    event?.flags = flags
    post(event)
}

let args = CommandLine.arguments
guard args.count >= 2 else { fail("usage: uihelper <cmd> ...") }

switch args[1] {
case "bounds":
    let owner = args[2]
    guard
        let windows = CGWindowListCopyWindowInfo(
            [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
            as? [[String: Any]]
    else { fail("no window list") }

    for window in windows {
        guard let name = window[kCGWindowOwnerName as String] as? String,
            name == owner,
            let bounds = window[kCGWindowBounds as String] as? [String: CGFloat],
            let id = window[kCGWindowNumber as String] as? Int,
            bounds["Height"]! > 50 // skip menus/shadows
        else { continue }

        print("\(id) \(Int(bounds["X"]!)) \(Int(bounds["Y"]!)) \(Int(bounds["Width"]!)) \(Int(bounds["Height"]!))")
        exit(0)
    }
    fail("window of \(owner) not found")

case "topat":
    // Owner of the frontmost normal-layer window containing the point —
    // the shared-desktop guard: only post input when this is our window.
    let p = CGPoint(x: Double(args[2])!, y: Double(args[3])!)
    guard
        let windows = CGWindowListCopyWindowInfo(
            [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
            as? [[String: Any]]
    else { fail("no window list") }

    for window in windows { // front-to-back order
        guard let layer = window[kCGWindowLayer as String] as? Int,
            layer == 0,
            let bounds = window[kCGWindowBounds as String] as? [String: CGFloat]
        else { continue }
        let rect = CGRect(
            x: bounds["X"]!, y: bounds["Y"]!, width: bounds["Width"]!,
            height: bounds["Height"]!)
        if rect.contains(p) {
            print(window[kCGWindowOwnerName as String] as? String ?? "?")
            exit(0)
        }
    }
    print("none")

case "click":
    let p = CGPoint(x: Double(args[2])!, y: Double(args[3])!)
    var flags: CGEventFlags = []
    var modifierKey: CGKeyCode? = nil
    if args.count > 4 {
        if args[4] == "shift" {
            flags = .maskShift
            modifierKey = 56
        }
        if args[4] == "cmd" {
            flags = .maskCommand
            modifierKey = 55
        }
    }
    // Modifiers must arrive as a real flagsChanged key event — winit tracks
    // them from the keyboard stream, not from mouse-event flags.
    if let key = modifierKey {
        let down = CGEvent(keyboardEventSource: nil, virtualKey: key, keyDown: true)
        down?.flags = flags
        post(down, 40)
    }
    mouse(.mouseMoved, p, flags: flags)
    usleep(60_000)
    mouse(.leftMouseDown, p, flags: flags)
    mouse(.leftMouseUp, p, flags: flags)
    if let key = modifierKey {
        let up = CGEvent(keyboardEventSource: nil, virtualKey: key, keyDown: false)
        up?.flags = []
        post(up, 40)
    }

case "drag":
    let from = CGPoint(x: Double(args[2])!, y: Double(args[3])!)
    let to = CGPoint(x: Double(args[4])!, y: Double(args[5])!)
    mouse(.mouseMoved, from)
    usleep(80_000)
    mouse(.leftMouseDown, from)
    let steps = 12
    for i in 1...steps {
        let t = Double(i) / Double(steps)
        let p = CGPoint(
            x: from.x + (to.x - from.x) * t, y: from.y + (to.y - from.y) * t)
        mouse(.leftMouseDragged, p)
    }
    usleep(80_000)
    mouse(.leftMouseUp, to)

case "scroll":
    let p = CGPoint(x: Double(args[2])!, y: Double(args[3])!)
    let dy = Int32(args[4])!
    let count = Int(args[5])!
    mouse(.mouseMoved, p)
    usleep(60_000)
    for _ in 0..<count {
        let event = CGEvent(
            scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 1,
            wheel1: dy, wheel2: 0, wheel3: 0)
        post(event, 8)
    }

case "type":
    let text = args[2]
    for character in text.unicodeScalars {
        let down = CGEvent(
            keyboardEventSource: nil, virtualKey: 0, keyDown: true)
        var chars = [UniChar](String(character).utf16)
        down?.keyboardSetUnicodeString(
            stringLength: chars.count, unicodeString: &chars)
        post(down, 30)
        let up = CGEvent(
            keyboardEventSource: nil, virtualKey: 0, keyDown: false)
        post(up, 30)
    }

case "key":
    // named keys: delete=51, return=36
    let code: CGKeyCode = args[2] == "delete" ? 51 : 36
    let times = args.count > 3 ? Int(args[3])! : 1
    for _ in 0..<times {
        post(CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true), 25)
        post(CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false), 25)
    }

default:
    fail("unknown command \(args[1])")
}
