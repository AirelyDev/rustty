# Parity audit — src/stb

This area covers the oracle `src/stb` shim plus the whole `src/synthetic` package that rides with it: 13 oracle units, 1785 lines read out of 1785 lines in scope (100% of the units in scope were read in full; no partial reads). Counterpart status: 11 ported, 1 partial (`src/synthetic/Kitty.zig`), 1 replaced (`src/stb/main.zig` — the `@cImport` shim becomes a hand-written `extern "C"` declaration over the byte-identical vendored header), 0 missing.

Findings: 2 total — 2 CONFIRMED, 0 DISPUTED, 0 REFUTED, 0 UNVERIFIED. By severity: 0 high, 1 medium, 1 low.

The single most important gap is **S121-F1**: `Kitty` draws the `i=` image id as a `u64` instead of the oracle's `u32`, which both changes the emitted id and consumes 4 extra PRNG bytes, desynchronizing the entire downstream seeded byte stream relative to Ghostty.

## Coverage

| Oracle unit | Rustty counterpart(s) | Status | Note |
| --- | --- | --- | --- |
| `src/stb/main.zig` | `crates/rustty-font-shaper/src/stb.rs`, `crates/rustty-font-shaper/vendor/stb_image_resize.h`, `crates/rustty-font-shaper/vendor/stb_impl.c`, `crates/rustty-font-shaper/build.rs` | replaced | The Zig `@cImport` shim is replaced by a hand-written `extern "C"` declaration of `stbir_resize_uint8` (stb.rs:12-29) over the byte-identical vendored header (`cmp` confirms identical to oracle `src/stb/stb_image_resize.h`), built via `cc` in build.rs. The only oracle use-site is `src/font/face/freetype.zig:641` (`stbir_resize_uint8`), covered by `crates/rustty-font-shaper/src/face/freetype.rs:819`. The other two re-exports (`stbi_load_from_memory`, `stbi_image_free`) have NO use-site anywhere in the oracle `src/` tree (grep confirms), so they are not-applicable; `rustty-vt/src/lib.rs:317-318` records them as commented-out. |
| `src/synthetic/main.zig` | `crates/rustty-synthetic/src/lib.rs` | ported | `pub use` of Bytes/Generator/Kitty/Osc/Utf8 + `cli` module; `refAllDecls` test → `ref_all_decls` (lib.rs:48-56). |
| `src/synthetic/Generator.zig` | `crates/rustty-synthetic/src/generator.rs` | ported | `ptr`+`next_fn` vtable, `init`/`next`; comptime pointer asserts become type-system guarantees. |
| `src/synthetic/Bytes.zig` | `crates/rustty-synthetic/src/bytes.rs` | ported | `generateAlphabet` (const-eval macro, same ascending `0..256` scan), defaults, `generator`/`newAlphabet`/`atLeast`/`atMost`/`next`/`format`/`write` all line-for-line; test `bytes` has identical assertion. |
| `src/synthetic/Kitty.zig` | `crates/rustty-synthetic/src/kitty.rs` | partial | Structure, base64 alphabet, prefix/ST, overhead assert, payload rounding and test `kitty` match; BUT the image id draw uses `intRangeAtMost(u64)` instead of `u32` (see finding S121-F1). |
| `src/synthetic/Osc.zig` | `crates/rustty-synthetic/src/osc.rs`, `crates/rustty-synthetic/src/enums.rs`, `crates/rustty-synthetic/src/rand.rs` | ported | All 13 `ValidKind` arms, 2 `InvalidKind` arms, alphabets, `bytes()`, `chooseValidity`, `test_seed` and the three tests (OSC generator / valid / invalid, 50 iterations each, same parser assertions via `rustty_vt::osc::Parser`) match; RNG draw order per arm verified identical. |
| `src/synthetic/Utf8.zig` | `crates/rustty-synthetic/src/utf8.rs` | ported | `Utf8Len`, `invalid_sequences` table (9 entries, same bytes), `next`/`writeInvalid`/`writeValid`/`utf8Len`/`invalidSequence`, `std.unicode` helpers ported locally; three tests with identical assertions. |
| `src/synthetic/cli.zig` | `crates/rustty-synthetic/src/cli.rs`, `crates/rustty-synthetic/src/main_gen.rs` | ported | `Action` enum (5 actions), `NoAction`/`InvalidAction`/`MultipleActions` errors, `detectArgs` replica, `Args {cli,string}`, per-action `mainActionImpl` monomorphisations (parse opts → time seed → 2048-byte buffered stdout → create/run/destroy → flush), ref-all-actions test. `Args::String` tokenisation differs from `ArgIteratorGeneral` (see low finding S121-F2); no in-tree caller uses `Args::String`. |
| `src/synthetic/cli/Ascii.zig` | `crates/rustty-synthetic/src/cli/ascii.rs` | ported | Options `line-min`/`line-max` (default 0), ascii alphabet (printable minus space), `create`/`destroy`/`run`/`writeChunk`, both tests with same bounds assertions and same "rest empty → break" semantics. |
| `src/synthetic/cli/Kitty.zig` | `crates/rustty-synthetic/src/cli/kitty.rs` | ported | Options `data-len`=4096, `buf` = `data-len`+128, run loop, test `Kitty`. |
| `src/synthetic/cli/Osc.zig` | `crates/rustty-synthetic/src/cli/osc.rs` | ported | `p-valid`=0.5, `style {streaming,parser}`=streaming, parser style writes `usize` LE length (`data.len-3`) then `data[2..len-1]`; test `Osc`. |
| `src/synthetic/cli/Styled.zig` | `crates/rustty-synthetic/src/cli/styled.rs` | ported | Options defaults, `create` validation (rates, weights, line-min, run-min), `State`, `writeLine`/`writeChar`/`writeSgr`/`toggleLink` with identical RNG draw order and byte output, 4 tests with identical assertions. |
| `src/synthetic/cli/Utf8.zig` | `crates/rustty-synthetic/src/cli/utf8.rs` | ported | Options (seed, 4 weights=1.0, ascii-printable-only, invalid-rate), `create` validation, seeded-prng override in `run`, `p_length.set` ×4, 1024-byte loop; test asserts 1024 bytes. |

**Partially read units:** none — all 13 oracle units in scope were read in full (1785 / 1785 lines).

## Findings

### High

None.

### Medium

#### S121-F1 — Kitty image id drawn as u64 instead of u32 — same-seed output differs from oracle

- **Status:** `CONFIRMED` (both verifier lenses failed to refute)
- **Kind:** behavioral-difference
- **Severity:** medium (corrected severity: medium)
- **Oracle:** `src/synthetic/Kitty.zig:49-53`
- **Rustty:** `crates/rustty-synthetic/src/kitty.rs:82-88`

**Description.** The oracle draws the `i=` image id with `self.rand.intRangeAtMost(u32, 1, 1_000_000)`. That path fills 4 bytes (`int(u32)` → one Xoshiro `next()`, low 32 bits) and runs Lemire's bounded-rand on a 32-bit lane: value = `(x32 * 1e6) >> 32`, with a rejection threshold of `2^32 mod 1e6`. The port calls `self.rand.int_range_at_most::<u64>(1, 1_000_000)` because `Uint` is not implemented for `u32` (rand.rs impls only u8/u16/u64/usize and the U1/U3/U21 newtypes). The u64 lane computes `(x64 * 1e6) >> 64` with a different rejection threshold, so for the same seed the emitted `i=` value (and, on the rare rejection path, the entire downstream byte stream) differs from the oracle. rand.rs' own module doc asserts R1 bit-faithfulness of the generator stream; this is the one draw in the synthetic package that breaks it. The port's own PORT NOTE at kitty.rs:82-87 acknowledges the width mismatch.

**Evidence.** Oracle `Kitty.zig:52` `self.rand.intRangeAtMost(u32, 1, 1_000_000)`; rustty `kitty.rs:88` `self.rand.int_range_at_most::<u64>(1, 1_000_000)`; `rand.rs`'s `impl_uint_prim!` list has no u32 (`impl_uint_prim!(u8,8,1); (u16,16,2); (u64,64,8); (usize,64,8)`). Zig `std.Random.uintLessThan(u32, n)` computes `(x * n) >> 32` on a 32-bit `x` from a 4-byte fill; the Rust u64 path computes `(x * n) >> 64` on a 64-bit `x` — different values for the same PRNG state. The semantic lens additionally confirmed that `int<T>` fills exactly `T::CEIL_BYTES` (rand.rs:316-325), so the u64 draw consumes 8 bytes where Zig's u32 consumes 4, desynchronizing the whole downstream base64 payload stream by 4 bytes per call. No golden test pins the value (kitty.rs tests at 128-215 check shape only).

**Suggested fix.** Add `impl_uint_prim!(u32, 32, 4);` in `rand.rs` and change `kitty.rs:88` to `self.rand.int_range_at_most::<u32>(1, 1_000_000)`; add a golden test pinning the first generated sequence for a fixed seed against the Zig output.

### Low

#### S121-F2 — `Args::String` is whitespace-split instead of `ArgIteratorGeneral` quote/backslash parsing

- **Status:** `CONFIRMED` (both verifier lenses failed to refute)
- **Kind:** behavioral-difference
- **Severity:** low (corrected severity: low)
- **Oracle:** `src/synthetic/cli.zig:80-87`
- **Rustty:** `crates/rustty-synthetic/src/cli.rs:396-413` (corrected location: `crates/rustty-synthetic/src/cli.rs:404-412`)

**Description.** For `Args.string`, the oracle tokenises the string with `std.process.Args.IteratorGeneral(.{})`, which honours double/single quotes and backslash escapes (so `--foo="a b"` is one arg). The port splits on ASCII whitespace only, so a quoted value containing whitespace would be split into several args and fail to parse (`InvalidField`/`InvalidValue`) where the oracle succeeds. No in-tree caller passes `Args::String` to the synthetic CLI (rustty-bench has its own `cli.rs`), so this is only reachable through the public `main_action` API, but it is an observable divergence of a `pub` function.

**Evidence.** Oracle `cli.zig:81-84` `std.process.Args.IteratorGeneral(.{}).init(alloc, str)`; rustty `cli.rs:406-410` `str.split(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r')).filter(..)` with a PORT NOTE admitting "the quote/escape handling is not exercised here". The presence lens searched for a relocated equivalent: `grep -rni 'iteratorgeneral' crates/` returns nothing, and rustty-args exposes only `SliceIterator`/`LineIterator`/`ArgsIterator` (`crates/rustty-args/src/args.rs:1923-1958`); the quote handling that does exist (`comma_splitter.rs`, `zig_string_literal_parse_write` at args.rs:1343+) is per-value decoding inside `parse`, not argv splitting. The semantic lens noted a faithful `ArgIteratorGeneral` state machine does exist in-tree at `crates/rustty-config/src/command.rs:392-440` — available but unused here.

**Suggested fix.** Port `ArgIteratorGeneral`'s quote/backslash state machine (or share the one rustty-bench should also have) and use it for `Args::String` in `parse_opts`.

## Performance-only differences (not findings)

- `Bytes.generateAlphabet`: Zig comptime `[]const u8` vs Rust const-evaluated static `([u8;256], usize)` sliced by an accessor fn — identical byte set/order, only the storage differs.
- `Generator` is generic over the writer type `W` instead of Zig's single runtime `*std.Io.Writer` interface; tests rebuild the `Generator` per iteration in `osc_generator` for lifetime reasons — no observable difference.
- `std.Random` is modelled as a `Copy` handle over `&RefCell<Xoshiro256>` instead of a `{ptr, fillFn}` vtable; draw sequence is bit-identical (pinned by the ported std `sequence`/`fill`/`splitmix64` tests in `rand.rs`).

## Refuted candidates

None — no finding in this area was refuted by either verifier lens, and no finding exceeded the verification cap (0 UNVERIFIED).
