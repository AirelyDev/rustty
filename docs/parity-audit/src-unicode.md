# Parity audit — src/unicode

8 oracle units audited (1 shard), 869 of 869 lines in scope read in full — 100% coverage, no partial units. All 8 units are **ported**; none missing, replaced, or not-applicable. Findings: **1 CONFIRMED**, 0 DISPUTED, 1 REFUTED, 0 UNVERIFIED. By corrected severity: 0 high, 0 medium, 1 low. The single most important gap is cosmetic: `lut.rs::write_zig` emits one extra blank line before the `stage3` declaration, so the regenerated Zig table source no longer byte-matches the reference generator's output — which is exactly what that function exists to allow. No behavioural divergence was found in the runtime lookup, grapheme-break, or width paths; the embedded tables are gated bit-for-bit against uucode by the `.port/phase-0/harness/unicode` differential.

## Coverage

| Oracle unit (path:lines) | Rustty counterpart(s) | Status | Note |
| --- | --- | --- | --- |
| `src/unicode/grapheme.zig` [1-371] | `crates/rustty-unicode/src/grapheme.rs`, `crates/rustty-unicode/src/generated/grapheme_data.rs` | ported | `GraphemeWidthEffect` (4 variants), `GraphemeWidth{len,width}`, `grapheme_break`, `grapheme_width_effect` (VS16/VS15 + `emoji_vs_base` guard + `width_zero_in_grapheme`), `grapheme_width` (empty / invalid-cp / loop / effect switch), `invalid_codepoint(>0x10FFFF)`, `Precompute` `Key.index` (`state \| gb1<<3 \| gb2<<8`) and `Value` decode (bit0 result, bits 1..3 state) all match. All 7 reference test blocks ported with equal assertion counts. The comptime `Precompute.data` (built from `uucode.computeGraphemeBreakNoControl`) is replaced by the bit-for-bit embedded `generated/grapheme_data.rs` `DATA[8192]`, validated by `.port/phase-0/harness/unicode`. `pub const std_options` (log_level) is Zig-log plumbing, not applicable. |
| `src/unicode/lut.zig` [1-188] | `crates/rustty-unicode/src/lut.rs` | ported | `Generator`/`Context`/`BlockMap`/`BLOCK_SIZE=256` / `0..=maxInt(u21)` sweep / `BlockTooLarge` / `Stage2TooLarge` / 3 `u16` length asserts / `Tables` / `TablesRef::get` all present. Only divergence is the `write_zig` stage3 header newline (finding S215-F1). Rust adds two unit tests (lut invariant + full u21 sweep vs reference indexing) beyond the reference. |
| `src/unicode/main.zig` [1-63] | `crates/rustty-unicode/src/lib.rs` | ported | `codepoint_width` + all 19 assertions of test `"codepointWidth"` ported verbatim; `refAllDecls` reproduced as `ref_all_decls`. `lib.rs` re-exports `graphemeBreak`/`Properties`/`table` at the crate root; `GraphemeWidth`/`GraphemeWidthEffect`/`grapheme_width`/`grapheme_width_effect`/`lut` are reachable via `pub mod grapheme` / `pub mod lut` (module-path-only difference, no behavioural effect). The candidate list's `.port/phase-2/mirihar-*/src/main.rs` entries are unrelated Miri harnesses, not counterparts. |
| `src/unicode/props.zig` [1-63] | `crates/rustty-unicode/src/props.rs` | ported | `Properties` fields + defaults (`width=0`, `width_zero_in_grapheme=false`, `grapheme_break=Other`, `emoji_vs_base=false`), `eql`, and `format()` emitting the identical Zig literal text. `_padding: u7` is a Zig bit-packing perf workaround with no Rust analogue (documented). `GraphemeBreakNoControl` discriminants 0..16 mirror uucode's `enum(u5)` and are machine-checked by the embedded-table checksum + harness. |
| `src/unicode/props_table.zig` [1-18] | `crates/rustty-unicode/src/props_table.rs` | ported | `table()` returns `TablesRef` over `generated::props_data` STAGE1/2/3, replacing the build-system-injected `@import("unicode_tables")`. |
| `src/unicode/props_uucode.zig` [1-83] | `crates/rustty-unicode/src/props_uucode.rs`, `crates/rustty-unicode/examples/dump_tables.rs` | ported | `get()` with the identical `>max_code_point` rule (width=1, wzg=true, Other, false); `MAX_CODE_POINT=0x10FFFF`; `main()` runs the same `Generator` + `write_zig`. Test `"unicode props: tables match uucode"` is replaced by a curated spot-check (30 codepoints x 4 fields + 4 out-of-range cases) **plus** the full-range differential gate at `.port/phase-0/harness/unicode` (`uucode_oracle.zig` + `diff_unicode.py` + `examples/dump_tables.rs`) — substantiated, files present. |
| `src/unicode/symbols_table.zig` [1-17] | `crates/rustty-unicode/src/symbols_table.rs` | ported | `table()` returns `TablesRef<bool>` over `generated::symbols_data` STAGE1/2/3. |
| `src/unicode/symbols_uucode.zig` [1-66] | `crates/rustty-unicode/src/symbols_uucode.rs`, `crates/rustty-unicode/examples/dump_tables.rs` | ported | `main()`'s `Ctx.get` reproduces `cp > max_code_point ? false : is_symbol(cp)`; `eql` is `a == b`. Test `"unicode symbols: tables match uucode"` replaced by a curated 24-codepoint spot-check + the same full-range harness differential. |

**Partially-read units:** none. Every unit in scope was read end to end (869 / 869 lines).

## Findings

### High severity

None.

### Medium severity

None.

### Low severity

#### S215-F1 — write_zig emits an extra blank line before the stage3 declaration

- **Status:** ✅ CONFIRMED (both verifier lenses failed to refute)
- **Kind:** behavioral-difference
- **Severity:** low (corrected severity: low)
- **Oracle:** `src/unicode/lut.zig:167-172`
- **Rustty:** `crates/rustty-unicode/src/lut.rs:196-199`

**Description**

The reference `writeZig` writes `};` (end of stage2) immediately followed by a multiline literal whose lines are `` (empty) and `pub const stage3: [{}]Elem = .{`, i.e. exactly one newline between them. The port writes two newlines, so the generated Zig source differs from the reference's byte-for-byte. `write_zig`'s stated purpose (`props_uucode.rs:61`, `symbols_uucode.rs:35`) is to re-derive/verify the embedded artifact against the reference generator, so the output should match exactly.

**Evidence**

Oracle `lut.zig:167` `try writer.writeAll("};");` then `:169-172` prints a multiline literal that joins to `"};\npub const stage3: [N]Elem = .{"`. Rustty `lut.rs:194` `out.push_str("};");` then `:196-199` `out.push_str(&format!("\n\npub const stage3: [{}]Elem = .{{", self.stage3.len()));` → `"};\n\npub const stage3: [N]Elem = .{"`. The asymmetry is deliberate in the oracle: the stage2 header at `lut.zig:160-165` *does* contain the blank line, and rustty `lut.rs:190` reproduces that one exactly — so this is not a formatting convention applied uniformly by the port. A workspace grep for other `write_zig` implementations or normalizers (`lut.rs:167/171/180`, `props.rs:112`) found nothing that post-processes or collapses the extra newline. `write_zig`'s output is the program's observable output (printed to stdout and piped to the build system), not an internal formatting detail.

**Suggested fix**

Change `lut.rs:197` to `"\npub const stage3: [{}]Elem = .{{"` (single newline).

## Performance-only differences (not findings)

- `lut.rs` `TablesRef::get` masks `cp & 0x1F_FFFF` and uses `get_unchecked` for stage2/stage3, with `debug_assert!` guards and a unit test (`generated_tables_uphold_lut_invariant`) machine-checking the invariant. The reference's `cp: u21` parameter type makes the mask a no-op for every expressible value; identical results.
- `lut.rs` uses const-generic fixed-size array references (S1/S2/S3) instead of `[]const` slices purely so LLVM can fold the bounds checks; same lookups.
- `#[inline]` on `grapheme_break`, `grapheme_width_effect`, `props_table::table`, `symbols_table::table`, `props_uucode::get` restores Zig's whole-program ReleaseFast inlining without LTO.
- `Precompute::DATA` is decoded from an embedded byte table at const-eval time instead of being computed comptime from uucode; the decoded values are identical (bit0 result, bits 1..3 state) and verified by the full-range harness diff.
- `props.rs` `Properties` is a plain struct rather than a `packed struct(u16)`; the `_padding: u7` field (and `Precompute` `Value`'s `_padding: u4`) exist only to give Zig a power-of-two backing integer and have no Rust analogue. Field values and the serialized table bytes are unchanged.
- `lut.rs` `BlockMap` uses Rust's default `HashMap` hasher over `[u16;256]` instead of the reference's explicit Wyhash + `std.mem.eql` context; structurally identical keying, and stage2 assignment order is driven by first-insertion order in both.
- `generated/checksum.rs` adds an FNV-1a integrity guard over the embedded tables (extra coverage, no reference counterpart).

## Refuted candidates

Checked and cleared — both verifier lenses refuted these:

- **S215-F2 — Debug `main` sweep drops the grapheme_break==.control skip (and the mismatch comparison/log)** — the uucode comparison is legitimately relocated to `.port/phase-0/harness/unicode` (`uucode_oracle.zig:132` emits `grapheme_break_no_control` per codepoint, `diff_unicode.py` diffs it); with the comparison gone, the `.control` filter only changes which pairs are fed to a `grapheme_break` whose result is discarded inside a never-called `#[allow(dead_code)] pub fn main()` that the oracle itself documents as unbuildable/debug-only, so the omission has no observable effect (progress lines are byte-identical; the dead `_uu_state` local is cosmetic).
