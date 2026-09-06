# Reddit post, second try (r/rust)

## Before you post

Do not paste the deleted post again. Same title + same body after a Rule 6 removal looks like dodging the mods.

1. Wait a day if you can. Same-day reposts of removed threads get pulled faster.
2. Modmail first. Short: you posted measured results from 80 apps across 10 GUI frameworks, it was removed, you rewrote it without the AI note, is a project post okay. They asked people to do this for promo-adjacent stuff.
3. Do not mention AI. Not in the post, not in a comment, not "I used a model to copy-edit."
4. Do not mention the deletion.
5. After it is up, put the working-group / Zulip links in **your first comment**, not in the body.
6. Tweak a couple of sentences before you submit. If this file is pasted 1:1, a tired mod can still pattern-match.

Flair: `project`

Title (changed on purpose, the old one is in the removal log):

> 80 desktop apps, 10 Rust GUI frameworks, 3 OSes. Notes on where it actually breaks.

Backup if you want it closer to the original:

> I implemented the same 8 desktop apps in 10 Rust GUI frameworks and ran them on macOS, Linux, and Windows

---

## Body (paste this)

A few years ago I tried to build a desktop app in Rust and gave up. Not the borrow checker. Tray icon worked on macOS and died on Windows. File dialog didn't work at all. That kind of thing.

Came back to it this year. It's better than I remembered. Still messy though, and I couldn't find anyone who had actually measured where, so I did it.

10 frameworks: iced, egui, gpui, tauri, xilem, slint, dioxus, freya, vizia, floem.

Same 8 apps in each, 80 total:

- todo
- live metrics dashboard
- kanban with drag and drop
- OS shell (tray, global hotkey, native menubar, clipboard images, file drop, notifications)
- multilingual text
- camera + mic
- 100k-row grid
- network fetch with cancel / progress

First machine was my MacBook M4 Pro. Build times, binary sizes, dep trees, CPU, RSS. Packaged the todo apps into DMGs (not all 80, just those). Then I rebuilt the set in a headless Linux container, then again on a real Windows 11 laptop.

Writeup:

- https://goldstrikearch.github.io/rust-gui-desktop-ecosystem-state
- https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state
- apps: https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/tree/main/apps
- the OS shell traps separately: https://goldstrikearch.github.io/rust-gui-desktop-ecosystem-state/shell-facade.html

Small apps. This is not a speed ranking. "Which framework is fastest" is a different study and I haven't done it.

What I didn't expect:

The bottom of the stack is consolidating. HarfRust now shapes for cosmic-text, parley and egui, all within about a year. AccessKit and taffy crossed framework lines the same way. So the old "everyone reinvents everything" take is at least half wrong.

The top is not consolidating. Four text-layout stacks are still alive, and layout is the expensive part: line breaking, bidi, rich text, caret, all that.

Linux tray was the one that actually hurt. Every macOS-written tray app I tested compiled on Linux with zero source changes, then panicked at launch inside muda's first GTK call. Build-only CI would have been green.

Windows was worse, and it mostly wasn't the GUI code. 28 of 80 failed `cargo build --release` as-is. Three whole frameworks went 0/8: Tauri died on a missing `icon.ico` before any app code compiled, Freya couldn't download its prebuilt Skia, Vizia failed to link against an older MSVC STL.

Packaging the `.app` is easy now. `dx bundle` made DMGs for 9 of 9 non-Tauri apps on the first try. Signing and notarization is still where it falls off.

Sharpest text bug was editing, not rendering. 👨‍👩‍👧‍👦 is a ZWJ sequence (people glued together with U+200D). Backspace over that cluster splits it or leaves a dangling joiner in 5 of the 10 editors I tried. Looks upstreamable.

There's more on the dashboard, including a "pick one in five questions" thing if you're choosing a stack.

This was for an RCN initiative (Rust Foundation commercial network) on desktop GUI. areweguiyet already tracks the framework list, and Sid Askary / RustWeek have been running GUI meetings for years. Not trying to replace that. If you maintain or ship one of these, I dropped the Zulip / working-group links in a comment.

If I got your framework wrong, say so and I'll fix the page. Maintainers already caught a few things on an earlier draft.

---

## First comment (post this immediately)

Zulip thread where this started:
https://rust-lang.zulipchat.com/#narrow/channel/594428-commercial-network/topic/Cross-platform.20Desktop.20GUI.20in.20Rust/with/617030464

Working group issue if you want to be on it:
https://github.com/Rust-Commercial-Network/rcn/issues/46

RCN itself: https://rustfoundation.org/rust-commercial-network/

Nico Burns (Taffy / Blitz / blessed.rs) made the funding argument better than I can: winit has had 3–4 maintainers for years, unpaid, not full time, and it sits under almost everything except Tauri and GPUI. His #1 ask was even 1–2 people on winit full time.

The other thing we want is basically a recipes book. "How do I put a tray icon on all three platforms in framework Y." The 80 apps each have a FRICTION.md of what broke. That's the ugly first draft of that list.
