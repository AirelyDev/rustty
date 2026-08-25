# Phase 5 assessment — area A: kitty subsystems

**Range:** `12967b68f` → `683d8db64` (169 non-merge commits total; **39** touch my file set).
**Scope (oracle paths, as assigned):** `src/terminal/kitty/**` (the directory only — the
osc parsers `kitty_clipboard_protocol.zig`/`kitty_dnd_protocol.zig`, `stream_terminal.zig`,
`src/terminal/paste.zig`, `src/terminal/sys.zig`, `simd/base64.zig`, `pkg/wuffs`,
`Terminal.zig`, `Surface.zig`, config/C-API/renderer halves are named as couplings, not
verdicted here).
**Oracle state:** working tree checked out AT the new pin `683d8db64`; end-state files read
directly, per-commit hunks and the old pin via `git show 12967b68f:<path>`.
**Rustty side:** `crates/rustty-vt/src/kitty/` (10 `.rs` files, Phase-4 finale shape),
plus the coupled `crates/rustty-vt/src/osc/parsers/kitty_dnd_protocol.rs`.
**No code modified.** Assessment only.

**Range mass in my set:** 18 files, **+10,679 / −465** (`git diff --stat` on
`src/terminal/kitty`). Five NEW clipboard files (1,989 end-state lines), five NEW dnd
files (2,094), NEW `graphics_animation.zig` (149) + `graphics_pixel.zig` (235), and heavy
churn to all four big graphics files Phase 4 just ported
(`graphics_exec.zig` +2,951-line delta, `graphics_storage.zig` +2,664,
`graphics_command.zig` +786, `graphics_image.zig` +244).

---

## 0. Coverage honesty — exhaustive vs sampled

**Exhaustive:**

- Commit enumeration: `git log --oneline --no-merges 12967b68f..683d8db64 --
  src/terminal/kitty` — all 39 commits listed and verdicted below; none skipped.
- Per-commit test inventory: `git show <c> -- src/terminal/kitty | grep -E '^[+-]test '`
  run for **every** commit — the §5 dropped-test map is built from that full extraction,
  including the 4 in-range test *deletions* (`70f006575`) and 6 in-range test *renames*
  (`bc2f7d7d2`/`e8d8945b5` supersede `25b1170d4`/`4f49dc2b8`/`7a940ec02` names).
- Per-file `^test ` counts at both pins (all 21 head files vs all 9 pin files) and
  `#[test]` counts for all 10 rustty files — the §5 arithmetic rests on these.
- Supersede analysis: `f3e98fb72` and `db2f8be59` diffs read **in full**; the
  `5350d4a5f`→`70f006575` truncate→reject reversal traced test-by-test; pin-era
  `AnimationFrameLoading.parse` read in full and diffed against head to separate
  pin-era behavior from in-range churn.

**Sampled (structure-level, not hunk-by-hunk):** the interior of the five clipboard and
five dnd files and the ~5,600 lines of graphics_{exec,storage} churn were read as: full
module doc-comments (`clipboard.zig`, `dnd.zig` — both read start to finish), all
`pub fn`/`pub const` surfaces, every `*Screen`/`*Terminal` parameter site in
`graphics_storage.zig`, `scrollMarginsBegin` + the `ScrollMargins` struct in full,
`dnd_drop.zig`/`clipboard_grants.zig`/`clipboard_write.zig`/`graphics_pixel.zig` key
signatures and decoder fields, and `4b915e3bc`'s command-parsing diff (~80 lines).
Individual test bodies of the 200+ new tests were NOT read; they are inventoried by name.
This is a re-vetting assessment, not the port itself — the porting workflow must read
every hunk.

**Rustty evidence discipline:** every ABSENT verdict is backed by a grep/read of the
corresponding rustty file showing the pre-range shape (cited below), not by hash
absence. Negative controls actually run:
`grep -rn 'scroll_margins|resolve_chain|animation_tick|placeholder_target|ScrollMargins' kitty/`
= **0 hits**; no `clipboard*.rs`/`dnd*.rs`/`graphics_animation.rs`/`graphics_pixel.rs`
exist in `crates/rustty-vt/src/kitty/` (ls); `kitty/mod.rs:15-33` declares only the 9
pin-era modules; `grep 'intersect|clamp|tall'` on storage/exec finds only pin-era
comment text. Positive control (proves the grep method sees what IS there):
`graphics_exec.rs:205` still carries the pin-era `"EINVAL: image ID and number are
mutually exclusive"` message, which `git show 12967b68f:…/graphics_exec.zig:148`
confirms is the **pre**-`83145c0a3` shape. Nothing was executed; `cargo` was not run.

---

## 1. Summary

| Verdict | Commits | Notes |
|---|---|---|
| **PORTED** | **0** | nothing from this range has landed in rustty's kitty tree |
| **PARTIAL** | **0** | rustty is uniformly at the Phase-4 finale (= old pin `12967b68f`) shape; no upstream backport matched |
| **ABSENT** | **37** | everything else, including 4 commits whose own tests/logic were partially superseded in-range (`25b1170d4`, `7a940ec02`, `4f49dc2b8`, `5350d4a5f`) — port their END STATE, noted per-row |
| **N-A** | **2** | `db2f8be59` (deletes 23 doc lines added in-range by `7c845e8af`/`50f69b883`; fold into the dnd end state), `f3e98fb72` (deletes the C-key composition-mode acceptance added in-range by `4b915e3bc`; rustty never had it — assess under the animation bundle end state) |

**Highest-severity findings:**

1. **Two whole protocols are absent**: Kitty clipboard (OSC 5522: command parsing,
   response encoding, write transactions with configurable byte limit, session-password
   grants with OTP, read-via-effect, paste events) and Kitty drag-and-drop (OSC 72:
   command decoding, response encoding, drop state machine + data serving). ~4,100
   end-state lines + 107 tests in my directory alone, plus large stream/C-API halves.
2. **The graphics subsystem Phase 4 just ported was substantially rewritten**:
   animation execution (frames, composition, playback, tick scheduling), relative
   placements (parent chains, cascaded deletion), margin-aware placement scrolling
   (a two-phase `ScrollMargins` begin/end guard called from `Terminal.zig`), auto-ID
   collision fixes, and a 12-commit spec-conformance wave. `graphics_exec` tests go
   12→66 upstream while rustty still has 12.
3. **In-range reversals are a live porting trap** (the reason the range-diff rule
   exists): write-limit **truncation** semantics (`5350d4a5f`) were replaced by
   **rejection** (`70f006575`) three days later — porting commit-by-commit would build
   and then delete a truncation engine and 4 tests; base64 "drop invalid chunk and
   continue" (`7a940ec02`) became "abort the transaction" (`bc2f7d7d2`); the
   `X`-or-`C` composition-mode acceptance (`4b915e3bc`) was removed (`f3e98fb72`);
   `AnimationFrameLoading.Background` field order flipped r,g,b,a → b,g,r,a in-range.
4. **One in-range test deletion targets a PIN-era test rustty carries today**:
   `f8b40a023` deletes `storage: delete all placements and images` (rustty has its
   port among the 38 `graphics_storage.rs` tests) and replaces it with 5
   scrollback-preserving variants. G1b sees only net counts — the stale test body
   must be replaced, not merely supplemented.
5. **`ScrollMargins` is a first-class SB hazard**: a guard object holding
   `screen: *terminal.Screen` **and** `pin: *PageList.Pin` pointers into
   `ImageStorage.placements` map values, kept alive across the scroll's row
   mutations, with a must-call-`end()`-exactly-once contract (§4.1).

---

## 2. Per-commit verdicts

Ordered oldest→newest within each cluster. "rustty evidence" cites the pre-range shape
that proves ABSENT. Where several commits rewrite the same code, the cluster is one
range-diff end-state unit (§3).

### 2.1 Graphics spec-conformance wave — 12 commits (2026-08-18/20), all ABSENT

All four graphics files in rustty are at pin shape: `graphics_command.rs` 32 tests /
`graphics_exec.rs` 12 / `graphics_image.rs` 25 / `graphics_storage.rs` 38 — exactly the
Phase-4 finale counts vs upstream head 38/66/27/71.

| upstream | what changed | rustty evidence | new tests to carry |
|---|---|---|---|
| `afb61e1b6` place cursor after tall images properly | exec `display` cursor movement after images taller than the screen (+104 exec) | no tall-image handling in `graphics_exec.rs` (grep `tall` = 0; display path is pin shape) | `kittygfx placement moves cursor past a tall image` |
| `83145c0a3` reject conflicting image identifiers | conflict (i vs I) rejection moved **before mutation** and generalized to every action; graphics_command +418/−160 restructure | `graphics_exec.rs:205` still the pin-era post-hoc check (positive control, §0) | `kittygfx image id and number are mutually exclusive for every action`, `…rejected before mutation` |
| `52190a5d8` intersect source rectangles before sizing | storage intersects the source rect with the image before computing dest sizing; renderer/image.zig + c/kitty_graphics.zig halves elsewhere | no `intersect` in `graphics_storage.rs` rect/sizing path (grep hits are pin-era delete-intersecting comments only) | `storage: default source rectangle is intersected before sizing`, `…explicit…` |
| `c5a3c7e2e` constrain placement offsets to cell bounds | x/y cell-offset clamping in storage + exec display | no offset clamp (grep `clamp` in storage = 1 pin-era comment at `:3868`, different feature) | `kittygfx display clamps cell offsets`, `storage: cell offsets stay within explicit destination rectangle`, `…clamp to cell bounds` |
| `abd77067d` graphics `S` value is exact byte, not max | file loads read **exactly** S bytes, reject short data (+117 image) | `graphics_image.rs` file-size path is pin shape (25 tests = pin 25) | `image load: regular file size reads exactly requested bytes`, `…rejects short data` |
| `e5747cf0b` abort incomplete graphics loads | delete/retransmit aborts an in-flight chunked load (+112 exec) | absent (exec 12 tests = pin 12) | `kittygfx delete aborts chunked image load`, `kittygfx retransmit same id removes image on first chunk` |
| `f8b40a023` fix various graphics deletion mismatches with spec | delete-all keeps scrollback placements; erase-display reclaims unplaced images; **deletes pin-era test** `storage: delete all placements and images`; Terminal.zig half (+12) | rustty carries the ported deleted test among its 38; none of the 5 replacement tests exist | `storage: delete all visible placements and matching images`, `…preserves scrollback`, `…includes placements spanning into active area`, `storage: uppercase range deletes unplaced image data`, `storage: erase display preserves scrollback and reclaims unplaced images` |
| `cfc5a9650` validate graphics query image data | `q` query decodes/validates payload without storing (+81 exec) | absent | `kittygfx query validates image data`, `kittygfx valid query does not replace or store image` |
| `86d94f150` preserve chunked response identifiers | responses to chunked loads echo the FIRST chunk's i/I/p (+9 image: identifiers stashed on the loading image) | absent | `kittygfx chunked success response uses initial identifiers`, `…error response…` |
| `306a16903` preserve data on unmatched delete | uppercase-id delete frees the image only when a placement matched; unplaced-image rules | absent | 1 exec + 3 storage tests (`…preserves image when placement does not match`, `…frees image after placement matches`, `…without placement frees unplaced image`) |
| `f7d29b19e` accept empty graphics delete ranges | empty x..y range parses and deletes nothing (command +70, storage +49) | absent | `delete range command 6`, `storage: delete images by empty range` |
| `242723223` fix various validation behaviors to match Kitty | lenient value handling: unknown format deferred to execution (EINVAL there), zero format = rgba, quiet>2 suppresses all, cursor>1 moves, virtual>1 virtual, X>1 alpha-blends, compose>1 overwrites, control>3 ignored; out-of-range display keys tolerated (command +195, exec +112) | rustty `graphics_command.rs:1139-1142` still hard-errors on X∉{0,1} (pin shape) | 10 command + 5 exec tests (names in §5) |
| `6b23c584c` prevent auto-assigned image ID collisions | `nextImageId` skips in-use ids and zero; number-based transmission takes smallest free id, never replaces a client image (exec +176, storage +94) | `graphics_exec.rs:576-578` still plain `wrapping_add(1)` on `next_image_id`; `graphics_storage.rs:175` pin init `2147483647` | `kittygfx number-based transmission assigns smallest free id`, `…does not replace client image` ×2, `storage: nextImageId number matches Kitty get_free_client_id`, `…implicit skips in-use ids and zero` |

### 2.2 Placement engine: relative placements, placeholder lookup, margin scrolling — 4 commits (2026-08-20), all ABSENT

| upstream | what changed | rustty evidence | new tests |
|---|---|---|---|
| `90bce0d2d` scroll and clip image placements within margins | `scrollMarginsBegin(io, t, delta, op) → ScrollMargins` two-phase API (storage +685) called from `Terminal.zig:2496` scroll paths (+134, terminal-core area); virtual/relative placements never adjusted; clipping, proportional scaling, inside-placement deletion on large scrolls | no `ScrollMargins`/`scroll_margins` anywhere in rustty kitty (grep = 0); rustty `Terminal` scroll paths don't call storage | 11 `storage: scroll margins …` tests + `storage: scroll without margins moves placement into scrollback` |
| `6b23c584c` | (verdicted in §2.1; storage half shares this cluster's files) | | |
| `a32a100d4` centralize placeholder placement lookup | `placeholderTarget` on storage (+111); `graphics_unicode.zig` −22 delegates to it | no `placeholder_target` (grep = 0); `graphics_unicode.rs` 16 tests = pin 16, pin shape | `storage: placeholderTarget lookup` |
| `cd5f9eef0` add relative placement storage | `PlacementId` internal/external tagged namespace with `preferredOver`, parent links, `resolveParent`/`resolveChain` (storage `:738`/`:846`), orphan removal on eviction (+529 storage); renderer/image.zig + c/kitty_graphics.zig halves | no `resolve_chain`/`resolveParent`/tagged `PlacementId` (grep = 0; rustty `PlacementKey` is the pin 2-field struct) | `storage: resolveChain accumulates offsets to the pin root`, `…finds virtual roots`, `storage: eviction removes orphaned relative placements` |
| `33ca9e5dd` create relative placements from put commands | exec put path creates relative placements: missing-parent ENOPARENT, self/cycle/depth-limit rejection, transitive deletion, parent fallback picks lowest external id, retained across full reset (+543 exec) | absent (exec pin shape) | 13 `kittygfx relative placement …` tests |

### 2.3 Animation — 5 commits (2026-08-20/21): 4 ABSENT, 1 N-A

| upstream | verdict | what changed | rustty evidence | new tests |
|---|---|---|---|---|
| `242723223` (animation-value arms) | (§2.1) | | | |
| `4b915e3bc` complete animation command parsing | **ABSENT** | `AnimationFrameLoading` gains a full embedded `Transmission` (frame data transmitted like image data: t/f/s/v/m keys); `gap_ms` u32→**i32** (negative = gapless) reconstructed from the KV bit pattern; `Background` field order becomes **b,g,r,a** (little-endian bit-cast doc); `Response.frame` echoed as `r=`; control/compose parse become infallible; **amends existing pin-era parse arms — no new `^test`, gate-invisible** | `graphics_command.rs:1082-1145` is the pin struct: no transmission field, `gap_ms` u32-ish via `kv.get`, X hard-errors, r,g,b,a order | — (assertion changes ride existing tests) |
| `d8b920e50` animation frame storage, composition, and playback | **ABSENT** | NEW `graphics_animation.zig` (Animation: frames, gaps, loop budget, generation) + NEW `graphics_pixel.zig` (rgba conversion, fill, compose-rect with clipping, alpha blend); storage `animationTick(io, now_ms) → ?u64` scheduler (`:1057`), `reserveAnimationBytes`/`releaseAnimationBytes` quota; image gains `animation: ?Animation` (+101) | no `graphics_animation.rs`/`graphics_pixel.rs` (ls); no `animation_tick` (grep = 0) | `animation gap helpers` + 6 pixel tests + 5 `storage: animation tick …` tests |
| `d2ffeba` execute animation commands | **ABSENT** | exec: `transmitAnimationFrame`/`completeAnimationFrame`/`controlAnimation`/`composeAnimation`/`ensureAnimation` (+1,485); pin-era exec returned unsupported for these actions | `graphics_exec.rs:97-102` still the pin-era commented-out/unsupported arms for `TransmitAnimationFrame`/`ControlAnimation`/`ComposeAnimation` | 23 `kittygfx animation: …` tests (minus the `f3e98fb72` assertion trims) |
| `f3e98fb72` X handling properly since 0.45 fix | **N-A** (supersede) | removes the C-key acceptance in `AnimationFrameLoading.parse` and its test block/assertion — code added in-range by `4b915e3bc`; final comment: "Tested only against 1 like Kitty." | rustty never had the C-in-AnimationFrameLoading code (`graphics_command.rs:1138` X-only) | end state carried by the bundle: X lenient (`v==1` overwrite else alpha_blend), NO C key, final comment text |
| `322d7ae78` switch to wuffs for pixel work | **ABSENT** | `graphics_pixel.zig` rewritten over `wuffs.swizzle.{rgbToRgba,gToRgba,gaToRgba,rgbaSrcOver}` (pkg/wuffs/src/swizzle.zig NEW, +53 — wuffs area); build_options/GhosttyZig `-Dvt-features` plumbing; **no test-name changes** — behavior pinned by the `d8b920e50` pixel tests | `graphics_pixel.rs` does not exist at all; rustty wuffs module has no swizzle entry points | — (port pixel end state = wuffs-backed directly; never port the 3-day-old hand-rolled intermediate) |

### 2.4 Kitty clipboard protocol (OSC 5522) — 10 commits (2026-08-21/25), all ABSENT

The five files are entirely new; rustty has only the Phase-4 `osc/parsers/kitty_clipboard_protocol.rs`
capture layer (`osc/mod.rs:317`) that this module consumes. Port the file-set END STATE;
the per-commit rows exist for test attribution and supersede notes.

| upstream | what changed | superseded-in-range notes |
|---|---|---|
| `25b1170d4` command parsing | NEW `clipboard.zig` + `clipboard_command.zig`: Metadata/Payload decode, id sanitize/truncate (512), base64 mime/name/pw, silent-drop rules modeled on kitty/clipboard.py | its tests `metadata: invalid mime base64 dropped` / `…utf8 dropped` are **renamed+repolarized** to `…reported` by `bc2f7d7d2`/`e8d8945b5` |
| `e28acd928` response encoding | NEW `clipboard_response.zig`: status/DATA/DONE packets, key order type,status,loc,id,mime,pw, `read_chunk_size` chunking, targets (`.`) listing always sent | — |
| `7a940ec02` write transactions | NEW `clipboard_write.zig`: WriteState, mime spools, aliases resolve at commit | `write: invalid base64 chunk is dropped, transaction continues` **deleted** by `bc2f7d7d2` (abort semantics) |
| `6f007e767` session password grants | NEW `clipboard_grants.zig`: per-direction grants, one-time consumption, capacity eviction | — |
| `4f49dc2b8` reads via clipboard_read effect | command changes (+51/−58) + big stream_terminal/c/terminal halves (areas: stream, C) | its `metadata: over-long name dropped` renamed to `…reported` by `e8d8945b5` |
| `928c7f0e7` exempt listing reads from prompts | `readPromptExempt` (command +16); Surface/stream/termio halves | — |
| `5350d4a5f` write limit configurable | limit plumbed from config/C API; **truncate-at-limit** engine + 7 tests | 4 truncation tests + the truncation engine **deleted** by `70f006575`; only `write: default limit when unset`, `…non-text under limit is unaffected`, `…non-text over limit rejects transaction` survive; port the limit *plumbing* (config/C-API surface) with reject semantics only |
| `70f006575` reject oversized writes | truncation → **rejection** (−150 net in write); config default rename; c/terminal half | end-state authority for limit semantics |
| `e8d8945b5` text input validation to spec | decoded text/alias payloads must be valid UTF-8, abort with EINVAL; over-long name **reported** not dropped (command +199/−?) | — |
| `bc2f7d7d2` base64 handling to spec | strict RFC 4648 everywhere; one streaming base64 stream per MIME across packets via `simd.base64.Streaming` (`clipboard_write.zig:59`) — NEW simd API (+349 `simd/base64.zig`, simd area); EOF-at-padding restarts the stream so independently-encoded chunks work; Surface/stream halves | end-state authority for base64 semantics |

The 40-line `clipboard.zig` module doc (read in full) is the normative behavior summary
— R1: port it verbatim (it is technical, no gag prose).

### 2.5 Kitty drag-and-drop (OSC 72) — 5 commits (2026-08-21/22): 4 ABSENT, 1 N-A

| upstream | verdict | what changed | notes |
|---|---|---|---|
| `7c845e8af` command decoding | **ABSENT** | NEW `dnd.zig` + `dnd_command.zig`: EventType/Metadata/Operations/Request classification, chunk reassembly reusing first-chunk metadata | 11 tests |
| `38746b8c1` response encoding | **ABSENT** | NEW `dnd_response.zig`: encode/encodeError, RequestKeys non-zero-only | 6 tests |
| `50f69b883` drop state machine | **ABSENT** | NEW `dnd_drop.zig` (715: State/DropTarget/Item/MoveEvent, dragMove/dragDrop/dragLeave, data serving, 16-item/1 MiB caps) + NEW `dnd_test.zig` (26 tests incl. `dnd: kitten 0.47 conversation replay`); **shrinks `osc/parsers/kitty_dnd_protocol.zig` 363→138 lines, 11→5 tests** — the pin-era in-parser state machine moves here | rustty `osc/parsers/kitty_dnd_protocol.rs` still carries the pin shape (11 `#[test]`) — the port must gut that file in the same change (osc-area coupling, §6) |
| `af8d28a94` stream handler | **ABSENT** (my slice is 2 lines) | dnd_drop tweak (+2/−1... `:2,-1`); the mass is stream_terminal (+253) / Terminal.zig (+10) / stream.zig — stream area | its tests are in `stream_terminal.zig` (part of that module's drop=38) |
| `db2f8be59` dnd docs | **N-A** (supersede) | deletes 23 doc lines from `dnd.zig` added earlier in-range | fold into the dnd.zig end state (69 lines, read in full: Divergences section — eager data capture, all-clients-local, no drag-out, terminator-echo divergence marked "on purpose forever") |

### 2.6 Paste events + secure random — 2 commits (2026-08-22), both ABSENT (my slices)

| upstream | what changed (my files) | cross-area mass |
|---|---|---|
| `a5bb22e23` shared paste core with paste events | `clipboard_response.zig` +84 (`PasteEvent`: pw in every packet, primary only on OK, empty listing packet still sent); `clipboard_grants.zig` +46 (`generateOtp` alphabet/length, no-entropy-is-error); `clipboard.zig` +2 re-exports | NEW `src/terminal/paste.zig` (+185) + `src/input/paste.zig` (+168) — the input_paste drop=7 module; lib_vt/main wiring (area C) |
| `dda8e6f31` sys secure random override | `clipboard_grants.zig` +24: `generateOtp(io: std.Io)` uses `sys.randomSecure` override else the Io source | NEW `src/terminal/sys.zig` (+48) + `c/sys.zig` + `include/ghostty/vt/sys.h` + TinyIo `RandomSecure` (+53) — area C/os owns; my `generateOtp: sys override supplies entropy without an Io source` test depends on it |

---

## 3. Atomic bundles

1. **B1 graphics spec-fix wave** (`afb61e1b6`, `83145c0a3`, `52190a5d8`, `c5a3c7e2e`,
   `abd77067d`, `e5747cf0b`, `f8b40a023`, `cfc5a9650`, `86d94f150`, `306a16903`,
   `f7d29b19e`, `242723223`, `6b23c584c`): `graphics_command.zig`'s parse was
   restructured twice (`83145c0a3` −160, `242723223`) and the delete paths three times
   (`f7d29b19e`/`306a16903`/`f8b40a023`) — port the four-file END STATE as one unit,
   carrying all ~40 tests and **replacing** the `f8b40a023`-deleted pin-era test.
2. **B2 relative placements** (`cd5f9eef0` → `33ca9e5dd`, + `a32a100d4`): storage
   structures before exec creation paths; `graphics_unicode.zig` delegation rides
   along. Renderer/image.zig + c/kitty_graphics.zig halves must land with it or the
   render side can't see parent-resolved coordinates (areas: renderer, C).
3. **B3 margin scrolling** (`90bce0d2d`): inseparable from its `Terminal.zig` +134
   half (terminal-core area) — the two-phase begin/end contract spans both.
4. **B4 animation** (`4b915e3bc` → `d8b920e50` → `d2ffeba`, end state incl.
   `f3e98fb72`'s trims and `242723223`'s lenient arms; `322d7ae78` pixel-on-wuffs is
   the pixel end state): frame parsing feeds storage feeds exec; the renderer
   `animationTick` call site (`renderer/generic.zig:1388`) is the consumer.
   `graphics_pixel` must be ported wuffs-backed directly (never the 3-day
   intermediate), which puts `pkg/wuffs/src/swizzle.zig` (wuffs area) on the
   critical path.
5. **B5 clipboard protocol** (`25b1170d4` → `e28acd928` → `7a940ec02` → `6f007e767`
   → `4f49dc2b8` → `928c7f0e7` → `5350d4a5f` → `70f006575` → `e8d8945b5` →
   `bc2f7d7d2`): one five-file end state. Sub-ordering that stays honest: the
   command/response/write/grants core + tests can land against a stub effect layer,
   but limit plumbing (config + C API), the clipboard_read effect, and the paste
   events need their cross-area halves. **Never port `5350d4a5f`'s truncation or
   `7a940ec02`'s drop-invalid-chunk semantics** — both superseded in-range.
6. **B6 dnd** (`7c845e8af` → `38746b8c1` → `50f69b883` end state incl. `db2f8be59`
   docs; `af8d28a94` stream half with the stream owner): MUST include the
   `osc/parsers/kitty_dnd_protocol.rs` 363→138-equivalent shrink in the same change,
   or rustty ends up with two dnd state machines.
7. **B7 paste/OTP glue** (`a5bb22e23` + `dda8e6f31`): my response/grants slices land
   with the paste-core (input area) and sys/TinyIo (area C/os) halves.

Recommended order: B1 → B2 → B3 → B4 (graphics restore parity first — they rewrite
files rustty already has, so drift compounds); then B5/B6/B7 (new subsystems, additive).

---

## 4. Aliasing / Stacked-Borrows hazards (raw-ptr Page/Row/Cell/Screen discipline)

1. **`ScrollMargins` guard object (B3) — the worst one.**
   `scrollMarginsBegin` (`graphics_storage.zig:459`) returns a struct holding
   `screen: *terminal.Screen` AND `restores: []{pin: *PageList.Pin, x, y}` where each
   `pin` points INTO a `Placement` value inside `self.placements` — held live across
   the caller's (`Terminal.zig:2496`) row-mutation phase, then `end()` re-derives
   pins via `screen.pages.pin(…)`. Three separate hazards for the Rust port:
   (a) `&mut Screen`/`&mut Terminal` is active between begin and end while the guard
   holds a Screen-derived pointer — the begin/end pair must traffic in raw pointers
   (`NonNull<Screen>`, `NonNull<Pin>`) per `rust-mut-args-strongly-protected`, with
   derefs only inside `end()`; (b) the `*Pin` pointers dangle if `placements`
   rehashes between begin and end — upstream's implicit contract (no storage mutation
   in between) must be written down and, ideally, encoded (e.g. store `PlacementKey`
   + re-lookup in `end()` ONLY if the behavior is provably identical — that is a
   deviation to argue, not assume); (c) `end()` must be called exactly once —
   Zig gets this by discipline; Rust should get it by a must-use guard. **Prove on an
   isolated non-FFI Miri harness before wiring** (mirihar doctrine,
   `.port/phase-2/mirihar-chunk3b` precedent). Also note
   `scrollMarginsBegin` itself contains `assert(self == &s.kitty_images)` — the
   storage-inside-screen self-alias is explicit upstream.
2. **Screen-parameter threading widened again.** `graphics_storage.zig` now has ~19
   `s: *terminal.Screen` parameter sites (`:171,:210,:274,:346,:363,:601,:624,:632,
   :647,:675,:741,:1013,:1452,:1491,:1610,:1622,:1709,:1812`) vs Phase 4's handful —
   every new one is the same `ImageStorage`-lives-inside-`Screen` self-alias; keep
   the raw-ptr-argument pattern rustty already uses in `delete()` uniformly.
3. **Hash-map interior pointers across mutation (B4, B2).** exec's
   `completeAnimationFrame`/`controlAnimation` obtain `imagePtrByIdOrNumber` (a
   pointer into `images`) and then call storage mutators
   (`reserveAnimationBytes` may **evict**, `releaseAnimationBytes`, placement
   removal) — pointer invalidation if eviction touches `images`. Same for
   `resolveChain`/cascaded transitive deletion (B2): parent-chain walks over
   `placements` interleaved with `removePlacementByPtr`-style removal. Rustty's
   sanctioned shape (collect keys first, re-lookup per mutation;
   `graphics_storage.rs` delete arms) must be extended, and Zig's
   remove-during-iteration legality must NOT be mirrored with Rust iterator
   aliasing.
4. **`animationTick` is driven from the RENDER thread** (`renderer/generic.zig:1388`)
   while exec mutates the same storage from the io/stream side — upstream is safe
   under the renderer/terminal mutex; the rustty port must place the tick behind the
   same lock discipline (renderer area consumes; document the contract on
   `animation_tick`'s doc comment).
5. **Streaming decoder state across packets (B5).** `WriteState.decoder:
   simd.base64.Streaming` persists between OSC packets and decodes **directly into
   the spool** (`clipboard_write.zig:167`) — a self-referential-adjacent pattern if
   the Rust decoder borrows the output buffer; port the simd streaming API with
   owned state + `&mut [u8]` per call, never a held borrow. The EOF-at-padding
   restart rule is load-bearing behavior (tested by `write: independently padded
   chunks accumulate`), not an implementation detail.
6. **R7 discipline hot spots:** `4b915e3bc`'s `gap_ms` i32-from-u32 **bit-pattern**
   reconstruction (`@bitCast` → `i32::from_ne_bytes`/`as` documented as truncate-free
   bit cast, NOT a checked narrowing); `Background` packed(u32) b,g,r,a little-endian
   bit-cast (byte-order comment is normative — golden-byte test it); the scroll
   arithmetic in B3 uses `i64` widening (`top/bottom: i64`) — keep the widths;
   `242723223`'s lenient value arms must stay *lenient* (no `try_from().unwrap()`
   traps on out-of-range display keys — the tests pin tolerance).
7. **DG2 anchor unaffected but adjacent:** the existing
   `image_load_expected_size_capacity_width_u32_max` regression anchor
   (PORT_RULES DG2) sits in `graphics_image.rs`, which `abd77067d`/`86d94f150`
   modify upstream — re-run it after B1 and do not let the S-exact-byte rewrite
   weaken the `max_dimension`-before-multiply order.

---

## 5. Dropped-test reconciliation (G1b `w0/g1b-after-at-683d8db64.txt`: kitty zig=334, rust=132, **drop=202**)

Counting method: `grep -c '^test '` on the head tree gives **337** across the 21 zig
files; the gate's 334 differs by 3 (its filter excludes unnamed aggregator
`test { … }` blocks — `clipboard.zig`, `dnd.zig`, and one of the new command files
carry them). rustty `#[test]` = **132** exactly matches the gate. The drop is the
range's new tests; per-file → per-commit attribution (extracted exhaustively, §0):

| file | pin → head | added by |
|---|---|---|
| `clipboard_command.zig` | 0 → 18 named | `25b1170d4` ×14 surviving (`metadata: empty is dropped`, `record without =`, `missing or unknown type`, `duplicate keys keep the last occurrence`, `basic read`, `unknown keys ignored`, `loc`, `id sanitized`, `id truncated to max`, `mime decoded`, `pw and name`, `empty name`, `payload: mime iterator`, `payload: invalid base64`), `e8d8945b5` ×2 (`invalid mime utf8 reported`, `over-long name reported` — supersede the `25b1170d4`/`4f49dc2b8` "dropped" names), `bc2f7d7d2` ×1 (`invalid mime base64 reported`), `928c7f0e7` ×1 (`read prompt exemption: only requests without data types`) |
| `clipboard_grants.zig` | 0 → 8 | `6f007e767` ×5 (`grants: basic grant and use`, `one-time consumed on check`, `…even on direction mismatch`, `directions are independent`, `capacity evicts the oldest`), `a5bb22e23` ×2 (`generateOtp: length and alphabet with a real Io`, `no entropy is an error, never a weak password`), `dda8e6f31` ×1 (`generateOtp: sys override supplies entropy without an Io source`) |
| `clipboard_response.zig` | 0 → 15 | `e28acd928` ×12 (3 `response:` + 9 `read success:`), `a5bb22e23` ×3 (`paste event: pw in every packet…`, `primary is reported only on the OK packet`, `empty listing packet is still sent`) |
| `clipboard_write.zig` | 0 → 20 | `7a940ec02` ×6 surviving (`basic transaction`, `chunked data accumulates`, `multiple mimes in order`, `reused mime overwrites`, `aliases resolve at commit`, `alias without data target is dropped`), `5350d4a5f` ×3 surviving (`default limit when unset`, `non-text under limit is unaffected`, `non-text over limit rejects transaction`), `70f006575` ×4 (`text over limit rejects transaction`, `text crossing limit across chunks…`, `data exactly at limit is accepted`, `total data across MIME types is limited`), `e8d8945b5` ×1 (`write: alias payload must be valid utf8`; its `payload: decoded text must be valid utf8` sits in command), `bc2f7d7d2` ×6 (`invalid base64 chunk aborts the transaction`, `chunks split one base64 stream at arbitrary boundaries`, `incorrectly padded stream aborts at commit`, `…at MIME switch`, `independently padded chunks accumulate`, `data after padding within one chunk aborts`) — the 4 `5350d4a5f` truncation tests and `7a940ec02`'s drop-chunk test were deleted in-range and must NOT be ported |
| `dnd_command.zig` | 0 → 11 | `7c845e8af` (8 `Metadata:` + `Request: classification` + 2 `Chunking:`) |
| `dnd_response.zig` | 0 → 6 | `38746b8c1` (`encode: bare message`, `…with client id`, `plain payload single chunk`, `base64 payload chunking`, `encodeError: with and without description`, `RequestKeys: only non-zero keys written`) |
| `dnd_test.zig` | 0 → 26 | `50f69b883` (all 26 `dnd: …`, incl. `kitten 0.47 conversation replay`, `over-cap registration list never completes`) |
| `graphics_animation.zig` | 0 → 1 | `d8b920e50` (`animation gap helpers`) |
| `graphics_pixel.zig` | 0 → 6 | `d8b920e50` (`rgba conversion`, `fill background`, `compose rect overwrite with clipping`, `compose rect entirely out of bounds`, `alpha blend source-over semantics`, `compose canvas rect`) |
| `graphics_command.zig` | 27 → 38 (+11) | `242723223` ×10 (`unknown format value is deferred to execution`, `zero format value is rgba`, `known format values are not unknown`, `quiet value above two suppresses all responses`, `cursor movement value above one moves the cursor`, `virtual placement value above one is virtual`, `animation frame composition value above one alpha blends`, `animation compose value above one overwrites`, `animation control action above three is ignored`, `unknown flag key values still fail parsing`), `f7d29b19e` ×1 (`delete range command 6`) |
| `graphics_exec.zig` | 12 → 66 (+54) | `d2ffeba` ×23 animation, `33ca9e5dd` ×13 relative-placement, `242723223` ×5, `6b23c584c` ×3, `83145c0a3` ×2, `86d94f150` ×2, `cfc5a9650` ×2, `e5747cf0b` ×2, `306a16903` ×1, `c5a3c7e2e` ×1, `afb61e1b6` ×1 (sum 55; `f3e98fb72` trims assertions, not tests; ±1 vs the raw grep is the `83145c0a3` pair's file placement) |
| `graphics_image.zig` | 25 → 27 (+2) | `abd77067d` (`image load: regular file size reads exactly requested bytes`, `…rejects short data`) |
| `graphics_storage.zig` | 37 → 71 (+34) | `90bce0d2d` ×11, `d8b920e50` ×5 (`storage: animation tick …`), `f8b40a023` +5/−1, `cd5f9eef0` ×3, `306a16903` ×3, `6b23c584c` ×2, `c5a3c7e2e` ×2, `52190a5d8` ×2, `a32a100d4` ×1, `f7d29b19e` ×1 |
| unchanged | `graphics.zig` 1, `graphics_unicode.zig` 16, `color.zig` 1, `key.zig` 3, `graphics_render.zig` 0 | — |

**Gate blind spots (G1b cannot see these; only porting the amended bodies does):**
`4b915e3bc` (rewrites existing animation-parse tests/struct semantics, adds no
`^test`), `f3e98fb72` (removes an assertion from an in-range test), `322d7ae78`
(reimplements pixel functions under unchanged test names), `83145c0a3`/`242723223`
(amend existing command-test bodies alongside their new tests), and `f8b40a023`'s
**deletion** of `storage: delete all placements and images`, whose rustty port must
be removed/replaced or it will assert the superseded delete-all semantics.

---

## 6. Cross-area couplings and ordering

**I depend on (cannot land my rows without):**

| my item | area/owner | needed |
|---|---|---|
| B4 pixel (`322d7ae78`, `d8b920e50`) | wuffs/pkg owner | `pkg/wuffs/src/swizzle.zig` port (rgbToRgba/gToRgba/gaToRgba/rgbaSrcOver) in rustty's wuffs module; `-Dvt-features` build_options mapping |
| B5 base64 (`bc2f7d7d2`) | simd owner | `simd/base64.zig` `Streaming` decoder (+349, incl. scalar half) in rustty-simd |
| B5 limits (`5350d4a5f`/`70f006575`) | config + area C | `Config.zig` clipboard-write-limit option; `c/terminal.zig` + `include/ghostty/vt/terminal.h` setters |
| B5 reads (`4f49dc2b8`) | area C + stream | clipboard_read effect surface (c/terminal.zig +75, header +31) and the stream_terminal read plumbing (+460) |
| B7 OTP entropy (`dda8e6f31`) | area C / os | NEW `src/terminal/sys.zig` + `c/sys.zig` + `vt/sys.h` + TinyIo `RandomSecure`; decide the rustty `std.Io` mapping before `generateOtp` |
| B7 paste events (`a5bb22e23`) | input/paste owner | NEW `src/terminal/paste.zig` shared core (input_paste drop=7) + lib_vt wiring |
| B3 (`90bce0d2d`) | terminal-core | `Terminal.zig` +134 scroll two-phase call sites (`Terminal.zig:2473-2496`) |
| B1 `f8b40a023` half | terminal-core | its `Terminal.zig` +12 |
| B6 stream (`af8d28a94`) | stream owner | `stream_terminal.zig` +253 dnd dispatch + `stream.zig`/`Terminal.zig` bits (part of stream_terminal drop=38) |

**Others depend on me / expect my surfaces:**

- **stream owner:** the bulk of stream_terminal drop=38 is kitty-clipboard/dnd
  dispatch tests (`bc2f7d7d2` +105, `e8d8945b5` +96, `4f49dc2b8` +460, `af8d28a94`
  +253, `928c7f0e7` +58, `5350d4a5f` +52, `70f006575` +19 in that file) — they cannot
  go green before B5/B6 land here.
- **osc owner:** B6 gutting of `kitty_dnd_protocol` (363→138) and the one-line
  `kitty_clipboard_protocol.zig` touch (`5350d4a5f`) — coordinate so the parser
  shrink and the `dnd_drop.rs` landing are one change.
- **area C:** `c/kitty_graphics.zig` (`cd5f9eef0`, `52190a5d8`), `c/terminal.zig`
  clipboard surfaces, `lib_vt.zig` paste exports.
- **renderer:** `renderer/image.zig` halves (`cd5f9eef0`, `c5a3c7e2e`, `52190a5d8`)
  and the `animationTick` drive (`renderer/generic.zig:1388`).
- **core-surface/termio:** `Surface.zig` (`bc2f7d7d2`, `928c7f0e7`) and
  `termio/stream_handler.zig` halves of the clipboard series.

**Recommended cross-area order:** wuffs swizzle + simd Streaming first (small, leaf,
unblock B4/B5); graphics bundles B1→B2→B3→B4 with terminal-core/renderer halves;
then B5 core (+config/C limits), B6 with osc+stream, B7 with input/os/C. Every bundle
lands with its tests in the same change; G1b stays RED until all of §5 is carried.
