# Iteration-5 Linux reality-check rows — raw agent returns

Run date 2026-07-10. Environment: measurements/linux-env.txt (arm64 Debian 12
container, Xvfb 1280x800, no WM/compositor/tray host, lavapipe Vulkan +
llvmpipe GL, Mesa 22.3.6 / LLVM 15.0.6, container rustc 1.97.0 vs macOS runs'
1.96.1 — lockfile-pinned deps). Logs/screenshots retained in linux-results/.

## Agent 1 — native four (iced, egui, slint, xilem todo apps)

```yaml
- app: iced-app
  compile_ok: yes (zero source changes; 237 crates)
  run_alive_10s: no (default) / yes (both fallbacks)
  renderer_path: "default = wgpu Vulkan on lavapipe → FATAL PANIC in Device::create_shader_module (no automatic downgrade to tiny-skia); ICED_BACKEND=tiny-skia renders; WGPU_BACKEND=gl also renders"
  workarounds_required: [ICED_BACKEND=tiny-skia (or WGPU_BACKEND=gl)]
  screenshot_ok: "default no (black); tiny-skia yes (full todo UI, correct fonts); wgpu-gl yes"
  evidence: observed (fallbacks screenshot-read-back; crash verbatim from run log)
  errors_verbatim: "wgpu error: Validation Error ... In Device::create_shader_module, label = iced_wgpu.quad.solid.shader ... unpack2x16float ... Shader requires capability Capabilities(SHADER_FLOAT16_IN_FLOAT32)"
  notes: iced's quad shader uses f16 unpacking that lavapipe (Mesa 22.3) does not expose; iced ABORTS instead of falling back. Both documented escape hatches render pixel-correct UI.

- app: egui-app
  compile_ok: yes (zero source changes; 264 crates; egui_glow not compiled in — glow is compile-time)
  run_alive_10s: yes
  renderer_path: "wgpu works out of the box; forced vulkan and gl runs produce byte-identical screenshots (SHA-1 match); default-backend identity inferred, outcome invariant"
  workarounds_required: []
  screenshot_ok: yes (dark-theme todo UI, fonts OK; 3 variant PNGs identical)
  evidence: observed
  errors_verbatim: "error: XDG_RUNTIME_DIR is invalid or not set in the environment. (x4, non-fatal — bare container env)"
  notes: Only framework of the four whose DEFAULT path just works on software Vulkan.

- app: slint-app
  compile_ok: yes (zero source changes; 400 crates — largest build)
  run_alive_10s: yes
  renderer_path: "self-reported: 'Backend: FemtoVG renderer with OpenGL backend' (llvmpipe GL). SLINT_BACKEND=software also renders."
  workarounds_required: []
  screenshot_ok: yes (light-theme todo UI; software variant also renders; only framework that prints an explicit backend line)
  evidence: observed (renderer identity observed directly)
  errors_verbatim: none
  notes: Cleanest Linux story — default GL and software fallback both verified. Idle FPS ~0-2 is refresh_lazy, not a defect.

- app: xilem-app
  compile_ok: yes (zero source changes) — first attempt SIGKILLed compiling ash (HOST OOM from ~7 concurrent sibling containers; infra, not code; retry 56 s)
  run_alive_10s: no (default) / yes (WGPU_BACKEND=gl)
  renderer_path: "default = vello/wgpu on lavapipe Vulkan → lavapipe's LLVM-15 shader JIT ABORTS the whole process (uncatchable, not a validation error) compiling a vello compute shader; WGPU_BACKEND=gl runs vello's full compute pipeline on llvmpipe and renders correctly"
  workarounds_required: [WGPU_BACKEND=gl]
  screenshot_ok: "default no (black); wgpu-gl yes (dark-theme todo UI, fonts OK)"
  evidence: observed (GL fallback); crash verbatim (default)
  errors_verbatim: "LLVM ERROR: Cannot select: 0xfffed87751e8: v4f32 = truncate ... In function: cs_co_variant"
  notes: lavapipe/LLVM-15 aarch64 JIT bug surfaced by vello's compute shaders — a hard abort an app cannot catch and fall back from at runtime. Real-GPU Linux likely unaffected; software-Vulkan CI will hit it.
```

SURPRISES:
- The two default-path failures (iced, xilem) are both lavapipe-Vulkan-specific and shader-content-specific — egui's wgpu runs clean on the exact same device. "wgpu works on Linux" decomposes per-framework.
- WGPU_BACKEND=gl (llvmpipe) rescues BOTH failures — even vello's compute pipeline renders on software GL. Software VULKAN is the fragile path here, inverting the "GL is legacy" assumption.
- iced does not automatically degrade to tiny-skia when its wgpu shaders fail — it panics; the fallback exists but is opt-in via env var.
- All four compiled with zero source changes and zero extra system packages — porting cost was entirely in the renderer/runtime layer.

TIME_SINK:
- xilem's first build OOM-SIGKILLed (7 sibling containers building concurrently); one retry cycle.
- No app initializes a logger (RUST_LOG inert) — renderer identity established by forced-backend elimination runs.
- run-app.sh hardcodes output names; fallback runs used a sed-suffixed copy to avoid clobbering default-run evidence.

## Agent 3 — GTK fault line probes + Babel text on Linux

```yaml
fault_line:
  iced_tray_compile: {result: "COMPILE_OK on Linux, zero source changes (the only macOS-only call, muda init_for_nsapp, was already cfg-gated)", errors_verbatim: "none from code; first attempt env-OOM on ash (SIGKILL), retry jobs=2 clean"}
  egui_tray_compile: {result: "COMPILE_OK first try (jobs=2)", errors_verbatim: none}
  # BOTH tray apps DIE AT RUNTIME: compile-clean, panic on launch, identical:
  #   Gtk-CRITICAL gtk_icon_theme_get_for_screen ... panicked at gtk-0.18.2/src/auto/menu.rs:29:
  #   "GTK has not been initialized. Call `gtk::init` first."
  # Fires at the FIRST muda menu-object creation (gtk::Menu::new) — muda PANICS instead of
  # returning Err, so the apps' graceful error handling never runs.
  muda_on_winit: {result: "CONFIRMED at the TYPE level — Menu::init_for_gtk_window requires W: IsA<gtk::Window> + IsA<gtk::Container> (muda 0.19.3 menu.rs:195-199); no winit-facing attach API exists on Linux", evidence: "probe fails to compile", verbatim: "error[E0277]: the trait bound `winit::window::Window: IsA<gtk::Window>` is not satisfied"}
  tray_without_gtk: {result: "NUANCED — bare TrayIconBuilder::build (no menu) returns Ok while spraying GTK criticals; with no gtk loop the icon can never function. The hard panic comes from muda menu creation, not TrayIcon itself.", evidence: "probe exit=0; tray-icon 0.24.1 lib.rs:17 documents the gtk-event-loop requirement verbatim", verbatim: "RESULT: TrayIcon::build returned Ok (claim NOT confirmed as an error)"}
  tray_with_gtk_no_host: {result: "SILENT success — gtk::init + build Ok with NO StatusNotifier host on the bus; no error, no feedback that the icon is invisible. (Headless caveat: proves the no-host failure is silent; cannot prove icon visibility on a real desktop.)", evidence: "DBus NameHasOwner(org.kde.StatusNotifierWatcher) → false captured in-session", verbatim: "RESULT: TrayIcon::build returned Ok"}
  global_hotkey_x11: {result: "FULLY CONFIRMED working on X11 — registration AND end-to-end delivery (xdotool-fired chord received by the listener thread)", evidence: observed, verbatim: "register(Ctrl+Shift+K) OK ... EVENT: GlobalHotKeyEvent id=34078749 state=Pressed"}
babel_linux:
  iced_babel:
    compile_ok: yes
    run_alive_10s: "yes — but only with ICED_BACKEND=tiny-skia; default wgpu path hits the same SHADER_FLOAT16_IN_FLOAT32 panic"
    screenshot_ok: yes (X11 capture + window::screenshot self-shot)
    rendering_notes: "All 11 lines render. AR/HE contextually shaped with correct LTR islands and RTL base direction — IDENTICAL layout to the macOS iced-babel artifact (cosmic-text BiDi is cross-platform consistent) [audit correction: visually/order-consistent in the observed run; no pixel-diff retained]. CJK via Noto Sans CJK correct; Devanagari conjuncts ligate; Thai correct. EMOJI: Noto Color Emoji CBDT renders in color (skin-tone, 🏳️‍🌈, flags) BUT singleton 👍/😀 are shadowed by monochrome fonts (Noto Sans Symbols2 / DejaVu) in fontdb fallback — mono/color split by codepoint coverage, a Debian-default-fonts artifact invisible on macOS. ZWJ clusters stay single. Combining marks attach (minor collisions)."
  gpui_babel:
    compile_ok: "yes, but only after fixing an image gap: ld cannot find -lxkbcommon-x11 (runtime lib present, dev symlink missing — ln -s workaround); plus one OOM retry"
    run_alive_10s: yes (also with WM/focus/resize variants)
    screenshot_ok: "file saved but 100% BLACK — no frame ever presented"
    rendering_notes: "TEXT STACK UNVERIFIABLE HEADLESSLY: gpui 0.2.2 creates the X11 window (mapped, IsViewable, depth 32), stays alive, prints NOTHING, never presents a frame — black under bare Xvfb, under openbox with focus/keys, after resize. Attribution clean: vkcube renders PERFECTLY under the same Xvfb+llvmpipe (control retained: vkcube-check.png) — the silent non-paint is gpui/blade-specific."
```

SURPRISES:
- The GTK fault line is SHARPER than the map claims: macOS-written tray apps compile unchanged on Linux, then panic at runtime inside muda's first gtk::Menu::new — build-only Linux CI would pass while every launch dies.
- Bare TrayIcon::build without gtk::init returns Ok (non-functional icon, GTK criticals) — the no-GTK failure is SILENT; the hard panic only comes via muda menus. With gtk but no SNI host: also silent success. Zero feedback either way.
- Emoji on Linux/iced split mono/color by codepoint coverage (fontdb fallback ordering) — Noto Sans Symbols2/DejaVu shadow Noto Color Emoji for singletons while ZWJ/skin-tone/flag sequences render full color.
- gpui is the only stack that silently renders nothing; the vkcube control experiment turns "black PNG" into an attributed finding.

TIME_SINK:
- Shared 7.75GiB Docker VM with up to 7 sibling containers: rustc OOM SIGKILLs killed builds twice each; retries needed CARGO_BUILD_JOBS=2 (~35 min lost).
- gpui black-window forensics: 5 diagnostic runs (xwininfo, WM+focus, resize, strip crop, vkcube control).
- iced-on-lavapipe shader crash masked the tray fault line on the first run — needed the sibling agent's ICED_BACKEND=tiny-skia trick to reach the GTK panic.

Artifacts: probes in linux/probes/ (pinned to the apps' lockfile versions); logs/screenshots/crops in linux-results/ (combined transcript probes.log).

## Agent 2 — gpui + webviews

```yaml
- app: gpui-app
  compile_ok: yes-with-workaround (link failed until apt libxkbcommon-dev/libxkbcommon-x11-dev added — image had only runtime .so.0; all 1100+ crates compiled clean on rustc 1.97)
  run_alive_10s: yes
  renderer_path: "X11Client (gpui's own backend) → blade → Vulkan → lavapipe: device init SUCCEEDS ('Using llvmpipe ... libvulkan_lvp.so', 12 threads); window mapped, IsViewable, depth-32"
  workarounds_required: ["apt libxkbcommon(-x11)-dev at link time"]
  screenshot_ok: no — uniform black at 10/15/30/45/60 s; same under openbox (only WM decorations render)
  evidence: observed (build/run logs, diag logs, epoll-diag, xwininfo, WM variants)
  errors_verbatim: "runtime fully silent — no panic, no log under any debug env (XDG_RUNTIME_DIR noise vanishes when set; behavior unchanged)"
  notes: "ROOT CAUSE ADDED: gpui 0.2.2's X11 event loop is deaf — window event mask is correct and the refresh-loop even guards Xvfb's degenerate RandR mode, but /proc fdinfo shows the X socket fd registered in NO polled epoll set: MapNotify never processed, periodic refresh timer never arms. gpui-specific; no env workaround exists. Renderer stack is NOT the problem. [audit correction: the root cause remains unproven — the retained epoll probe itself failed (awk error) and enumerated no registrations, so the 'X socket in no polled epoll set' event-loop defect is a HYPOTHESIS, not an established root cause; the verified finding is a mapped, healthy-looking window that never presented a frame while a vkcube control rendered fine] [probe2 2026-07-10: hypothesis REFUTED with a working probe (linux/probes/gpui-epoll-probe2.sh; artifacts linux-results/gpui-epoll-probe2-*) — the X socket fd IS registered in the main thread's polled epoll set (fdinfo 'tfd: 8' in epoll fd 3, corroborated by the launch connect() trace and the ss -xp peer map) and IS serviced (292 epoll_pwait(3,…) calls, 370 recvmsg reads on the X fd in a 6 s strace window); the loop renders and submits full non-black frames, but each one goes out as a core-protocol PutImage (opcode 72) with depth 24 onto the depth-32 ARGB window (visual 0x40) and Xvfb rejects every one with BadMatch: 51 PutImage → 51 BadMatch in 6 s, swallowed silently; same-run screenshot still pure black (mean=0). Actual root cause: present-path depth mismatch at the gpui/blade ↔ Mesa-lavapipe software-WSI boundary, not the event loop; vkcube renders because it uses the default depth-24 visual]"

- app: tauri-app
  compile_ok: yes (attempt 1 died to docker-VM OOM — infra; retry clean)
  run_alive_10s: yes (PLAIN, no workarounds)
  renderer_path: "WebKitGTK 4.1 (2.50.6) on X11; WebKitWebProcess + NetworkProcess spawns confirmed"
  workarounds_required: []   # DMABUF and COMPOSITING variants verified UNNECESSARY (identical results)
  screenshot_ok: yes — todo UI renders in plain AND both workaround variants
  evidence: observed (4 run-log/screenshot variants + OOM attempt log)
  errors_verbatim: "dbind-WARNING AT-SPI: Error retrieving accessibility bus address (benign, all runs)"
  notes: Cleanest cell. WebKitGTK 2.50.6 works out-of-the-box under headless Xvfb + llvmpipe.

- app: dioxus-app
  compile_ok: yes (first attempt)
  run_alive_10s: yes (plain)
  renderer_path: "wry/tao → WebKitGTK 4.1 (2.50.6); WebProcess spawn confirmed"
  workarounds_required: []
  screenshot_ok: yes (incl. tao's GTK 'Window/Edit' menubar — the only visible delta vs tauri)
  evidence: observed (4 variants)
  errors_verbatim: "AT-SPI dbind warning (benign)"
  notes: Behaves exactly like tauri (same engine).
```

SURPRISES:
- WebKitGTK 2.50.6 needs NO workarounds on Xvfb+llvmpipe: both WEBKIT_DISABLE_DMABUF_RENDERER and WEBKIT_DISABLE_COMPOSITING_MODE verified unnecessary for both apps — the widely-documented 2.42-2.48-era blank-window fix appears OBSOLETE on 2.50. [audit correction: verified unnecessary only for WebKitGTK 2.50.6 in this Xvfb/llvmpipe environment; current Tauri docs still recommend these workarounds for NVIDIA/driver-conflict setups, so this does not demonstrate general obsolescence]
- gpui fails silently and LATE: Vulkan init succeeds, window mapped — zero frames, dead input, nothing on stderr under any debug env. Worst possible headless-CI failure mode; defect is event-loop plumbing, not the GPU stack. [probe2 2026-07-10: the event-loop attribution is REFUTED — the loop polls and reads the X socket and submits frames; the defect is in the present path: depth-24 PutImage blits onto the depth-32 ARGB window, all rejected BadMatch by the X server and silently swallowed]
- gpui's only compile blocker was a missing -dev symlink (one apt package).
- The shared 8GB docker VM OOM-killed rustc (signal 9) in 2 of 3 first builds with 6 sibling containers — indistinguishable from compiler failure until you read the log tail.

TIME_SINK:
- OOM-retry cycles + container-slot coordination (~25 min).
- gpui black-window diagnosis chain (strace, /proc epoll probes, crate source) before the stop order.
- Cold builds at capped job counts on the contended VM (10-20 min each).

## 2026-08-02 — fresh nine-framework cohort

The current evidence is one atomic, serial 17-target cohort on arm64 Debian
12.14, rustc 1.96.1, Xvfb
1280×800×24, Mesa 22.3.6, and WebKitGTK 2.50.6. Canonical aggregate:
`measurements/reruns/20260802-nine-framework-macos26.6-rust1.96.1/linux/results.csv`;
per-variant logs/results/screenshots are under the sibling `linux/runs/`
directory. All 17 compiled; 11 defaults survived; iced-app, xilem-app, and
iced-babel survived prescribed fallbacks, yielding 14/17 on any tested path.

Fresh Freya/Vizia rows:

| Target | Compile | Default alive 10 s | Visibly painted | Fallback | Note |
|---|---:|---:|---:|---|---|
| freya-app | yes | yes | yes | none | default path |
| freya-tray | yes | yes | yes | none | window/process only; no tray host |
| freya-babel | yes | yes | yes | none | all 11 corpus rows visible |
| vizia-app | yes | yes | yes | none | default path |
| vizia-tray | yes | **no** | no | none | GTK-not-initialized panic, exit 101 |
| vizia-babel | yes | yes | yes | none | all 11 corpus rows visible |

The exact contract also contains all nine `*-app` baselines plus iced/egui/
Freya/Vizia tray and iced/GPUI/Freya/Vizia Babel. The fresh run retains the
default iced shader panics, xilem LLVM abort, GPUI black-frame behavior, and
the separately recorded iced tiny-skia/xilem GL workarounds. July's detailed
raw rows above remain historical diagnostic evidence rather than the current
aggregate.

Freya/Vizia Grid, Fetch, and Peek remain outside the Linux contract and are
explicitly pending: no Linux build/run, live-server cancellation, or hardware
capture result is inferred for those six targets.

## 2026-08-03 status note — the Linux round for the expansion cohort is pending

The `2026-08-02 — fresh nine-framework cohort` table above was produced against
Freya and Vizia crates that were **never retained**: both frameworks' apps were
re-implemented from scratch on 2026-08-03, and a third framework (Floem, pinned
to the git rev `778bb5f2` of lapce/floem `main` — crates.io 0.2.0 is 20 months
stale and `main` is unpublishable) was added at the same time.

**No Linux build or run was performed on 2026-08-03.** Nothing below is
inferred; this note only records what the harness now has to cover.

- The `freya-app / freya-tray / freya-babel / vizia-app / vizia-tray /
  vizia-babel` rows above describe the discarded implementation. Their headline
  results (all six compiled; five defaults alive and painted; **vizia-tray dies
  with a GTK-not-initialized panic, exit 101**) are the right *questions* to
  re-ask, not current answers. The vizia-tray panic in particular should
  reproduce or not on the new crate: today's re-implementation still takes muda
  through `tray-icon`'s re-export, so the same first `gtk::Menu::new` is on the
  path, but the tray icon is now created on the first tick of a 100 ms timer
  rather than at startup, which changes *when* the panic would fire.
- The Linux harness must now include **three new targets**: `floem-app`,
  `floem-tray` and `floem-babel`, alongside the re-run Freya/Vizia six. Three
  floem-specific things are worth capturing on the first Linux pass:
  - renderer path — floem's default tree is floem-vger on **wgpu 27** with a
    **tiny-skia + softbuffer software fallback** compiled in, so it is the first
    framework in the study that could plausibly self-degrade where iced and
    xilem hard-fail on lavapipe. Whether that fallback is reachable by env var
    or only by feature selection is unknown and must be observed, not assumed.
  - the GTK fault line — `floem-tray` links **tray-icon 0.24 (muda 0.19)** *and*
    floem's own **muda 0.17**, i.e. two muda instances in one binary. The
    macOS-only finding is that they coexist because the versions differ; on
    Linux both would want a GTK main loop, so this is the sharpest new probe in
    the set.
  - text — floem's parley/fontique stack renders **Han/kana as tofu on macOS at
    this rev** while Hangul/Devanagari/Thai/BiDi are fine. Whether Linux
    (Debian's Noto fonts, a different fontique discovery path) shows the same
    split is a directly answerable question for `floem-babel`.
- Freya/Vizia **Grid, Fetch and Peek remain outside the Linux contract**, and
  the same now holds for all six floem Grid/Fetch/Peek targets. No Linux
  build/run, live-server cancellation or hardware-capture result is inferred for
  any of them.

```yaml
linux_status_20260803:
  freya: {rows_above: stale, reason: "apps re-implemented from scratch 2026-08-03", rerun: pending_new_cohort}
  vizia: {rows_above: stale, reason: "apps re-implemented from scratch 2026-08-03", rerun: pending_new_cohort}
  floem: {rows_above: none, reason: "framework added 2026-08-03", rerun: pending_new_cohort}
  harness_targets_added: [floem-app, floem-tray, floem-babel]
  still_out_of_contract: [freya-grid, freya-fetch, freya-peek, vizia-grid, vizia-fetch, vizia-peek, floem-grid, floem-fetch, floem-peek]
```
