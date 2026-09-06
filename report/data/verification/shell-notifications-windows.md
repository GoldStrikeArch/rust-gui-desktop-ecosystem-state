# Verification — notifications, hide/reopen, framework-side shell traps ("Tray Notes" round)

Agent domain: N-1 … N-6. Repo root = `/Users/mpl4/Desktop/workspace/self_learning/rust/gui-ecosystem-research`.
Vendored sources = `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/` (abbrev. `$R` below).
Clones for upstream diffing: `<local>/verify/{mns,nr,freya-upstream,xilem-upstream}`.
Date of upstream checks: 2026-08-30.

## Pinned versions actually used (from each `apps/*-tray/Cargo.lock`)

| app | framework | winit | notify-rust | mac-notification-sys | arboard | tray-icon | muda | objc2 |
|---|---|---|---|---|---|---|---|---|
| dioxus-tray | dioxus 0.7.9 (tao 0.34.8) | — | 4.18.0 | 0.6.15 | 3.6.1 | 0.21.3 | 0.17.2 | 0.6.4 |
| egui-tray | egui/eframe 0.35.0 | 0.30.13 | *(removed)* | *(removed)* | 3.6.1 | — | 0.19.3 | 0.5.2 + 0.6.4 |
| floem-tray | floem 0.2.0 (git) | 0.30.x | 4.18.0 | 0.6.15 | 3.6.1 | 0.24.2 | 0.17.2 + 0.19.3 | 0.5.2 + 0.6.4 |
| freya-tray | freya 0.4.0 / freya-winit 0.4.1 | 0.30.13 | 4.18.0 | 0.6.15 | 3.6.1 | 0.21.3 | 0.17.2 | 0.5.2 + 0.6.4 |
| gpui-tray | gpui 0.2.2 | — | 4.18.0 | 0.6.15 | — | 0.24.1 | 0.19.3 | 0.6.4 |
| iced-tray | iced 0.14.0 | 0.30.13 | 4.18.0 | 0.6.15 | 3.6.1 | 0.24.1 | 0.19.3 | 0.5.2 + 0.6.4 |
| slint-tray | slint / i-slint-core 1.17.1 | 0.30.13 | 4.18.0 | 0.6.15 | 3.6.1 | — | 0.19.3 | 0.5.2 + 0.6.4 |
| tauri-tray | tauri 2.11.5 | — | 4.18.0 | 0.6.15 | 3.6.1 | 0.24.1 | 0.19.3 | 0.6.4 |
| vizia-tray | vizia 0.4.0 | 0.30.13 | 4.18.0 | 0.6.15 | 3.6.1 | 0.24.2 | 0.19.3 | 0.5.2 + 0.6.4 |
| xilem-tray | xilem 0.4.0 / masonry_winit 0.4.0 | 0.30.13 | 4.18.0 | 0.6.15 | 3.6.1 | 0.21.3 | 0.17.2 | 0.5.2 + 0.6.4 |

Note: `apps/egui-tray/Cargo.lock` contains **no** notify-rust / mac-notification-sys — the crate was removed
after the experiment, so the exact version egui hit is not recoverable from the lock (FRICTION says 4.18).

---

## The one mechanism behind almost everything (read this first)

`notify-rust` 4.18.0 macOS default backend (`nsusernotifications`) and `mac-notification-sys` 0.6.15:

1. **`Notification::show()` does not send anything.**
   `$R/notify-rust-4.18.0/src/macos/nsusernotifications.rs:252-257`
   ```rust
   pub(crate) fn show_notification(notification: &Notification) -> Result<NotificationHandle> {
       Ok(NotificationHandle::new(notification.clone()))
   }
   ```
   The actual send happens in `impl Drop for NotificationHandle` (same file, `:142-162`), which calls
   `send_mac_notification(..).ok()` — **the error is discarded**. So on macOS `.show()?` can never fail and
   `Ok` carries zero information about delivery.

2. **The send pumps the main run loop synchronously.**
   `$R/mac-notification-sys-0.6.15/objc/notify.m:167-179` (fire-and-forget path, `shouldWait == NO`):
   ```objc
   if ([NSThread isMainThread]) {
       NSDate* deadline = [NSDate dateWithTimeIntervalSinceNow:kDeliveryTimeoutSecs];   // 2.0 s
       while (!rust_notification_is_delivered(notificationId) && [deadline timeIntervalSinceNow] > 0) {
           [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
       }
   }
   ```
   (the `shouldWait == YES` path pumps too, `:192-209`). Every `.show()` from the main thread re-enters the
   Cocoa run loop for up to 2 s.

3. **Bundle identity is resolved through AppleScript, not LaunchServices.**
   `$R/mac-notification-sys-0.6.15/src/lib.rs:132-138`
   ```rust
   fn ensure_application_set() -> NotificationResult<()> {
       if INIT_APPLICATION_SET.is_completed() { return Ok(()); };
       let bundle = get_bundle_identifier_or_default("use_default");
       set_application(&bundle)
   }
   ```
   → `$R/mac-notification-sys-0.6.15/objc/notify.m:8-13`
   ```objc
   NSString* getBundleIdentifier(NSString* appName) {
       NSString* findString = [NSString stringWithFormat:@"get id of application \"%@\"", appName];
       NSAppleScript* findScript = [[NSAppleScript alloc] initWithSource:findString];
       NSAppleEventDescriptor* resultDescriptor = [findScript executeAndReturnError:nil];
       return [resultDescriptor stringValue];
   }
   ```
   `LSCopyApplicationURLsForBundleIdentifier` appears only *later*, in `setApplication` (`notify.m:22`), to
   validate an already-chosen id. The identity itself is faked by swizzling
   `-[NSBundle bundleIdentifier]` (`$R/mac-notification-sys-0.6.15/objc/notify.h:26-44`,
   `method_exchangeImplementations`), defaulting to `@"com.apple.Terminal"`.

4. **A modern backend already exists, opt-in, in the pinned version.**
   `$R/notify-rust-4.18.0/Cargo.toml:67` → `preview-macos-un = ["dep:mac-usernotifications"]`,
   `src/macos/mod.rs:20-38`. `mac-usernotifications` 0.3.1 (crates.io, first published 2026-05-31) wraps
   `UNUserNotificationCenter` and exposes `check_bundle`, `request_auth`, `get_notification_settings`, and a
   handle whose `show()` really reports failure. **Our apps could have enabled it and did not.**

---

## N-1 (T10) — "use_default" bundle-id chooser modal

**1. CLAIM.** `report/19-shell-facade-spec.md:194-200` (T10): "From an unbundled binary, mac-notification-sys
resolves the default bundle id `use_default` via LaunchServices; macOS 26 answers with a blocking
'Where is use_default?' chooser while `.show()` returns `Ok` (freya, vizia, xilem all hit it)."
Sources: `apps/vizia-tray/FRICTION.md:42`, `apps/freya-tray/FRICTION.md:21`, `apps/xilem-tray/FRICTION.md:41`.

**2. EVIDENCE CHECK.** Three independent app-level observations, consistent wording, and all three converged
on the same workaround (`notify_rust::set_application("com.apple.Terminal")` before `.show()`;
`apps/vizia-tray/src/main.rs:326`). Contradiction found: the corpus attributes the lookup to **LaunchServices**.
It is **NSAppleScript**. The visible modal is AppleScript's "Where is …?" application-chooser panel, which is
exactly what `get id of application "use_default"` produces when no app of that name is resolvable.

**3. ROOT CAUSE — CONFIRMED.** `mac-notification-sys-0.6.15/src/lib.rs:136` passes the sentinel string
`"use_default"` into `getBundleIdentifier`, which builds and runs the AppleScript
`get id of application "use_default"` (`objc/notify.m:9-11`). `NSAppleScript::executeAndReturnError:` blocks
the calling thread while the chooser is up. Because `.show()` never returns the send result (see §mechanism 1),
the caller sees `Ok` regardless.

**4. UPSTREAM STATUS — ALREADY REPORTED, still present on main.**
- `h4llow3En/mac-notification-sys` **#81** — "macOS 26: Stuck sending notification on
  `get_bundle_identifier_or_default`" (open, filed 2025-12-27 by davidkna via starship). Reproducer in the
  thread; maintainer response: "I'm pretty sure you're not supposed to use that API outside the main thread…
  that is not documented well enough." Downstream: starship/starship#7128, PR #7187 (workaround =
  call `set_application` first). Related third-party: julienXX/terminal-notifier#301.
- Also related: **#30** "set_application doesn't seem to take effect (and can't be set twice)" (open, 2021) —
  `set_application` is `Once`-guarded (`src/lib.rs:149`) so the second call returns `ApplicationError::AlreadySet`.
- main (`ed6015d`, 2026-06-16) is identical to the published 0.6.15: `use_default` and the AppleScript lookup
  are unchanged.

**5. FILE? — NO new issue; COMMENT on mac-notification-sys#81 instead.** #81 covers the symptom but neither the
thread nor the maintainer's reply identifies the AppleScript mechanism, and the thread frames it as a
worker-thread problem. Our three cases are on the **main thread**, from inside a GUI event loop, and the caller
never asked for a bundle lookup at all. Draft comment:

> Another data point, and I think the mechanism is narrower than "don't call it off the main thread".
>
> `ensure_application_set()` (src/lib.rs:136) calls `get_bundle_identifier_or_default("use_default")`, and
> `getBundleIdentifier` (objc/notify.m:9) runs the AppleScript `get id of application "use_default"`. On
> macOS 26 there is no application named `use_default`, so AppleScript raises its "Where is use_default?"
> chooser panel and `-[NSAppleScript executeAndReturnError:]` blocks until a human dismisses it — on the main
> thread this freezes the whole UI. We hit this from three different Rust GUI frameworks (freya 0.4, vizia 0.4,
> xilem 0.4, all on macOS 26.5/26.6, mac-notification-sys 0.6.15 via notify-rust 4.18), each time from a
> normal main-thread `Notification::show()` with no prior `set_application`.
>
> Two small things that would fix it without changing the API:
> 1. Don't route the default through AppleScript at all — if the caller never called `set_application`, take
>    `[[NSBundle mainBundle] bundleIdentifier]` and, if that's nil, fall straight to the existing
>    `com.apple.Finder`/`com.apple.Terminal` fallback. The sentinel string only exists to be looked up and fail.
> 2. If the AppleScript path is kept, pass `NSAppleScript` an execution that cannot prompt (or check
>    `LSCopyApplicationURLsForBundleIdentifier` / `NSWorkspace URLForApplicationWithBundleIdentifier:` first) —
>    the chooser is a UI prompt in a library that has no way to know it may prompt.
>
> Evidence from our corpus: <https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/vizia-tray/FRICTION.md>,
> <https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/xilem-tray/FRICTION.md>.

**6. CORPUS CORRECTIONS.**
- T10 (`report/19-shell-facade-spec.md:196`) and `report/12-shell-integration-results.md:53` say
  **LaunchServices**; it is **NSAppleScript** (`get id of application "…"`). "LaunchServices chooser" in
  `report/12…:53` should become "AppleScript application chooser".
- Add: already reported upstream as mac-notification-sys#81.

---

## N-2 (T11) — run-loop re-entrancy aborts the process

**1. CLAIM.** `report/19-shell-facade-spec.md:201-213` (T11): "mac-notification-sys pumps the main runloop;
called from inside a winit event callback, winit 0.30 panic-aborts ('tried to handle event while another
event is currently being handled' — xilem, vizia, independently); called from a `slint::Timer` callback it
aborts with 'Recursion in timer code'. Safe shape: detached thread — which then silently drops the banner for
unbundled binaries (xilem's third failure)."
Sources: `apps/xilem-tray/FRICTION.md:41`, `apps/vizia-tray/FRICTION.md:42`, `apps/slint-tray/FRICTION.md:36`.

**2. EVIDENCE CHECK.** Supported. Two independent frameworks produced the same winit panic string, a third
(slint) produced the slint-specific assert. vizia's FRICTION additionally names the right call site:
"`NotificationHandle::drop` calls NSUserNotification, which spins the Cocoa run loop" — which matches the
source exactly (see mechanism 1+2 above). All three shipped the same mitigation (background thread /
`osascript`); `apps/vizia-tray/src/main.rs:323-330` and `apps/xilem-tray/src/main.rs:201-227`.

**3. ROOT CAUSE — CONFIRMED (all four legs).**
- (a) **mac-notification-sys pumps the run loop**: `objc/notify.m:167-179` (fire-and-forget, main thread,
  `[[NSRunLoop currentRunLoop] runUntilDate:…]` in a loop for up to `kDeliveryTimeoutSecs = 2.0`), and
  `:192-209` for the wait-for-response path. Reached by every `.show()` because
  `notify-rust`'s `Drop` path sets `asynchronous(true)` → `needs_response() == false`
  (`mac-notification-sys-0.6.15/src/notification.rs:306-311`) → `should_wait == false` → the `:167` branch.
- (b) **winit 0.30's guard**: `$R/winit-0.30.13/src/platform_impl/macos/event_handler.rs:111-136`; the handler
  is behind a `RefCell`, and a re-entrant `handle_event` takes the `Err(_)` arm:
  `panic!("tried to handle event while another event is currently being handled")`. The panic unwinds through
  an ObjC frame → `abort`.
- (c) **slint's guard**: `$R/i-slint-core-1.17.1/timers.rs:262`
  `assert!(timers.borrow().callback_active.is_none(), "Recursion in timer code");` inside
  `maybe_activate_timers` — the nested run loop re-enters slint's timer pump.
- (d) **Detached thread → silent drop**: on a non-main thread `notify.m:175-177` takes the
  `rust_wait_for_delivery` branch (a 2 s timed Condvar, `src/bridge.rs:7`), so nothing pumps and nothing
  aborts. Whether the banner appears is then purely macOS's decision: the process's *real* identity is an
  unsigned/unbundled binary and the "identity" mac-notification-sys supplies is a swizzled
  `-[NSBundle bundleIdentifier]` returning a **borrowed** id (`objc/notify.h:26-34`). On macOS 26 the
  (deprecated since 10.14, removed-in-practice) `NSUserNotificationCenter` path drops such requests. This last
  step is **LIKELY, not CONFIRMED** — I could not locate the OS-side rejection from source; but the observable
  chain (delivery timeout after 2 s → `.ok()` discards it → `show()` already returned `Ok`) is confirmed and
  fully explains "returns Ok, no banner".

**4. UPSTREAM STATUS.**
- mac-notification-sys main (`ed6015d`) == 0.6.15: `runUntilDate:` pumping still present
  (`objc/notify.m:66,173,198,206`). Issue search (`run loop`, `reentran`, `winit`, `hang`) returns **no** issue
  describing main-thread re-entrancy. Adjacent, closed: **#86** (`wait_for_click(true)` busy-spun an empty run
  loop, ~100 % CPU per notification) — same subsystem, different symptom, fixed; **#4** (2017, "issues with
  threading", `keepRunning` hang). So: **still present on main, not reported.**
- notify-rust main (`26f658a`, 2026-06-16) == 4.18.0: macOS `show()`/`Drop` unchanged. `gh search issues` for
  `use_default`, `run loop`, `winit`, `main thread macos`, `reentran` → nothing relevant. **Not reported.**
- winit: **#3992** "MacOS panic: 'tried to handle event while another event is currently being handled'"
  (open since 2024-11-10, 3 comments) is a *different* reporter with an unidentified cause. winit's behaviour
  here is defensible (the alternative is UB), so this is not a winit bug — but #3992 would benefit from a
  concrete known cause.

**5. FILE? — YES, one issue on `h4llow3En/mac-notification-sys`** (notify-rust is a thin passthrough; a
docs-only note there can follow if the maintainer prefers). Optionally a short comment on winit#3992.

> **Title:** `sendNotification` pumps the main run loop, which aborts winit/slint apps that call it from an event callback
>
> **Repo:** h4llow3En/mac-notification-sys
>
> **Body:**
>
> **Versions:** mac-notification-sys 0.6.15 (via notify-rust 4.18.0), macOS 26.5/26.6 on Apple Silicon,
> rustc 1.96.
>
> **What happens.** On the main thread, `sendNotification` spins a nested Cocoa run loop:
>
> ```objc
> // objc/notify.m:167-179  (shouldWait == NO — i.e. the ordinary fire-and-forget send)
> if ([NSThread isMainThread]) {
>     NSDate* deadline = [NSDate dateWithTimeIntervalSinceNow:kDeliveryTimeoutSecs];
>     while (!rust_notification_is_delivered(notificationId) && [deadline timeIntervalSinceNow] > 0) {
>         [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
>     }
> }
> ```
>
> Any GUI toolkit that calls this from inside its own event dispatch therefore gets its event handler
> re-entered. Two concrete outcomes we reproduced independently in different frameworks:
>
> * **winit 0.30.13** panics — and, because the panic unwinds through an Objective-C frame, the process
>   aborts: `tried to handle event while another event is currently being handled`
>   (winit `src/platform_impl/macos/event_handler.rs:135`; the handler is a `RefCell` that is already borrowed).
>   Hit from xilem 0.4 and vizia 0.4 independently.
> * **Slint 1.17.1** aborts with `Recursion in timer code`
>   (`i-slint-core/timers.rs:262`) when `.show()` is called from a `slint::Timer` callback.
>
> Note this is not only the interactive path: `notify-rust`'s `NotificationHandle::drop` sends with
> `asynchronous(true)`, so `needs_response()` is false and the `:167` branch above is the one a plain
> `Notification::new().summary(..).show()` takes.
>
> **Minimal repro** (winit 0.30, no framework):
>
> ```rust
> // Cargo.toml: winit = "0.30", mac-notification-sys = "0.6"
> use winit::{application::ApplicationHandler, event::WindowEvent, event_loop::*, window::*};
> struct A(Option<Window>);
> impl ApplicationHandler for A {
>     fn resumed(&mut self, el: &ActiveEventLoop) {
>         self.0 = Some(el.create_window(Window::default_attributes()).unwrap());
>     }
>     fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, e: WindowEvent) {
>         if let WindowEvent::MouseInput { .. } = e {
>             mac_notification_sys::set_application("com.apple.Terminal").ok();
>             let _ = mac_notification_sys::send_notification("hi", None, "from a winit callback", None);
>         }
>         if let WindowEvent::CloseRequested = e { el.exit() }
>     }
> }
> let el = EventLoop::new().unwrap();
> el.run_app(&mut A(None)).unwrap();
> ```
>
> Click in the window → abort.
>
> **Expected.** Sending a fire-and-forget notification should not require the caller to own the run loop, and
> should not re-enter it.
>
> **Suggestions** (any one would be enough for us):
> 1. In the `shouldWait == NO` path, don't wait for `didDeliverNotification:` on the main thread at all —
>    `-[NSUserNotificationCenter deliverNotification:]` has already been called by then; the wait exists only so
>    a CLI can exit promptly, which a GUI never needs. A `Notification::asynchronous(true)`-style opt-out that
>    actually skips the pump would work too.
> 2. If the wait must stay, use `CFRunLoopRunInMode(kCFRunLoopDefaultMode, .., true)` guarded by a
>    "already inside a run loop" check, or hop the whole send to a dedicated thread and signal back.
> 3. At minimum, document on `send_notification` / `Notification::send` that they must not be called from
>    inside a UI event callback on the main thread, and why.
>
> **Context / evidence.** Ten-framework macOS survey; the three affected apps are
> <https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/xilem-tray/FRICTION.md>,
> <https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/vizia-tray/FRICTION.md>,
> <https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/slint-tray/FRICTION.md>.

**5b. FILE? — YES, a second, smaller issue on `hoodie/notify-rust`** (independent of the above and, I think,
the highest-value one in this whole batch):

> **Title:** macOS: `Notification::show()` returns `Ok` before sending, and the real send error is discarded in `Drop`
>
> **Repo:** hoodie/notify-rust · **Version:** 4.18.0, default features, macOS 26
>
> On macOS (`NSUserNotificationCenter` backend) `show()` does not send:
>
> ```rust
> // src/macos/nsusernotifications.rs:252
> pub(crate) fn show_notification(notification: &Notification) -> Result<NotificationHandle> {
>     Ok(NotificationHandle::new(notification.clone()))
> }
> ```
>
> The send happens in `Drop` (`:142-161`), which ends in `send_mac_notification(..).ok();` — the error is
> dropped on the floor. Consequences:
>
> * `notification.show()?` on macOS is infallible in practice. It cannot report a bad bundle identity, a
>   delivery timeout, or a refusal.
> * The failure is invisible even when nothing is displayed: in a 10-framework survey, several apps logged
>   `notification: OK` from `.show()` while no banner ever appeared.
> * The side effects the caller cares about (AppleScript bundle-id lookup, run-loop pumping) happen at an
>   unexpected point — the end of the enclosing statement/scope, not inside `show()`.
>
> This differs from the XDG and Windows backends, where `show()` really sends, and from the
> `preview-macos-un` backend, whose handle documents "returns this handle as soon as macOS accepts the
> notification request".
>
> Suggestion: on macOS, send inside `show()` and return the error (keeping `Drop` a no-op when the handle has
> already sent), or — if the deferred-send shape must stay for source compatibility — say so explicitly in the
> `Notification::show` doc for `target_os = "macos"`, since today it reads "Sends Notification to
> `NSUserNotificationCenter`".

**6. CORPUS CORRECTIONS.**
- T11's "Safe shape: detached thread — which then *silently drops the banner* for unbundled binaries" is right
  about the observation but the causal claim should be softened: the thread is not why the banner is dropped;
  the borrowed bundle identity is. Suggested rewording: "…detached thread — which avoids the abort but does
  nothing about the identity problem, so unbundled binaries still get no banner, and `show()`'s `Ok` cannot
  tell you (see T12)."
- T12 now has a source-level root cause (see the notify-rust draft above) — it should cite
  `notify-rust/src/macos/nsusernotifications.rs:252` and `:142-161` rather than being an empirical observation.
- §3 item 6 of the facade spec ("macOS backend should target the modern UserNotifications framework, not
  deprecated NSUserNotification via a 'small subset' backend") should note that **notify-rust 4.18 — the exact
  version we pinned — already ships that backend behind `features = ["preview-macos-un"]`
  (`mac-usernotifications` 0.3.1)**, with `check_bundle` / `request_auth` / real errors. This is an
  ergonomics/defaults finding, not a missing-capability finding.

---

## N-3 — egui/eframe froze after a notify-rust call: was it delegate replacement?

**1. CLAIM.** `apps/egui-tray/FRICTION.md:21` and `:32-35`: "In two of three full-app runs, the first
notify-rust notification appeared and eframe then stopped scheduling frames … `mac-notification-sys` replacing
the NSApplication delegate is a plausible explanation, but no minimized reproduction was retained, so the root
cause is **not proven**." Repeated as `report/19…:210-212` ("delegate replacement suspected") and
`report/12-shell-integration-results.md:50-51` ("delegate replacement is the leading inferred cause").

**2. EVIDENCE CHECK.** The corpus is honest that this is unproven, and the crate has since been removed from
`apps/egui-tray/Cargo.lock`, so nothing is re-runnable. 2-of-3 reproduction rate, no log retained.

**3. ROOT CAUSE — the stated hypothesis is REFUTED; a better one is LIKELY.**
mac-notification-sys 0.6.15 never touches `NSApp.delegate`. There is exactly one `setDelegate`-equivalent in
the whole crate, and it targets the *notification centre*:
```objc
// objc/notify.m:303-311
+ (instancetype)sharedDelegate {
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
      instance = [[NotificationCenterDelegate alloc] init];
      [NSUserNotificationCenter defaultUserNotificationCenter].delegate = instance;   // <- not NSApp
    });
    return instance;
}
```
`grep -n NSApp objc/notify.m` → no hits. So winit's `ApplicationDelegate` is untouched and frame scheduling
cannot be lost that way.

Two mechanisms that *are* in the crate and fit the symptom better:
- **The nested run loop** (`notify.m:167-179`, up to 2 s of `runUntilDate:` on the main thread) runs inside
  eframe's own callback. eframe/winit 0.30 drive redraw scheduling through `ControlFlow::WaitUntil` plus a
  self-posted wake-up; draining those re-entrantly can consume a wake-up that eframe then never re-arms. This
  matches the symptom precisely — *tray/hotkey callbacks stayed alive* (they are AppKit/Carbon callbacks,
  independent of eframe's scheduler) *while frames stopped*. It also matches the mitigation the app ended up
  shipping for an adjacent problem: `apps/egui-tray/FRICTION.md:46-51` describes a 2 Hz
  `request_repaint_after` watchdog added because "a `request_repaint` from a native callback was once observed
  to be dropped by eframe's 'outdated RequestRepaint' lost-wakeup guard, permanently stalling the queue".
- **Process-wide swizzle of `-[NSBundle bundleIdentifier]`** (`objc/notify.h:36-44`) — a global mutation with
  unknown blast radius on other AppKit consumers. Speculative.

**Rating: hypothesis-as-stated REFUTED (code located, `notify.m:308` sets only the
`NSUserNotificationCenter` delegate). Replacement explanation LIKELY (run-loop re-entrancy) but UNVERIFIED —
no reproduction, and egui was never observed to hit the winit re-entrancy *panic*, so it is not the same
failure as N-2.**

**4. UPSTREAM STATUS.** N/A — no separate defect established.

**5. FILE? — NO.** There is nothing filable without a reproduction, and the delegate claim is wrong.
The re-entrancy issue drafted in N-2 covers the plausible cause.

**6. CORPUS CORRECTIONS (important — this text is wrong in three places).**
- `apps/egui-tray/FRICTION.md:21` and `:32-35`: drop "mac-notification-sys replacing the NSApplication
  delegate is a plausible explanation". It does not replace it; it only sets
  `[NSUserNotificationCenter defaultUserNotificationCenter].delegate`.
- `report/19-shell-facade-spec.md:211` "delegate replacement suspected" → "cause unproven; the crate pumps
  the main run loop for up to 2 s inside the caller's callback, which is the more plausible cause — it does
  **not** replace the NSApplication delegate".
- `report/12-shell-integration-results.md:50-51` "delegate replacement is the leading inferred cause" → same
  correction.

---

## N-4 (T13) — Hide / reopen

### N-4(a) eframe never runs `App::ui` for a hidden viewport

**1. CLAIM.** `report/19…:220-223` / `apps/egui-tray/FRICTION.md:24`: "eframe never runs `App::ui` for a
hidden viewport, so reopen logic in `ui` is dead … the fix — `App::logic` — is undocumented for this purpose."

**2. EVIDENCE CHECK.** The behaviour claim is right. The "undocumented" claim is wrong.

**3. ROOT CAUSE — CONFIRMED, and it is intentional.**
- `$R/eframe-0.35.0/src/native/wgpu_integration.rs:648-660`:
  ```rust
  let is_visible = viewport.info.visible().unwrap_or(true);
  let run_ui = is_visible || is_viewport_or_descendant_visible(viewports, viewport_id);
  ```
  (identical in `glow_integration.rs:571-580`), passed to
- `$R/eframe-0.35.0/src/native/epi_integration.rs:279-297`:
  ```rust
  let full_output = self.egui_ctx.run_ui(raw_input, |ui| {
      …
      { profiling::scope!("App::logic"); app.logic(ui.ctx(), &mut self.frame); }
      if is_visible { profiling::scope!("App::ui"); app.ui(ui, &mut self.frame); }
  });
  ```
  So `logic` runs on every pass, `ui` only when visible. The method the corpus was looking for is indeed
  `App::logic` (not `raw_input_hook`, which exists at `epi.rs:273` but is a different thing).
- **It is documented.** `$R/eframe-0.35.0/src/epi.rs:152-161`:
  "Called once before each call to [`Self::ui`], **and additionally also called when the UI is hidden**, but
  [`egui::Context::request_repaint`] was called." egui main has since expanded this further
  ("While the window is hidden, `eframe` runs no egui pass at all … and calls this via
  `egui::Context::run_logic` instead").

**4. UPSTREAM STATUS.** Behaviour still on main and deliberate; the doc on main is strictly better than 0.35's.
egui issue search (`hidden window update`, `minimize to tray`, `App::logic`, `viewport not visible update`)
found nothing matching; the only `App::logic` hit is emilk/egui#8470 (showing viewports from `App::logic`),
unrelated.

**5. FILE? — NO.** At most a one-line docs PR adding "not called while the viewport is hidden — use
[`Self::logic`]" to `App::ui`'s doc comment. Low value; the maintainer already improved the neighbouring doc.
**NOT-A-BUG.**

**6. CORPUS CORRECTION.** `report/19…:222-223` "the fix — `App::logic` — is undocumented for this purpose" and
`apps/egui-tray/FRICTION.md:64-67` "nothing in the tray-icon/eframe docs points at it" are **overstated**:
`App::logic`'s own rustdoc in eframe 0.35 says it is called when the UI is hidden. Fair residual complaint:
`App::ui`'s doc does not warn you, so you only find it if you read `logic`.

### N-4(b) gpui: no per-window hide; `cx.hide()` ignored during menu dismissal

**1. CLAIM.** `report/19…:224-225` / `apps/gpui-tray/FRICTION.md:32`.

**2/3. ROOT CAUSE — half CONFIRMED, half NOT-A-BUG.**
- "No per-window hide" is **CONFIRMED**: gpui 0.2.2 `PlatformWindow` has no `set_visible`/`order_out`
  (`$R/gpui-0.2.2/src/platform.rs`), and `platform/mac/window.rs` only ever calls `miniaturize_`
  (`:1340`, `:1529`). The app-level escape hatch is `App::hide()` (`src/app.rs:984`) →
  `platform/mac/platform.rs:571-576` → `msg_send![app, hide: nil]`, i.e. `-[NSApplication hide:]`, which hides
  every window of the process (including gpui-tray's About window — as observed).
- "Silently ignored while the NSMenu is still dismissing" is **Cocoa semantics, not a gpui bug** (UNVERIFIED in
  source, but expected): `-[NSApplication hide:]` is a no-op while the app is in a modal/tracking run-loop mode
  (`NSEventTrackingRunLoopMode`), which is what an open `NSStatusItem` menu puts it in. gpui does not (and
  arguably should not) special-case this. The app's 300 ms settle delay is the standard workaround; the
  canonical one is `dispatch_async(main)` / `performSelector:withObject:afterDelay:` so the call lands after
  the tracking loop unwinds.

**4. UPSTREAM STATUS.** gpui 0.2.2 is the pinned version; not re-checked against zed main for this item.

**5. FILE? — MAYBE, low priority, as a gpui *feature request*, not a bug: "expose per-window
show/hide (`orderOut:` / `orderFront:`) on `Window`". A menu-tracking-mode note would be a docs nicety.
Not drafted here — it is a feature ask with no bug behind it, and the corpus already lists gpui's missing
tray/activation-policy surface separately.

### N-4(c) freya: "close hook can't touch the window it's about (`pub(crate)`)"

**1. CLAIM.** `report/19…:225-226`, `apps/freya-tray/FRICTION.md:24` and its Surprises bullet:
"`AppWindow`'s `window` is `pub(crate)`, so the one hook that is *about* a window (`with_on_close`) cannot act
on it."

**2/3. ROOT CAUSE — NOT-A-BUG / OUR OWN MISTAKE. The field is `pub(crate)`, but public accessors exist.**
In the pinned `freya-winit 0.4.1`:
- `src/window.rs:77` — `pub(crate) window: Window` ✔ (the field is indeed private), **but**
- `src/window.rs:379-385`:
  ```rust
  pub fn window(&self) -> &Window { &self.window }
  pub fn window_mut(&mut self) -> &mut Window { &mut self.window }
  ```
- `src/renderer.rs:99-108` — `RendererContext.windows: &'a mut FxHashMap<WindowId, AppWindow>` is a **public
  field**, plus `pub fn windows_mut()` at `:144`.
- `src/renderer.rs:661-677` — the runner deliberately `take()`s `on_close` out of the `AppWindow` *before*
  constructing the `RendererContext`, precisely so the hook can borrow the window map mutably.

So the hook can do exactly what the FRICTION says it cannot:
```rust
.with_on_close(|mut ctx, window_id| {
    if let Some(app) = ctx.windows_mut().get_mut(&window_id) { app.window_mut().set_visible(false); }
    CloseDecision::KeepOpen
})
```
(freya main, 0.5.0-rc.4, is the same: `crates/freya-winit/src/window.rs:462-468`.)

**4. UPSTREAM STATUS.** Nothing to fix.

**5. FILE? — NO. Do not file.** The app's flag-plus-poll workaround
(`Platform::with_window(None, |w| w.set_visible(false))`) was unnecessary.

**6. CORPUS CORRECTIONS (must fix — this would have been a wrong issue filed at a volunteer).**
- `apps/freya-tray/FRICTION.md:24` and its Surprises "Bad:" bullet.
- `report/19-shell-facade-spec.md:225-226` "freya's close hook can't touch the window it's about
  (`pub(crate)`)" → delete, or replace with the (mild, real) point that the route is undiscoverable:
  `windows_mut()` returns a map of an opaque `AppWindow` whose only relevant accessor is `window_mut()`, and
  none of it is documented.

### N-4(d) "8 of 10 frameworks can't drop the Dock icon (`ActivationPolicy::Accessory` unreachable)"

**1. CLAIM.** `report/19…:106` ("menu-bar-only apps are impossible in 8 of 10 frameworks today") and
`:226-227`. Also `apps/vizia-tray/FRICTION.md` Surprises ("no reachable `ActivationPolicy::Accessory`").

**2/3. ROOT CAUSE — the number is WRONG. 6 of 10 expose a first-class route; 4 do not.**
API facts (all read from the pinned sources):
- winit 0.30.13 exposes `EventLoopBuilderExtMacOS::with_activation_policy`
  (`$R/winit-0.30.13/src/platform/macos.rs:383-441`). It does **not** have
  `ActiveEventLoopExtMacOS::set_activation_policy` — that trait only has `hide_application`,
  `hide_other_applications`, `set_allows_automatic_window_tabbing`, `allows_automatic_window_tabbing`
  (`:478-509`). (The brief's assumption about a runtime setter is wrong for 0.30.)
- tao 0.34/0.35 exposes `EventLoopExtMacOS::set_activation_policy` **and**
  `EventLoopWindowTargetExtMacOS::set_activation_policy_at_runtime`
  (`$R/tao-0.35.3/src/platform/macos.rs:310-428`; same in 0.34.8 at `:318`, `:397`).

| framework | first-class route to `Accessory`? | evidence |
|---|---|---|
| tauri 2.11.5 | **yes** — `App/AppHandle::set_activation_policy` | `$R/tauri-2.11.5/src/app.rs:640`, `:1273-1285` |
| egui/eframe 0.35 | **yes** — `NativeOptions::event_loop_builder` hook | `$R/eframe-0.35.0/src/epi.rs:34`, `:345` |
| slint 1.17 | **yes** — `slint::BackendSelector::with_winit_event_loop_builder` (also `i_slint_backend_winit::Backend::builder().with_event_loop_builder`) | `$R/i-slint-backend-winit-1.17.1/lib.rs:309`; re-exported per `$R/slint-1.17.1/lib.rs:687-690` |
| xilem 0.4 | **yes** — `Xilem::run_in(EventLoopBuilder)` | `$R/xilem-0.4.0/src/app.rs:169`; `EventLoopBuilder` re-exported at `src/lib.rs:159` |
| freya 0.4 | **yes** — `LaunchConfig::with_event_loop(EventLoop)` | `$R/freya-winit-0.4.1/src/config.rs:381-388`; consumed at `src/lib.rs:74` |
| dioxus 0.7 | **yes** — `Config::with_event_loop(tao EventLoop)`, or `set_activation_policy_at_runtime` from any `use_wry_event_handler` | `$R/dioxus-desktop-0.7.9/src/config.rs:177-181` |
| iced 0.14 | **no** — event loop built internally, no hook | `$R/iced_winit-0.14.0/src/lib.rs:79` |
| vizia 0.4 | **no** | `$R/vizia_winit-0.4.0/src/application.rs:135` |
| floem 0.2 | **no** | floem git checkout `src/app/mod.rs:271` (`EventLoop::new()`) |
| gpui 0.2.2 | **no** — hard-codes Regular | `$R/gpui-0.2.2/src/platform/mac/platform.rs:1390` `app.setActivationPolicy_(NSApplicationActivationPolicyRegular)` |

Additionally, for **all ten**, an app can call `-[NSApplication setActivationPolicy:]` itself via
`objc2-app-kit` after startup (~4 lines); gpui hard-codes Regular during `run`, so the app just has to set
Accessory afterwards. So "impossible" is wrong even for the four.

**Rating: the corpus claim is CONFIRMED-WRONG.** What we actually measured is "none of the ten apps did it",
which is a different statement.

**4/5. FILE? — NO** for the six that expose a hook. **MAYBE** as small feature requests for iced / vizia /
floem ("expose an `EventLoopBuilder` hook", which is a general request, not notification/tray-specific) and
gpui ("expose `NSApplicationActivationPolicy`" — gpui-tray FRICTION already lists this). Not drafted; out of
this domain's core scope and not a defect.

**6. CORPUS CORRECTION (dashboard-visible).** Replace "8 of 10 frameworks can't drop the Dock icon
(`ActivationPolicy::Accessory` unreachable)" (`report/19…:226-227`) and "menu-bar-only apps are impossible in
8 of 10 frameworks today" (`:106`) with: **"none of the ten apps drops the Dock icon; 6 of 10 frameworks
expose a first-class route (tauri, eframe, slint, xilem, freya, dioxus), 4 do not (iced, vizia, floem, gpui),
and even those can call `-[NSApplication setActivationPolicy:]` directly."** The `apps/vizia-tray/FRICTION.md`
Surprises bullet ("like the rest of the cohort … no reachable `ActivationPolicy::Accessory`") is correct
*for vizia* but wrong about the cohort.

---

## N-5 (T14 / Secondary observations)

### N-5(a) arboard TIFF-flavor incompatibility

**1. CLAIM.** `report/19…:232-236` (T14): "one hit a TIFF-flavor incompatibility (dioxus/AppleScript
screenshots)". Source `apps/dioxus-tray/FRICTION.md:25`: "arboard fails with 'could not be converted' on
AppleScript's legacy `TIFF picture` flavor (fine with PNG flavor and real screenshots)".

**2. EVIDENCE CHECK.** The FRICTION line is precise; the facade-spec paraphrase is not — it reads as though
real screenshots are affected, which the FRICTION explicitly denies.

**3. ROOT CAUSE — CONFIRMED, and reproduced live during this verification.**
`$R/arboard-3.6.1/src/platform/osx.rs:216-241` reads **only** `NSPasteboardTypeTIFF` and forces the decoder:
```rust
let image_data = unsafe { self.clipboard.pasteboard.dataForType(NSPasteboardTypeTIFF) }
    .ok_or(Error::ContentNotAvailable)?;
let data = Cursor::new(unsafe { image_data.as_bytes_unchecked() });
let reader = image::io::Reader::with_format(data, image::ImageFormat::Tiff);
reader.decode().map_err(|_| Error::ConversionFailure)
```
and on macOS the `image` dependency is built with only the `tiff` feature (`$R/arboard-3.6.1/Cargo.toml:100-102`),
so there is no fallback. The AppleScript idiom `set the clipboard to (read (POSIX file "…") as TIFF picture)`
puts the **raw file bytes** under `public.tiff` regardless of what they are; macOS's own ImageIO sniffs and
copes, `image`'s strict TIFF decoder does not.

Live repro on this machine (macOS 26.6, arboard 3.6.1, scratch project `<local>/arbtest`):
```
$ osascript -e 'set the clipboard to (read (POSIX file "…/t.png") as TIFF picture)'
$ osascript -e 'clipboard info'
TIFF picture, 75, «class AVIF», 375, … , «class PNGf», 173, …      # 75 = the PNG's byte count
$ ./arbtest
ERR The image or the text that was about the be transferred to/from the clipboard could not be converted to the appropriate format.
$ screencapture -x -c && ./arbtest
OK 3024x1964
```
i.e. exactly the dioxus observation, and real screenshots are unaffected.

**4. UPSTREAM STATUS — still present on main.** `1Password/arboard` main `src/platform/osx.rs:224-241` is
identical (TIFF-only, `ImageFormat::Tiff`). Latest release v3.6.1 (2025-08-23). Issue search
(`tiff`, `get_image macos`, `ConversionFailure`, `macos image`, `PNG`) found nothing matching: #104
("Unable to read image placed by Spectacle"), #184 ("Get image, always empty"), #211 (set_image DPI),
#158 (closed) are all different. **Not reported.**

**5. FILE? — MAYBE (small robustness issue, not a defect in the strict sense: the input is mislabeled by
AppleScript).** If filed:

> **Title:** macOS `get_image()` only reads `public.tiff` and force-decodes it as TIFF
> **Repo:** 1Password/arboard · **Version:** 3.6.1, macOS 26.6, Apple Silicon
>
> `osx.rs:231-237` fetches `NSPasteboardTypeTIFF` and decodes with
> `Reader::with_format(data, ImageFormat::Tiff)`. Two consequences:
> 1. If the `public.tiff` flavor does not actually contain TIFF, the read fails with `ConversionFailure` even
>    though the pasteboard carries a perfectly good `public.png` alongside. AppleScript's
>    `set the clipboard to (read (POSIX file "x.png") as TIFF picture)` produces exactly this: raw PNG bytes
>    filed under `public.tiff`. macOS's own ImageIO reads it fine; `image`'s strict TIFF decoder does not.
> 2. `image` is pulled in with only the `tiff` feature on macOS (Cargo.toml), so there is no fallback path.
>
> Repro (macOS):
> ```
> osascript -e 'set the clipboard to (read (POSIX file "/tmp/x.png") as TIFF picture)'
> # then: Clipboard::new()?.get_image()  ->  Err(ConversionFailure)
> ```
> `screencapture -c` works, so this only bites the mislabeled-flavor case.
>
> Suggested fix, cheapest first: try `NSPasteboardTypePNG` when TIFF decode fails; or use
> `Reader::new(..).with_guessed_format()`; or decode through `NSImage`/`CGImageSource`, which handles every
> flavor macOS advertises.

**6. CORPUS CORRECTION.** `report/19…:235` "(dioxus/AppleScript screenshots)" is misleading — real screenshots
work. Suggest "(dioxus; AppleScript's `as TIFF picture` flavor only)".

### N-5(b) freya's release-only panic hook

**1. CLAIM.** `report/19…:247-249`: "freya's release-only panic hook converts panics into a modal with empty
stderr (diagnosability)." Source: `apps/freya-tray/FRICTION.md:51-62`.

**2/3. ROOT CAUSE — CONFIRMED (code located); the "empty stderr" wording is slightly off.**
`$R/freya-winit-0.4.1/src/lib.rs:61-72` (`set_hook` at :64; identical in 0.4.0):
```rust
#[cfg(all(not(debug_assertions), not(target_os = "android")))]
{
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        rfd::MessageDialog::new()
            .set_title("Fatal Error")
            .set_description(&panic_info.to_string())
            .set_level(rfd::MessageLevel::Error)
            .show();                 // blocks until dismissed
        previous_hook(panic_info);   // <- stderr happens only AFTER dismissal
        std::process::exit(1);       // <- no unwind, no cleanup
    }));
}
```
So stderr is not permanently empty — it is *deferred behind a blocking modal*. The `apps/freya-tray/FRICTION.md`
wording ("a frozen window and a modal alert with nothing on stderr") is accurate for what an operator sees;
the facade-spec's compressed "empty stderr" is what needs the footnote.

**4. UPSTREAM STATUS — still present on main.** freya main (`b57fe8a`, 2026-08-27, `freya-winit 0.5.0-rc.4`):
`crates/freya-winit/src/lib.rs:65` still has the `rfd::MessageDialog` hook. No matching freya issue found
(`gh search issues --repo marc2332/freya "panic hook"` → none).

**5. FILE? — YES, small, `marc2332/freya`.**

> **Title:** Release builds replace the panic hook with a blocking dialog, so the panic message reaches stderr only after someone dismisses it
> **Repo:** marc2332/freya · **Versions:** freya 0.4.0 / freya-winit 0.4.1 (still on `main`, freya-winit 0.5.0-rc.4), macOS 26
>
> `freya_winit::launch` installs, in release builds only, a panic hook that shows an `rfd::MessageDialog`,
> *then* chains to the previous hook, *then* `std::process::exit(1)`
> (`crates/freya-winit/src/lib.rs:61-73`). Two costs we hit while debugging a real panic:
> * The default hook's message (and any backtrace) is printed only **after** the modal is dismissed, so a
>   release binary run from CI, from a script, or with the dialog behind another window looks like a silent
>   freeze rather than a crash.
> * `process::exit(1)` from inside the hook means no unwinding, so nothing else gets a chance to log.
>
> Suggested change (either would be enough): call `previous_hook(panic_info)` **before** showing the dialog,
> and/or make the dialog opt-in (`LaunchConfig::with_panic_dialog(bool)`, or skip it when stderr is not a TTY
> / when `RUST_BACKTRACE` is set). A library taking over the process-wide panic hook is surprising in itself;
> an opt-out would be welcome.
>
> Context: <https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/freya-tray/FRICTION.md>
> (§"Second trap: release-mode panics become a modal dialog, not a backtrace").

### N-5(c) slint's single-shot tray-icon handle

**1. CLAIM.** `report/19…:250-252`: "slint's tray icon handle is created once from a never-refiring change
tracker (silent stderr-only failure)". Source `apps/slint-tray/FRICTION.md:29`.

**2/3. ROOT CAUSE — CONFIRMED. This is a slint API gap, not just our app design.**
`$R/i-slint-core-1.17.1/items/system_tray.rs:256-299`:
```rust
self.data.change_tracker.init_delayed(
    self_rc.downgrade(),
    |_| true,                          // <- depends on no property, so it fires exactly once
    |self_weak, has_icon| {
        …
        let handle = match SystemTrayIconHandle::new(…) {
            Ok(handle) => handle,
            Err(err) => {
                crate::debug_log!("Slint: Failed to create system tray icon: {err}");
                return;                // <- no retry, no error surfaced to the app
            }
        };
        let _ = tray.data.inner.set(handle);
        …
```
and every subsequent tracker is gated on that handle existing —
`visible_tracker` (`:312-315`), `icon_tracker` (`:335-338`), `tooltip_tracker` (`:355-358`),
`title_tracker` (`:373-376`) all do `if let Some(handle) = …data.inner.get()`. So if creation fails once
(e.g. the `icon` property is still an empty `Image` at that moment, as it is when the icon is assigned from
Rust after `new()`), the tray is dead for the process lifetime, later `set_icon(...)` cannot revive it, and the
only signal is a `debug_log!` line on stderr.

**4. UPSTREAM STATUS.** `SystemTrayIcon` is new in slint 1.17; issue search on slint-ui/slint
(`SystemTrayIcon`, `system tray icon`) turned up only feature requests (#12330 colorizable icon hint, #12360
named/themed icons) and closed items (#11637 appearance, #10792 `window.show()` makes the icon disappear —
worth a glance but a different symptom). **Not reported.**

**5. FILE? — YES, `slint-ui/slint`.**

> **Title:** `SystemTrayIcon`: platform handle creation is single-shot, so a failed create can never recover and is only reported via `debug_log`
> **Repo:** slint-ui/slint · **Version:** 1.17.1, macOS 26, winit backend
>
> `SystemTrayIcon::init` (`internal/core/items/system_tray.rs`) spawns the platform handle from a
> `ChangeTracker` whose eval closure is `|_| true`. Because it reads no property, it fires exactly once, and
> if `SystemTrayIconHandle::new` returns `Err` at that moment the code logs
> `Slint: Failed to create system tray icon: …` via `debug_log!` and returns. Every other tracker
> (`visible_tracker`, `icon_tracker`, `tooltip_tracker`, `title_tracker`) is guarded on
> `data.inner.get()`, so nothing can retry later.
>
> The practical failure: bind `icon` to a property that Rust sets after `ComponentHandle::new()`. At tracker
> time the icon is still the default empty `Image`, creation fails
> (`Failed to create a rgba8 buffer from an icon image`), and the tray never appears — the app has no way to
> observe it (no `Err`, no callback), and assigning a valid icon afterwards does nothing. Using
> `@image-url(...)` so the icon is non-empty at creation is the workaround.
>
> Suggestions: make the eval closure depend on `icon` (so a later assignment re-fires creation), and/or retry
> creation from `icon_tracker` when `data.inner` is still empty; and surface the failure to the application
> rather than only to `debug_log!`.
>
> Context: <https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/slint-tray/FRICTION.md>

### N-5(d) xilem's duplicated objc2 / keyboard-types

**1. CLAIM.** `report/19…:249-250`: "xilem's dependency graph carries two objc2 families and two
`keyboard-types` (type-identity conflicts across the shell crates)".

**2/3. VERIFIED — but MISATTRIBUTED to xilem.** From `apps/xilem-tray/Cargo.lock`:
- `objc2` 0.5.2 and 0.6.4 (2 entries). Reverse edges: **0.5.2 ← winit 0.30.13**, accesskit_macos 0.22.2,
  copypasta 0.10.2, and the whole `objc2-*-0.2.2` generation; **0.6.4 ← arboard, rfd, tray-icon, muda,
  global-hotkey, mac-notification-sys, fontique, dispatch2** and the `objc2-*-0.3.x` generation.
  This split is **winit 0.30 vs the tauri-apps shell crates**, and it appears identically in *every*
  winit-0.30 lock in the round: egui, floem, freya, iced, slint, vizia, xilem all carry 0.5.2 + 0.6.4.
  (gpui/tauri/dioxus, which don't use winit 0.30, carry only 0.6.4.) Nothing xilem-specific.
- `keyboard-types` 0.7.0 and 0.8.3. Reverse edges: **0.7.0 ← global-hotkey 0.7.0 and muda 0.17.2**;
  **0.8.3 ← ui-events 0.2.0** (masonry/xilem). This one *is* a xilem-vs-shell-crates split, and it is caused by
  adding the shell crates.

**4. UPSTREAM STATUS.** linebender/xilem main (`b81d8d7`, 2026-08-28) `Cargo.lock`: still 2 objc2 (0.5.2 +
0.6.4 — same winit-0.30 vs modern-objc2 split), and **one** `keyboard-types` (0.8.3). The single
`keyboard-types` on main is not a fix, though: 0.7.0 only enters when you add muda/global-hotkey, which the
upstream repo does not. So "has xilem main since unified?" is **not answerable that way** — the duplicate is a
property of the combination, not of xilem.

**5. FILE? — NO.** Informational. The real observation is "winit 0.30 is still on the objc2 0.5 generation
while the tauri-apps shell crates moved to 0.6" — a transition state, not a defect, and one that resolves when
winit 0.31 lands.

**6. CORPUS CORRECTION.** `report/19…:249-250` should read: "**every winit-0.30 app in the round** carries two
objc2 generations (winit 0.30 → objc2 0.5.2; tray-icon/muda/global-hotkey/rfd/arboard → objc2 0.6.4), and
adding muda/global-hotkey to xilem also duplicates `keyboard-types` (0.7.0 vs masonry's 0.8.3)". Not a
xilem-specific finding.

---

## N-6 — other upstream-attributable findings in this domain, not yet in the trap list

Skimmed all ten `apps/*-tray/FRICTION.md` (there are no `GAPS.md` files in the `*-tray` apps). New in-domain
items, one-line assessment each:

1. **`set_application` is `Once`-guarded and can never be changed** — `mac-notification-sys/src/lib.rs:149`;
   second call returns `ApplicationError::AlreadySet`, so a library that sets it wins for the process.
   *Already upstream as mac-notification-sys#30 (open since 2021); worth a +1, not a new issue.*
2. **notify-rust's `preview-macos-un` (UNUserNotificationCenter) backend exists in the pinned 4.18.0 and none
   of the ten apps used it** — the fix path for T10/T11/T12 was already on the shelf. *Not a bug; a discoverability/
   defaults finding, and a correction the dashboard should carry.*
3. **slint#10792 "Calling `window.show()` causes the icon to disappear"** (closed) sits adjacent to our
   single-shot-handle finding — worth referencing in the slint issue draft, not a separate report.
4. **vizia: `Context::load_image` takes `&'static [u8]` and is unreachable from an `EventContext`**
   (`apps/vizia-tray/FRICTION.md:40`) — the clipboard-image path has to go through
   `ContextProxy::load_image(String, &[u8], policy)`. *In-domain (clipboard image), genuinely awkward API;
   a small vizia issue is defensible but it is an ergonomics ask, not a defect. Not drafted.*
5. **vizia: no window-visibility getter** (`apps/vizia-tray/FRICTION.md:45`) — the app has to shadow the flag
   it just set. *Trivial vizia feature request; low value.*
6. **tauri: window creation must happen on the main thread; from workers use `run_on_main_thread`**
   (`apps/tauri-tray/FRICTION.md:30`). *Documented Tauri behaviour, not a bug.*
7. **wry `dragDropEnabled` makes native drops and HTML5 DnD mutually exclusive per window** (already a
   secondary observation at `report/19…:252-253`). *Out of my domain (file drop); flagged only so it is not
   lost — it is a real, documented wry constraint, not filable.*
8. **gpui action handlers must `cx.defer` or silently no-op, and a scripted menu click aborted on a RefCell
   borrow** (`apps/gpui-tray/FRICTION.md:48-62`, `report/19…:265-268`). *Adjacent to hide/reopen (menu-driven
   window commands) and the closest thing here to a real gpui bug, but it belongs to whoever owns the
   menus/gpui domain; not drafted here.*

---

## Summary of dashboard-visible corrections requested

| # | Location | Change |
|---|---|---|
| C1 | `report/19…:196`, `report/12…:53` | "LaunchServices" → "NSAppleScript (`get id of application "use_default"`)"; the modal is AppleScript's app chooser |
| C2 | `report/19…:210-212`, `report/12…:50-51`, `apps/egui-tray/FRICTION.md:21,32-35` | Drop "delegate replacement": mac-notification-sys sets only `NSUserNotificationCenter.delegate`, never `NSApp.delegate` |
| C3 | `report/19…:222-223`, `apps/egui-tray/FRICTION.md:64-67` | `App::logic` **is** documented as running while hidden (eframe 0.35 `epi.rs:152-161`); the gap is that `App::ui`'s doc doesn't cross-reference it |
| C4 | `report/19…:225-226`, `apps/freya-tray/FRICTION.md:24` + Surprises | **Retract**: freya's close hook *can* act on the window (`AppWindow::window_mut()` + `RendererContext::windows_mut()`, both public in 0.4.1) |
| C5 | `report/19…:106, 226-227` | "8 of 10 can't drop the Dock icon" → 6 of 10 expose a route (tauri, eframe, slint, xilem, freya, dioxus); 4 don't (iced, vizia, floem, gpui); none of our apps used it |
| C6 | `report/19…:235` | TIFF failure is AppleScript's `as TIFF picture` flavor only — real screenshots work |
| C7 | `report/19…:249-250` | Two objc2 generations affect *all seven* winit-0.30 apps (winit 0.5.2 vs shell crates 0.6.4), not just xilem |
| C8 | `report/19…:207-209` (T11 last sentence) | Detached thread doesn't cause the dropped banner — the borrowed bundle identity does; and `show()`'s `Ok` can't report it |
| C9 | `report/19…` §3 item 6, T12 | notify-rust 4.18 already ships a `UNUserNotificationCenter` backend behind `preview-macos-un`; T12's root cause is `show_notification` returning `Ok` before sending + `Drop`'s `.ok()` |

## Filing decisions

| ID | Target | Action | Confidence |
|---|---|---|---|
| N-1 | h4llow3En/mac-notification-sys **#81** | comment (mechanism + fix suggestions) — do **not** open a duplicate | CONFIRMED |
| N-2a | h4llow3En/mac-notification-sys | **new issue** — main-thread run-loop pumping (winit/slint aborts), repro included | CONFIRMED |
| N-2b | hoodie/notify-rust | **new issue** — macOS `show()` returns `Ok` before sending; `Drop` discards the error | CONFIRMED |
| N-2c | rust-windowing/winit **#3992** | optional comment naming a concrete cause | CONFIRMED |
| N-3 | — | do not file; correct the corpus | REFUTED (as stated) |
| N-4a | emilk/egui | do not file (optional 1-line doc PR on `App::ui`) | NOT-A-BUG |
| N-4b | zed-industries/zed (gpui) | maybe — per-window show/hide feature request; not drafted | CONFIRMED (gap) / NOT-A-BUG (Cocoa) |
| N-4c | marc2332/freya | **do not file** — our mistake | NOT-A-BUG |
| N-4d | — | do not file; correct the "8 of 10" claim | CONFIRMED-WRONG |
| N-5a | 1Password/arboard | maybe — TIFF-only read; draft ready, live repro captured | CONFIRMED |
| N-5b | marc2332/freya | **new issue** — release-only panic hook ordering/opt-out | CONFIRMED |
| N-5c | slint-ui/slint | **new issue** — single-shot `SystemTrayIcon` handle creation | CONFIRMED |
| N-5d | — | informational only | VERIFIED, misattributed |
