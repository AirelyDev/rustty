# Phase 4 area B — snapshot-io: re-vetting assessment

**Range:** `6ad1fe7d8..12967b68f` (oracle `origin/main`, 2026-08-17; working tree AT the pin)
**Assessed against:** rustty `main` @ `8120dd10` (clean working tree)
**File-set owned:** `src/terminal/snapshot/**`, `src/crc32c.zig`, `src/lib/TinyIo.zig`,
`src/terminal/compress/**`, `src/benchmark/**`, `src/synthetic/**`
**Author lane:** Phase 4 area B (snapshot serialization + io + benchmark/synthetic). Assessment only — no production code changed.

---

## 1. Summary — verdict

**The entire snapshot serialization subsystem is ABSENT from rustty, wholesale and
uniformly: all 73 range commits touching this area's files have verdict ABSENT.**
There is no partially-started port, no stray symbol, no `crc32c` module, no `TinyIo`
analogue, no `GHOSTSNP` magic anywhere in the tree (`grep -rn "GHOSTSNP\|RUSTTSNP"`
over `*.rs`/`*.h` → zero hits), no new benchmark, no `Styled` generator. This is the
expected result — the subsystem was born inside this range — and it makes this
assessment primarily a **structural map for the port**, not a divergence hunt.

| Verdict | Count |
|---|---|
| PORTED | 0 |
| PARTIAL | 0 |
| ABSENT | **73** |
| N-A | 0 |
| **total commits owned** | **73** |

`src/terminal/compress/**` had **zero commits in the range** (verified:
`git log 6ad1fe7d8..12967b68f -- src/terminal/compress` is empty); the Phase-3
B-compression verdicts stand untouched and this area inherits nothing there.

Upstream volume owned by this area (end state @ 12967b68f):

| Component | Files | Lines | Zig tests |
|---|---|---|---|
| `src/terminal/snapshot/*.zig` (15 files) | 15 | 14,478 | **127** |
| `src/terminal/snapshot/snapshot.ksy` | 1 | 1,071 | — (kaitai spec) |
| `src/terminal/snapshot/verify-kaitai.py` | 1 | 400 | — (CI-side verifier) |
| `src/terminal/snapshot/AGENTS.md` | 1 | 7 | — (policy) |
| `src/terminal/snapshot/testdata/*.hex` (22 goldens) | 22 | 346 | consumed by fixture.zig |
| `src/crc32c.zig` | 1 | 424 | 5 |
| `src/lib/TinyIo.zig` | 1 | 1,522 | 20 |
| `src/benchmark/{TerminalSnapshot,TerminalFormatter,TerminalResize}.zig` | 3 | 1,189 | 8 |
| `src/synthetic/cli/Styled.zig` | 1 | 410 | 4 |
| deltas to existing `benchmark/{ApcParser,TerminalStream}.zig`, `synthetic/cli/Ascii.zig` | 3 | ~100 | +2 |
| **total** | **49** | **~19,950** | **166** |

None of these 166 tests exist in rustty. None of them are counted by G1b either —
see §7 (gate-coverage gap).

---

## 2. Subsystem map (what the port must build)

### 2.1 Module DAG (from `snapshot/main.zig` + import scan of all 15 files)

```
main.zig  (root: re-exports everything; codec = snapshot.zig)
 ├── envelope.zig      10-byte header: magic "GHOSTSNP" + u16 version (=1)
 ├── record.zig        v1 record framing: Tag(u16 enum: terminal=1 screen=2 page=3
 │                     history=4 ready=5 finish=6 continuation=7), Header, Checksum,
 │                     record.Writer (begin/finish/cancel), record.Reader
 │                     (payloadReader/finish; borrows fully buffered payloads —
 │                     c1a61fddd). pub re-exports Crc32c from ../../crc32c.zig
 ├── io.zig            LE writeInt/readInt helpers (readInt deliberately avoids
 │                     takeInt's buffer-size assert — comment is load-bearing, R1)
 ├── checkpoint.zig    READY / FINISH empty marker records (BLAKE3 digests REMOVED
 │                     by 219173ab3 — end state has none)
 ├── continuation.zig  CONTINUATION record: canonical unfinished TerminalStream
 │                     input (depends on ../stream_continuation.zig — area A)
 ├── grid.zig          rows/cells/grapheme-suffix codec. 8-byte cell words, blank
 │                     elision, per-row variable cell width (1/2/4/8B via 2 flag
 │                     bits), vectorized encode+decode, batched grapheme suffixes,
 │                     per-row wide-pair resolution
 ├── style.zig         style-table entry codec (single-pass after 1359973ae)
 ├── hyperlink.zig     hyperlink-table codec (implicit/explicit IDs, hardened)
 ├── page.zig          PAGE record: staged payload decode directly into a
 │                     terminal_page.Page; remap tables skipped for style-less
 │                     pages (47a518262); verification gated on
 │                     build_options.slow_runtime_safety (9f6656347)
 ├── screen.zig        SCREEN record: cursor (clamped on decode, cafe7d5da),
 │                     pending-wrap incl. mixed-width (d14847183), saved cursor,
 │                     selection, kitty graphics (gated build_options.kitty_graphics),
 │                     history size hints (moved here by e2e74fecb), trackPin usage
 ├── history.zig       HISTORY record: newest-to-oldest page manifest, prepend on
 │                     arrival
 ├── terminal.zig      TERMINAL record: modes, tabstops, charsets, mouse, colors,
 │                     screen set/keys
 ├── snapshot.zig      the codec: encode / decode / decodeExact / incremental
 │                     Decoder (init → ready() → next() per history page → null
 │                     validates FINISH); EncodeOptions/DecodeOptions
 │                     (.continuation, .max_continuation_bytes); Decoded.toOwned()
 └── fixture.zig       test-only golden harness: parses testdata/*.hex annotated
                       hex, byte-compares, on mismatch writes candidate to repo
                       root for review; kaitai metadata lines in each golden header
```

Terminal-core dependencies (all already in rustty-vt, so the DAG is portable today):
`../Terminal.zig`, `../Screen.zig`, `../ScreenSet.zig (Key)`, `../PageList.zig`,
`../page.zig`, `../style.zig`, `../hyperlink.zig`, `../osc.zig`, `../kitty.zig`,
`../ansi.zig`, `../modes.zig`, `../mouse.zig`, `../charsets.zig`, `../Tabstops.zig`,
`../size.zig`, `../sgr.zig`, `../color.zig`, `../compress.zig`,
`../stream_terminal.zig (Stream)`, and **`../stream_continuation.zig` which does NOT
exist in rustty yet** (owned by area A, commit 68beeeb3f) — hard ordering edge, §5.

### 2.2 Wire format anchors (bit-for-bit obligations)

- Envelope: exactly 10 bytes, magic `GHOSTSNP`, version `u16 = 1`, little-endian;
  `comptime assert(encoded_len == 10)`.
- Every record independently framed (Tag u16 + payload len) and CRC32C-protected,
  including the empty READY/FINISH markers.
- Record group order: TERMINAL → SCREEN×n + PAGE×m → CONTINUATION → READY →
  HISTORY×n + PAGE×m (newest-to-oldest) → FINISH. Bytes after FINISH belong to the
  transport (`d37e1fe18`: format does NOT require EOF; `decodeExact` keeps the
  strict check).
- Grid row: 1 flag byte (wrap, wrap-cont, 2-bit semantic prompt, 2-bit encoded cell
  width, 2 reserved-zero) + u16 encoded cell count + width×count cell bytes;
  trailing all-zero cells elided; grapheme suffix section after all rows.
- **`GHOSTSNP` magic is a WIRE constant, not prose** — R2 token renaming must NOT
  touch it. The 22 hex goldens embed it (`47 48 4f 53 54 53 4e 50 01 00` at offset
  0 of `complete-v1.hex`); renaming it would break every golden and interop with
  upstream snapshots. Carry all 22 goldens **byte-for-byte** (R1 comptime
  artifacts) including their `# Kaitai type/params/offset/Wire version` header
  comments, which `fixture.zig` and `verify-kaitai.py` both parse.

### 2.3 `src/crc32c.zig` (424 lines, 5 tests) — end state after 3 commits

Comptime backend select: `aarch64_crc` (inline asm `crc32cb/cw/cx`), `x86_64_sse42`
(guarded on `zig_backend == .stage2_llvm`, a Zig-self-hosted-backend caveat that is
N-A in Rust — use `core::arch` intrinsics + runtime/compile-time feature detect),
`software` = slicing-by-16 (36d8e3f77) with **three interleaved CRC streams over
48-byte blocks** merged via a comptime shift-by-64 table (eb09bf829). Public shape:
`Crc32c { init, update, final, hash }`, iSCSI parameter set, result identical across
backends (tests assert cross-backend equality against `std.hash.crc.Crc32Iscsi`).
Rust fork note: a `crc32c` crate exists on crates.io, but R1 fidelity + the wasm
"faster than stdlib" software path argue for porting this file as written
(rustty-vt has no permission to take new deps casually; decide explicitly, record
in ledger).

### 2.4 `src/lib/TinyIo.zig` (1,522 lines, 20 tests)

A tiny blocking `std.Io` **vtable implementation** for libghostty (replaces
`Io.Threaded`: ~100–200KB binary, ~300KB runtime savings; no cancelation, no
concurrency; direct POSIX syscalls; `supported == false` on
windows/wasi/freestanding → behaves as `std.Io.failing`). Consumers: `lib_vt.zig`,
`terminal/lib.zig`, `terminal/c/{terminal,snapshot}.zig` — i.e. the C-API layer
passes an `Io` into `snapshot.decode(alloc, io, reader, opts)`. **Port-design
fork:** Rust has no `std.Io` vtable; the faithful move is to port the *seam* — an
`Io`-like parameter type on the decode API whose blocking impl is TinyIo's syscall
set — and keep the `supported` gating semantics. Do not silently substitute
`std::io` without recording the fork (memory: discuss-tradeoffs-before-recommending).

### 2.5 Benchmarks + synthetic

- `benchmark/TerminalSnapshot.zig` (350 L, 3 tests; modes exercising encode +
  incremental decode — the +43 hunk of e37865bed benchmarks `Decoder`),
  `TerminalFormatter.zig` (482 L, 4 tests; rewritten +108/−6 by 2ed67cadd pin-map
  redesign — port END STATE), `TerminalResize.zig` (357 L, 1 test). Each registers
  in `benchmark/cli.zig` (+2 lines each) and `benchmark/main.zig` (+1 each);
  rustty-bench's `cli.rs` has none of the three actions (grep verified).
- `synthetic/cli/Styled.zig` (410 L, 4 tests) + registration in `synthetic/cli.zig`;
  rustty-synthetic has no `styled.rs`.
- `synthetic/cli/Ascii.zig` +54/−6 (7031c892b): `line-min`/`line-max` Options,
  CR-LF-terminated uniform-length lines, new `"Ascii lines"` test. rustty
  `cli/ascii.rs` has no `line_min`/`line_max` (grep verified) — delta absent.
- `benchmark/ApcParser.zig`: b53728241 (`apc.end()` now returns a `result` wrapper)
  and 68beeeb3f (`stream .init(.{ .allocator, .handler })` shape). rustty
  `apc_parser.rs:104-126` still mirrors the OLD upstream (`.init(.{ .alloc })`,
  `|cmd|`) — deltas absent, and they only compile once area A lands the apc/stream
  API changes (bundle, §5).
- `benchmark/TerminalStream.zig`: 68beeeb3f adds `continuation-enabled` /
  `continuation-max-bytes` options + a tracked-run test. rustty
  `terminal_stream.rs` has no continuation option (grep verified) — absent,
  bundled with area A's `stream_continuation.zig`.

---

## 3. Per-commit verdicts

Evidence convention: every verdict below is ABSENT with the same base evidence —
**no `snapshot` module exists under `crates/rustty-vt/src/` (dir listing), no
`crc32c`/`TinyIo`/`GHOSTSNP` symbol matches anywhere in rustty (repo-wide grep),
and the named rustty counterpart file lacks the delta (per-file grep cited
inline)**. "END-STATE" flags commits whose content was later rewritten in-range:
port the final file, carry the commit's tests.

### Cluster C1 — codec scaffold, framing, checkpoints, continuation, incremental decoder (16 commits, ~3,540 end-state lines)

| Commit | Subject | Verdict | Notes |
|---|---|---|---|
| fdf8dfd7b | define v0 record framing | ABSENT | envelope.zig/io.zig/record.zig born; END-STATE (record.zig heavily rewritten later) |
| d44baa914 | setup the snapshot main | ABSENT | main.zig; END-STATE (root doc + exports grew) |
| 83e482700 | ready/finish checkpoints | ABSENT | checkpoint.zig born WITH BLAKE3 — superseded by 219173ab3; port end state (no BLAKE3) |
| 86ec14633 | full encode/decode | ABSENT | snapshot.zig born (413 L → 2,097 end) |
| 05d493484 | better root export | ABSENT | main.zig re-export shape (codec aliases) — port exactly this surface |
| d37e1fe18 | format doesn't require EOF | ABSENT | introduces decode vs decodeExact split; tests in snapshot.zig |
| f0fe788fc | less buffering, better stream writing | ABSENT | touches 8 files; encoder buffers ONLY current record payload (main.zig doc §Encoding) |
| e8e56e782 | small edits | ABSENT | cross-file polish; END-STATE |
| cbc9f360b | report invalid decoder states | ABSENT | Decoder state-machine errors; tests |
| 7d9aaa297 | clarify incremental history errors | ABSENT | +8 error docs/paths |
| 58e92098a | snapshot robustness | ABSENT | history/main/snapshot hardening + tests |
| f8ac0ca98 | accept kitty placeholder cells, track rows | ABSENT | +73 snapshot.zig tests; grid/terminal tweaks |
| e37865bed | add incremental decoder | ABSENT | `snapshot.Decoder` (init/ready/next), +881 snapshot.zig, +98 page.zig, +43 bench hunk (also in C8) |
| 70e41e96d | pty continuation | ABSENT | continuation.zig (320 L) + 7 continuation-*.hex goldens + ksy; **depends on area A's stream_continuation.zig** |
| 219173ab3 | remove BLAKE3 digests | ABSENT | end state has NO BLAKE3 anywhere; also rewrites checkpoint-ready/complete goldens + ksy + verify-kaitai; do not port any BLAKE3 |
| c1a61fddd | borrow fully buffered record payloads | ABSENT | record.Reader borrows staged payloads (perf); +50 record.zig — Rust: borrow, don't copy |

### Cluster C2 — page / style / hyperlink records (15 commits, ~2,740 end-state lines)

| Commit | Subject | Verdict | Notes |
|---|---|---|---|
| b4fd26f0d | hyperlink and style encoding | ABSENT | hyperlink.zig + style.zig born; END-STATE |
| 805c3b0ba | start page encoding | ABSENT | page.zig born; END-STATE |
| 2fc238ed0 | decode directly into pages | ABSENT | decodes into `terminal_page.Page` in place — **SB hazard, §6** |
| 406f5e7d8 | encode sparse page grids | ABSENT | grid.zig born (562 L; also C3); +552 page.zig |
| 83ffa74e2 | page records | ABSENT | PAGE record framing; +172 record.zig |
| 3e5d12835 | stage PAGE payloads while decoding | ABSENT | staging buffer before commit |
| 9f6656347 | gate page verification on slow runtime safety | ABSENT | `build_options.slow_runtime_safety` — map to rustty's slow-runtime-safety cfg (page.zig:284 end state) |
| 43ec9b373 | harden hyperlink decoding, allow invalid for page | ABSENT | +142 screen.zig tests |
| a44eb8335 | style/hyperlink robustness in page and screen | ABSENT | AGENTS.md robustness principle applied; big page.zig rewrite |
| 9a5279db6 | release decoded style table refs | ABSENT | error-path ref-count release (+79 page.zig) — leak-on-error tests |
| b5290e74c | release decoded hyperlink table refs | ABSENT | same for hyperlinks (+62) |
| f99896bf8 | preserve style reader errors | ABSENT | +77 page.zig, +31 style.zig; error fidelity (R1 error paths) |
| a508720a8 | grid decode robustness principle | ABSENT | **creates AGENTS.md (7 L)** — port as crate-local AGENTS.md like compress did NOT get in Phase 3 (flagged there; don't repeat) |
| 1359973ae | single-pass style entry codec | ABSENT | perf rewrite of style.zig (47/47) + page.zig — END-STATE target |
| 47a518262 | skip remap tables for pages without styles | ABSENT | perf; grid+page |

### Cluster C3 — grid codec + vectorized perf wave (10 commits, ~2,400 end-state lines)

| Commit | Subject | Verdict | Notes |
|---|---|---|---|
| 13bc78b7f | grid tests | ABSENT | +162 grid tests, grid-v1.hex + checkpoint-ready-v1.hex goldens |
| ed0f54fb8 | 8-byte grid cells with blank elision | ABSENT | major format change (+990/−422 grid.zig) + 4 goldens + ksy rewritten |
| 9e3019f19 | variable-width grid cell encoding | ABSENT | per-row 1/2/4/8-byte widths (+726 grid.zig) + goldens + ksy |
| 973f619a2 | vectorize grid row encoding | ABSENT | `@Vector` encode — Rust: portable SIMD per rustty-simd conventions; carry scalar-equivalence tests |
| 2aaad3ca9 | vectorize grid cell decoding | ABSENT | ditto decode side |
| 593762cfa | batch grapheme suffix codec | ABSENT | +115/−44 grid.zig |
| 7c1014ef6 | resolve wide pairs in a per-row pass | ABSENT | wide/spacer-tail pairing moved to per-row pass |
| 418b5d180 | harden grapheme suffix decode | ABSENT | +53 bounds/robustness tests |
| 727b8a02f | terminal: bound OSC and grapheme allocations | ABSENT | grid.zig hunk (26/18) only; rest is area A (osc/page/PageList) — bundle §5 |
| d7bb4b863 | libghostty-vt: add C API for snapshotting | ABSENT | ONLY the grid.zig 7/4 hunk owned here; `c/{snapshot,io}.zig`, headers, example → area C |

### Cluster C4 — screen / terminal / history records (16 commits, ~5,020 end-state lines)

| Commit | Subject | Verdict | Notes |
|---|---|---|---|
| 6508cbbb4 | screen record | ABSENT | screen.zig born (1,703 L); END-STATE (rewritten ×6 later) |
| f50bdfab2 | screen plus active encoding | ABSENT | |
| d34fd0593 | screen decoding | ABSENT | decode side; +76 page.zig |
| 0288bec3c | terminal record | ABSENT | terminal.zig born (1,806 L) |
| 7d91b8776 | history record | ABSENT | history.zig born (730 L) |
| e2e74fecb | move history size hints to screen record | ABSENT | format change; screen-header/history-header goldens + ksy churn |
| b867a0f59 | use lib.Enum enums where possible | ABSENT | −451 screen.zig; `lib.Enum` is the c_abi enum helper — coupling with area owning `src/lib/` packed/c_abi work |
| 465488d6b | screen robustness | ABSENT | 244/227 rewrite |
| 9d1c6a921 | terminal robustness | ABSENT | 211/134 rewrite |
| 7c64181b6 | history robustness | ABSENT | 61/56 |
| cafe7d5da | clamp decoded saved cursors | ABSENT | +63 screen.zig; decode-clamping tests |
| d14847183 | preserve mixed-width pending wrap | ABSENT | +53 screen.zig; pending-wrap fidelity (ties to formatter pending-wrap work, area A) |
| e89ff37aa | encode screen pages safely | ABSENT | +63 screen.zig — encode-side integrity, §6 |
| 4371871bc | kitty: evict without scratch allocation | ABSENT | 1/1 screen.zig hunk only; kitty core → area D |
| b5e86a428 | kitty: release evicted placement pins | ABSENT | +1 snapshot.zig hunk only; kitty core → area D |
| d0c516f8f | kitty: release replaced placement pins | ABSENT | +1 snapshot.zig hunk only; kitty core → area D |

### Cluster C5 — goldens, fixtures, kaitai verification (3 commits, ~2,300 lines incl. spec+script+goldens)

| Commit | Subject | Verdict | Notes |
|---|---|---|---|
| 32f11a466 | test fixtures | ABSENT | fixture.zig (416 L) + 13 goldens born; the on-mismatch copy-candidate-to-repo-root workflow is part of the harness contract |
| 38d92c50c | kaitai verification | ABSENT | snapshot.ksy (998 L) + verify-kaitai.py (422 L) + `# Kaitai ...` headers added to every golden; CI-side, but the .ksy is the format's normative spec — carry verbatim (R1); decide where rustty runs verify-kaitai (xtask?) |
| 92c8dfd50 | clean up tests | ABSENT | −~250 test dedup across 7 files; END-STATE means you never port the removed duplicates |

### Cluster C6 — CRC32C (3 commits, 424 end-state lines)

| Commit | Subject | Verdict | Notes |
|---|---|---|---|
| 9cc061c28 | hardware-accelerated CRC32C | ABSENT | crc32c.zig born (193 L; aarch64/x86_64 inline asm) + record.zig switch from std Crc32Iscsi |
| 36d8e3f77 | slicing-by-16 software CRC32C | ABSENT | software backend rewrite (+104) |
| eb09bf829 | interleave software CRC32C streams | ABSENT | 3-stream interleave over 48-byte blocks + comptime shift table (+171/−30); END-STATE target; 5 tests incl. cross-backend equality |

### Cluster C7 — TinyIo (2 commits, 1,522 end-state lines)

| Commit | Subject | Verdict | Notes |
|---|---|---|---|
| 82df79ec8 | replace Io.Threaded with custom Io impl (TinyIo) | ABSENT | TinyIo.zig born (1,521 L, 20 tests); consumers in lib_vt/terminal-lib/c → area C bundle |
| a15dddae2 | TinyIo: fix failing tests in CI | ABSENT | 5/4 fix; subsumed by end state — carry its test shape |

### Cluster C8 — benchmarks + synthetic (8 commits, ~1,700 lines)

| Commit | Subject | Verdict | Notes |
|---|---|---|---|
| 0b5e12453 | benchmark: add terminal-snapshot | ABSENT | TerminalSnapshot.zig (307 L) + cli/main registration; rustty-bench `cli.rs` lacks the action (grep) |
| e37865bed | (bench hunk) +43 TerminalSnapshot.zig | ABSENT | incremental-decode bench mode; counted once in C1 for totals |
| dc52c248e | benchmark: terminal-resize | ABSENT | TerminalResize.zig (357 L) |
| 85b1dd0dd | benchmark: formatter benchmark | ABSENT | TerminalFormatter.zig born (380 L) |
| 2ed67cadd | terminal: redesign pin map for formatter | ABSENT | +108/−6 TerminalFormatter.zig hunk owned here; Screen/StringMap/formatter/style → area A (this is the commit behind G1b's `formatter drop=2`) |
| b11d60818 | synthetic: styled output generator | ABSENT | Styled.zig (410 L) + cli registration; no `styled.rs` in rustty-synthetic (dir listing) |
| 7031c892b | synthetic: line length options for ascii | ABSENT | rustty `cli/ascii.rs` has no `line_min/line_max` (grep exit 1) |
| b53728241 | terminal/apc: report unknown APC (bench hunk) | ABSENT | ApcParser.zig 4/4 `cmd→result`; rustty `apc_parser.rs:104-114` still old shape; apc core → area A/D (G1b `apc drop=4`) |
| 68beeeb3f | stream continuation tracking (bench hunks) | ABSENT | ApcParser +4/−1 stream-init shape, TerminalStream +26/−4 continuation opts + test; `terminal_stream.rs` lacks `continuation` (grep); core → area A (G1b `stream drop=8`) |

(2ed67cadd/b53728241/68beeeb3f/e37865bed each appear in exactly one cluster's
commit list for counting; cross-listed rows above are annotations, not double counts.
Counting set: C1=16, C2=15, C3=10, C4=16, C5=3, C6=3, C7=2, C8=8 → **73**.)

---

## 4. Atomic bundles

1. **stream-continuation bundle (with area A):** 68beeeb3f (`stream_continuation.zig`
   + Stream init API change) must land before/with: `snapshot/continuation.zig`
   (70e41e96d), the CONTINUATION record in snapshot.zig, the ApcParser/TerminalStream
   bench deltas, and the 7 `continuation-*.hex` goldens. Porting the snapshot codec
   without it cannot even express record tag 7's payload semantics.
2. **apc-result bundle (with areas A/D):** b53728241 changes `apc.end()`'s return
   type; the ApcParser bench hunk is uncompilable without it (and is the source of
   G1b `apc drop=4`).
3. **pin-map/formatter bundle (with area A):** 2ed67cadd rewrites Screen's pin map +
   formatter; TerminalFormatter.zig's end state benches the NEW API (G1b
   `formatter drop=2`).
4. **crc32c ↔ record framing:** 9cc061c28/36d8e3f77/eb09bf829 and record.zig's
   Checksum are one unit — record.zig `pub const Crc32c = @import("../../crc32c.zig")`.
   Landing framing with a different CRC (e.g. std) would produce goldens that
   pass locally and violate the wire format.
5. **goldens ↔ codec version:** all 22 `.hex` goldens + `snapshot.ksy` +
   `verify-kaitai.py` + `fixture.zig` encode ONE consistent v1 format (post
   219173ab3 / e2e74fecb / ed0f54fb8 / 9e3019f19 / 70e41e96d). Never mix a golden
   from an intermediate commit with end-state code — several goldens were rewritten
   3–4 times in-range.
6. **TinyIo ↔ C-API decode surface (with area C):** 82df79ec8's consumers are
   `c/{terminal,snapshot}.zig` + `lib_vt.zig`; the `Io` parameter threading through
   `snapshot.decode` is the seam. Area C also owns 9be6c2ea2 + head 12967b68f
   (`retain continuations on snapshot decode` — `c/snapshot.zig` +313, header +60):
   **not in my file-set**, but its Zig-side `DecodeOptions` knobs live in MY
   snapshot.zig — coordinate so the option enum matches the C header.

---

## 5. Cross-area couplings

| Other area | Coupling |
|---|---|
| **A (terminal core)** | `stream_continuation.zig` (68beeeb3f) — snapshot/continuation.zig imports it; pin-map redesign (2ed67cadd); OSC/grapheme allocation bounds (727b8a02f, shared commit); apc result type (b53728241); pending-wrap semantics (d14847183 mirrors formatter pending-wrap work); ScreenSet.Key, Screen, PageList, Tabstops, page/style/hyperlink types are the codec's whole input domain |
| **C (C-API / headers / lib)** | `c/{snapshot,io}.zig`, `include/ghostty/vt/{snapshot,io}.h`, example/c-vt-snapshot (d7bb4b863, 219173ab3 header hunks, 9be6c2ea2, 12967b68f); TinyIo consumers (82df79ec8); `lib.Enum` / c_abi usage in screen.zig (b867a0f59); `terminal_options` build flags (`slow_runtime_safety`, `kitty_graphics` = `-Dvt-features` work) |
| **D (terminal subsystems / kitty)** | kitty graphics_storage churn commits each carry 1-line snapshot hunks (b5e86a428, d0c516f8f, 4371871bc); screen.zig kitty-graphics encode is gated on `build_options.kitty_graphics` and one snapshot test SkipZigTest's without it |
| **compression (Phase-3 B)** | snapshot imports `../compress.zig` (page payload compression on the wire); no range churn, but the codec is a new consumer of that API |

---

## 6. UB/SB hazards for the port (PORT_RULES raw-pointer discipline)

The snapshot codec is a **bulk reader/writer of Page memory** — exactly the shape
that produced rustty's known SB findings (memory: search-screen-sb-ub-finding,
rust-mut-args-strongly-protected):

1. **Decode-into-Page (2fc238ed0, 3e5d12835):** upstream decodes staged payloads
   directly into a live `Page`, then constructs pins/rows/cells from it
   (screen.zig:394-425 end state: `pages.pin(...)`, `pages.trackPin(...)`,
   `pin.rowAndCell()`). In Rust this is a long-lived alias into PageList memory
   interleaved with `&mut` PageList calls — the search-subsystem bug shape. Route
   row/cell access through raw-ptr helpers with narrow scope; **pre-port a
   LIFETIMES.tsv row set for `snapshot::screen`/`snapshot::page` decoded-state
   structs** (adopt-lifetimes-tsv is locked policy) and prove the interleave on a
   Miri harness (shaper-hook precedent) before trusting hand reasoning.
2. **Encode-side (e89ff37aa "encode screen pages safely", `assert_integrity`
   seams):** encoding walks pages while computing integrity checks; per
   rust-mut-args-strongly-protected, do not pass `&mut Page`/`&Row` across
   re-reads — raw-ptr args.
3. **Transactional decode (page.zig:319 comment):** "on failure, all page, pin,
   hyperlink, and style state is rolled back" — the rollback path releases
   ref-counted style/hyperlink entries (9a5279db6, b5290e74c). Rust drop-order +
   raw-ptr mix here is the most leak/UAF-prone corner; the release-on-error tests
   from those two commits are the regression net — carry every one.
4. **Vectorized grid codec (973f619a2, 2aaad3ca9):** `@Vector` loads over cell
   words; in Rust keep loads on `[u64]`-typed owned buffers, not on `*Cell`
   reinterprets, to avoid provenance games; assert scalar/SIMD equivalence in tests.
5. **TinyIo:** raw-syscall port is `unsafe`-heavy → R9 accounting up front; DG5
   abort-at-C-ABI applies at every vtable-equivalent boundary.

---

## 7. Dropped-test targets and gate coverage

From `w0/g1b-after-at-12967b68f.txt`: **no RED row belongs to this area's file-set**
— the gate's module map has no `snapshot`, `crc32c`, `tinyio`, or
benchmark/synthetic-new-file rows at all. Three RED rows are *caused by* commits
whose benchmark hunks I own but whose core belongs to area A: `formatter drop=2`
(2ed67cadd), `apc drop=4` (b53728241), `stream drop=8` (68beeeb3f).

**Gate-coverage gap (action for the port, per derived-gate-green-can-be-bookkeeping):**
when this area lands, G1b's zig/rust module map must gain rows for the 15 snapshot
modules (127 tests), crc32c (5), TinyIo-analogue (20), the 3 new benchmarks (8),
Styled (4), and the Ascii/TerminalStream test additions — otherwise the finale's
G1b GREEN would be bookkeeping, not coverage. Likewise G3b's reference-comment
counts will only see the new subsystem once the rustty files exist
(`g3b-after-at-12967b68f.txt` contains zero snapshot references today).

---

## 8. Coverage honesty

- **Read line-by-line:** `snapshot/main.zig` (complete, 182 L — the format
  documentation), `envelope.zig` (complete), `io.zig` (complete), `AGENTS.md`
  (complete), `crc32c.zig` lines 1–120 + test/backend scan, TinyIo.zig lines 1–60
  (doc + vtable head), grid.zig lines 1–80 (format doc), record.zig Tag enum +
  API-line scan, `verify-kaitai.py` header, `complete-v1.hex` header lines, and the
  FULL diffs of: b53728241, 68beeeb3f (both complete incl. commit message),
  7031c892b (complete), plus per-commit `--numstat` for **all 73 commits**.
- **Sampled/structural only:** the bodies of the 15 snapshot .zig files beyond the
  headers named above (14.5k lines) were mapped via import-graph grep
  (`grep @import` across all files), `pub const/pub fn` API scans (record.zig),
  test-count greps (`^test` per file), and targeted greps (`build_options`, `pin`).
  I did NOT read screen.zig/terminal.zig/page.zig/grid.zig/snapshot.zig hunk-by-hunk
  across their 60+ owning commits — with a 0-line rustty counterpart there is no
  divergence to find, and Phase-4 rules say port the END STATE from the working
  tree, which the porting tasks must read in full.
- **rustty-side checks run:** dir listings of `crates/rustty-vt/src`,
  `crates/rustty-bench/src`, `crates/rustty-synthetic/src{,/cli}`; greps for
  `GHOSTSNP|RUSTTSNP` (exit 1), `crc32c` (only unrelated zlib/wuffs/png hits),
  `TinyIo|tiny_io` (none), `line_min|line-max` in ascii.rs (exit 1),
  `continuation` in terminal_stream.rs (none), `Styled|styled` in synthetic
  cli.rs/lib.rs (none), snapshot/formatter/resize actions in rustty-bench (none),
  `stream_continuation` in rustty-vt lib.rs (none).
- **Verified negative:** `src/terminal/compress` has zero commits in range
  (empty `git log` output shown in my working notes).
- No code was modified in either repo; the oracle was only read.

---

## 9. Port-sequencing recommendation (non-binding)

crc32c (leaf, 5 tests, differential-testable against `std.hash.crc.Crc32Iscsi`
vectors) → io.zig + envelope + record framing (goldens: envelope-v1,
record-page-header-v1) → style/hyperlink (goldens: style-v1, hyperlink-*) → grid
(grid-v1; port the END-STATE vectorized form with scalar oracle tests) → page
(page-*, needs compress) → screen/terminal/history (screen-*, terminal-header,
history-header; needs area-A stream_continuation for nothing yet) → checkpoint +
continuation (needs stream_continuation; continuation-* goldens) → snapshot.zig
codec + Decoder (complete-v1, checkpoint-ready-v1) → fixture harness + kaitai
verification wiring → benchmarks/synthetic → C-API seam with area C (TinyIo
analogue + retain-continuation options). Each stage has its own goldens, so
per-sub-batch scoped gating (per-subbatch-scoped-g3b) applies cleanly.
