# Parity audit — src/terminfo

Audited all 3 oracle units in `src/terminfo` at pin `20abdb50a` (`Source.zig`, `ghostty.zig`, `main.zig`), reading 673 of 673 lines in scope — full coverage, no partial units.
All 3 units are **ported**; the 270-entry capability table was mechanically diffed one-for-one (order, kind, exact value bytes) with zero differences, and both `Source.zig` tests plus the `ghostty.zig` `encode` test are present with identical assertion counts and expected bytes.
Findings: **0 confirmed, 0 disputed, 0 refuted, 0 unverified** — no high, medium, or low severity issues.
Most important gap: none functional. The only residue is four documented performance/representation-only differences (runtime `HashMap` for the xtgettcap map, `OnceLock` Wyhash `version()`, dropped `[:0]` NUL sentinels, and a `@compileError` → runtime `panic!` for the unreachable `Canceled` arm) — of these, the `Canceled` arm is the one worth re-checking if a future capability of that kind is ever added to the table.

## Coverage

| Oracle path:lines                | Rustty counterpart(s)                                                      | Status | Note                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| -------------------------------- | -------------------------------------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/terminfo/Source.zig:1-249`  | `crates/rustty-terminfo/src/source.rs`                                     | ported | Struct `Source` (names/capabilities), `Capability`, `Value` union (canceled/boolean/numeric u32/string) → source.rs:16-50. `encode` (source.rs:56-83) writes names joined by `\|`, `,\n`, then per-cap `\t` + name + `@` / `` / `#{d}` / `={s}` + `,\n` — identical. `xtgettcapMap` → `xtgettcap_map` (source.rs:104-197): same len+3, same TN/Co=256/RGB=8 seeds at indices 0..2, same `%`-parameter shortcut, same `\\E`→`0x1B` replace, same `^` control-char loop with `?`→`0x7F` and else `c-64`, same `idx>0` panic, same numeric decimal render, same `hexencode` (upper) of key and value, same `\x1bP1+r<key>[=<hexval>]\x1b\\` framing including the empty-value branch. `hexencode`/`comptimeReplace` → `hexencode`/`bytes_to_hex`/`comptime_replace` (source.rs:200-235). Both tests ported with identical assertion counts and expected byte strings: `"xtgettcap map"` → `xtgettcap_map_test` (5 assertions, source.rs:242-299); `"encode"` → `encode` (1 assertion, same expected text, source.rs:301-340). |
| `src/terminfo/ghostty.zig:1-410` | `crates/rustty-terminfo/src/entry.rs`, `crates/rustty-identity/src/lib.rs` | ported | `pub const ghostty: Source` → `pub fn rustty() -> Source` (entry.rs:20). Names come from `rustty-identity` per R2 (`TERMINFO_PRIMARY_NAME`/`TERMINFO_PREFERRED_NAME`/`APP_NAME_TITLECASE` = `xterm-rustty`/`rustty`/`Rustty` by default, `xterm-ghostty`/`ghostty`/`Ghostty` under the `identity-oracle` feature), preserving the reference's order and the tcell HACK comment. Capability table mechanically diffed: all 270 entries match one-for-one in ORDER, kind (boolean/numeric/string) and exact value bytes (script compare of `ghostty.zig:37-391` vs the entry.rs table; zero diffs; 270 == 270 on both sides). `pub const version` → `pub fn version()` (entry.rs) = lowercase-hex Wyhash (seed 0) over the encoded source bytes, with a faithful `std.hash.Wyhash` port validated by Zig's own test vectors. Test `"encode"` → `entry.rs tests::encode` with the same single `len > 0` assertion.                                                                                                            |
| `src/terminfo/main.zig:1-14`     | `crates/rustty-terminfo/src/lib.rs`                                        | ported | Module doc comment ported verbatim (lib.rs:1-6). `pub const ghostty` → `pub use entry::rustty` (lib.rs:25), `pub const version` → `pub use entry::version` (lib.rs:27), `pub const Source` → `pub use source::Source` (lib.rs:28). `test { refAllDecls(@This()) }` → lib.rs:36-48 `ref_all_decls`, which names each public decl; Rust type-checks all pub items at build time, so the guarantee is equal or stronger.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |

**Partially-read units:** none. Every unit in scope was read end to end (673/673 lines).

## Findings

No findings were raised for this area — no confirmed, disputed, or unverified items at any severity.

### High

None.

### Medium

None.

### Low

None.

## Performance-only differences (not findings)

- `source.rs` `xtgettcap_map` builds a runtime `HashMap<Vec<u8>, Vec<u8>>` instead of the reference's comptime `std.StaticStringMap([:0]const u8)`; lookups and byte values are identical (verified against the 5 expected responses in the ported test).
- `entry.rs` `version()` memoizes the content hash in a `OnceLock<String>` computed at first use instead of at Zig comptime; the hash algorithm is a faithful one-shot port of `std.hash.Wyhash` (guarded by the official Zig test vectors in `wyhash_test_vectors`), and the formatting (`{:x}`) matches `comptimePrint("{x}")`.
- The reference's `[:0]` NUL sentinels (numeric buf 10→11, response buf +1, `comptimeReplace` +1) are dropped because rustty's `write_pty` takes `&[u8]`; the response BYTES are unchanged (documented PORT NOTE at `crates/rustty-terminfo/src/source.rs:97-103`).
- Zig's `.canceled => @compileError("canceled not handled yet")` inside `xtgettcapMap` becomes a runtime `panic!("canceled not handled yet")` at `crates/rustty-terminfo/src/source.rs:119`; the message and unreachability are identical and no capability in the table is `Canceled`, so there is no observable difference (the check moves from compile time to run time).

## Refuted candidates

None — no candidate findings were raised in this area, so none were refuted by the verifier lenses.
