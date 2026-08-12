# Fuzz Inventory & Differential-Fuzz Wiring (Phase 0)

**Reference:** `/Users/gael/projects/ghostty` @ `4749c4e` (READ-ONLY).
**Method:** enumerated by reading `test/fuzz-libghostty/` (the dedicated AFL++
harness package), `pkg/afl++/` (the AFL build glue), `src/synthetic/` (corpus
generator), and grepping `src/` for `fuzz`/`AFL` regression markers.

---

## 1. Reference fuzz targets

Ghostty's fuzzers live in **`test/fuzz-libghostty/`** — AFL++ harnesses for the
`ghostty-vt` (libghostty-vt) Zig module. There are **three** targets; each is
`src/fuzz_<name>.zig` exporting `zig_fuzz_init()` + `zig_fuzz_test(buf, len)`,
compiled to an instrumented binary at `zig-out/bin/fuzz-<name>` via `afl-cc`
(driven by `pkg/afl++/afl.c` + `build.zig`, `lib.root_module.fuzz = true`).

| Target | Binary | Entry under test | Source |
|---|---|---|---|
| **osc** | `fuzz-osc` | `osc.Parser.next()` byte-loop + `.end(terminator)`, with a **fixed-buffer allocator** (8 MiB) so allocating writer paths for large payloads are exercised. First input byte selects terminator: `0→BEL(0x07)`, `1→ST(0x9c)`, `2→missing`. | `src/fuzz_osc.zig` |
| **parser** | `fuzz-parser` | `Parser.next(byte)` — the raw VT state machine, byte-at-a-time, no terminal. | `src/fuzz_parser.zig` |
| **stream** | `fuzz-stream` | Full terminal **Stream** handler over a real `Terminal` (80×24, scrollback 100, 64 MiB fixed-buffer alloc): printing, CSI dispatch, OSC, DCS, SGR, cursor moves, scrolling regions, UTF-8. First input byte selects path: even→`nextSlice` (SIMD fast-path), odd→`next` scalar (byte-at-a-time UTF-8). | `src/fuzz_stream.zig` |

Shared helper: `src/mem.zig` — `FuzzAllocator(N)`, a `FixedBufferAllocator`
reset per iteration for deterministic, bounded memory.

**Build/run** (from `test/fuzz-libghostty/`): `zig build` → the three
`zig-out/bin/fuzz-*`; `zig build run-{osc,parser,stream}` wraps `afl-fuzz`, or
directly `afl-fuzz -i corpus/<t>-initial -o afl-out/<t> -- zig-out/bin/fuzz-<t>`.
Harness reads input from **stdin** (NOT `@@`) — so `afl-cmin/tmin/showmap` must
be run without `@@`.

### Notes on coverage breadth

- There is **no separate `page`/PageList fuzzer binary** in this revision.
  PageList is fuzzed *transitively* through `fuzz-stream` (every print/scroll/
  resize routes through PageList) and guarded by AFL-found **regression tests**
  embedded in the unit suite (see §3). `sgr.zig` likewise has no standalone
  fuzzer; SGR is covered by `fuzz-stream` + `fuzz-parser` and SGR regression
  tests. If the port wants finer isolation, adding `fuzz-page`/`fuzz-sgr`
  targets is a port-side option (does not change parity).

### Corpora shipped with the reference (the shared differential-fuzz seeds)

| Dir | Files | Role |
|---|---|---|
| `corpus/osc-initial/` | 20 | hand-written OSC seeds (OSC 52/66/133/3008/1337/5522, truncated, invalid) |
| `corpus/osc-cmin/` | 20 | `afl-cmin` edge-deduped OSC corpus |
| `corpus/parser-initial/` | 50 | hand-written VT-parser seeds (CSI/intermediate/DA/incomplete-esc, …) |
| `corpus/parser-cmin/` | 616 | `afl-cmin` edge-deduped parser corpus |
| `corpus/stream-initial/` | 24 | hand-written stream seeds (plain/CSI/OSC/DCS/APC/SGR/UTF-8/scroll/…) |
| `corpus/stream-cmin/` | 3271 | `afl-cmin` edge-deduped stream corpus |

The **hand-written `*-initial` seeds** have been copied (read-only source →
outputs) into the harness corpus area for the shared differential corpus:
`.port/phase-0/corpora/cvt/{stream,osc,parser}-initial/`
(24 + 20 + 50 files). The large `*-cmin` sets stay in the reference tree and are
pulled on demand for deep runs.

### Synthetic corpus generator

`src/synthetic/` builds **`ghostty-gen`** (`src/main_gen.zig`), a CLI emitting
structured random VT data for seeding fuzzers (NOT a fuzzer itself). Actions
(`src/synthetic/cli.zig` dispatcher): `ascii`, `osc`, `utf8` (+ `Bytes`, `Utf8`,
`Osc`, `Generator` building blocks). The port's `rustty-synthetic` crate
reproduces it; it feeds the *shared corpus* below.

---

## 2. Differential-fuzz wiring (reference ⊕ port)

Goal (spec §8.2 "Differential fuzzing stood up early"): run the SAME fuzz inputs
through reference and port and flag any **canonical-output divergence** — i.e.
fuzzing the *comparator* (DESIGN.md), not just each side for crashes.

### Architecture

```
                 ┌──────────────────── shared corpus ────────────────────┐
                 │  corpora/cvt/{stream,osc,parser}-initial  (seeds)      │
   ghostty-gen / │  + AFL-evolved queue (afl-out/<t>/.../queue) when run  │
 rustty-synthetic│  + ghostty stream-cmin / parser-cmin / osc-cmin        │
                 └───────────────────────────┬───────────────────────────┘
                                              │ each input file
                        ┌─────────────────────┴─────────────────────┐
                        ▼                                            ▼
              [ref  vt_driver_ref]                         [port vt_driver_port]
              libghostty-vt formatter                      librustty-vt (identity-oracle)
                        │  canonical_ref                              │  canonical_port
                        └───────────────────► comparator.py ◄────────┘
                                       diff → byte_divergence%
                                  divergence > 0  ⇒  MINIMIZED & filed
```

### Components

1. **Shared corpus (single source of truth).** The reference `*-initial` seeds
   (copied) + on-demand `*-cmin` + AFL-evolved `queue/` + `ghostty-gen`/
   `rustty-synthetic` output. Both sides consume the *same bytes* for every run.
2. **Two oracles via one CLI contract.** `vt_driver_ref` (reference,
   libghostty-vt) and `vt_driver_port` (port, librustty-vt built with
   `--features identity-oracle`, R2). Identical flags/stdin (DESIGN.md §2).
3. **Differential check = `comparator.py`.** For each corpus input, diff the two
   canonical serializations; **any** `byte_divergence > 0` is a divergence to
   triage. (Per-side crash/hang detection still comes from native AFL on each
   instrumented binary — the two modes are complementary.)
4. **Two fuzzing modes:**
   - *Per-side crash fuzzing* — native AFL++ on `fuzz-{osc,parser,stream}`
     (reference) and the mirror port harnesses; finds panics/UB/hangs.
   - *Differential fuzzing* — replay every evolved/seed input through BOTH
     oracles via the comparator; finds **behavioral** divergence (parity bugs)
     that don't crash either side. This is the parity-critical mode.
5. **Triage loop.** A divergent input is shrunk with `afl-tmin` (no `@@`; stdin),
   recorded with both canonical outputs + the unified diff, and (per R1/R5) filed
   as a blocker until parity is restored. Divergent inputs become permanent
   differential regression fixtures.
6. **Identity matching.** No canonicalization in the comparator. The port harness
   is built with `identity-oracle` so identity tokens (XTVERSION/DA/version/
   `TERM`) match byte-for-byte; a divergence in those bytes means the harness was
   mis-built — a real finding, not noise (R2).

### Port-side harness (mirrors the reference, Phase 1+)

Mirror `src/fuzz_{osc,parser,stream}.zig` as Rust harnesses over `rustty-vt`
(`cargo-fuzz`/libFuzzer or AFL via `afl.rs`), reusing the **same first-byte
selector semantics** (osc terminator; stream slice/scalar) so a given corpus byte
exercises the same path on both sides — a prerequisite for meaningful diffing.
The `vt_driver_port` (DESIGN.md §3) provides the differential leg without needing
the instrumented binaries.

**DELIVERED (2026-08-10, codex-vet W13C-01).** The port-side harness now exists:

- Harness bodies: `crates/rustty-vt/src/fuzz_harness.rs` — faithful ports of
  the three `zig_fuzz_test` bodies (osc terminator-selector `%3`, parser
  byte-loop, stream 80x24/scrollback-100 with even→`next_slice` /
  odd→scalar-`next` selector).
- cargo-fuzz targets: `crates/rustty-vt/fuzz/fuzz_targets/{fuzz_osc,fuzz_parser,
  fuzz_stream,vt_diff}.rs` (nightly; `vt_diff` = the G5-named smoke target,
  stream harness). `run_fuzz_smoke.sh` auto-flips from FUZZ-WIRED progress mode
  to the REAL smoke and is GREEN.
- Deterministic corpus replay, AFL-free, in plain `cargo test`:
  `crates/rustty-vt/tests/fuzz_corpus_replay.rs` replays the in-tree
  `*-initial` seeds always, and ALL reference corpora (4,001 files:
  osc 20+20, parser 50+616, stream 24+3271 at oracle pin 6ad1fe7d) when
  `GHOSTTY_ORACLE=<ghostty checkout>` is set — replayed clean.
- The ref-vs-port *differential* leg intentionally stays in this phase-0
  comparator (the Zig reference cannot link into a libFuzzer binary).

---

## 3. AFL-found regression tests (the existing safety net)

This revision embeds AFL++/fuzz-discovered crashes as **permanent unit tests** in
the VT core — these are part of the R1 test-block contract and MUST port
assertion-for-assertion:

| File | Marker |
|---|---|
| `src/terminal/Parser.zig:1077` | crash found by fuzzing (afl) — DCS handling |
| `src/terminal/stream.zig:3426,3446` | fuzz-crash CSI param value regression (exact input) |
| `src/terminal/PageList.zig:10648` | fuzz-crash: shrink cols then … |
| `src/terminal/Terminal.zig:3489,5394,10149,12705,13200` | AFL++ crashes (`afl-out/stream/default/crashes/id:…`), unwrapped-spacer-head panic, … |
| `src/terminal/sgr.zig:1078` | fuzz crash `afl-out/stream/default/crashes/id:000021` |

---

## 4. Status & blocker

- **Port-side harness: delivered** (see §2 "Port-side harness" DELIVERED note):
  `rustty_vt::fuzz_harness` + `crates/rustty-vt/fuzz/` (cargo-fuzz, G5 real
  smoke GREEN) + `tests/fuzz_corpus_replay.rs` (full 4,001-file reference
  corpus replays clean under plain `cargo test` with `GHOSTTY_ORACLE` set).
- **Wiring + shared corpus: delivered.** Seeds copied; comparator ingests corpus
  files directly (demonstrated over all 24 `stream-initial` seeds — control 0%,
  empty-port divergence 100% on every visible-content seed).
- **Live fuzzing verified, all four targets (2026-08-11, codex-vet F-017).**
  `run_fuzz_smoke.sh` takes the REAL branch and is GREEN (`vt_diff`, 3,299 runs,
  no artifact); `fuzz_parser` and `fuzz_stream` also run clean. **`fuzz_osc`
  found a REAL P1 crash** in `rustty_vt::color::RGB::parse` — it takes `&str`
  and then byte-indexes it (`&input[0..2]`, …) off `input.len()`, so any
  multi-byte UTF-8 scalar in a color spec lands a byte index mid-character and
  `str` panics, where the oracle (`color.zig:657`, `parse(value: []const u8)`)
  slices bytes and returns `error.InvalidFormat`. Reachable from untrusted
  `OSC 4 / 10 / 11 / 104 / 21`. Minimized reproducers and the full analysis are
  checked in at `crates/rustty-vt/fuzz/findings/` (that dir is deliberately
  outside the gitignored `corpus/`+`artifacts/`, so the inputs survive without
  living in an evolving corpus). This is the harness doing its job on its first
  live run.
- **That P1 is FIXED (2026-08-11, codex-vet batch w1f-vt-b01).** `RGB::parse`,
  `RGB::from_hex` and `RGB::from_intensity` now take `&[u8]` and slice bytes,
  matching `color.zig:657`'s `parse(value: []const u8)`; the OSC 21 value's
  `String::from_utf8_lossy` (which widened invalid bytes to 3-byte U+FFFD and so
  also picked the WRONG length arm) is gone, and the `str_from_bytes` pre-check
  on the OSC 4/10-19/104 path with it. Both reproducers are now replayed by
  `tests/fuzz_corpus_replay.rs::replay_fixed_fuzz_findings_osc`, which is a
  proven-failing test against the pre-fix code, so the crashes stay fixed under
  plain `cargo test`.
- **Blocker (tooling, non-parity): AFL++ not installed** — `afl-cc`/`afl-fuzz`
  are absent in this environment, so live instrumented fuzzing of the reference
  cannot be *run* here (install: `brew install aflplusplus`). The *differential*
  mode (comparator over corpus/synthetic inputs) needs no AFL and is fully
  operational now; AFL only accelerates corpus evolution. This is the same class
  of gap as the missing `cmake` noted in `build-summary.md`.
