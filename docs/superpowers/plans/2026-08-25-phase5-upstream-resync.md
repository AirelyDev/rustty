# Phase 5 — Upstream Resync Master Plan (pin `12967b68f` → head `683d8db64`)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. In this program the execution vehicle is the Workflow tool with worktree-isolated agents and patch harvest (`.port/phase-5/wf-*.js`), exactly as Phase 4 ran. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Absorb `12967b68f..683d8db64` (245 commits, 169 non-merge, 155 files, +25,534/−3,360) with zero deferrals, ending with G1b (295 dropped tests today) and G3b GREEN again, both platforms.

**Architecture:** Nine workstreams in four layers. The range is dominated by **three new kitty subsystems** — the OSC 5522 clipboard protocol (the mega-bundle: it spans all five areas), the OSC 72 drag-and-drop rework, and graphics animation — plus a graphics spec-conformance wave that REWRITES files Phase 4 just ported, OSC 99 desktop notifications, and a macOS KeyboardLayout/AppleScript wave. Layer 1 is leaf prerequisites; layer 2 is the kitty/vt mass; layer 3 is C-API + UI tiers; layer 4 the finale.

**Tech stack / constraints:** identical to Phase 4 — see `docs/superpowers/plans/2026-08-17-phase4-upstream-resync.md` Global Constraints (binding here verbatim: range-diff END STATE, R1/R2/R5/R7/R9/DG5, SB raw-ptr discipline with Miri proofs, deletion-with-consumer, escalate upstream bugs, bookkeeping greens are not coverage). **The per-commit spec is `.port/phase-5/assessments/{A-kitty-subsystems,B-terminal-core,C-capi-lib,D-ui-tiers,E-platform-tail,RECONCILIATION}.md`.**

## Range-diff traps (named by the assessments — do NOT port intermediates)

- `5350d4a5f` clipboard truncation engine + 4 tests **REVERTED** by `70f006575` (reject + keep limit plumbing); `7a940ec02` drop-invalid-chunk **REVERTED** by `bc2f7d7d2` (abort; 3 tests renamed dropped→reported).
- `4b915e3bc` C-key acceptance **REVERTED** by `f3e98fb72`; `b56c6d88f` reverted in-range.
- `db2f8be59` supersedes DnD docs; the macOS `f6990690f` revert inside bundle D-B5.
- Gate-invisible class: `4b915e3bc`/`322d7ae78`/`f3e98fb72`/`83145c0a3`/`242723223` amend EXISTING test bodies; `f8b40a023` **DELETES** a pin-era test rustty carries ("storage: delete all placements and images") — replace, don't keep.

## SB/UB hazards (LIFETIMES rows + Miri harnesses required)

1. **`90bce0d2d` ScrollMargins guard** holds `*Screen` AND `*Pin` pointers into the placements map across Terminal scroll mutations (call-end-exactly-once contract; `*Pin` dangles if the map rehashes) — raw-ptr guard + `.port/phase-5/mirihar-scroll-margins`.
2. **~19 `s: *Screen` sites** in graphics_storage (self-alias) — the Phase-4 raw-ptr pattern.
3. **Animation exec** holds map-interior image pointers across storage mutators that can evict; resolveChain interleaves placement walks with removal — collect-keys-first, never removeByPtr-during-iteration.
4. **`animationTick` threading**: render thread vs io-side exec mutation — safe only under the renderer/terminal mutex; document and enforce.
5. **Clipboard WriteState** holds simd base64 Streaming decoder state across OSC packets — owned decoder state + per-call `&mut` output, no held borrow; EOF-at-padding restart is tested behavior.

## Workstreams

### `P5-W1` — leaf prerequisites (first)

1. **simd base64 Streaming** decoder (+349, `src/simd/base64.zig`) — hard prereq for clipboard write (B-3; strict/streaming split). 2. **wuffs swizzle** (`pkg/wuffs/src/swizzle.zig` rgbToRgba/gToRgba/gaToRgba/rgbaSrcOver + the vendored stdlib.h/string.h shim headers) into wuffs-sys — prereq for graphics_pixel. 3. **paste core**: NEW `src/terminal/paste.zig` + `src/input/paste.zig` (input_paste drop=7). 4. **`src/terminal/sys.zig` randomSecure** override + os glue (OTP entropy, `dda8e6f31`). 5. **config clipboard write-limit** option (E bundle KC's config half).

### `P5-W2` — kitty graphics (assessment A B1–B4)

1. Spec-conformance END-STATE wave (13 commits; exec 12→66 / command 32→38 / image 25→27 / storage 38→71 tests; `242723223` lenient parsing replaces rustty's hard-error at graphics_command.rs:1139; replace the `f8b40a023`-deleted test; DG2 anchor: max_dimension-before-multiply survives). 2. Relative placements + placeholder lookup (tagged PlacementId, resolveChain, cascaded deletion; renderer + c/kitty_graphics halves ride with W6/W7). 3. Margin-aware scrolling (`90bce0d2d`, hazard #1, Terminal.zig +134 call sites, 11 tests). 4. Animation (NEW graphics_animation/graphics_pixel, exec +1485, gap_ms i32 bit-pattern + Background b,g,r,a order golden-byte tests; needs W1.2; animationTick drive is W7's renderer half).

### `P5-W3` — kitty clipboard OSC 5522 (the mega-bundle; assessments A B5 + B B-1 + C/D/E halves)

Stage 1 (vt): `kitty/clipboard{,_command,_grants,_response,_write}.rs` END STATE (61 named tests; reject-not-truncate; abort-not-drop), osc parser hunks, PasteEvent glue. Stage 2 (dispatch): stream_terminal halves (the bulk of drop=38), Surface.zig arms, termio halves. Stage 3 (platform, with W6/W7): C effects (reply-based ClipboardWrite REWORK), embed ABI layout change (content len — the E2 bundle; positional-ordinals + A4 discipline), config option wiring, macOS/apprt halves.

### `P5-W4` — kitty DnD OSC 72 (assessment A B6)

`kitty/dnd{,_command,_drop,_response}.rs` + `dnd_test` kitten-0.47 replay (43 tests) **ATOMIC WITH** gutting `osc/parsers/kitty_dnd_protocol.rs` to the 138-line end state (6 tests migrate) — two state machines must never coexist.

### `P5-W5` — OSC 99 desktop notifications + osc metadata (assessment B B-2)

9 commits: NEW `osc/parsers/kitty_desktop_notification.zig` + `osc/kitty_metadata.zig` (osc drop=47 closes here + B's osc churn).

### `P5-W6` — C-API (assessment C; after W2/W3 vt-sides)

Bundles P (terminal_paste streaming C API + MimeReader), C (clipboard read/write effects), E2 (embed clipboard ABI — a LAYOUT change to content_s: byte-parity gates re-pinned in the same motion), KG (graphics C reads: sourceRect, relative-placement resolve), sys.h/c/sys.zig (randomSecure). A5 floor/registry re-float with measured counts; arm (d) delta enumerated by the assessment.

### `P5-W7` — UI tiers (assessment D)

macOS clipboard halves (D-B1), renderer GPU-release-for-hidden-surfaces + animation clock (D-B2 incl. the head commit), placement-geometry renderer half (D-B3), macOS menu-shortcut/KeyboardLayout wave (D-B5 incl. the `f6990690f` revert; carry ScriptKeyEventCommandTests + KeyboardLayoutTests), apprt tail. Gates: xcodebuild test; menu-shortcut-parity harness (embed-abi memory).

### `P5-W8` — platform tail (assessment E)

po re-mirror (4 catalogs, Phase-4 method), cli, termio non-kitty, os, .github, typos, valgrind.supp, pkg/afl.

### `P5-W9` — finale

G1b GREEN (295→0: kitty 202, osc 47, stream_terminal 38, input_paste 7, parser 1), G3b sweep GREEN, A5/A3/A4/G7/G-BENCH/D11, Miri harnesses, macOS full + xcodebuild + Linux `rvm` full lanes, ledgers + ORACLE_SYNC finale note, census, worktrees swept, push `phase-5-resync`.

## Self-review

Every assessment cluster maps to a workstream (A B1→W2.1, B2→W2.2, B3→W2.3, B4→W2.4, B5→W3, B6→W4, B7→W1.3/W1.4; B B-1→W3, B-2→W5, B-3→W1.1, B-4→W2.1/W8; C P/C/E2/KG/sys→W6; D B1→W7, B2→W7, B3→W7, B5→W7; E KC→W3.3/W1.5, po→W8, cli/termio/os→W8). G1b drop map: kitty→W2+W3+W4, osc→W5, stream_terminal→W3.2, input_paste→W1.3, parser→W5. Reconciliation orphans (10, assessed inline) ride their assigned areas' workstreams.
