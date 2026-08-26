# Live fuzz findings (crash reproducers)

Inputs in this directory are **real crashes** the cargo-fuzz targets found in
the port. They are kept OUT of `corpus/` and `artifacts/` (both gitignored, and
`corpus/` is replayed by `crates/rustty-vt/tests/fuzz_corpus_replay.rs`) so they
survive in git without turning an unrelated lane's test run red.

Each file is a raw fuzz-target input: byte 0 is the harness selector (see
`rustty_vt::fuzz_harness`), the rest is the payload.

Reproduce:

```
cd crates/rustty-vt
cargo +nightly fuzz run fuzz_osc findings/<file>
```

These inputs are ALSO replayed by plain `cargo test`, no nightly/cargo-fuzz
toolchain required:

```
cargo test -p rustty-vt --test fuzz_corpus_replay replay_fixed_fuzz_findings
```

That is what keeps each fix nailed down. Inputs stay HERE once fixed — they are
deliberately NOT copied into `.port/phase-0/corpora/cvt/*-initial/`, because
those dirs are a byte-for-byte mirror of the oracle's own seed corpus
(`ghostty test/fuzz-libghostty/corpus/*-initial`) and port-local files would
break that provenance. When a finding is fixed, move its row below from OPEN to
FIXED and record the fix.

---

## FIXED (2026-08-11): `RGB::parse` panicked on a non-char-boundary byte index (P1, DoS)

|            |                                                                                         |
| ---------- | --------------------------------------------------------------------------------------- |
| Files      | `osc4-rgb-parse-char-boundary`, `osc21-kitty-color-lossy-char-boundary`                 |
| Crash site | `crates/rustty-vt/src/color.rs` — `RGB::parse`, the `input.len()` 3/6 and `#`-form arms |
| Oracle     | `ghostty/src/terminal/color.zig:657` `pub fn parse(value: []const u8)`                  |
| Found by   | `cargo fuzz run fuzz_osc` (this crate), 2026-08-11                                      |

`RGB::parse` takes `&str` and then indexes it with **byte** ranges
(`&input[0..2]`, `&input[2..4]`, `&input[1..3]`, …) chosen from `input.len()`,
which is a _byte_ length. Any multi-byte UTF-8 scalar in the spec puts one of
those byte indices inside a character, and `str`'s `Index<Range>` panics.

The oracle's `parse` takes `[]const u8` and slices **bytes**, which can never
trap: the sliced bytes simply fail `fromHex`/`parseInt` and the function returns
`error.InvalidFormat`.

Reachable from untrusted terminal input on every color-spec OSC — `OSC 4`,
`OSC 10/11/12`, `OSC 104`, and kitty `OSC 21` — so a remote/`cat`-able byte
sequence aborts the VT thread. Confirmed with plain, _valid_ UTF-8; the strict
`str_from_bytes` guard in `osc/parsers/color.rs:526` does **not** prevent it:

```
$ printf '\x004;1;a\xc3\xa9bcd' > in    # OSC 4;1;aébcd  (valid UTF-8)
$ cargo +nightly fuzz run fuzz_osc in
thread '<unnamed>' panicked at crates/rustty-vt/src/color.rs:1159:44:
end byte index 2 is not a char boundary; it is inside 'é' (bytes 1..3 of string)
```

`osc21-kitty-color-lossy-char-boundary` is the fuzzer's own 10-byte minimized
input, `01 32 31 3b 31 3d 55 8e 76 76` = selector `1` (ST) + `21;1=U\x8evv`. It
reaches the same panic by a second route: `osc/parsers/kitty_color.rs:96` decodes
the value with `String::from_utf8_lossy`, so the invalid byte `0x8e` becomes a
**3-byte** `U+FFFD`. That both changes `input.len()` away from the oracle's byte
length (4 → 6, selecting a different `parse` arm than Zig would) and plants a
multi-byte scalar for the byte index to land inside. The comment there —
"UTF-8 cannot be a valid spec, so a lossy decode is faithful" — is wrong for
exactly this reason.

**Fix (applied 2026-08-11, codex-vet batch `w1f-vt-b01`):** `RGB::parse`,
`RGB::from_hex` and `RGB::from_intensity` now take the oracle's `&[u8]` and
slice bytes throughout, per the port's Zig-byte-slice rule — Zig `[]const u8`
maps to `&[u8]`, never to a lossy `String`. That fixed both routes at once:
`kitty_color.rs` dropped its `String::from_utf8_lossy`, and the OSC 4/10-19/104
path dropped its `str_from_bytes` pre-check (`str_from_bytes` survives only for
`parse_int_u9`). `x11_color::map`'s lookup and `from_intensity`'s float parse
are the two ASCII-only sub-parsers that still want a `str`; each views the bytes
with `std::str::from_utf8` and treats invalid UTF-8 as "no match" / "bad float",
which is what the reference's byte-level compare and `std.fmt.parseFloat` do
anyway.

Proving tests:

| Test                                                                                                                                        | Guards                                                                                                |
| ------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `tests/fuzz_corpus_replay.rs::replay_fixed_fuzz_findings_osc`                                                                               | both reproducer files, end-to-end through `fuzz_harness::fuzz_osc`                                    |
| `color.rs::parse_byte_slices_never_panics_on_a_non_char_boundary`                                                                           | every length arm of `parse` (3/6/`#`4/7/10/13/`rgb:`/`rgbi:`), valid multi-byte AND raw invalid UTF-8 |
| `color.rs::parse_palette_entry_non_ascii_spec_is_invalid_format_not_a_panic`                                                                | the `parsePaletteEntry` entry point                                                                   |
| `osc/parsers/color.rs::{osc_4,osc_10}_non_ascii_spec_stops_the_list_instead_of_panicking`, `osc_parser_non_ascii_color_spec_is_not_a_crash` | the OSC 4 / 10-19 / 104 routes, incl. the real `osc::Parser` byte loop                                |
| `osc/parsers/kitty_color.rs::osc_21_{invalid_utf8,non_ascii}_value_is_skipped_not_a_crash`                                                  | the kitty OSC 21 route (the lossy-decode route)                                                       |
| `rustty-config value_types.rs::color_parse_cli_non_ascii_is_invalid_value_not_a_panic`                                                      | the config/CLI `Color::parse_cli` delegation                                                          |
| `rustty-vt-c color.rs::parse_invalid`                                                                                                       | the C-ABI `rustty_color_rgb_parse` surface                                                            |

Non-vacuity was proved by reverting the non-test code to its pre-fix shape and
re-running the replay: it failed with
`byte index 2 is not a char boundary; it is inside '\u{fffd}' (bytes 1..4) of `U\u{fffd}vv``
at the `color.rs` `parse` arm, naming the reproducer file.
