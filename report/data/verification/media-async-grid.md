# Verification report — media / async / grid (round-4 findings)

Verifier scope: A-1 nokhwa (Peek), A-2 dioxus occlusion freeze, A-3 iced executor,
A-4 slint/gpui/ehttp/tauri/xilem async traps, A-5 grid widgets, A-6 sweep of 30 FRICTION files.

Host of record for the corpus: macOS 26 / M4 Pro (aarch64). All crate line numbers below
are from the vendored registry copies under
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.

Repo link prefix used in issue drafts:
`https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/`

---

## A-1. nokhwa 0.10.11 / nokhwa-bindings-macos 0.2.4 / nokhwa-core 0.1.9

Pinned in every `apps/*-peek/Cargo.lock` (except gpui-peek, which uses AVFoundation directly):
nokhwa 0.10.11, nokhwa-bindings-macos 0.2.4, nokhwa-core 0.1.9, nokhwa-bindings-windows 0.4.6.

Upstream repo: `l1npengtul/nokhwa`. Default branch is `senpai` (0.11 rewrite, last commit
2026-04-13 `0ceddb5`). Latest crates.io release: nokhwa 0.10.11 (2026-05-15). There is **no
0.10.12**. The 0.10.x line is maintained from tags, not from `senpai`.

### A-1(a) fourcc mis-map: 420v/420f -> FrameFormat::YUYV, NV12 -> a 10-bit format

**CLAIM** — `report/15-media-hardware-results.md:67-70`; `apps/dioxus-peek/FRICTION.md:122`;
`apps/egui-peek/FRICTION.md:70-73`; `apps/slint-peek/FRICTION.md:95`.

**EVIDENCE CHECK** — supported. Three independent apps (egui, slint, dioxus) recorded the same
symptom: any `Closest(NV12 ...)`/`Exact(NV12 ...)` request returns "Cannot fulfill request",
and every enumerated format is labelled `YUYV`. egui-peek kept a dedicated probe
(`apps/egui-peek/examples/probe.rs`) dumping the raw AVFoundation format list.

**ROOT CAUSE — CONFIRMED.**

`nokhwa-bindings-macos-0.2.4/src/lib.rs:394-407`:

```rust
fn raw_fcc_to_frameformat(raw: OSType) -> Option<FrameFormat> {
    match raw {
        kCMVideoCodecType_422YpCbCr8 | kCMPixelFormat_422YpCbCr8_yuvs => Some(FrameFormat::YUYV),
        kCMVideoCodecType_JPEG | kCMVideoCodecType_JPEG_OpenDML => Some(FrameFormat::MJPEG),
        kCMPixelFormat_8IndexedGray_WhiteIsZero => Some(FrameFormat::GRAY),
        kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange      // 'x420', 10-bit
        | kCVPixelFormatType_420YpCbCr8BiPlanarFullRange      // '420f'
        | kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange     // '420v'
            => Some(FrameFormat::YUYV),                       // <-- 4:2:0 bi-planar reported as 4:2:2 packed
        kCMPixelFormat_24RGB => Some(FrameFormat::RAWRGB),
        _ => None,
    }
}
```

And the reverse map, `nokhwa-bindings-macos-0.2.4/src/lib.rs:2338-2343`:

```rust
FrameFormat::NV12 => kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,   // 10-bit, not 420v/420f
```

Consequences, both real: (1) no OSType maps to `FrameFormat::NV12`, so `NV12` can never appear
in `supported_formats()` and `Closest(NV12 ...)`/`Exact(NV12 ...)` are unsatisfiable on macOS;
(2) asking for NV12 sets the *ten-bit* biplanar format on `AVCaptureVideoDataOutput`.

Why the apps still saw correct colours: `AVFoundationCaptureDevice::open_stream`
(`nokhwa-0.10.11/src/backends/capture/avfoundation.rs:256`) calls
`output.set_frame_format(self.camera_format().format())`, which pins the output's
`kCVPixelBufferPixelFormatTypeKey` to `'yuvs'`, and AVFoundation converts. So the mis-map is a
*negotiation* bug, not a colour bug, on this path.

**UPSTREAM STATUS — already reported, open PR, not fixed in any release.**
- PR **#246 "Various Mac fixes onto 0.10"** (open, 2026-07-08, 0 reviews/comments) changes
  exactly this: `420f|420v => FrameFormat::NV12`, drops the 10-bit mapping, and changes
  `FrameFormat::NV12 => kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange`.
- `senpai` (main) already has `420f | 420v | 875704438 => Some(FrameFormat::NV12)`
  (`nokhwa-bindings-macos/src/lib.rs:372-374`), i.e. the fix will ship with 0.11.
- Historical closed report of the same class: **#78** "`Camera::frame_format` reports as `NV12`
  despite actually being `YUV422`" (2022).

**FILE? — No.** Duplicate of #246 / already fixed on main. Optional: a short confirming comment
on #246 ("reproduced on an M4 Pro FaceTime HD camera with 0.10.11 + bindings 0.2.4") to help it
get merged.

### A-1(b) `supported_formats` advertises range minimums that `set_all` can never set

**CLAIM** — `report/15-media-hardware-results.md:71-73`; `apps/egui-peek/FRICTION.md:74-79`;
`apps/slint-peek/FRICTION.md:93-97`.

**EVIDENCE CHECK** — supported, with a probe dump: this host's FaceTime camera advertises
fps_list `[15, 30]` on every resolution (`apps/egui-peek/FRICTION.md:81-83`), and
`RequestedFormatType::None` picked 640x480@15 and then failed
`"Not Found/Rejected/Unsupported"`.

**ROOT CAUSE — CONFIRMED.** Three-line chain:

1. `nokhwa-bindings-macos-0.2.4/src/lib.rs:812-822` — `AVCaptureDeviceFormat::try_from`
   flattens each `AVFrameRateRange` into **both** endpoints:
   ```rust
   .flat_map(|v| if v.min() != 0_f64 && v.min() != 1_f64 { vec![v.min(), v.max()] }
                 else { vec![v.max()] })
   ```
2. `nokhwa-bindings-macos-0.2.4/src/lib.rs:961-977` — `supported_formats()` emits one
   `CameraFormat` per fps entry, so `640x480 @15 YUYV` is advertised as supported.
3. `nokhwa-bindings-macos-0.2.4/src/lib.rs:1051-1058` — `set_all()` matches a range **only by
   its `maxFrameRate`**:
   ```rust
   let max_fps: f64 = unsafe { msg_send![range.inner, maxFrameRate] };
   if (f64::from(descriptor.frame_rate()) - max_fps).abs() < 0.999 { selected_range = ...; break; }
   ```
   -> a 15-30 range never matches a request for 15 -> `SetPropertyError { error:
   "Not Found/Rejected/Unsupported" }` (lib.rs:1063-1069).

`RequestedFormatType::None` amplifies this: `nokhwa-core-0.1.9/src/types.rs:198-201` returns the
**first** decoder-compatible entry in `supported_formats()`, which on this camera is a min-fps
entry. `Exact(fmt)` (types.rs:147-153) is also unsound: it returns the requested format without
checking membership in `all_formats`.

**UPSTREAM STATUS — symptom already reported, root cause not; still present on main.**
- Issue **#204 "Hangs and errors on MacOS"** (open since 2025-02-25, MacBook Pro M2, FaceTime HD)
  contains the exact failure: `Could not set device property CameraFormat with value
  1920x1080@15FPS, YUYV Format: Not Found/Rejected/Unsupported` from
  `RequestedFormatType::None`. No diagnosis in the thread.
- Related but **not** the same bug: #247 / PR #248 / PR #249 fix the `selected_format` vs
  `selected_range` *pairing* (SIGABRT). PR #246 also only fixes the pairing. **None of them
  changes the max-only matching or the min-fps enumeration.** `senpai` still matches only
  `maxFrameRate` (`nokhwa-bindings-macos/src/lib.rs:997-1000`), with the tolerance tightened
  from 0.999 to 0.01 (which would additionally break the 29.97 case the 0.999 comment mentions).
- Nothing found for `supported_formats`, `minFrameRate`, `unsettable`.

**FILE? — Yes (or comment on #204).** Preferred: post the root cause as a comment on #204 and,
if a maintainer wants it separate, open the issue below. Do not open a duplicate silently.

> **Title:** macOS: `supported_formats()` advertises frame-rate-range minimums that `set_all()`
> can never select (root cause for #204)
>
> **Repo:** l1npengtul/nokhwa
>
> **Body:**
> nokhwa 0.10.11, nokhwa-bindings-macos 0.2.4, macOS 26 / M4 Pro, built-in FaceTime HD camera
> (every format advertises a single 15-30 fps range).
>
> `Camera::new(idx, RequestedFormat::new::<RgbFormat>(RequestedFormatType::None))` fails with
> `Could not set device property CameraFormat with value 640x480@15FPS, YUYV Format:
> Not Found/Rejected/Unsupported`. Same failure is in #204 with `1920x1080@15FPS`.
>
> Root cause is a mismatch between what the bindings enumerate and what they can set:
> * `AVCaptureDeviceFormat::try_from` (`nokhwa-bindings-macos/src/lib.rs:812-822`) expands each
>   `AVFrameRateRange` into `[min, max]` when `min` is not 0 or 1, so a 15-30 range yields both
>   `@15` and `@30` entries.
> * `supported_formats()` (`src/lib.rs:961-977`) emits one `CameraFormat` per entry, so `@15` is
>   advertised as supported.
> * `set_all()` (`src/lib.rs:1051-1058`) compares the requested fps only against
>   `range.maxFrameRate`, so no `@15` request can ever match -> `Not Found/Rejected/Unsupported`.
>
> `RequestedFormatType::None` hits this by default because
> `nokhwa-core/src/types.rs:198-201` returns the *first* compatible entry, which is a min-fps
> entry on this device. (`RequestedFormatType::Exact` has a related hole: `types.rs:147-153`
> returns the requested `CameraFormat` without checking it is in `all_formats`.)
>
> Expected: a format returned by `supported_formats()` can be opened; or the minimums are not
> enumerated. Actual: every `@min-fps` entry the crate itself enumerates is unopenable.
>
> Two possible fixes: (a) match a range when
> `min - eps <= requested <= max + eps` and set `activeVideoMinFrameDuration` from the requested
> fps rather than from `minFrameDuration`; or (b) stop expanding ranges to their minimum in
> `try_from`. (a) is the more useful behaviour.
>
> Workaround for users: `RequestedFormatType::AbsoluteHighestFrameRate` (always a range max) or
> `Closest(YUYV WxH@max)`.
>
> Evidence: raw AVFoundation format dump and the fallback ladder we ended up with are in
> `apps/egui-peek/FRICTION.md` and `apps/egui-peek/examples/probe.rs` of
> https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state
>
> Note this is distinct from #247/#248/#249 (format/range *pairing*), which do not change the
> max-only matching.

### A-1(c) `nokhwa =0.10.9` no longer compiles

**CLAIM** — `report/15-media-hardware-results.md:74-75`; `apps/slint-peek/FRICTION.md:43,90-92`.

**ROOT CAUSE — CONFIRMED (semver violation in a patch release).**
- `nokhwa-bindings-macos` 0.2.3 (2025-07-06): `pub type CompressionData<'a> = (Cow<'a, [u8]>,
  FrameFormat);` (0.2.3 `src/lib.rs:381`).
- `nokhwa-bindings-macos` 0.2.4 (2026-05-15): `(Cow<'a, [u8]>, FrameFormat, Option<Duration>)`
  (0.2.4 `src/lib.rs:508`), and `nokhwa-core` gained `Buffer::with_timestamp`.
- `nokhwa` 0.10.9 (2025-07-06) and 0.10.10 (2025-11-10) both declare
  `frame_buffer_receiver: Arc<Receiver<(Vec<u8>, FrameFormat)>>`
  (`nokhwa-0.10.9/src/backends/capture/avfoundation.rs:57-58`, `0.10.10:58-59`) and depend on
  `nokhwa-bindings-macos = "0.2"` (caret). A fresh resolve therefore picks 0.2.4 and the build
  fails with `expected a tuple with 2 elements, found one with 3`.
- Same coupling in the other direction breaks 0.10.11 against a lockfile pinned to 0.2.3.

**UPSTREAM STATUS — already reported: issue #245** (open, 2026-06-10, no maintainer reply),
which shows the mirror-image failure (nokhwa 0.10.11 + bindings 0.2.3, plus
`Buffer::with_timestamp` missing from nokhwa-core 0.1.8). Root cause is identical: the two
crates are coupled by a private type but versioned with caret ranges.

**FILE? — No new issue.** Comment on #245 with the other direction (`nokhwa =0.10.9` / `=0.10.10`
now fail to build because 0.2.4 is picked) and the concrete remedy: either yank/republish
`nokhwa-bindings-macos` 0.2.4 as 0.3.0, or pin `nokhwa-bindings-macos = "=0.2.4"` (and
`nokhwa-core = "=0.1.9"`) in nokhwa 0.10.11. Only nokhwa 0.10.11 currently builds on macOS.

### A-1(d) `Camera::new` -> "Lock Rejected" while another app holds the camera

**CLAIM** — `report/15-media-hardware-results.md:77-81`; `apps/slint-peek/FRICTION.md:108`;
`apps/dioxus-peek/FRICTION.md:93-94,119-121`; `apps/tauri-peek/FRICTION.md:65`.

**ROOT CAUSE — CONFIRMED as to origin; the *behaviour* is NOT-A-BUG, but the surrounding code
has three real defects.**

Origin: `AVCaptureDevice::lock()` at `nokhwa-bindings-macos-0.2.4/src/lib.rs:994-1021`, reached
from `set_all()` (lib.rs:1031) which `AVFoundationCaptureDevice::new` calls at
`nokhwa-0.10.11/src/backends/capture/avfoundation.rs:77`.

The observed behaviour itself is Apple's: `-[AVCaptureDevice lockForConfiguration:]` is an
exclusive, machine-wide configuration lock; WebKit's brokered capture does not need it, which is
exactly why the webview frameworks kept streaming. Not a nokhwa bug. **But:**

1. **`err_ptr` is dead code** (lib.rs:1003-1011). `let err_ptr: *mut c_void = std::ptr::null_mut();`
   is passed *by value* as the `NSError**` out-parameter and then tested with
   `if !err_ptr.is_null()`. The local can never change, so the `NSError` AVFoundation would have
   produced is discarded and the caller gets no reason at all.
2. **Operator-precedence bug making the check arch-dependent** (lib.rs:1013):
   `if !accepted == YES`. On aarch64 `objc 0.2.7` defines `BOOL = bool` (`objc-0.2.7/src/runtime.rs:29-33`),
   so this happens to be correct; on x86_64 `BOOL = c_schar` and `!accepted` is a *bitwise* NOT
   (`!1i8 == -2`, `!0i8 == -1`), so the comparison is **always false** and `lock()` returns
   `Ok(())` even when `lockForConfiguration:` returned NO. Intel Macs therefore proceed to
   `setValue:forKey:` on an unlocked device instead of getting `Lock Rejected`.
3. **`self.locked` is never set to `true`** after a successful lock (lib.rs:994-1021 sets it only
   in `unlock()`, lib.rs:1024-1025), so `unlock()` never calls `unlockForConfiguration`.

Defect 3 is already reported (in the body of #247 and fixed in PR #248). Defects 1 and 2 are, as
far as I can find, unreported. `senpai` has the same code (`nokhwa-bindings-macos/src/lib.rs:947-960`).

**FILE? — Yes, one small combined issue** (low priority, easy fix).

> **Title:** macOS: `AVCaptureDevice::lock()` discards the `NSError` and its
> `if !accepted == YES` check is a no-op on x86_64
>
> **Repo:** l1npengtul/nokhwa
>
> **Body:**
> nokhwa-bindings-macos 0.2.4 (also present on `senpai`), `src/lib.rs:994-1021`.
>
> Two small bugs in `AVCaptureDevice::lock`:
>
> 1. `let err_ptr: *mut c_void = std::ptr::null_mut();` is passed by value to
>    `lockForConfiguration:` (which takes `NSError**`) and then checked with
>    `if !err_ptr.is_null()`. The local can never be written, so the check is dead and the
>    `NSError` explaining *why* the lock failed is never surfaced. Callers only ever see the
>    string `"Lock Rejected"`. Passing `&mut err` and reporting `localizedDescription` would make
>    device-contention failures diagnosable.
>
> 2. `if !accepted == YES` parses as `(!accepted) == YES`. On aarch64 `objc 0.2.7` sets
>    `BOOL = bool`, so this works by accident. On x86_64 `BOOL = c_schar`, `!` is bitwise NOT,
>    and both `!1i8 == -2` and `!0i8 == -1` compare unequal to `YES`, so the branch is
>    unreachable: a rejected `lockForConfiguration:` returns `Ok(())` and `set_all` then sends
>    `setValue:forKey:` to an unlocked device. Suggested: `if accepted != YES`.
>
> Context: on macOS 26 / M4 Pro, `Camera::new` correctly returns
> `lockForConfiguration ... Lock Rejected` whenever another AVFoundation app (Zoom, FaceTime)
> holds the device; on Intel it would silently proceed instead. (The related `self.locked` flag
> bug is already covered by #247 / #248.)

### A-1(e) bonus, already fixed upstream: `compatible_list_by_resolution` inverted filter

`apps/slint-peek/FRICTION.md:97-99` records "`compatible_camera_formats()` returned an empty
list pre-open". Root cause **CONFIRMED**:
`nokhwa-0.10.11/src/backends/capture/avfoundation.rs:154-158` filters
`.filter(|x| x.format() != fourcc)` — it keeps everything *except* the requested fourcc.
Already fixed in the open PR **#246**. No action beyond noting it.

### A-1(f) related, from the FRICTION sweep: `RequestedFormatType::Closest` is not "closest"

`apps/slint-peek/FRICTION.md:92` and `apps/egui-peek/FRICTION.md:70-73`.
**CONFIRMED** in `nokhwa-core-0.1.9/src/types.rs:155-197`: `Closest` picks the nearest resolution
into `resolution` (line 176), then computes `frame_rates` by filtering on
`cfmt.resolution() == c.resolution()` — the **requested** resolution, not the one just chosen —
and returns `CameraFormat::new(resolution, c.format(), frame_rate)`. If the exact requested
resolution is absent, `framerate_map.first()?` returns `None` and the whole request fails.
Unreported upstream as far as I can find (#231 is only about the doc comment being reversed).
Small, self-contained. **FILE? — maybe**, bundled into the A-1(b) issue or as its own two-line
report; low priority relative to (b).

---

## A-2. dioxus-desktop 0.7.9 — the occlusion freeze

**CLAIM** — `report/17-async-network-results.md:60-64`; `apps/dioxus-fetch/FRICTION.md:14-36`
("Three of six verification runs froze ... wry 0.53 has the knob ... dioxus-desktop 0.7.9's
`Config` does not plumb it through").

**EVIDENCE CHECK** — supported, with the honest caveat the corpus itself states: 3 of 6 runs
froze, `with_always_on_top` took it to 0 of 1 (n=1 on the mitigation), and two frozen-run logs
are retained (`run-stdout-frozen.log`, `run-stdout-frozen2.log`). The *mechanism* is labelled
source-only in the FRICTION file; the report's summary line reads more confident than the
evidence.

**ROOT CAUSE — CONFIRMED for the gate; LIKELY for the trigger.**

The gate (this is the important part, and it is a design coupling, not a missing knob):
`dioxus-desktop-0.7.9/src/webview.rs:562-566`, inside `poll_vdom`:

```rust
// If we're waiting for a render, wait for it to finish before we continue
let edits_flushed_poll = self.edits.wry_queue.poll_edits_flushed(&mut cx);
if edits_flushed_poll.is_pending() {
    return;
}
...
let fut = self.dom.wait_for_work();
```

`self.dom.wait_for_work()` is what services **every** dioxus task (`use_resource`, `spawn`,
timers). It is only reached after the edit batch has been acknowledged. The acknowledgement is
the `oneshot::Receiver<()>` stored in `edits_in_progress`
(`dioxus-desktop-0.7.9/src/edits.rs:55-62`, field at `edits.rs:118`, comment at `edits.rs:117`:
"We don't run the virtual dom while this is true"), which is resolved only when the **webview's
JavaScript** pulls the batch over the edits websocket (`edits.rs:396-433`, driven by
`window.interpreter.waitForRequest(...)` at `webview.rs:556-560`).

So: any stall of the page's JS -> `edits_in_progress` never resolves -> `poll_vdom` returns
early forever -> all Rust futures, including tokio timers and an in-flight
`reqwest::Response::chunk()` loop, stop. That matches the observed symptom exactly ("froze
immediately after a signal write").

The trigger (WebKit background throttling vs macOS App Nap) is an inference in our corpus, but
it is independently corroborated upstream — see below, where a different reporter explicitly
ruled out App Nap (`caffeinate`, `NSAppSleepDisabled=YES` did not help; only
`with_background_throttling(Disabled)` did).

wry side confirmed: `wry-0.53.5/src/lib.rs:1384-1400` (`with_background_throttling`),
enum at `lib.rs:2462-2471`, macOS implementation at
`wry-0.53.5/src/wkwebview/mod.rs:430-454` — it sets `WKPreferences`
`setValue:forKey:@"inactiveSchedulingPolicy"` to `WKInactiveSchedulingPolicy::None`, guarded to
macOS 14+/iOS 17+.

dioxus side confirmed: `grep -rn background_throttling dioxus-desktop-0.7.9/src/` -> nothing.
`Config` (`config.rs:137-370`) has no webview-builder escape hatch at all (`with_on_window`
hands you the tao `Window`, not the `WebViewBuilder`), so **there was no app-level workaround**
short of the window-level one we used. Still true in **0.7.10** (2026-07-30) and
**0.8.0-alpha.1** (2026-07-30): same `poll_edits_flushed` gate (`webview.rs:563` / `:559`), same
`Config` surface, no `background_throttling`.

**UPSTREAM STATUS — already reported, PR open, unmerged.**
- Issue **#5586** and PR **#5587**, both "feat(desktop): expose
  `Config::with_background_throttling` to opt out of WKWebView WebContent suspension"
  (goosewobbler, 2026-05-26, both open, PR has 1 comment, not merged as of dioxus-desktop
  0.7.10 / 0.8.0-alpha.1). The reporter hit it on macOS-ARM CI with a hidden/unfocused window
  and states plainly that `caffeinate -dims`, `NSAppSleepDisabled=YES` and JS keep-alive hacks
  all failed because the throttling is WebKit-internal, not App Nap.
- Adjacent, same family, different platform: **#5091** "iOS Scroll freezes entire Rust event
  loop" (open) and **#4374** "iOS apps freezing after restore from background" (closed; the
  fix for it is the `poll_new_edits_location` block right above our gate, `webview.rs:546-560`).

**FILE? — No new issue for the knob** (duplicate of #5586/#5587). **Yes to a comment on #5586**,
because our data escalates the severity from "webview JS is suspended" to "all Rust async is
suspended", which is a much stronger argument for merging #5587 *and* for fixing the gate.

> **Comment on DioxusLabs/dioxus#5586:**
>
> Another data point, and a reason this is more than a webview-side inconvenience: on
> dioxus-desktop 0.7.9 the WebContent suspension also parks the **Rust** side of the app.
>
> `poll_vdom` returns early while an edit batch is unacknowledged
> (`dioxus-desktop/src/webview.rs:562-566`, `edits.rs:117-118`), and the acknowledgement comes
> from the page's JS over the edits websocket. `dom.wait_for_work()` is what services every
> dioxus task, so while the webview is throttled, tokio timers do not fire and an in-flight
> `reqwest` download stops consuming its body. We saw an 8 MiB streaming download and a 250 ms
> debounce both stop dead, with the process alive at 0% CPU, in 3 of 6 runs of a
> shell-launched (never-activated) window on macOS 26 / M4 Pro; forcing `with_always_on_top`
> took it to 0 failures.
>
> Repro/evidence:
> https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/dioxus-fetch/FRICTION.md
> (frozen-run logs under the same directory's measurements).
>
> With no `Config` escape hatch to the `WebViewBuilder`, there is currently no application-level
> workaround other than keeping the window visible, so #5587 is the only fix available to apps.

**Optional second issue (recommend: yes, it is a distinct defect).** #5587 is a workaround; the
underlying problem is that Rust task servicing is gated on webview liveness.

> **Title:** desktop: VirtualDom task loop is gated on webview edit acknowledgement, so any
> webview stall parks all Rust futures
>
> **Repo:** DioxusLabs/dioxus
>
> **Body:**
> dioxus-desktop 0.7.9 (still the same on 0.7.10 and 0.8.0-alpha.1), macOS 26 / M4 Pro.
>
> `poll_vdom` (`packages/desktop/src/webview.rs`, the `poll_edits_flushed` early-return) will not
> call `dom.wait_for_work()` until the webview has fetched and applied the previous edit batch.
> Because `wait_for_work` is what polls every task created by `spawn`/`use_resource`, a webview
> that stops pulling edits stops *all* Rust async: timers, in-flight HTTP bodies, background
> workers - not just rendering.
>
> On macOS this is reachable without any user error: a shell-launched, never-activated (or
> occluded) window gets its WebContent process throttled by WebKit, and the app freezes
> permanently until re-exposed. We reproduced 3 of 6 runs; forcing the window always-on-top
> made it go away. iOS has the same shape from a different trigger (#5091, #4374).
>
> Expected: rendering back-pressure should throttle *rendering*, not the async runtime. Options
> we can see: poll `dom.wait_for_work()` even while edits are in flight and only defer
> `render_immediate`/`send_edits`; or bound the wait with a timeout and continue.
>
> #5586/#5587 (exposing `with_background_throttling`) removes one trigger on macOS but not the
> coupling.
>
> Evidence:
> https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/dioxus-fetch/FRICTION.md

**Corpus corrections.**
- `report/17-async-network-results.md:60-64` should say the trigger is **WebKit-internal
  WebContent throttling** (upstream #5586 rules out App Nap explicitly), and should note the
  mitigation observation is n=1.
- The line "wry 0.53 has `with_background_throttling(Disabled)` but dioxus 0.7.9 doesn't plumb
  it through" is correct, but the more important half is the `poll_edits_flushed` gate; the
  dashboard should mention it, and should record that #5586/#5587 already exist.

---

## A-3. iced 0.14 — "the default thread-pool executor has no reactor"

**CLAIM** — `report/17-async-network-results.md:42-44`; `apps/iced-fetch/FRICTION.md:19,67-68`.
Plus the older `report/11` item 5 ("iced's `Animation`/`time::every` behind a misleading compile
error").

**ROOT CAUSE — CONFIRMED as description; NOT-A-BUG as a defect.**
- `iced-0.14.0/Cargo.toml` `[features] default = [..., "thread-pool", ...]`.
- `thread-pool` -> `iced_futures/thread-pool` -> `iced_futures-0.14.0/src/backend/native/thread_pool.rs:4`:
  `pub type Executor = futures::executor::ThreadPool;` with an **empty** `pub mod time {}`.
- Selection logic: `iced_futures-0.14.0/src/backend/default.rs:10-30` (tokio > smol > thread-pool > null).
- `futures::executor::ThreadPool` has no IO driver and no timer, so a reqwest future polled on it
  panics `there is no reactor running, must be called from the context of a Tokio 1.x runtime`.
  That panic is reqwest/tokio's, and it is the documented behaviour of running tokio-coupled code
  off a tokio runtime. iced is not doing anything wrong; it simply cannot detect it.

iced already does more than nothing here:
- `iced-0.14.0/src/lib.rs:487-497` is a `compile_error!` if *no* executor feature is enabled,
  naming all three options.
- `iced::time::every` only exists via the tokio/smol backends, so using it on defaults is a
  **compile** error, and `iced-0.14.0/src/time.rs:5-13` carries
  `#[cfg_attr(docsrs, doc(cfg(any(feature = "tokio", feature = "smol", ...))))]` so docs.rs shows
  the feature gate. That gate was added by the merged PR **#2188** ("Add doc to direct developers
  to enable features for `iced::time::every`", 2024).

**UPSTREAM STATUS** — nothing open that matches. Closest hits are #2841 (closed, dependency
hygiene) and #3053 (open, unrelated single-threaded-executor issue). The `time::every`
documentation ask is already closed as #2188.

**FILE? — No.** The report/11 item 5 framing ("misleading compile error") is now inaccurate: the
error is a plain "cannot find function `every`", and docs.rs does show the feature gate. The only
genuine residue is that nothing in iced's docs says "the default executor cannot run
tokio-based libraries such as reqwest". That is a one-paragraph docs PR at most; **not filing**
unless we are already contributing to iced.

**Corpus correction.** `report/17:42-44` is fine as a *description of the trap for the reader*
but should not be presented as an iced defect. Suggest rewording to "iced's default
`futures::executor::ThreadPool` has no reactor, so tokio-based clients (reqwest) panic at
runtime; enable `features = ["tokio"]`. `iced::time::every` at least fails at compile time."

---

## A-4. The remaining "one trap per framework" items

### A-4.1 slint 1.17.1 — `slint::spawn_local` panics with reqwest

**CLAIM** — `report/17-async-network-results.md:57-59`; `apps/slint-fetch/FRICTION.md:11-12,58`.

**EVIDENCE CHECK** — the observation is solid and better-evidenced than most in the corpus: a
dedicated probe binary (`apps/slint-fetch/src/bin/spawnlocal_probe.rs`) exits 101 with
`there is no reactor running, must be called from the context of a Tokio 1.x runtime`
(`probe-stdout.log`).

**RATING — NOT-A-BUG, and one corpus sentence is wrong.**
`slint-1.17.1/lib.rs:289-353` documents this at length in the `spawn_local` rustdoc, under a
heading **"Compatibility with Tokio and other runtimes"**: "Tokio futures require entering the
context of a global Tokio runtime"; "Tokio's current-thread scheduler cannot be used in Slint
main thread"; and it tells you to wrap the future in
`async_compat::Compat::new()`, with a complete worked example using `tokio::net::TcpStream`. It
also warns against `#[tokio::main]`.

`apps/slint-fetch/FRICTION.md:58` says "nothing in the API surface warns you" — **that is
incorrect**; the rustdoc for the exact function warns you in detail. (The same FRICTION file
line 74 shows we knew about `async-compat` and rejected it, so this is a wording error, not a
research gap.)

**FILE? — No.** Documented, expected behaviour.
**Corpus correction — required:** drop "nothing in the API surface warns you" from
`apps/slint-fetch/FRICTION.md:58` and soften `report/17:57-59` to "documented constraint:
`spawn_local` is an executor without a reactor; tokio futures need `async_compat::Compat` (as
the rustdoc says) or a side runtime."

### A-4.2 gpui 0.2.2 — `Application::with_http_client` has no usable implementation

**CLAIM** — `report/17-async-network-results.md:47-49`; `apps/gpui-fetch/FRICTION.md:22-26,76-77,93`.

**ROOT CAUSE — CONFIRMED, with one overstatement to fix.**
- `gpui-0.2.2/src/app.rs:164-170` — `pub fn with_http_client(self, http_client: Arc<dyn HttpClient>)`.
- `gpui-0.2.2/src/app.rs:139` and `:150` — both `Application::new()` and
  `Application::headless()` install `Arc::new(NullHttpClient)`.
- `gpui-0.2.2/src/app.rs:2343-2356` — `NullHttpClient::send` is
  `async move { anyhow::bail!("No HttpClient available") }`.

**Overstatement:** "the only implementation is a `NullHttpClient`" is not literally true. The
re-exported `gpui_http_client` 0.2.2 also ships `HttpClientWithProxy` (`src/http_client.rs:149`),
`HttpClientWithUrl` (`:290`), `BlockedHttpClient` (`:354`, errors `PermissionDenied`
unconditionally) and `FakeHttpClient` (`:458`, behind `test-support`). The first two are
*decorators* that wrap an `Arc<dyn HttpClient>` — they contain no transport. So the accurate
statement is: **no crate in gpui's published dependency graph provides an HTTP transport**;
Zed's `reqwest_client` is not published under an official name. Third parties have republished
it (`reqwest-client-gpui-unofficial`, `bezel-zed-http-client`, `http_client-gpui-standalone`),
which the corpus correctly declined to use.

**UPSTREAM STATUS — unknown / nothing found.** Searches of zed-industries/zed for
`with_http_client` and `NullHttpClient` turned up only unrelated internal PRs.

**FILE? — maybe, low value.** gpui is published as a Zed byproduct and issue triage there is
dominated by the editor. If filed, keep it to a docs request:

> **Title:** gpui: `Application::with_http_client` has no shipped transport implementation
>
> **Repo:** zed-industries/zed
>
> **Body:** gpui 0.2.2. `Application::with_http_client` (`crates/gpui/src/app.rs`) and
> `cx.http_client()` suggest the framework provides an HTTP client, but the default is
> `NullHttpClient`, whose `send` unconditionally returns `No HttpClient available`, and the
> re-exported `gpui_http_client` 0.2.2 ships only decorators (`HttpClientWithProxy`,
> `HttpClientWithUrl`), `BlockedHttpClient` and a test `FakeHttpClient` - no transport. Zed's
> `reqwest_client` is not published to crates.io. Suggestion: either publish it, or add one
> sentence to the `with_http_client` rustdoc saying an application must supply its own
> implementation. Currently the only way to discover this is to read `app.rs`.

**Corpus correction.** Replace "the only implementation is a `NullHttpClient`" with "no
transport implementation is published; the default is `NullHttpClient`, which errors
unconditionally".

### A-4.3 ehttp 0.7.1 — the timeout claim is wrong, but there is a real (bigger) bug

**CLAIM** — `report/17-async-network-results.md:45-46`: "ehttp's `Request::get` sets a default
timeout that kills an 8 s stream - must clear it; plain (non-streaming) fetches cannot be
aborted." App code: `apps/egui-fetch/src/main.rs:181` `request.timeout = None; // ~8 s stream;
don't let the default timeout kill it`.

**EVIDENCE CHECK — the stated consequence is NOT supported.** `Request::DEFAULT_TIMEOUT` is
**30 s** (`ehttp-0.7.1/src/types.rs:200-201`, set in `Request::new` at `types.rs:211`). A 30 s
budget does not kill an 8 s stream. The app set `timeout = None` defensively after a source
read; there is no log of an actual timeout kill. `report/17:45-46` should be corrected.

**ROOT CAUSE — CONFIRMED: a different, real defect in the same function.**
`ehttp-0.7.1/src/types.rs:389-435`, `fetch_raw_native(&self, with_timeout: bool)`:

```rust
if self.method.contains_body() {                 // POST / PATCH / PUT
    req = { if with_timeout { req.config() }                                  // <- NO timeout
            else          { req.config().timeout_recv_body(self.timeout) }    // <- timeout
            .http_status_as_error(false).build() };
} else {                                          // GET / DELETE / HEAD / ...
    req = req.config()
        .timeout_recv_body(self.timeout)          // <- `with_timeout` ignored entirely
        .http_status_as_error(false).build();
}
```

Callers:
- `ehttp-0.7.1/src/native.rs:27` — `fetch_blocking` calls `fetch_raw_native(true)` (wants a timeout).
- `ehttp-0.7.1/src/streaming/native.rs:12` — `fetch_streaming_blocking` calls
  `fetch_raw_native(false)` (wants no timeout, because a stream can be long).

So the flag is honoured **nowhere**:
- a streaming **GET** (the common case) gets `timeout_recv_body(Some(30s))` even though the
  streaming path asked for none -> **any stream longer than 30 s is aborted mid-body**;
  `ureq-3.3.0/src/config.rs:742-745` documents `timeout_recv_body` as "Max duration for
  receiving the response body", default `None`.
- a non-streaming **POST/PUT/PATCH** gets **no** timeout at all despite `with_timeout == true`.

Introduced in `Update to ureq 3 (#76)` (2026-01-06), released in ehttp 0.7.0; **still present on
`emilk/ehttp` main** (`ehttp/src/types.rs:389-435`, verified against a fresh clone).

**UPSTREAM STATUS — not reported.** Searches of emilk/ehttp for `timeout`, `streaming timeout`,
`abort`, `cancel` return only #71 (closed, added the configurable timeout), #72, #74.

**FILE? — Yes.**

> **Title:** `fetch_raw_native`'s `with_timeout` flag is ignored for GET and inverted for
> POST/PUT/PATCH
>
> **Repo:** emilk/ehttp
>
> **Body:**
> ehttp 0.7.1 (and current `main`), native backend, `ehttp/src/types.rs` `fetch_raw_native`.
>
> ```rust
> pub fn fetch_raw_native(&self, with_timeout: bool) -> Result<..> {
>     if self.method.contains_body() {
>         req = { if with_timeout { req.config() }                                // no timeout
>                 else            { req.config().timeout_recv_body(self.timeout) }
>                 .http_status_as_error(false).build() };
>     } else {
>         req = req.config()
>             .timeout_recv_body(self.timeout)      // `with_timeout` never consulted
>             .http_status_as_error(false).build();
>     }
> ```
>
> `fetch_blocking` calls it with `true` and `streaming::fetch_streaming_blocking` calls it with
> `false`, so:
>
> * **Streaming GET is capped at `Request::DEFAULT_TIMEOUT` (30 s).** `timeout_recv_body` is
>   ureq's *total* body-receive budget, so any streamed download longer than 30 s is aborted
>   mid-body even though the streaming path explicitly requested no timeout. Callers have to
>   know to set `request.timeout = None` by hand.
> * **Non-streaming POST/PUT/PATCH gets no body timeout at all**, the opposite of what
>   `fetch_blocking(true)` asks for.
>
> Expected: `with_timeout == true` applies `self.timeout`, `false` applies none, for every
> method. Fix looks like moving the conditional out of the `contains_body()` branch and applying
> it in both.
>
> Found while building an egui client against a deliberately slow local server; the corpus is at
> https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/egui-fetch/FRICTION.md

**Second half of the claim — "plain fetches cannot be aborted" — CONFIRMED as an API-shape
limitation, NOT-A-BUG.** `ehttp::fetch` spawns a thread and takes a callback; it returns no
handle, so there is nothing to abort. `ehttp::streaming::fetch` *can* abort via
`ControlFlow::Break` (our egui app proved it with a server-side `ABORT /download after 20/64
chunks`). A "return an abort handle from `fetch`" feature request would be reasonable but is
not a defect; **not filing**.

### A-4.4 tauri-plugin-http 2.5.9 — late abort raises an unhandled rejection

**CLAIM** — `report/17-async-network-results.md:54` ("its fetch shim once froze the webview on a
late abort"); `apps/tauri-fetch/FRICTION.md:68-84`.

Two separable things:

**(i) The webview freeze — UNVERIFIED, n=1, do not file.** `apps/tauri-fetch/FRICTION.md:70-76`
is explicit: observed once (fetch-run4), never reproduced, root cause not isolated. The
report/17 wording "once froze the webview" is accurate but should not appear in a bug-count
column. Likewise the separate `fetch-run1` "no output at all" anomaly
(`apps/tauri-fetch/FRICTION.md:86-89`) is n=1 and unexplained.

**(ii) The unhandled rejection on abort-after-settle — CONFIRMED and deterministic.**
`tauri-plugin-http-2.5.9/api-iife.js` (minified; source at `guest-js/index.ts:213,273`):

```js
const p = () => invoke("plugin:http|fetch_cancel", { rid: h });
signal?.addEventListener("abort", () => { p() });                 // never removed, no .catch
...
new ReadableStream({
  start: c => { signal?.addEventListener("abort", () => { c.error(r), R() }) },  // R = fetch_cancel_body
  ...
})
```

Neither listener is removed when the request settles, and neither `invoke` is `.catch()`ed.
After `fetch_send` has consumed the request resource, `fetch_cancel`
(`tauri-plugin-http-2.5.9/src/commands.rs:355-363`) does
`resources_table.get::<FetchRequest>(rid)?` on a dead rid and returns
`The resource id N is invalid`; `fetch_cancel_body` (`commands.rs:451-458`) likewise
`resources_table.close(rid)?`. Result: an unhandled promise rejection in the page, plus
`controller.error()` on an already-closed stream. WHATWG `fetch` treats an abort after
settlement as a no-op, so this is a real deviation from the shim's "fetch-compatible" claim.

**UPSTREAM STATUS — related issue closed, this residue not reported.**
**#1376** "HTTP Request Cancellation Semantics" (tauri-apps/plugins-workspace, closed
2024-07-08 as completed by PR #1395) fixed the *opposite* failure (aborts not taking effect).
Our cancellation worked (server logged `ABORT /download after 14/64 chunks`); the leftover is
the post-settle rejection. Nothing found for it.

**FILE? — Yes, small.**

> **Title:** http: aborting an `AbortController` after the request has settled raises an
> unhandled rejection (`The resource id N is invalid`)
>
> **Repo:** tauri-apps/plugins-workspace
>
> **Body:**
> tauri-plugin-http 2.5.9 / @tauri-apps/plugin-http, tauri 2.11.5, macOS 26 (WKWebView).
>
> The JS shim registers two `abort` listeners that are never removed and whose `invoke` calls are
> not `.catch()`ed (`plugins/http/guest-js/index.ts`, around the `signal?.addEventListener('abort', ...)`
> at the request level and inside `ReadableStream.start`). Once the request has settled,
> `plugin:http|fetch_cancel` hits a dead resource id
> (`plugins/http/src/commands.rs`, `fetch_cancel` -> `resources_table.get::<FetchRequest>(rid)?`)
> and rejects with `The resource id N is invalid`. Because nothing handles that promise, the page
> gets an unhandled rejection; the stream branch additionally calls `controller.error()` on an
> already-closed stream.
>
> Repro:
> ```ts
> import { fetch } from "@tauri-apps/plugin-http";
> const ac = new AbortController();
> const res = await fetch(url, { signal: ac.signal });
> await res.text();
> ac.abort();            // -> unhandled rejection: The resource id N is invalid
> ```
>
> Expected (per the fetch standard): aborting after the fetch has settled is a no-op.
> Actual: unhandled rejection. Fix: remove the abort listeners when the request settles, and/or
> `.catch()` the fire-and-forget `fetch_cancel` / `fetch_cancel_body` invokes.
>
> Related: #1376 (closed by #1395) fixed the inverse - aborts not taking effect. Cancellation
> itself now works correctly for us (server-side `ABORT` confirmed mid-stream).
>
> Context:
> https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/tauri-fetch/FRICTION.md

**Corpus correction.** `report/17:54` should read "its fetch shim raises an unhandled rejection
on a late abort (deterministic); one run additionally froze the webview (n=1, unexplained)".

### A-4.5 xilem 0.4.0 — re-exports tokio without `macros`

**CLAIM** — `report/17-async-network-results.md:55-56`.
**ROOT CAUSE — CONFIRMED, trivial.** `xilem-0.4.0/src/lib.rs:164` `pub use tokio;`, and
`xilem-0.4.0/Cargo.toml:192-200` enables only `["rt", "rt-multi-thread", "time", "sync"]`. So
`xilem::tokio::select!` does not exist and an app needs its own `tokio` dependency with
`features = ["macros"]`.

**FILE? — No** (or at most a one-line feature request to add `macros` to the re-export, which
costs downstream users a proc-macro dep they may not want). Keep as a documented papercut.

---

## A-5. Grid (report/16)

Pinned: iced 0.14.0 / **iced_widget 0.14.2**; egui + egui_extras + eframe **0.35.0**;
slint / i-slint-core / i-slint-compiler **1.17.1**; xilem 0.4.0 / masonry 0.4.0 /
ui-events 0.2.0 / ui-events-winit 0.2.0; **winit 0.30.13** in all four apps.

### A-5(a) iced 0.14 `table` materializes one Element per cell for all rows

**CLAIM** — `report/16-data-grid-results.md:54-56`; `apps/iced-grid/FRICTION.md:33-34`.
Evidence label in FRICTION is honestly `source-only` for the built-in's limits.

**ROOT CAUSE — CONFIRMED.** `iced_widget-0.14.2/src/table.rs:96-127`:

```rust
let mut cells = Vec::with_capacity(columns.size_hint().0 * (1 + rows.size_hint().0));
...
for row in rows { for view in &views { let cell = view(row.clone()); ... cells.push(cell); } }
```

`Table` stores a flat `cells: Vec<Element<'a, ...>>` (`table.rs:56`) built at construction, so
every `view()` and every `T::clone()` runs for every row on every view call. Exact count for our
app: 100 000 rows x 6 columns + 6 headers = **600 006 Elements**. The whole builder surface is
`width / padding* / separator*` on `Table` and `width / align_x / align_y` on `Column`
(`table.rs:149-193, 650-673`) — no virtualization, sort, selection or resize.
Sub-claim also confirmed: `lazy` is a diff-skipping wrapper, not a virtualizer
(`iced_widget-0.14.2/src/lazy.rs:28`).

**UPSTREAM STATUS — still present on `master`** (identical double loop at
`widget/src/table.rs:117-122`); no virtual-list widget exists on master. The only virtualization
tracker is **#160 `InfiniteList` widget** (open since 2020). No issue or discussion raises
`table`'s non-virtualization.

**Verdict — not a defect; a documentation gap.** `table.rs:1` is `//! Display tables.` and the
type doc is "A grid-like visual representation of data distributed in columns and rows" —
nothing about the cost model. (Commit `420de715` "Clarify description of `Table` in docs" only
reworded that sentence.)

**FILE? — Yes, docs issue, iced-rs/iced.**

> **Title:** docs: `widget::table` materializes one `Element` per cell for every row — say so in
> the rustdoc
>
> **Body:** iced 0.14.0 / iced_widget 0.14.2 (same on `master`, 2026-08); platform-independent.
> `table::Table::new` builds and stores an `Element` for every cell of every row up front
> (`widget/src/table.rs:118-127` in 0.14.2, `:117-122` on master), so
> `table(columns_6, rows_100k)` constructs 600 000 `Element`s and 600 000 `T::clone()`s per view
> call. That is a reasonable design for a layout helper — the request is only that the docs say
> so. Today the module doc is `//! Display tables.` and the type doc is "A grid-like visual
> representation of data distributed in columns and rows"; neither mentions that the widget is
> O(rows x columns) in widgets, nor that it offers no virtualization, sort, selection or resize
> (the full builder surface is `width`/`padding*`/`separator*`). Readers comparing frameworks
> will take "iced has a table widget" to mean "iced has a data grid".
> Suggested: one paragraph on `table()`/`Table` — "`Table` is a layout helper: it creates an
> `Element` for every cell of every row. It is intended for small, fully visible data sets. For
> large data sets you need to window the rows yourself (see #160)." — plus a link to #160.

### A-5(b) Slint `StandardTableView`: text-only cells, single current-row

**ROOT CAUSE — CONFIRMED (both halves), still present on master.**
`i-slint-compiler-1.17.1/widgets/cupertino/tableview.slint:125`
`in property <[[StandardListViewItem]]> rows;`; the cell body is a hard-coded `Text` with
`text: cell.text` (`:246-263`); `StandardListViewItem` has exactly one field
(`i-slint-core-1.17.1/model.rs:832-841`); there is no cell-render callback in the component's
surface (`:124-144`). Selection is a scalar `in-out property <int> current-row: -1;` (`:128`)
with the highlight bound `selected: idx == root.current-row` (`:232`). Same in fluent, cosmic,
material and qt styles. `row-pointer-event` does deliver `event.modifiers`, so the *input* for
range selection exists but there is nowhere to store a second selected row.

**UPSTREAM STATUS —**
- custom cell content: **already tracked** — slint-ui/slint **#3596** (open, 2023-10-03,
  `a:widgets`) and an unchecked box on the tracking issue **#2033** ("More cell types like
  selection (ComboBox) and bool (CheckBox)").
- multi/range selection: **no issue found** (two query phrasings; #2824 is closed and is what
  shipped the single `current-row`).

**FILE? — maybe, one issue for the selection half only** (the cell-content half would duplicate
#3596/#2033).

> **Title:** `StandardTableView`: no way to express multi-row / range selection (single
> current-row only)
>
> **Body:** slint 1.17.1 (also on master); style-independent. Selection is a single scalar —
> `internal/compiler/widgets/*/tableview.slint`: `in-out property <int> current-row: -1;` with
> `selected: idx == root.current-row`. Shift- or cmd-clicking two rows highlights only the last.
> `row-pointer-event(row, event, position)` does deliver `event.modifiers`, so the input is
> available, but there is no property to hold a second selected row and the highlight binding
> cannot render one; any range/multi-select requirement forces abandoning the widget for a
> hand-built `ListView`. Suggested shape: `in-out property <[int]> selected-rows` (or a
> `selection-mode` enum) with the highlight bound to membership. Related: #2033 (tracking), #3596
> (custom cell rendering), #2824 (closed — shipped the single-row model).

### A-5(c) egui: selectable label text eats row clicks in `sense()`-enabled tables

**ROOT CAUSE — CONFIRMED.** Three steps:
1. `egui-0.35.0/src/widgets/label.rs:141-168` — when `selectable`, the label's sense becomes
   `Sense::click_and_drag()`.
2. `egui-0.35.0/src/style.rs:934,1482` — `Style::interaction.selectable_labels` defaults to
   `true`.
3. `egui-0.35.0/src/hit_test.rs:76-79` — "In tie, pick last = topmost", and the label is added
   inside (above) the cell `Ui`, so it out-competes the cell's own `WidgetRect`.
   `egui_extras-0.35.0/src/layout.rs:172` (`child_ui.response()`) and
   `egui_extras-0.35.0/src/table.rs:1345-1353` (`TableRow::response()` = union of cell responses)
   therefore report no click.
Docs: `TableBuilder::sense()` rustdoc (`egui_extras-0.35.0/src/table.rs:291-295`) is one line and
does not warn.

**UPSTREAM STATUS — already reported and labelled `bug`: emilk/egui #5045** (open since
2024-08-31), identical symptom and identical workaround (`Label::new(..).selectable(false)`);
still present on main (`crates/egui/src/widgets/label.rs:154-169` byte-identical).
Related open: #2000 (table row selection), #8144 (per-button senses).

**FILE? — No.** Duplicate of #5045. (An optional easy docs PR could add a warning to
`TableBuilder::sense()` / `TableRow::response()`.)

### A-5(d) "winit delivers no modifiers on mouse events"

**ROOT CAUSE — CONFIRMED for winit and iced; MISATTRIBUTED for xilem.**
- winit 0.30.13 `src/event.rs:278` — `MouseInput { device_id, state, button }`, no modifiers;
  modifiers arrive only via `ModifiersChanged` (`event.rs:222`). Same on master's successor
  variant `PointerButton` (`winit-core/src/event.rs:311-345`).
- iced 0.14: `iced_core-0.14.0/src/mouse/event.rs:26-29` carries no modifiers, and `Modifiers`
  is reachable only under `iced_core/src/keyboard*`. iced's own widgets each keep a private copy
  (`iced_widget-0.14.2/src/text_input.rs:1262,778,911-915`; `slider.rs:410`; `scrollable.rs:999`;
  `pick_list.rs:522`), so the persistent subscription is iced's own idiom.
- **xilem: the corpus footnote is wrong.** `ui-events-0.2.0/src/pointer/mod.rs:144` —
  `PointerState { ..., pub modifiers: Modifiers }` — and it *is* populated by
  `ui-events-winit-0.2.0/src/lib.rs:105-107` doing exactly the persistent tracking iced makes the
  app do; `PointerState` rides on every `PointerButtonEvent`/`PointerUpdate`/`PointerScrollEvent`.
  What xilem 0.4 lacks is a **view-layer** hook (grep for `modifiers` across `xilem-0.4.0/src/`
  hits only a doc comment in `split.rs:31`). `apps/xilem-grid/FRICTION.md:23` states this
  correctly; only report/16's compression conflates the layers.

**UPSTREAM STATUS**
- winit: acknowledged design decision. **#4522** (open, 2026-03-17) proposes a
  `Window::modifiers()` getter instead of per-event modifiers; also **#4236**, **#4239**.
- iced: **already requested and rejected.** PR **#2733** "Add keyboard modifiers to mouse events"
  (closed) and issue **#3158** "Modifiers system needs rework" (closed 2025-12-20), the last
  comment being hecrj's "No, I don't think so."

**FILE? — No.** Every half is upstream-decided or already litigated.

### A-5(e) xilem sort arrows ▲/▼ as tofu

Observation exists (`apps/xilem-grid/FRICTION.md:20,72-73`); same fontique root cause as the
text-stack finding (xilem-grid pins fontique 0.6.0 / parley 0.6.0 via masonry 0.4.0). **Not
re-investigated here; belongs to the fontique verifier.** One nuance: the shipped app shows no
tofu — the arrows were replaced with ASCII `^`/`v` before the recorded run — so report/16's
phrasing reads more like a shipped defect than it was.

### A-5 corpus corrections
1. **`report/16:73-75` (footnote ⁵) is misattributed and should be rewritten.** Proposed:
   "winit delivers modifiers only via a separate `ModifiersChanged` event: iced surfaces that raw
   model to the app, so shift/cmd-click needs a persistent subscription; masonry already folds it
   into `PointerState.modifiers`, but xilem 0.4 exposes no stock view that reads it, so a custom
   masonry widget is required." Add a half-sentence that iced's per-event modifiers were proposed
   (PR #2733) and declined (#3158) — otherwise a reader takes it for an open opportunity.
2. **`report/16:66-67` (footnote ³)** — "sort arrows rendered as tofu" was a development-time
   sighting; suggest "were rendered as tofu until swapped for ASCII".
3. **`report/16:54-56` (footnote ¹)** — "600k widgets" is loose but conservative; the exact figure
   is 600 006 `Element`s, recreated per view call along with 600 000 `T::clone()`s, and the
   widget also has no selection.
4. Footnotes ¹ (iced) and ⁶ (egui) are otherwise accurate.

**A-5 search-coverage caveat:** GitHub search rate-limited during this pass. Linebender
(xilem/masonry) issues were **not** searched for a "click-with-modifiers view" request, and the
Slint multi-selection "no issue found" rests on two query phrasings rather than an exhaustive
sweep. Everything marked CONFIRMED rests on vendored source or a fetched master file.

---

## A-6. Sweep of the 30 FRICTION files — other upstream-attributable defects

Triage pass over `apps/*-{peek,fetch,grid}/FRICTION.md`, excluding the items verified above and
excluding cpal/rodio API churn. These are **triage-grade**: source-level root causes were not
located except where noted, so each needs its own verification pass before filing.

**Tier 1 — strongest candidates (wrong results / silent failures / unbounded growth)**

| # | Finding | Crate @ version | Rating | Repro |
|---|---|---|---|---|
| 1 | floem-vger atlas: a region that fails to pack is dropped **silently, without cleanup, and does not count toward usage**, while the self-heal (full clear) only fires above 70% tracked usage -> a 1080p preview goes black after ~3 frames and never recovers (`apps/floem-peek/FRICTION.md:18`) | floem-vger 0.3.2 (git `lapce/vger-rs` `54ab8135`) via floem 0.2.0 (git `778bb5f2`) | LIKELY-BUG | bisected with a synthetic source; black at 1080p and 640x360, fine at 320x180 |
| 2 | floem-vger: cached atlas rects are **not invalidated on clear**, so a reused content hash draws a garbled tile (`apps/floem-peek/FRICTION.md:34`) | same | LIKELY-BUG | constant-hash synthetic variant |
| 3 | vizia `VirtualTable`'s own row/cell wrappers eat the click that selects the row -> stock `selectable`/`on_row_select` is dead; only fix is a CSS rule against undocumented internal class names (`apps/vizia-grid/FRICTION.md:47`) | vizia / vizia_core 0.4.0 | LIKELY-BUG | deterministic, in stock widget code |
| 4 | floem `VirtualStack` silently materializes all rows when `visual_rect == layout_rect` -> 16 GiB RSS, no window ever appears, nothing warns (`apps/floem-grid/FRICTION.md:14`) | understory_virtual_list 0.1.0 (git `jrmoulton/understory` `2c2abb8c`) + taffy 0.9.2, floem 0.2.0 | LIKELY-BUG (may be arguably taffy's `min_height: auto`) | hit twice independently (also inflated floem-babel to 1.9 GiB) |
| 5 | iced `scrollable`'s programmatic `scroll_to` does not emit `on_scroll`, so app windowing state and widget offset drift silently (`apps/iced-grid/FRICTION.md:85`) | iced 0.14.0 / iced_widget 0.14.2 | **CONFIRMED at source by me** | structural |
| 6 | gpui `RenderImage` GPU atlas grows without bound unless the app calls `window.drop_image` by hand; nothing warns (`apps/gpui-peek/FRICTION.md:42,123`) | gpui 0.2.2 | LIKELY-BUG | measured with/without the manual drop (RSS stable at ~265 MiB only when dropping) |
| 7 | tauri-codegen embeds frontend assets at macro expansion with no `rerun-if-changed`, so editing `ui/*.js` does not dirty the crate and you silently run stale UI (`apps/tauri-peek/FRICTION.md:193`) | tauri-build / tauri-codegen 2.6.3 | LIKELY-BUG | deterministic |
| 8 | freya's release-mode panic hook shows an `rfd` modal and `exit(1)`s **before** chaining to the previous hook -> hung window, empty stderr, panic message destroyed (`apps/freya-fetch/FRICTION.md:26-30`) | freya 0.4.0 / freya-core 0.4.1 (+ rfd 0.17.2) | LIKELY-BUG | deterministic |
| 9 | `egui_kittest::Harness::run` panics when the UI is continuously animating; must drive with `step()` (`apps/egui-fetch/FRICTION.md:43`) | egui_kittest 0.35.0 | LIKELY-BUG | hit at least twice across apps |

Spot-check I did do, for #5: `iced_widget-0.14.2/src/scrollable.rs:1676-1693` — the
`operation::Scrollable` impl for `State` calls `State::snap_to` / `State::scroll_to` /
`State::scroll_by` (`:1843-1862`), which mutate `offset_x`/`offset_y` directly. The `on_scroll`
publish lives only in `notify_scroll`/`notify_viewport` (`:1572-1633`) on the `update` path,
which the operation never reaches. So programmatic scrolling really does change the offset
without emitting `on_scroll`. Whether that is a bug or intended ("you know you scrolled") is
arguable — but the app cannot obtain the resulting `Viewport`. Low priority; if filed, frame it
as a feature request.

**Tier 2 — plausible, need upstream confirmation**

| # | Finding | Crate @ version | Rating |
|---|---|---|---|
| 10 | masonry `VirtualScroll` hands the driver ids transiently outside the valid range; empty range is "documented jank" (`apps/xilem-grid/FRICTION.md:19`) | masonry / masonry_core 0.4.0 | NEEDS-INVESTIGATION (out-of-range index to a user callback is a contract violation; the jank half may be a known TODO) |
| 11 | vizia auto-width label under-measures with padding and clips the last glyph (`apps/vizia-grid/FRICTION.md:61`) | vizia 0.4.0 over skia-safe 0.93.1 | LIKELY-BUG, n=1 as written |
| 12 | eframe silently drops `ViewportCommand::Screenshot` while occluded (capture lives in the paint path) — command accepted, never fulfilled, no error (`apps/egui-peek/FRICTION.md:103`) | eframe 0.35.0 / egui 0.35.0 | LIKELY-BUG (silent failure) |
| 13 | vizia grid RSS 140 -> 290 MiB over one scroll sweep of recycled rows (Skia paragraph/glyph caches + per-entity style stores) (`apps/vizia-grid/FRICTION.md:38,105`) | vizia 0.4.0 + skia-safe 0.93.1 | NEEDS-INVESTIGATION (plateaus, so may be an intentional unbounded cache) |
| 14 | xilem's stock `image()` view calls `request_layout()` on every `set_image_data` (`apps/xilem-peek/FRICTION.md:24`) | xilem 0.4.0 / masonry 0.4.0 | NEEDS-INVESTIGATION (borders on a perf observation) |
| 15 | nokhwa `RequestedFormatType::Closest` is not "closest" | nokhwa-core 0.1.9 | **Covered above as A-1(f), CONFIRMED at source** |
| 16 | tauri-plugin-http late-abort unhandled rejection | tauri-plugin-http 2.5.9 | **Covered above as A-4.4, CONFIRMED at source** |

**Tier 3 — anecdotes, do not file**
- iced "ghost no-draw": one capture showed a flat theme background while fps counters read 28/28;
  13/13 later captures fine. n=1, and `apps/iced-peek/FRICTION.md:84` already disowns it.
- Tauri first-launch run with no output at all, never reproduced across five later launches
  (`apps/tauri-fetch/FRICTION.md:86-89`). Unexplained.

**Explicitly excluded after review** (so nobody re-triages them): both App Nap findings
(attributed to macOS, not a crate); "framework doesn't have X" items (freya `PressEventData` has
no modifiers, gpui `uniform_list` vertical-only, vizia `ContextProxy::load_image` takes only
encoded bytes, floem `Img` not re-exported, iced/freya table widgets that are layout helpers);
`ContextProxy::emit` failing silently after loop close (our FRICTION calls it correct);
`setPointerCapture` throwing on synthetic pointerIds (harness artifact); the two self-test bugs
tauri-grid found that were "neither in Tauri"; freya `use_side_effect` running at startup and the
spurious initial `FILTER_MS 0` (expected effect semantics); the slint DSL `row` property-name
collision with grid placement (language design); all rodio/cpal renames.

---

## Summary of filing decisions

| Finding | Rating | Upstream | File? |
|---|---|---|---|
| A-1(a) nokhwa fourcc 420v/420f -> YUYV, NV12 -> 10-bit | CONFIRMED | PR #246 open; fixed on `senpai` | No (comment on #246) |
| A-1(b) min-fps entries enumerated but unsettable | CONFIRMED | symptom = #204 open; root cause unreported; on main | **Yes** (or comment on #204) |
| A-1(c) `=0.10.9` no longer compiles (bindings 0.2.4 semver break) | CONFIRMED | #245 open (mirror direction) | No (comment on #245) |
| A-1(d) "Lock Rejected" | NOT-A-BUG behaviour, but `lock()` has 2 unreported defects | 3rd defect in #247/#248 | **Yes** (small) |
| A-1(e) `compatible_list_by_resolution` inverted filter | CONFIRMED | fixed in PR #246 | No |
| A-1(f) `Closest` filters fps on the requested resolution | CONFIRMED | unreported | maybe (fold into A-1(b)) |
| A-2 dioxus occlusion freeze | CONFIRMED (gate) / LIKELY (trigger) | #5586 + PR #5587 open, unmerged | No new issue for the knob; **comment on #5586**; optional 2nd issue for the gate |
| A-3 iced default executor has no reactor | NOT-A-BUG | #2188 merged for the `time::every` docs | No |
| A-4.1 slint `spawn_local` panics with reqwest | NOT-A-BUG (rustdoc documents it) | n/a | No; **corpus wording must be corrected** |
| A-4.2 gpui `with_http_client` / no transport | CONFIRMED (overstated as written) | nothing found | maybe (docs) |
| A-4.3 ehttp timeout | corpus claim WRONG; a **different** confirmed bug found | unreported, present on main | **Yes** |
| A-4.4 tauri-plugin-http late abort | rejection CONFIRMED; freeze UNVERIFIED (n=1) | #1376 closed (different half) | **Yes** (rejection only) |
| A-4.5 xilem tokio without `macros` | CONFIRMED, trivial | n/a | No |
| A-5(a) iced `table` no virtualization | CONFIRMED | on master; #160 is the only virtualization tracker | **Yes** (docs) |
| A-5(b) Slint `StandardTableView` | CONFIRMED | cells = #3596/#2033; selection unreported | maybe (selection half) |
| A-5(c) egui selectable-label eats clicks | CONFIRMED | #5045 open, `bug` | No |
| A-5(d) no modifiers on mouse events | CONFIRMED for winit/iced; MISATTRIBUTED for xilem | winit #4522; iced PR #2733 + #3158 both closed/rejected | No; **corpus footnote must be corrected** |
| A-5(e) xilem tofu arrows | defer to fontique verifier | — | No |

**Net: 6 issues worth filing** — nokhwa min-fps (or a comment on #204), nokhwa `lock()`
diagnostics, ehttp `with_timeout`, tauri-plugin-http late abort, iced `table` docs, slint
multi-selection — plus 2 comments (nokhwa #245, dioxus #5586) and 1 optional dioxus issue for the
edit-flush gate.
