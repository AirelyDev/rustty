# WS12 — vaxis Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **Tasks 1 and 2 are BLOCKING human-decision/pre-flight gates — do not start Task 3 until they are closed.**

**Goal:** Leave `crates/rustty-vaxis` with **zero non-`#[cfg(windows)]` `unimplemented!()` seams** — the four image-tier functions (`Vaxis::transmit_image`, `Vaxis::load_image`, `Image::draw`, `Image::cell_size`) become LIVE with real KATs; ratify (or reverse) the uucode width-table fork with the decision recorded in-repo instead of hanging open in three ledgers; and convert `+boo` from "believed working" into an **executed, committed, reproducible end-to-end pty KAT**. Along the way, un-BLOCK the vaxis lane of the test-count gate, which is currently vacuous on this machine.

**Architecture:** `rustty-vaxis` is the vendored-dep port of `libvaxis` 0.5.1 (rockorager, hash `vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS`), 13,613 LOC across chunks 1a–4b, consumed by `rustty-cli` (`list_colors`/`list_keybinds` pretty-print, the `list_themes` fuzzy picker, and `+boo`). Two independent sub-systems remain open. (1) **Unicode data:** already resolved as uucode **Option B** — a vendored `uucode`-0.1.0 runtime subset at `crates/rustty-vaxis/src/uucode/` whose stage1/stage2 tables are regenerated at build time by `crates/rustty-vaxis/build.rs` (552 LOC) from a 3.7 MB vendored UCD 16.0.0 corpus at `crates/rustty-vaxis/ucd/`. WS12 ratifies that fork and makes its correctness gate non-vacuous. (2) **Image tier:** the vaxis `Image`/kitty-graphics surface. Its two _portable_ halves (`transmit_local_image_path`, `transmit_pre_encoded_image`, `free_image`) are already LIVE; the four remaining functions were seamed because vaxis reaches them through the unported Zig dep `zigimg`. WS12 fills all four: `Image::draw`/`Image::cell_size` need only `Window` geometry (zero decode), and the two `Vaxis` transmit paths get a new `rustty-zigimg` crate that re-homes the exact zigimg surface vaxis instantiates (decode → `rustty-wuffs`, the codec ghostty itself vendors; pixel-format convert + PNG encode written locally over `zlib-sys`).

**Tech Stack:** Rust 1.93.1 (pinned), `rustty-vaxis` (`libc` only today), `rustty-wuffs` (safe wrapper over vendored Wuffs `png`/`jpeg` decoders), `crates-sys/zlib-sys` (vendored zlib 1.3.1: `deflateInit2_`/`deflate`/`deflateEnd`, `MAX_WBITS`, `adler32_checksum`), Python 3 `pty`/`termios`/`fcntl` for the `+boo` end-to-end harness, oracle git repo at `/Users/gael/projects/ghostty` (working tree pinned at `4749c4e93`), vendored-dep tarballs from `deps.files.ghostty.org` / GitHub (URLs pinned in `../ghostty/build.zig.zon.json`).

## Global Constraints (inherited from `2026-07-23-rustty-completion-ROADMAP.md`)

- R1 full fidelity vs the ghostty oracle at `../ghostty` (pinned `4749c4e93`); the oracle is reference-only — no build-time dependency on it (R3). For this WS the _primary_ reference is the **vendored dep** `libvaxis` 0.5.1, not ghostty `src/`.
- **NEVER `git checkout` in the oracle repo** — five other lanes depend on its working tree staying at the pin. Read post-pin upstream with `git -C ../ghostty show origin/main:<path>`. See `.port/ORACLE_SYNC.md`.
- R2 product identity: rustty naming with per-token casing preservation (Ghostty→Rustty, ghostty→rustty); `ghostty_*` C-ABI aliases stay. The new crate is `rustty-zigimg` (same convention as `rustty-z2d`, `rustty-zf`, `rustty-vaxis` — the vendored-dep-port pattern).
- R3: no build-time dependency on the oracle or on any network fetch. The UCD corpus and every C codec are already vendored in-tree; Task 3's dep-tarball restore writes ONLY into `~/.cache/zig/p/` (a _verification_ input for `check_test_counts.py`), never into the build graph.
- R4/R5: acyclic crate graph; gates (`check_acyclic.sh`, A4 ABI gate, Miri lanes) are invariant — never weakened to pass. `rustty-zigimg` deps only `rustty-wuffs` + `zlib-sys` (both leaves); `rustty-vaxis → rustty-zigimg` is a new DAG edge with no back-edge.
- DG5: C-ABI panic policy is ABORT. (WS12 touches no C-ABI surface.)
- Toolchain: pinned Rust 1.93.1 (`rust-toolchain.toml`). LTO stays banned.
- Zig byte-slices → `Vec<u8>`/`&[u8]`, never lossy `String` (memory: zig-byteslice-to-rust-rawbytes). Applies to `Image::Source::{Path,Mem}`, the base64 payloads, and the PNG bytes.
- Strip snark/gag editorializing when porting prose (memory: strip-ghostty-gag-prose). `Vaxis.zig`'s `// HACK: yuck. there has to be a better way` is a technical note, not a gag — the existing port keeps it; keep it that way.
- Ledger discipline: `.port/phase-2/DEFERRALS.md` carries **four** stale/contradictory WS12 entries (enumerated below); Task 12 rewrites them in the same PR.
- Commit style: `subsystem: summary (WS12 T<n>)`. Branch `ws12-vaxis-completion`; merge to main only with BOTH platforms green (`cargo check --workspace --all-targets` + `cargo build --workspace` on macOS AND on the VM).
- `cargo check --workspace --all-targets` is the real compile gate. `cargo build --workspace` compiles neither examples nor test targets and has let breakage through — never substitute it.
- **Machine load:** other build/Miri lanes run concurrently. Check `df -h /` before any heavy build; abort if `/` is above 90% used (a build already died of ENOSPC once in this program). Prefer `cargo check -p <crate>` over workspace builds while iterating. **Do NOT run benchmarks** — results are invalid under contention. **Do NOT run `.port/tools/gates/no_homebrew_gettext_gate.sh`** (mutates Homebrew state, breaks concurrent lanes).

---

## PREMISE CORRECTION (read before anything else)

The ROADMAP row for WS12 (`2026-07-23-rustty-completion-ROADMAP.md:81`) reads:

> Resolve the uucode width-table fork (presents **Option A** extend rustty-unicode vs **Option B** vendor uucode-0.1.0 tables to the human — decision REQUIRED in-plan, not deferred); zigimg image tier (`transmit_image`/`load_image`/`Image::draw`/`cell_size`); verify `boo` end-to-end.

**Two of its three clauses are stale.** Verified at plan-authoring time on `main` @ `ab2c08c`:

| ROADMAP claim                                       | Reality on `main`                                                                                                                                                                       | Evidence                                                                                                                                                                                                                                                                                                                                                               |
| --------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| The uucode fork is an **open** decision             | **DECIDED (Option B) AND SHIPPED** 2026-07-10 in commit **`e0abb03`** ("rustty: vaxis chunk 1b — uucode-0.1.0 unicode-data foundation + gwidth/unicode (tables bit-exact, Miri-clean)") | `crates/rustty-vaxis/src/uucode/{mod,get,grapheme,tables,types,utf8}.rs` (1,035 LOC), `crates/rustty-vaxis/build.rs` (552 LOC), `crates/rustty-vaxis/ucd/` (3.7 MB UCD 16.0.0 + `PROVENANCE.md`), `gwidth.rs` (311 LOC, **12** `#[test]`), `unicode.rs` (94 LOC). `.port/phase-2/VAXIS_C1B_DESIGN.md:19` states verbatim: "**uucode DECISION (given):** Option **B**". |
| `boo` needs verifying (implying it may be unported) | **`boo` is fully LIVE**, un-deferred 2026-07-?? in commit **`e7e5a8c`** ("rustty: un-defer +boo — vxfw widget-framework subset + framedata asset")                                      | `crates/rustty-cli/src/boo.rs` = 525 LOC with a real `run()` at :406, no `unimplemented!()`; dispatched at `crates/rustty-cli/src/rustty.rs:386` (`Action::Boo => boo::run()`); test `decompress_frames_round_trip` (boo.rs:496). And it **actually runs** — see "Environment findings" below.                                                                         |
| zigimg image tier is open                           | **TRUE — this is the only genuine product gap.**                                                                                                                                        | `crates/rustty-vaxis/src/vaxis.rs:997` (`transmit_image`), `:1018` (`load_image`), `crates/rustty-vaxis/src/image.rs:231` (`draw`), `:257` (`cell_size`) are the crate's only non-`#[cfg(windows)]` `unimplemented!()` bodies.                                                                                                                                         |

**A fourth item the ROADMAP does not mention, found while probing, and squarely in WS12's lane:**

**The vaxis lane of the test-count gate (G2) is BLOCKED, not green.** `/usr/bin/python3 .port/tools/gates/check_test_counts.py` exits with:

```
TEST-COUNT GATE: BLOCKED — missing zig reference file(s); the external
oracle/dep checkouts are absent or incomplete (R3), so the per-module
counts cannot bind (a missing ref would vacuously count zig=0):
  zf:     /Users/gael/.cache/zig/p/zf-0.10.3-OIRy8RuJAACKA3Lohoumrt85nRbHwbpMcUaLES8vxDnh/src/zf/zf.zig
  vaxis:  /Users/gael/.cache/zig/p/vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS/src/Cell.zig
  … (17 vaxis files, 3 zf files, 18 libxev files)
```

`~/.cache/zig/` exists but contains **no `p/` directory at all** — the entire vendored-dep source corpus the gate reads (`GHVAXIS` at `check_test_counts.py:200`) has been evicted from this machine. The gate is _correctly_ designed (it BLOCKS rather than silently counting `zig=0`), so nothing false has been asserted — but WS12's own test-count contract cannot bind until the corpus is restored. Task 3 restores it from the hash-pinned tarballs. Confirmed reachable at authoring time:

```
$ curl -m 25 -sSI https://deps.files.ghostty.org/vaxis-7dbb9fd3122e4ffad262dd7c151d80d863b68558.tar.gz
HTTP/2 200 … content-length: 1209803
```

**Not applicable to this crate (do not chase it):** `check_comment_coverage.py --crate rustty-vaxis` answers `RED: unknown crate 'rustty-vaxis' (not in crate_map)`. That is by design — the crate_map is the ghostty `src/` partition, and `crates/rustty-vaxis/Cargo.toml:6-8` states that a vendored dep "is absent from that crate_map". **G3b comment coverage is NOT a WS12 gate.** The applicable gates are: build/check, tests, clippy `-D`, fmt, `check_acyclic.sh` (G7), `check_test_counts.py` (G2, after Task 3), and Miri (G6, R6 — the crate is Miri-primary).

**Stale in-repo claims WS12 must repair (Task 12), each with its line:**

1. `.port/phase-2/DEFERRALS.md:4082-4157` — "rustty-vaxis Chunk 1b … uucode-0.1.0 width-table blocker": written as an OPEN blocker "that needs a **human decision** (Option A vs B below)" with an "Un-defer trigger". Shipped as Option B in `e0abb03`.
2. `.port/phase-2/DEFERRALS.md:4189-4205` — the Chunks 4a/4b zigimg seam block. Accurate today; struck by Tasks 6–9.
3. `.port/phase-2/DEFERRALS.md:2989-2996` — "**`boo` — STILL DEFERRED** (the crate's ONLY live seam, `boo.rs:22`)". Flatly contradicted by `DEFERRALS.md:2747` ("`boo` … ✅ RESOLVED") **in the same file**, and by boo.rs itself.
4. `crates/rustty-cli/src/lib.rs:12` — "The crate's ONLY live seam is `boo.rs`". False since `e7e5a8c`.
5. `crates/rustty-vaxis/Cargo.toml:15-16` — "gwidth.zig + unicode.zig are DEFERRED to Chunk 1b (uucode-0.1.0 width-table blocker)" and "Pure Rust, NO dependencies". Both false (1b landed; `libc` is a dep; `rustty-zigimg` will be another).
6. `crates/rustty-vaxis/src/lib.rs:60-64` — the "remaining live seams" paragraph; must shrink to the `#[cfg(windows)]` family only.
7. `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md:81` — the WS12 row.

---

## DECISION 1 (BLOCKING) — the uucode width-table fork: **RATIFY Option B as shipped**

The ROADMAP requires this fork be put to the human in-plan. It is presented here in full **even though it is already implemented**, because a shipped decision that was never ratified is exactly the kind of thing that rots into a third contradictory ledger entry. The human must either ratify or order a reversal; Task 2 records the answer.

### What vaxis actually needs

`vaxis/build.zig` instantiates `uucode` 0.1.0 with **one table, four fields**: `east_asian_width`, `grapheme_break`, `general_category`, `is_emoji_presentation`. `gwidth.zig` (216 LOC, 11 `test{}`) and `unicode.zig` (64 LOC) consume exactly: `uucode.get(.east_asian_width|.general_category|.is_emoji_presentation, cp)`, `uucode.types.EastAsianWidth`, and `uucode.grapheme.Iterator(uucode.utf8.Iterator)`.

### Option A — extend `rustty-unicode` to expose the three raw fields

| Axis                   | Assessment                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Correctness / fidelity | **Weakest.** `rustty-unicode` is ghostty's `src/unicode/` port; its embedded tables come from **uucode 0.2.0** run through **ghostty's stripped `uucode_config.zig`**, exposing only `Properties { width, width_zero_in_grapheme, grapheme_break, emoji_vs_base }`. The three fields vaxis needs are **absent from the data**, not merely unexported — verified again at authoring time: `grep -rn 'east_asian_width\|general_category\|is_emoji_presentation' crates/rustty-unicode/src/*.rs` returns **two hits, both doc-comments** in `symbols_uucode.rs:87,141`. So Option A means regenerating `rustty-unicode`'s tables from a _different_ uucode config. |
| Version skew           | Two independent skews would have to be reconciled and _proven_: 0.1.0-vanilla vs 0.2.0, and vanilla vs ghostty's folded `GraphemeBreakNoControl` break derivation. vaxis's 11 pinned `gwidth` tests + the UAX #29 corpus assert 0.1.0-vanilla results.                                                                                                                                                                                                                                                                                                                                                                                                           |
| Table size             | Best: one shared table for two consumers. `crates/rustty-unicode` is 316 KB today.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| Maintenance            | Worst in practice: couples a crate marked `PORT STATUS: done` (and consumed by `rustty-vt`, the highest-traffic crate in the tree) to a TUI dependency's needs. Any future vaxis bump can force a rustty-unicode table regeneration and re-validation of ghostty's _own_ width behavior.                                                                                                                                                                                                                                                                                                                                                                         |
| Build-time cost        | Neutral-to-better (one generator run instead of two).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| R1 implication         | Requires proving byte-equality of a 0.2.0+ghostty-config derivation against 0.1.0-vanilla for every code point. Unproven, and if it fails anywhere, the fix is Option B anyway.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Blast radius if wrong  | **Very large** — a bad regeneration silently changes ghostty's own grapheme/width behavior in `rustty-vt`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |

### Option B — `rustty-vaxis` ships its own uucode-0.1.0-derived tables (**SHIPPED**)

| Axis                   | Assessment                                                                                                                                                                                                                                                                                                                                  |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Correctness / fidelity | **Strongest.** Mirrors the exact dependency version vaxis compiles against; no cross-version inference. Table _values_ (not bit layout — the reference table is unpacked, so layout is free, see `VAXIS_C1B_DESIGN.md:§3.3`) are reproduced by a faithful Rust port of uucode's `src/build/{Ucd,tables}.zig` restricted to the four fields. |
| Version skew           | Eliminated by construction.                                                                                                                                                                                                                                                                                                                 |
| Table size             | Costs **3.7 MB of vendored UCD 16.0.0 text** (`crates/rustty-vaxis/ucd/`) — **build-time input only, never shipped in any artifact**; and a generated `OUT_DIR/uucode_tables.rs` (`STAGE1: [u32; 4352]` + a deduped `STAGE2: [u16; _]`, ~14 bits/cp packed).                                                                                |
| Maintenance            | Duplicates ~550 LOC of table-generator machinery and one UCD corpus. Isolated: a vaxis bump cannot perturb `rustty-vt`.                                                                                                                                                                                                                     |
| Build-time cost        | One `build.rs` UCD parse per clean build of `rustty-vaxis`, with `cargo:rerun-if-changed` on all five UCD inputs. Measured effect: `cargo test -p rustty-vaxis --lib` from a warm tree runs its 97 tests in **0.06 s**; the exhaustive 1,114,112-code-point `exhaustive_decode` self-check is inside that.                                  |
| Licensing              | Clean: UCD is under the Unicode License (permissive, attribution); `PROVENANCE.md` is already in `ucd/`. Task 2 verifies it names version + license + retrieval URLs.                                                                                                                                                                       |
| R1 implication         | Fully satisfied for the 4-field surface; nothing faked. Explicitly _not_ satisfied for the ~40 other uucode fields, which vaxis never instantiates and which are therefore out of scope, not deferred.                                                                                                                                      |
| Blast radius if wrong  | Confined to `rustty-vaxis`, and pinned by 12 `gwidth_*` tests + the 1093-case UAX #29 `GraphemeBreakTest.txt` corpus + `exhaustive_decode` + `spot_check`.                                                                                                                                                                                  |

### Option C — map `eawToWidth` onto ghostty's derived `width`

**Rejected, and stays rejected.** Ghostty's `width` is a _different algorithm_ (control-char `-1` sentinels, a hardcoded zero-width list, marks→0). Substituting it fakes widths — violates R1 and the standing "do not fake widths" instruction.

### Recommendation

**Ratify Option B as shipped, unchanged.** It is the only option whose fidelity story needs no unproven cross-version equivalence, its blast radius is one leaf crate, and the 3.7 MB build-time corpus plus ~550 LOC of duplicated generator are a cheap price for not coupling `rustty-vt`'s unicode behavior to a TUI library's dependency pin. Option A would only become preferable if someone first _proves_ that a 0.2.0+ghostty-config derivation reproduces 0.1.0-vanilla for all 1.1 M code points across all four fields — and having done that proof, the remaining benefit is 3.7 MB of build-time text and ~550 LOC, against a permanent coupling of the port's most load-bearing crate to vaxis. That trade is not worth it.

**What ratification obliges WS12 to do** (Task 2 — because a decision is not ratified by prose alone): prove the Option-B gate is **non-vacuous**, i.e. that the tests would actually go RED if the generated tables were wrong. Today that is asserted, not demonstrated.

- [ ] **HUMAN DECISION REQUIRED:** ratify Option B as shipped (recommended), or order a reversal to Option A. Record the answer verbatim in Task 2 before touching code.

---

## DECISION 2 (BLOCKING) — the zigimg tier: how to back `transmit_image` / `load_image`

This is the _actually_ open fork, and the ROADMAP does not surface it. Filling `Image::draw` and `Image::cell_size` needs no decision (pure `Window` geometry — Tasks 10/11). The two `Vaxis` transmit paths do, because the oracle reaches them through `zigimg`.

### The exact surface vaxis touches

From the oracle (`libvaxis` 0.5.1 `src/Vaxis.zig:983-1033`, verbatim; the same body is already preserved as a block comment at `crates/rustty-vaxis/src/vaxis.rs:962-990`):

- `img.imageByteSize()`, `img.rawBytes()`, `img.width`, `img.height`, `img.deinit()`
- `img.writeToMemory(png_buf, .{ .png = .{} })` — PNG **encode** into a caller buffer sized `imageByteSize()`
- `img.convert(.rgb24)` / `img.convert(.rgba32)` — pixel-format conversion
- `zigimg.Image.fromFilePath(alloc, path, &read_buffer)` / `fromMemory(alloc, bytes)` — format sniff + **decode**

**Six operations. That is the whole surface.**

### What zigimg actually is (measured, not estimated)

Fetched the pinned tarball (`zigimg-0.1.0-8_eo2vHnEwCIVW34Q14Ec-xUlzIoVg86-7FU2ypPtxms`, commit `d7b7ab0ba0899643831ef042bd73289510b39906`) and counted:

- `src/` = **18,316 LOC** across **15 format modules** (`bmp farbfeld gif iff jpeg pam netpbm pcx png qoi ras sgi tga tiff xbm`), plus `color.zig` **3,173** LOC, `PixelFormatConverter.zig` **1,408**, `io.zig` 562, `ImageUnmanaged.zig` 394, `pixel_format.zig` (24 pixel formats).
- `tests/` = a further **~10,700 LOC** (`pixel_format_converter_test.zig` 2,188; `color_test.zig` 1,883; `tga_test.zig` 1,516; `png_test.zig` 1,078; …).

### Option Z-A — full faithful `rustty-zigimg` port

| Axis     | Assessment                                                                                                                                                                                       |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Fidelity | Maximal.                                                                                                                                                                                         |
| Size     | ~29 k LOC (src + tests). **Larger than WS7**, the program's biggest workstream to date, for code that **ghostty's own source never calls** (nothing in `../ghostty/src/` reaches `vaxis.Image`). |
| Risk     | Very high: 15 decoders' edge cases, a 1,408-LOC converter matrix, and a colour-space module, all with zero downstream consumer to shake them out.                                                |
| Verdict  | Wrong shape for WS12. Correct only as its own multi-wave workstream, and even then the cost/benefit is poor.                                                                                     |

### Option Z-B — scoped `rustty-zigimg` port: core + PNG module only

| Axis     | Assessment                                                                                                                                                                                                                             |
| -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Fidelity | High for PNG; **absent for the other 14 formats** — `load_image` of a JPEG/BMP/GIF would fail where the oracle succeeds.                                                                                                               |
| Size     | ~5–6 k LOC (`Image`, `ImageUnmanaged`, `io`, `pixel_format`, the needed `color` storage, `PixelFormatConverter` rgb24/rgba32 destinations, `FormatInterface`, `formats.zig`, `formats/png.zig` + `png/reader.zig` + the PNG encoder).  |
| Risk     | Medium.                                                                                                                                                                                                                                |
| Verdict  | **Leaves a residual, i.e. a deferral** ("the other 14 formats"), which the standing no-deferrals instruction forbids — and it pays 5–6 k LOC to end up with a _narrower_ format set than Option Z-C, because it drops JPEG. Dominated. |

### Option Z-C — R3 re-home onto ghostty's own vendored codecs (**RECOMMENDED**)

Keep the vaxis-side API and semantics exactly; back the six operations with code rustty already vendors, in a new leaf crate `rustty-zigimg`:

- **decode** → `rustty_wuffs::png::decode` / `rustty_wuffs::jpeg::decode`. This is **ghostty's own image decoder** (`pkg/wuffs`), already a full R1 port with its own KATs, returning `ImageData { width: u32, height: u32, data: RawBuf }` in **RGBA_NONPREMUL** — which is precisely zigimg's `rgba32`. Format sniff by magic bytes (PNG `89 50 4E 47 0D 0A 1A 0A`, JPEG `FF D8 FF`).
- **convert(rgb24|rgba32)** → a small faithful converter (rgba32 is identity after decode; rgb24 drops the alpha channel), mirroring `PixelFormatConverter`'s rgba32→rgb24 path.
- **PNG encode** → a ~150-LOC encoder (IHDR / one IDAT / IEND, filter type 0, 8-bit RGB or RGBA) over `zlib_sys` (`deflateInit2_` with `windowBits = MAX_WBITS = 15` for a zlib-wrapped IDAT stream; CRC-32 via a small pure-Rust table since `zlib-sys` exposes `adler32` but no `crc32` binding).
- `imageByteSize()`/`rawBytes()`/`width`/`height`/`deinit()` → trivial.

| Axis                             | Assessment                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Fidelity of what vaxis **emits** | **Exact.** The kitty-graphics escape bytes, chunking, base64, and error set are unchanged (the emitting halves — `transmit_pre_encoded_image` at oracle `Vaxis.zig:925`, `transmit_local_image_path` at `:866` — are already LIVE and untouched). PNG is lossless, so a PNG round-trip is byte-identical in _pixels_; the compressed bytes differ from zigimg's encoder, which is unobservable to a terminal.                                                    |
| Fidelity divergence              | **One, bounded and nameable:** the set of input file formats `load_image` accepts becomes **PNG + JPEG** instead of zigimg's 15. This is a superset of what ghostty itself supports for kitty graphics (PNG via wuffs) and for background images. It is a documented PORT-NOTE representation choice — the same class as the existing `Cow`/byte-slice and `[0u8; 8192]`-for-`undefined` notes in this crate — **not** an `unimplemented!()` and not a deferral. |
| Size                             | ~600 LOC + tests.                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| Risk                             | Low; both codecs are already exercised by existing KATs.                                                                                                                                                                                                                                                                                                                                                                                                         |
| R3                               | Clean — nothing new is vendored.                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| R4                               | Clean — `rustty-zigimg → {rustty-wuffs, zlib-sys}`, both leaves; `rustty-vaxis → rustty-zigimg`. No cycle.                                                                                                                                                                                                                                                                                                                                                       |
| R6 / Miri                        | The crate becomes FFI-touching. **Precedent exists**: `rustty-vt` links `zlib-sys` and still runs a Miri lane (`.port/phase-2/mirihar-pagelist-compress` runs under `cargo +nightly miri`). Miri only fails when a foreign function is _called_, so the decode/encode tests get `#[cfg_attr(miri, ignore)]` plus a `MIRI_EXCEPTIONS` row; every pure `rustty-vaxis` test stays Miri-live.                                                                        |
| Precedent in the ledger          | `DEFERRALS.md:4196` itself blesses this: "Un-defer trigger: a `rustty-zigimg` port (**or an R3 pure-Rust re-home of the two conversions**)".                                                                                                                                                                                                                                                                                                                     |

### Recommendation

**Option Z-C.** It makes all four functions LIVE (the WS12 ask) for ~600 LOC instead of ~29 k, uses ghostty's own vendored decoder rather than importing a second independent codec stack, keeps the emitted escape bytes byte-exact, and confines its single fidelity divergence to a surface no ghostty code path touches. Z-A is a workstream, not a task; Z-B is strictly dominated by Z-C (more code, narrower format support).

- [ ] **HUMAN DECISION REQUIRED:** approve Option Z-C (recommended), or direct Z-A / Z-B. Record the answer verbatim in Task 2 before starting Task 6.

---

## Environment findings (probed at authoring time — the plan is written against these)

### `+boo` runs end-to-end **today**, and the harness needed to prove it is non-obvious

Driven against the `main` debug binary (`/Users/gael/projects/rustty/target/debug/rustty`, built `Jul 29 10:56`) through Python `pty.fork()`:

1. **A pty with no winsize renders nothing.** `script -q /dev/null rustty +boo` (stdin a pipe ⇒ winsize 0×0) emits only `ESC[?2026h ESC[?25l ESC[H ESC[m ESC[?2026l` per frame — 2,640 bytes total, zero cell content. **The harness MUST `ioctl(TIOCSWINSZ)` on the master.** (Also note `boo`'s own floor: `boo.rs:465` prints "Screen must be at least 100w x 41h" below 100×41.)
2. **With a 205×50 winsize it renders real frames.** 4 s capture = **153,113 bytes / 73 synchronized-update frames** (≈18 fps under 5-lane contention; target is `1000/30`), containing the ghost ASCII art with real SGR colour runs (`ESC[34m` outline / `ESC[39m` default) and cursor addressing — e.g. `ESC[35;67H ESC[34m o ESC[35;69H ==`.
3. **SIGWINCH is honoured.** Resizing 205×50 → 30×120 mid-run drops the output rate from 59,399 bytes/2.5 s to 3,245 bytes/3 s. This is the **positive control that `Loop` event delivery works**, since the winsize path goes through `Loop::winsize_callback` → `queue.push` → the main loop's `try_event()`.
4. **The tty read thread really drains input.** Writing 4 KiB chunks non-blocking to the master: **249,368 bytes accepted, only 58 EAGAINs** over ~6 s. With no reader, a pty input queue saturates after a few KiB. Positive control that `Loop::tty_run` → `Tty::read` → `Parser::parse` is live.
5. **The trap that makes `+boo` _look_ broken, and its resolution.** Sending ESC leaves the process alive indefinitely, and neither ESC nor `0x03` (ctrl-c) appears to quit. **They do quit.** The last bytes of the capture are `…ESC[?2026l ESC[5n` — `ESC[5n` is emitted only by `Loop::stop()` (`loop_.rs:215`, the DSR "wake the blocked reader" write). So `ctx.quit` was set, `App::run` returned, and the hang is in `Loop::stop()`'s `handle.join()`: the read thread is blocked in `read(/dev/tty)`, and the DSR wake only works against a terminal that **answers** `ESC[5n` with `ESC[0n`. A dumb pty never answers ⇒ join blocks forever. **This is upstream-faithful vaxis behavior, not a rustty defect.**
   - Harness answering DSR: `RESULT: EXITED (master EOF) status=0 0.01s after ESC`.
   - Negative control, same harness with `--no-reply`: `RESULT: ALIVE 5s after ESC (bytes=97319)`.
   - Therefore the committed KAT (Task 5) **must** implement a minimal terminal responder. The startup query block it must satisfy, captured verbatim: `ESC[?1049h ESC[?1016$p ESC[?2027$p ESC[?2031$p ESC[?2048h ESC[H ESC]66;w=1; ESC\ ESC[6n ESC[H ESC]66;s=2; ESC\ ESC[> q ESC[6n ESC[>0q ESC[?u ESC_Gi=1,a=q ESC\ ESC[c ESC[?2004h ESC[?996n ESC[?2031h ESC[?1002;1003;1004;1006h ESC[H ESC[m ESC[J`. Answering `ESC[6n`→`ESC[1;1R`, `ESC[c`→`ESC[?62;22c`, `ESC[?u`→`ESC[?0u`, `ESC[?996n`→`ESC[?997;1n`, and **`ESC[5n`→`ESC[0n`** is sufficient.
6. **`+list-themes` cannot serve as a comparison TUI here**: it exits in 30 ms with "Could not find the Rustty resources directory" — the resource tree is WS1's business, not WS12's. Do not chase it.

### Vendored-dep sources are absent locally but restorable

- `~/.cache/zig/p/` does not exist. Three roots are needed by `check_test_counts.py`: `vaxis-0.5.1-BWNV_…`, `zf-0.10.3-OIRy8…`, `libxev-0.0.0-86vtc4…`.
- URLs are recorded in `../ghostty/build.zig.zon.json` and `build.zig.zon.txt` and were live at authoring time (HTTP 200 for the 1,209,803-byte vaxis tarball; GitHub 302→200 for zigimg).
- **The zigimg tarball is NOT needed by any gate** — it was fetched only to size Decision 2. Do not add it to the restore script's required set.

### Baselines (record these; every Task-13 assertion is a delta from them)

| Measurement                                   | Value at `ab2c08c`                                                                                                                                                                                                                                                                                                                                                                    | Command                                                     |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| `rustty-vaxis` lib tests                      | **97 passed, 0 failed, 0 ignored** in 0.06 s                                                                                                                                                                                                                                                                                                                                          | `cargo test -p rustty-vaxis --lib`                          |
| `rustty-vaxis` non-windows `unimplemented!()` | **4** (`vaxis.rs:997`, `vaxis.rs:1018`, `image.rs:231`, `image.rs:257`)                                                                                                                                                                                                                                                                                                               | `grep -rn 'unimplemented!' crates/rustty-vaxis/src/`        |
| `gwidth.rs` tests                             | 12 (`gwidth_a`, `gwidth_emoji_with_zwj`, `gwidth_emoji_with_vs16_selector`, `gwidth_emoji_with_skin_tone_selector`, `gwidth_zero_width_space`, `gwidth_zero_width_non_joiner`, `gwidth_combining_marks`, `gwidth_flag_emoji_regional_indicators`, `gwidth_text_variation_selector`, `gwidth_keycap_sequence`, `gwidth_base_letter_with_combining_mark`, `gwidth_missing_wide_anchor`) | `grep -n '#\[test\]' -A1 crates/rustty-vaxis/src/gwidth.rs` |
| uucode table tests                            | `uucode::tables::tests::exhaustive_decode`, `uucode::tables::tests::spot_check`                                                                                                                                                                                                                                                                                                       | ditto on `src/uucode/tables.rs`                             |
| UAX #29 conformance tests                     | `uucode::grapheme::tests::grapheme_break_test_compute_grapheme_break` (:570), `…::grapheme_break_test_is_break` (:575)                                                                                                                                                                                                                                                                | ditto on `src/uucode/grapheme.rs`                           |
| Crate graph                                   | `GREEN: rustty crate graph is acyclic (G7)`                                                                                                                                                                                                                                                                                                                                           | `bash .port/tools/gates/check_acyclic.sh`                   |
| Test-count gate                               | **BLOCKED** (see Premise Correction)                                                                                                                                                                                                                                                                                                                                                  | `/usr/bin/python3 .port/tools/gates/check_test_counts.py`   |

---

## Non-vacuity controls (the plan's own falsifiability contract)

Five-plus vacuous assertions have already been found in this program. Every check below names how it can FAIL. **A step whose check cannot fail is a plan defect — stop and fix the plan, do not tick the box.**

| Check                              | Negative / positive control that makes it falsifiable                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| uucode tables are correct (Task 4) | **Negative control, executed:** in a scratch copy of the tree, perturb one `STAGE2` entry emitted by `build.rs` (e.g. force `east_asian_width` to `Neutral` for `U+4E00`) and confirm `spot_check` AND at least one `gwidth_*` test go RED. If they stay green the tests do not read the tables and the gate is vacuous.                                                                                                                                                                                                                                                                        |
| UCD provenance (Task 4)            | Compare recorded SHA-256s in `ucd/PROVENANCE.md` against the on-disk files; mutate one byte of a scratch copy and confirm the check goes RED.                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| Test-count gate binds (Task 3)     | Before: gate prints `BLOCKED`. After: gate prints a per-module PASS for `vaxis`. **Control:** delete one restored file and confirm the gate returns to `BLOCKED` rather than silently passing with `zig=0`.                                                                                                                                                                                                                                                                                                                                                                                     |
| `+boo` end-to-end (Task 5)         | Three arms, all asserted: (a) with a replying terminal, ESC ⇒ exit status 0 within 2 s; (b) **negative control** — the same run with DSR replies suppressed does NOT exit within 2 s (proves the exit in (a) is caused by the key, not by a timeout or a crash); (c) frame assertion — the capture contains ≥10 `ESC[?2026h` frames AND ≥1 `ESC[34m` colour run AND ≥1 cursor address with column ≥ 60 (proves _content_, not just an empty sync-update pair; the 0×0-winsize run from finding 1 is the recorded example of a capture that satisfies "it emitted bytes" while proving nothing). |
| PNG encoder (Task 7)               | Round-trip: `wuffs::png::decode(encode(pixels)) == pixels` bit-exact. **Negative control:** flip one byte of the encoded IDAT and assert decode fails or mismatches — proves the round-trip is not comparing a value to itself.                                                                                                                                                                                                                                                                                                                                                                 |
| `transmit_image` (Task 8)          | Assert the **exact byte string** emitted for a fixed 2×2 image (opener params + base64 payload + `ESC\`), not merely `is_ok()`. **Negative control:** the `caps.kitty_graphics == false` path must return `Err(ImageError::NoGraphicsCapability)` and write **zero** bytes.                                                                                                                                                                                                                                                                                                                     |
| `Image::draw` (Task 10)            | Assert the resulting `Placement.options.size` for **each** of the four `Scale` arms plus the `contain`-fits early-break, against hand-computed values from the oracle body. **Negative control:** a `Scale::None` case must leave `size == None` — if all four arms produced the same answer the test would be vacuous.                                                                                                                                                                                                                                                                         |
| `Image::cell_size` (Task 11)       | Assert both the normal `divCeil` result and the two `catch 0` arms (zero `pix_per_col`/`pix_per_row`). **Negative control:** a case where naive integer division and `divCeil` differ (e.g. 100 px image over 30 px/col ⇒ 4, not 3).                                                                                                                                                                                                                                                                                                                                                            |
| No regression (Task 13)            | `cargo check --workspace --all-targets` (NOT `cargo build --workspace`) plus the release lane gate `check_release_lane.sh`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |

---

## File Structure

- `crates-sys/`, `Cargo.toml` — **modify** `Cargo.toml` workspace `members` + `[workspace.dependencies]` to add `rustty-zigimg`.
- `crates/rustty-zigimg/` — **NEW**. `Cargo.toml`, `src/lib.rs` (the `Image` value type + `PixelFormat` + `Error`), `src/decode.rs` (magic-byte sniff → `rustty-wuffs`), `src/convert.rs` (rgba32→rgb24), `src/png_encode.rs` (IHDR/IDAT/IEND + CRC-32 + zlib IDAT), `tests/` KATs.
- `crates/rustty-vaxis/Cargo.toml` — **modify**. Add the `rustty-zigimg` dep; rewrite the stale header block (Premise Correction item 5).
- `crates/rustty-vaxis/src/vaxis.rs` — **modify**. Fill `transmit_image` (:997) restoring the dropped `img` parameter, and `load_image` (:1018). Delete both R5-seam preambles, keeping the oracle body as ordinary line-by-line reference comments per house style.
- `crates/rustty-vaxis/src/image.rs` — **modify**. Fill `draw` (:231) restoring the dropped `win: Window` parameter, and `cell_size` (:257) likewise.
- `crates/rustty-vaxis/src/lib.rs` — **modify**. Shrink the "remaining live seams" paragraph (:60-64) to the `#[cfg(windows)]` family.
- `crates/rustty-vaxis/ucd/PROVENANCE.md` — **modify** if it lacks per-file digests (Task 4).
- `crates/rustty-vaxis/tests/image_tier.rs` — **NEW**. The `draw` / `cell_size` / `transmit_image` / `load_image` KATs.
- `crates/rustty-cli/src/lib.rs` — **modify**. Repair the ":12" claim.
- `crates/rustty-cli/tests/boo_pty_e2e.rs` — **NEW**. The `+boo` end-to-end KAT (`#[ignore]` by default; run explicitly and by the WS12 gate script).
- `.port/tools/boo_pty_kat.py` — **NEW**. The pty terminal-responder harness the KAT shells out to.
- `.port/tools/restore_zig_deps.sh` — **NEW**. Hash-pinned restore of the three vendored-dep source roots into `~/.cache/zig/p/`.
- `.port/phase-2/MIRI_EXCEPTIONS.md` — **modify**. One row for the FFI-touching `rustty-zigimg` tests.
- `.port/phase-2/DEFERRALS.md` — **modify**. Rewrite items 1–3 of the Premise Correction.
- `.port/phase-2/VAXIS_C1B_DESIGN.md` — **modify**. Add a one-line RATIFIED banner (Task 2).
- `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md` — **modify**. Correct the WS12 row.

---

## Task 1 — Branch + pre-flight

- [ ] `df -h /` — abort the whole workstream if `/` is above 90% used and report back instead.
- [ ] `git -C /Users/gael/projects/rustty status --short` to confirm no other lane is mid-edit in the files above; work only in this worktree.
- [ ] Create the branch: `git switch -c ws12-vaxis-completion`.
- [ ] Record the baselines from the "Baselines" table by actually running each command; paste outputs into the task log. Do **not** copy the numbers from this plan — a baseline you did not run is a vacuous baseline.
- [ ] Confirm the four seam line numbers are still `vaxis.rs:997`, `vaxis.rs:1018`, `image.rs:231`, `image.rs:257`; if they have drifted, update them everywhere in this plan before proceeding.

No commit.

## Task 2 — Close both decisions (BLOCKING, no code)

- [ ] Put **Decision 1** and **Decision 2** to the human, in that order, presenting the tradeoff tables above and the two recommendations (ratify Option B; approve Option Z-C).
- [ ] Record both answers verbatim in a new `## WS12 decisions (2026-07-2x)` section at the top of `.port/phase-2/VAXIS_C1B_DESIGN.md`, naming who decided and when.
- [ ] If Decision 1 is a reversal to Option A: **stop and re-plan** — Option A is a multi-task workstream of its own (regenerate `rustty-unicode`'s tables from a 4-field-extended config, extend its differential harness, then prove 0.1.0-vanilla equivalence for all 1,114,112 code points across four fields). Do not attempt it inside WS12.
- [ ] If Decision 2 is Z-A or Z-B: **stop and re-plan** Tasks 6–9 accordingly; Task 6's sizing (~600 LOC) does not survive either.

**Commit:** `docs: record the WS12 uucode + zigimg-tier decisions (WS12 T2)`

## Task 3 — Un-BLOCK the test-count gate: restore the vendored-dep source corpus

The gate cannot bind for `vaxis` (or `zf`/`xev_*`) while `~/.cache/zig/p/` is empty. This is WS12's own gate, so WS12 fixes it.

- [ ] Read the three URL/hash pairs out of the oracle without checking anything out: `git -C ../ghostty show HEAD:build.zig.zon.json | python3 -c "import json,sys; d=json.load(sys.stdin); [print(k, v['url']) for k,v in d.items() if k.split('-')[0] in ('vaxis','zf','libxev')]"`.
- [ ] Write `.port/tools/restore_zig_deps.sh`:
  - Hard-code the three `(<zig-hash-dir>, <url>, <sha256>)` triples. Compute each SHA-256 once, from the downloaded tarball, and bake it in — a restore script without a digest check is a supply-chain hole.
  - For each: skip if `~/.cache/zig/p/<hash>/` already exists and is non-empty; else download to a temp file, verify SHA-256, extract, and move the single top-level directory into place under the zig-hash name.
  - Refuse to run if `~/.cache/zig/p` is not writable; print a one-line summary per root (`RESTORED` / `PRESENT` / `FAILED`).
  - Header comment must state: **verification input only, never a build dependency (R3)** — nothing in `Cargo.toml` or any `build.rs` may read `~/.cache/zig`.
- [ ] Run it: `bash .port/tools/restore_zig_deps.sh`.
- [ ] `/usr/bin/python3 .port/tools/gates/check_test_counts.py` — must no longer print `BLOCKED`, and must print a per-module result for `vaxis`. **If the `vaxis` module now reports a MISMATCH, that is a real finding**: record the exact numbers, do not adjust the expected count to match. Triage it against the `check_test_counts.py:207` `vaxis` file list (which covers `Cell/Key/Mouse/GraphemeCache/Image/gwidth/unicode/Screen/InternalScreen/Window/Parser/event/ctlseqs/queue/tty/Loop/widgets/TextInput` — note it deliberately omits `Vaxis.zig` and `vxfw/`).
- [ ] **Negative control:** `mv ~/.cache/zig/p/vaxis-0.5.1-BWNV_*/src/Cell.zig{,.bak}`, re-run the gate, confirm it returns to `BLOCKED` (not a silent pass), then restore the file.

**Commit:** `tools: restore_zig_deps.sh — hash-pinned vendored-dep source restore un-BLOCKs G2 (WS12 T3)`

## Task 4 — Make the uucode Option-B gate non-vacuous

Ratification without a falsifiable gate is just prose. This task proves the 12 `gwidth_*` tests + `exhaustive_decode` + `spot_check` + the two UAX #29 conformance tests actually read the generated tables.

- [ ] Verify `crates/rustty-vaxis/ucd/PROVENANCE.md` records, for each of the six vendored UCD inputs (`UnicodeData.txt`, `extracted/DerivedEastAsianWidth.txt`, `emoji/emoji-data.txt`, `auxiliary/GraphemeBreakProperty.txt`, `auxiliary/GraphemeBreakTest.txt`, `DerivedCoreProperties.txt`): Unicode version (16.0.0), retrieval URL, and a SHA-256. Add whatever is missing (compute with `shasum -a 256`).
- [ ] Add `crates/rustty-vaxis/tests/ucd_provenance.rs`: parse `PROVENANCE.md`'s digest table and assert each on-disk file matches. Must FAIL on a one-byte perturbation — verify that in a scratch copy, then discard the perturbation.
- [ ] **Negative control (the core of this task), in a scratch copy of the worktree only:** patch `build.rs` to force one wrong value (e.g. write `EAW_NEUTRAL` for `U+4E00`), run `cargo test -p rustty-vaxis --lib`, and confirm **both** `uucode::tables::tests::spot_check` and at least one `gwidth_*` test go RED. Paste the failing output into the task log. Discard the scratch copy. If either stays green, the corresponding test does not bind the tables — file it as a real gap and add a test that does.
- [ ] Confirm the UAX #29 corpus is genuinely driven: `grapheme.rs:505-512` `include_str!`s `ucd/auxiliary/GraphemeBreakTest.txt` (compile-time embed, deliberately so the corpus also runs under Miri isolation). Assert the parsed case count is non-zero by adding `assert!(cases > 0)` to `test_grapheme_break` — a corpus-driven test that silently iterates zero lines is the classic vacuous check.
- [ ] `cargo test -p rustty-vaxis --lib` — still 97 + the new provenance test, all green.

**Commit:** `rustty-vaxis: UCD provenance digests + non-vacuity pins for the uucode-0.1.0 tables (WS12 T4)`

## Task 5 — `+boo` end-to-end KAT (executed, committed, reproducible)

- [ ] Write `.port/tools/boo_pty_kat.py` (std-lib only; `pty`, `fcntl`, `termios`, `struct`, `select`):
  - `pty.fork()` + `execv` the binary given in `argv[1]`, with `+boo`.
  - `ioctl(TIOCSWINSZ)` a **205×50** window on the master (above boo's 100×41 floor — finding 1).
  - Act as a minimal terminal: answer `ESC[6n`→`ESC[1;1R`, `ESC[c`→`ESC[?62;22c`, `ESC[?u`→`ESC[?0u`, `ESC[?996n`→`ESC[?997;1n`, and **`ESC[5n`→`ESC[0n`** (finding 5 — without this the process cannot exit).
  - After 3 s, write `ESC`; then wait up to 2 s for child exit.
  - `--no-dsr` flag suppresses only the `ESC[5n` reply (the negative-control arm).
  - Exit 0 on clean child exit; print machine-readable lines: `EXIT_STATUS=<n>`, `FRAMES=<count of ESC[?2026h>`, `COLOR_RUNS=<count of ESC[34m>`, `MAX_COL=<max column from ESC[<r>;<c>H>`, `BYTES=<n>`.
- [ ] Write `crates/rustty-cli/tests/boo_pty_e2e.rs`, `#[ignore]` by default with a doc-comment naming the exact run command (pty + a real animation loop makes it unsuitable for the default `cargo test` lane):
  - Locate the `rustty` binary via `env!("CARGO_BIN_EXE_rustty")` if `rustty-cli` produces it, else `target/<profile>/rustty`; **fail loudly** if absent rather than skipping — a self-skipping KAT is vacuous.
  - Arm A: run the harness; assert `EXIT_STATUS=0`, `FRAMES >= 10`, `COLOR_RUNS >= 1`, `MAX_COL >= 60`.
  - Arm B (negative control): run with `--no-dsr`; assert the child did **not** exit within the window. Document in-test why: `Loop::stop()` (`loop_.rs:215`) wakes the blocked reader with a DSR that only a replying terminal answers — upstream-faithful, verified in WS12 planning.
- [ ] Run both arms and paste the real output into the task log. Expected shape from planning: Arm A `EXITED status=0 0.01s after ESC`; Arm B `ALIVE 5s after ESC (bytes=97319)`.
- [ ] Add a note to `crates/rustty-cli/src/boo.rs`'s module header: `+boo` is verified end-to-end by `tests/boo_pty_e2e.rs`, and a bare pty **must** answer DSR or shutdown blocks in `Loop::stop`.

**Commit:** `rustty-cli: +boo end-to-end pty KAT (frames + colour + clean ESC exit; no-DSR negative control) (WS12 T5)`

## Task 6 — `rustty-zigimg` crate skeleton (Option Z-C)

Oracle surface: `zigimg` `src/Image.zig:55,65,98,108,129,136` and `src/ImageUnmanaged.zig:176,221,231,268,280`.

- [ ] `mkdir crates/rustty-zigimg`; add to `Cargo.toml` `members` and `[workspace.dependencies]` (`rustty-zigimg = { path = "crates/rustty-zigimg" }`).
- [ ] `Cargo.toml` header comment must state: this crate **re-homes** the six-operation zigimg surface `libvaxis` 0.5.1 instantiates (`Vaxis.zig:983-1033`) onto ghostty's own vendored codecs per WS12 Decision 2 (Option Z-C); it is **not** a port of zigimg's 15 format modules, and that difference — input formats PNG+JPEG rather than 15 — is a documented, bounded PORT-NOTE divergence, not a seam.
- [ ] Deps: `rustty-wuffs = { workspace = true }`, `zlib-sys = { workspace = true }`, `allocator-api2 = { workspace = true }` (needed by `rustty_wuffs::png::decode`'s allocator parameter).
- [ ] `src/lib.rs`:
  - `pub enum PixelFormat { Rgb24, Rgba32 }` — the only two zigimg formats vaxis names, each with a PORT-NOTE citing `pixel_format.zig:42-43`.
  - `pub struct Image { pub width: usize, pub height: usize, pub pixel_format: PixelFormat, pixels: Vec<u8> }`, byte-slice pixels (never `String`).
  - `pub fn raw_bytes(&self) -> &[u8]` (`ImageUnmanaged.zig:221`), `pub fn image_byte_size(&self) -> usize` (`:231`, `= raw_bytes().len()`), `pub fn row_byte_size(&self)` (`:226`).
  - `pub fn convert(&mut self, dst: PixelFormat)` (`:280`) — early-return when already in `dst` (the oracle's `activeTag == destination_format` guard).
  - `pub enum Error { UnsupportedFormat, Decode, NoSpaceLeft, Io(std::io::Error) }` — `NoSpaceLeft` is required to reproduce the oracle's `writeToMemory` overflow path (see Task 7).
  - **No `Drop`/`deinit`** — Rust ownership replaces `img.deinit()`; PORT-NOTE it.
- [ ] `cargo check -p rustty-zigimg --all-targets`; `bash .port/tools/gates/check_acyclic.sh` must stay GREEN.

**Commit:** `rustty-zigimg: crate skeleton — the six-operation zigimg surface vaxis instantiates (WS12 T6)`

## Task 7 — `rustty-zigimg` decode + convert + PNG encode

- [ ] `src/decode.rs` — `pub fn from_memory(bytes: &[u8]) -> Result<Image, Error>` (`Image.zig:65`): sniff magic (`89 50 4E 47 0D 0A 1A 0A` ⇒ PNG, `FF D8 FF` ⇒ JPEG, else `Error::UnsupportedFormat`), decode via `rustty_wuffs::{png,jpeg}::decode(allocator_api2::alloc::Global, bytes)`, copy the RGBA_NONPREMUL bytes into `Image { pixel_format: Rgba32 }`. PORT-NOTE that wuffs' output format _is_ zigimg's `rgba32`.
- [ ] `pub fn from_file_path(path: &[u8], read_buffer: &mut [u8]) -> Result<Image, Error>` (`Image.zig:55`): read the file into `read_buffer` (the oracle threads a caller buffer; preserve the parameter), then `from_memory`. Path is `&[u8]` — a Zig `[]const u8` path, never a lossy `String`.
- [ ] `src/convert.rs` — rgba32→rgb24 drops every 4th byte; rgb24→rgba32 inserts `0xFF`. Mirror `PixelFormatConverter.zig`'s rgba32↔rgb24 arms; PORT-NOTE that alpha is discarded, not composited (matching the reference).
- [ ] `src/png_encode.rs` — `pub fn write_to_memory(img: &Image, out: &mut [u8]) -> Result<&[u8], Error>` (`Image.zig:129` / `ImageUnmanaged.zig:268`):
  - 8-byte signature, `IHDR` (bit depth 8; colour type 2 for `Rgb24`, 6 for `Rgba32`; deflate/adaptive/no-interlace), one `IDAT`, `IEND`.
  - Scanlines prefixed with filter byte `0`; the concatenation compressed with `zlib_sys::deflateInit2_(level = Z_DEFAULT_COMPRESSION, method = Z_DEFLATED, windowBits = MAX_WBITS /* 15 ⇒ zlib-wrapped, which is what PNG IDAT requires */, memLevel = 8, strategy = Z_DEFAULT_STRATEGY)` → `deflate(Z_FINISH)` → `deflateEnd`.
  - CRC-32 per chunk via a local pure-Rust table (`zlib-sys` exposes `adler32` but no `crc32` binding — do not add one).
  - **Return `Error::NoSpaceLeft` when the encoded PNG exceeds `out.len()`.** This is load-bearing fidelity: the oracle sizes its buffer at `img.imageByteSize()` (`Vaxis.zig:996-998`), so a noisy small image legitimately overflows upstream too. PORT-NOTE it as a preserved upstream quirk, not a bug to "fix".
- [ ] Tests in `crates/rustty-zigimg/tests/codec.rs`, all `#[cfg_attr(miri, ignore)]` (they call C):
  - `png_encode_decode_round_trip_rgba32` / `_rgb24`: synthesize a 3×2 gradient, encode, decode with `rustty_wuffs::png::decode`, assert **bit-exact** pixels.
  - **Negative control** `png_round_trip_detects_corruption`: flip one byte inside the IDAT payload and assert decode errors or mismatches.
  - `write_to_memory_no_space_left`: a 1-byte `out` buffer ⇒ `Err(Error::NoSpaceLeft)`.
  - `from_memory_rejects_unknown_magic`: `b"NOTANIMAGE"` ⇒ `Err(Error::UnsupportedFormat)`.
  - `convert_rgba32_to_rgb24_drops_alpha`: exact expected byte vector.
- [ ] Add one `MIRI_EXCEPTIONS.md` row: `crates/rustty-zigimg/tests/codec.rs` — wuffs/zlib FFI, `#[cfg_attr(miri, ignore)]`; precedent `rustty-vt`+`zlib-sys`.
- [ ] `cargo test -p rustty-zigimg`; `cargo clippy -p rustty-zigimg --all-targets -- -D warnings`.

**Commit:** `rustty-zigimg: decode (wuffs png/jpeg) + rgb24/rgba32 convert + PNG encoder over vendored zlib (WS12 T7)`

## Task 8 — `Vaxis::transmit_image` LIVE

Oracle: `libvaxis` 0.5.1 `src/Vaxis.zig:983-1015`. The verbatim body is already preserved at `crates/rustty-vaxis/src/vaxis.rs:962-990` — port it line-by-line from there.

- [ ] Add `rustty-zigimg = { workspace = true }` to `crates/rustty-vaxis/Cargo.toml` and rewrite that file's stale header block (Premise Correction item 5): 1b landed, `libc` is a dep, `rustty-zigimg` is a dep, and the crate is Miri-primary **except** for the FFI-touching image tests.
- [ ] Replace the `unimplemented!()` at `vaxis.rs:997`. **Restore the dropped parameter** — the signature becomes `pub fn transmit_image<W: Write>(&mut self, tty: &mut W, img: &mut rustty_zigimg::Image, format: TransmitFormat) -> Result<Image, ImageError>`. The current 3-arg signature (which silently dropped `img`) is what made the seam un-callable; a filled body must not keep it.
- [ ] Body, in oracle order: `if !self.caps.kitty_graphics { return Err(ImageError::NoGraphicsCapability) }`; then match `format` — `Png` ⇒ allocate `vec![0u8; img.image_byte_size()]` (the oracle's arena alloc; PORT-NOTE the arena → `Vec` mapping, the established house pattern) and `write_to_memory`; `Rgb` ⇒ `img.convert(Rgb24)` then `raw_bytes()`; `Rgba` ⇒ `img.convert(Rgba32)` then `raw_bytes()`. Base64-encode with the same encoder `transmit_local_image_path` already uses (do **not** introduce a second base64 path). Tail-call the existing LIVE `transmit_pre_encoded_image(tty, encoded, img.width as u16, img.height as u16, format)`.
- [ ] Delete the `// R5 SEAM (zigimg …)` preamble; keep the oracle body as ordinary reference comments interleaved with the Rust, matching the file's existing style.
- [ ] KATs in `crates/rustty-vaxis/tests/image_tier.rs` (`#[cfg_attr(miri, ignore)]` for the PNG arms):
  - `transmit_image_rgb_emits_exact_escape`: a 2×2 RGB image over a `Vec<u8>` sink with `caps.kitty_graphics = true`; assert the **complete expected byte string** (opener with `f=`/`s=`/`v=`/`i=`/`m=` params, the exact base64 payload, `ESC\`). Hand-compute the expected base64 in the test with a comment showing the source pixels.
  - `transmit_image_rgba_converts_then_emits`: same, asserting the payload length is `w*h*4`-derived.
  - `transmit_image_png_round_trips`: extract the base64 payload from the emitted bytes, decode it, and assert `rustty_wuffs::png::decode` recovers the original pixels.
  - **Negative control** `transmit_image_without_kitty_graphics_writes_nothing`: `caps.kitty_graphics = false` ⇒ `Err(ImageError::NoGraphicsCapability)` **and** the sink is empty.

**Commit:** `rustty-vaxis: Vaxis::transmit_image LIVE over rustty-zigimg (exact-escape KATs) (WS12 T8)`

## Task 9 — `Vaxis::load_image` LIVE

Oracle: `Vaxis.zig:1017-1033`; verbatim body preserved at `vaxis.rs:1000-1013`.

- [ ] Replace the `unimplemented!()` at `vaxis.rs:1018`. Signature keeps `src: Source`; body: capability guard, then a **1 MiB** read buffer (the oracle's `var read_buffer: [1024 * 1024]u8` — PORT-NOTE that Rust heap-allocates it as `vec![0u8; 1024 * 1024]` rather than blowing a 1 MiB stack frame; the size is preserved because it caps loadable file size, which is observable), then `Source::Path(p)` ⇒ `rustty_zigimg::from_file_path(p, &mut buf)`, `Source::Mem(b)` ⇒ `from_memory(b)`, then `self.transmit_image(tty, &mut img, TransmitFormat::Png)`.
- [ ] Map `rustty_zigimg::Error` into `ImageError` (extend `ImageError` at `vaxis.rs:186-190` with a `Decode` variant if needed; keep the existing three variants byte-compatible with their current users).
- [ ] KATs in `tests/image_tier.rs`:
  - `load_image_from_mem_png_transmits`: encode a 2×2 PNG in-test, `Source::Mem`, assert the emitted escape matches the `transmit_image(..., Png)` output for the same pixels.
  - `load_image_from_path_png_transmits`: same via a `tempfile`-free `std::env::temp_dir()` file written and removed by the test; assert identical bytes to the `Mem` arm.
  - `load_image_rejects_non_image`: `Source::Mem(b"nope")` ⇒ `Err`, sink empty.
  - **Negative control** `load_image_without_kitty_graphics_writes_nothing`.

**Commit:** `rustty-vaxis: Vaxis::load_image LIVE (path + mem, 1 MiB read buffer) (WS12 T9)`

## Task 10 — `Image::draw` LIVE

Oracle: `libvaxis` 0.5.1 `src/Image.zig:87-170`; verbatim body preserved at `crates/rustty-vaxis/src/image.rs:145-230`. **No zigimg involved** — this is pure `Window` geometry and could have been filled at any point.

- [ ] Replace the `unimplemented!()` at `image.rs:231`. **Restore the dropped parameter**: `pub fn draw(&self, win: Window<'_>, opts: DrawOptions) -> Result<(), DrawError>`. (`Window` is `Copy` and holds `screen: *mut Screen<'a>` at `window.rs:44`, so reading `win.screen.width_pix` is an `unsafe` deref — keep the access narrow, no whole-`&mut Screen` formed while other aliases live, per the crate's aliasing discipline in `vxfw/app.rs:8-17`.)
- [ ] Port the four `Scale` arms exactly: `None` ⇒ nothing; `Fill` ⇒ `size = { rows: win.height, cols: win.width }`; `Fit`/`Contain` ⇒ the `pix_per_col`/`pix_per_row` `divCeil` block, `fit_x`/`fit_y`, the `Contain`-and-both-fit early break, the four `if` branches on `diff_x > diff_y`, and the final `else` with the oracle's two `std.debug.assert`s (as `debug_assert!`). Then build `Placement { img_id: self.id, options: p_opts }` and `win.write_cell(0, 0, Cell { image: Some(p), ..Default::default() })`.
- [ ] `divCeil` can fail on a zero divisor in Zig (`try`) — reproduce with a `DrawError::DivisionByZero` (or the crate's existing error type) rather than a silent `0`; the oracle's `try` propagates.
- [ ] KATs in `tests/image_tier.rs`, over a hand-built `Screen` (`width`, `height`, `width_pix`, `height_pix` are all `pub` — `screen.rs:21-25`) and a `Window`:
  - `image_draw_scale_none_leaves_size_none` — **negative control**: proves the four arms are not collapsing to one answer.
  - `image_draw_scale_fill_sets_window_size`.
  - `image_draw_scale_fit_scales_by_width` and `_by_height` — pick pixel geometries that force each branch, with the expected `size` hand-computed from the oracle body in a comment.
  - `image_draw_scale_contain_fits_leaves_size_unset` — exercises the `break :contain` early exit.
  - `image_draw_writes_placement_into_cell_0_0` — assert the `Cell` at (0,0) carries `Some(Placement { img_id, .. })` with the expected `img_id`.

**Commit:** `rustty-vaxis: Image::draw LIVE (win param restored; all four Scale arms KAT'd) (WS12 T10)`

## Task 11 — `Image::cell_size` LIVE

Oracle: `Image.zig:171-188`; verbatim body preserved at `image.rs:235-256`.

- [ ] Replace the `unimplemented!()` at `image.rs:257`. Signature: `pub fn cell_size(&self, win: Window<'_>) -> Result<CellSize, DrawError>`.
- [ ] Port exactly: `pix_per_col = divCeil(u16, x_pix, w)?`, `pix_per_row = divCeil(u16, y_pix, h)?`, then `cell_width = divCeil(u16, self.width, pix_per_col).unwrap_or(0)` and `cell_height = divCeil(u16, self.height, pix_per_row).unwrap_or(0)` — note the asymmetry: the first two `try`, the last two `catch 0`. Return `CellSize { rows: cell_height, cols: cell_width }` (**rows from height, cols from width** — the oracle's field order is easy to transpose).
- [ ] KATs:
  - `image_cell_size_div_ceil` — geometry where `divCeil` and truncating division differ (e.g. 100 px wide image at 30 px/col ⇒ `cols == 4`, not 3). **This is the negative control** against an accidental `/` port.
  - `image_cell_size_zero_pix_per_col_yields_zero` — drive `pix_per_col == 0` and assert `cols == 0` (the `catch 0` arm).
  - `image_cell_size_zero_screen_cells_errors` — `w == 0` ⇒ `Err` (the `try` arm), proving the two arms are genuinely different.
  - `image_cell_size_rows_from_height` — an asymmetric image (e.g. 100×10 px) so a transposed return is caught.
- [ ] `grep -rn 'unimplemented!' crates/rustty-vaxis/src/` — the only remaining hits must be the `#[cfg(windows)]` family (`tty.rs:419,422,425,439,442,445,539`; `loop_.rs:341`). Any other hit is a bug in this task.

**Commit:** `rustty-vaxis: Image::cell_size LIVE (divCeil + catch-0 arms KAT'd) (WS12 T11)`

## Task 12 — Ledger + prose truth pass

- [ ] `.port/phase-2/DEFERRALS.md:4082-4157` — replace the open-blocker text with a RESOLVED block: decided Option B, shipped `e0abb03`, ratified per Task 2, gated by the Task-4 non-vacuity pins. Keep the Option A/B/C analysis as historical record, clearly marked, so the reasoning survives.
- [ ] `.port/phase-2/DEFERRALS.md:4189-4205` — strike the two zigimg items; replace with: RESOLVED by WS12 via Option Z-C (`rustty-zigimg`), naming the four now-LIVE functions and the **one** documented divergence (`load_image` accepts PNG+JPEG, not zigimg's 15 formats), plus the commits.
- [ ] `.port/phase-2/DEFERRALS.md:2989-2996` — delete the "`boo` — STILL DEFERRED" block outright; it contradicts `:2747` in the same file. Leave a one-line pointer to `:2747` and to the new `tests/boo_pty_e2e.rs`.
- [ ] `crates/rustty-cli/src/lib.rs:12` — replace "The crate's ONLY live seam is `boo.rs`" with the truth: no live seams; `boo` landed in `e7e5a8c` and is verified end-to-end by `tests/boo_pty_e2e.rs`.
- [ ] `crates/rustty-vaxis/src/lib.rs:60-64` — shrink "remaining live seams" to the `#[cfg(windows)]` family only, and state that the image tier is LIVE over `rustty-zigimg`.
- [ ] `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md:81` — rewrite the WS12 row to the delivered scope, and add a "Ledger corrections already established" bullet recording that the WS12 row was stale in two of three clauses (the same pattern as WS16's and WS9's rows) so the next planner does not re-derive it.
- [ ] `bash .port/tools/gates/check_stale_comments.sh` if it applies to these paths; otherwise `grep -rn 'DEFERRALS.md' crates/rustty-vaxis/src/ crates/rustty-cli/src/` and confirm every surviving citation still points at a live entry (dangling `(DEFERRALS.md)` citations are a known past defect, see `DEFERRALS.md:4189`).

**Commit:** `docs: WS12 ledger truth pass — uucode ratified, zigimg tier resolved, boo double-entry removed (WS12 T12)`

## Task 13 — Full gate sweep + both platforms

- [ ] `df -h /` first; abort if above 90%.
- [ ] `cargo check --workspace --all-targets` (the real compile gate — **not** `cargo build --workspace`).
- [ ] `cargo test -p rustty-vaxis -p rustty-zigimg -p rustty-cli` — green; `rustty-vaxis` lib count is baseline 97 plus the new tests, with **zero** newly-ignored tests except the documented `miri`/pty ones.
- [ ] `cargo test --test boo_pty_e2e -p rustty-cli -- --ignored --nocapture` — both arms, output pasted into the log.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --all --check`.
- [ ] `bash .port/tools/gates/check_acyclic.sh` — GREEN (G7), with the new `rustty-vaxis → rustty-zigimg → {rustty-wuffs, zlib-sys}` edges.
- [ ] `/usr/bin/python3 .port/tools/gates/check_test_counts.py` — no `BLOCKED`, `vaxis` module resolved.
- [ ] `cargo +nightly miri test -p rustty-vaxis` (G6/R6) — green; every skip accounted for by a `MIRI_EXCEPTIONS.md` row.
- [ ] `bash .port/tools/gates/check_release_lane.sh` (G-REL).
- [ ] VM: push the branch, then `ssh rvm 'cd ~/projects/rustty && git fetch && git checkout ws12-vaxis-completion && cargo check --workspace --all-targets && cargo build --workspace'`. `rustty-zigimg` must build on Linux (wuffs + zlib are both already built there).
- [ ] Confirm the final seam census: `grep -rn 'unimplemented!\|todo!' crates/rustty-vaxis/src/ crates/rustty-zigimg/src/` yields **only** the `#[cfg(windows)]` family.

**Commit:** `gates: WS12 full sweep green on macOS + Linux (WS12 T13)`

---

## Out of scope (stated so it is not silently assumed done)

- **The `#[cfg(windows)]` family** (`tty.rs` `WindowsTty` ~450 LOC, `loop_.rs:341` `tty_run` arm, `tty.rs:539` `TestTty::next_event`). Undischargeable here with evidence: they are `#[cfg(windows)]`, this program has no Windows target and no Windows host, and the non-Windows build never names them. `DEFERRALS.md` "rustty-vaxis Chunk 3b" items 1/3 remain the correct home; Task 12 must **not** strike them.
- **A full `rustty-zigimg` port of zigimg's 15 format modules** (~29 k LOC) — Decision 2 Option Z-A, explicitly rejected with reasons above. If a future consumer needs BMP/GIF/TGA input, that is a new workstream, not a WS12 residual.
- **`+list-themes` interactive verification** — blocked on the resource tree (`share/rustty/themes`), which is WS1's deliverable, not WS12's. Recorded in the Environment findings.
- **`~/.cache/zig/p/` restoration for `z2d`/`uucode`** — not required by any gate; Task 3 restores exactly the three roots `check_test_counts.py` reads.
