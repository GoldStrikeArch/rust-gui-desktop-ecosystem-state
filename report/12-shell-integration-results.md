# OS shell integration: "Tray Notes" in seven frameworks (macOS)

**Run date:** 2026-07-09.

Iteration 3, SPEC-4 (`apps/SPEC-4.md`): one quick-note app per framework
exercising the layer the initiative calls the "hard parts" — tray, global
hotkey, native menubar, dialogs, clipboard (text + image), Finder file drop,
notifications, live dark mode, multi-window, close-to-tray. All seven built
and survived the scripted launch check; a later audit observed their on-screen
windows on macOS 26.5.2. Individual capabilities used a mix of real OS
interaction, app self-tests/synthetic input, and source/API-path verification
(notably Finder drops); per-app `FRICTION.md` files identify the evidence.
Raw rows: [data/shell-text-rows.md](data/shell-text-rows.md).

## The capability matrix

Ratings: **built-in** / **assembled** (compose framework + helper crates,
small glue) / **hand-rolled** / not-achievable. These ratings describe the
implementation path, not the evidence level; footnotes and FRICTION files say
whether that path was exercised end-to-end.

| Capability | iced | egui | gpui | tauri | xilem | slint | dioxus |
|---|---|---|---|---|---|---|---|
| System tray | assembled | assembled | assembled | **built-in** | assembled | **built-in** | **built-in** |
| Global hotkey | assembled | assembled | assembled | assembled¹ | assembled | assembled | **built-in** |
| Native menubar | assembled² | assembled² | **built-in** | **built-in** | assembled² | **built-in** | **built-in**³ |
| Native dialogs | assembled | assembled | **built-in** | assembled¹ | assembled | assembled | assembled |
| Clipboard text | built-in | built-in⁴ | built-in | built-in | built-in | built-in | built-in |
| Clipboard image | assembled | assembled | **built-in** | assembled¹ | assembled | assembled | assembled |
| File drop (Finder) | built-in | built-in | built-in | built-in | assembled⁵ | assembled⁶ | built-in |
| Notification | assembled | **hand-rolled**⁷ | assembled | assembled¹ | **hand-rolled**⁷ | assembled | assembled |
| Dark mode (live) | built-in | built-in | built-in | built-in | hand-rolled⁸ | built-in | built-in |
| Multi-window | built-in | built-in | built-in | built-in | built-in | built-in | built-in |
| Close-to-tray | built-in | assembled⁹ | assembled | built-in | assembled | built-in | built-in |

¹ via official first-party Tauri plugins (one Cargo line + one `.plugin()` line each).
² muda works, but its **predefined Edit items (Cut/Copy/Paste) silently
swallow ⌘X/⌘C/⌘V** before the framework's own bindings see them — hit
independently by iced, egui, AND xilem; all three had to ship custom menu
items that re-inject synthetic paste/copy events.
³ dioxus trap: tray-icon and muda share ONE global MenuEvent handler slot and
dioxus installs the tray receiver last — menubar events arrive as tray events.
⁴ egui clipboard is text-only; images need arboard.
⁵ masonry_winit 0.4 drops winit's `DroppedFile` on the floor; caught by
wrapping the ApplicationHandler.
⁶ Slint 1.17's new DnD API cannot receive external drops (`DataTransfer` has
no file-path representation, slint#1967); requires the **unstable**
`unstable-winit-030` raw-event filter.
⁷ notify-rust was rejected after failures reproduced in these tested versions:
the egui/macOS app lost frame scheduling after a notification (delegate
replacement is the leading inferred cause, pending a minimized upstream
reproduction); on xilem it failed three ways
(LaunchServices chooser, runloop-reentrancy panic-abort, dropped banners).
Both shipped `osascript` shell-outs instead. Everywhere else it "worked" at
the API level, but unbundled cargo binaries have no app bundle identity so
macOS may silently drop banners. Reliable app-attributed delivery normally
requires a bundled identity; this is not a universal claim that every possible
notification mechanism requires an `.app`.
⁸ xilem's theme is dark-only; live dark mode meant intercepting winit
ThemeChanged and hand-painting a second palette.
⁹ eframe landmine: hidden viewports never run `App::ui`, so reopen logic must
live in `App::logic` or the window can never come back.

## What the round settled

1. **Zero implementation-level not-achievable cells in these seven macOS
   implementations.** The predicted failure (tray/hotkey fighting the event loop in
   iced/egui/xilem) did not materialize: on macOS the tauri-apps shell crates
   (tray-icon, muda, global-hotkey) attached successfully to every framework
   tested here. Event delivery differs: Tauri/Dioxus pump natively; several
   native stacks drain channels on timers; egui uses callbacks/request_repaint
   plus a watchdog. **The GTK fault line is Linux-specific** — on Linux, muda menubars
   still can't attach to winit windows at all (§1.6 of the ecosystem map);
   this round measured macOS only and does not soften that finding.
2. **The real difference is defaults and traps, not source-level feasibility.**
   Under this rubric Tauri had 7, Dioxus 8, and Slint 6 of 11 capabilities
   built in. The other apps had larger total implementations, but those totals
   include UI, editors, verification hooks, and sometimes config—not isolated
   shell glue. Dioxus matching Tauri on
   (tray, hotkey, menubar, multi-window, close-to-tray all built-in) was the
   round's biggest upset.
3. **The muda Edit-roles trap reproduced in three integrations:** the tested
   winit-based apps that installed the predefined Edit menu lost
   clipboard shortcuts. This is a one-fix-helps-everyone upstream issue —
   exactly the kind of thing the initiative's working group should file/fund.
4. **notify-rust was the least reliable helper in this macOS round** (the
   tested eframe setup froze, Xilem hit three failure modes, and banner display
   often depended on bundle identity) — a local result rather than a universal
   platform verdict. It connects directly to the packaging round:
   **notification attribution and
   delivery should be tested from a bundled, identified `.app`**, not inferred
   from a successful API call in a bare cargo binary. This round did not
   establish the same bundle requirement for every shell capability.
5. **Clipboard images and native dialogs had viable macOS paths** across the board
   (arboard/rfd or built-ins) — gpui even decodes pasteboard images natively.

## Effort profile (total measured source LoC for the same spec)

dioxus 394 · slint 447 (incl. DSL) · egui 514 · tauri 559 · iced 682 ·
xilem 774 · gpui 1,575 (836 of which is the reused hand-rolled text editor —
the missing-text-widget tax compounding again).

Counts are `.rs` plus Slint/HTML/JS/CSS, excluding JSON/TOML config and
including verification hooks. They are total implementation sizes, not
shell-only or production-only LoC.

The shell layer's dependency cost is also measurable (`measurements/results-iter3.csv`):
adding tray/hotkey/menubar/dialogs/clipboard/notifications grew the dependency
tree by 8–112 crates vs the same framework's todo app (iced 140→252,
slint 302→317, tauri 204→235 via four official plugins, egui 156→167,
xilem 143→169, dioxus 279→287, gpui 391→412). Clean builds stayed 35–67 s;
all 14 current iteration-3 executables survived eight seconds and exposed a
visible window in the retained serial verification run.

## Caveats

- macOS only; the Linux story (GTK fault line, portals) is documented from
  sources in `00-ecosystem-map.md` §1.6 but not empirically tested here.
- Seven agents shared one desktop, clipboard, hotkey namespace, and OS theme
  during verification; agents compensated (AX-scoped clicks, clipboard
  sentinels, pixel-verification) and flagged uncertain results as such.
- Finder drags cannot be synthesized programmatically; file-drop wiring was
  verified at the event/code level everywhere.
