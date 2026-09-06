# TEXT / EDITING verification — "Babel" round

Verifier notes. Machine: macOS **26.6.2 (25G83)**, Apple M4 Pro. All probes below were
re-run today against the *pinned* crate sources (`~/.cargo/registry/src/index.crates.io-*`)
and against upstream clones in a local clone directory
(`parley-main`, `vello-main`, `cosmic-text-main`, `vizia-main`, `freya-main`, `druid-main`).

Standalone reproductions I built (all in this dir, all runnable):

| dir | what it proves |
|---|---|
| `parley-repro` (=0.6.0), `parley-repro7/8/10`, `parley-main-repro` | ZWJ backspace + cluster dump + RI-flag/symbol/Han shaping, per parley version |
| `vizia-repro` | verbatim copy of `vizia_core-0.4.0/src/text/backspace.rs` + a `&str` shim |
| `fontique-probe` | `Collection::fallback_families` for 10 scripts under fontique 0.6 vs 0.10 |
| `vello-scene-size` | encodes the real 11k-line corpus through parley 0.6 → vello 0.6 `Scene` and prints the packed scene-buffer size + wgpu limits |

---

## T-A — "Backspace over a ZWJ family emoji splits the cluster in five of the ten editing paths"

### 1. CLAIM

`report/13-text-i18n-results.md` editing matrix + footnote ¹ (lines ~55-75);
per-app rows in `apps/<fw>-babel/FRICTION.md`.

### 2. EVIDENCE CHECK — the five are: **iced, egui, xilem, vizia, freya**

Read every `apps/*-babel/FRICTION.md`. The ten paths split cleanly:

| framework | observed backspace over `👨‍👩‍👧‍👦` | FRICTION line |
|---|---|---|
| **iced** (cosmic-text `Editor`) | `a👨‍👩‍👧‍👦` → `a👨‍👩‍👧‍` (dangling ZWJ) | `apps/iced-babel/FRICTION.md:21` |
| **egui** (`TextEdit`) | 1 scalar/press, 7 presses, dangling ZWJ | `apps/egui-babel/FRICTION.md:53` |
| **xilem** (masonry `TextArea` → parley `PlainEditor`) | hexdump `61 62 f09f91a8 e2808d … e2808d` = dangling ZWJ | `apps/xilem-babel/FRICTION.md:18` |
| **vizia** (`Textbox`) | 26 B → 22 B (drops U+1F466) → 19 B (drops ZWJ) | `apps/vizia-babel/FRICTION.md:36` |
| **freya** (`freya-edit` 0.4.1) | `a👨‍👩‍👧‍👦b` → `\u{200d}👩‍👧‍👦b` — **corrupts, leading ZWJ** | `apps/freya-babel/FRICTION.md:21` |
| gpui | whole cluster (hand-rolled + `unicode-segmentation`) | `gpui-babel/FRICTION.md:44` |
| tauri / dioxus | whole cluster (WebKit) | `tauri:21`, `dioxus:24` |
| slint | codepoint-by-codepoint **by design** | `slint-babel/FRICTION.md:40` |
| floem | not defective — caret stops `[0,1,26,27]`; `DeleteBackward` → `buffer.move_left(.., Insert, 1)` which is grapheme-based in xi-rope (`~/.cargo/git/checkouts/floem-ab9be4e01bb293da/778bb5f/editor-core/src/editor.rs:1127`) | `floem-babel/FRICTION.md:21` |

So "five of the ten" is **correct**, and the five are iced / egui / xilem / vizia / freya.
Slint is a sixth codepoint-deleter but is deliberate and non-corrupting — the corpus already says so.

Cross-platform corroboration: `report/data/windows-rows.md:89` —
`selftest backspace over family: "a👨‍👩‍👧‍" (len 7) -> CORRUPTED/partial` (iced-babel on Windows).
Artifact: `measurements/reruns/20260808-ten-framework-tri-platform/windows/babel-shots/iced-babel.png`.

### 3. ROOT CAUSE — per path

**iced / cosmic-text 0.15.0 — CONFIRMED (code located).**
`cosmic-text-0.15.0/src/edit/editor.rs:634-659`, `Action::Backspace`:

```rust
self.cursor.index = self.with_buffer(|buffer| {
    buffer.lines[self.cursor.line].text()[..self.cursor.index]
        .char_indices().next_back().map_or(0, |(i, _)| i)   // one char = one scalar
});
```

Deletes **by char**. The *forward* `Action::Delete` twenty lines below
(`editor.rs:675-681`) uses `.grapheme_indices(true)`. So the crate already depends on
`unicode-segmentation` (`src/edit/editor.rs:12`) and already deletes by grapheme in one
direction — Backspace is simply the odd one out. That asymmetry is the whole bug.
Caret motion is grapheme-based too (`editor.rs:44,137-140`), which is why iced-babel
measured "motion grapheme-atomic, delete scalar".

**egui 0.35.0 — CONFIRMED.** `egui-0.35.0/src/widgets/text_edit/text_buffer.rs:133-141`
`delete_previous_char` = `[ccursor - 1, ccursor]` in `CharIndex` units; called from
`builder.rs:1304-1312` on `Key::Backspace`. Motion is `index ± 1` too
(`epaint-0.35.0/.../text_layout_types.rs`, per FRICTION). This is **by char, everywhere** —
egui is internally consistent, it just isn't grapheme-aware.

**xilem / parley 0.6.0 — CONFIRMED, and reproduced standalone.**
masonry 0.4 calls `drv.backdelete()` (`masonry-0.4.0/src/widgets/text_area.rs:692`).
`parley-0.6.0/src/layout/editor.rs:293-330` *tries* to do the right thing:

```rust
let start = if cluster.is_hard_line_break() || cluster.is_emoji() {
    range.start          // "For newline sequences and emoji, delete the previous cluster"
} else { /* previous char */ };
```

but in parley ≤ 0.11 a `Cluster` is **one character**, not one grapheme. `parley-repro`
(parley 0.6.0, this machine) dumps the layout for `ab👨‍👩‍👧‍👦cd`:

```
cluster range=2..6   text="👨"      glyphs=1     <- the ligature glyph
cluster range=6..9   text="\u{200d}" glyphs=0
cluster range=9..13  text="👩"      glyphs=0
... 7 clusters for one displayed glyph ...
cluster range=23..27 text="👦"      glyphs=0
```

so "delete the previous cluster" removes 4 bytes. Running `backdelete()` from the end:

```
start : … 1F467 200D 1F466 0063 0064
bksp3 : U+0061 U+0062 U+1F468 U+200D U+1F469 U+200D U+1F467 U+200D   <- dangling ZWJ
```

byte-identical to `apps/xilem-babel/FRICTION.md:18`.

**vizia 0.4.0 — CONFIRMED, and this is the most clear-cut bug of the five.**
`vizia_core-0.4.0/src/views/textbox.rs:1243-1265` emits
`TextEvent::DeleteText(Movement::Grapheme(Direction::Upstream))`; `delete_text`
(`textbox.rs:330-345`) calls `offset_for_delete_backwards` →
`vizia_core-0.4.0/src/text/backspace.rs:4-171`, a port of Druid's port of Android's
emoji backspace state machine. The port dropped the cursor advance:

```rust
while state != State::Finished && cursor > 0 {
    let code_point = text.prev_codepoint(cursor).unwrap_or('0');   // cursor never moves
    match state { … }
}
```

Druid's original (`druid-main/druid/src/druid/src/text/backspace.rs:33-38`) uses a
**stateful** cursor whose `prev_codepoint()` advances:

```rust
let mut cursor = text.cursor(start).expect(…);
while state != State::Finished && cursor.pos() > 0 {
    let code_point = cursor.prev_codepoint().unwrap_or('0');
```

vizia replaced it with the stateless `EditableText::prev_codepoint(offset)`
(`vizia_core-0.4.0/src/text/editable_text.rs:204`) and never decrements `offset`, so the
machine sees the same code point forever and always exits after ≤ 2 iterations.
`vizia-repro` (verbatim copy of the file + a `&str` shim) reproduces the FRICTION numbers exactly:

```
start: 26 bytes  U+0061 U+1F468 U+200D U+1F469 U+200D U+1F467 U+200D U+1F466
bksp1: 22 bytes  … U+1F467 U+200D            <- only 👦 removed
bksp2: 19 bytes  … U+1F467                   <- only the ZWJ removed
```

Same effect for flags, keycaps, tag sequences and skin tones — the entire table of
`EMOJI_TABLE`/`is_emoji_modifier`/`is_regional_indicator_symbol` logic is dead code.

**freya-edit 0.4.1 — CONFIRMED (this is also T-G).**
`freya-edit-0.4.1/src/text_editor.rs:560-571`: `self.remove(cursor_pos - 1 .. cursor_pos)`
where `cursor_pos` is a **UTF-16 code-unit** offset
(`rope_editor.rs:195-208` converts with `rope.utf16_cu_to_char`, `len_utf16_cu()` at
`rope_editor.rs:250`). One backspace removes one UTF-16 unit, i.e. *half a surrogate pair*
for any astral character; `cursor_left/right` step by one UTF-16 unit too, so the caret can
sit inside a surrogate pair (freya-babel measured offsets `[0,1,2,3]` over an 11-unit
cluster). `unicode-segmentation` is a dependency of `freya-edit` 0.4.1 but is not used here.

**slint 1.17.1 — NOT-A-BUG.** `i-slint-core-1.17.1/items/text.rs:981-984`:
`// Special case: backspace breaks the grapheme and selects the previous character`,
and the enum variant is documented at `:1291`
(`/// breaks grapheme boundaries, so only used by delete-previous-char`). Intentional.

**Is "one codepoint" ever intended?** For a terminal/vi-style editor, yes — cosmic-text
also ships a `vi` mode. But the same crates already do grapheme motion (cosmic-text,
vizia, iced) or grapheme *forward* delete (cosmic-text), so the mismatch inside one editor
is the defect, not the granularity per se. Slint is the one case where the asymmetry is
a documented product decision.

### 4. UPSTREAM STATUS

| crate | latest release | main | status |
|---|---|---|---|
| **cosmic-text** | 0.15.0 | `cosmic-text-main` @ daae9c7 | **still present** — `Action::Backspace` on main is byte-identical to 0.15.0; `Action::Delete` still uses `grapheme_indices`. No issue found (`issues?q=backspace+grapheme` → *no results*; the emoji issues #327/#316/#310 are rendering, not editing). |
| **egui** | 0.35.0 | — | still per-char. Tracking-adjacent: no grapheme issue found; egui's text limitations are tracked by #1016 (BiDi) and #3378 (cosmic-text). |
| **parley** | 0.11.1 (2026-08-16) | `parley-main` @ d1c6e87 | **fixed on main, unreleased.** PR **#715** "Differentiate shaped clusters and graphemes, introduce 'atoms'" (f132cbd, 2026-08-05) makes `Cluster` span a full grapheme (CHANGELOG *Unreleased*: *"Breaking change: `Cluster` now spans a full grapheme cluster instead of a single character"*). `parley-main-repro` confirms: `bksp3` now deletes the whole family. Not in any tag (`git tag --contains f132cbd` → empty). Related open issue: **#694** "Cursor motion steps are codepoints, not grapheme clusters" (open, 2026-07-16) — motion only, references a deletion fix in #693. |
| **vizia** | 0.4.0 | `vizia-main` @ 426d2e7 | **still present on main** — `crates/vizia_core/src/text/backspace.rs` still has the non-advancing loop (only `cursor = offset` at line 169, inside the *final* deletion loop). No existing issue (searched backspace/emoji/grapheme → only #403, #286, both closed and about rendering). |
| **freya-edit** | 0.5.0-rc.4 (2026-08-23) | `freya-main` | **fixed and released.** freya PR **#2034** "feat(core): More accurate cursor rendering in special characters" (7baeeee, 2026-07-30) added `grapheme_cluster_at()` (`crates/freya-edit/src/text_editor.rs:185-200`, uses `unicode_segmentation::graphemes(true)`); Backspace is now `grapheme_cluster_at(cursor_pos-1).start .. cursor_pos` (`:683`), `cursor_left/right` likewise (`:335-350`). Verified present in the published 0.5.0-rc.1 and 0.5.0-rc.4 tarballs; **absent** from 0.4.1 (0.4 maintenance branch, published 2026-08-02 *after* the fix). |

### 5. FILE?

**YES — cosmic-text.** Clean, small, self-evidently inconsistent, still on main, no duplicate.

> **Title:** `Editor`: Backspace deletes one `char`, Delete deletes one grapheme cluster
> **Repo:** pop-os/cosmic-text
> **Body:**
> `cosmic-text 0.15.0` and `main` (daae9c7), macOS 26.6.2 and Windows 11, via `iced 0.14`'s `text_editor`.
>
> `Action::Delete` deletes the next **grapheme cluster** (`src/edit/editor.rs:675-681`, `grapheme_indices(true)`),
> but `Action::Backspace` deletes the previous **char** (`src/edit/editor.rs:640-646`, `char_indices().next_back()`).
> Caret motion is grapheme-based (`editor.rs:44, 137-140`). The result is that Backspace over a
> ZWJ emoji sequence dismembers a cluster the caret cannot otherwise enter.
>
> Repro (no window needed, through iced's `text_editor::Content`, which is a thin wrapper over `Editor`):
> ```rust
> let mut c = Content::with_text("a\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}");
> c.perform(Action::Move(Motion::DocumentEnd));
> c.perform(Action::Edit(Edit::Backspace));
> // expected: "a"
> // actual:   "a\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}"  (dangling ZWJ)
> ```
> Right/Left arrow correctly treat the family as one stop, so motion and deletion disagree.
> Same result on Windows 11 (`(len 7) -> CORRUPTED/partial`).
>
> Suggested fix: mirror `Action::Delete` — walk `grapheme_indices(true)` backwards from `cursor.index`.
>
> Evidence: https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/iced-babel/FRICTION.md
> and https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/report/data/windows-rows.md

**YES — vizia.** One-line fix, still on main, nobody has reported it.

> **Title:** `backspace_offset` never advances its cursor, so the emoji/ZWJ state machine is dead code
> **Repo:** vizia/vizia
> **Body:**
> vizia 0.4.0 and `main` (426d2e7), macOS.
>
> `crates/vizia_core/src/text/backspace.rs` is a port of Druid's Android-derived backspace
> state machine, but the scan loop never moves `cursor`:
> ```rust
> while state != State::Finished && cursor > 0 {
>     let code_point = text.prev_codepoint(cursor).unwrap_or('0');  // same code point every iteration
>     match state { … }
> }
> ```
> Druid's original walks a stateful `EditableTextCursor` whose `prev_codepoint()` advances
> (`druid/src/text/backspace.rs`). Because `cursor` is fixed, the machine always terminates after at
> most two iterations and `delete_code_point_count` stays 1, so every ZWJ sequence, flag pair,
> keycap and tag sequence is deleted one code point at a time.
>
> Repro (pure function, no window):
> ```
> "a👨‍👩‍👧‍👦"  26 bytes
> backspace -> 22 bytes  (only U+1F466 removed)
> backspace -> 19 bytes  (only U+200D removed)
> ```
> Expected: one backspace removes the whole 25-byte cluster, as `Movement::Grapheme(Upstream)`
> implies and as `move_cursor` already does.
>
> Fix: decrement `cursor` by the code point's length at the end of each loop iteration
> (`cursor = text.prev_codepoint_offset(cursor).unwrap_or(0);`).
>
> Evidence: https://github.com/GoldStrikeArch/rust-gui-desktop-ecosystem-state/blob/main/apps/vizia-babel/FRICTION.md

**MAYBE — parley (see T-E note below).** The ZWJ case is fixed on main by #715, so do **not**
file it. What is *still* wrong on main is narrower and worth one issue:

> **Title:** `backdelete()` still splits grapheme clusters outside the hard-coded emoji ranges (flag pairs, Indic conjuncts)
> **Repo:** linebender/parley
> **Body:**
> Tested on `main` @ d1c6e87 (after #715, so `Cluster` already spans a grapheme), macOS 26.6.2.
> `PlainEditorDriver::backdelete` (`parley/src/editing/editor.rs:286-320`) deletes the whole cluster
> only when `cluster.is_emoji()`, and `is_emoji` is a hard-coded range list
> (`parley/src/layout/data.rs`) that omits regional indicators U+1F1E6–U+1F1FF:
> ```
> "a🇺🇳"     backspace -> "a🇺"    (lone regional indicator, re-renders as a letter box)
> "aक्ष"     backspace -> "aक्"    then "aक"
> ```
> ZWJ families and skin tones are fine post-#715. Now that clusters are graphemes,
> `is_emoji()` looks redundant — deleting `range.start..range.end` unconditionally would
> match platform behaviour. (Related: #694.)

**NO — egui.** Per-char editing is uniform and consistent with egui's whole text model
(no BiDi, no system fonts, no color emoji). Filing a grapheme-backspace issue alone would be
noise next to #1016/#2551/#3378. Mention it there if anywhere.

**NO — freya.** Already fixed upstream (#2034, shipping in 0.5.0-rc). The corpus should say
"fixed in freya-edit 0.5.0-rc.1; 0.4.x affected".

**NO — slint.** Documented intent.

### 6. Corpus corrections

- `report/13` footnote ¹ says "four implementation paths"; with vizia/freya/floem added the
  count is **ten paths, five defective**. Fine as written for the seven-framework round, but
  the dashboard should not reuse the "four" number.
- `apps/freya-babel/FRICTION.md:54-55` ("the single worst text finding of the cohort") should
  gain "**fixed upstream in freya-edit 0.5.0-rc.1 / freya PR #2034**".

---

## T-B — fontique's macOS Han fallback returns None (xilem tofu, floem tofu, ▲▼ tofu)

### 1. CLAIM

`report/13-text-i18n-results.md` footnote ⁵; `apps/xilem-babel/FRICTION.md:17`;
`apps/floem-babel/FRICTION.md` (cjk_render row).

### 2. EVIDENCE CHECK — symptom confirmed, *explanation* wrong

`fontique-probe` on this machine, `Collection::fallback_families` per script:

```
fontique 0.6   Hani: []                     <- no Han fallback at all
fontique 0.6   Hira: ["Hiragino Sans"]      Kana: ["Hiragino Sans"]
fontique 0.6   Hang: ["Apple SD Gothic Neo"]  Deva/Thai/Arab/Hebr: all resolve
fontique 0.10  Hani: ["Heiti SC"]           <- resolves
```

That is *exactly* the corpus symptom: Han tofu, kana fine (Hiragino), Hangul fine
(Apple SD Gothic Neo). `parley-repro` also reproduces the two sibling symptoms in one shot:

```
parley 0.6 : ▲▼ = [0,0]   ⌘⇧ = [0,0]   世界 = [0,0]      (all .notdef)
parley 0.10: ▲▼ = [49348,49751]  ⌘ = 49261, ⇧ = 0  世界 = [537,646]
```

So the **xilem-grid ▲/▼ sort arrows and the floem-tray ⌘/⇧ tofu are the same root cause**,
not separate defects. (U+21E7 ⇧ is still `.notdef` on fontique 0.10 — a residual, smaller gap.)

**Wrong in the corpus:** "macOS 26 stores PingFang … in `/System/Library/AssetsV2/…`".
On this machine:

- `ls /System/Library/Fonts | grep -i pingfang` → nothing
- `/System/Library/AssetsV2/com_apple_MobileAsset_Font7|Font8` contain only a `.xml` manifest
- `find /System /Library -maxdepth 8 -iname 'PingFang*'` → nothing
- `mdfind -name PingFang` → only Outlook font-preview PNGs

and, decisively, `PingFang SC` is missing from the name map of fontique **0.10** too
(`0.10 PingFang SC present in name map: None`) even though 0.10 enumerates via
`CTFontCollection::from_available_fonts` and reads `kCTFontURLAttribute`. There is no
readable PingFang font *file* on this system at all. Upstream's own comment names the real
reason: fontique main, `fontique/src/backend/coretext.rs`, in `fallback()`:

```rust
// HACK: if we don't have a usable PingFangUI due to our inability
// to render hvgl outlines then try another font
```

i.e. PingFang on modern macOS is delivered as **PingFangUI with `hvgl` outlines**, which the
Rust font stack cannot read — not as an AssetsV2 file the scanner merely missed.

### 3. ROOT CAUSE — CONFIRMED

`fontique-0.6.0/src/backend/coretext.rs:34-45` (identical in 0.7.0:36-42):

```rust
let paths = NSSearchPathForDirectoriesInDomains(
        NSSearchPathDirectory::LibraryDirectory,
        NSSearchPathDomainMask::AllDomainsMask, true)
    .into_iter().map(|p| format!("{p}/Fonts/"));
let scanned = scan::ScannedCollection::from_paths(paths, 8);
```

→ `{~,/,/System,/Network}/Library/Fonts/` only. And `fallback()` (`coretext.rs:67-73`):

```rust
let font = create_fallback_font_for_text(sample, key.locale(), false)?;  // CoreText knows the answer
let family_name = unsafe { font.family_name() };
self.name_map.get(&family_name.to_string()).map(|n| n.id())              // …but must be in the scan
```

CoreText correctly answers "PingFang SC" for a Han sample; the directory-scanned `name_map`
has never heard of it; `.map()` yields `None`; parley then has **no** Han fallback and emits
`.notdef`. Any family CoreText can resolve but the scanner cannot see is unusable.

### 4. UPSTREAM STATUS — **FIXED in fontique 0.8.0**

`git log -- fontique/src/backend/coretext.rs` in `parley-main`:

| commit | date | PR | in tag |
|---|---|---|---|
| `df564f4` fontique: enumerate system fonts via CoreText | 2026-02-06 | #536 | v0.8.0 + |
| `8de139f` fix: combine CoreText enumeration with Library/Fonts directory scan | 2026-03-03 | #568 | v0.8.0 + |
| `0a3eea5` Use Heiti SC/TC on MacOS when PingFangUI contains hvgl outlines | 2026-04-09 | #598 | v0.9.0 + |

Tag dates: v0.7.0 2025-11-24, **v0.8.0 2026-03-27**, v0.9.0 2026-04-21, v0.10.0 2026-06-01,
v0.11.0 2026-06-26, v0.11.1 2026-08-16. Main now does
`scan_system_fonts()` = `CTFontCollection::from_available_fonts` + URL extraction + the old
directory scan, plus the Heiti SC/TC Han hack. Verified empirically above (0.10 resolves Han).

So: **not a bug to file.** Xilem 0.4 pins parley/fontique **0.6**; floem @ `778bb5f2` pins
parley/fontique **0.7.0** (`apps/floem-babel/Cargo.lock` — note the corpus says "0.6" for
floem; it is 0.7.0, same code, same defect). Slint 1.17.1 pins fontique **0.10.0** — hence
"unaffected", as the corpus observed.

No linebender/parley issue about AssetsV2 / PingFang / macOS discovery was found beyond the
merged PRs above (#597 "macOS: CJK punctuation marks not rendered correctly" is closed,
same era).

### 5. FILE? — **NO** for fontique. The actionable items are downstream bumps:

- **xilem/masonry**: ship a release on parley ≥ 0.8 (xilem 0.4 → parley 0.6). Worth a short
  issue/comment on linebender/xilem: *"Xilem 0.4 renders Han as tofu on macOS 26 because it
  pins fontique 0.6, which predates the CoreText-enumeration fix (#536/#568/#598, fontique 0.8)."*
- **floem**: same, bump past parley 0.7.
- Optional micro-report to parley: U+21E7 (⇧) still `.notdef` on fontique 0.10 on macOS 26.
  Low value; verify against 0.11.1 before filing.

### 6. Corpus corrections

- Replace "macOS 26 stores PingFang in `/System/Library/AssetsV2/…` on-demand storage" with
  "PingFang(UI) has no readable font file on macOS 26 — CoreText resolves it, but its
  outlines are `hvgl`, which the Rust stack cannot rasterize; fontique 0.8+ falls back to
  Heiti SC/TC."
- Add "floem pins fontique **0.7.0**, not 0.6."
- Add "fixed upstream in fontique 0.8.0 (2026-03-27)" — the corpus's *"Current fontique must
  be retested before generalizing"* is now settled.
- Fold the xilem-grid ▲/▼ and floem-tray ⌘/⇧ tofu into this one root cause.

---

## T-C — xilem: 11k-line prose aborts on a wgpu buffer limit

### 1. CLAIM

`report/13` large-document table + `apps/xilem-babel/FRICTION.md:21,78-79`:
*"192 MiB scene > 128 MiB buffer limit"*.

### 2. EVIDENCE CHECK — no retained artifact, but **independently reproduced**

`apps/xilem-babel/FRICTION.md:21` states outright *"The raw run output was not retained"*;
there is no log under `apps/xilem-babel/` (only `Cargo.*`, `FRICTION.md`, `screenshot.png`, `src/`)
and no `192`/`max_buffer_size` string anywhere in `measurements/`. So the *quoted error text*
is unverifiable.

The *claim* is now verified arithmetically without a GPU. `vello-scene-size` lays out
`apps/babel-assets/corpus.txt` ×1000 through parley 0.6 at 15 px / 1100 px width (exactly what
`apps/xilem-babel/src/main.rs:22,47-54,66` does) and encodes it into a `vello::Scene` with the
same calls as `masonry_core::core::text::render_text`:

```
text bytes=1381000 lines=11000
layout lines=11001
packed scene buffer = 168837696 bytes = 161.0 MiB
wgpu default max_storage_buffer_binding_size = 134217728 bytes = 128 MiB
```

**161 MiB > 128 MiB.** Direction and magnitude confirmed; the corpus's "192 MiB" is a bit high
(plausibly a different width/DPI, plus selection/caret fills). I'd restate it as "~160–190 MiB".

### 3. ROOT CAUSE — CONFIRMED, two independent contributors

**(a) No culling — the commenter is right.**
`masonry_core-0.4.0/src/core/text.rs:42-110` `render_text`:

```rust
for line in layout.lines() {
    for item in line.items() { … scene.draw_glyphs(font)…draw(Fill::NonZero, glyph_run.glyphs()…) }
}
```

Every line of the parley layout is encoded every frame; there is no viewport, clip or
`block_min/max_coord` test anywhere in the function, and `TextArea::paint`
(`masonry-0.4.0/src/widgets/text_area.rs:924-976`) adds selection rects for the whole layout
before calling it. `text_area.rs:60` even says *"if clipping is desired, that should be added
by the parent widget"* — but a clip does not reduce what is **encoded**. So yes: masonry/xilem
have no pre-render culling for text, and vello encodes off-screen glyphs.

**(b) The 128 MiB is self-imposed.**
`vello-0.6.0/src/render.rs:182` uploads the whole packed scene as one buffer:
`recording.upload("vello.scene", packed)`, then binds it as a storage buffer to ~12 compute
passes (`render.rs:206,246,275,289,…`). The binding limit comes from the device, and vello
requests the conservative default:

- `vello-0.6.0/src/util.rs:165` → `let limits = Limits::default();` (128 MiB) — unchanged on
  `vello-main` @ c93cb0d, `vello/src/util.rs:167`
- masonry ships its own copy with the same line: `masonry_winit-0.4.0/src/vello_util.rs:210`

whereas this machine's adapter reports:

```
IntegratedGpu Apple M4 Pro backend=Metal max_storage_buffer_binding_size=4095 MiB  max_buffer_size=13639 MiB
```

So a 161 MiB scene fails on hardware that supports 4 GiB bindings, purely because
`Limits::default()` was requested and no size check exists before the bind.

### 4. UPSTREAM STATUS

- vello `main` @ c93cb0d: still `Limits::default()`; no scene-size guard; classic pipeline
  unchanged in this respect. Sparse-strips work has viewport culling (#1304, closed) but that
  is the new CPU/hybrid renderer, not the wgpu classic path xilem 0.4 uses.
- Issue search: nothing matching "scene exceeds max_storage_buffer_binding_size". Nearest
  neighbours are **#736** (buffer size 17179869184 > 268435456 — a bump-buffer blow-up, different),
  **#788** (encoding overflow with many images), **#152** ("Variable size scene encoding"),
  **#366** (no dynamic allocation inside a submitted command buffer), **#291** (closed).
  → **not already reported.**

### 5. FILE? — **YES, on vello**, plus a smaller one on masonry. Rating: CONFIRMED for the
mechanism, and I now have a GPU-free repro to attach.

> **Title:** Large scenes hit `max_storage_buffer_binding_size` because the device is created with `Limits::default()`
> **Repo:** linebender/vello
> **Body:**
> vello 0.6.0 and `main` (c93cb0d), macOS 26.6.2, Apple M4 Pro / Metal, wgpu 26.
>
> The whole encoded scene is uploaded and bound as a single storage buffer
> (`vello/src/render.rs:182` `recording.upload("vello.scene", packed)`, bound in ~12 passes).
> `RenderContext::new_device` requests `wgpu::Limits::default()` (`vello/src/util.rs:167`), i.e.
> `max_storage_buffer_binding_size = 128 MiB`, while this adapter advertises **4095 MiB**.
> A moderately large text scene therefore dies on a wgpu validation error on hardware that
> could render it.
>
> Repro (no GPU needed to see the size):
> ```rust
> // 11k lines of mixed-script text, one parley layout, encoded exactly like
> // masonry_core::core::text::render_text
> let mut resolver = vello_encoding::Resolver::new();
> let mut packed = Vec::new();
> resolver.resolve(scene.encoding(), &mut packed);
> // packed = 168_837_696 bytes = 161.0 MiB  vs  Limits::default().max_storage_buffer_binding_size = 128 MiB
> ```
> Two things would help, independently:
> 1. request `adapter.limits()` (or at least raise `max_storage_buffer_binding_size` /
>    `max_buffer_size` toward the adapter's) in `RenderContext::new_device`;
> 2. return a typed `Error` (e.g. `SceneTooLarge { bytes, limit }`) from `Renderer::render_to_*`
>    instead of letting wgpu validation abort the process, so downstreams can degrade gracefully.
>
> (Downstream context: `masonry_winit` ships its own copy of this device setup with the same
> `Limits::default()` — `masonry_winit/src/vello_util.rs:210`.)

> **Title:** `render_text` encodes every line of the layout, with no viewport culling
> **Repo:** linebender/xilem (masonry)
> **Body:**
> masonry 0.4 / masonry_core 0.4, macOS.
> `masonry_core::core::text::render_text` (`src/core/text.rs:42`) iterates `layout.lines()`
> unconditionally, and `TextArea::paint` adds selection geometry for the whole layout first.
> A `prose` / `TextArea` holding an 11,000-line document therefore encodes ~11k lines of glyphs
> into the Vello scene every frame; the parent's clip removes them from the *output* but not
> from the *encoding*. Measured: 161 MiB packed scene, over wgpu's default 128 MiB storage-buffer
> binding limit, so the app dies on a validation error rather than being merely slow.
> Skipping lines whose `LineMetrics::block_{min,max}_coord` fall outside `ctx.clip_rect()` (or
> equivalent) would be a cheap fix. `virtual_scroll` exists as a workaround but a single long
> `Prose` is the obvious thing to reach for.

### 6. Corpus corrections

- "192 MiB scene" → I measure 161 MiB for the same input; restate as approximate and note that
  the raw run output was not retained.
- Add: the limit is *not* the hardware's — the adapter here allows 4095 MiB; vello and
  masonry_winit both request `Limits::default()`.

---

## T-D — egui

### (1) macOS AX RTL selection — **UNVERIFIED, do not file**

`apps/egui-babel/FRICTION.md` "Other findings": `AXSelectedText` for a selection containing
Arabic came back as `اببحرمرحبا` (doubled/reordered) while ⌘C was byte-exact;
`AXSelectedTextRange`/`AXNumberOfCharacters` reported inconsistent units; AX
`SetTextSelection` writes ignored. No trace retained, and the FRICTION file itself says
screen-reader output was not tested.

Context that matters: **epaint 0.35 has no BiDi at all** — `// TODO(emilk): heed bidi
characters`, epaint `text/font.rs:830`; the FRICTION glyph-x probes show multi-word Arabic
renders in logical order and Arabic-Indic digits mirror (١٢٣ → ٣٢١). So a "wrong RTL
selection" is the *expected* consequence of the tracked limitation, not a new bug.

Tracking issue: **emilk/egui#1016 "Bidirectional text support"** (open since 2021-12-29,
labels feature/text). Related: #3378 "Cosmic Text for font rendering" (open). #5546
"RTL layout breaks with TextEdit" is closed.

→ Comment on #1016 if anything. The *doubled-characters* AX readback might be a genuine
separate accesskit/egui bug, but without a retained repro I would not file it.

### (2) "egui dropped combining marks" — **NOT-A-BUG (font coverage), root cause of the
*silence* unverified**

`apps/egui-babel/FRICTION.md` mixed_fallback_line row: 13 combining marks (U+0308 &c.) in the
Zalgo line exist in **none of the six bundled Noto fonts**, and render as *nothing* — no tofu
box. Precomposed `ë` is fine.

egui has no system-font discovery, so "not in a bundled font" is the app author's problem, not
a defect. The only arguably-reportable half is that they vanish silently rather than showing
`replacement_char`; epaint *does* have a replacement path
(`epaint-0.35.0/src/text/font.rs:770-773`, `face.glyph_info(self.cached_family.replacement_char, …)`),
so the silent drop probably happens earlier in the harfrust shaping path — I did not chase it
to a line, and I would not file without doing so. No existing issue found. **Do not file.**

### (3) Color emoji / regional-indicator flags as boxed letters — **known, already tracked**

epaint 0.35 rasterizes outlines only (no COLR/CBDT/sbix), so bundled Noto Emoji renders
monochrome and RIS pairs fall back to Noto's boxed letters. FRICTION confirms shaping itself
is *correct* (harfrust GSUB ligates the family, the skin tone and 🇺🇳 into one advancing glyph).

Tracking issue: **emilk/egui#2551 "Color Emoji support"** (open, 2023-01-06, feature).
→ **Do not file.** Corpus is accurate here.

---

## T-E — xilem/parley: regional-indicator flag pairs never ligate → tofu

### 1-2. CLAIM & EVIDENCE

`apps/xilem-babel/FRICTION.md:16`: *"country flags 🇺🇳 🇷🇸 are two pairs of tofu boxes …
Apple Color Emoji implements them via AAT tables the swash pipeline doesn't apply here."*
Same symptom independently in `apps/floem-babel/FRICTION.md:19` (parley 0.7).

### 3. ROOT CAUSE — CONFIRMED (shaping, not fallback), and the corpus's *explanation* is
roughly right but the fix was a **harfrust** upgrade, not a parley change

`parley-repro*` glyph dumps on this machine, same text, same font (Apple Color Emoji is
selected — the family and 🏳️‍🌈 in the *same run* ligate correctly):

| parley (harfrust) | `🇺🇳` glyph ids | `👨‍👩‍👧‍👦` | `🏳️‍🌈` |
|---|---|---|---|
| 0.6.0 (harfrust 0.3.2) | `[0, 0]` — two `.notdef` | `[3237]` | `[992, 3]` |
| 0.7.0 (harfrust 0.3.2) | `[0, 0]` | `[3237]` | `[992, 3]` |
| **0.8.0 (harfrust 0.5.2)** | **`[607]`** — one flag glyph | `[3237]` | `[992, 3]` |
| 0.10.0 (harfrust 0.8.4) | `[607]` | `[3237]` | `[992]` |
| main (harfrust 0.12) | `[607]` | `[3237]` | `[992]` |

So it is **not** a fontique fallback failure — the emoji family is resolved and other emoji in
the same run shape fine. It is AAT/`morx` handling of regional-indicator pairs in the shaper.
All three harfrust versions ship `src/hb/aat/layout_morx_table.rs` and gate on
`face.aat_tables.morx.is_some()` (`hb/ot_shape.rs:62`), so the 0.3.2 failure is a bug inside
that path, fixed by harfrust 0.5.2.

### 4. UPSTREAM STATUS — **fixed in parley 0.8.0 / harfrust 0.5.2** (verified empirically).

### 5. FILE? — **NO.** Same downstream-bump story as T-B: xilem 0.4 (parley 0.6) and floem
@778bb5f2 (parley 0.7) are simply behind.

### 6. Corpus correction

`report/13` footnote ⁷ and `apps/xilem-babel/FRICTION.md:16` should say
"fixed in parley 0.8 / harfrust 0.5.2 — a shaper bug in AAT `morx` handling of
regional-indicator pairs, not a fallback gap", and drop "the swash pipeline" (parley shapes
with harfrust; swash is only used for outlines/analysis).

---

## T-F — Linux: singleton emoji monochrome because Symbols2/DejaVu shadow Noto Color Emoji

### 1-2. CLAIM & EVIDENCE

`report/18-linux-reality-results.md:84-88`; `report/data/linux-rows.md:83,94`.
Retained artifact `linux-results/crop-lin-emoji.png` **does** show the split: the bare 👍 is
monochrome line art while 👍🏽 next to it is full colour, and 🏳️‍🌈 / 🇺🇳 / 🇷🇸 are colour.
(In that crop the inline 😀 looks coloured and the ZWJ family looks dark/mono, so the exact
per-emoji list in the corpus is looser than the artifact supports — the *mechanism* is right,
the enumeration "👍, 😀" should be softened to "some single-codepoint emoji, e.g. 👍".)

### 3. ROOT CAUSE — CONFIRMED, and it is cosmic-text's list, not purely a distro artifact

`cosmic-text-0.15.0/src/font/fallback/unix.rs:31-48`:

```rust
const fn common_fallback() -> &'static [&'static str] {
    &[ "Noto Sans", "DejaVu Sans", "FreeSans",
       "Noto Sans Mono", "DejaVu Sans Mono", "FreeMono",
       /* Symbols fallbacks */  "Noto Sans Symbols", "Noto Sans Symbols2",
       /* Emoji fallbacks*/     "Noto Color Emoji" ]
}
```

Static, ordered, first-font-that-has-the-codepoint wins (`font/fallback/mod.rs:91-116`
flattens the lists into one ordered range). Single-codepoint emoji that Noto Sans Symbols2
covers monochromatically are claimed before `Noto Color Emoji` is ever reached; multi-codepoint
sequences (ZWJ, skin tone, RI pairs) are not in Symbols2, fall through, and render in colour.
There is no Emoji_Presentation awareness anywhere in the file (`grep -i emoji` → only the
`"Noto Color Emoji"` entry). Debian's font set makes it visible; the ordering is cosmic-text's.

### 4. UPSTREAM STATUS — **still present on main** (`cosmic-text-main`, identical list),
and **already reported**: **pop-os/cosmic-text#327 "Inconsistent emoji rendering"** (open,
2024-11-16) — "only some emojis in color while rendering other in black/white", a regression
between 0.6 and 0.7, with linked PRs **#328** and **#524**. Also adjacent: #316 (emoji
variation sequences, "not planned"), #310, #446 (COLRv1), #499 (fontconfig aliases).

### 5. FILE? — **NO new issue.** Optionally add a comment to #327 with the exact root cause
(`unix.rs` `common_fallback()` orders Symbols2 before Noto Color Emoji; suggested fix: consult
`Emoji_Presentation` / emoji-presentation selectors before the symbol fonts, or move
`"Noto Color Emoji"` ahead of the Symbols entries for chars with default emoji presentation)
and the Debian-12 artifact link.

### 6. Corpus correction

`report/18-linux-reality-results.md:87` calls it "a distro-fonts artifact". It is **half** that:
the ordering is hard-coded in cosmic-text, so any distro that installs Noto Sans Symbols2
alongside Noto Color Emoji gets it. Reword to "cosmic-text's hard-coded Unix fallback list
places Noto Sans Symbols2 ahead of Noto Color Emoji (upstream pop-os/cosmic-text#327)".

---

## T-G — freya-edit moves the caret by UTF-16 code unit

**CONFIRMED** in 0.4.1 (see T-A). `cursor_pos()`/`len_utf16_cu()` are UTF-16 code-unit
offsets (`freya-edit-0.4.1/src/rope_editor.rs:195-208,250`, backed by `ropey 1.6.1`), Backspace
is `remove(cursor_pos-1 .. cursor_pos)` (`text_editor.rs:568`), Delete is
`remove(cursor_pos .. cursor_pos+1)` (`text_editor.rs:583`), arrows step ±1 unit. That is both
the ZWJ split *and* the surrogate-pair split — freya is the only one of the five that can leave
the buffer in a state no other framework produced (a **leading** dangling ZWJ).

**FIXED upstream**: freya PR **#2034** (7baeeee, 2026-07-30) added `grapheme_cluster_at()`;
verified present in the published **freya-edit 0.5.0-rc.1** and **0.5.0-rc.4** tarballs and
absent from **0.4.1**. Related closed issues: freya#2024 ("Better cursor rendering when
navigating emojis"), #2134 (later clamp fix). **Do not file.**

**Windows cross-platform note (for the cosmic-text issue, not freya):**
`report/data/windows-rows.md:89` records iced-babel's selftest on Windows 11 returning
`"a👨‍👩‍👧‍" (len 7) -> CORRUPTED/partial` after one Backspace — the same cosmic-text defect,
different OS. Screenshot: `measurements/reruns/.../windows/babel-shots/iced-babel.png`.
Worth one line in the cosmic-text issue body (already included above).

---

## Summary of dashboard corrections

1. "five of the ten editing paths" is right; name them (iced, egui, xilem, vizia, freya).
2. freya's ZWJ corruption is **fixed** in freya-edit 0.5.0-rc.1 (PR #2034).
3. parley's ZWJ backspace is **fixed on main** (PR #715, unreleased); flags **fixed in 0.8.0**.
4. fontique's macOS Han/symbol blind spot is **fixed in 0.8.0** (#536/#568/#598) — the
   remaining item is "xilem 0.4 and floem pin old parley", not a fontique bug.
5. The AssetsV2 explanation is wrong — PingFang(UI) has no readable font file; upstream's own
   comment cites `hvgl` outlines.
6. floem pins fontique/parley **0.7.0**, not 0.6.
7. xilem-grid's ▲/▼ tofu and floem-tray's ⌘/⇧ tofu are the **same** fontique root cause.
8. The xilem scene is ~161 MiB by my reconstruction, not 192 MiB; and the 128 MiB ceiling is
   `Limits::default()`, not the hardware (this M4 Pro allows 4095 MiB).
9. Linux emoji mono/colour split is cosmic-text's hard-coded fallback order (already reported
   as #327), not only a distro artifact; soften "👍, 😀" to match the retained crop.
10. egui's RTL/AX and combining-mark findings are consequences of tracked limitations
    (#1016, #2551) — no new issues.

## Issues to file (4)

| # | repo | title | confidence |
|---|---|---|---|
| 1 | pop-os/cosmic-text | Backspace deletes one `char`, Delete deletes one grapheme cluster | CONFIRMED, still on main, no duplicate |
| 2 | vizia/vizia | `backspace_offset` never advances its cursor → emoji state machine is dead code | CONFIRMED (one-line fix), still on main, no duplicate |
| 3 | linebender/vello | Large scenes hit `max_storage_buffer_binding_size` because the device uses `Limits::default()` | CONFIRMED mechanism + GPU-free repro, no duplicate |
| 4 | linebender/xilem (masonry) | `render_text` encodes every line, no viewport culling | CONFIRMED, no duplicate |

Optional 5th (lower value): linebender/parley — `backdelete()` still splits non-emoji-range
grapheme clusters (RI flag pairs, Indic conjuncts) even after #715; relates to #694.
