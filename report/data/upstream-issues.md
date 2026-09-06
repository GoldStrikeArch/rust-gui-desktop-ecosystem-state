# Upstream issues to file — verified list (2026-08-30)

Every candidate finding from the eight rounds was re-verified before it
earned a place here: the claim was checked against the repo's own evidence,
the root cause was located in the *pinned* crate source (file:line), and the
crate's current `main` and issue tracker were checked for fixes or existing
reports. The full per-finding write-ups, including ready-to-paste issue
drafts, are in `report/data/verification/`:

| file | domain | section ids |
|---|---|---|
| `shell-muda-tray-hotkey.md` | muda, tray-icon, global-hotkey | M-1 … M-6 |
| `shell-notifications-windows.md` | notify-rust, mac-notification-sys, hide/reopen, arboard, freya/slint shell traps | N-1 … N-6 |
| `media-async-grid.md` | nokhwa, dioxus, iced, slint, gpui, ehttp, tauri-plugin-http, grid widgets | A-1 … A-6 |
| `build-packaging-platform.md` | DMG tools, dx bundle, Windows/Linux build & render paths | P-1 … P-6 |
| `text-stack.md` | cosmic-text, parley/fontique, vello/masonry, vizia, freya-edit, egui text | T-A … T-G |
| `interaction-traps.md` | round-2 framework traps (freya, vizia, iced, egui, masonry, dioxus, wry, slint, gpui) | I-1 … I-9 |
| `accesskit-maintainers.md` | not an issue; the maintainer/funding fact-check | — |

Ratings: **CONFIRMED** = offending code located in the pinned source (and,
where possible, reproduced standalone); **LIKELY** = strong inference, stated
as such in the draft. Anything that turned out to be documented behaviour,
our own bug, or already fixed/reported is listed at the bottom so it is not
filed twice.

## Tier 1 — file now (crashes, memory-safety, data corruption, silent wrong results; confirmed; unreported or unreleased)

| # | repo | title (draft in) | rating | why it matters |
|---|---|---|---|---|
| 1 | tauri-apps/muda | **Comment, not a new issue:** ask for a release containing a1550bd / PR #361 — the macOS use-after-free on first menu click (`Cell<*const MenuChild>` ivar) is fixed on `main` but every published version (0.17.2, 0.19.3) still has it (M-3) | CONFIRMED | affects every winit app with a menubar; reported twice before and closed as user error |
| 2 | pop-os/cosmic-text | Backspace deletes one `char` while Delete deletes one grapheme cluster (`edit/editor.rs:640` vs `:675`) — splits ZWJ emoji, dangling U+200D (T-A) | CONFIRMED, still on main | iced and every cosmic-text editor; reproduced on macOS and Windows |
| 3 | vizia/vizia | `backspace_offset` never advances its cursor, so the Druid-derived emoji state machine is dead code and always deletes one scalar (`src/text/backspace.rs:26-28`) (T-A) | CONFIRMED, one-line fix, reproduced standalone | vizia textbox corrupts any multi-scalar cluster |
| 4 | linebender/vello | Large scenes die on `max_storage_buffer_binding_size` because the device is requested with `Limits::default()` (128 MiB) while the adapter allows 4 GiB (`util.rs:167`); no typed error (T-C) | CONFIRMED, GPU-free repro (~161 MiB scene) | xilem 11k-line prose abort; same in masonry_winit `vello_util.rs:210` |
| 5 | linebender/xilem (masonry) | `render_text` encodes every line of a `prose` with no viewport culling (`masonry_core/core/text.rs:42`) (T-C) | CONFIRMED | the other half of #4; the commenter's culling diagnosis was right |
| 6 | emilk/ehttp | `fetch_raw_native(with_timeout)` ignores the flag for GET and inverts it for POST/PUT/PATCH (`types.rs:389-435`): streaming GETs silently capped at 30 s, non-streaming POSTs get no timeout (A-4.3) | CONFIRMED, on main | our original "default timeout kills streams" claim was wrong; this is the real bug |
| 7 | zed-industries/zed (gpui) | X11: unconditional depth-32 ARGB visual (`x11/window.rs:405-411`) + swallowed `Event::Error` (`client.rs:1262`) → software presentation blits depth-24 PutImage, 51/51 BadMatch, silent black window (P-4c) | CONFIRMED end to end from the retained probe | gpui 0.2.2 via Blade/ash, not `gpui_wgpu` — say so in the title |
| 8 | linebender/xilem (masonry_winit) | surface creation `.unwrap()` (`event_loop_runner.rs:971`) — panic exit 101 instead of backend fallback / error (P-3d) | CONFIRMED | 4 of 8 xilem apps died on the Windows default env; `WGPU_BACKEND=dx12` rescued all |
| 9 | lapce/floem | `rx.recv().unwrap().unwrap()` throws away a properly modelled surface error (`app/handle.rs:100`) (P-3d) | CONFIRMED | 6 of 6 floem apps died on the Windows default env |
| 10 | burtonageo/cargo-bundle | `--format msi` writes raw filenames into MSI Identifier columns (`msi_bundle.rs:315/499/512/630`) — any hyphenated binary name fails (P-6) | CONFIRMED, reproduces with `cargo new my-app` | explains 0/10 on Windows |
| 11 | crabnebula-dev/cargo-packager | NSIS step runs vendored `makensis.exe` without `NSISDIR` (`nsis/mod.rs:578-595`) → loads the system plugin dir, `nsis_tauri_utils` not found (P-6) | CONFIRMED | explains 0/10 on Windows |
| 12 | h4llow3En/mac-notification-sys | Pumps the main run loop synchronously (`objc/notify.m:167-179`, `runUntilDate:` up to 2 s) — from a winit 0.30 callback the process panic-aborts, from a `slint::Timer` it aborts with "Recursion in timer code"; minimal winit repro in draft (N-2a) | CONFIRMED | hit by xilem and vizia independently; likely also the eframe frame-scheduling freeze |
| 13 | hoodie/notify-rust | macOS `show()` returns `Ok(NotificationHandle)` before sending; the send happens in `Drop` and its error is discarded with `.ok()` (`nsusernotifications.rs:252`, `:142-161`) (N-2b) | CONFIRMED | "Ok but no banner" across the whole cohort |
| 14 | slint-ui/slint | `SystemTrayIcon` handle is created from a change tracker that never re-fires (`items/system_tray.rs:258-261`, `\|_\| true`), so a failed first creation is permanent and stderr-only (N-5c) | CONFIRMED | silent tray failure |
| 15 | tauri-apps/plugins-workspace (http) | Late `abort()` after the fetch settled raises an unhandled rejection: two never-removed abort listeners call `fetch_cancel` on a freed rid (`api-iife.js`, `commands.rs:355-363`); WHATWG says no-op (A-4.4) | CONFIRMED (the one-off webview freeze stays n=1) | |
| 16 | l1npengtul/nokhwa | macOS `supported_formats` advertises both ends of each frame-rate range but `set_all` matches only `maxFrameRate`, so every min-fps entry is unsettable and `RequestedFormatType::None` picks an unsettable one (`bindings-macos:812-822, 961-977, 1051-1058`) (A-1b) | CONFIRMED | symptom is open #204 (undiagnosed) — file or comment there with the root cause |
| 17 | l1npengtul/nokhwa | `lock()`: `NSError` out-param passed by value so the check is dead (`:1003-1011`), and `if !accepted == YES` precedence bug makes the branch unreachable on x86_64 (`:1013`) (A-1d) | CONFIRMED | "Lock Rejected" is expected; the diagnostics around it are broken |
| 18 | marc2332/freya | `canvas()` never repaints when only its closure changes: `RenderCallback::eq` is hard-coded `true` (`canvas.rs:55-59`) so `Element::eq` keeps the stale element; adding any event handler masks it because `EventHandler::eq` is `false` (`event_handler.rs:85-90`) (I-1f) | CONFIRMED, on main | silent stale rendering; the strongest defect in the round-2 sweep |
| 19 | marc2332/freya | `DropZone` calls `stop_propagation()` unconditionally on mouse-up, before checking whether a drag is in flight (`drag_drop.rs:215`) — swallows ancestor `on_mouse_up` for plain clicks inside a zone (I-1b′) | CONFIRMED, on main | the code-located version of our "on_mouse_up never fires" observation |
| 20 | vizia/vizia | Actions (`on_press`, `on_double_click`, hover, drag start) only fire when `cx.current == meta.target` — every view is hoverable by default, so a click on a card's own `Label` child does nothing (`actions.rs:172-199`); vizia#406 was closed in 2023 but the gate is identical on main (I-2) | CONFIRMED | regression report |
| 21 | linebender/xilem (masonry) | `capture_pointer` overwrites the previous capture unconditionally (`contexts.rs:388-397`); `Button` captures without `set_handled` and ancestors run last, so a draggable container breaks every stock button inside it (I-5a) | CONFIRMED, on main | |

## Tier 2 — file (design gaps, silent no-ops, docs that would have saved days; confirmed)

| # | repo | title (draft in) | rating |
|---|---|---|---|
| 22 | tauri-apps/muda | macOS predefined Edit items (`cut/copy/paste/select_all`) no-op **and** swallow ⌘X/⌘C/⌘V/⌘A on hosts without an NSText responder (winit, masonry, egui, iced) because `setAutoenablesItems(false)` + `setEnabled(true)` keep the key equivalent claimed (`macos/mod.rs:134, 880, 978-999`) (M-1) | CONFIRMED; three frameworks hit it independently |
| 23 | tauri-apps/muda | `MenuEvent::set_event_handler` is a process-global `OnceCell` whose `set` result is discarded — first caller wins, later calls silently ignored; ask for per-menu routing or a loud error (`lib.rs:491, 515-521`) (M-2) | CONFIRMED; root of dioxus#4495 and floem's version-divergence dependence |
| 24 | tauri-apps/muda | Linux: `Menu::new()`/`Submenu::new()` panic via gtk-rs `assert_initialized_main_thread!` when `gtk::init` wasn't called; `gtk::is_initialized()` is public — return `Err` (`gtk/mod.rs:377`) (M-4 T6) | CONFIRMED; turns green build-only CI into a launch death |
| 25 | tauri-apps/tray-icon | Linux `TrayIconBuilder::build()` returns `Ok` when no GTK loop / no StatusNotifier host exists; `set_tooltip`/`rect` are silent no-ops (`gtk/mod.rs:23-51`) (M-4 T7) | CONFIRMED; low priority (ksni migration is announced) |
| 26 | marc2332/freya | Release-only panic hook shows an `rfd` modal before anything reaches stderr and destroys the panic message (`freya-winit/src/lib.rs:61-72`, still in 0.5.0-rc.4); ask for opt-out or print-first (N-5b, A-6) | CONFIRMED |
| 27 | marc2332/freya | Docs: `EventsCombos::pressed`/`PressEventType` (the double/triple-click classifier) is not linked from `on_press`, `MouseEventData` or the events guide, and `events_combos.rs` has no doc comments; also its callers mix element-relative (`Input`, `SelectableText`) and global (`WindowDragExt`) coordinates within one 5 px threshold (this session's re-check, `apps/freya-board/FRICTION.md`) | CONFIRMED (docs) |
| 28 | iced-rs/iced | Docs: `widget::table` materialises one `Element` per cell for all rows (600 006 at 100k rows, plus 600 000 clones per view call) — say so and point at #160 (A-5a) | CONFIRMED |
| 29 | rust-skia/rust-skia | Docs: state the minimum MSVC toolset the prebuilt Windows `skia.lib` needs (0.93 fails LNK1120 on 5 `__std_*` externals under 14.34) (P-3c) | CONFIRMED |
| 30 | DioxusLabs/dioxus (dx) | `dx bundle`: (a) notarization (`APPLE_*`, notarytool, stapler) is implemented but documented nowhere; (b) missing `Dioxus.toml` keys surface one `bail!` at a time (`cli/bundle.rs:170-175`); (c) ignores `[package.metadata.bundle]` (P-2) | CONFIRMED |
| 31 | burtonageo/cargo-bundle | `compress_disk_image()` on master discards the `hdiutil convert` exit status (regression vs 0.11.0) and never retries (P-1) | CONFIRMED |
| 32 | create-dmg/create-dmg | Retry covers `create`/`detach` and greps for "Resource busy"; extend to `convert` and EAGAIN (tauri-bundler and cargo-packager vendor/download this script) (P-1) | CONFIRMED (our error string is reconstructed; say so) |
| 33 | 1Password/arboard | macOS `get_image` reads the TIFF flavor only (`osx.rs:231-237`) and fails on AppleScript's `as TIFF picture`; live repro captured (N-5a) | CONFIRMED |
| 34 | linebender/parley | After PR #715 `backdelete()` still splits regional-indicator pairs and Indic conjuncts (relates #694) (T-A) | CONFIRMED, optional |
| 35 | slint-ui/slint | `StandardTableView`: multi-row selection is not expressible (cell content is tracked in #3596/#2033; selection is not) (A-5b) | CONFIRMED, maybe |
| 36 | zed-industries/zed (gpui) | Per-window show/hide (only app-level `hide()`); and `build.rs` hard-requires `xcrun metal`, which Xcode 26 dropped, with no hint about `runtime_shaders` (N-4b, P-6) | CONFIRMED gaps, feature requests |
| 37 | marc2332/freya | `Size::flex` is silently ignored unless the parent sets `Content::flex()` (torin `measure.rs:644,673`; otherwise it evaluates to the full parent extent, `size.rs:273`); rustdoc never mentions the pairing — docs + a debug warning (I-1c) | CONFIRMED |
| 38 | marc2332/freya | `Input`: `on_pre_key_down` replaces the stock key filter, so an Escape handler must re-implement Enter/Shift/Tab — ask for an `on_escape`/cancel hook (multiline is already fixed on main, #2192) (I-1d) | CONFIRMED |
| 39 | marc2332/freya | No timer/interval primitive in the public API (components use `async_io::Timer` internally; only the one-shot `sdk::use_timeout` behind a non-default feature) (I-1e) | CONFIRMED, low priority |
| 40 | linebender/xilem | No autofocus / programmatic focus request for `TextInput`/`TextArea` from the view layer, on 0.4.0 and on main (Escape-cancel is fixed on main by #1826) (I-5b) | CONFIRMED |
| 41 | slint-ui/slint | `DragArea.drag-image` is `in property <image>` only — no way to use an element as the drag preview (`builtins.slint:1242`) (I-8a) | CONFIRMED, feature request |

## Comments on existing issues (root cause or new data to add)

| repo / issue | what to add | ref |
|---|---|---|
| DioxusLabs/dioxus #4495 | our tray+menubar reproduction; direction is "menubar registered first wins" | M-2b |
| DioxusLabs/dioxus #5586 (+PR #5587) | occluded window parks **all** Rust futures, not just JS: `poll_vdom` waits for the JS edit ack (`webview.rs:562-566`); 3/6 runs, deterministic with always-on-top | A-2 |
| h4llow3En/mac-notification-sys #81 | mechanism is the AppleScript `get id of application "use_default"` (`notify.m:9-11`); fix suggestions | N-1 |
| rust-windowing/winit #3992 | a concrete cause: helpers that pump the run loop inside a callback | N-2c |
| l1npengtul/nokhwa #245, #246, #204 | #245: our `=0.10.9` break confirms the `CompressionData` semver break; #246: fourcc 420v/420f→YUYV confirmed on FaceTime; #204: root cause = min-fps entries (Tier 1 #16) | A-1 |
| iced-rs/iced #3265 | lavapipe `SHADER_FLOAT16_IN_FLOAT32` panic never reaches the tiny-skia fallback because the fallback is type-driven (`iced_renderer/fallback.rs`) | P-4a |
| pop-os/cosmic-text #327 | exact ordering root cause (`font/fallback/unix.rs:31-48`); crop evidence | T-F |
| emilk/egui #5822 | root cause (`hit_test.rs:388-403` + `interaction.rs:195-203`): buttons inside `dnd_drag_source` are inert; our `UiBuilder::sense(Sense::drag())` workaround | I-4 |
| tauri-apps/tauri #14373 | `dragDropEnabled` default swallows HTML5 DnD on macOS too (docs say Windows); wry's `drag_drop.rs` wins over the page | I-7 |

## Dependency-bump requests (not bugs in the crate named)

- **linebender/xilem** and **lapce/floem**: bump parley/fontique to ≥ 0.8.0 —
  fixes Han fallback on macOS (PingFang has no readable file; 0.8 falls back
  to Heiti), regional-indicator flags (harfrust 0.5.2), and, on parley
  `main`, the ZWJ backspace (#715). xilem 0.4 pins 0.6, floem 0.7.0.
- **tauri-apps/tauri (tauri-bundler)** and **cargo-packager**: refresh the
  vendored/downloaded create-dmg once #32 lands.

## Candidates that need a minimized reproduction first (A-6 / P-6 sweeps; details in the reports)

floem-vger image-atlas silent pack failure (permanent black preview) and
stale rect after clear; floem `VirtualStack` degrading to a 16 GiB
allocation; vizia `VirtualTable` wrappers eating row clicks; gpui
`RenderImage` atlas leak; tauri-codegen missing `rerun-if-changed`;
`egui_kittest::Harness::run` panicking on animating UIs; iced
`scrollable::scroll_to` never emitting `on_scroll` (confirmed at source,
`scrollable.rs:1676-1693`); wgpu `FailedToCreateSurfaceForAnyBackend` with
an empty per-backend map on Radeon 890M (capture `RUST_LOG=wgpu_core=debug`
first).

## Verified as *not* ours to file (so nobody files them twice)

| finding | verdict |
|---|---|
| muda `init_for_gtk_window` needs a GTK window on Linux | by design; DBusMenu/ksni work tracked in muda #239, tray-icon #201 |
| global-hotkey X11-only, no Wayland | documented; tracked #28, #162 |
| global-hotkey Pressed+Released; Windows `AlreadyRegistered` on Win+Shift+9 | correct behaviour; our missing filter / our `.expect()` |
| eframe never runs `App::ui` for hidden viewports | documented on `App::logic`; at most a one-line doc cross-reference |
| freya close hook "can't touch its window" | our mistake — `window_mut()` is public |
| freya-edit UTF-16 caret / ZWJ split | fixed in freya-edit 0.5.0-rc.1 (PR #2034) |
| parley ZWJ backspace | fixed on main, PR #715 (unreleased) |
| fontique macOS Han fallback ("AssetsV2") | fixed in fontique 0.8.0; our explanation was wrong |
| xilem/parley regional-indicator flags | fixed in parley 0.8.0 / harfrust 0.5.2 |
| egui RTL/AX selection, combining marks, colour emoji | consequences of tracked limitations (#1016, #2551) |
| egui selectable label eats table-row clicks | egui #5045 (open, `bug`) |
| nokhwa fourcc mis-map | PR #246 open, fixed on `senpai` |
| iced default executor has no reactor; `time::every` behind a feature | documented; compile error is plain (#2188) |
| slint `spawn_local` + reqwest panics | documented in the `spawn_local` rustdoc (`async_compat`) |
| tauri 0/8 on Windows (`icons/icon.ico`) | our repo never committed the icon |
| freya 0/8 on Windows (prebuilt Skia download) | asset exists and downloads; machine-local curl(3) failure, unisolated |
| gpui/dioxus/floem peek apps on Windows (`core-foundation`/`objc2`) | our un-target-gated `[dependencies]` |
| vello on lavapipe LLVM-15 JIT abort | Mesa bug, not linebender's |
| notify-rust "toasts work on Windows" | only `show()` returned `Ok`; nothing observed |
| wry `dragDropEnabled` vs HTML5 DnD | documented Tauri window config; tauri#14373 tracks the docs |
| freya `DropZone` has no size setters | fixed on freya main (#2151 / issue #2147), unreleased |
| freya ancestor `on_mouse_up` "never fires" in empty column area | unconfirmed — no dispatch mechanism supports it; the real defect is Tier 1 #19 |
| iced `text_input` captures Escape so `event::listen()` never fires | documented (`event::listen` = uncaptured events only); gap is an `on_escape` hook, adjacent iced#2678 |
| Dioxus mouse events "expose no element geometry" | overstated — `element_coordinates()` exists; only the target size is async via `onmounted` |
| masonry Escape-cancel "has no API" | fixed on main by #1826 (2026-07-28), after 0.4.0 |
| gpui element bounds outside drag events | design; `canvas` is the sanctioned probe |
