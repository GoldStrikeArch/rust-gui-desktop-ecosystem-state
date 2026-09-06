# Iteration-3 packaging results: `.app` bundles + `.dmg` images on macOS

> Measured 2026-07-09 on Apple M4 Pro / 24 GB, macOS 26.5.2 (Tahoe), rustc/cargo 1.96.1, arm64.
> Inputs: the seven already-built iteration-1 todo apps (`cargo build --release` executables, approximately 5.0–14.7 MiB).
> Goal: empirically test the ecosystem-map §1.8 claim that packaging/signing is the true hard part of desktop Rust — how far is a working binary from a double-clickable `.app` and verified `.dmg`?
> Scope: macOS only; ad-hoc signing only (no Apple ID / Developer ID available, notarization **not attempted** — by design).
> Artifacts: `dist/<framework>/<Name>.app` + `.dmg` (gitignored). All seven `.app`s were observed launching via `open` before and after the local ad-hoc seal; all seven DMGs pass `hdiutil verify`. The DMGs contain ad-hoc-signed app bundles; the images were not Developer-ID signed or notarized.

## Headline

**Every framework packaged in this macOS arm64 experiment.** Six of seven used the same four-line `[package.metadata.bundle]` shape with cargo-bundle; Tauri used its own bundler by changing its config. This establishes a low-effort `.app` path for these already-built inputs, not a universal cross-platform result. No Developer ID identity or notarization credentials were configured: cargo-bundle has no signing facility, while Tauri and cargo-packager skipped their configured-signing paths. All seven therefore began with only the linker's ad-hoc executable signature. The built-in decorative DMG flow was unreliable on this macOS 26 machine, while the simple `hdiutil create -format UDZO` fallback eventually produced all seven images. Gatekeeper's distribution assessment rejected every locally ad-hoc-signed, unnotarized app as expected; each locally built app nevertheless launched on this machine.

## Tool installs (cold, `--locked`, crates.io)

| Tool | Version | Wall time | CPU time | Result |
|---|---|---|---|---|
| cargo-bundle | 0.11.0 | **87 s** | 388 s | OK, no failures |
| cargo-packager | 0.11.8 | **60 s** | 271 s | OK, no failures |
| tauri-cli | 2.11.4 | **201 s** | 1212 s | OK, no failures on this machine |

## Per-framework results

All six non-Tauri apps used **cargo-bundle** (first tool tried; it never failed to produce a `.app`, so cargo-packager was never needed for those final artifacts). Tauri used **tauri-cli / tauri-bundler**. Sizes are MiB: `.app` is `du -sk / 1024`; `.dmg` is logical bytes / 1,048,576. "Local seal" means `codesign -s - --deep --force` followed by `codesign -v --deep --strict`; it is a verification step, not distribution-signing guidance. "spctl" means `spctl --assess --type execute`.

| Framework | Tool | Config effort | Bundle time¹ | `.app` | `.dmg`² | Launch (`open`) | Local ad-hoc seal | spctl | Tool's own DMG step |
|---|---|---|---|---|---|---|---|---|---|
| iced | cargo-bundle 0.11.0 | 4 LOC + icon.icns | 18 s | 9.9 MiB | 4.2 MiB | OK (pre+post seal) | OK | rejected (exit 3) | **worked** (3.7 MiB) |
| egui | cargo-bundle 0.11.0 | 4 LOC + icon.icns | 19 s | 11.9 MiB | 5.5 MiB | OK | OK | rejected | failed ×2 (`hdiutil convert`) |
| gpui | cargo-bundle 0.11.0 | 4 LOC + icon.icns | 12 s | 5.0 MiB | 2.5 MiB | OK | OK | rejected | failed (`hdiutil convert`) |
| tauri | tauri-cli 2.11.4 | 2 LOC (conf.json) | 96 s³ | 8.0 MiB | 3.0 MiB | OK (pre+post seal) | OK | rejected | **failed ×3** (`bundle_dmg.sh` → `hdiutil convert`) |
| xilem | cargo-bundle 0.11.0 | 4 LOC + icon.icns | 11 s | 11.4 MiB | 4.8 MiB | OK | OK | rejected | failed (`hdiutil convert`) |
| slint | cargo-bundle 0.11.0 | 4 LOC + icon.icns | 26 s | 14.7 MiB | 7.4 MiB | OK | OK | rejected | failed (`hdiutil convert`) |
| dioxus | cargo-bundle 0.11.0 | 4 LOC + icon.icns | 37 s | 5.7 MiB | 2.5 MiB | OK | OK | rejected | **worked** (2.0 MiB) |

¹ `cargo bundle --release` wall time with the release binary already built (it re-runs `cargo build --release`, which no-ops); includes the tool's own dmg attempt where it ran.
² Final shipped `.dmg`, produced by one-shot `hdiutil create -volname <Name> -srcfolder <App> -ov -format UDZO` from the **locally ad-hoc-sealed** `.app` (2–13 s each; UDZO ≈ 38–50% of `.app` size).
³ Includes a full re-link forced by the tauri.conf.json edit (the config is compiled into the binary via tauri-build). A later bundle-only rerun (`cargo tauri bundle`) took 25 s.

### The cargo-bundle config (identical shape for all six)

```toml
[package.metadata.bundle]
name = "Iced Tasks"
identifier = "org.rcn.icedapp"
icon = ["icon.icns"]
```

That's the entire framework-side story. Everything else (`CFBundleVersion`, executable name, plist skeleton, Resources layout) is derived. The produced bundle is minimal and sane: `Contents/Info.plist` + `Contents/MacOS/<binary>` + `Contents/Resources/icon.icns`; correct `CFBundleIdentifier`, `CFBundleName`/`DisplayName`, `CFBundleIconFile`, `NSHighResolutionCapable`. Two nits: no `LSMinimumSystemVersion` (tauri-bundler sets 10.13) and a legacy `LSRequiresCarbon` key (harmless; tauri-bundler emits it too).

### The tauri config

`apps/tauri-app/tauri.conf.json`: `"bundle": { "active": false → true }` plus `"targets": ["app", "dmg"]`. Icons were already wired (iteration 1). No Node/npm anywhere: `frontendDist: "ui"` is a static folder, so `cargo tauri build` worked plain. Cost hiding in plain sight: touching tauri.conf.json invalidates the build script, so enabling bundling forced a 71 s recompile of the tauri stack before bundling even started.

### Icons

One 22,456-byte (21.9 KiB) `icon.icns` was derived from `apps/tauri-app/icons/icon-512.png` with stock macOS tools (`sips` resize to 16→512 px + `iconutil -c icns`) and dropped into each app directory — about one minute, once, for all frameworks. (No 1024 px master existed, so the 512@2x slot reuses the 512 px art; `iconutil` accepts it.) `sips -g pixelWidth` confirms the bundled icns reads back as valid 512 px; Finder icon rendering was not visually audited.

## The three real findings

### 1. No distribution identity was configured

Before the explicit local `codesign` pass, **all seven** final `.app`s — plus cargo-packager's comparison build — carried only `flags=0x20002(adhoc,linker-signed)` on the inner executable: the signature the Apple linker auto-applies to every arm64 binary. There was no bundle/resource seal acceptable to a distribution channel. cargo-bundle has no signing support; Tauri and cargo-packager both support signing when an identity is configured, but no identity was provided in this experiment. `codesign -s - --deep --force <app>` added a local ad-hoc seal and `codesign -v --deep --strict` passed 7/7. That command is useful for this local structural check; it is **not** a substitute for a Developer ID distribution signature.

### 2. The tested built-in DMG paths shared an unreliable flow

tauri-bundler and cargo-packager used a create-dmg path (create a UDRW image → attach → AppleScript/Finder layout → detach → convert to UDZO); cargo-bundle's own `dmg_bundle.rs` has no layout step but ends in the same `hdiutil convert` (source re-check 2026-08-30 — an earlier draft lumped all three together). The retained result table and its explicit `×N` retry markers support two different counts:

- **App/tool paths:** cargo-bundle eventually succeeded for 2 of 6 apps and failed for 4; Tauri failed for its one app; the iced cargo-packager comparison failed. Total: **2 successes and 6 final failures across 8 paths**.
- **Literal executions reconstructed from the table:** cargo-bundle 7 executions/5 failures (egui was tried twice), Tauri 3/3 failures, cargo-packager 3/3 failures. Total: **13 executions and 11 failures**.

The exact command-by-command logs were not retained, so the literal count is a reconstruction from the per-app notes. The `Resource temporarily unavailable` failure reproduced outside the sandbox and is consistent with transient post-detach contention, but no minimized trace proves a `diskimagesiod` root cause. None of the tested flows retried `convert` or fell back automatically (tauri-bundler's vendored create-dmg retries `detach`, not `convert`). The simple `hdiutil create -srcfolder … -format UDZO` path eventually succeeded for all seven apps: **7 app outcomes from 8 literal executions**, because Slint needed one retry. The practical recommendation is to keep that simple fallback; this one-machine result does not justify disabling built-in DMG layout universally. cargo-packager also downloaded a pinned create-dmg script from `raw.githubusercontent.com` on its first DMG build and cached it under `~/Library/Caches/.cargo-packager`, a runtime network dependency observed in this version.

### 3. Gatekeeper exposes the missing trust material

`spctl --assess --type execute` returned `rejected` (exit 3) for all seven locally ad-hoc-signed apps. That distribution-trust result is expected without a Developer ID Application identity and notarization; it is separate from the successful local-launch checks in the table above. Tooling does cover much of the workflow: [Tauri supports configured signing and notarization](https://v2.tauri.app/distribute/sign/macos/), [cargo-packager supports signing/distribution workflows](https://docs.crabnebula.dev/packager/), and [rcodesign](https://docs.rs/apple-codesign/latest/apple_codesign/) is a principal Rust-native cross-platform implementation for signing, notarization and stapling. None can supply Apple programme membership, certificates, identities or credentials. The artifacts here are locally launchable and structurally verified, but they are not generally downloadable/store-ready releases.

## cargo-packager head-to-head (run on iced-app only, since cargo-bundle never failed)

The §1.8 "intended framework-neutral answer" needed **9 LOC** of `Packager.toml` vs cargo-bundle's 4, and cost three debugging rounds where cargo-bundle cost zero:

1. **Real bug found (0.11.8):** if `name` is unset in `Packager.toml`, the CLI's `find_nearset_pkg_name()` calls `std::env::set_current_dir()` on the config *file* path → "Not a directory" I/O error → silently swallowed by `filter_map(.ok())` during auto-detection → surfaces only as `Couldn't detect a valid configuration file or all configurations are disabled!`. (Explicit `-c Packager.toml` exposes the real error.) Workaround: set `name` explicitly. Verified against the vendored source (`cargo-packager-0.11.8/src/cli/config.rs`, `find_nearset_pkg_name` / `detect_configs`).
2. Standalone `Packager.toml` infers nothing from Cargo.toml (the `[package.metadata.packager]` route does): `binaries`, `binaries-dir` must be spelled out, and `out-dir` otherwise defaults to the project root — first successful run dropped `Iced Tasks.app` next to `src/`.
3. Its dmg step failed 3/3 (finding 2 above).

Its `.app` output, once running, was comparable to cargo-bundle's (10,180 KiB
= 9.94 MiB versus 10,144 KiB = 9.91 MiB, both 9.9 MiB at one decimal; same
plist shape, same initial linker-signed state, launches fine). For this one
plain "wrap my binary" macOS comparison, cargo-bundle required less
configuration and debugging. cargo-packager's broader advantages—other
installer formats, configured signing/notarization and updater artifacts—were
outside the credential-free comparison.

## Total time, binary → locally launchable verified DMG

For one framework, cold start (no tools installed, release binary in hand), the observed local path is:
`cargo install cargo-bundle` (87 s) + four-line stanza + icon derivation
(about one minute, amortized) + `cargo bundle --release` (11–37 s) + local
`codesign -s -` (<1 s) + `hdiutil create` (2–13 s) was under four minutes;
the observed marginal time per additional cargo-bundle app was under one
minute. Tauri's path was 201 s for CLI installation plus 96 s for the
config-triggered rebuild and bundle.

For these inputs, producing a locally launchable `.app` and verified `.dmg` was low effort on macOS. Distribution still requires identity, notarization/stapling, channel-specific updater work and equivalent Windows/Linux testing. Tauri, cargo-packager and rcodesign automate parts of that path; they cannot provide credentials. rcodesign's release cadence can be reported directly, but a release gap does not establish that the project is idle or that it is the ecosystem's only possible path.

## Files touched (complete list)

| File | Change |
|---|---|
| `apps/{iced,egui,gpui,xilem,slint,dioxus}-app/Cargo.toml` | +7 lines each: comment + 4-line `[package.metadata.bundle]` stanza |
| `apps/{iced,egui,gpui,xilem,slint,dioxus}-app/icon.icns` | new, 22,456 bytes each, derived from `apps/tauri-app/icons/icon-512.png` (`sips` + `iconutil`) |
| `apps/tauri-app/tauri.conf.json` | `bundle.active`: false → true; +`"targets": ["app", "dmg"]` |
| `apps/iced-app/Packager.toml` | new, 9 config LOC — cargo-packager comparison run only |
| `.gitignore` | +1 line: `dist/` |
| `dist/<framework>/` | output: locally ad-hoc-sealed `.app` + verified `.dmg` per framework (gitignored) |
| `report/data/packaging-results.md` | this file |

No app source code was modified. `measurements/` and other report files untouched.

## Raw numbers

```yaml
# app sizes: du -sk (KiB); dmg sizes: stat -f%z (bytes); times: /usr/bin/time -p wall clock
tools:
  cargo-bundle:   {version: 0.11.0, install_s: 87.3}
  cargo-packager: {version: 0.11.8, install_s: 60.0}
  tauri-cli:      {version: 2.11.4, install_s: 200.7}
apps:
  iced:   {app_kib: 10144, dmg_bytes: 4429035, bundle_s: 17.8, tool: cargo-bundle}
  egui:   {app_kib: 12220, dmg_bytes: 5790820, bundle_s: 18.6, tool: cargo-bundle}
  gpui:   {app_kib: 5140,  dmg_bytes: 2624019, bundle_s: 11.9, tool: cargo-bundle}
  tauri:  {app_kib: 8148,  dmg_bytes: 3132946, bundle_s: 96.4, tool: tauri-bundler, bundle_only_rerun_s: 25.5}
  xilem:  {app_kib: 11648, dmg_bytes: 5021471, bundle_s: 11.0, tool: cargo-bundle}
  slint:  {app_kib: 15036, dmg_bytes: 7753066, bundle_s: 26.0, tool: cargo-bundle}
  dioxus: {app_kib: 5860,  dmg_bytes: 2582475, bundle_s: 37.0, tool: cargo-bundle}
comparison:
  iced_cargo_packager: {app_kib: 10180, dmg: "failed 3/3 (hdiutil convert EAGAIN)", config_loc: 9, launched_ok: true}
dmg_flake:
  error: "hdiutil: convert failed - Resource temporarily unavailable"
  error_provenance: "reconstructed from per-app notes; no raw hdiutil log was retained (flagged 2026-08-30)"
  app_tool_paths: {paths: 8, final_successes: 2, final_failures: 6}
  reconstructed_multi_step_executions: {attempts: 13, successes: 2, failures: 11} # exact command logs not retained
  one_shot_hdiutil_create: {attempts: 8, successes: 7, failures: 1} # Slint needed one retry; all 7 final DMGs shipped this way
signing:
  configured_distribution_identity: false
  pre_seal_state: "linker-signed ad-hoc executable only (all 7 final apps) — no bundle/resource seal"
  local_adhoc_codesign: "OK 7/7 (codesign -v --deep --strict clean); apps still launch post-seal; not a distribution signature"
  spctl_assess: "rejected 7/7, exit 3 — expected: no Developer ID, no notarization attempted"
validation:
  info_plist_required_fields: "OK 7/7 (identifier, name/display name, icon, high-resolution flag)"
  minimum_system_version: "Tauri=10.13; cargo-bundle apps omitted LSMinimumSystemVersion"
  legacy_carbon_key: "present in generated bundles; recorded, not treated as a validation failure"
  codesign_deep_strict: "OK 7/7 after local ad-hoc seal"
  hdiutil_verify: "VALID 7/7"
  finder_icon_visual_audit: false
```

## 2026-08-02 Freya/Vizia extension

Measured on Apple M4 Pro / macOS 26.6 / arm64 with the same cargo-bundle
0.11.0 metadata shape. Both release apps launched before/after a local ad-hoc
seal, passed `codesign -v --deep --strict`, and their one-shot UDZO disk
images passed `hdiutil verify`. `spctl --assess --type execute` rejected both
as expected because no Developer ID identity or notarization was configured.

```yaml
apps:
  freya: {app_kib: 20776, dmg_bytes: 9721367, tool: cargo-bundle, config_loc: 4, launched_ok: true, local_adhoc_seal: true, dmg_verify: true, spctl: rejected}
  vizia: {app_kib: 22276, dmg_bytes: 10223200, tool: cargo-bundle, config_loc: 4, launched_ok: true, local_adhoc_seal: true, dmg_verify: true, spctl: rejected}
commands:
  app: "cargo bundle --release --format osx"
  dmg: "hdiutil create -srcfolder <App> -ov -format UDZO"
built_in_dmg_attempts: "failed for both Freya and Vizia; one bounded hdiutil fallback succeeded first try for each"
```

## 2026-08-03 status note — the Freya/Vizia packaging rows above are stale, Floem is pending

The `2026-08-02 Freya/Vizia extension` block above was measured against Freya
and Vizia crates that were **never retained**. Both frameworks' apps were
re-implemented from scratch on 2026-08-03 (see the `2026-08-03-expansion`
blocks in `shell-text-rows.md`, `iter4-rows.md`, `interactive-rows.md` and
`stack-rows.md`), so:

- The `freya: {app_kib: 20776, dmg_bytes: 9721367}` and
  `vizia: {app_kib: 22276, dmg_bytes: 10223200}` figures describe binaries that
  no longer exist in the tree. Any `.app`/`.dmg` still sitting under `dist/`
  for those two frameworks is stale and must not be quoted as a current
  artifact size.
- The four-line `[package.metadata.bundle]` shape, the `cargo bundle --release
  --format osx` + `hdiutil create … -format UDZO` command pair, the built-in
  DMG-step failure and the local-ad-hoc-seal / `spctl rejected` outcomes are
  *procedural* results and are not invalidated by the re-implementation; only
  the sizes are.
- **No re-measurement was performed on 2026-08-03.** No packaging command was
  run, no bundle was produced, and no size is inferred for the re-implemented
  apps. Re-running the same two commands per app is the whole remaining task.

`floem-app` has **not been packaged at all**. It is pending the next packaging
round, and two floem-specific questions should be answered when it runs:

- the git-pinned dependency set (`floem` + forked `floem-winit` + `understory_*`
  via git) has no bearing on `cargo bundle`'s metadata shape, but it does mean
  the bundled binary is not reproducible from crates.io alone — worth recording
  alongside the artifact;
- floem ships the vger GPU renderer *and* a tiny-skia software fallback plus the
  full Lapce editor core in default features (315 unique crates), so its `.app`
  size is expected to differ materially from the Freya/Vizia Skia-static shape.

```yaml
packaging_status_20260803:
  freya: {rows_above: stale, reason: "apps re-implemented from scratch 2026-08-03; dist/ artifacts predate them", remeasurement: pending_new_cohort}
  vizia: {rows_above: stale, reason: "apps re-implemented from scratch 2026-08-03; dist/ artifacts predate them", remeasurement: pending_new_cohort}
  floem: {rows_above: none, reason: "framework added 2026-08-03; never packaged", remeasurement: pending_new_cohort}
# SUPERSEDED 2026-08-08: all three re-measured with cargo-bundle below; floem packaged for the first time.
```

## 2026-08-08 re-measurement: freya / vizia / floem via cargo-bundle

Measured on the same M4 Pro, macOS 26.6.1, rustc 1.96.1, cargo-bundle 0.11.0,
against the current (2026-08-03 re-implemented) apps' already-built release
binaries. Same procedure as July: `cargo bundle --release --format osx`, launch
check pre/post local ad-hoc seal, `codesign -v --deep --strict`, one-shot
`hdiutil create … -format UDZO`, `hdiutil verify`, `spctl --assess --type
execute`. This resolves the 2026-08-03 status note: the freya/vizia rows are no
longer stale and **floem has now been packaged for the first time**.

| Framework | `.app` | `.dmg` | Launch (pre+post seal) | deep-strict | DMG verify | spctl |
|---|---|---|---|---|---|---|
| freya | 20,760 KiB | 9,720,420 B | OK | OK | VALID | rejected (exit 3) |
| vizia | 22,248 KiB | 10,216,966 B | OK | OK | VALID | rejected (exit 3) |
| floem | 17,144 KiB | 7,154,179 B | OK | OK | VALID | rejected (exit 3) |

Notes: the re-implemented freya/vizia bundles land within 0.1% of the stale
2026-08-02 sizes (20,776→20,760 KiB; 22,276→22,248 KiB) — app-side code is a
rounding error against static Skia. Floem's two open questions from the status
note: (a) recorded — the bundle embeds a binary built from git-pinned
`floem`/`floem-winit`/`understory_*` revs, so the artifact is not reproducible
from crates.io alone; (b) its `.app` is *smaller* than the Skia-static pair
(17.1 vs 20.3/21.7 MiB), not larger — the vger+tiny-skia+Lapce-core stack costs
less binary than static Skia. `cargo bundle` itself took &lt;1 s per app (the
release build no-oped); `hdiutil create` took 4–16 s. Plist required fields OK
3/3. Bundle stanzas and `icon.icns` were already present in all three app dirs.

## 2026-08-08 experiment: `dx bundle` as the framework-neutral packager

Prompted by Nico Burns's RCN Zulip comment that `dx` (dioxus-cli) provides
framework-agnostic packaging. Tested: **dioxus-cli 0.7.10** (installed
`cargo install dioxus-cli --locked`: **137 s** wall, 990 s CPU — between
cargo-packager's 60 s and tauri-cli's 201 s) against all nine non-Tauri todo
apps. Verified upstream: since 0.7.0 dx officially supports any Rust project;
its bundler was historically a `tauri-bundler` wrapper and is a vendored
in-tree rewrite by 0.7.10; macOS signing/notarization
(`APPLE_ID`/`APPLE_API_KEY` envs, hardened runtime, `notarytool`, `stapler`)
exists in the 0.7.10 code; the 0.7 config reference does document the `[bundle.macos]` signing keys, but notarization (`APPLE_*` env vars, notarytool, stapler) is documented nowhere — docs lag code (wording corrected 2026-08-30: the tutorial sentence saying signing is "not provided" is about mobile distribution). Not exercised here (no credentials, by design).

**Result: 9/9 frameworks produced a launching `.app` and a valid `.dmg`.**

| Framework | bundle time¹ | `.app` | `.dmg` | Launch | deep-strict² | DMG verify | spctl |
|---|---|---|---|---|---|---|---|
| iced | 24 s cold / 6 s warm | 10,488 KiB | 4,394,586 B | OK | OK | VALID | rejected |
| egui | 27 s | 12,548 KiB | 5,771,596 B | OK | OK | VALID | rejected |
| gpui | 53 s | 5,320 KiB | 2,611,518 B | OK | OK | VALID | rejected |
| xilem | 28 s | 12,000 KiB | 4,981,586 B | OK | OK | VALID | rejected |
| slint | 43 s | 15,260 KiB | 7,708,195 B | OK | OK | VALID | rejected |
| dioxus | 39 s | 6,072 KiB | 2,566,205 B | OK | OK | VALID | rejected |
| freya | 40 s | 20,912 KiB | 9,731,440 B | OK | OK | VALID | rejected |
| vizia | 24 s | 22,468 KiB | 10,227,760 B | OK | OK | VALID | rejected |
| floem | 50 s | 17,680 KiB | 7,070,837 B | OK | OK | VALID | rejected |

¹ `dx bundle --release` wall time **including a full rebuild**: dx compiles under an ad-hoc `desktop-release` cargo profile (`target/desktop-release/`; `target/dx/<crate>/bundle/` is only the artifact dir) and ignores the existing `target/release` binary — unlike cargo-bundle, whose build step no-ops. The
iced row is split because its timing was captured across the config-discovery
runs (22.9 s compile + ~1 s bundle; 5.7–5.8 s bundle-only reruns).
² After the same local `codesign -s - --deep --force` seal as the July round;
pre-seal, dx output carries only the linker's ad-hoc executable signature
(`flags=0x20002(adhoc,linker-signed)`) — identical initial state to
cargo-bundle/tauri/cargo-packager output.

### Findings vs the July tools

1. **The DMG step just works: 9/9 first-try.** The headline. July's
   create-dmg-style flows failed 6 of 8 tool paths (`hdiutil convert` EAGAIN);
   dx's vendored DMG path succeeded for all nine apps with zero retries
   (2–6 s each; floem 5 s), removing the need for the manual `hdiutil create`
   fallback entirely.
2. **Config cost matches cargo-bundle: 4 LOC — but its own file, discovered
   through sequential errors.** dx ignores `[package.metadata.bundle]` and
   requires `Dioxus.toml` with `[bundle] identifier`, `publisher`, `icon`.
   The CLI demanded `identifier` and `publisher` one error at a time (two
   failed runs to discover); `icon` accepted the existing `icon.icns`
   unchanged. Without `icon` it silently bundles an empty `Resources/`.
3. **Plist quality is slightly better than cargo-bundle's:** dx sets
   `LSMinimumSystemVersion` 10.13 (cargo-bundle omits it) and derives
   name/version from Cargo metadata (`IcedApp`, `0.1.0` — display-name
   configuration exists but was not needed for the test).
4. **The rebuild-in-own-profile cost is real:** ~24–53 s per app vs
   cargo-bundle's no-op, and roughly a duplicate `target/` per app on disk.
   For a repo that already built release binaries, dx is the slower path;
   for a cold CI job the build had to happen anyway.
5. **PATH hazard on this machine:** Deno also installs a `dx` binary;
   `/opt/homebrew/bin/dx` shadowed `~/.cargo/bin/dx`.

```yaml
dx_bundle_20260808:
  tool: {name: dioxus-cli, version: 0.7.10, install_s: 137.1, install_cpu_s: 990.4}
  config: {file: Dioxus.toml, loc: 4, keys: [identifier, publisher, icon], reads_package_metadata_bundle: false, error_guided_iterations: 2}
  outcomes: {bundled: "9/9", launch_post_seal: "9/9", deep_strict_after_local_seal: "9/9", dmg_first_try: "9/9", hdiutil_verify: "9/9 VALID", spctl: "rejected 9/9 exit 3 (expected; no identity)"}
  signing: {configured: false, code_paths_present_0710: [codesign hardened-runtime, notarytool, stapler], docs_state: "0.7 config reference documents [bundle.macos] signing keys; notarization flow/env undocumented — docs lag code (corrected 2026-08-30)"}
  artifacts: "apps/<fw>-app/target/dx/<fw>-app/bundle/macos/macos/ (gitignored); logs in measurements/<fw>-app-dx-bundle.log"
  files_touched: "apps/<fw>-app/Dioxus.toml (new, 4 LOC × 9); no app source modified"
```

**Verdict for §funding-recommendation 5:** on this macOS sample, `dx bundle`
is the strongest framework-neutral candidate measured so far — cargo-bundle's
config economy with a working DMG step and (unexercised but present) signing/
notarization automation, at the cost of a duplicate build profile and a
config format all its own. Windows/Linux formats (.msi/.exe, .deb/.rpm/
.appimage) remain untested here.
