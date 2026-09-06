# Hot-patching convergence: subsecond as a cross-framework layer

> Added 2026-08-08, prompted by RCN Zulip comments from Nico Burns (Dioxus
> Labs) and Mikayla Maki (Zed). All claims below were re-verified against
> primary sources on 2026-08-08 before recording; this corrects the earlier
> corpus framing of subsecond as a Dioxus-only feature.

## Verified claims

```yaml
subsecond:
  crate: subsecond (DioxusLabs/dioxus monorepo, packages/subsecond)
  version_checked: "0.7.10 (2026-07-30)"
  mechanism: "hot-patching via function-call detour through a jump table; does not modify process memory; requires an external tool implementing the subsecond compiler/protocol (dx, or an alternative)"
  framework_agnostic: VERIFIED
  evidence:
    - "README: 'Framework authors can interleave their own hot-reload entrypoints alongside user code'"
    - https://docs.rs/subsecond/latest/subsecond/index.html
    - https://github.com/DioxusLabs/dioxus/tree/main/packages/subsecond
adopters:
  bevy:
    status: VERIFIED — shipped
    detail: "bevy#19309 merged 2025-06-03; Bevy 0.17 (2025-09-30) cargo feature `hotpatching` = bevy_ecs/hotpatching + dep:dioxus-devtools; driven via `dx serve --hot-patch --features bevy/hotpatching`"
    limits: "binary crate only; no Wasm; changed system parameters not hot-reloaded"
    sources: [https://github.com/bevyengine/bevy/pull/19309, https://bevy.org/news/bevy-0-17/]
  iced:
    status: VERIFIED — in progress (PR open at check time)
    detail: "iced#3000 'Hot Reloading' by hecrj: iced integrates internally with subsecond ('your code does not need to change at all'); root-crate changes only, State/Message type changes need cold restart, no Wasm"
    source: https://github.com/iced-rs/iced/pull/3000
  axum:
    status: VERIFIED — documented
    detail: "dioxus 0.7.0 added `dioxus::serve` enabling hot-patching of axum routers; starter integrations listed for Axum, Bevy, Ratatui"
    source: https://github.com/DioxusLabs/dioxus/releases/tag/v0.7.0
drivers:
  dx: "dioxus-cli; since 0.7.0 'usable with any Rust project, not just Dioxus projects' (release notes)"
  cargo-hot:
    status: VERIFIED to exist; experimental
    detail: "hecrj/cargo-hot (personal repo of the iced author, NOT iced-rs org); 'cargo run but with hot reloading, powered by subsecond'; 'Most of the code is taken from their dioxus-cli tool'; README: 'Currently just an exploration. Very broken!'"
    source: https://github.com/hecrj/cargo-hot
caveat_rustc: "Mikayla Maki (Zed) on the RCN Zulip: the binding constraint is rustc rebuild latency, not the patching mechanism. Consistent with this corpus (1–4 s incrementals on tiny apps) but NOT tested here at production scale — recorded as an external assessment, not a measurement."
```

## What this changes in the corpus

- The dioxus verdict's "experimental subsecond hotpatch" is no longer a
  dioxus-only differentiator: the same layer now drives Bevy in a shipped
  release and iced in an open PR, making hot-patching a convergence story
  (like winit/wgpu/taffy/AccessKit) rather than a per-framework feature.
- No measurement in this corpus exercises hot-patching; this file records
  upstream adoption facts only.
