# Corrections log — 2026-08-30

Triggered by the r/rust thread on the 2026-08 post. Every entry names what
the corpus said, what is true, how it was established, and which files
changed. Evidence for the re-checks lives in `report/data/verification/`.

## 1. Freya *does* support double-click (maintainer correction)

- **Said:** "Freya 0.4 has no double-click event — no `on_double_click`, no
  click count in `MouseEventData`"; the kanban timed presses itself (400 ms).
  Rated *hand-rolled*.
- **True:** Freya 0.4.x ships a multi-press classifier in the prelude:
  `EventsCombos::pressed(location) -> PressEventType::{Single, Double, Triple,
  Quadruple}` (freya-core 0.4.1 `src/events_combos.rs`, 500 ms / 5 px). It is
  what freya-edit uses for double-click word selection and what
  `WindowDragExt` uses for double-click-to-maximise. There is still no
  `on_double_click` handler and the classifier is not linked from the
  `on_press` docs, which is how it was missed.
- **Done:** `apps/freya-board/src/main.rs` now calls the classifier from
  `on_press` (timer, `Instant`, `last_press` state removed); rebuilt
  `--release --locked` (40 s, warning-free); re-verified with synthetic
  CGEvents — double-click opens the editor, single click does not, typing +
  Enter commits, Escape closes it while the input has focus. Rating changed
  to *assembled* (Escape plumbing is still hand-rolled). Updated
  `apps/freya-board/FRICTION.md`, `report/data/interactive-rows.md`,
  `report/data/stack-rows.md`, and three `dashboard.html` passages (the
  interaction-gap sentence, the freya verdict card, the round-2 matrix cell).

## 2. AccessKit: one active maintainer, and a grant we missed

- **Said:** authorship "dominated by Matt Campbell and Arnold Loubriat";
  "no later direct institutional AccessKit grant"; sponsors link → Campbell.
- **True** (repo re-clone 2026-08-29, `report/data/verification/accesskit-maintainers.md`):
  since mid-March 2026 Arnold Loubriat authored 42/58 commits, merged all 54
  PRs and cut every release; Campbell's last commit is 2026-03-04 (still
  reviews, still a crates.io owner). Loubriat is the only public org member
  and calls it spare-time work. NLnet's NGI0 Commons Fund has funded "iOS
  support for AccessKit" since 2025-11 (delivered as `accesskit_ios` 0.1.0 on
  2026-05-11) — feature funding, not maintenance. The commenter's "no GitHub
  sponsor" is slightly off: Loubriat's page lists two individual sponsors
  (Campbell is one) and no corporate one; the project's own sponsor link
  points at Campbell's page (6 sponsors, one company).
- **Done:** rewrote the "Bonus: AccessKit" paragraph, the funding
  recommendation and the load-bearing-crates row in `dashboard.html`;
  `report/00-ecosystem-map.md` §AccessKit governance; `report/30-primer.md`;
  `report/data/load-bearing-crates.md` row (bus factor 2 → 1).

## 3. The muda handler-slot trap had its direction inverted

- **Said:** "last registration wins — dioxus installs tray after menubar, so
  menubar events arrive as tray events".
- **True:** `MENU_EVENT_HANDLER` is a `OnceCell` whose `set` result is
  discarded (muda 0.19.3 `src/lib.rs:491,517`), so the **first** registration
  wins. dioxus-desktop 0.7.9 registers the menubar handler (`app.rs:449`)
  before the tray-menu one (`app.rs:470`): tray-menu clicks arrive as
  `MudaMenuEvent`, `use_tray_menu_event_handler` never fires. Already
  upstream as DioxusLabs/dioxus#4495 with the same root cause.
- **Done:** `apps/dioxus-tray/FRICTION.md`, `report/12` footnote 3,
  `report/19` T2(b), `shell-facade.html` T2(b).

## 4. muda use-after-free: real, not 0.17-only, fixed on `main`, unreleased

- **Said:** "(muda 0.17) … the idiomatic append-then-init shape drops the
  items and the first click reads freed memory".
- **True:** muda 0.17.2 and 0.19.3 carry identical code
  (`Cell<*const MenuChild>` ivar, `FIXME: Use Rc`). Dropping *item handles*
  is safe (the `Menu` holds `Rc` clones); dropping the owning
  `Menu`/`Submenu` after `init_for_nsapp` is what dangles. Reported as muda
  #202/#233 (closed as user error); fixed on `main` by PR #361 / a1550bd
  (2026-07-30); no release contains it. freya's "`PredefinedMenuItem::about`
  panics" is almost certainly the same bug.
- **Done:** `report/19` T3, `shell-facade.html` T3, `apps/freya-tray/FRICTION.md`,
  `apps/freya-tray/src/main.rs` comment.

## 5. "muda panics" on Linux → muda fails to guard; gtk-rs panics

- The panic text comes from gtk-rs's `assert_initialized_main_thread!`
  (gtk 0.18.2 `src/rt.rs:25`) reached via `gtk::Menu::new()` from muda
  `src/platform_impl/gtk/mod.rs:377`. The ask (check `gtk::is_initialized()`
  and return `Err`) stands. Updated `report/19` T6 and `shell-facade.html` T6.

## 6. "Polling tax: no waker integration" was overstated

- muda, tray-icon and global-hotkey all ship `set_event_handler` (a callback
  from the platform thread) and the docs recommend pairing it with an
  `EventLoopProxy`. Nine of ten integrations polled anyway; the structural
  problem is the one-shot `OnceCell` slot (item 3). Updated `report/19` T15,
  `shell-facade.html` T15 + the "9/10" tile, `apps/iced-tray/FRICTION.md`.

## 7. Smaller wording fixes

- floem requires muda `"0.17.1"` (caret), not `=0.17`; the unification hazard
  comes from an *older* tray-icon (0.21, muda 0.17), not a newer one
  (`apps/floem-tray/FRICTION.md`, `report/19` T2(c), `shell-facade.html`).
- global-hotkey cross-process contention: slint's "only one process can own
  the combo" was an inference; two other apps saw every registrant fire.
  Withdrawn as a finding (`apps/slint-tray/FRICTION.md`, `report/19` T9,
  `shell-facade.html` T9).

## 8. Notifications: mechanism corrected, one hypothesis refuted, one complaint retracted

- `use_default` is resolved by an AppleScript (`get id of application
  "use_default"`), not LaunchServices; the modal is AppleScript's chooser
  (already upstream: mac-notification-sys #81).
- notify-rust 4.18's macOS `show()` returns `Ok` before sending; the send
  happens in `Drop` and its error is discarded — that is the root cause of
  "Ok but no banner" (T12). The same release already ships a
  UserNotifications backend behind `preview-macos-un`; none of our apps used it.
- The eframe freeze after a notification is *not* delegate replacement
  (mac-notification-sys never touches `NSApp.delegate`); run-loop re-entrancy
  is the likely cause. Still unreproduced in isolation.
- Retracted: "freya's close hook can't touch its window (`pub(crate)`)" —
  `AppWindow::window_mut()` and `RendererContext::windows_mut()` are public.
- "8 of 10 frameworks can't drop the Dock icon" → none of the ten *apps* did;
  6 of 10 frameworks expose a route (tauri, eframe, slint, xilem, freya,
  dioxus), 4 do not (iced, vizia, floem, gpui).
- eframe's `App::logic` is documented as running while hidden; the gap is a
  missing cross-reference from `App::ui`.
- The arboard TIFF failure is specific to AppleScript's `as TIFF picture`
  flavor; real screenshots work. Two objc2 generations affect all seven
  winit-0.30 apps, not just xilem.
- Files: `report/19` (§3 items 6–7, T10–T14, secondary), `shell-facade.html`
  (T10–T13), `report/12` fn 7, `apps/egui-tray/FRICTION.md`,
  `apps/freya-tray/FRICTION.md`, `apps/vizia-tray/FRICTION.md`, `dashboard.html`.

## 9. Windows/Linux/packaging: three claims were ours, not upstream's

- freya 0/8 on Windows: the prebuilt-Skia asset for the pinned feature set
  *does* exist and downloads (HTTP 200, 22.3 MB); the curl(3) failure was
  machine-local and unisolated (plausibly a 272-character output path).
- tauri 0/8: `icons/icon.ico` is missing from *our* repo; tauri-build errors
  correctly.
- gpui-peek / dioxus-peek / floem-peek / floem-babel on Windows: our crates
  list `core-foundation`/`objc`/`objc2` unconditionally instead of under a
  macOS target table — our gating bug, not "permanent no-Windows-path".
- `FailedToCreateSurfaceForAnyBackend` carried an empty per-backend map;
  "Vulkan surface creation" was over-specific.
- "notify-rust toasts WORK on Windows" only proved `show()` returned `Ok`.
- `dx bundle` builds under an ad-hoc `desktop-release` profile, not
  "target/dx"; the docs gap is notarization, not signing.
- cargo-bundle has no AppleScript layout step; the retained `hdiutil`
  error string was itself reconstructed. gpui 0.2.2 on Linux renders via
  Blade/ash, not wgpu; the missing package was `libxkbcommon-x11-dev`.
- Files: `report/21`, `report/18`, `report/data/packaging-results.md`,
  `dashboard.html`.

## 10. Async/grid: two claims were wrong, three were "documented behaviour"

- ehttp's default timeout is 30 s and cannot kill an 8 s stream; the real
  (unreported) bug is that `fetch_raw_native(with_timeout)` ignores the flag
  for GET and inverts it for POST/PUT/PATCH.
- slint's `spawn_local` constraint is documented in its rustdoc (prescribes
  `async_compat`); "nothing in the API surface warns you" was wrong.
- iced's default executor having no reactor is not an iced defect; the
  `time::every` compile error is plain and documented.
- gpui: "only a NullHttpClient" → "no published transport".
- dioxus occlusion freeze: the gate is `poll_vdom` waiting for the webview's
  JS to ack each edit batch; already tracked as dioxus#5586 / PR #5587.
- report/16 fn ⁵: masonry *does* deliver `PointerState.modifiers`; only
  xilem 0.4's view layer lacks a hook. Sort-arrow tofu is the fontique bug.
- Files: `report/17`, `report/16`, `report/11`, `apps/slint-fetch/FRICTION.md`,
  `dashboard.html`.

## 11. Text stack: the AssetsV2 explanation was wrong; most defects are already fixed upstream

- fontique 0.6 (xilem) / 0.7 (floem — not 0.6 as stated) asks CoreText for a
  Han fallback and rejects the answer ("PingFang") because it is missing from
  its own directory scan; PingFang has **no readable font file** on macOS 26
  (nothing scannable under `AssetsV2` either). Fixed in fontique 0.8.0 (falls
  back to Heiti SC/TC). Same root cause as xilem-grid's ▲/▼ and floem-tray's
  ⌘/⇧ tofu. The Reddit post's "never scans AssetsV2" sentence needs a
  public correction.
- regional-indicator flags: a harfrust AAT shaping bug, fixed in parley
  0.8.0 / harfrust 0.5.2 — not a fallback gap.
- xilem 11k-line abort: ~161 MiB scene by GPU-free reconstruction (192 MiB
  as reported at the time; raw output not retained) against a 128 MiB
  `Limits::default()` that vello/masonry_winit request while the adapter
  allows 4 GiB; masonry's `render_text` encodes every line with no culling.
- ZWJ backspace, the five defective paths: iced (cosmic-text 0.15 — still on
  main, unreported), egui (per-char by design), xilem (parley — fixed on
  main by PR #715, unreleased), vizia (dead emoji state machine — unreported,
  one-line fix), freya (fixed in freya-edit 0.5.0-rc.1).
- Linux mono emoji: cosmic-text's hard-coded fallback order (#327), not just
  a distro artifact.
- Files: `report/13`, `report/18`, `apps/xilem-babel/FRICTION.md`,
  `apps/floem-babel/FRICTION.md`, `apps/freya-babel/FRICTION.md`,
  `dashboard.html`.

## 12. Round-2 interaction traps: two softened, one re-attributed, three labelled "already tracked"

- freya "ancestor `on_mouse_up` never fires in the empty column area": no
  dispatch mechanism supports the asymmetry — unconfirmed. The verified
  defect nearby is `DropZone` calling `stop_propagation()` unconditionally
  on mouse-up. `DropZone` sizing is fixed on freya main (#2151).
- freya "`Size::flex` collapses" → it *fills* (evaluates to the parent
  extent) unless the parent sets `Content::flex()`; the pairing is
  undocumented. `Input` multiline is fixed on main (#2192).
- iced "`text_input` captures Escape so `keyboard::listen()` never fires":
  the API is `iced::event::listen()`, which is documented to deliver
  uncaptured events only — behaviour, not a defect; the gap is an
  `on_escape` hook (iced#2678).
- Dioxus "events expose no element geometry": `element_coordinates()`
  exists; only the target size is missing (async via `onmounted`).
- masonry Escape-cancel is fixed on main (#1826); autofocus is still absent.
- Already tracked upstream and now labelled as such: egui drag-source click
  swallowing (egui#5822), wry `dragDropEnabled` (tauri#14373).
- Files: `apps/freya-board/FRICTION.md`, `report/data/interactive-rows.md`,
  `report/11`, `dashboard.html`.
