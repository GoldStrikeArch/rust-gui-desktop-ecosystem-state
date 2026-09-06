# Draft replies for the r/rust thread (2026-08-30)

Three replies, one per commenter. Plain text, no AI mention needed beyond
the disclaimer already in the post. Numbers below are from the re-check in
`report/data/verification/`; links assume the repo at
`GoldStrikeArch/rust-gui-desktop-ecosystem-state`.

---

## Reply to the Freya author

You're right, and thanks for the pointer. `EventsCombos::pressed` /
`PressEventType` is in the 0.4 prelude (it's what freya-edit uses for
double-click word selection), and the kanban missed it and timed presses by
hand. Fixed: the app now calls the classifier from `on_press` (two lines,
timer gone), re-verified with synthetic clicks, and the rating moved from
hand-rolled to assembled. The FRICTION file, the report rows and the three
places on the dashboard that said "no double-click event" are corrected and
dated.

Two small things I noticed while fixing it, in case they're useful: the
classifier isn't linked from the `on_press` docs or `MouseEventData`, which
is how it got missed; and `Input`/`SelectableText` feed it element-relative
coordinates while `WindowDragExt` feeds it global ones, so the 5 px threshold
compares two coordinate spaces. Happy to file that as a docs issue if you
want one.

While re-checking Freya I also found that the freya-edit UTF-16 caret / ZWJ
split the text round reported is already fixed in 0.5.0-rc.1 (#2034), so the
dashboard now says so.

---

## Reply to CouteauBleu

Thanks, this was the push I needed to re-verify things properly instead of
just updating a sentence.

**AccessKit.** You're right on the substance. I re-cloned the repo: since
mid-March Arnold Loubriat authored 42 of 58 commits, merged all 54 PRs and
cut every release; Matt Campbell's last commit was 2026-03-04 (he still
reviews and is a crates.io owner). Loubriat is the only public member of the
org and describes it as spare-time work. On funding I was wrong: NLnet's
NGI0 Commons Fund has funded "iOS support for AccessKit" since Nov 2025, and
it shipped as `accesskit_ios` 0.1.0 in May. One nuance on "no GitHub
sponsor": his page lists two individual sponsors (one of them Campbell) and
no corporate one — and the project's own "Sponsor" link on accesskit.dev
points at Campbell's page, not his. The dashboard, the ecosystem map and the
load-bearing-crates table now say "one active maintainer", record the NLnet
grant as scoped feature funding rather than maintenance, and the funding
recommendation names him.

**fontique / the tofu.** Your instinct that this needed a closer look was
right: the "never scans AssetsV2" explanation in the post is wrong. PingFang
has no readable font file anywhere on macOS 26 (CoreText resolves it, but
the outlines are `hvgl`, per upstream's own comment); fontique 0.6/0.7 asks
CoreText for a Han fallback and then rejects the answer because it isn't in
its directory scan. Fixed in fontique 0.8.0 (falls back to Heiti). xilem 0.4
pins 0.6 and floem 0.7.0, so the actionable item is a bump, not a fontique
issue. The regional-indicator flags were a harfrust AAT shaping bug, also
gone in 0.8. I'll correct the post.

**The 11k-line abort.** You were right about culling: `render_text` in
masonry encodes every line. The other half is that vello and masonry_winit
request `Limits::default()` (128 MiB storage binding) while this M4 Pro
advertises 4 GiB; a GPU-free re-encode of the corpus comes to ~161 MiB. Two
issues (vello: limits + a typed error; masonry: viewport culling), drafted.

**Issues.** Every candidate finding got re-verified against the pinned
sources and current main/issue trackers before filing; the list is in
`report/data/upstream-issues.md` with drafts. Headlines: cosmic-text's
Backspace deletes a `char` while Delete deletes a grapheme; vizia's
backspace emoji state machine never advances its cursor (one-line fix);
muda's macOS use-after-free is fixed on main but unreleased (asking for a
release); muda's Edit roles swallow ⌘C/⌘V on non-NSText hosts; ehttp's
`with_timeout` flag is ignored for GET and inverted for POST (my original
"default timeout kills streams" claim was wrong); gpui's X11 depth-32 visual
gives a silent black window on software Vulkan; masonry_winit/floem
`.unwrap()` the surface; cargo-bundle's MSI identifiers and cargo-packager's
NSIS dir explain the 0/10 Windows installers. Also a list of things I'm *not*
filing because they were our bugs (the tauri `icon.ico`, un-gated Apple
deps in three peek apps) or already documented/fixed upstream.

**SPEC-1** was just the internal spec name for the todo app; I've stopped
using the prefix in public text.

---

## Reply to the "modal + numeric fields" commenter

Both are now drafted as the next two specs.

`SPEC-9 "Windows"`: a main list window, a modal edit dialog (recording which
modality the framework can actually reach: OS window-modal / app-modal /
overlay, verified by a synthetic click on the blocked parent), a singleton
inspector window with live bidirectional shared state, ping/pong messages
between windows, a preferences window that restyles every window, close
veto with Save/Discard/Cancel, parenting, position persistence, and a
multi-monitor DPI move.

`SPEC-10 "Ledger"`: a 12-row expense table with typed numeric fields
(filter while typing, locale parse/format with a live toggle, arrow
stepping, paste of `$1,234.56` and `(12.50)`), decimal alignment with
tabular figures, validation state, tab order and Enter-moves-down, undo, and
the first accessibility-tree dump per framework. Kept out of the todo app so
the iteration-1 build/size baselines stay valid.

Specs: `apps/SPEC-9.md`, `apps/SPEC-10.md`; the ranked backlog behind them
(editor, app lifecycle, chrome, print, embedding, RTL mirroring, a screen-
reader pass) is `report/data/next-apps-candidates.md`. If you have a
specific modal pattern from your accounting/automation work that you think
will break, tell me and I'll put it in the spec.

---

## Post edit (top of the original post)

> **Correction (2026-08-30):** two claims below were wrong and are fixed on
> the pages: Freya does support double-click (`EventsCombos::pressed`); and
> the fontique "AssetsV2" explanation for xilem's CJK tofu was wrong — the
> real cause (PingFang has no readable font file; old fontique rejects
> CoreText's answer) is fixed in fontique 0.8.0, so it's a dependency bump
> for xilem/floem, not a fontique bug. AccessKit's status is also updated:
> one active maintainer, plus an NLnet grant for the iOS adapter that I had
> missed. Full log: `report/data/corrections-2026-08-30.md`.
