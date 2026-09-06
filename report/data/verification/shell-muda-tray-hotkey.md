# Verification report — tauri-apps shell crates (muda / tray-icon / global-hotkey)

Domain: "Tray Notes" round (`apps/*-tray`), `report/12`, `report/18` §Headline 2,
`report/19` §4 T1–T9/T15, `report/21` §Headline 2, `report/data/linux-rows.md`,
`linux-results/`.

Sources read: pinned crate sources under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`, floem git checkout
`~/.cargo/git/checkouts/floem-ab9be4e01bb293da/778bb5f/`, and upstream clones in
a local clone directory (`muda-upstream` @ `b3d70d1` 2026-08-27, `tray-icon-upstream`
@ `1c23131` 2026-08-10, `global-hotkey-upstream` @ `b5329b9` 2026-08-27).

---

## 0 · Pinned versions per app (from each app's Cargo.lock)

| app | muda | tray-icon | global-hotkey | winit / tao | notes |
|---|---|---|---|---|---|
| dioxus-tray | **0.17.2** | 0.21.3 | 0.7.0 | tao 0.34.8 | one unified muda (tray-icon 0.21 requires `muda "0.17"`) |
| egui-tray | 0.19.3 | 0.24.1 | 0.7.0 | winit 0.30.13 | |
| **floem-tray** | **0.17.2 AND 0.19.3** | 0.24.2 | 0.7.0 | (floem winit fork) | floem requires `muda = "0.17.1"`; tray-icon 0.24.2 requires `muda = "0.19.1"` → two compiled instances |
| freya-tray | **0.17.2** | 0.21.3 | 0.7.0 | winit 0.30.13 | |
| gpui-tray | 0.19.3 | 0.24.1 | 0.7.0 | — | |
| iced-tray | 0.19.3 | 0.24.1 | 0.7.0 | winit 0.30.13 | |
| slint-tray | 0.19.3 | — (ksni 0.3.5) | 0.7.0 | winit 0.30.13 | slint uses ksni for tray, muda for menubar |
| tauri-tray | 0.19.3 | 0.24.1 | **0.8.0** | tao 0.35.3 | |
| vizia-tray | 0.19.3 | 0.24.2 | 0.7.0 | winit 0.30.13 | |
| xilem-tray | **0.17.2** | 0.21.3 | 0.7.0 | winit 0.30.13 | |

tray-icon → muda requirement (Cargo.toml): 0.21.3 → `muda "0.17"`;
0.24.1 / 0.24.2 → `muda "0.19.1"` (`tray-icon-0.24.1/Cargo.toml:61-63`).
All three tray-icon versions re-export muda verbatim:
`pub mod menu { pub use muda::*; }` (`tray-icon-0.24.1/src/lib.rs:144-145`).

---

## M-1 (T1) — muda predefined Edit roles: dead selectors + stolen key equivalents (macOS)

**CLAIM** — `report/19-shell-facade-spec.md` T1; `apps/iced-tray/FRICTION.md:14`;
`apps/xilem-tray/FRICTION.md:36`; `apps/egui-tray/FRICTION.md:68-70`;
`apps/floem-tray/FRICTION.md:20`; `apps/freya-tray/FRICTION.md` "Third trap";
`report/12-shell-integration-results.md:37` (footnote 2).

**EVIDENCE CHECK** — supported. Three independent frameworks (iced, egui, xilem)
hit it and shipped the same workaround; four more (vizia, freya, floem, slint)
avoided it by prior knowledge. iced records a direct observation ("verified: ⌘V
pasted nothing").

**ROOT CAUSE — CONFIRMED (both halves located).**

1. muda maps the Edit roles to bare Cocoa responder-chain selectors, with the
   target left `nil` (only `About` gets an explicit target):
   `muda-0.19.3/src/platform_impl/macos/mod.rs:978-999`
   ```rust
   PredefinedMenuItemType::Copy      => Some(sel!(copy:)),
   PredefinedMenuItemType::Cut       => Some(sel!(cut:)),
   PredefinedMenuItemType::Paste     => Some(sel!(paste:)),
   PredefinedMenuItemType::SelectAll => Some(sel!(selectAll:)),
   ```
   `create_ns_item_for_predefined_menu_item` (`:854-893`) builds the item with
   that selector and *does not* call `setTarget` for these types → target `nil`
   → AppKit dispatches up the responder chain.
   The default accelerators (⌘X/⌘C/⌘V/⌘A) are attached as real NSMenuItem key
   equivalents by `MenuItem::create` (`:1133-1158`, `setKeyEquivalentModifierMask`).
   Identical code in muda 0.17.2 (`macos/mod.rs:976-997`) and on muda main
   (`src/platform_impl/macos/mod.rs:1029-1031`).

2. winit's `WinitView` implements **none** of `copy:`/`cut:`/`paste:`/`selectAll:`.
   Grep of `winit-0.30.13/src/platform_impl/macos/view.rs` for those selectors →
   zero hits; the view implements `NSTextInputClient`
   (`view.rs:243`) with `insertText:replacementRange:` (`:392`) and
   `doCommandBySelector:` (`:420`) only. winit also implements no
   `performKeyEquivalent:` anywhere in `platform_impl/macos/`, and its own default
   menu (`platform_impl/macos/menu.rs`) contains **no Edit menu at all** — only
   About/Services/Hide/HideOthers/ShowAll/Quit.

3. **Why the keystroke is swallowed** (the part the corpus asserts but never
   explains). muda turns AppKit's automatic menu validation off:
   `ns_menu.setAutoenablesItems(false)` — `muda-0.19.3/macos/mod.rs:134` (also
   `:338`, `:791`; same lines in 0.17.2; `:130` on main) — and then explicitly
   `setEnabled(self.enabled)` (= `true`) on the predefined item (`:880`).
   With autoenabling **on** (AppKit's default) AppKit would ask the responder
   chain whether anything implements `copy:`; finding nothing it would grey the
   item out, and a *disabled* item does not claim its key equivalent, so ⌘C would
   fall through to the key window/view. With autoenabling **off** the item stays
   enabled, `NSMenu performKeyEquivalent:` matches it, fires
   `sendAction:to:nil:from:` (which walks the chain, finds nothing, and returns
   NO), and the key event is consumed by the menu rather than delivered to the
   focused view. Confidence: HIGH for the mechanism (the two ingredients are in
   the source and the observed behaviour matches exactly); the final AppKit step
   is not source-visible (closed-source AppKit) — call it CONFIRMED-by-code +
   LIKELY-by-Cocoa-semantics.

**Whose bug is it?** Genuinely a three-way integration gap, but muda is where a
cheap fix lives:
- winit's position is defensible: it does not pretend to be an NSText responder
  and has no menu API, so implementing `copy:`/`paste:` would mean inventing
  events it has no model for. No winit issue found for this (searches
  `repo:rust-windowing/winit macOS menu bar edit`, `"key equivalent"` → nothing
  relevant). Filing against winit would be a large design request with no
  champion; **not recommended as the primary target**.
- muda is where the trap is *manufactured*: it ships items whose only possible
  behaviour is "do nothing and eat your shortcut" for every non-webview,
  non-NSText host, and documents none of it. `PredefinedMenuItem::copy/cut/
  paste/select_all` doc comments are literally "Copy menu item" etc.
  (`muda-0.19.3/src/items/predefined.rs:42-60`).

**UPSTREAM STATUS — still present on main.** muda main (`b3d70d1`) keeps the
selectors and keeps `setAutoenablesItems(false)`. PR #381 (`97c82c3`,
"render unsupported predefined menu items as disabled items") added
`PredefinedMenuItemType::is_supported_on_macos()` but lists Copy/Cut/Paste/
SelectAll as *supported* on macOS, so they stay enabled. Related open issue:
**muda #365** ("[macOS] Accessing Services submenu crashes app when using
`Menu::init_for_nsapp`") — same construction (Edit submenu of predefined roles
on a tao/wry window); the reporter guesses "related to the responder chain".
No issue found for the key-equivalent swallowing itself.

**FILE? YES — tauri-apps/muda** (docs + an opt-in behaviour change). Draft:

> **Title:** macOS: predefined Edit items (`copy`/`cut`/`paste`/`select_all`) no-op *and* swallow ⌘X/⌘C/⌘V/⌘A on hosts with no NSText responder (winit, masonry, egui, iced)
>
> **Repo:** tauri-apps/muda
>
> **Versions:** muda 0.17.2 and 0.19.3 (also `main` @ b3d70d1), macOS 26, winit 0.30.13.
>
> On macOS `PredefinedMenuItem::{cut,copy,paste,select_all}` are built with the bare
> Cocoa selectors `cut:`/`copy:`/`paste:`/`selectAll:` and no target
> (`src/platform_impl/macos/mod.rs:978-999`, `:854-893`), so they rely on the
> responder chain. That works for a WKWebView or an NSTextView, but winit's
> `WinitView` implements none of those selectors (winit 0.30.13
> `src/platform_impl/macos/view.rs` — it implements `NSTextInputClient` only), and
> neither do masonry/egui/iced views. Two things then happen:
>
> 1. clicking the item does nothing (expected — nothing responds), and
> 2. **the item still consumes its key equivalent**, because muda calls
>    `ns_menu.setAutoenablesItems(false)` (`macos/mod.rs:134`) and then
>    `setEnabled(true)` on the item (`:880`). With AppKit's automatic validation
>    the item would be disabled (no responder) and a disabled item does *not*
>    claim its key equivalent; with validation off, NSMenu claims ⌘C/⌘V/⌘X/⌘A
>    before the key window sees them.
>
> Net effect: adding a standard Edit menu **silently breaks copy/paste app-wide**
> in the app's own text widget. Three separate frameworks hit this independently
> in our survey (iced, egui, xilem) and each had to replace the predefined roles
> with custom items that re-inject synthetic clipboard events into one
> hard-coded widget.
>
> **Repro:** any winit app; `Menu::with_items(&[&Submenu::with_items("Edit", true,
> &[&PredefinedMenuItem::cut(None), &PredefinedMenuItem::copy(None),
> &PredefinedMenuItem::paste(None), &PredefinedMenuItem::select_all(None)])?])?
> .init_for_nsapp();` then type into any text widget and press ⌘V → nothing is
> pasted and the widget never sees the keystroke.
>
> **Asks (any one would help):**
> - Document on `PredefinedMenuItem::{cut,copy,paste,select_all}` that on macOS
>   they require a responder implementing the Cocoa editing selectors, and that
>   they capture their key equivalents regardless.
> - Consider not forcing `setAutoenablesItems(false)` for menus containing these
>   items (or expose `Menu::set_autoenables_items(bool)`), so AppKit greys them
>   out and lets the shortcut through when nothing responds.
> - Optionally: an opt-in mode where these items emit a normal `MenuEvent` instead
>   of a responder-chain selector, so a non-NSText host can implement them.
>
> Evidence: https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/iced-tray/FRICTION.md ,
> .../apps/xilem-tray/FRICTION.md , .../apps/egui-tray/FRICTION.md ,
> .../report/12-shell-integration-results.md

**Optionally also file a winit issue** (lower priority, "feature request"):
winit's macOS view could implement the standard editing selectors and surface
them as a new `WindowEvent`, which would make every muda/tao menubar work
out of the box. No such issue exists today. Recommend: mention it in the muda
issue and only open a winit issue if a maintainer asks for it.

---

## M-2 (T2) — one global `MenuEvent` handler slot

**CLAIM** — `report/19` T2 (a/b/c); `apps/dioxus-tray/FRICTION.md:22`;
`apps/floem-tray/FRICTION.md:40-52`; `report/12-shell-integration-results.md:38-39`
(footnote 3).

**ROOT CAUSE — CONFIRMED, but the corpus states failure shape (b) BACKWARDS.**

The channel is a process-global `Lazy` crossbeam channel plus a **`OnceCell`**
handler slot (`muda-0.19.3/src/lib.rs:487-527`, identical in 0.17.2):
```rust
static MENU_CHANNEL: Lazy<(Sender<MenuEvent>, MenuEventReceiver)> = Lazy::new(unbounded);
static MENU_EVENT_HANDLER: OnceCell<Option<MenuEventHandler>> = OnceCell::new();
...
pub fn set_event_handler<F: ...>(f: Option<F>) {
    if let Some(f) = f { let _ = MENU_EVENT_HANDLER.set(Some(Box::new(f))); }
    else { let _ = MENU_EVENT_HANDLER.set(None); }
}
```
`OnceCell::set` returns `Err` once occupied and the result is discarded with
`let _ =`. So the semantics are **first registration wins, all later ones are
silent no-ops** — *not* "last registration wins". `tray_icon::TrayIconEvent`
(`tray-icon-0.24.1/src/lib.rs:653-690`) and `global_hotkey::GlobalHotKeyEvent`
(`global-hotkey-0.7.0/src/lib.rs:87-125`) use the exact same pattern.

(a) **separate muda instance → separate channel — CONFIRMED.** Statics are
per-compiled-crate, and `tray_icon::menu` is a verbatim re-export of whatever
muda tray-icon resolved. Five frameworks in the corpus pre-empted this by going
through `tray_icon::menu`.

(b) **dioxus — CONFIRMED as a real bug, but the corpus has the direction wrong.**
`dioxus-desktop-0.7.9/src/app.rs:92-100` calls, in order,
`set_global_hotkey_handler()`, `set_menubar_receiver()`, `set_tray_icon_receiver()`.
`set_menubar_receiver` (`:442-453`) installs `muda::MenuEvent::set_event_handler`
→ `UserWindowEvent::MudaMenuEvent`. `set_tray_icon_receiver` (`:456-475`) then
calls `tray_icon::menu::MenuEvent::set_event_handler` → `TrayMenuEvent`; because
tray-icon 0.21.3 requires `muda "0.17"` and dioxus-desktop also depends on muda
0.17, cargo unifies them into **one** crate instance, so this second call hits an
already-occupied `OnceCell` and does nothing.
Therefore **everything** (menubar *and* tray menu) arrives as `MudaMenuEvent`, and
`use_tray_menu_event_handler` (`hooks.rs:60-68`) **never fires at all**.
The corpus says the opposite ("menubar events actually arrive as `TrayMenuEvent`
… `use_muda_event_handler` alone would go silent"). Our app registered both hooks
(`apps/dioxus-tray/src/main.rs:157-158`), so the app worked either way and the
routing direction was never actually measured — it was inferred, and inferred wrong.

(c) **floem — CONFIRMED with one wording fix.** floem
(`~/.cargo/git/checkouts/floem-ab9be4e01bb293da/778bb5f/Cargo.toml:64`) requires
`muda = { version = "0.17.1", default-features = false }` (a caret requirement,
not `=0.17` as `apps/floem-tray/FRICTION.md:44` says) and claims the slot in
`src/app/mod.rs:286`. tray-icon 0.24.2 requires `muda "0.19.1"`, which is
semver-incompatible, so two instances coexist and each has its own free slot.
The hazard is real but the trigger is the *reverse* of how FRICTION phrases it:
if the app had used **tray-icon 0.21.x** (muda 0.17) instead of 0.24.x, cargo
would have unified, floem's `Application::new()` registration would have won
(first-wins), and every tray-menu click would have gone to floem's dispatcher and
been dropped. Price today: two copies of muda in the binary.

**UPSTREAM STATUS**
- muda: **not documented, no issue found.** Searches over `tauri-apps/muda` for
  `set_event_handler` / `"event handler"` / `"global handler"` returned only #202,
  #233, #162, #128 (unrelated). The crate docs actively *recommend*
  `set_event_handler` for winit/tao users (`muda-0.19.3/src/lib.rs:164-186`)
  without mentioning that only one caller in the process can ever have it.
- dioxus: **already reported — DioxusLabs/dioxus #4495, open since 2025-07-31**,
  "use_tray_menu_event_handler no longer works". A commenter (`Aziks0`) already
  posted the exact root cause, including the `OnceCell` link and the conclusion
  "The one with the `MudaMenuEvent` happens to be the first, so this is the event
  that is sent every time, not `TrayMenuEvent`. Use `use_muda_event_handler`
  instead." Confirms our corrected reading and refutes the corpus's version.

**FILE?**
- **muda: YES (design request).** Draft:
  > **Title:** `MenuEvent::set_event_handler` is a process-global `OnceCell` — only the first caller in a process can ever receive menu events
  >
  > **Repo:** tauri-apps/muda (a companion issue applies to `tray-icon`'s
  > `TrayIconEvent` and `global-hotkey`'s `GlobalHotKeyEvent`, same pattern)
  >
  > `MENU_EVENT_HANDLER` is an `OnceCell` and `set_event_handler` discards the
  > `Err` from `OnceCell::set` (`src/lib.rs:491`, `:515-521`). The result is
  > first-registration-wins with no feedback: a second caller's handler is
  > silently dropped and it also can't fall back to `MenuEvent::receiver()`,
  > because `send` only uses the channel when the cell holds `None`.
  >
  > This bites whenever two layers of a program both use muda, which is now the
  > normal case because `tray-icon` re-exports muda:
  > * dioxus-desktop registers a menubar handler and then a tray handler on the
  >   same instance; the tray one is a no-op and its
  >   `use_tray_menu_event_handler` hook never fires
  >   (DioxusLabs/dioxus#4495).
  > * a framework that claims the slot for its own menu API (e.g. floem, freya)
  >   makes it impossible for the app to receive events for menus the app built
  >   itself — unless the framework happens to resolve a *different* muda major,
  >   i.e. correctness currently depends on a version mismatch.
  >
  > Asks, in increasing order of effort:
  > 1. Make `set_event_handler` report replacement (return the previous handler,
  >    or at minimum use a `Mutex`/`RwLock` so a later caller wins deliberately
  >    rather than being silently ignored), and document the current semantics.
  > 2. Better: per-`Menu`/per-item event routing — e.g. `Menu::on_event(impl Fn)`
  >    or a `MenuEventReceiver` owned by the `Menu`, so a library and its user can
  >    coexist without a global.
  >
  > Evidence: https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/report/19-shell-facade-spec.md (trap T2),
  > .../apps/floem-tray/FRICTION.md , .../apps/dioxus-tray/FRICTION.md
- **dioxus: NO new issue — #4495 already covers it and already has the diagnosis.**
  Optional: add a comment noting that the same slot collision also silently
  disables `use_tray_menu_event_handler` on macOS/Linux (the issue reports
  Windows 11 only), and that a fix is one line — have
  `set_tray_icon_receiver` not attempt a second muda registration and route
  everything through `MudaMenuEvent`, or dispatch to both hook types from the
  single handler.

**CORPUS CORRECTIONS:** `apps/dioxus-tray/FRICTION.md:22`,
`report/19-shell-facade-spec.md` T2(b), `report/12-shell-integration-results.md`
footnote 3, and `apps/dioxus-tray/src/main.rs:138-142` all state the routing
direction backwards. Correct text: *muda's handler slot is a `OnceCell` —
**first** registration wins; dioxus installs the menubar receiver first, so all
menu events (menubar and tray) arrive as `MudaMenuEvent` and
`use_tray_menu_event_handler` never fires (DioxusLabs/dioxus#4495).*
Also fix `apps/floem-tray/FRICTION.md:44` "`floem pins muda =0.17`" → `"0.17.1"`
(caret), and the hazard direction as described in (c).

---

## M-3 (T3) — muda use-after-free on menu click (macOS)

**CLAIM** — `apps/freya-tray/FRICTION.md:26-46`, `report/19` T3: "muda stores a
raw `*const MenuChild` in each NSMenuItem without retaining it; dropping the
items and clicking reads freed memory".

**ROOT CAUSE — CONFIRMED (soundness bug), with a precision fix to the claim.**

`muda-0.17.2/src/platform_impl/macos/mod.rs:1023-1030` (and **identically**
`muda-0.19.3:1025-1032`):
```rust
define_class!(
    #[unsafe(super(NSMenuItem))]
    #[name = "MudaMenuItem"]
    #[thread_kind = MainThreadOnly]
    // FIXME: Use `Rc` or something else to access the MenuChild.
    #[ivars = Cell<*const MenuChild>]
    struct MenuItem;
```
The pointer is set from a borrow of the Rust-side child
(`ns_menu_item.ivars().set(&*self)` — 0.19.3 `:842`, `:872`, `:915`, `:947`) and
dereferenced unconditionally on click:
```rust
let item = unsafe { self.ivars().get().as_ref() }
    .expect("MenuItem's MenuChild pointer was unset");   // :1057-1059
```

**Precision fix to the corpus wording:** dropping the *item handles* alone is
**safe** — `Menu::add_menu_item` clones the `Rc<RefCell<MenuChild>>` into the
menu's own `children` vec (`macos/mod.rs:146-165`, `Menu { children:
Vec<Rc<RefCell<MenuChild>>> }` at `:110-116`). What actually dangles is dropping
the **root `Menu`** (or a `Submenu` not otherwise retained) after
`init_for_nsapp()` / `TrayIconBuilder::with_menu`: `NSApp` retains the `NSMenu`
(and therefore the `NSMenuItem`s), the Rust `Menu` drops its `children`, the last
`Rc` goes away, and every ivar pointer in the still-live NSMenuItems dangles.
That is exactly the shape our freya app had before the fix — `install_menubar()`
built `menu`/`Submenu`s/items as locals and called `menu.init_for_nsapp()`
(`apps/freya-tray/src/main.rs:178-222`); the fix parks all of them in a
`thread_local` `Vec<Box<dyn Any>>` (`:236-262`).
The observed symptom is fully explained: garbage read as
`predefined_item_type == Some(About(Some(meta)))` with an icon of width 0 →
`PlatformIcon::to_png` → `png::Encoder::write_header().unwrap()` panics with
`FormatError { inner: ZeroWidth }` (`muda-0.17.2/src/platform_impl/macos/icon.rs:25-36`).

**Is this a legitimate use per muda's docs?** Yes — muda's own crate-level
example builds the menu and items as locals and calls `init_for_nsapp()`
(`muda-0.19.3/src/lib.rs:39-110`), and nowhere in the docs is there a statement
that handles must outlive the platform menu. This is a documentation gap on top
of a soundness bug.

**UPSTREAM STATUS — already reported (twice), and FIXED ON MAIN but UNRELEASED.**
- **muda #202** (closed 2024-09-23) "App crashes when clicking on a menu item on
  macOS" — same repro shape (winit + `Menu::with_items(...).init_for_nsapp()`),
  same debug symptom (`slice::from_raw_parts requires the pointer to be aligned
  and non-null`) and the same release symptom
  (`macos/icon.rs:29 ... FormatError { inner: ZeroWidth }`). Closed by the
  reporter with "this error can happen if the `Menu` hasn't been saved until
  program termination" — i.e. closed as user error, not fixed.
- **muda #233** (closed 2024-10-14) "About menu item crashes on macOS" — an
  eframe+muda repro that drops the `Menu` at the end of the frame closure. Same
  bug. This is also the most likely explanation for our own note that
  `PredefinedMenuItem::about(..)` "panics inside muda's macOS icon conversion"
  (`apps/freya-tray/src/main.rs:185-188`) — treat that note as *probably the same
  UAF*, not an independent About-specific bug (we did not re-test `about()` with
  everything retained).
- **Fixed on main:** commit `a1550bd` "fix(macos): replace raw MenuChild pointer
  with Rc (#361)", 2026-07-30, by Divanshu, co-authored by Amr Bashir. The ivar
  is now `#[ivars = Cell<Option<Rc<RefCell<MenuChild>>>>]`
  (`muda-upstream/src/platform_impl/macos/mod.rs:1088`) and the `FIXME` is gone.
  Changelog file `.changes/fix-macos-menu-item-use-after-free.md`:
  *"Fix a use-after-free on macOS when a menu item outlives its associated Rust
  menu item."* `Cargo.toml` on main is still `version = "0.19.3"`, so **the fix
  is not in any published release**: 0.17.2, 0.19.3 and everything between are
  affected.

**FILE? NO new bug report.** Two duplicate issues already exist and the fix is
merged. **Instead: post a short comment asking for a release** (and, if you want,
ask that #202/#233 be linked to a1550bd so searchers land on the answer):

> On tauri-apps/muda (or on #202): a1550bd (#361) fixes this — the `*const
> MenuChild` ivar is now `Rc<RefCell<MenuChild>>`. It is unreleased; every
> published version through 0.19.3 still dereferences a raw pointer on click, so
> dropping the root `Menu` after `init_for_nsapp()` is a use-after-free
> (`src/platform_impl/macos/mod.rs:1025-1059` in 0.19.3). Downstream this reaches
> a lot of people via `tray-icon` (0.21.x → muda 0.17, 0.24.x → muda 0.19) and
> the frameworks that re-export it. Any chance of a patch release? #202 and #233
> are the same bug and are closed as user error.

Also worth adding to the muda docs (can go in the same comment or the M-1 issue):
the crate-level example should say the `Menu` must outlive the platform menu.

**CORPUS CORRECTIONS:** (i) the bug is **not** "muda 0.17" only — 0.19.3 has the
identical code; (ii) "dropping the items" is imprecise — dropping *item handles*
is safe, dropping the owning `Menu`/`Submenu` is what dangles; (iii) add
"reported as muda #202/#233, fixed on main by a1550bd (#361), unreleased".

---

## M-4 (T5/T6/T7) — Linux

### T5 — `Menu::init_for_gtk_window` requires a GTK window: CONFIRMED, BY DESIGN
`muda-0.19.3/src/menu.rs:195-199`:
```rust
pub fn init_for_gtk_window<W, C>(&self, window: &W, container: Option<&C>) -> crate::Result<()>
where
    W: gtk::prelude::IsA<gtk::Window>,
    W: gtk::prelude::IsA<gtk::Container>,
    C: gtk::prelude::IsA<gtk::Container>,
```
There is no non-GTK attach API on Linux; the whole `platform_impl/gtk` backend
builds real `gtk::MenuBar`/`gtk::Menu` widgets. Matches the retained E0277 in
`report/data/linux-rows.md:74`. Documented at crate level only as
"Linux (gtk Only)" (`muda-0.19.3/src/lib.rs:9-13`).

**UPSTREAM STATUS:** a non-GTK Linux path is *in progress but not landed*:
muda **#239** (open, 2024-11-01) "Export more internals so muda can be used in
combination with ksni"; muda **#372** (closed) "Add Linux KSNI menu compatibility
model"; tray-icon **#201** (open) "Replace libappindicator with ksni",
**#339/#340** (both closed, drafts stacked on muda#372/#373 and tao#1262)
"Add Linux KSNI tray backend". A tray-icon maintainer (FabianLars, on #336) says
"we'll be switching out appindicator for ksni soon anyway". muda main also gained
a `gtk4` backend (`88e0025`, PR #272) — still GTK.
**FILE? NO** — the request already exists in several forms (#239, #372, #201).
If anything, add a +1/data point to muda#239 noting the winit-host case (a
DBusMenu/ksni menu path would be the only way to get a menubar on a plain winit
window on Linux). Low priority.

### T6 — panic instead of `Err` when GTK is uninitialized: CONFIRMED, and the
panic is **not** muda's own
`linux-results/iced-tray-run.log` and `linux-results/egui-tray-run.log`,
verbatim:
```
(process:44): Gtk-CRITICAL **: gtk_icon_theme_get_for_screen: assertion 'GDK_IS_SCREEN (screen)' failed
(process:44): GLib-GObject-WARNING **: invalid (NULL) pointer instance
(process:44): GLib-GObject-CRITICAL **: g_signal_connect_data: assertion 'G_TYPE_CHECK_INSTANCE (instance)' failed
thread 'main' (44) panicked at .../gtk-0.18.2/src/auto/menu.rs:29:9:
GTK has not been initialized. Call `gtk::init` first.
```
The panic text comes from gtk-rs's `assert_initialized_main_thread!` macro
(`gtk-0.18.2/src/rt.rs:19-28`), fired inside `gtk::Menu::new()`
(`gtk-0.18.2/src/auto/menu.rs:29`), which muda calls from
`platform_impl/gtk/mod.rs:377` (`self.gtk_menu.1 = Some(gtk::Menu::new())`),
`:982`, `:1028` and `gtk::MenuBar::new()` at `:268`. The Gtk-CRITICALs that
precede it come from libappindicator inside `tray_icon::TrayIcon::new`
(`tray-icon-0.24.1/src/platform_impl/gtk/mod.rs:24-26`, `AppIndicator::new`).

So the accurate statement is: **muda (and tray-icon) call gtk-rs constructors
without checking `gtk::is_initialized()`, so gtk-rs's assertion aborts the
process.** `gtk::is_initialized()` is public (`gtk-0.18.2/src/rt.rs:46-54`), so a
guard returning `crate::Error` is a few lines. Neither muda's nor tray-icon's
docs state `gtk::init` is a precondition — muda's docs say only "Linux (gtk
Only)" plus which system packages to install (`lib.rs:9-36`); tray-icon's say
"on Linux, a gtk event loop [must be running]"
(`tray-icon-0.24.1/src/lib.rs:17`), which implies but never states `gtk::init`.

**UPSTREAM STATUS:** no issue found (`repo:tauri-apps/muda "gtk::init" OR "GTK
has not been initialized"` and `repo:tauri-apps/tray-icon gtk init panic` → no
relevant hits). Still present on main.
**FILE? YES — one issue, tauri-apps/muda, cross-referencing tray-icon.** Draft:

> **Title:** Linux: `Menu::new()` / `Submenu::new()` panic ("GTK has not been initialized") instead of returning `Err` when `gtk::init` wasn't called
>
> **Repo:** tauri-apps/muda (same request applies to `tray-icon`)
>
> **Versions:** muda 0.19.3 (via tray-icon 0.24.1), gtk 0.18.2, Debian container, X11.
>
> A macOS/Windows app that adds a tray + menu through `tray-icon`/`muda` compiles
> unchanged on Linux (our two winit-based apps needed zero source changes) and
> then dies on the first menu object:
> ```
> thread 'main' panicked at gtk-0.18.2/src/auto/menu.rs:29:9:
> GTK has not been initialized. Call `gtk::init` first.
> ```
> The panic is gtk-rs's `assert_initialized_main_thread!` (`gtk-0.18.2/src/rt.rs:25`)
> reached from `gtk::Menu::new()` in `src/platform_impl/gtk/mod.rs:377`
> (also `:268`, `:982`, `:1028`).
>
> Because it's a panic and not an `Err`, an app's graceful-degradation path
> ("no tray on this platform, carry on") never runs, and build-only CI on Linux
> passes while every launch dies. `Menu::new()` already returns nothing, but
> `TrayIconBuilder::build()` and `Menu::init_for_gtk_window` return `Result`, so
> the error type exists.
>
> Ask: check `gtk::is_initialized()` (it's public, `gtk-0.18.2/src/rt.rs:46`) at
> the entry points and return an error (e.g. `Error::NotInitialized`) instead of
> letting gtk-rs abort the process; and document `gtk::init` as a precondition in
> the crate docs next to "Linux (gtk Only)".
>
> Evidence (logs, verbatim):
> https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/linux-results/iced-tray-run.log ,
> .../linux-results/egui-tray-run.log , .../report/18-linux-reality-results.md

### T7 — Linux tray "silent successes": CONFIRMED
`tray-icon-0.24.1/src/platform_impl/gtk/mod.rs:23-51`: `TrayIcon::new` calls
`AppIndicator::new(...)`, `set_status(Active)`, writes a PNG to a temp dir and
returns `Ok(Self{..})`. There is **no** check that a StatusNotifierHost /
`org.kde.StatusNotifierWatcher` exists on the bus, and none that GTK is running.
Also silently no-op on Linux: `set_tooltip` returns `Ok(())` and does nothing
(`:81-83`), and `rect()` returns `None` (`:104-106`). This matches both retained
probe results (`report/data/linux-rows.md:75-76`, verbatim
"RESULT: TrayIcon::build returned Ok" in both the no-GTK and the no-host case,
with `NameHasOwner(org.kde.StatusNotifierWatcher) → false` captured).

For contrast, `ksni` (used by slint-tray, 0.3.5) models the watcher explicitly —
its `Handle`/`TrayMethods` API is async and surfaces registration failure as an
error rather than a value-less `Ok` (not re-verified line-by-line here; treat as
a pointer, not a citation).

**UPSTREAM STATUS:** closest existing issue is tray-icon **#336** (open) "Tray
icon never registers with StatusNotifierWatcher on KDE Plasma 6 / Wayland
(libayatana-appindicator)" — a symptom of the same blindness; maintainer reply
there is "maybe we should ignore it since we'll be switching out appindicator for
ksni soon anyway". Also #260 (open) "libayatana-appindicator is deprecated",
#104 (open) "Tray click events on linux not firing".
**FILE? MAYBE (low priority)** — a small, well-scoped request that survives the
ksni migration:

> **Title:** Linux: `TrayIconBuilder::build()` returns `Ok` when the icon cannot possibly appear (no GTK loop, or no StatusNotifier host on the bus)
>
> **Repo:** tauri-apps/tray-icon
>
> `platform_impl/gtk/mod.rs:23-51` constructs an `AppIndicator` and returns
> `Ok` unconditionally. We probed two failure cases in a headless Debian
> container (tray-icon 0.24.1):
> * no `gtk::init` at all → `build()` returns `Ok` while emitting Gtk-CRITICALs;
>   the icon can never function.
> * `gtk::init` called, but `NameHasOwner(org.kde.StatusNotifierWatcher)` is
>   `false` → `build()` still returns `Ok`.
>
> There is no way for an app to tell "tray shown" from "tray silently
> unavailable", so it can't fall back (to a window, a notification, whatever).
> Ask: either return `Err` when no StatusNotifier host is registered, or expose a
> `TrayIcon::is_registered()` / `tray_icon::is_supported()` probe. (`set_tooltip`
> and `rect` are also silent no-ops on Linux — worth a doc note.) If the ksni
> migration (#201/#339) is where this should land, feel free to fold it in there.
>
> Evidence: https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/report/data/linux-rows.md

---

## M-5 (T9) — global-hotkey

Pinned: 0.7.0 in nine apps, 0.8.0 in tauri-tray. Upstream main is 0.8.0
(`b5329b9`, 2026-08-27).

**(a) X11-only Linux backend — CONFIRMED, DOCUMENTED, ALREADY REPORTED.**
`global-hotkey-0.7.0/src/platform_impl/` contains only `macos/`, `windows/`,
`x11/`, `no-op.rs`; main is unchanged. The crate docs say it in the first lines:
"Platforms-supported: Windows, macOS, Linux (X11 Only)"
(`src/lib.rs:9-13`, repeated at `:45-49`). No `ashpd`/portal/GlobalShortcuts
reference anywhere in the source or Cargo.toml.
Open upstream issues: **#28** "Global Shortcut support on Wayland" and **#162**
"Wayland support". **FILE? NO — already reported (twice).**
Corpus is accurate here; only note that this is a documented limitation, not a
hidden defect.

**(b) Pressed *and* Released — NOT-A-BUG (our bug).** `HotKeyState { Pressed,
Released }` is a public, documented enum (`src/lib.rs:63-72`) carried on every
`GlobalHotKeyEvent` (`:74-83`). Our freya app's "unfiltered toggle nets to zero"
(`apps/freya-tray/FRICTION.md:15`) is our own missing filter. **FILE? NO.**
Corpus already calls it a gotcha rather than a bug; keep, but label it "by design
/ our bug".

**(c) Cross-process contention — UNVERIFIED, and our two observations conflict.**
macOS backend registers with Carbon `RegisterEventHotKey(..., GetApplicationEventTarget(), 0, ..)`
(`platform_impl/macos/mod.rs:110-135`) and only ever synthesises
`AlreadyRegistered` for *media* keys via its own per-process `HashSet`
(`:140-146`); a normal duplicate registration by another process is simply passed
to Carbon and, if Carbon accepts it, returns `Ok`. So contention behaviour is
entirely OS-defined and the crate neither documents nor normalises it.
Our two data points disagree: `apps/egui-tray/FRICTION.md:71-73` ("the same
Cmd+Shift+9 hotkey can be registered by multiple processes at once on macOS —
all of them fire", plus the same observation in `apps/dioxus-tray/FRICTION.md:23`)
versus `apps/slint-tray/FRICTION.md:30` ("Only one process on the machine can own
the combo"). Two frameworks, one crate, one API — the slint observation is
uncorroborated and was almost certainly a mis-read (e.g. its own registration
failing, or the sibling process having exited). I could not find a source-level
basis for "single owner" on macOS.
**FILE? NO.** **CORPUS CORRECTION:** `report/19` T9 says "contention behavior
across processes was observed inconsistently (egui: all registrants fire; slint:
single owner)" and asks the facade to "define and test the contention contract".
Recommend downgrading this to: *on macOS the Carbon path let several processes
register the same chord and all received events (egui, dioxus); one contradicting
note (slint) is unexplained and was not re-tested.* Don't carry "single owner" as
a finding.

**(d) Windows `AlreadyRegistered` on Win+Shift+9 — NOT-A-BUG (our code).**
`report/21-windows-reality-results.md:102-112`: egui-tray and xilem-tray died
with `AlreadyRegistered(HotKey { mods: Modifiers(SHIFT | SUPER), key: Digit9 })`
because Win+Shift+digit is an OS taskbar shortcut. The crate did exactly the right
thing — returned a typed error (`windows/mod.rs:104`); our apps `.expect()`ed it
(`apps/egui-tray/src/main.rs:211-213`). Five other tray apps logged and continued.
**FILE? NO.** (A docs PR mentioning OS-reserved chords would be nice-to-have and
very low value; not worth a volunteer's time.)

**(e) Delivery model / "polling tax" (T15) — the corpus claim is OVERSTATED.**
All three crates expose the same two-mode API: a `Lazy` crossbeam channel plus a
`set_event_handler` callback —
`global_hotkey::GlobalHotKeyEvent::{receiver, set_event_handler}` (`lib.rs:87-125`),
`muda::MenuEvent::{receiver, set_event_handler}` (`muda-0.19.3/src/lib.rs:487-527`),
`tray_icon::TrayIconEvent::{receiver, set_event_handler}` (`tray-icon-0.24.1/src/lib.rs:653-690`).
muda's docs contain an explicit **"Note for winit or tao users"** telling you to
use `set_event_handler` + `EventLoopProxy` "so that the event loop is awakened on
each menu event" (`muda-0.19.3/src/lib.rs:164-186`); tray-icon repeats it
(`lib.rs:94-118`). dioxus-desktop does exactly that
(`dioxus-desktop-0.7.9/src/app.rs:428-475`, with the comment "The event loop
becomes the hotkey receiver … we don't need to poll the receiver on every tick").
So there **is** a documented waker-friendly path; the crates deliver callbacks
from the platform's own thread (macOS: the Carbon/AppKit main-thread handler,
`global-hotkey macos/mod.rs:324-331`; Windows and X11: a crate-spawned thread,
`windows/mod.rs:157`, `x11/mod.rs:33`), so the app still needs a proxy/wake hop
— but not a timer.
The *real* obstacle is M-2: `set_event_handler` is a `OnceCell`, so in any process
where the framework (or another library) already claimed the slot, an app has no
choice but to poll `receiver()` — and if the framework claimed it, `receiver()`
is dead too. Our apps chose polling largely for portability across frameworks.
**CORPUS CORRECTION:** rewrite T15 from "Tray, menu, and hotkey channels have no
waker integration" to *"delivery is either a global crossbeam channel or a single
process-global callback; the callback (documented for winit/tao users) is
waker-friendly but can be claimed only once per process, so most apps fall back
to a 50–100 ms poll"*. Same fix in `apps/iced-tray/FRICTION.md:48` ("three
separate crossbeam channels … with no waker integration") — the channels do have
a callback alternative; the constraint is the single slot.
**FILE? NO separate issue** — this is folded into the M-2 muda design request.

---

## M-6 — other shell-crate items across the ten `*-tray/FRICTION.md`

Skimmed all ten. Attributable to muda/tray-icon/global-hotkey, beyond T1–T9:

1. **tray-icon: creation must be on the main thread *after* the loop starts**
   (T4). Documented — `tray-icon-0.24.1/src/lib.rs:18` names
   `StartCause::Init` for winit explicitly. Cost every framework something, but
   it's a documented platform constraint, not a bug. **No issue.**
2. **muda app-menu title comes from the process name on unbundled binaries**
   (dioxus, vizia FRICTION). That's `NSRunningApplication::localizedName` in
   `new_predefined` (`muda-0.19.3/macos/mod.rs:347-360`) — correct macOS
   behaviour for an unbundled binary. **Not a bug.**
3. **muda #365 (open): Services submenu crashes with `init_for_nsapp`** — we did
   not hit it (no app added `PredefinedMenuItem::services`), so we have nothing
   to add. Worth watching: it may share a root cause with the responder-chain
   story (M-1) or with the UAF (M-3, fixed on main).
4. **`PredefinedMenuItem::about(..)` "panics inside muda's PNG encoder"**
   (`apps/freya-tray/src/main.rs:185-188`) — almost certainly the M-3 UAF (muda
   #233 is exactly this), not an independent bug. We never retested `about()`
   with everything retained, so the corpus note is **UNVERIFIED as an independent
   finding** and should be softened / merged into T3.
5. **muda `Menu` is `!Send` / main-thread-only** and `TrayIcon` likewise —
   documented (`muda lib.rs:15-18`). Caused `Rc` gymnastics in frameworks with
   off-thread state; not a defect.
6. **slint's tray-icon handle is created once from a never-refiring change
   tracker** (`apps/slint-tray/FRICTION.md:28`) — that's an i-slint-core bug
   (`items/system_tray.rs`), not tray-icon. Out of my domain; flagging for
   whoever owns the slint findings.
7. **`tray_icon::TrayIcon::set_tooltip` and `rect()` are silent no-ops on Linux**
   (`platform_impl/gtk/mod.rs:81-83`, `:104-106`) — new observation from this
   pass, not in the corpus. Folded into the T7 issue draft as a doc ask.
8. **xilem's "two objc2 families and two `keyboard-types`"** — a consequence of
   tray-icon 0.21/muda 0.17 vs newer deps; muda main has since moved to
   `keyboard-types` 0.8 (`b3d70d1`, PR #387) and global-hotkey 0.8.0 likewise
   (PR #210). Will resolve itself as the stack moves forward. **No issue.**

---

## Filing recommendation (priority order)

| # | target | kind | status |
|---|---|---|---|
| 1 | tauri-apps/muda | **comment**, not a new issue: ask for a release containing a1550bd (#361) — the macOS use-after-free is fixed on main but unreleased and affects every published version | ready (M-3) |
| 2 | tauri-apps/muda | **new issue**: macOS predefined Edit roles no-op and swallow their key equivalents on non-NSText hosts | ready (M-1) |
| 3 | tauri-apps/muda | **new issue**: `set_event_handler` is a process-global `OnceCell`; ask for per-menu routing or at least non-silent replacement | ready (M-2) |
| 4 | tauri-apps/muda | **new issue**: Linux — return `Err` instead of letting gtk-rs panic when `gtk::init` wasn't called | ready (M-4 T6) |
| 5 | tauri-apps/tray-icon | **new issue** (low priority): Linux `build()` returns `Ok` when the icon can never appear; no host detection; `set_tooltip`/`rect` silent no-ops | ready (M-4 T7) |
| 6 | DioxusLabs/dioxus | **comment on #4495** only (root cause already posted there) | optional (M-2b) |
| — | rust-windowing/winit | not recommended as a standalone filing; mention inside #2 | — |
| — | tauri-apps/global-hotkey | nothing to file: Wayland already tracked (#28, #162); Pressed/Released and `AlreadyRegistered` are correct behaviour | — |

## Corpus corrections to make on the dashboard

1. **T2(b) / `apps/dioxus-tray/FRICTION.md:22` / `report/12` fn.3 / `apps/dioxus-tray/src/main.rs:138-142`:**
   routing direction is inverted. muda's slot is a `OnceCell` → **first**
   registration wins → dioxus's *menubar* handler wins → everything arrives as
   `MudaMenuEvent`; `use_tray_menu_event_handler` never fires. Already upstream as
   DioxusLabs/dioxus#4495.
2. **T3 / `apps/freya-tray/FRICTION.md:26`:** not "muda 0.17" — muda **0.19.3 has
   identical code**; and it isn't dropping the *item handles* (the `Menu` keeps
   `Rc` clones) but dropping the owning `Menu`/`Submenu`. Add: reported as muda
   #202/#233, fixed on main by a1550bd (#361), **unreleased**.
3. **T6 / `report/18` Headline 2:** "muda panics" → the panic is gtk-rs's
   `assert_initialized_main_thread!` (`gtk-0.18.2/src/rt.rs:25`) reached through
   `gtk::Menu::new()`; muda's defect is the *missing guard*, not a `panic!` of its
   own. The ask ("return `Err`") stands and is easy — `gtk::is_initialized()` is
   public.
4. **T9 contention:** drop "slint: single owner" as a finding; it contradicts two
   other observations of the same API and has no source basis.
5. **T15 / `apps/iced-tray/FRICTION.md:48`:** "no waker integration" is
   overstated. All three crates ship `set_event_handler`, and muda/tray-icon
   docs explicitly recommend it with an `EventLoopProxy` for winit/tao. The real
   constraint is that the callback slot is a process-global `OnceCell`.
6. **`apps/floem-tray/FRICTION.md:44`:** floem requires `muda = "0.17.1"`
   (caret), not `=0.17`; and the unification hazard would materialise if the app
   used tray-icon **0.21.x** (muda 0.17), not if tray-icon moved forward.
7. **`apps/freya-tray/src/main.rs:185-188`** ("`PredefinedMenuItem::about` panics
   with or without metadata"): unverified as an independent bug; almost certainly
   the same UAF (muda #233). Soften or merge into T3.
