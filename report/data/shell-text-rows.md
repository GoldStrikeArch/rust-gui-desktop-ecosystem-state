# Iteration-3 structured rows (Tray Notes + Babel) — reconciled agent returns

These qualitative rows were reconciled on 2026-07-09 against
`../../measurements/results-iter3.csv`. Canonical build fields below use clean
seconds, incremental seconds, unique crate names, and raw/stripped bytes from
that CSV. Evidence labels follow `../../measurements/EVIDENCE.md`:
`source/API-path`, `self-test/synthetic-input`, `observed-local` (no reusable
trace retained), and `unexercised`. A successful notification API call is not
evidence that macOS displayed a banner. Finder file drops were source/API-path
verified across all seven apps because synthetic input cannot produce a real
Finder drag session. LoC fields likewise describe the canonical measurement
snapshot; later audit-only comment edits may change a physical recount without
changing the measured implementation.

## tauri

```yaml
framework: tauri
tray:
  build_ok: true
  canonical_measurement: {clean_s: 52, incremental_s: 11, deps_unique: 235, binary_bytes: 10505264, binary_stripped_bytes: 8423608}
  process_survived: true         # process survived; early ~123 MiB app-only anecdote is not a controlled total-process-tree sample
  window_observed: true
  loc: 559                # canonical source: 338 Rust + 221 HTML/JS/CSS; excludes 43 config lines
  helper_crates: [tauri-plugin-global-shortcut =2.3.2, tauri-plugin-dialog =2.7.1, tauri-plugin-notification =2.3.3, tauri-plugin-clipboard-manager =2.3.2]  # official first-party plugins; unique names grew 204→235, not 271→312
  ratings:
    tray: {rating: built-in, note: "TrayIconBuilder in core behind the tray-icon cargo feature; ~10 LoC."}
    global_hotkey: {rating: assembled, note: "Official plugin (global-hotkey/Carbon underneath); handler FIRED from synthetic Cmd+Shift+9, toggling the hidden window. Rust-side → zero ACL."}
    native_menubar: {rating: built-in, note: "tauri::menu (muda) = real NSMenu — UI-scripted: clicked File→Save… and the handler ran; predefined Edit roles are what make ⌘V reach WKWebView at all."}
    dialogs: {rating: assembled, note: "Official plugin (rfd underneath) from JS; verified end-to-end: real NSSavePanel sheet appeared, Escape dismissed."}
    clipboard_text: {rating: built-in, note: "textarea paste is WebKit + Edit-menu paste role; no plugin, no permission."}
    clipboard_image: {rating: assembled, note: "clipboard-manager's desktop backend IS arboard. Verified round-trip: Rust write_image 48×32 → RGBA over ipc raw bytes → canvas thumbnail (in screenshot)."}
    file_drop: {rating: built-in, evidence: "source/API-path", note: "WindowEvent::DragDrop with dragDropEnabled:true (opposite of iteration 2; native and HTML5 DnD are mutually exclusive per window); the API yields real paths, but no Finder drop was performed."}
    notification: {rating: assembled, evidence: "self-test/API-path; banner unobserved", note: "Official plugin (notify-rust underneath); show() returned Ok from the unbundled binary, but macOS display is bundle/permission gated and no banner was visually confirmed."}
    dark_mode_live: {rating: built-in, note: "Verified LIVE both directions via osascript: Rust ThemeChanged AND matchMedia change fired; CSS variables restyled with zero JS."}
    multi_window: {rating: built-in, note: "WebviewWindowBuilder + second static about.html. Trap: creation must run on the main thread (run_on_main_thread)."}
    close_to_tray: {rating: built-in, note: "CloseRequested → prevent_close()+hide() (main window only); restored via hotkey + RunEvent::Reopen on dock click."}
babel:
  build_ok: true
  canonical_measurement: {clean_s: 36, incremental_s: 9, deps_unique: 204, binary_bytes: 8301920, binary_stripped_bytes: 6617000}
  process_survived: true         # local ~121–123 MiB app-process observation; not the controlled total-process-tree dataset
  window_observed: true
  loc: 321                # canonical source: 65 Rust + 256 frontend; excludes 39 config lines
  fonts_bundled: none     # 0 bytes; every script via automatic CoreText fallback
  screenshot_ok: true     # window-scoped; all 11 lines verified incl. zoomed emoji crop
  ratings:
    bidi_render: {rating: built-in, note: "[AR]/[HE] read RTL with embedded English + digits correct. Asterisk: the ASCII '[AR] ' prefix defeats first-strong heuristics (dir=auto ⇒ LTR base), so 6 lines of JS pick per-line dir — the engine does all actual BiDi."}
    cjk_render: {rating: built-in, note: "Zero tofu, correct fullwidth punctuation, automatic PingFang/Hiragino/Apple SD Gothic fallback."}
    emoji_zwj: {rating: built-in, note: "Family renders as ONE glyph — Apple's ≥14.4 SILHOUETTE design (single duotone glyph by design, not a fallback); skin tones and flags all correct."}
    mixed_fallback_line: {rating: built-in, note: "All 8 scripts + emoji in one paragraph, no tofu, nothing configured."}
    grapheme_caret: {rating: built-in, note: "Real CGEvent arrows: 6th ← jumped 11 units over 👨‍👩‍👧‍👦; backward-delete removed the whole cluster. Quirk: programmatic setSelectionRange NOT snapped mid-cluster."}
    selection: {rating: built-in, note: "Shift+→ selected exactly the whole family cluster. Mouse selection across BiDi boundary not exercisable headlessly."}
    ime: {rating: built-in, evidence: "unexercised WebKit/API path", note: "WKWebView textareas use native macOS IME machinery; a CJK input source could not be script-activated and was not exercised."}
    large_doc_scroll: {rating: built-in, note: "Local unretained probe: 11,000 lines built in ~20 ms; 149-frame continuous scroll averaged 16.60 ms with 0 frames >33 ms; app RSS ~121 MiB, helpers excluded."}
```

SURPRISES:
- Both apps compiled without API-fix iterations, and most shell paths were exercised on the first release build. Finder drop remained source/API-path evidence, notification banner display was unconfirmed, and CJK IME was unexercised.
- The official plugins wrap helper crates other frameworks hand-wire (clipboard-manager=arboard, dialog=rfd, notification=notify-rust, global-shortcut=global-hotkey), integrated through one Cargo line + one `.plugin()` line each. Unique names grew 204→235; flattened name-version rows grew by 41.
- The ACL bill is per-plugin-used-from-JS, not per plugin installed: only the JS-invoked dialog plugin cost a permission line; Rust-side plugin use bypasses capabilities entirely.
- 👨‍👩‍👧‍👦 is one glyph but no longer full-color: Apple's ≥14.4 silhouette family design could be misread as a fallback artifact in cross-framework screenshot comparisons.

TIME_SINK:
- Building the headless verification harness (selftest threads, eval hooks, osascript theme/hotkey/menubar UI-scripting, caret tracing) — several times the cost of the features.
- Window-scoped screenshots of unbundled binaries: CGWindowID needs a Swift helper; sips crop offsets fiddly.
- Proving native-ness of menubar/dialogs: System Events clicking real File→Save… and counting NSSavePanel sheets.

## packaging round (todo apps → .app + .dmg)

Sizes are MiB. `local_adhoc_seal_verified` means the explicit local
`codesign -s - --deep --force` structural check passed; it is not a Developer
ID distribution signature. No signing identity or notarization credentials
were configured.

```yaml
- {app: iced-app,   tool_used: cargo-bundle 0.11.0,        config_effort_loc: 4, app_size_mib: 9.9,  dmg_size_mib: 4.2, launched_ok: true, local_adhoc_seal_verified: true, spctl_result: "rejected (expected — no Developer ID/notarization)", gotcha_oneline: "4-line stanza worked; cargo-packager comparison exposed a 0.11.8 config auto-detection bug"}
- {app: egui-app,   tool_used: cargo-bundle 0.11.0,        config_effort_loc: 4, app_size_mib: 11.9, dmg_size_mib: 5.5, launched_ok: true, local_adhoc_seal_verified: true, spctl_result: "rejected (expected)", gotcha_oneline: "cargo-bundle's built-in DMG step failed twice; .app unaffected"}
- {app: gpui-app,   tool_used: cargo-bundle 0.11.0,        config_effort_loc: 4, app_size_mib: 5.0,  dmg_size_mib: 2.5, launched_ok: true, local_adhoc_seal_verified: true, spctl_result: "rejected (expected)", gotcha_oneline: "smallest bundle; runtime_shaders caused no packaging friction"}
- {app: tauri-app,  tool_used: tauri-cli 2.11.4 (bundler), config_effort_loc: 2, app_size_mib: 8.0,  dmg_size_mib: 3.0, launched_ok: true, local_adhoc_seal_verified: true, spctl_result: "rejected (expected)", gotcha_oneline: "bundle_dmg.sh failed 3/3 at hdiutil convert; no Node needed for this static app"}
- {app: xilem-app,  tool_used: cargo-bundle 0.11.0,        config_effort_loc: 4, app_size_mib: 11.4, dmg_size_mib: 4.8, launched_ok: true, local_adhoc_seal_verified: true, spctl_result: "rejected (expected)", gotcha_oneline: "built-in DMG step failed; .app unaffected"}
- {app: slint-app,  tool_used: cargo-bundle 0.11.0,        config_effort_loc: 4, app_size_mib: 14.7, dmg_size_mib: 7.4, launched_ok: true, local_adhoc_seal_verified: true, spctl_result: "rejected (expected)", gotcha_oneline: "simple hdiutil fallback needed one retry"}
- {app: dioxus-app, tool_used: cargo-bundle 0.11.0,        config_effort_loc: 4, app_size_mib: 5.7,  dmg_size_mib: 2.5, launched_ok: true, local_adhoc_seal_verified: true, spctl_result: "rejected (expected)", gotcha_oneline: "plain binary with embedded assets; no dioxus-cli needed"}
```

PACKAGING VERDICT:
- In this macOS arm64 run, cargo-bundle produced all six non-Tauri `.app`s from the same four-line metadata shape; Tauri used its own bundler. The iced-only cargo-packager comparison needed nine config lines and exposed a real 0.11.8 auto-detection bug, but cargo-packager also covers broader installer/signing/update-artifact workflows that this credential-free comparison did not exercise.
- Producing locally launchable `.app` and `.dmg` artifacts from already-built binaries was low effort here. That does not generalize to Windows/Linux installers or distribution-ready signing/notarization.
- cargo-bundle has no integrated signing. Tauri and cargo-packager support configured signing/notarization paths, and rcodesign implements signing/notarization/stapling, but no identity was configured. All final apps therefore started linker-ad-hoc-signed and were explicitly given only a local ad-hoc verification seal.
- Built-in DMG outcomes were 2 successes and 6 final failures across 8 app/tool paths. Reconstructing retry markers yields 13 literal executions and 11 failures; exact command logs were not retained. The simple `hdiutil create -format UDZO` fallback produced 7 app outcomes from 8 executions because Slint needed one retry.
- `spctl` rejected all seven locally ad-hoc-signed apps as expected. Distribution still needs Developer ID credentials and notarization; Tauri, cargo-packager, rcodesign, and platform tools automate parts of that path but cannot supply credentials.

Full write-up: `packaging-results.md`. Artifacts: `../../dist/<framework>/`
(`.app` plus `.dmg`, all `hdiutil verify`-checked). Structured plist,
signature, `codesign`, `hdiutil`, and `spctl` results live in that file.

## iced

```yaml
framework: iced
tray:
  build_ok: true
  canonical_measurement: {clean_s: 54, incremental_s: 2, deps_unique: 252, binary_bytes: 15144400, binary_stripped_bytes: 12940312}
  process_survived: true   # process/window observed; most flows used AX/CGEvent, while Finder drop remained source/API-path only
  window_observed: true
  loc: 682
  helper_crates: [tray-icon 0.24, muda 0.19 (via tray-icon re-export), global-hotkey 0.7, rfd 0.17, arboard 3.6, notify-rust 4.18]
  ratings:
    tray: {rating: assembled, note: "tray-icon NSStatusItem created lazily on main thread after loop start (boot Task; iced runs boot/update/view on main thread). Events polled off crossbeam channel every 100ms. AX-clicked tray menu items all fired."}
    global_hotkey: {rating: assembled, note: "global-hotkey (Carbon, no permissions). ⌘⇧9 toggles both directions incl. while hidden."}
    native_menubar: {rating: assembled, note: "muda init_for_nsapp; ⌘N/O/S verified. HEADLINE GOTCHA: PredefinedMenuItem cut/copy/paste use responder-chain selectors winit doesn't implement — they no-op AND swallow ⌘X/C/V before iced's own bindings (⌘V dead). Fixed with custom items hand-routed to the one editor; no focus-based dispatch exists; no undo/redo in iced."}
    dialogs: {rating: assembled, note: "rfd AsyncFileDialog composes directly with Task::perform; real NSOpenPanel verified. Least friction of all helpers."}
    clipboard_text: {rating: built-in, note: "iced clipboard tasks + text_editor bindings — but only reachable after removing muda's predefined Edit items."}
    clipboard_image: {rating: assembled, note: "arboard get_image → image::Handle::from_rgba (needs non-default image feature). Verified with planted PNG."}
    file_drop: {rating: built-in, evidence: "source/API-path", note: "window::Event::FileDropped via event::listen_with; Finder drags could not be synthesized, so plumbing was confirmed in iced_winit source but not exercised end-to-end."}
    notification: {rating: assembled, evidence: "API-path; banner unobserved", note: "notify-rust returned Ok from the unbundled binary; macOS banner display is bundle/approval gated and was not confirmed."}
    dark_mode_live: {rating: built-in, note: "Zero code in 0.14 (PR #3051): shell re-resolves system theme on ThemeChanged. Verified live — even followed sibling agents' toggles in real time."}
    multi_window: {rating: built-in, note: "daemon + window::open/close, per-window view/title. Verified independent."}
    close_to_tray: {rating: built-in, note: "exit_on_close_request:false + close_requests() → set_mode(Hidden); daemon keeps process alive at zero windows. Cosmetic gap: Dock icon remains; no reopen-event or ActivationPolicy access through iced."}
babel:
  build_ok: true
  canonical_measurement: {clean_s: 25, incremental_s: 2, deps_unique: 171, binary_bytes: 10918928, binary_stripped_bytes: 9309624}
  process_survived: true
  window_observed: true
  loc: 336
  fonts_bundled: none   # 0 bytes; everything from macOS fonts via fontdb/cosmic-text fallback
  screenshot_ok: true   # all 11 lines, no tofu
  ratings:
    bidi_render: {rating: built-in, note: "AR/HE reorder correctly RTL with embedded English + digits properly ordered. No API for explicit paragraph base direction (auto-derived)."}
    cjk_render: {rating: built-in, note: "System fonts, full-width punctuation fine, no tofu, zero config."}
    emoji_zwj: {rating: built-in, note: "Family is ONE glyph (Apple silhouette design); skin tones, flags correct; inline emoji don't break shaping."}
    mixed_fallback_line: {rating: built-in, note: "Every script in one paragraph without tofu in both text widget and editor; Zalgo renders."}
    grapheme_caret: {rating: built-in, note: "SPLIT VERDICT: caret/selection motion grapheme-atomic (👨‍👩‍👧‍👦 = one step) — but Backspace deletes one scalar and splits the displayed cluster, leaving a dangling ZWJ while the UTF-8 buffer remains valid. Tested cosmic-text editor-path asymmetry."}
    selection: {rating: built-in, note: "Click/dbl-click-word/drag + Shift+arrows verified live; drag across BiDi boundary grows logically-ordered selection; ⌘A/⌘C/⌘V round-trip preserves all scripts + ZWJ emoji."}
    ime: {rating: built-in, evidence: "self-test/synthetic-input (dead key); CJK unexercised", note: "Full preedit plumbing in 0.14; marked text was exercised via ⌥e dead-key composition. No CJK input source was installed."}
    large_doc_scroll: {rating: built-in, note: "Local unretained probe: 11k lines caused an 803 ms layout pause on load (no virtualization), then 118.5 fps scripted scrolling (120 Hz-limited); app RSS 97→410 MiB."}
```

SURPRISES:
- muda's predefined Edit items don't just no-op on iced — their key equivalents SWALLOW ⌘X/⌘C/⌘V before iced's text_editor bindings ever see them; installing the standard Edit menu silently breaks paste app-wide.
- window::screenshot renders every widget except text_editor content (editor pane empty in offscreen pass) — fell back to screencapture -l.
- Dark mode in iced 0.14 genuinely zero-code and live.
- Caret/selection was grapheme-atomic over ZWJ emoji, but Backspace split the
  cluster in this tested Iced/cosmic-text editor path.

TIME_SINK:
- Diagnosing the dead ⌘V → muda key-equivalent interception, then hand-routing Edit actions to one hard-coded widget (no focus-based menu dispatch in iced).
- Sharing one screen/clipboard/theme/focus with 6 parallel agents: "failed" tests were focus races with other frameworks' overlapping windows.
- Main-thread/ordering constraints for tray+menubar+hotkey (create after run-loop start via boot Task; 100 ms channel polling because iced can't ingest foreign event sources).

## dioxus

```yaml
framework: dioxus
tray:
  build_ok: true
  canonical_measurement: {clean_s: 37, incremental_s: 4, deps_unique: 287, binary_bytes: 9324656, binary_stripped_bytes: 8124152}
  process_survived: true  # process/window observed; early ~93 MiB app-only anecdote is noncanonical
  window_observed: true
  loc: 394
  helper_crates: [rfd 0.17 (already inside dioxus-desktop, not re-exported), arboard 3.6, image 0.25, base64 0.22, notify-rust 4.18]
  ratings:
    tray: {rating: built-in, note: "dioxus::desktop::trayicon::init_tray_icon + use_tray_menu_event_handler (re-exported tray-icon, events pumped through tao loop). Verified live: left-click restores hidden window, menu items fire, Quit exits."}
    global_hotkey: {rating: built-in, note: "use_global_shortcut(\"super+shift+9\") — global-hotkey wired in. Fired incl. while unfocused/hidden."}
    native_menubar: {rating: built-in, note: "muda via Config::with_menu; predefined roles + custom Accelerators. TRAP: tray-icon shares muda's single global MenuEvent handler slot and dioxus installs the tray receiver last — menubar events arrive as TrayMenuEvent; handle menubar ids in use_tray_menu_event_handler."}
    dialogs: {rating: assembled, note: "rfd AsyncFileDialog in spawn(); native panels verified. dioxus-desktop already ships rfd internally, so zero new transitive deps."}
    clipboard_text: {rating: built-in, note: "WKWebView textarea + muda Edit roles; ⌘C/⌘V round-trip byte-exact incl. RTL/CJK/ZWJ."}
    clipboard_image: {rating: assembled, note: "arboard→image(png)→base64 data-URI thumbnail, verified with real clipboard screenshot. arboard rejects AppleScript's legacy TIFF flavor."}
    file_drop: {rating: built-in, evidence: "source/API-path", note: "wry drag-drop is on by default; Dioxus injects native paths into HTML ondrop (`evt.files()[].path()`). Finder drag was not synthesizable."}
    notification: {rating: assembled, evidence: "API-path; banner unobserved", note: "notify-rust returned Ok from the unbundled binary; reliable app attribution normally needs a bundled identity and no banner artifact was retained."}
    dark_mode_live: {rating: built-in, note: "CSS prefers-color-scheme switched live both directions. Wart: paint of the flip deferred while window idle+unfocused until next event."}
    multi_window: {rating: built-in, note: "window().new_window(VirtualDom, Config). TRAP: child Config needs .with_menu(None) or its default menubar clobbers the app-global macOS menu."}
    close_to_tray: {rating: built-in, note: "Config::with_close_behaviour(WindowHides) + with_exits_when_last_window_closes(false). Zero custom code."}
babel:
  build_ok: true
  canonical_measurement: {clean_s: 34, incremental_s: 1, deps_unique: 284, binary_bytes: 6092016, binary_stripped_bytes: 5192968}
  process_survived: true
  window_observed: true
  loc: 188
  fonts_bundled: none  # 0 bytes
  screenshot_ok: true  # all 11 corpus lines verified
  ratings:
    bidi_render: {rating: built-in, note: "RTL with embedded English + digits (incl. Arabic-Indic) correctly ordered."}
    cjk_render: {rating: built-in, note: "Proper system CJK fonts, zero tofu."}
    emoji_zwj: {rating: built-in, note: "ONE glyph (Apple silhouette design); skin tones and flags single color glyphs."}
    mixed_fallback_line: {rating: built-in, note: "All 8 scripts + emoji, silent per-script fallback, no tofu."}
    grapheme_caret: {rating: built-in, note: "Measured: one ← moved caret 11 UTF-16 units across the ZWJ cluster; one backspace deleted the whole family emoji CLEANLY (unlike iced/cosmic-text)."}
    selection: {rating: built-in, note: "Cluster-whole Shift+→; monotonic logical offsets across AR→HE with correct contiguous RTL highlight; round-trip byte-exact."}
    ime: {rating: built-in, evidence: "unexercised WebKit/API path", note: "Input-source switching was not scriptable; the stock WKWebView textarea path exists, but CJK IME behavior was not exercised."}
    large_doc_scroll: {rating: built-in, note: "11k lines: 181 frames/3.01s ≈ 60 fps sustained; RSS ~92-106 MiB. rAF throttles to ~12fps unfocused (WebKit power saving)."}
```

SURPRISES:
- dioxus-desktop 0.7 includes most of the tested shell: tray, menubar, global hotkey, clipboard text, file drop, dark mode, multi-window and close-to-tray rated built-in — 8 of 11 cells. Dialogs, clipboard image, and notifications used direct helper APIs.
- The one real wart: with the window idle+unfocused, background-task work (timer signal writes, new_window, even the PAINT of a live theme flip) is deferred until the next real windowing event wakes the tao loop.
- muda's single global MenuEvent handler slot: tray receiver (installed last) swallows menubar events — use_muda_event_handler goes silent once a tray icon exists.
- Babel's exercised rendering/editing matrix passed with zero text-related code, crates, or fonts, including the local caret/backspace probe. CJK IME was not exercised.

TIME_SINK:
- Fighting six sibling agents on one desktop (focus steals, theme toggles, clipboard overwrites, shared hotkey) — fixed with AX-based driving + env-gated selftest hooks.
- Clipboard-image forensics: three distinct failure sources untangled with a standalone arboard repro.
- Screenshot/window automation plumbing (CGWindowID, XPC-hosted save panels invisible to per-process AX).

## slint

```yaml
framework: slint
text_stack: "fontique + Parley + HarfRust for discovery/layout/shaping; swash rasterizes the FemtoVG/software path"
tray:
  build_ok: true
  canonical_measurement: {clean_s: 58, incremental_s: 5, deps_unique: 317, binary_bytes: 16654032, binary_stripped_bytes: 14870136}
  process_survived: true
  window_observed: true
  loc: 447          # 279 Rust + 3 build.rs + 165 .slint
  helper_crates: [rfd 0.15.4, arboard 3.6.1, notify-rust 4.18.0, global-hotkey 0.7.0]
  ratings:
    tray: {rating: built-in, note: "SystemTrayIcon DSL element (1.17, default feature) = real NSStatusItem with declarative reactive menu. Sharp edge: platform handle created once from a never-refiring change tracker — icon set from Rust after new() silently fails; must be @image-url at creation."}
    global_hotkey: {rating: assembled, note: "global-hotkey crate; no waker integration with Slint's loop — polled via 50 ms slint::Timer. Verified while unfocused/hidden."}
    native_menubar: {rating: built-in, note: "MenuBar DSL element = real macOS menu bar via muda; @keys(Control+N) maps to Cmd. No standard Edit roles — Cut/Copy/Paste hand-wired to the TextEdit's public functions."}
    dialogs: {rating: assembled, note: "No native file-dialog API in Slint (its `Dialog` is a rendered element); rfd::AsyncFileDialog awaited inside slint::spawn_local — no event-loop freeze."}
    clipboard_text: {rating: built-in, note: "TextInput/TextEdit paste native (copypasta in backend); verified via Edit→Paste round-trip."}
    clipboard_image: {rating: assembled, note: "arboard get_image → SharedPixelBuffer → slint::Image::from_rgba8, ~15 lines, zero drama."}
    file_drop: {rating: assembled, evidence: "source/API-path", note: "DropArea does not receive external drops in 1.17.1 (DataTransfer has no file-path representation, slint#1967). This app uses the unstable-winit-030 raw event filter; the path was source-verified, not exercised with Finder."}
    notification: {rating: assembled, evidence: "API-path; banner unobserved", note: "notify-rust returned Ok. Calling show() inside a slint::Timer callback hard-aborts ('Recursion in timer code'), so the app dispatches from its own thread; no retained banner artifact exists."}
    dark_mode_live: {rating: built-in, note: "Zero code; fluent style follows winit ThemeChanged. Verified live both ways."}
    multi_window: {rating: built-in, note: "Second Window component opens/closes independently. Gotcha: preferred-width/height overridden by root layout's preferred size — use min-*."}
    close_to_tray: {rating: built-in, note: "on_close_requested → HideWindow (even the default); app kept alive by run_event_loop_until_quit(). Verified full cycle."}
babel:
  build_ok: true
  canonical_measurement: {clean_s: 48, incremental_s: 4, deps_unique: 306, binary_bytes: 15445616, binary_stripped_bytes: 13910440}
  process_survived: true
  window_observed: true
  loc: 312          # 226 Rust/build.rs (including ~95 lines of test hooks) + 86 .slint
  fonts_bundled: none   # 0 bytes; fontique system fallback covered every script + emoji
  screenshot_ok: true   # all 11 corpus lines + [MIXED]-seeded editor
  ratings:
    bidi_render: {rating: built-in, note: "OCR token bounding boxes matched UBA run order for [AR]/[HE], including embedded English/digits. Remaining explicit base-direction/default-alignment and UI-mirroring work is tracked by open slint#2294."}
    cjk_render: {rating: built-in, note: "Perfect via fontique fallback (PingFang/Hiragino/Apple SD Gothic auto-discovered); no tofu."}
    emoji_zwj: {rating: built-in, note: "Family ZWJ = ONE color glyph; skin tones, rainbow flag, regional-indicator flags, mid-word ZWJ astronaut all correct."}
    mixed_fallback_line: {rating: built-in, note: "All scripts in one paragraph with per-run fallback; Devanagari conjuncts true ligated forms. One rare Zalgo mark → notdef box; tall stacks clip at row height."}
    grapheme_caret: {rating: built-in, note: "Split verdict: arrows/Shift+arrow move by grapheme (family = one unit), but backspace deletes by codepoint BY DESIGN (7 presses, through progressively smaller valid emoji — no corruption)."}
    selection: {rating: built-in, note: "Sane across BiDi boundaries; logical-order selection (correct split visual highlights). Clipboard round-trip byte-identical."}
    ime: {rating: built-in, evidence: "source/API-path; CJK unexercised", note: "No helper crate was used. Built-in plumbing exists end-to-end in source (winit Ime → TextInput preedit), but no CJK input source was available."}
    large_doc_scroll: {rating: built-in, note: "11k lines via VIRTUALIZED ListView: 120 µs model swap, steady 25-jump sweep, RSS 33→104 MiB. Caveat: ListView does the work — a single 11k-line TextEdit would be worse."}
```

SURPRISES:
- Slint 1.17.1 correctly reordered the tested BiDi runs (OCR-measured against UBA), revising iteration 1. Open #2294 tracks explicit paragraph base direction/default alignment and UI mirroring; codepoint-based backspace remains a separate editing behavior.
- SystemTrayIcon + MenuBar make six of the eleven recorded shell capabilities
  built-in (~55% under this rubric), with substantial declarative coverage;
  external file drop is the one SPEC-4 capability requiring an unstable
  feature because DataTransfer cannot represent a file path yet.
- Two distinct integration traps appeared: setting the tray icon from Rust
  after `new()` silently failed, while calling notify-rust inside a
  `slint::Timer` callback panicked the runtime. Only the latter is abort-class.
- Backspace intentionally deletes by codepoint while arrows move by grapheme —
  jarring but it passes through valid smaller emoji, unlike the dangling-ZWJ
  result in the tested Iced path.

TIME_SINK:
- Proving BiDi honestly (~40% of babel): Vision-OCR bounding-box methodology.
- The tray-icon-from-Rust dead end (~30% of tray): silent failure + i-slint-core source reading.
- Desktop shared with 6 parallel agents (hotkey contention, focus/clipboard stealing) — drove editor tests to in-process WindowEvent dispatch, where it emerged Key::End/Home are cfg'd out on Apple targets.

## gpui

```yaml
framework: gpui
tray:
  build_ok: true
  canonical_measurement: {clean_s: 67, incremental_s: 2, deps_unique: 412, binary_bytes: 8764400, binary_stripped_bytes: 7502040}
  process_survived: true   # process/window observed; 0.6% CPU / 95 MiB app RSS was a local, noncanonical anecdote
  window_observed: true
  loc: 1575         # main.rs 739 + editor.rs 836
  helper_crates: [tray-icon 0.24.1, global-hotkey 0.7.0, notify-rust 4.18, unicode-segmentation 1]
  ratings:
    tray: {rating: assembled, note: "tray-icon (NSStatusItem) just works on gpui's real AppKit runloop; events drained by an 80 ms gpui timer pump. Costs: no menu-bar-only mode (no activation-policy API), polling required."}
    global_hotkey: {rating: assembled, note: "global-hotkey (Carbon) drained in the same pump; ⌘⇧9 verified while another app frontmost."}
    native_menubar: {rating: built-in, note: "cx.set_menus + actions! + keymap = real macOS menu bar with accelerators and os_action Edit wiring; AX-verified. Gotcha: handlers touching windows must cx.defer or silently no-op."}
    dialogs: {rating: built-in, note: "cx.prompt_for_paths / prompt_for_new_path are native panels; scripted round-trips verified."}
    clipboard_text: {rating: built-in, note: "read_from/write_to_clipboard; round-trip verified against pbcopy/pbpaste."}
    clipboard_image: {rating: built-in, note: "SURPRISE: gpui's ClipboardItem decodes pasteboard images natively (ClipboardEntry::Image, PNG verified rendering via img()); no arboard needed."}
    file_drop: {rating: built-in, evidence: "source/API-path", note: "Typed .on_drop::<ExternalPaths>() + drag-move hover highlight, using the same path as Zed; code/API verified, but no Finder drop was performed."}
    notification: {rating: assembled, evidence: "API call observed; banner not delivered", note: "notify-rust returned Ok, but the unbundled cargo binary lacked app identity and macOS did not display the banner. A bundled identity is normally needed for reliable app-attributed delivery."}
    dark_mode_live: {rating: built-in, note: "observe_window_appearance + window.appearance() two-palette theme; live toggle capture-verified."}
    multi_window: {rating: built-in, note: "Second cx.open_window opens/closes/reopens independently; stale WindowHandle detected and recreated."}
    close_to_tray: {rating: assembled, note: "on_window_should_close veto works, but gpui has no per-window hide/orderOut — 'hide' is app-level cx.hide(), which AppKit ignores during NSMenu dismissal (needed 300 ms settle)."}
babel:
  build_ok: true
  canonical_measurement: {clean_s: 55, incremental_s: 1, deps_unique: 398, binary_bytes: 5615696, binary_stripped_bytes: 4771256}
  process_survived: true   # process/window observed; 0.5% CPU / 80 MiB app RSS was a local anecdote
  window_observed: true
  loc: 1090         # main.rs 254 + editor.rs 836 (shared with gpui-tray)
  fonts_bundled: none   # 0 bytes; CoreText fallback covered every script
  screenshot_ok: true   # all 11 lines + editor
  ratings:
    bidi_render: {rating: built-in, note: "macOS-only inheritance: gpui lays out each line as one CTLine and paints CTRun visual positions — full UBA free. [AR]/[HE] correctly reordered incl. both digit systems. Proven by CTLine caret probes + instrumented pixel-scan A/B, not by eye."}
    cjk_render: {rating: built-in, note: "CoreText cascade, zero config, no tofu."}
    emoji_zwj: {rating: built-in, note: "One glyph (Apple silhouette design), skin tones + all three flags correct, inline ZWJ doesn't break shaping."}
    mixed_fallback_line: {rating: built-in, note: "Every script tofu-free; adjacent Arabic+Hebrew merge into one RTL run and swap correctly; Zalgo stacks."}
    grapheme_caret: {rating: hand-rolled, note: "No gpui text widget; our editor + unicode-segmentation. Scripted proof: one Shift+← selected exactly the 25-byte family cluster; one backspace deleted it whole, byte-verified."}
    selection: {rating: hand-rolled, note: "All verified — but an endpoint inside the RTL run splits highlight from content: gpui's x_for_index assumes monotonic logical→visual order. The RTL deficit is caret/selection GEOMETRY, not rendering."}
    ime: {rating: assembled, evidence: "self-test/synthetic-input (dead key); CJK unexercised", note: "EntityInputHandler supplies the NSTextInputClient bridge including bounds_for_range; ⌥e dead-key preedit→commit was exercised, but no CJK IME was available."}
    large_doc_scroll: {rating: built-in, note: "Local unretained probe: uniform_list loaded 11k lines at +4 MiB app RSS and scrolled end-to-end in ~4 s at ~35% of one core. Trade-off: uniform row height, no wrapping."}
```

SURPRISES:
- gpui renders CORRECT full BiDi on macOS despite RTL being officially unsupported (zed#31102): the mac backend paints CTLine visual positions — UBA reordering, digit handling, Arabic+Hebrew run merging all platform-correct free; the deficit is confined to caret/selection geometry (logical==visual assumption).
- Reading mixed-direction screenshots by eye is dangerously unreliable — contradictory readings from the same image; settled only by CTLine caret probes + pixel-column scans.
- Hand-rolled editor + unicode-segmentation passed the harshest scriptable tests byte-exactly.
- In the chosen implementations, 11,000 multi-script rows cost +4 MiB app RSS in GPUI and scrolled end-to-end in ~4 s, versus Iced's local +313 MiB / 803 ms load observation. These unretained probes used different mechanisms and are not a controlled framework benchmark.

TIME_SINK:
- Proving BiDi correctness honestly (CoreText ground-truth renders, caret probes, pixel scans).
- Scripted editor verification choreography on the shared desktop (clipboard sentinels, re-activation before every keystroke burst).
- The 836-line hand-rolled editor remains gpui's biggest structural cost — every editor rating is "our code", not the framework's.

## egui

```yaml
framework: egui
tray:
  build_ok: true
  canonical_measurement: {clean_s: 36, incremental_s: 1, deps_unique: 167, binary_bytes: 13447056, binary_stripped_bytes: 11814216}
  process_survived: true
  window_observed: true
  loc: 514
  helper_crates: [tray-icon 0.24.1, global-hotkey 0.7.0, rfd 0.17.2, arboard 3.6.1, "REJECTED: notify-rust 4.18"]
  ratings:
    tray: {rating: assembled, note: "tray-icon; must be created in the eframe creator closure (main thread, post-NSApp init). Template icon gives auto light/dark tinting; menu events AX-verified."}
    global_hotkey: {rating: assembled, note: "global-hotkey (Carbon), fires while hidden — but events must be drained in App::logic since hidden viewports never run ui."}
    native_menubar: {rating: assembled, note: "muda via tray_icon::menu re-export, init_for_nsapp from creator closure. Use the re-export so tray + menubar share the single global MenuEvent channel."}
    dialogs: {rating: assembled, note: "rfd genuine native panels; blocking on main thread works (AppKit nested modal loop)."}
    clipboard_text: {rating: built-in, note: "⌘V into TextEdit is core egui. But PredefinedMenuItem cut/copy/paste can't work (egui's NSView implements no responder-chain selectors) — bridged via custom items injecting egui::Event::{Cut,Copy,Paste}."}
    clipboard_image: {rating: assembled, note: "egui clipboard is text-only; arboard → ColorImage → load_texture. Verified."}
    file_drop: {rating: built-in, evidence: "source/API-path", note: "raw.dropped_files carries paths; verified by code review/API contract, not a performed Finder drop."}
    notification: {rating: hand-rolled, evidence: "observed-local; no structured trace retained", note: "notify-rust was rejected after frame scheduling failed in 2/3 local reproductions; delegate replacement is the leading inferred cause pending a minimized repro. The shipped osascript fallback displayed locally."}
    dark_mode_live: {rating: built-in, note: "ThemePreference::System default; both themes across OS flips, screenshot-verified."}
    multi_window: {rating: built-in, note: "show_viewport_immediate About window independent (AX-verified). Immediate viewports only render while parent ui runs."}
    close_to_tray: {rating: assembled, note: "CancelClose + Visible(false) easy; LANDMINE: eframe 0.35 never calls App::ui for hidden viewports — reopen logic MUST live in App::logic or the window can never come back."}
babel:
  build_ok: true
  canonical_measurement: {clean_s: 24, incremental_s: 1, deps_unique: 156, binary_bytes: 31674480, binary_stripped_bytes: 30192184}
  process_survived: true
  window_observed: true
  loc: 314          # ~170 app + ~140 cfg(test) glyph probes
  fonts_bundled: "6 Noto fonts via include_bytes!, 18,963,776 B ≈ 18.1 MiB (NotoSansCJKsc alone 16,437,364 B ≈ 15.68 MiB); canonical raw binary 31,674,480 B (30.2 MiB) — egui has no system-font discovery/fallback"
  screenshot_ok: true  # all 11 lines, no tofu; monochrome emoji
  ratings:
    bidi_render: {rating: not-achievable, note: "No paragraph BiDi in the tested epaint path (explicit TODO, font.rs:830). Per-word RTL shaping makes single AR/HE words look correct while multi-word RTL sentences read backwards and Arabic-Indic digits mirror: ١٢٣ renders ٣٢١ (glyph x-coords verified). This is a severe visual-ordering error; the underlying buffer remains intact. Fixing it requires replacing or changing the epaint text path."}
    cjk_render: {rating: assembled, note: "Flawless in the screenshot, but only via the bundled 16,437,364-byte (15.68 MiB) NotoSansCJKsc; no system-font discovery was built in."}
    emoji_zwj: {rating: assembled, note: "harfrust GSUB genuinely ligates: family = ONE glyph (probe-verified), skin tones + 🏳️‍🌈 too — but MONOCHROME outlines only (no COLR/CBDT); RIS flags render as boxed 'UN'/'RS' letters."}
    mixed_fallback_line: {rating: assembled, note: "All scripts, no tofu, via hand-ordered 6-font fallback (order is load-bearing). Uncovered combining marks drop silently."}
    grapheme_caret: {rating: not-achievable, note: "Caret and backspace are scalar-based (index±1). Live-verified: caret parks inside 👨‍👩‍👧‍👦, typing splits the ligature; backspace strips one scalar per press (dangling ZWJ reshapes to family-of-3); 7 presses to delete."}
    selection: {rating: built-in, note: "All mechanics work (byte-exact copies across 5 scripts). Granularity per-scalar — can select half a ZWJ cluster. BiDi-boundary selection trivially contiguous since visual order = logical order (i.e., because rendering is wrong)."}
    ime: {rating: built-in, evidence: "source/API-path; CJK unexercised", note: "Full winit→egui IME plumbing includes preedit rendering; verified in source, not with a CJK input source."}
    large_doc_scroll: {rating: built-in, note: "Local unretained probe: 11k lines stayed smooth via ScrollArea::show_rows virtualization; app RSS 117→119 MiB. This opt-in assumes uniform row height; a naive label loop would shape every row."}
```

SURPRISES:
- egui's RTL failure is subtler than "no BiDi": harfrust shapes each whitespace-word correctly, so single RTL words (and [MIXED]) look perfect while multi-word sentences silently read backwards — and Arabic-Indic digit runs MIRROR (١٢٣→٣٢١), easy to miss in review.
- A local AXSelectedText query for an Arabic selection returned doubled/reordered characters while ⌘C was byte-exact. Actual screen-reader output was not exercised, so assistive-technology impact is inferred and needs a retained minimized reproduction.
- ZWJ emoji, skin tones, Latin ligatures all genuinely ligate (harfrust GSUB) — the gap is purely color (monochrome emoji, boxed-letter flags) and editing granularity, not shaping.
- In this tested macOS/eframe setup, frame scheduling failed after notify-rust
  in two of three local reproductions. Delegate replacement is the leading
  inferred cause, not a proven permanent mechanism; a minimized retained
  reproduction is still needed.

TIME_SINK:
- Fighting flaky synthetic input for live editor tests (~40%): unlocked by batching per AX-activation window + reading the editor back via the AccessKit value attribute.
- Disambiguating RTL rendering: advancing-only and digit-x-coordinate probes to prove word-internal reversal vs line-level order.
- (Inherited) font hunting and fallback-order iteration — order is load-bearing, ~18.1 MiB bundled.

## xilem

```yaml
framework: xilem
tray:
  build_ok: true
  canonical_measurement: {clean_s: 35, incremental_s: 2, deps_unique: 169, binary_bytes: 12234880, binary_stripped_bytes: 10459512}
  process_survived: true
  window_observed: true
  loc: 774          # main.rs 403 + shell.rs 371
  helper_crates: [masonry_winit =0.4.0, tray-icon 0.21, global-hotkey 0.7, rfd 0.15, arboard 3, "notify-rust 4 (REJECTED on macOS)"]
  ratings:
    tray: {rating: assembled, note: "tray-icon; only possible because masonry_winit's external-event-loop embedding hands you ApplicationHandler::new_events(Init) — stock Xilem::run_in has no hook."}
    global_hotkey: {rating: assembled, note: "global-hotkey (Carbon) registered at Init; ⌘⇧9 verified while unfocused/hidden."}
    native_menubar: {rating: assembled, note: "muda init_for_nsapp; trap: predefined cut/copy/paste silently swallow ⌘X/C/V from masonry — fixed with accelerator-less items injecting synthetic TextEvents. (Same muda trap as iced/egui.)"}
    dialogs: {rating: assembled, note: "rfd sync panels on main thread inside winit dispatch; modal runloop coexists fine."}
    clipboard_text: {rating: built-in, note: "masonry ClipboardPaste + TextArea ⌘X/C/A; pbpaste-verified."}
    clipboard_image: {rating: assembled, note: "arboard → peniko ImageData → stock image() view; verified."}
    file_drop: {rating: assembled, evidence: "source/API-path", note: "masonry_winit 0.4 drops WindowEvent::DroppedFile; this app catches it in its own ApplicationHandler before forwarding. Code-path verified, but no Finder drop was performed."}
    notification: {rating: hand-rolled, evidence: "observed-local; no structured trace retained", note: "notify-rust hit three local failure modes (LaunchServices chooser, winit runloop-reentrancy panic-abort, and dropped banners); the shipped out-of-process osascript fallback displayed locally."}
    dark_mode_live: {rating: hand-rolled, note: "Works live but everything manual: ThemeChanged intercepted at winit layer, initial theme via defaults read, hand-maintained palette (masonry 0.4 theme is dark-only)."}
    multi_window: {rating: built-in, note: "First-class in 0.4: window-iterator app logic. No API to reposition/hide a window after creation."}
    close_to_tray: {rating: assembled, note: "Wrapper AppDriver intercepts on_close_requested → set_visible(false); tray/hotkey restore verified."}
babel:
  build_ok: true
  canonical_measurement: {clean_s: 27, incremental_s: 2, deps_unique: 143, binary_bytes: 12011424, binary_stripped_bytes: 10288040}
  process_survived: true   # survives editor abuse — only Load big doc kills it
  window_observed: true
  loc: 110
  fonts_bundled: none   # deliberate — records that stock fontique discovery fails Han on macOS 26
  screenshot_ok: true   # all 11 lines verified
  ratings:
    bidi_render: {rating: built-in, note: "Flawless, verified glyph-by-glyph from zoomed slices: RTL runs reversed correctly, digit order kept, trailing period resolves to LTR paragraph level."}
    cjk_render: {rating: assembled, note: "The chosen zero-bundled-font stock path showed [ZH] near-total tofu, [JA] kanji tofu/kana fine, and [KO] perfect. The tested fontique 0.6 coretext.rs scans */Library/Fonts while macOS 26 keeps PingFang in AssetsV2 on-demand storage, so Han fallback returned None. Bundling Noto CJK is an available application fix; the stock-discovery path is the gap and needs a minimized upstream reproduction."}
    emoji_zwj: {rating: built-in, note: "Family ONE color glyph; skin tone, 🏳️‍🌈, astronaut correct — but country flags are tofu pairs (RI sequences never ligate)."}
    mixed_fallback_line: {rating: assembled, note: "In the chosen stock zero-font path, ten scripts had exactly one hole: 世界 tofu from the same Han discovery issue. Bundling an appropriate CJK font is an available fix."}
    grapheme_caret: {rating: not-achievable, note: "The tested Masonry/Parley 0.6 editor path moved by Unicode scalar despite ligated rendering: Shift+Left×3 selects '👦cd', and backspace leaves a dangling ZWJ (hexdump-verified). This describes that stock integration, not Parley's layout engine in every possible editor."}
    selection: {rating: built-in, note: "Logical and sane across the BiDi boundary; mouse drag gave continuous highlight and exact logical substring; round-trips byte-perfect."}
    ime: {rating: built-in, evidence: "source/API-path; CJK unexercised", note: "Source-verified: winit Ime → TextEvent::Ime → TextArea Preedit/Commit plus StartIme/ImeMoved candidate-window positioning. It was not exercised with a CJK input source."}
    large_doc_scroll: {rating: hand-rolled, note: "The chosen monolithic-prose path locally panic-aborted twice: 11k lines in one prose → RSS 114→613 MiB in ~1 s, then a wgpu validation error (192 MiB scene buffer > 128 MiB max_storage_buffer_binding_size). Exact raw traces were not retained. Xilem 0.4 ships virtual_scroll as the intended scalable path, but this app did not wire or test it."}
```

SURPRISES:
- fontique 0.6's Han fallback fails at FILE DISCOVERY, not matching: macOS 26 moved PingFang into /System/Library/AssetsV2/ which fontique never scans, while other scripts' fonts still live in /System/Library/Fonts — producing the kana-render/kanji-tofu split inside one [JA] line. Upstream-actionable bug.
- In two local observations, this app's monolithic `prose` implementation
  panic-aborted with a wgpu buffer-binding validation error. That does not
  establish a framework-wide limit because `virtual_scroll` was not tested and
  the exact raw traces were not retained.
- Rendering ligated the family emoji into one glyph, but the tested
  Masonry/Parley 0.6 editor path stepped through it scalar-by-scalar and left a
  dangling ZWJ on backspace.
- BiDi and logical selection are flawless out of the box — the exact opposite ends of maturity in the same text stack.

TIME_SINK:
- Screen warfare with sibling agents (pixel-verifying the button under the cursor was OURS before every click; relocating the window twice).
- The system clipboard is a shared global raced by parallel agents — every assertion became an atomic activate→keys→pbpaste sequence with sentinels.
- Proving BiDi from screenshots: vision transcription re-orders RTL text; needed 5×-zoomed line slices to pin actual visual token order.

## freya

```yaml
framework: freya
version: "=0.4.0"
cohort: 2026-08-03-expansion
supersedes: "the 2026-08-02-expansion freya block previously in this file. Those crates were never retained; the apps were re-implemented from scratch on 2026-08-03 and the LoC, helper-crate list and several ratings differ materially (see the conflict note at the end of this section)."
text_stack: "Skia textlayout (HarfBuzz shaping + ICU BiDi) over SkFontMgr/CoreText, via the freya-skia-safe 0.98.1 fork; editing model is freya-edit (ropey + unicode-segmentation)"
tray:
  build_ok: true          # cargo build --release clean; --locked --release reproduces
  canonical_measurement: pending_new_cohort
  process_survived: true
  window_observed: true
  interaction_evidence: "AX clicks on both menu bars, synthetic CGEvent mouse + keystrokes, osascript dark-mode toggle, window-scoped screencapture"
  loc: 865                # single src/main.rs
  loc_production: 805     # incl. ~90 LoC of hand-assembled multi-line text editor
  loc_verification: 60
  helper_crates: [global-hotkey 0.7.5, rfd 0.17.2, arboard 3.6.1, notify-rust 4.18, async-io 2.6.0]  # tray-icon/muda (via freya::tray), bytes, ropey and image deliberately NOT needed — Freya re-exports them
  ratings:
    tray: {rating: built-in, evidence: synthetic-input, note: "LaunchConfig::with_tray(builder, handler) behind the `tray` cargo feature. Freya owns the GLOBAL tray-icon/muda TrayIconEvent+MenuEvent handlers itself and forwards both into one TrayEvent callback, so the app never links its own muda copy — the channel-splitting trap iced/egui/dioxus hit cannot happen here. Builder closure runs on the main thread after loop start. Verified: AX click on `menu bar 2` → Show/Hide hid the window, New note re-showed it."}
    global_hotkey: {rating: assembled, evidence: synthetic-input, note: "global-hotkey 0.7.5 (Carbon RegisterEventHotKey, no accessibility permission); Freya has nothing here. Gotcha: the channel reports BOTH Pressed and Released, so an unfiltered toggle nets out to no change — filter on HotKeyState::Pressed. Verified: ⌘⇧9 removed the window from CGWindowListCopyWindowInfo and a second press restored it, from an unfocused state."}
    native_menubar: {rating: assembled, evidence: synthetic-input, note: "muda via freya::tray::menu (= tray_icon::menu); Menu::init_for_nsapp() from the root component's first render — a component render IS main-thread-after-loop-start, so no boot-task dance. `menu bar 1` reports Apple/freya-tray/File/Edit and AX clicks on File→New/About and Edit→Select All/Copy all fired. HEADLINE TRAP: muda stores a raw *const MenuChild in each NSMenuItem and does NOT retain it (FIXME in muda 0.17), so the idiomatic append-then-init shape leaves every item dangling — build is clean, menu renders, and the first click reads freed memory (observed: freed MenuChild reinterpreted as PredefinedMenuItemType::About with a zero-sized icon, panicking in muda's PNG encoder). Items must be parked for the process lifetime. Predefined Edit roles deliberately unused (dead Cocoa selectors + swallowed ⌘X/C/V); four custom items replay EditableEvent::KeyDown into the editor instead."}
    dialogs: {rating: assembled, evidence: observed, note: "rfd 0.17.2 FileDialog::pick_file()/save_file(); a real NSOpenPanel appeared in the process window list and Escape dismissed it. The synchronous API blocks the UI thread while the panel is up (matches macOS app-modal behaviour; AsyncFileDialog + spawn is the non-blocking form)."}
    clipboard_text: {rating: built-in, evidence: self-test, note: "use_editable implements ⌘C/⌘X/⌘V/⌘A itself on top of freya-clipboard. Verified end-to-end: typed text → Edit▸Select All → Edit▸Copy → pbpaste returned `clipboard roundtrip test`."}
    clipboard_image: {rating: assembled, evidence: self-test, note: "Freya's built-in Clipboard (freya-clipboard → copypasta) is TEXT-ONLY, so arboard 3.6 get_image() → ImageHandle::from_rgba(w,h,Bytes,AlphaType::Unpremul) → image() element. `paste-image: OK 700x632` and the thumbnail renders. Naming AlphaType requires the `engine` feature even though the image() element does not."}
    file_drop: {rating: built-in, evidence: source-only, note: ".on_file_drop(|e: Event<FileEventData>| …) is a first-class ELEMENT event with a PathBuf payload — no winit plumbing, and it attaches to any sub-tree rather than the whole window. A real Finder drag cannot be synthesized with CGEvents (no drag pasteboard session), so the handler was not exercised."}
    notification: {rating: assembled, evidence: "self-test; banner unobserved", note: "notify-rust 4.18 .show() returns Ok from the unbundled binary. TRAP: with no bundle identifier, mac-notification-sys asks the OS to choose one and pops a modal 'Choose Application' panel that blocks the UI thread indefinitely (observed — it froze the self-test until dismissed); notify_rust::set_application(\"com.apple.Terminal\") before show() makes it non-interactive."}
    dark_mode_live: {rating: built-in, evidence: observed, note: "Platform::get().preferred_theme is a reactive State<PreferredTheme>; one use_side_effect swaps light_theme()/dark_theme() into the component theme context. Verified LIVE both directions with `osascript … set dark mode to not dark mode`: `theme-changed: Light` then `theme-changed: Dark`. Zero platform code."}
    multi_window: {rating: built-in, evidence: observed, note: "Platform::get().launch_window(WindowConfig::new(about_app)…).await returns the new WindowId; close_window(id) closes it. `About — Tray Notes (freya)` observed alongside the main window, independently closeable."}
    close_to_tray: {rating: assembled, evidence: synthetic-input, note: "WindowConfig::with_on_close(..) → CloseDecision::KeepOpen plus LaunchConfig::with_exit_on_close(false) (Freya also refuses to exit while a tray handler is registered). SHARP EDGE: AppWindow's `window` field is pub(crate), so the one hook that is ABOUT a window cannot hide it — it sets a flag that the 80 ms UI poll turns into Platform::with_window(None, |w| w.set_visible(false)). Verified: red close button removed the window from the window list while the process stayed alive."}
  bridge_note: "with_tray's handler runs on the renderer thread outside any component scope, so it cannot touch signals or call Platform::get(). It pushes menu ids into a static Mutex<VecDeque<String>> drained by an 80 ms async-io timer — the same loop that polls global-hotkey's crossbeam channel and the close-to-tray flag. Freya has no way to inject an event into the reactive runtime from the platform layer, and no timer primitive."
babel:
  build_ok: true
  canonical_measurement: pending_new_cohort
  process_survived: true
  window_observed: true
  loc: 530                # single src/main.rs
  loc_production: 440     # incl. ~110 LoC of hand-assembled multi-line text editor
  loc_verification: 90
  fonts_bundled: none     # 0 bytes; SkFontMgr + Freya's default_fonts() fallback resolved every script from macOS system fonts
  screenshot_ok: true     # window-scoped screencapture -l at 800x600: all 11 corpus lines + editor pane, no tofu anywhere
  ratings:
    bidi_render: {rating: built-in, evidence: observed, note: "Skia textlayout (HarfBuzz + ICU BiDi). [AR]/[HE] read RTL; embedded English words, Western digits (456/789) and Arabic-Indic ١٢٣ are ordered correctly inside the RTL runs, and the leading ASCII tag anchors the paragraph to LTR base. No API for forcing base direction."}
    cjk_render: {rating: built-in, evidence: observed, note: "[ZH]/[JA]/[KO] all render from system fonts via SkFontMgr fallback — full-width punctuation, 「brackets」, …… ellipsis and Hangul all correct. Zero configuration."}
    emoji_zwj: {rating: built-in, evidence: observed, note: "👨‍👩‍👧‍👦 renders as ONE glyph (Apple silhouette family design); 👍🏽 and 👩🏾‍🚀 take skin tone; 🏳️‍🌈, 🇺🇳 and 🇷🇸 are single flag glyphs; the inline e😀mo👩🏾‍🚀ji run does not break shaping."}
    mixed_fallback_line: {rating: built-in, evidence: observed, note: "Latin + 世界 + مرحبا + שלום + नमस्ते + ไทย + 한글 + ZWJ family in one paragraph without a single tofu box, in both the read-only pane and the editor."}
    grapheme_caret: {rating: not-achievable, evidence: self-test, note: "THE WORST TEXT FINDING OF THE COHORT. freya-edit's cursor is a UTF-16 offset and Arrow-Right advances by ONE UTF-16 CODE UNIT. Probed against RopeEditor (the core the widget drives) on `a👨‍👩‍👧‍👦b`: the family is 1 grapheme / 7 chars / 11 UTF-16 units spanning 1..12, and three Right presses give offsets [0,1,2,3] — offset 2 lands INSIDE the first surrogate pair. Backspace then deletes per code point: `a👨‍👩‍👧‍👦b` → `\u{200d}👩‍👧‍👦b` (dangling ZWJ), i.e. the cluster is corrupted AND a dangling ZWJ is left. unicode-segmentation is already a dependency of freya-edit and is not used for caret movement."}
    selection: {rating: assembled, evidence: "self-test + observed", note: "Mouse press/drag selection works and highlights render as a contiguous block (verified by dragging across the wrapped [MIXED] line and reading the capture back). Shift+Right extends by the same one-UTF-16-unit step, so a selection can also end mid-surrogate. Building the selection UI is app work: get_visible_selection(EditorLine::Paragraph(i)) per line fed into paragraph().highlights(..)."}
    ime: {rating: built-in, evidence: source-only, note: "Freya has a first-class on_ime_preedit element event (ImePreeditEventData { text, cursor }) and RopeEditor has real preedit state (set_preedit, preedit_text_segments, underline styling) which the stock Input wires up. This app's hand-assembled multi-line editor does not wire preedit, and no CJK input source is installed on the reference machine — nothing exercised."}
    large_doc_scroll: {rating: built-in, evidence: self-test, note: "11,000 lines behind VirtualScrollView::new_with_data(..).length(11_000).item_size(24.): the toggle is 0 ms (the state flip is 2.7 µs; nothing is materialised eagerly) and RSS moved only 104.6 → 107.3 MiB. Trade-off is the fixed item_size, so big-doc rows are max_lines(1); the 11-line corpus view uses a plain ScrollView with real wrapping paragraphs."}
```

SURPRISES:
- The rendering half of Freya's text stack is complete and needed zero work (BiDi, CJK, Indic, Thai, colour emoji, ZWJ all correct with 0 bytes of bundled fonts) while the EDITING half moves the caret in UTF-16 code units and can silently corrupt a ZWJ cluster with one Backspace.
- muda's dangling-`*const MenuChild` bug is a use-after-free on the first menu click in the idiomatic Rust shape, and Freya's release-only panic hook (rfd modal "Fatal Error" → exit(1) BEFORE chaining) hides the message: a panic in release is a frozen window plus an alert, nothing on stderr.
- Freya's tray feature owning the global muda/tray-icon handlers is the structural inverse of the iced/egui/dioxus channel-splitting trap — the app cannot get it wrong.
- `Platform` is a genuinely complete platform surface (launch_window/close_window/focus_window/with_window/post_callback plus reactive preferred_theme, accent_color, is_app_focused, scale_factor): live dark mode and multi-window cost ~5 lines each.
- Freya has no window-screenshot API, no timer primitive and no way to signal the reactive runtime from a platform callback — async-io + `screencapture -R` fill all three gaps.

TIME_SINK:
- The muda dangling-pointer bug (~40% of tray), amplified by the release-mode modal panic dialog hiding the backtrace.
- Writing a multi-line text editor twice (tray ~90 LoC, babel ~110 LoC): Freya's Input is hard-wired to max_lines(1), so a text area is assembled from use_editable — one paragraph element per line, a persistent non-reactive ParagraphHolder per line for hit testing, per-line selection ranges, manual key/pointer plumbing.
- The notify-rust "Choose Application" modal, and discovering the probes could run against RopeEditor directly (no window, no reactive runtime) which is what made the grapheme findings reproducible.

CONFLICT WITH THE 2026-08-02 ROWS (superseded):
- LoC: old freya tray 413 / babel 174; today's re-implementation is 865 / 530.
- helper_crates: the old row listed image 0.25.10, bytes 1.10.1 and ropey 1.6.1 for tray; today's app deliberately drops all three (Freya re-exports Bytes, Rope and takes raw RGBA) and adds async-io 2.6.0 as the timer.
- Evidence class: the old rows were `source-only` across the board; today's shell capabilities are synthetic-input/observed/self-test except file_drop (source-only) and notification (banner unobserved).
- Ratings that changed: babel grapheme_caret built-in → **not-achievable**; babel selection built-in → assembled; tray close_to_tray built-in → assembled; tray native_menubar keeps `assembled` but now carries the use-after-free trap; tray global_hotkey keeps `assembled` but the old "toggles every live window incl. About" behaviour is not what today's app does.

## vizia

```yaml
framework: vizia
version: "=0.4.0"
cohort: 2026-08-03-expansion
supersedes: "the 2026-08-02-expansion vizia block previously in this file. Those crates were never retained; the apps were re-implemented from scratch on 2026-08-03 (LoC, notification mechanism and several ratings differ materially — see the conflict note at the end of this section)."
text_stack: "Skia SkParagraph (skia-safe 0.93.1 with the textlayout feature) over CoreText's system font manager — shaping (HarfBuzz inside Skia), BiDi resolution and per-script fallback are all the platform's; no cosmic-text/fontdb layer exists"
tray:
  build_ok: true          # cargo build --release and --locked --release clean
  canonical_measurement: pending_new_cohort
  process_survived: true
  window_observed: true
  interaction_evidence: "CGEvent synthetic input scoped to this app's window and menu-bar extra, plus CGWindowListCopyWindowInfo assertions and window/menu-bar-strip screenshots"
  loc: 720                # single src/main.rs; RSS ≈ 108 MiB
  loc_production: 665
  loc_verification: 55    # opt-in TRAY_SELFTEST* hooks
  helper_crates: [tray-icon 0.24, global-hotkey 0.7, rfd 0.17, arboard 3.6, "image 0.25 (png only)", notify-rust 4]  # muda taken deliberately THROUGH tray-icon's re-export
  ratings:
    tray: {rating: assembled, evidence: "observed + synthetic-input", note: "vizia has nothing; tray-icon 0.24 NSStatusItem built on the FIRST TICK of a 100 ms timer so it happens after the winit run loop is up. vizia makes this unusually painless: Model has NO Send bound and Model::event runs on the main thread, so the !Send TrayIcon just lives in app state — no boot-task dance, no Option<Rc<..>> gymnastics. Hand-drawn 22×22 icon visible in the menu-bar strip capture; clicking New note produced `new-note`."}
    global_hotkey: {rating: assembled, evidence: synthetic-input, note: "global-hotkey 0.7 (Carbon RegisterEventHotKey, no accessibility permission). Verified twice in one run: ⌘⇧9 while the About window had focus removed the main window from the on-screen window list, and ⌘⇧9 again brought it back at the same position."}
    native_menubar: {rating: assembled, evidence: "observed + synthetic-input", note: "vizia ships a MenuBar VIEW (in-window, drawn by Skia) — not the macOS menu bar — so SPEC-4 needs muda, taken through the tray_icon::menu re-export ON PURPOSE: a separately resolved muda would own a different static MenuEvent channel and silently eat every click. Menu-bar strip capture shows `vizia-tray  File  Edit` and ⌘N fired new-note. Cosmetic gap: the app-menu title comes from the process name because the binary is unbundled."}
    dialogs: {rating: assembled, evidence: source-only, note: "rfd 0.17 BLOCKING API (FileDialog::pick_file/save_file) rather than the async one — vizia has no async executor to await on and Model::event is already on the main thread, so a modal NSOpenPanel needs no plumbing. Not driven under synthetic input (a modal panel on a shared desktop steals global focus); the non-interactive save path is exercised via TRAY_SELFTEST_SAVE instead."}
    clipboard_text: {rating: built-in, evidence: synthetic-input, note: "vizia's default `clipboard` feature (copypasta) plus TextEvent::{Cut,Copy,Paste,SelectAll}, which the app routes from the Edit menu with cx.emit_to(editor_entity, ..). Better than the iced equivalent: clipboard actions are ORDINARY EVENTS ADDRESSED TO A WIDGET, so the Edit menu is one emit_to per item instead of a hand-rolled clipboard task. ⌘A then ⌘C logged select-all/copy. PredefinedMenuItem cut/copy/paste avoided for the same reason as iced (dead selectors + swallowed key equivalents under winit)."}
    clipboard_image: {rating: assembled, evidence: self-test, note: "arboard 3.6 get_image() → image 0.25 RGBA→PNG → ContextProxy::load_image → Image view thumbnail. A 300×150-point (600×300-pixel) PNG round-tripped end to end (`paste-image: OK 600x300`). TRAP: Context::load_image takes &'static [u8] and is NOT reachable from an EventContext; the reachable route at event time is ContextProxy::load_image(String, &[u8], policy)."}
    file_drop: {rating: built-in, evidence: source-only, note: "Genuinely zero helper code: winit's DroppedFile is surfaced as WindowEvent::Drop(DropData::File(PathBuf)), and the SAME .on_drop(..) modifier used for in-app card dragging receives it. Not interactively verified — a Finder drag cannot be synthesized; handler and vizia_winit plumbing read in source."}
    notification: {rating: assembled, evidence: "self-test; banner unobserved", note: "notify-rust 4 (this replaces the 2026-08-02 app's detached osascript child). TWO real macOS traps, both found by crashing: (1) mac-notification-sys defaults to bundle id \"use_default\" and from an unbundled binary macOS 26 answers with a modal 'Choose Application' panel that appears WHILE .show() still returns Ok — fixed with notify_rust::set_application(..); (2) sending from inside vizia's event dispatch ABORTS the process — NotificationHandle::drop calls NSUserNotification, which spins the Cocoa run loop and re-enters winit's handler, panicking with 'tried to handle event while another event is currently being handled' inside a non-unwinding block. Fixed by sending from a plain background thread."}
    dark_mode_live: {rating: built-in, evidence: "observed at startup; live toggle deliberately not performed", note: "Zero code: vizia resolves its light/dark theme from the OS and re-resolves on WindowEvent::ThemeChanged(ThemeMode); the app only LISTENS in order to display the mode (`theme-changed: DarkMode` at startup, dark palette rendered). The osascript system-wide toggle was NOT performed — shared desktop with other agents' apps and screenshot runs in flight."}
    multi_window: {rating: built-in, evidence: synthetic-input, note: "Window::new(cx, content) inside a Binding, with .on_close/.title/.inner_size. Verified: pressing About created a second 360×212 window alongside the 500×452 main one (both listed by CGWindowListCopyWindowInfo) and it closes independently."}
    close_to_tray: {rating: assembled, evidence: synthetic-input, note: "The nicest structural result of this app: vizia's event manager visits MODELS on an entity BEFORE the VIEW on that entity, and the app model sits on the window entity — so the model sees WindowEvent::WindowClose first, calls meta.consume(), and answers with WindowEvent::SetVisible(false). vizia's own Window view never runs its close path, so should_close is never set. Verified: red traffic light removed the window from the on-screen list while the process kept running; ⌘⇧9 brought it back. vizia exposes no window-visibility GETTER, so the app tracks the flag itself."}
babel:
  build_ok: true
  canonical_measurement: pending_new_cohort
  process_survived: true
  window_observed: true
  loc: 241                # single src/main.rs
  loc_production: 171
  loc_verification: 70    # BABEL_SELFTEST probes + EDIT instrumentation
  fonts_bundled: none     # 0 bytes; every script incl. colour emoji from macOS system fonts through Skia's CoreText font manager
  screenshot_ok: true     # window-scoped screencapture -l at the spec's 800x600 unresized: all 11 lines visible and unwrapped, no tofu
  ratings:
    bidi_render: {rating: built-in, evidence: observed, note: "[AR]/[HE] read RTL; embedded `English words`, Western digits 456/789 and Arabic-Indic ١٢٣ visually ordered correctly, and the bracketed tags stay at the visual left because base direction is derived per paragraph from its first strong character. No API for forcing base direction."}
    cjk_render: {rating: built-in, evidence: observed, note: "[ZH]/[JA]/[KO] from system fonts with no configuration; full-width punctuation, 「brackets」 and …… all correct. No tofu."}
    emoji_zwj: {rating: built-in, evidence: observed, note: "👨‍👩‍👧‍👦 renders as ONE glyph (Apple silhouette family design); skin tone applied (👍🏽, 👩🏾‍🚀); 🏳️‍🌈, 🇺🇳 and 🇷🇸 are single flag glyphs; the inline e😀mo👩🏾‍🚀ji run does not break surrounding Latin shaping."}
    mixed_fallback_line: {rating: built-in, evidence: "observed + self-test", note: "Latin + Han + Arabic + Hebrew + Devanagari + Thai + Hangul + ZWJ emoji in one paragraph, no tofu and no visible seam. The probe confirms 8 distinct scripts in 114 bytes / 52 grapheme clusters (`PROBE line=MIXED … scripts=8`)."}
    grapheme_caret: {rating: "built-in for motion / hand-rolled edge for delete", evidence: self-test, note: "SPLIT VERDICT, proven by byte counts from the EDIT log. Motion and selection are grapheme-atomic: from `a👨‍👩‍👧x👨‍👩‍👧‍👦` (45 bytes, 4 clusters) two Shift+Left presses then Backspace removed exactly 26 bytes — the whole 25-byte family cluster plus `x` — leaving 19. Plain Backspace is NOT: at the end of `a👨‍👩‍👧‍👦` (26 bytes) one Backspace produced 22 bytes (dropped only U+1F466) and the next 19 (dropped only the ZWJ), corrupting the cluster. Same failure mode as iced from a completely different text stack: the delete path walks scalars while the movement path walks graphemes."}
    selection: {rating: built-in, evidence: "self-test + observed", note: "Mouse click/drag select in the editor; Shift+Left/Right extends by whole grapheme clusters (proven above); ⌘A selects all and ⌘C/⌘V round-trip through the system clipboard (default `clipboard` feature). Selection across the [MIXED] BiDi boundary stays contiguous in logical order."}
    ime: {rating: built-in, evidence: source-only, note: "Full IME plumbing exists: WindowEvent::{ImeActivate,ImePreedit,ImeCommit,SetImeCursorArea} and Textbox keeps a preedit_backup with TextEvent::{UpdatePreedit,ClearPreedit}, driven from winit's Ime events. Not exercised: no CJK input source on the reference machine and the dead-key path could not be driven reliably under synthetic input on a shared desktop."}
    large_doc_scroll: {rating: "built-in, expensive", evidence: self-test, note: "Load big doc builds 11 × 1000 = 11,000 lines (`BIGDOC lines=11000 generate_ms=0.93` — the DATA is free) as 11,000 Label VIEWS in one ScrollView. Responsive again after ~1.2 s wall and smooth scrolling afterwards, but RSS 114.9 → 1005.2 MiB, i.e. ≈82 KiB of RSS per one-line Label, flat across a long scroll. DELIBERATELY not virtualized: vizia ships VirtualList/VirtualTable (used in apps/vizia-grid) but using them here would measure the virtualizer rather than the text stack. The finding is that vizia's per-view overhead (entity + style store + Skia paragraph cache) is what makes a naive 11k-view document expensive, not the shaping."}
```

SURPRISES:
- vizia's entire i18n story is "Skia + CoreText", so it is exactly as good as the platform: 0 bytes bundled, nothing configured, no first-paint font-database stall, and the corpus rendered correctly on the first run.
- Two facts — Model has no Send bound, and models are visited before views on the same entity — give a clean home for !Send OS handles AND make close-to-tray a three-line intercept rather than a framework opt-out flag.
- Clipboard actions are addressable events (cx.emit_to(editor, TextEvent::Paste)), which makes an Edit menu trivial with no focus-dispatch machinery (at the cost of targeting one hard-coded widget).
- notify-rust cost ~40% of the tray app for two undiscoverable reasons: a modal "Choose Application" panel that coexists with an Ok return value, and a non-unwinding abort deep inside winit's event handler when sent from event dispatch.
- ≈82 KiB RSS per Label: the naive 11k-line document costs a gigabyte here, far more than in the immediate-mode members of the cohort.

TIME_SINK:
- The two notify-rust traps (~40% of tray); neither is discoverable from the crate docs.
- Getting the clipboard image onto a Skia surface: Context::load_image looks like the API, is &'static [u8], and is unreachable at event time.
- Writing FALSIFIABLE babel probes: vizia's Movement/Direction types are pub(crate), so TextEvent::MoveCursor/DeleteText cannot be constructed from app code — caret/grapheme behaviour had to be probed through real key events and read back through on_edit byte counts (which turned out to be a better test).

CONFLICT WITH THE 2026-08-02 ROWS (superseded):
- LoC: old vizia tray 382 / babel 107; today's re-implementation is 720 / 241.
- notification: the old row described "a detached osascript child … not timeout-managed"; today's app uses notify-rust 4 with two documented workarounds (set_application + background-thread send).
- helper_crates: old row listed tray-icon 0.24.1, global-hotkey 0.7.0, rfd 0.17.2, arboard 3.6.1, image 0.25.10; today's list is the same set plus notify-rust 4, with muda explicitly taken through tray-icon's re-export.
- global_hotkey mechanism changed: the old row's Arc<Mutex<Vec<_>>> + 100 ms Vizia timer bridge (because ContextProxy is not Sync) is not how today's app is built — the tray icon itself is created on the first 100 ms timer tick and the hotkey path is a plain callback.
- Evidence class: old rows were source-only/unexercised throughout; today's are synthetic-input/observed/self-test except dialogs and file_drop (source-only) and the dark-mode LIVE toggle (deliberately not performed on a shared desktop).
- Ratings that changed: babel grapheme_caret is now a SPLIT verdict (motion grapheme-atomic, Backspace scalar-based) rather than a flat `built-in`; babel large_doc_scroll is now `built-in, expensive` with the 1005 MiB datum instead of an unmeasured VirtualList claim; the old block's `combining_stress` cell has no counterpart in today's report and is dropped.

## floem

```yaml
framework: floem
version: "git-778bb5f2"   # rev 778bb5f2aa08429e579ee2e6ac97e84fbf18b618 of lapce/floem main (2026-06-21)
version_deviation: "SPEC.md wants the latest crates.io release pinned =x.y.z. Floem's latest is 0.2.0 (2024-11) — 20 months stale at measurement time, with a substantially different API (winit re-export, cosmic-text stack, no typed event listeners). The maintainers direct users to `main`, and `main` CANNOT be published: it depends on a forked winit (floem-winit) and on understory_* crates, both via git. The git rev is pinned identically across all 8 floem apps under the SPEC's git-fallback clause; the unpublishable-main situation is itself a headline ecosystem finding."
cohort: 2026-08-03-expansion
text_stack: "parley 0.7 + fontique 0.7 (system discovery/fallback) + swash 0.2 + harfrust 0.3 at this rev — NOT cosmic-text, which the stale crates.io 0.2.0 used; the editor pane is the Lapce editor core (lapce-xi-rope)"
tray:
  build_ok: true          # release build clean, locked rebuild too
  canonical_measurement: pending_new_cohort
  process_survived: true
  window_observed: true
  interaction_evidence: "self-test hooks (image paste, save, notification, screenshot) plus synthetic ⌘⇧9 keystrokes toggling the window both directions"
  loc: 610                # single src/main.rs
  loc_production: 550
  loc_verification: 60    # self-test hooks + PNG/screenshot helpers
  helper_crates: [tray-icon 0.24.1, global-hotkey 0.7, arboard 3.6, notify-rust 4, png 0.18]  # REJECTED: a direct muda dep (dangerous, see muda note) and rfd (floem ships dialogs built-in)
  ratings:
    tray: {rating: assembled, evidence: observed, note: "floem has nothing; tray-icon 0.24 NSStatusItem created in an exec_after callback (main thread, run loop live — tray-icon #90). Icon appears, menu opens, Show/Hide toggles. Event delivery is NICER than under iced: no polling at all, because global-hotkey and the tray hook wake the UI thread through ExtSendTrigger."}
    global_hotkey: {rating: assembled, evidence: synthetic-input, note: "global-hotkey 0.7 (Carbon RegisterEventHotKey). Its set_event_handler pushes into a queue and wakes the UI thread via floem's ExtSendTrigger + register_ext_trigger — floem is the only framework tested with a first-class 'poke the reactive graph from a foreign thread' primitive, which eliminates the 100 ms polling loop the iced port needed. Verified with synthetic ⌘⇧9 both directions, including while hidden."}
    native_menubar: {rating: built-in, evidence: observed, note: "HEADLINE. floem::Menu builds muda menus with per-item action CLOSURES and set_window_menu() installs them on NSApp — no MenuId bookkeeping, no event channel. File ⌘N/⌘O/⌘S work as accelerators. Same Edit-role trap as iced (muda PredefinedMenuItem cut/copy/paste go through Cocoa responder-chain selectors floem's winit-fork NSView doesn't implement), but the routing target is much better: Document::run_command(ClipboardCut/Copy/Paste/SelectAll) drives the real Lapce editor-core behaviour at the cursor/selection."}
    dialogs: {rating: built-in, evidence: observed, note: "rfd is a floem DEPENDENCY: floem::open_file/save_as(FileDialogOptions, callback), with the callback delivered back on the UI thread via create_ext_action. Zero integration code, and the app's own rfd dependency was rejected as redundant."}
    clipboard_text: {rating: built-in, evidence: source-only, note: "floem::Clipboard::get/set_contents plus the editor core's own ⌘C/⌘V bindings, which call the same Clipboard internally (verified in views/editor/text.rs). Menu-routed Cut/Copy/Paste exercise the same path. Headless typing into the editor was not scriptable without Accessibility, hence source-only for the in-editor bindings."}
    clipboard_image: {rating: assembled, evidence: self-test, note: "arboard::Clipboard::get_image() → RGBA. PAPERCUT: floem's img view only accepts ENCODED bytes, so the RGBA must be PNG-encoded in memory first (png 0.18) — there is no raw-pixels image view (iced has Handle::from_rgba). Verified: osascript-placed PNG → `paste-image: OK 64x48`, thumbnail rendered."}
    file_drop: {rating: built-in, evidence: source-only, note: "Typed listener::FileDragDrop event with paths: Rc<[PathBuf]> plus drop position; the handler loads .txt. Not interactively verified (a real Finder drag cannot be synthesized); plumbing confirmed in floem's app handle (DragDropped → file_drag_dropped)."}
    notification: {rating: assembled, evidence: "self-test; banner unobserved", note: "notify-rust 4 .show() returned Ok from the unbundled binary. Same macOS caveat as iced: banner display is gated on per-app notification approval, and no banner artifact was retained."}
    dark_mode_live: {rating: built-in, evidence: observed, note: "floem's default theme has a dark_mode() style selector re-resolved on the OS ThemeChanged event; the whole UI restyled live. The typed listener::ThemeChanged (winit Theme payload) surfaces the mode in the status bar (`theme-changed: dark` at startup)."}
    multi_window: {rating: built-in, evidence: observed, note: "new_window(view_fn, config) / close_window(id); the About window opens and closes independently."}
    close_to_tray: {rating: built-in, evidence: observed, note: "listener::WindowCloseRequested + cx.prevent_default() swallows the close; WindowIdExt::set_visible(false) hides. floem's macOS AppConfig even defaults to exit_on_close: false. BONUS vs iced: AppEvent::Reopen exposes Dock-icon reopen (applicationShouldHandleReopen), so the hidden window comes back on a Dock click — something iced structurally could not do."}
  muda_version_minefield: "floem pins muda =0.17 and claims that instance's single global MenuEvent::set_event_handler slot at Application::new() for its own menu system. tray-icon 0.24 bundles muda 0.19 — a SEPARATE compiled instance with a FREE handler slot, which this app hooks. The two coexist ONLY because the versions differ: had tray-icon resolved to muda 0.17.x, cargo would have unified the crates and floem's handler would silently swallow every tray-menu click (no error, no event). An app author has no way to see this except by reading floem's source. Price paid: two copies of muda in the binary."
babel:
  build_ok: true
  canonical_measurement: pending_new_cohort
  process_survived: true
  window_observed: true
  loc: 326                # single src/main.rs
  loc_production: 231
  loc_verification: 95
  fonts_bundled: none     # 0 bytes — with the documented CJK consequence below
  screenshot_ok: "partial — window-scoped screencapture -l of the release build; all 11 corpus rows present but [ZH]/[JA] Han+kana and the RIS flags render as tofu"
  ratings:
    bidi_render: {rating: built-in, evidence: observed, note: "[AR]/[HE] read RTL with embedded English and digits (456, 789, ١٢٣) correctly ordered — visually equivalent to the iced (cosmic-text) rendering."}
    cjk_render: {rating: not-achievable, evidence: observed, note: "HEADLINE. [ZH] and [JA] Han/kana render as PURE TOFU: fontique's fallback fails to resolve PingFang/Hiragino on macOS at this rev, while [KO] Hangul, [HI] Devanagari (with conjuncts) and [TH] Thai all render fine. 世界 inside the [MIXED] line is tofu too, in both the label pane and the editor. The same stack renders CJK on other platforms, so this reads as a macOS fallback-resolution bug rather than a missing feature; no app-side fix short of bundling a CJK font (deliberately not done — the gap IS the datum). Corroborating symptom: ⌘/⇧ symbols also render as tofu in floem-tray's labels."}
    emoji_zwj: {rating: "built-in (mostly)", evidence: observed, note: "👨‍👩‍👧‍👦 renders as ONE colour family glyph in both label pane and editor; skin tone 👍🏽 applied; 🏳️‍🌈 renders; 👩🏾‍🚀 is one glyph. BUT regional-indicator flags 🇺🇳 🇷🇸 are tofu (two boxes each) — RIS-pair → Apple flag glyph resolution is missing."}
    mixed_fallback_line: {rating: assembled, evidence: observed, note: "One paragraph walks Latin → Hebrew → Arabic → Devanagari → Thai → Hangul → emoji without tofu EXCEPT the Han run (世界); rated partial/assembled for that reason."}
    grapheme_caret: {rating: built-in, evidence: self-test, note: "The self-test drives the LIVE editor through Document::run_command (the same path as key bindings): caret byte-offsets over `a👨‍👩‍👧‍👦b` = [0, 1, 26, 27] — the 25-byte ZWJ family is ONE caret stop. Best grapheme behaviour in this three-framework cohort."}
    selection: {rating: built-in, evidence: "self-test + observed", note: "Shift+Right×16 → (0,16); ×22 crosses the BiDi boundary into Arabic → (0,24): byte-monotonic selection, no corruption. Mouse selection exercised interactively in the editor pane. Clipboard round-trip goes through the REAL system clipboard (floem::Clipboard + ClipboardPaste): `start שלום 世界 👨‍👩‍👧‍👦` → OK."}
    ime: {rating: built-in, evidence: source-only, note: "floem has IME plumbing (ImePreedit/ImeCommit events, set_ime_allowed/set_ime_cursor_area actions, an editor preedit field) but activating a CJK IME requires manual keyboard-source switching that this run could not script — unexercised."}
    large_doc_scroll: {rating: built-in, evidence: self-test, note: "11k lines through VirtualStack: 11.7 ms first frame after the big-doc swap, 112 fps scripted scroll (one 120 px step per frame for 5 s), RSS 105 → 106 MiB — flawless ONCE CORRECT. The asterisk is the taffy min-content TRAP: without min_height(0) on the scroll's flex ancestors, taffy sizes the scroll to min-content, the clip never applies, and the 'virtualized' list silently materializes EVERY line — first measured run was 961 ms first frame and 1.9 GiB RSS for the same 11k lines. Nothing warns; one obscure style line fixes it. (Same pathology diagnosed at 100k rows in floem-grid: 16 GiB and no window.)"}
```

SURPRISES:
- floem is the first framework in this experiment where menubar, dialogs, file-drop, dark-mode, multi-window AND dock-icon reopen are all built-in; only tray, hotkey, image clipboard and notifications needed crates. Shell integration is the most complete of the cohort.
- ExtSendTrigger / create_ext_action remove the poll-the-channels pattern entirely — no 100 ms drain loop anywhere in the app.
- The muda-version coupling is a silent-failure trap that only exists because floem's muda 0.17 and tray-icon's muda 0.19 are DIFFERENT compiled instances; version unification would have silently killed every tray-menu click.
- The text stack was swapped wholesale on main (cosmic-text → parley/fontique) and macOS Han/kana fallback is broken at this rev while Hangul/Devanagari/Thai/BiDi are fine — a partial regression invisible on other platforms.
- floem has no window-capture API (iced does); the screenshot hook resolves the NSWindow windowNumber through the brand-new WindowIdExt::with_window_handle (added in the very commit pinned here) plus objc2, then shells out to `screencapture -l`.

TIME_SINK:
- Reading floem source to discover what is built-in (menubar, dialogs, reopen, prevent_default on close) — none of it is documented anywhere outside the source at this rev.
- The muda handler-slot analysis.
- Editor plumbing: getting text in and out of the Lapce editor core (doc.edit_single(Selection::region(..)), rope.slice_to_cow) and routing menu items via run_command — powerful but wholly undocumented.
- Confirming the CJK tofu was real (retaking the screenshot window-scoped to rule out capture artifacts) and separating which scripts fail.
