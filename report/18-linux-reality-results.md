# Linux reality check: the macOS-built apps on Linux (Docker/Xvfb)

**Run date:** 2026-07-10. **Environment** (full manifest:
`measurements/linux-env.txt`): arm64 Debian 12 container, Xvfb 1280x800 (no
WM/compositor/tray host/portals), software GPU only — lavapipe Vulkan +
llvmpipe GL (Mesa 22.3.6 / LLVM 15.0.6), WebKitGTK 2.50.6, Noto
core/CJK/color-emoji fonts, container rustc 1.97.0 (macOS runs used 1.96.1;
lockfiles pin dependencies). Canonical matrix:
`measurements/results-linux.csv`; verbatim logs + screenshots:
`linux-results/`; raw agent rows: [data/linux-rows.md](data/linux-rows.md);
fault-line probe sources: `linux/probes/`.

**Scope caveat up front:** a headless software-GPU container is not a Linux
desktop. It CAN prove compile status, default-render-path behavior on
software Vulkan/GL (≈ CI environments), API-level fault-line behavior, and
rendering correctness where frames appear. It CANNOT prove real-GPU behavior,
tray-icon visibility on a desktop with a StatusNotifier host, or Wayland
(untested — X11 only).

## Headline 1 — porting cost is zero at the source level, real at the render path

All 11 tested apps (7 todo + 2 tray + 2 babel) **compiled on Linux with zero
source changes** (gpui needed one `-dev` package at link time). But the
default render path produced no usable UI for 3 of 5 native stacks on
software Vulkan (2 crashed outright — iced panicked, xilem aborted — and
gpui stayed alive showing a black window):

| App | Compiles | Default runs | Default path | Fix |
|---|---|---|---|---|
| iced | ✓ | **✗ panic** | wgpu/lavapipe: shader needs `SHADER_FLOAT16_IN_FLOAT32`; **no auto-fallback to its own tiny-skia** | `ICED_BACKEND=tiny-skia` or `WGPU_BACKEND=gl` — then pixel-correct |
| egui | ✓ | ✓ | wgpu just works (vulkan/gl byte-identical screenshots) | — |
| slint | ✓ | ✓ | FemtoVG/GL, self-reported; software fallback also verified | — |
| xilem | ✓ | **✗ abort** | vello compute shader triggers an **uncatchable** lavapipe/LLVM-15 JIT abort | `WGPU_BACKEND=gl` — vello's full compute pipeline renders on software GL |
| gpui | ✓* | **✗ black** | Vulkan init *succeeds*, window mapped — **zero frames ever presented, silently** | no workaround found in this Xvfb/X11 configuration |
| tauri | ✓ | ✓ | WebKitGTK 2.50.6 **plain** | — |
| dioxus | ✓ | ✓ | WebKitGTK 2.50.6 plain | — |

Notable inversions of common belief: **software Vulkan is the fragile path
and GL the rescue** (llvmpipe ran even vello's compute shaders); and the
WebKitGTK workarounds were not needed here — `WEBKIT_DISABLE_DMABUF_RENDERER`
and `WEBKIT_DISABLE_COMPOSITING_MODE` were both tested and verified
unnecessary for WebKitGTK 2.50.6 in this Xvfb/llvmpipe environment for both
webview apps; current Tauri docs still recommend them for
NVIDIA/driver-conflict setups, so this does not demonstrate general
obsolescence.

\* gpui: GPUI 0.2.2 produced a mapped, healthy-looking window that never
presented a frame in this environment (a vkcube control rendered fine —
retained as `linux-results/vkcube-check.png`). A working probe (probe2,
2026-07-10: `linux/probes/gpui-epoll-probe2.sh`, artifacts
`linux-results/gpui-epoll-probe2-*`) has now settled the root cause and
REFUTED the earlier event-loop hypothesis: the X socket fd IS registered in
the main thread's polled epoll set and is actively read (292 epoll_pwait
calls and 370 reads on it in a 6 s strace window), and gpui renders and
submits complete non-black frames — but the X server rejects every one
with BadMatch, because the software presentation path (Mesa 22.3.6
lavapipe) blits the swapchain via core-protocol PutImage at depth 24 onto
the depth-32 ARGB window gpui creates (51 PutImage requests → 51 BadMatch
errors in 6 s), and gpui swallows the errors silently. vkcube renders
because it uses the default depth-24 visual. Worst-in-round failure mode
regardless: healthy-looking process, no diagnostics, black window.

## Headline 2 — the GTK fault line, confirmed and sharpened

- **muda menubars on winit windows are impossible at the type level**:
  `Menu::init_for_gtk_window` requires `W: IsA<gtk::Window> + IsA<gtk::Container>`
  (probe fails to compile; verbatim E0277 retained).
- **Our macOS-written tray apps compile UNCHANGED on Linux and panic at
  launch** in muda's first `gtk::Menu::new` ("GTK has not been initialized")
  — muda *panics* rather than returning `Err`, so graceful degradation code
  never runs. Build-only Linux CI passes; every launch dies.
- **The failure modes are silent where they aren't panics**: bare
  `TrayIcon::build` *without* GTK returns `Ok` (icon can never function);
  *with* GTK but no StatusNotifier host on the bus (verified via DBus
  `NameHasOwner` → false), everything still reports success. Zero feedback.
- **global-hotkey on X11: fully confirmed working** end-to-end (registration
  + delivery of an xdotool-fired chord).

## Headline 3 — the text stack travels better than the shell

iced-babel on Linux (tiny-skia): all 11 corpus lines render; **BiDi layout is
visually/order-consistent with the macOS artifact in the observed run** (same
cosmic-text stack);
Devanagari conjuncts, Thai stacking, CJK all correct from Noto. Color emoji:
CBDT **does** render in color (skin tones, 🏳️‍🌈, flags) — but **singleton
emoji (👍, 😀) render monochrome** because Noto Sans Symbols2/DejaVu shadow
Noto Color Emoji in fontdb's fallback ordering — a distro-fonts artifact
invisible on macOS. gpui-babel: unverifiable (black window defect above).

## What this changes in earlier reports

- `20-how-to-build.md`'s Linux advice: replace "WEBKIT_DISABLE_DMABUF_RENDERER
  workaround" guidance with scoped wording (verified unnecessary for WebKitGTK
  2.50.6 in this Xvfb/llvmpipe environment; current Tauri docs still recommend
  the workarounds for NVIDIA/driver-conflict setups); add "test your renderer
  on software Vulkan — software Vulkan is one plausible CI configuration, and
  three of five native defaults fail on it (two fatally, one silently)".
- The GTK fault line (§1.6 / fragmentation story 4) upgrades from
  source-verified to **empirically confirmed with retained artifacts**, plus
  the new nuance that the failures are panics/silence, not errors.
- gpui's platform matrix: "Linux (Wayland+X11)" needs an asterisk — the
  published 0.2.2 X11 path never painted in this environment; root cause now
  proven (probe2 refuted the epoll event-loop hypothesis — the X fd is
  polled and read, frames are rendered and submitted, but every depth-24
  PutImage present is rejected BadMatch by the X server against gpui's
  depth-32 ARGB window, silently).

## Caveats

Headless/software-GPU/X11-only/arm64/one distro (Debian 12); container rustc
1.97.0; no real-GPU or Wayland or desktop-tray verification; the Docker VM
was memory-contended during parallel builds (OOM SIGKILLs documented as infra
artifacts, retried).
