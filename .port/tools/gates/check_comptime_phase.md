# check_comptime_phase — D4 comptime-phase preservation checklist

**Detector:** D4 (`.port/phase-2/BUN_PORT_LEARNINGS.md` §4) — *comptime-phase
preservation + golden VT bytes*. Kills Bun regression **Class 3**
(oven-sh/bun#30693: a `comptime`-string builder ported to a runtime function let
an OSC 8 ST (`ESC \`) collide with the tag grammar and corrupt output,
`ai`→`air`).

**Likelihood for rustty: VERY HIGH.** Ghostty comptime-builds CSI/SGR/DCS/OSC
responses and the data domain *is* escape sequences. This is the single most
dangerous mismatch class for this codebase.

This file is a **completeness-critic checklist item** (part (a) of D4). It is a
review gate, not an executable pass/fail script — the executable half lives in
the golden byte tests (`crates/rustty-vt/tests/d4_golden_vt_bytes.rs`, run by the
`cargo test --workspace` lane) and the thin runner
`.port/tools/gates/check_comptime_phase.sh`.

---

## Part (a) — the per-chunk `comptime`-parameter map (MANDATORY, per sub-batch)

For every chunk (sub-batch) whose Zig files are being ported, before the chunk's
gate goes green, produce and review this map:

1. **Enumerate every `comptime` parameter** in the chunk's Zig source. Grep seed:

   ```sh
   rg -n 'comptime [A-Za-z_][A-Za-z0-9_]*\s*:' <chunk zig files>
   rg -n 'comptimePrint|inline fn|inline for|@compileError|std\.fmt\.comptimePrint' <chunk zig files>
   ```

2. **Map each one to a Rust construct on the comptime ladder** (§3.1 comptime
   ladder — prefer higher rungs):

   | Zig `comptime` form                    | Faithful Rust rung (preferred → fallback)                     |
   | -------------------------------------- | ------------------------------------------------------------- |
   | `comptime T: type`                     | plain generic `<T>` (NOT a const generic)                     |
   | `comptime b: bool`                     | `<const B: bool>`                                             |
   | `comptime n: usize` / integer          | `const` item / associated `const` / `<const N: usize>`        |
   | `comptime fmt: []const u8` + `args`    | `macro_rules!`/`concat!`/proc-macro — the template is a       |
   |                                        | **compile-time literal**, never a runtime-parsed string       |
   | `comptimePrint(...)`                   | `concat!(...)` / a `const` string                             |
   | `inline for` over a comptime set       | macro expansion or a `const` table generator                  |

3. **A runtime-fn port of a `comptime` string builder is a FLAGGED DEVIATION.**
   It is allowed ONLY with a written **grammar-collision argument** recorded in
   the chunk's port notes proving:

   > *No runtime byte that can flow into this function can alias the template
   > grammar* — specifically, no data value can contain a byte sequence the
   > builder would re-interpret as a delimiter/terminator (e.g. an OSC/DCS/APC
   > opener `ESC ] / ESC P / ESC _`, an ST `ESC \` or `0x9c`, a BEL `0x07`, a
   > CSI `ESC [`, or a parameter/intermediate separator). If any such alias is
   > possible, the port MUST restore phase separation (macro/const template),
   > exactly as Bun's fix did.

   Missing argument ⇒ the chunk's completeness-critic review is **RED**.

## Part (b) — golden VT bytes + ST-integrity (the executable half)

Every crate that emits escape/format output MUST have byte-exact golden tests +
an ST-integrity sweep. For `rustty-vt` this is
`crates/rustty-vt/tests/d4_golden_vt_bytes.rs`. When a chunk adds or changes ANY
reply emitter, tick every box:

- [ ] A **byte-exact golden** `assert_eq!` exists for the new/changed emitter,
      covering the default output AND at least one non-default field permutation.
- [ ] The emitter's output is run through `assert_st_integrity(..)` (directly or
      via the `full_corpus_st_sweep`): every OSC/DCS/APC/SOS/PM opener is paired
      with a correct terminator (ST = `ESC \` / `0x9c`, or BEL `0x07`), no stray
      `ESC` sits inside a string body, and there is no orphan terminator.
- [ ] If the emitter frames a **string sequence** (OSC/DCS/APC), a golden covers
      BOTH terminator flavours where the emitter can produce them (ST and BEL).
- [ ] The template that produces the framing bytes is a **compile-time literal**
      (`write!`/`concat!`/`macro`), never assembled by parsing rendered bytes.
- [ ] The Zig test-for-test parity assertions for the emitter are preserved (R1);
      the golden strings match the reference's expected bytes exactly (identity
      differences are handled by the `identity-oracle` profile, R2 — never by
      canonicalizing the golden).

### Emitter inventory currently covered by the golden corpus

`device_attributes` DA1/DA2/DA3 (DA3 is a DCS), `size_report` (CSI…t family),
`kitty::graphics_command::Response` (APC `ESC _ G`), `apc::glyph::response`
(APC `ESC _ 25a1`), and the `osc::Terminator` surface + canonical OSC-8 / OSC-52 /
OSC-4 reply shapes (the literal #30693 site).

**When a new VT-emitting crate appears** (renderer/apprt title reports, termio,
etc.), add a sibling `tests/d4_golden_vt_bytes.rs` (or extend an existing one)
and list it in `check_comptime_phase.sh`'s crate set.

---

## How to run

```sh
# executable half (byte-exact goldens + ST-integrity, native speed):
cargo test -p rustty-vt --test d4_golden_vt_bytes

# thin CI runner (asserts this checklist + the test file exist, then runs the
# golden tests; exit 0 pass / 3 blocked / non-zero fail):
bash .port/tools/gates/check_comptime_phase.sh
```
