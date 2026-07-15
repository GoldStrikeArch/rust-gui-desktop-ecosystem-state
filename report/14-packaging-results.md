# Packaging: seven todo apps → `.app` + `.dmg` on macOS

**Run date:** 2026-07-09.

Iteration 3 tested how far seven already-built iteration-1 binaries were from
macOS application bundles and disk images. The resulting DMGs contain
**locally ad-hoc-signed `.app` bundles**; the DMGs themselves were verified,
not Developer-ID signed or notarized. The `spctl` result below assesses
distribution trust; it does not contradict the separate observation that each
locally built app launched on this machine. Full measurements and reconstruction:
[data/packaging-results.md](data/packaging-results.md). Artifacts are under
`dist/<framework>/`; all seven DMGs pass `hdiutil verify`.

## Results

Sizes are MiB: app values are `du -sk / 1024`; DMG values are logical bytes
divided by 1,048,576.

| App | Tool | Config | `.app` | `.dmg` | Launches | Local ad-hoc seal | Gatekeeper |
|---|---|---:|---:|---:|---|---|---|
| iced | cargo-bundle | 4 lines | 9.9 MiB | 4.2 MiB | yes | yes | rejected* |
| egui | cargo-bundle | 4 lines | 11.9 MiB | 5.5 MiB | yes | yes | rejected* |
| gpui | cargo-bundle | 4 lines | 5.0 MiB | 2.5 MiB | yes | yes | rejected* |
| tauri | tauri-cli bundler | 2 lines | 8.0 MiB | 3.0 MiB | yes | yes | rejected* |
| xilem | cargo-bundle | 4 lines | 11.4 MiB | 4.8 MiB | yes | yes | rejected* |
| slint | cargo-bundle | 4 lines | 14.7 MiB | 7.4 MiB | yes | yes | rejected* |
| dioxus | cargo-bundle | 4 lines | 5.7 MiB | 2.5 MiB | yes | yes | rejected* |

\* Expected for this credential-free run: an ad-hoc seal is not a Developer
ID distribution signature and was not notarized.

## What the experiment establishes

On this **macOS 26.5.2 arm64 machine**, wrapping already-built binaries was
low effort. cargo-bundle packaged all six non-Tauri apps from the same
four-line metadata shape; Tauri used its own bundler. cargo-packager was also
tested on iced as a comparison, but was not needed to produce the final six
non-Tauri bundles. This is a macOS result, not evidence about Windows or Linux
installer pipelines.

The built-in decorative DMG paths were unreliable on this machine. Counts
must distinguish app/tool outcomes from literal executions:

- **App/tool paths:** 8 evaluated (six cargo-bundle apps, one Tauri app and
  one iced cargo-packager comparison); 2 eventually produced a built-in DMG
  and 6 did not.
- **Literal built-in-DMG executions:** 13 reconstructed from the retained
  per-app `×N` notes; 2 succeeded and 11 failed. Exact command logs were not
  retained, so this is a reconstruction, not a raw event log.
- **Plain `hdiutil create` fallback:** final DMGs succeeded for 7/7 apps;
  Slint needed one retry, so that is 7 successes in 8 literal executions.

Keep a simple `hdiutil create -format UDZO` fallback. It was more reliable in
this macOS 26 run; the evidence does not justify a universal rule to disable
every tool's built-in DMG path.

## Signing and trust

No Developer ID identity or notarization credentials were configured, so no
tested run could produce a distribution signature. The explicit
`codesign -s - --deep --force` pass was a **local ad-hoc verification step**;
it is not distribution-signing guidance.

Tool capabilities differ:

- cargo-bundle has no integrated signing support.
- Tauri can use a configured Apple signing identity and automate
  notarization/stapling ([Tauri signing guide](https://v2.tauri.app/distribute/sign/macos/)).
- cargo-packager supports configured signing and distribution/update
  workflows ([cargo-packager documentation](https://docs.crabnebula.dev/packager/)).
- rcodesign is a principal Rust-native cross-platform implementation for
  signing, notarization and stapling
  ([apple-codesign documentation](https://docs.rs/apple-codesign/latest/apple_codesign/)).

These tools cannot supply Apple programme membership, certificates,
identities or credentials. Gatekeeper's distribution assessment rejected all
seven ad-hoc-signed apps, which confirms the missing trust material—not an
absence of Rust tooling or a failure of local launch.
Distribution code signing is also separate from signing updater artifacts.

## Practical conclusion

For this sample, macOS bundling was inexpensive once a release binary and icon
existed. The remaining release work is credential- and channel-specific:
distribution identity, notarization, stapling, updater production/runtime,
and equivalent Windows/Linux installer testing. Packaging can affect runtime
behavior too: the notification experiment did not observe a banner from its
raw unbundled process, so app identity should be tested as part of shell
integration rather than assumed.

## Caveats

macOS 26.5.2/arm64 only. No Developer ID or Apple notarization run was
attempted; Windows MSI/SmartScreen and Linux AppImage/Flatpak paths were not
tested. The retained evidence verifies bundle structure, ad-hoc seal,
launching, `hdiutil verify`, and expected `spctl` rejection—not a
store-ready or generally downloadable release.
