# Verification: build, packaging and platform / render-path findings

Verifier pass, 2026-08-30. Repo read-only. Crate sources read from
`~/.cargo/registry/src/index.crates.io-*/`, `~/.cargo/git/checkouts/`, and (for
the three CLI packagers, which are not vendored as `src/`) from the `.crate`
tarballs in `~/.cargo/registry/cache/`, extracted to
`<local-clone>/src/{cargo-bundle-0.11.0,cargo-packager-0.11.8,tauri-bundler-2.9.4}`.

Repo path prefix for evidence links:
`https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/`

---

## P-1 — DMG creation: built-in decorative DMG paths failed on macOS 26

### 1. CLAIM
- `report/14-packaging-results.md:41-55` — "The built-in decorative DMG paths were unreliable on this machine … Keep a simple `hdiutil create -format UDZO` fallback."
- `report/data/packaging-results.md:64-71` — "cargo-bundle, tauri-bundler, and cargo-packager all used a create-dmg-style path: create a UDRW image → attach → use AppleScript/Finder for layout → detach → convert to UDZO … None of the tested flows retried or fell back automatically."
- `report/data/packaging-results.md:136-141` — `error: "hdiutil: convert failed - Resource temporarily unavailable"`; `iced_cargo_packager: dmg: "failed 3/3 (hdiutil convert EAGAIN)"`.

### 2. EVIDENCE CHECK — partial, with two corrections
- **No raw log of the failure was retained anywhere in the repo.** I grepped the
  entire tree for `hdiutil`, `Resource temporarily unavailable`, `EAGAIN`,
  `Resource busy` across `*.md/*.log/*.txt/*.csv`: the only occurrences of the
  error string are the *reconstruction* in `report/data/packaging-results.md:138`
  itself. `dist/` holds only artifacts; `measurements/` holds only the
  2026-08-08 `dx bundle` logs. The corpus already says the count is a
  reconstruction; it should also say the **error text itself is a
  reconstruction**, not a captured line. Rating for the error string:
  **UNVERIFIED (self-reported, no artifact).**
- **The "AppleScript/Finder layout" characterisation is wrong for cargo-bundle.**
  `cargo-bundle-0.11.0/src/bundle/dmg_bundle.rs` (whole file read, 165 lines)
  contains no AppleScript, no Finder call and no `.DS_Store` layout step. Its
  flow is: `hdiutil create -fs HFS+ -size N` → `hdiutil attach` → plain
  `fs::copy` + `/Applications` symlink → `hdiutil detach` → `hdiutil convert
  -format UDZO -imagekey zlib-level=9`. The "decorative" adjective and the
  AppleScript step apply only to tauri-bundler and cargo-packager (both of which
  really do drive create-dmg with `osascript`).
- **"None of the tested flows retried" is right for `convert`, wrong as a blanket
  statement.** tauri-bundler's vendored script *does* retry — but only `detach`.

### 3. ROOT CAUSE — CONFIRMED per tool (code located); the errno itself UNVERIFIED
| Tool | DMG code | retry on `hdiutil convert`? |
|---|---|---|
| cargo-bundle 0.11.0 | `src/bundle/dmg_bundle.rs:115-132` — single `Command::new("hdiutil").args(["convert", …]).status()`; `if !status.success() { bail!("hdiutil convert failed") }` | **no** |
| tauri-bundler 2.9.4 | `src/bundle/macos/dmg/mod.rs:76-81,114-189` writes and execs the vendored `src/bundle/macos/dmg/bundle_dmg` (a create-dmg fork, `CDMG_VERSION='1.2.1'`). `bundle_dmg:46-64` defines `hdiutil_detach_retry()` (`MAXIMUM_UNMOUNTING_ATTEMPTS`, exponential `sleep $(( 1 * (2 ** n) ))`), wired at lines 508 and 550 — **detach only**. The final compress at `bundle_dmg:555` is a bare `hdiutil convert …` | **no** |
| cargo-packager 0.11.8 | `src/package/dmg/mod.rs:17` downloads create-dmg at runtime from `https://raw.githubusercontent.com/create-dmg/create-dmg/28867ba3563ddef62f55dcf130677103b4296c42/create-dmg` (commit dated 2023-09-04, `CDMG_VERSION='1.1.1'`), caches it, and execs it at :91-189 | **no** |

The corpus's separate claim that cargo-packager has "a runtime network dependency
on `raw.githubusercontent.com`" is **CONFIRMED** at `dmg/mod.rs:17`.

The macOS-26 errno root cause (why `hdiutil convert` returns EAGAIN after a
successful detach) is **UNVERIFIED** — no minimized trace, no Apple-side note found.

### 4. UPSTREAM STATUS
- **No issue in any of the three repos matches this failure class** (searched
  `hdiutil`, `Resource temporarily unavailable`, EAGAIN, `convert failed`,
  `resource busy`, Tahoe / `macOS 26`). Nearest neighbours, all different:
  tauri [#14686](https://github.com/tauri-apps/tauri/issues/14686) (detach
  timeout, open 2025-12-22), tauri [#14500](https://github.com/tauri-apps/tauri/issues/14500)
  (Tahoe DMG icon, cosmetic), cargo-bundle
  [#245](https://github.com/burtonageo/cargo-bundle/issues/245) (unrelated).
  cargo-packager (org is **crabnebula-dev**, not tauri-apps): 0 hits on every keyword.
- **Upstream create-dmg has the retry but not where it matters.**
  [PR #198](https://github.com/create-dmg/create-dmg/pull/198) "Add
  `--hdiutil-retries` and retry all `create` and `detach` operations" merged
  2025-11-17; master is `CDMG_VERSION='1.3.0'` with `hdiutil_retry()` /
  `MAXIMUM_HDIUTIL_RETRIES=5`. Two gaps: (a) it wraps `create` and `detach`, not
  the final `convert`; (b) its retry predicate greps the log for the literal
  string `Resource busy`, so `Resource temporarily unavailable` would not retry
  even if `convert` were wrapped. Related EBUSY reports:
  [#143](https://github.com/create-dmg/create-dmg/issues/143),
  [#190](https://github.com/create-dmg/create-dmg/issues/190),
  [#114](https://github.com/create-dmg/create-dmg/issues/114) — all open.
- Neither tauri-bundler (fork at 1.2.1) nor cargo-packager (pin at 1.1.1, 2023)
  has picked up PR #198.
- All three tested versions are the **current releases** (cargo-bundle 0.11.0
  2026-05-30, tauri-bundler 2.9.4 2026-06-28, cargo-packager 0.11.8 2025-11-27).
  No fix has shipped that we are missing.
- **New, independently verified defect on cargo-bundle master:** its
  `compress_disk_image()` now calls `.output()` and **discards the exit status**
  — no `status.success()` check, no `bail!`. Released 0.11.0 *does* check
  (`dmg_bundle.rs:130-132`). So a future cargo-bundle release will emit a
  missing/corrupt DMG **silently** on exactly this failure. Verified by fetching
  `master/src/bundle/dmg_bundle.rs`.

### 5. FILE?
- **cargo-bundle — YES (high confidence, unrelated to macOS 26 flakiness).**
  Title: *`hdiutil convert` failure is silently ignored on master (exit status
  discarded)*. Body: 0.11.0's `dmg_bundle.rs:127-132` checks
  `status.success()` and bails; master's `compress_disk_image()` calls
  `.output()?` and returns `Ok(())` without inspecting the status, so a failed
  compress yields a success exit with no `.dmg` (or a stale one, since `-ov`
  overwrites). This is a regression relative to 0.11.0. Suggested fix: check the
  status and include `stderr` in the error; optionally add a bounded retry.
- **create-dmg — YES (maybe; low cost, honest framing).** Title: *`--hdiutil-retries`
  does not cover the final `hdiutil convert`, and the retry predicate only matches
  "Resource busy"*. Body: PR #198 wrapped `create`/`detach`; the compress step at
  the end is still bare, and `Resource temporarily unavailable` (EAGAIN) is not
  matched by the `grep -w -c 'Resource busy'` predicate. Report our observation as
  a one-machine data point (macOS 26.5.2 arm64, error text self-reported, no log
  retained) rather than as a reproducer.
- **tauri-bundler — MAYBE.** Title: *vendored `bundle_dmg` is create-dmg 1.2.1;
  1.3.0 adds `--hdiutil-retries`*. Small, factual, easy for a maintainer to
  action. Do not claim a reproducer.
- **cargo-packager — MAYBE, and the stronger ask is the network dependency.**
  Title: *DMG packaging downloads `create-dmg` from raw.githubusercontent.com at
  package time (pinned to a 2023 commit)*. `src/package/dmg/mod.rs:17`. Ask: vendor
  the script (as tauri-bundler does) and refresh to 1.3.0. Offline/air-gapped CI
  cannot build a DMG today.
- **Do NOT file "hdiutil is flaky on macOS 26" anywhere** — we have no retained
  log and no minimized reproducer. Say so in the corpus.

### 6. CORPUS CORRECTIONS
- `report/data/packaging-results.md:66` — drop "use AppleScript/Finder for layout"
  from the cargo-bundle description; cargo-bundle 0.11.0 has no layout step at all.
- Same paragraph — "None of the tested flows retried" → "None retried the
  `convert` step (tauri-bundler's vendored create-dmg retries `detach` only)".
- `report/data/packaging-results.md:138` — mark the error string as
  self-reported/reconstructed, same caveat as the counts.

---

## P-2 — `dx bundle` (dioxus-cli 0.7.10)

Source read: `~/.cargo/registry/src/index.crates.io-*/dioxus-cli-0.7.10/`.

### (a) Ignores `[package.metadata.bundle]`, requires its own `Dioxus.toml`
**CONFIRMED.** `grep -rn 'metadata.bundle\|package.metadata' src/` in
dioxus-cli-0.7.10 returns **zero** hits. Config is loaded only from
`Dioxus.toml`/`dioxus.toml` (`src/workspace.rs:393-470`), with an inline-in-source
override path (`src/config/inline_config.rs`). Our nine `apps/*/Dioxus.toml` are
4 lines each (`[bundle]` + identifier/publisher/icon) — verbatim in the repo.
Upstream: not documented as reading cargo metadata, and **no upstream issue exists**
(searched DioxusLabs/dioxus for `metadata.bundle`, `cargo-bundle`). The 0.7 config
reference (https://dioxuslabs.com/learn/0.7/guides/deploy/config) documents the
`[bundle]` keys and never mentions `package.metadata.bundle`.

### (b) One missing key at a time
**CONFIRMED, exact code:** `src/cli/bundle.rs:170-175`
```rust
if build.config.bundle.identifier.is_none() {
    bail!("\n\nBundle identifier was not provided in `Dioxus.toml`. …");
}
if build.config.bundle.publisher.is_none() {
    bail!("\n\nBundle publisher was not provided in `Dioxus.toml`. …");
}
```
Two sequential `bail!`s: the first returns before the second is evaluated, so
fixing `identifier` is required before `publisher` is even reported. Matches our
"two failed runs to discover". Not reported upstream. (Adjacent-but-different:
[#3785](https://github.com/DioxusLabs/dioxus/issues/3785), closed, about
`files`/`hardened_runtime` defaults.)

### (c) Rebuilds instead of reusing `target/release`
**CONFIRMED, with a wording correction.** Evidence in our own logs:
`measurements/egui-app-dx-bundle.log:2` starts `Compiled [  1/208]: cfg_aliases`
— a full 208-crate rebuild for an app whose `target/release` binary already existed.
Mechanism: `src/platform.rs:306-318` `profile_name()` returns
`"{desktop|wasm|ios|android|server}-{release|dev}"`, i.e. **`desktop-release`**, and
`src/build/request.rs:3039-3058` `profile_args()` injects
`--config profile.desktop-release.inherits="release"` plus
`profile.desktop-release.strip=false`. A distinct cargo profile means a distinct
`target/<profile>/` output dir and fingerprint set → nothing is reused.
*Correction for the corpus:* `report/data/packaging-results.md:269-272` says dx
"compiles into its own `target/dx/<crate>` profile". `target/dx/<crate>/bundle/...`
is where the **bundle artifacts** land (confirmed verbatim in
`measurements/iced-app-dx-bundle.log`); the **compile** goes to
`target/desktop-release/` under an ad-hoc cargo profile. The substance (full
rebuild, duplicate artifacts on disk) is right; the path is not.
Upstream: no issue about reusing an existing release binary. Nearest:
[#3600](https://github.com/DioxusLabs/dioxus/issues/3600) (closed — `dx bundle`
does not respect `--profile`).

### (d) Docs lag code on signing — **PARTIALLY CONFIRMED, corpus is overstated**
Code side **CONFIRMED**: `src/bundler/macos.rs:855-895` implements
`xcrun notarytool submit … --wait` and `xcrun stapler staple`, reading
`APPLE_ID`/`APPLE_PASSWORD`/`APPLE_TEAM_ID` or
`APPLE_API_KEY`/`APPLE_API_ISSUER`/`APPLE_API_KEY_PATH`; `codesign` with
`--options runtime` gated on `hardened_runtime` (`src/config/bundle.rs:189-215`).
Merged upstream work confirms it is live:
[PR #5527](https://github.com/DioxusLabs/dioxus/pull/5527) and its 0.7 backport
[PR #5528](https://github.com/DioxusLabs/dioxus/pull/5528) (both 2026-05-04,
"Staple notarization ticket to `.app` not `.zip`", `hardened_runtime` default fix)
— so 0.7.10 contains them.

Docs side is **weaker than the corpus states**. The actual sentence, from
https://dioxuslabs.com/learn/0.7/tutorial/bundle/ (the URL in our brief 404s):

> "When distributing mobile apps, you _are required_ to sign and notarize your
> apps. Currently, Dioxus doesn't provide built-in utilities for this, so you'll
> need to figure out signing by reading 3rd-party documentation."

Its grammatical subject is **mobile** distribution, not macOS desktop — though it
sits directly after a macOS sentence and the page then defers to Tauri's signing
docs for every platform. And the **config reference page does document macOS
signing** (`signing_identity`, `provider_short_name`, `hardened_runtime`,
`entitlements` under `[bundle.macos]`). What is genuinely undocumented anywhere
is **notarization**: the `APPLE_*` env vars, `notarytool`, and `stapler` appear
in no doc page.

### 5. FILE?
- **YES — docs issue, DioxusLabs/dioxus.** Title: *0.7 docs: bundling tutorial says
  signing isn't provided, but `dx bundle` implements codesign + notarytool +
  stapler*. Body: quote the tutorial sentence; point at
  `packages/cli/src/bundler/macos.rs` (`codesign --force --sign`, `xcrun notarytool
  submit --wait`, `xcrun stapler staple`) and at PRs #5527/#5528; note that the
  config reference documents `[bundle.macos]` signing keys but that the `APPLE_*`
  notarization env vars are documented nowhere. Ask: one paragraph on the config
  reference page listing the six env vars and the two credential modes.
- **YES — small UX issue.** Title: *`dx bundle` reports missing `[bundle]` keys one
  at a time*. `src/cli/bundle.rs:170-175`; ask for a single collected error
  (identifier + publisher + optionally icon).
- **MAYBE — feature request.** Title: *`dx bundle` cannot package an
  already-built `target/release` binary*. Frame it as the measured cost
  (24–53 s and a duplicate `target/` per app across nine apps on our M4 Pro),
  and note the ad-hoc `desktop-release` profile as the mechanism. Weakest of the
  three; for a cold CI job the build had to happen anyway.
- **NO** for (a) on its own — it is a design choice, and `dx` supporting a
  competitor's config key is a big ask. Fold it into the docs issue at most.

### Bonus, verified while here (worth keeping in the corpus)
dx's DMG step is `hdiutil create … -format UDZO` (`src/bundler/macos.rs`, the
documented step 3 of the `.dmg` flow; the log line "Creating DMG at …" is in every
`measurements/*-dx-bundle.log`). That is **literally the fallback our July round
recommended** — which fully explains "9/9 first try, zero retries" and is a
better framing than "dx's DMG path is more robust": dx simply never runs the
attach/AppleScript/detach/convert dance.

---

## P-3 — Windows build failures

### (a) tauri: `icons/icon.ico` missing → **NOT-A-BUG (our repo's omission)**
Verbatim from
`measurements/reruns/20260808-ten-framework-tri-platform/windows/runs/tauri-app/default/build.log`:
```
  package.metadata does not exist
  `icons/icon.ico` not found; required for generating a Windows Resource file during tauri-build
```
Root cause **CONFIRMED**: `tauri-build-2.6.3/src/lib.rs:604-675` —
under `if target_triple.contains("windows")`, it picks
`config.bundle.icon.iter().find(|i| i.ends_with(".ico")).unwrap_or("icons/icon.ico")`
and then:
```rust
if window_icon_path.exists() { res.set_icon_with_id(…, "32512"); }
else { return Err(anyhow!(format!(
    "`{}` not found; required for generating a Windows Resource file during tauri-build",
    window_icon_path.display()))); }
```
Our `apps/tauri-app/tauri.conf.json` lists only `icons/32x32.png`,
`icons/128x128.png`, `icons/icon.icns`; `apps/tauri-app/icons/` contains
`128x128.png`, `32x32.png`, `icon-512.png`, `icon.icns` and **no `.ico`**. The
standard `cargo tauri init` / `tauri icon` output does include `icons/icon.ico`;
this app was hand-assembled on macOS without one.
**FILE? NO.** Honest verdict: our repo omitted the file, the error names the
missing path, and the fix is one `.ico`. The only arguable upstream point — that a
purely cosmetic Windows resource is a hard build-script failure rather than a
warning — is a deliberate tauri design choice and not worth a volunteer's time.
*Corpus wording:* report/21 Headline 1 groups this with freya/vizia as
"infrastructure, not Rust". True as a category, but it should say plainly that
**this one is our own missing file**, not a tauri defect. Currently the report only
hints at it ("fix would be one .ico (sources deliberately not patched)").

### (b) freya: prebuilt Skia download — **stated cause REFUTED**
Verbatim from `runs/freya-app/default/build.log` (identical in all 9 freya apps):
```
  TRYING TO DOWNLOAD AND INSTALL SKIA BINARIES: 0.98.1/b5756d8613bf27909a64-x86_64-pc-windows-msvc-gl-jpegd-jpege-svg-textlayout-vulkan-webpd-webpe
    FROM: https://github.com/marc2332/rust-skia/releases/download/0.98.1/skia-binaries-b5756d8613bf27909a64-x86_64-pc-windows-msvc-gl-jpegd-jpege-svg-textlayout-vulkan-webpd-webpe.tar.gz
  DOWNLOAD AND INSTALL FAILED: curl error code: "3"
  curl stderr: "curl: (3) URL using bad/illegal format or missing URL\r\n"
  STARTING A FULL BUILD
  …
  thread 'main' panicked at …\freya-skia-bindings-0.98.1\build_support\platform\windows.rs:40:13:
  Unable to locate LLVM installation
```

**The corpus's stated cause ("malformed/missing URL for the Windows feature set")
is false.** I fetched that exact URL: **HTTP 200**, and downloaded the asset with
the *identical* curl flag order the build script uses
(`curl -L -f -sS -C - --create-dirs --output <path> <url>`) — 22,315,880 bytes.
The 0.98.1 release on marc2332/rust-skia has 18 assets including four
`x86_64-pc-windows-msvc` tarballs. The Windows prebuilt exists.

Root cause of curl(3) is **UNVERIFIED and machine-local**. Facts that bound it:
- The URL is constructed correctly (`build_support/binary_cache/env.rs:22-25`
  template, `binaries.rs:104-110` substitution) and printed correctly.
- `-f` means a 404 would be exit **22**, not 3. So it is not a missing asset.
- On the *same machine, same downloader code*, `skia-bindings 0.93.1` (vizia)
  **successfully** downloaded its Windows prebuilt (the vizia link line consumes
  `…\build\skia-bindings-c96378ee1ed1aded\out\skia\skia.lib` etc.). So the
  downloader works there.
- Leading hypothesis: **Windows MAX_PATH**. `utils.rs:51-60` writes to
  `OUT_DIR/.cache/<filename>`. For freya that path is **272 characters**
  (`…\apps\freya-app\target\release\build\freya-skia-bindings-97e6847842ab645d\out\.cache\`
  = 158 + a 113-char filename); for vizia's shorter feature set it is ~231.
  272 > 260. Against this hypothesis: curl's own sanitizer prints
  `curl: (3) bad output filename`, not the generic URL-malformat text we captured.
  So it is **suggestive, not proven**.
- Not recorded in `environment.txt`: `curl --version`, PATH, proxy vars,
  `SKIA_BINARIES_URL`. Any of those could explain it.

**FILE? NO — not as stated.** We would be filing "your Windows binaries are
missing" against a repo whose Windows binaries are present and downloadable.
Before anything is filed, one Windows re-run is needed:
`CARGO_TARGET_DIR=C:\t cargo build --release` in `apps/freya-app`, plus
`curl --version` and `echo %SKIA_BINARIES_URL%`. If a short target dir fixes it,
*then* there is a real, narrow issue for the freya fork / rust-skia: "prebuilt
download fails when OUT_DIR is deep because the 113-char cache filename pushes the
`--output` path past MAX_PATH". Note also that `freya-skia-bindings` is now at
**0.100.0** and freya `main` pins `freya-skia-safe 0.99.1`, so 0.98.1 may be moot.
(marc2332/rust-skia has **issues disabled**; anything would go to marc2332/freya.)
Secondary, genuinely defensible observation if we do file: on download failure the
build script falls through to a **full source build** and then panics
`Unable to locate LLVM installation` — the download error becomes a confusing
"install LLVM" message. Asking for a hard failure (or `FORCE_SKIA_BINARIES_DOWNLOAD`
guidance in the error) is reasonable.

**CORPUS CORRECTION (important):** report/21 Headline 1 and its table both say
freya "can't download its prebuilt Skia — curl(3), malformed/missing URL for the
Windows feature set". Replace with: "the prebuilt exists and downloads from
another machine; on this machine curl exited 3 for reasons we did not isolate,
and the source fallback then demanded LLVM."

### (c) vizia: LNK1120 / 5 unresolved `__std_*` — **CONFIRMED, toolchain mismatch**
Verbatim from `runs/vizia-app/default/build.log` (link.exe 14.34.31933):
```
skunicode_icu.lib(icu.SkLoadICU.obj) : error LNK2019: unresolved external symbol __std_find_last_trivial_2 …
skia.lib(core.SkStroke.obj)          : error LNK2019: unresolved external symbol __std_min_element_f …
skia.lib(core.SkStroke.obj)          : error LNK2019: unresolved external symbol __std_max_element_f …
skia.lib(core.SkGlyph.obj)           : error LNK2019: unresolved external symbol __std_minmax_element_f …
skia.lib(skia.SkSLErrorReporter.obj) : error LNK2019: unresolved external symbol __std_search_1 …
vizia_app.exe : fatal error LNK1120: 5 unresolved externals
```
These are MSVC-STL vectorized-algorithm helpers that live in the STL import lib
and were added **after** toolset 14.34. rust-skia's prebuilt Windows `skia.lib`
is produced by `.github/workflows/windows-binaries.yaml` on `runs-on:
windows-2022`, whose VS is well past 14.34. So: prebuilt built with a newer STL,
linked against an older STL. **Root cause CONFIRMED by symbol identity; it is a
C++ ABI/STL version mismatch, not a Rust or vizia code defect.**

Upstream status:
- **rust-skia has no issue for this** (searched `__std_`, LNK2019, LNK1120,
  "unresolved external", toolset, "Visual Studio 2022"). The failure class is
  well documented elsewhere (Exiv2 #2429, vcpkg #42564, actions/runner-images #6091).
- **rust-skia documents no minimum MSVC toolset.** Its README's Windows section
  says only "Install Visual Studio 2022 Build Tools … Desktop Development with
  C++ workload", and that is in the *build-from-source* section; the prebuilt
  table lists target triples only. This is an **undocumented implicit requirement**.
- **vizia `main` has already moved past 0.93**: workspace `skia-safe = "0.99"`,
  with an explicit `cfg(target_os = "windows")` section
  (`features = ["textlayout","svg","d3d","vulkan","gl"]`). Published **vizia
  0.4.0** (still the latest release, 2026-04-23) pins `skia-safe ^0.93.1` and has
  **no Windows target section at all**. Current skia-safe on crates.io: 0.99.0.
- Our own caveat stands: the machine's default 14.44 toolset was an incomplete
  stub, so the 14.34 pin was forced. A complete 14.4x toolset would very likely
  link fine.

**FILE? YES, one small docs issue; NOT a vizia bug report.**
- **rust-skia/rust-skia — docs.** Title: *Document the minimum MSVC toolset
  required to link the prebuilt Windows binaries*. Body: prebuilts are built on
  `windows-2022`; linking them under toolset 14.34.31933 fails LNK1120 on five
  `__std_*` STL helpers (list them); the README states no minimum. Ask: one line
  in the prebuilt-binaries section, e.g. "Windows prebuilts require VS 2022
  toolset ≥ 14.3x (built on `windows-2022`); older toolsets must build from
  source or use a matching STL." Cheap, factual, useful.
- **vizia — NO issue needed** (main already on 0.99). At most a comment on any
  existing "when is the next release" thread; we found none, so skip.
- **Corpus wording:** report/21's verdict "no cargo knob exists; needs a complete
  newer toolset or source Skia" is accurate. Add that **vizia main is already on
  skia-safe 0.99**, so this is a released-version-lag issue, not a dead end.

### (d) `FailedToCreateSurfaceForAnyBackend` — CONFIRMED symptom; two separate findings
Verbatim, from `runs/floem-app/default/app-stderr.log`:
```
thread 'main' panicked at …\floem-ab9be4e01bb293da\778bb5f\src\app\handle.rs:100:71:
called `Result::unwrap()` on an `Err` value: SurfaceCreationError(CreateSurfaceError { inner: Hal(FailedToCreateSurfaceForAnyBackend({})) })
```
and from `runs/xilem-*/default/app-stderr.log`:
```
thread 'main' panicked at …\masonry_winit-0.4.0\src\event_loop_runner.rs:971:6:
called `Result::unwrap()` on an `Err` value: WgpuCreateSurfaceError(CreateSurfaceError { inner: Hal(FailedToCreateSurfaceForAnyBackend({})) })
```
and from `runs/egui-*/default/app-stderr.log` — **no panic**, a clean:
```
Error: Wgpu(CreateSurfaceError(CreateSurfaceError { inner: Hal(FailedToCreateSurfaceForAnyBackend({})) }))
```

**Versions differ across the four frameworks** (from each app's Cargo.lock):
iced 0.14.0 → wgpu **27.0.1**; floem (git 778bb5f) → wgpu **27.0.1**;
xilem/masonry_winit 0.4.0 → wgpu **26.0.1**; egui/eframe → wgpu **29.0.4**.
So iced and floem shipped the **same wgpu** and behaved oppositely (iced 0/8
failures, floem 6/6) in the same sessions — **the wgpu version is not the
discriminator**, which undercuts any "framework X uses a bad wgpu" story.

**Mechanism, read from `wgpu-core-27.0.3/src/instance.rs:205-233`:**
```rust
for (backend, instance) in &self.instance_per_backend {
    match unsafe { instance.as_ref().create_surface(display_handle, window_handle) } {
        Ok(raw)  => { surface_per_backend.insert(*backend, raw); }
        Err(err) => { log::debug!(…); errors.insert(*backend, err); }
    }
}
if surface_per_backend.is_empty() {
    Err(CreateSurfaceError::FailedToCreateSurfaceForAnyBackend(errors))
```
It fans out to **all** enabled backends and fails only if all fail — it is not
"tries Vulkan, gives up". Critically, **our error map is printed as `{}` — empty**.
An empty map means the loop body never ran, i.e. `instance_per_backend` was
**empty**: no backend HAL instance existed at all. `Instance::new`
(`instance.rs:94-149`) adds backends via `try_add_hal`, which on failure only
`log::debug!`s and moves on. So the observed error is *not* "surface creation
failed"; it is "the wgpu Instance ended up with zero backends", reported through
a message that names surfaces. That also explains why `WGPU_BACKEND=dx12` (which
narrows the requested set) rescued 14/14: whatever poisons the full-set
initialisation does not occur for DX12 alone.

Root-cause rating: symptom **CONFIRMED**; the underlying AMD-driver/instance-init
cause is **UNVERIFIED** — `RUST_LOG=wgpu_core=debug` was not captured, and that is
exactly the log that holds the per-backend `InstanceError`.

Upstream status: **nothing reported** for AMD Radeon 890M / driver 32.0.13058.2 /
intermittent surface creation in gfx-rs/wgpu (searched
`FailedToCreateSurfaceForAnyBackend`, `CreateSurfaceError`, `890M`,
`vkCreateWin32SurfaceKHR`, AMD + surface). Adjacent open issues: 
[#6329](https://github.com/gfx-rs/wgpu/issues/6329) (AMD Vulkan Windows driver
access violation in pipeline creation), 
[#7241](https://github.com/gfx-rs/wgpu/issues/7241) (driver blacklist for
uncatchable init faults). Current wgpu: 30.0.1.

**FILE? — three, in descending confidence:**
1. **linebender/xilem (masonry_winit) — YES.** Title: *masonry_winit `.unwrap()`s
   surface creation; a driver-side failure aborts the process instead of surfacing
   an error*. Pointer: `masonry_winit-0.4.0/src/event_loop_runner.rs:956-971`
   (`pollster::block_on(render_cx.create_surface(...)).unwrap()`). Contrast with
   eframe, which returns the same error and exits 1 cleanly. Ask: propagate to
   the app, or at minimum panic with a message that names the environment
   variable that fixes it. Our repro: Windows 11 25H2, Radeon 890M 32.0.13058.2,
   4/8 xilem apps, `WGPU_BACKEND=dx12` rescues 4/4.
2. **lapce/floem — YES.** Title: *`rx.recv().unwrap().unwrap()` discards
   `GpuResourceError`*. Pointer: `src/app/handle.rs:100`. floem already models the
   failure properly — `renderer/src/gpu_resources.rs:65-72` returns
   `GpuResourceError::SurfaceCreationError` — and then throws it away one layer up.
   Cheap fix, real payoff (6/6 of our floem apps died on this on the default env).
3. **gfx-rs/wgpu — MAYBE (diagnostics).** Title: *`FailedToCreateSurfaceForAnyBackend({})`
   with an empty error map is unactionable when `instance_per_backend` is empty*.
   Ask: distinguish "no backend instance could be created" from "every backend's
   surface creation failed", and/or include the requested vs. actually-initialised
   backend sets in the message. **Capture `RUST_LOG=wgpu_core=debug` on the
   Windows machine before filing** — a maintainer will ask for it immediately.
   Do **not** file "AMD 890M Vulkan is flaky": one machine, no driver-level trace.

**Corpus wording:** report/21 Headline 2 says the error comes "from wgpu's Vulkan
surface creation". Based on the empty error map that is probably wrong — it is
more likely instance initialisation, not surface creation, and no Vulkan-specific
error was ever recorded. Soften to "from wgpu's surface-creation path (the
per-backend error map came back empty, so which stage failed was not captured)".

### (e) peek apps' `core-foundation` / `objc2` failures on Windows — **OUR BUG**
**NOT-A-BUG upstream. CONFIRMED by reading our own manifests:**
- `apps/gpui-peek/Cargo.toml` — `objc`, `block`, `gpui_media`, `core-video`,
  `core-foundation = "=0.10.0"`, `dispatch` are all in the plain `[dependencies]`
  table, **not** under `[target.'cfg(target_os = "macos")'.dependencies]`. Only
  `nokhwa` was target-gated for the Windows campaign.
- `apps/dioxus-peek/Cargo.toml` — `objc2-foundation = { version = "0.3", … }`
  unconditional.
- `apps/floem-peek/Cargo.toml` — `objc2 = "0.6"` unconditional
  (comment: "Verification-only … resolve the NSWindow windowNumber").
- `apps/floem-babel/Cargo.toml:27` — `objc2 = "0.6"` unconditional.
No framework crate is dragging objc2 onto Windows; **we are**. The fix is four
`[target.'cfg(target_os = "macos")'.dependencies]` moves plus `#[cfg]` on the
call sites.
**FILE? NO.** And report/21's footnote 2 ("something in the peek dependency graph
still drags Apple-only objc2 onto Windows") and Headline 1's "failed on Apple-only
dependencies" should be corrected to name our manifests as the cause. The
*conclusion* — gpui-peek has no Windows camera path — may still hold for other
reasons (gpui's `surface()` takes a CVPixelBuffer), but the E0433 we observed is
our packaging of the app, not a platform verdict.

---

## P-4 — Linux render path

### (a) iced 0.14 on lavapipe — **CONFIRMED, and ALREADY REPORTED upstream**
Verbatim, `linux-results/iced-app-run.log`:
```
thread 'main' panicked at …/wgpu-27.0.1/src/backend/wgpu_core.rs:1058:30:
wgpu error: Validation Error
Caused by:
  In Device::create_shader_module, label = 'iced_wgpu.quad.solid.shader'
Shader validation error: Function [1] 'unpack_u32' is invalid
 10 │     let rg: vec2<f32> = unpack2x16float(data.x);
    = Shader requires capability Capabilities(SHADER_FLOAT16_IN_FLOAT32)
```
Mechanism **CONFIRMED**: iced's fallback is type-driven and fires only when the
primary compositor's constructor returns `None`
(`iced_renderer-0.14.0/src/fallback.rs`, `Compositor::new` → `A::new(...)` then
`B::new(...)`). The wgpu **validation** error above is delivered through wgpu's
uncaptured-error handler and **panics**; it never becomes an `Err`/`None` that
`fallback::Compositor` could observe. So `ICED_BACKEND=tiny-skia` works while
automatic fallback cannot.

Upstream: **[iced#3265](https://github.com/iced-rs/iced/issues/3265)** — open,
2026-02-25, labeled `bug`, titled "linux + vulkan crashes on old gpus `Shader
requires capability Capabilities(SHADER_FLOAT16_IN_FLOAT32)`", body explicitly
asks for the tiny-skia fallback. History: [PR #3164](https://github.com/iced-rs/iced/pull/3164)
(request `SHADER_F16`) merged 2025-12-28, i.e. **after** 0.14.0 (2025-12-07);
[PR #3183](https://github.com/iced-rs/iced/pull/3183) (only request it when
available) merged 2026-01-19 and a maintainer suspects it re-broke #3265.
Fallback-chain work is open and unmerged:
[#3011](https://github.com/iced-rs/iced/pull/3011),
[#3365](https://github.com/iced-rs/iced/pull/3365).
**FILE? NO — already tracked.** Optional: add a comment to #3265 with our exact
environment (Mesa 22.3.6 lavapipe / LLVM 15.0.6, arm64 Debian 12, wgpu 27.0.1,
iced 0.14.0) and the verbatim panic, since #3265's reporter tested master and we
have the released 0.14.0 data point.

### (b) xilem/vello on lavapipe — **CONFIRMED; Mesa's bug, do not file at linebender**
Verbatim, `linux-results/xilem-app-run.log`:
```
LLVM ERROR: Cannot select: 0xfffed87751e8: v4f32 = truncate 0xfffed8706890
  0xfffed8706890: v4i32,ch = CopyFromReg 0xfffed82902a8, Register:v4i32 %72
    0xfffed876d2b0: v4i32 = Register %72
In function: cs_co_variant
```
That is an LLVM instruction-selection failure inside lavapipe's JIT while
compiling a compute shader (`LLVM ERROR:` calls `abort()` — genuinely
uncatchable from Rust, so "uncatchable abort" is accurate). **Root cause is
Mesa/LLVM 15**, not vello.
Upstream: nothing in linebender/vello or linebender/xilem mentions
lavapipe/llvmpipe crashes (lavapipe appears there only as a CI *tool*). Mesa
GitLab has the same class but not this instance:
[mesa#7331](https://gitlab.freedesktop.org/mesa/mesa/-/work_items/7331) (open
since 2022, llvmpipe `glDispatchCompute` crash on 22.2),
[mesa#15489](https://gitlab.freedesktop.org/mesa/mesa/-/work_items/15489) (open,
LLVM 22 SelectionDAG crash on a lavapipe compute shader),
[mesa#13865](https://gitlab.freedesktop.org/mesa/mesa/-/work_items/13865) (closed,
a wgpu/iced app segfaulting through lavapipe under LLVM 20).
**FILE? NO for a bug.** Our Mesa is 22.3.6 / LLVM 15.0.6, both years out of
date; a Mesa maintainer would close it as "test a current Mesa". **MAYBE** a
one-line vello docs PR: its README says "Vello needs a GPU with support for
compute shaders" and defers platform support to wgpu, which by its own criterion
puts lavapipe in scope; a sentence noting that software Vulkan (lavapipe) is not
tested and may abort inside the driver's JIT would be honest and cheap.
Alternatively add our trace to mesa#7331. Lowest priority item in this report.

### (c) gpui 0.2.2 X11 depth-32 window vs depth-24 PutImage — **CONFIRMED end to end**
Evidence in the repo (`linux/probes/gpui-epoll-probe2.sh`, artifacts
`linux-results/gpui-epoll-probe2-*`):
- `gpui-epoll-probe2-xwininfo.txt`: window `0x200001 "Tasks (gpui)"`,
  **Depth: 32**, Visual 0x40, TrueColor, `Map State: IsViewable`.
- `gpui-epoll-probe2-run.txt`: `PutImage submissions on X fd 8: 51`;
  `X ERROR packets received on fd 8: 51 / of which BadMatch (code 8): 51`;
  screenshot `mean=0 stddev=0` (pure black); `app still alive: yes`.
- I decoded the retained PutImage request myself:
  `writev(8, "H\2\0\0\7\260\4\0\1\0 \0\5\0 \0\340\1\200\2\0\0\0\0\0\30\0\0", …)`
  → opcode `0x48` PutImage, format 2 (ZPixmap), BIG-REQUESTS length,
  drawable `0x00200001` (the window), gc `0x00200005`, 480×640 at (0,0),
  left-pad 0, **depth byte = `\30` octal = 24**. The error reply
  `"\0\10\352\0\5\0 \0\0\0H…"` is error class 0, code `\10`=8 (**BadMatch**),
  bad resource `0x00200005`, major opcode `H`=72 (PutImage). Depth-24 blit onto a
  depth-32 window. **The corpus's account is exactly right.**

gpui's side, **CONFIRMED in source** (`gpui-0.2.2/src/platform/linux/x11/window.rs`):
```rust
let visual_set = find_visuals(xcb, x_screen_index);
let visual = match visual_set.transparent {           // :405-411
    Some(visual) => visual,
    None => { log::warn!("Unable to find a transparent visual",); visual_set.inherit }
};
…
xcb.create_window(visual.depth, x_window, visual_set.root, …)   // :464
```
`find_visuals` (:172-228) classifies any visual whose alpha mask is non-zero as
`transparent`; gpui then prefers it **unconditionally**, with no fallback when
the presentation path cannot use it and no way to opt out. The X errors are
dropped: the event pump's catch-all arm (`client.rs:586-592`) pushes unknown
events into the queue and `handle_event`'s `_ => {}` (`client.rs:1262`)
discards them; only `ConnectionError`s are logged (`client.rs:595`). Note the
BadMatch errors arrive on gpui's *own* X connection (fd 8 — the one in gpui's
polled epoll set), because Mesa's WSI presents over the connection the app hands
it, so gpui is the process that sees and silently drops them.

**Important correction to our own framing:** gpui 0.2.2 does **not** use wgpu.
It renders through **blade-graphics / blade-util / ash** (verified in
`apps/gpui-app/Cargo.lock`: `blade-graphics`, `ash`, no `wgpu`). Every recent Zed
issue about software-Vulkan adapter selection is filed against the newer
`gpui_wgpu` crate, which is a different backend. Any issue we file must say
"gpui 0.2.2, Blade/Vulkan (ash) X11 backend" or it will be triaged onto the wrong
code path.

Upstream: **not reported.** `"BadMatch"` across zed-industries/zed returns one
issue ([#50184](https://github.com/zed-industries/zed/issues/50184), closed —
Wayland, Intel HD 4000); `X11 PutImage` returns none; nothing about a
32-bit-depth/ARGB visual against a 24-bit software present path. The adjacent
active work ([#63327](https://github.com/zed-industries/zed/issues/63327),
[PR #63346](https://github.com/zed-industries/zed/pull/63346),
[PR #63313](https://github.com/zed-industries/zed/pull/63313)) is all `gpui_wgpu`
adapter selection, not X11 visuals. gpui on crates.io is still 0.2.2.

**FILE? YES — zed-industries/zed, one issue, high quality.**
Title: *gpui 0.2.2 X11: unconditional depth-32 ARGB visual + silently swallowed X
errors ⇒ live black window under software Vulkan*.
Body sketch: environment (arm64 Debian 12 container, Xvfb 1280x800x24, Mesa
22.3.6 lavapipe / LLVM 15.0.6, gpui 0.2.2 with `runtime_shaders`); symptom
(process alive, window mapped `IsViewable`, zero frames, no diagnostic, screenshot
mean=0); proof (51 PutImage → 51 BadMatch in a 6 s strace window; decoded depth
byte = 24 against a depth-32 window; a `vkcube` control on the default depth-24
visual renders fine, `linux-results/vkcube-check.png`); root-cause pointers
`window.rs:405-411` and `window.rs:464`, plus the dropped `Event::Error` at
`client.rs:1262`. Two asks, either of which fixes the class: (1) fall back to the
root/inherit visual when the transparent visual is unusable, or expose an opt-out;
(2) log X protocol errors instead of discarding them — one `log::warn!` would have
turned a silent black window into a one-line diagnosis. Evidence links:
`linux/probes/gpui-epoll-probe2.sh`, `linux-results/gpui-epoll-probe2-run.txt`,
`linux-results/gpui-epoll-probe2-xwininfo.txt`.
Caveat to state in the issue: headless Xvfb + software Vulkan; we did not test a
real GPU or a compositing WM, where an ARGB visual is normally the right choice.

### (d) gpui's one `-dev` package at link time — **informational, resolved**
`linux-results/gpui-app-build-linkfail.log` (last line):
```
/usr/bin/ld: cannot find -lxkbcommon-x11: No such file or directory
```
So the package is **`libxkbcommon-x11-dev`**. Not a defect — gpui's X11 feature
genuinely needs it. Worth naming explicitly in `report/18` and
`report/20-how-to-build.md` instead of "one -dev package".

---

## P-5 — "notify-rust toasts WORK from a bare unsigned exe" — **OVERSTATED**

CLAIM: `report/21-windows-reality-results.md:145` — "notify-rust toasts WORK from
a bare unsigned exe: dioxus-tray logged `[tray-notes] notification posted via
notify-rust` — directly refuting WINDOWS-RUN.md risk #4 (toasts 'silently absent'
without an AUMID/Start-Menu shortcut)".

EVIDENCE CHECK — the log line exists and is exactly one line:
`measurements/reruns/.../windows/selftests/dioxus-tray.log:4`. Its source is
`apps/dioxus-tray/src/main.rs:311-317`:
```rust
match notify_rust::Notification::new().summary("Tray Notes").body("Note saved").show() {
    Ok(_) => println!("[tray-notes] notification posted via notify-rust"),
```
So the line proves exactly one thing: **`show()` returned `Ok`**. It does not
prove a banner was drawn. Corroborating absences: `runs/dioxus-tray/default/`
`app-stdout.log` and `app-stderr.log` are **both empty**; `tray-shots/` contains
only `iced-tray.png` — there is **no screenshot of a toast**, and no operator note
recording one. Windows notification delivery is also gated by Focus Assist and
per-app notification settings, neither of which was recorded.

Mechanism note: notify-rust 4.18.0 → `tauri-winrt-notification` 0.7.3 (both in
`apps/dioxus-tray/Cargo.lock`), whose Windows backend defaults to the PowerShell
AUMID when the caller sets none, and returns `Ok` once the WinRT call is accepted.
That is a plausible reason a toast *could* appear without our own Start-Menu
shortcut — but it is inference, not our evidence.

**Rating: WEAK / OVERSTATED.** The defensible claim is: *"the notify-rust call
path is reachable and returns `Ok` from a bare unsigned, unbundled exe on Windows
11 — unlike macOS, where the unbundled binary is bundle-id gated. Whether a banner
was rendered was not observed."* WINDOWS-RUN.md risk #4 predicted silent absence;
we refuted the *error* half of it, not the *visibility* half.
**FILE? NO** (nothing upstream here). **Corpus correction: required** — rewrite
report/21 Headline 3's toast sentence and `report/data/windows-rows.md:464`
("posted a real Windows toast" → "the notify-rust call succeeded; no banner was
observed or captured").

---

## P-6 — Other upstream-attributable build/packaging/platform defects found while verifying

Three of these are strong and were **not** in the finding list I was given.

### P-6.1 cargo-bundle 0.11.0 `--format msi`: hyphenated binary names are rejected — **CONFIRMED, FILE**
Verbatim (`.../windows/packaging/logs/iced-app-cargo-bundle.log`), identical for
all 7 buildable apps (0/10 in `packaging/results.csv`):
```
warning: MSI bundle support is still experimental.
    Bundling Iced Tasks.msi
error: Failed to generate Component table
  Caused by: "iced-app.exe" is not a valid value for column "KeyPath"
```
Root cause **CONFIRMED**, `cargo-bundle-0.11.0/src/bundle/msi_bundle.rs`:
- `:499` declares `msi::Column::build("KeyPath").nullable().id_string(72)` — the
  MSI **Identifier** category.
- `:315` `directory.files.push(resource.filename.clone())` — raw filenames.
- `:512` `msi::Value::Str(directory.files[0].clone())` is written into `KeyPath`.
- `:630` the same raw filename is also used as the `File` table's primary key
  (`:602`, also `id_string(72)`), so that table would fail next.
MSI `Identifier` permits letters, digits, `_` and `.` and must start with a letter
or `_` — **`-` is illegal**. Cargo's default binary name for a package called
`iced-app` is `iced-app.exe`, so **every hyphenated Rust crate hits this**, on
every platform, for every app. That is why it was 7/7.
**FILE? YES — burtonageo/cargo-bundle.** Title: *`cargo bundle --format msi` fails
for any binary whose name contains a hyphen ("… is not a valid value for column
KeyPath")*. Repro: `cargo new my-app && cargo bundle --format msi`. Fix pointer:
mint sanitized MSI Identifiers (e.g. `F0001`, or slug the filename) for the `File`
primary key and reference that key from `Component.KeyPath`, keeping the real name
in the `FileName` column (which is `Category::Filename`, not `Identifier`).
Evidence link: `measurements/reruns/20260808-ten-framework-tri-platform/windows/packaging/logs/`.
*Note:* the corpus calls this "the historical fragility the runbook set it up to
demonstrate" — we can now say precisely what it is, and it is a one-line-ish fix.

### P-6.2 cargo-packager 0.11.8 NSIS: its own makensis resolves plugins from the system NSIS — **CONFIRMED, FILE**
Verbatim (`.../packaging/logs/iced-app-cargo-packager-nsis.log`), 7/7 buildable
apps failed identically:
```
INFO Running makensis.exe to produce …\iced-app_0.1.0_x64-setup.exe
ERROR Error running makensis.exe: failed to run command: "C:\Users\…\AppData\Local\.cargo-packager\NSIS\makensis.exe" "-V2" "…\installer.nsi"
stdout: Plugin directories:
  C:\Users\M.Pertsev\scoop\apps\nsis\current\Plugins\x86-unicode
stderr: Plugin not found, cannot call nsis_tauri_utils::SemverCompare
Error in script "…\installer.nsi" on line 178 -- aborting creation process
```
Root cause **CONFIRMED**, `cargo-packager-0.11.8/src/package/nsis/mod.rs`:
- `:300-334` downloads `nsis-3.09.zip`, extracts it to `%LOCALAPPDATA%\.cargo-packager\NSIS\`,
  and writes `ApplicationID.dll` and `nsis_tauri_utils.dll` into
  `<that dir>/Plugins/x86-unicode/`.
- `:578-595` invokes it: `Command::new(nsis_path.join("makensis.exe"))`, args `-V2`
  + the `.nsi` path, `.current_dir(intermediates_path)` — and **sets no `NSISDIR`
  env var and passes no `-NOCONFIG` / `!AddPluginDir`**.
makensis resolves `${NSISDIR}` (and therefore its plugin search path) from the
environment/registry, so on a machine with any other NSIS installed — here a scoop
install exporting `NSISDIR` — the vendored compiler loads the *system's* plugin
directory, which does not contain `nsis_tauri_utils.dll`. The generated
`installer.nsi` calls `nsis_tauri_utils::SemverCompare` at line 178 and aborts.
Net effect: **a fully present, correctly downloaded NSIS toolchain produces 0/10**.
**FILE? YES — crabnebula-dev/cargo-packager.** Title: *NSIS packaging fails when
another NSIS is installed: the vendored `makensis.exe` uses the system NSISDIR
plugin directory*. Fix pointer: `nsis_cmd.env("NSISDIR", nsis_path)` (and/or
`-X'!AddPluginDir /x86-unicode "<nsis_path>/Plugins/x86-unicode"'`) before the
`.output_ok()` at `src/package/nsis/mod.rs:578-595`. Include the verbatim
"Plugin directories:" line — it is the whole proof.

### P-6.3 gpui 0.2.2 build.rs requires the Metal Toolchain — **CONFIRMED, maybe file**
`apps/gpui-app/GAPS.md:8-24` records that the default build fails on macOS with
Xcode 26:
```
cargo::error=metal shader compilation failed:
error: error: cannot execute tool 'metal' due to missing Metal Toolchain;
use: xcodebuild -downloadComponent MetalToolchain
```
Root cause **CONFIRMED**: `gpui-0.2.2/build.rs:72-75` selects
`compile_metal_shaders()` unless the `runtime_shaders` feature is on, and
`:196-244` shells out to `xcrun … metal` / `xcrun -sdk macosx metallib`,
`cargo::error=`-ing on failure. Xcode 26 no longer ships the Metal Toolchain by
default, so a fresh macOS machine fails a default `cargo build` on gpui. GAPS.md
calls it "the single biggest setup trap for gpui on a fresh macOS machine" —
that matches the code.
**FILE? MAYBE — zed-industries/zed.** Title: *gpui build.rs fails on Xcode 26
(missing Metal Toolchain) without mentioning the `runtime_shaders` feature*. Ask:
either detect the missing toolchain and fall back to `runtime_shaders`, or extend
the `cargo::error=` message to name the feature. Low effort, high newcomer value.
Check first whether Zed's `main` already handles this — I did not verify main.

### P-6.4 Noted, not filed
- **dx bundle version-skew warning:** `dx and dioxus versions are incompatible!`
  (0.7.10 CLI vs 0.7.9 dioxus) — dx warned and bundled anyway
  (`report/21`, packaging §, footnote 3). Behaviour is arguably correct (warn, don't
  block); not worth an issue.
- **dioxus installed-copy launch failure** (3 reproductions across dx-NSIS and
  cargo-packager-WiX installs, `report/21` packaging §): a real, cross-installer
  observation, but with **no diagnostic captured at all** it is unfilable as it
  stands. Needs one Windows session running the installed exe from a console.
- **`Start-Process msiexec.exe` → ERROR_BAD_EXE_FORMAT** in harness run 3, not
  reproducible in an isolated elevated session (`report/21`): our harness /
  session-state anomaly. Not upstream.
- **freya's debug FPS overlay** auto-injected by `freya-performance-plugin` under
  `#[cfg(debug_assertions)]` inside `freya::prelude::launch` (`apps/freya-app/GAPS.md:42-44`):
  surprising but documented behaviour; not a defect.
- Everything in `report/data/fixes3-disposition.md` is internal-corpus bookkeeping;
  no upstream-attributable defect there that is not already covered above.

---

## Summary of corpus statements I now believe are WRONG or overstated

1. **report/data/packaging-results.md:66** — cargo-bundle does **not** use an
   AppleScript/Finder layout step; there is no decorative layout in its DMG path.
2. **report/data/packaging-results.md:71** — "None of the tested flows retried":
   tauri-bundler's vendored create-dmg retries `detach` (not `convert`).
3. **report/data/packaging-results.md:138** — the `hdiutil: convert failed -
   Resource temporarily unavailable` string is itself reconstructed; no log was
   retained. Flag it like the counts.
4. **report/data/packaging-results.md:269-272** — dx compiles into
   `target/desktop-release/` (ad-hoc cargo profile), not "its own `target/dx/<crate>`
   profile"; `target/dx/<crate>/bundle/` is the artifact dir.
5. **report/data/packaging-results.md:253, 308** — "the 0.7 docs still say signing
   is not provided" overstates it: the sentence's subject is mobile distribution,
   and the 0.7 config reference *does* document `[bundle.macos]` signing keys. The
   real gap is **notarization** (`APPLE_*` env vars, notarytool, stapler) documented
   nowhere.
6. **report/21 Headline 1 + framework table (freya row)** — "can't download its
   prebuilt Skia — malformed/missing URL for the Windows feature set" is **false**:
   the asset exists and downloads (HTTP 200, 22,315,880 B). The curl(3) cause was
   not isolated and is machine-local.
7. **report/21 Headline 1 + footnote 2 (peek apps)** — the `core-foundation`/`objc2`
   Windows failures are caused by **our own un-target-gated `[dependencies]`**, not
   by a framework crate; say so.
8. **report/21 Headline 1 (tauri row)** — should state plainly that the missing
   `icons/icon.ico` is our repo's omission, not a tauri defect.
9. **report/21 Headline 2** — "from wgpu's Vulkan surface creation": the retained
   error carries an **empty** per-backend error map, which points at instance
   initialisation rather than Vulkan surface creation. No Vulkan-specific error was
   ever captured.
10. **report/21 Headline 3** — "notify-rust toasts WORK … directly refuting risk #4":
    only proves `show()` returned `Ok`. No banner was observed or captured.
11. **report/18 §gpui footnote** — accurate on the X11 mechanism, but it should say
    gpui 0.2.2 renders via **Blade (ash/Vulkan)**, not wgpu; otherwise the finding
    gets triaged onto Zed's newer `gpui_wgpu` backend.
12. **report/18 Headline 1** — "gpui needed one `-dev` package": name it,
    `libxkbcommon-x11-dev`.
