# Parity closure checklist — src/input

9 open finding(s): 1 high, 2 medium, 6 low. Full evidence per row is in [`docs/parity-audit/src-input.md`](../../docs/parity-audit/src-input.md).

A row is CLOSED only when its fix has a test that fails without it (or a written, reviewed justification for why no test is possible).


## High (1)

- [ ] **S103-F1** · CONFIRMED · `ABI` · behavioral-difference
  - **What:** Command.C is not a C-ABI struct but is handed to C as rustty_command_s (command-palette-entry ABI mismatch)
  - **Oracle:** `src/input/command.zig:23-29, 64-77, 121-128` → **Rustty:** `crates/rustty-input/src/command.rs:47-58, 130-149`
  - **Audit's suggested fix:** Make `C` a `#[repr(C)]` struct of four `*const c_char` (NUL-terminated) that borrow the owning Command/arena-allocated action string (as the oracle does: shares memory with the Command, action string allocated separately), keep the owned NUL buffers alongside in RepeatableCommand, and add an A4-style size assertion `size_of::<C>() == 4 * size_of::<*const c_char>()` against rustty-embed.h.

## Medium (2)

- [ ] **S101-F1** · CONFIRMED · `INDIV` · missing-branch
  - **What:** Trigger.foldedCodepoint Unicode slow path (case_folding_full) not ported — non-ASCII triggers are not case-folded
  - **Oracle:** `src/input/Binding.zig:1995-2012` → **Rustty:** `crates/rustty-input/src/binding.rs:3400-3412`
  - **Audit's suggested fix:** Port the Unicode CaseFolding.txt (status C+F, i.e. full case folding) table into rustty-unicode (e.g. a `case_folding_full(cp) -> &[u21]` accessor generated alongside the existing uucode-derived tables) and use it in `folded_codepoint`, copying up to 3 codepoints into the array exactly as the oracle does; add a test with a non-ASCII pair (e.g. 'É'/'é', 'ẞ'/'ss') through `Set::get_event` / `Trigger::folded_equal`.
- [x] **S104-F1** · CONFIRMED · `ABI` · behavioral-difference
  - **What:** Alt-ESC prefix writes the byte UTF-8-encoded instead of raw for bytes 0x80..=0xFF
  - **Oracle:** `src/input/key_encode.zig:407-412, 528-534` → **Rustty:** `crates/rustty-vt/src/input/key_encode.rs:483-488, 637-641`
  - **Audit's suggested fix:** Replace both sites with `writer.write_all(&[0x1B, byte])` so the raw byte is emitted exactly as the oracle does.

## Low (6)

- [ ] **S102-F1** · CONFIRMED · `INDIV` · weakened-assertion
  - **What:** parse_chain: first p.next() result is discarded instead of `try`-checked
  - **Oracle:** `src/input/Binding.zig:3484-3489` → **Rustty:** `crates/rustty-input/src/binding.rs:6280-6286`
  - **Audit's suggested fix:** Replace with `p.next().unwrap();` (or `assert!(p.next().is_ok())`).
- [ ] **S103-F2** · CONFIRMED · `INDIV` · different-constant
  - **What:** Markdown header ends with three newlines instead of two
  - **Oracle:** `src/input/helpgen_actions.zig:45-58` → **Rustty:** `crates/rustty-input/src/helpgen_actions.rs:66-80`
  - **Audit's suggested fix:** Drop one `\n` so the header ends with exactly two newlines.
- [ ] **S103-F3** · CONFIRMED · `INDIV` · weakened-assertion
  - **What:** `w3c` test iterates the hand-maintained TAG_NAMES table rather than every enum field, and the doc-claimed completeness test does not exist
  - **Oracle:** `src/input/key.zig:731-741` → **Rustty:** `crates/rustty-vt/src/input/key.rs:1417-1426, 583-588`
  - **Audit's suggested fix:** Add a test asserting `TAG_NAMES.len() == <variant count>` (e.g. `Key::Paste as usize + 1`) and that each index i has `TAG_NAMES[i].0 as usize == i`, or derive the table with a macro.
- [ ] **S105-F2** · DISPUTED · `ABI` · behavioral-difference
  - **What:** Mods drops the `_padding: u6` bits: from_int()/int() do not round-trip bits 10..15 as the Zig @bitCast does
  - **Oracle:** `src/input/key_mods.zig:29-40, 81-83, 116-118, 464-468` → **Rustty:** `crates/rustty-vt/src/input/key_mods.rs:82-91, 211-219, 260-262, 319-351`
  - **Audit's suggested fix:** Carry a `padding: u8` (6-bit) field in `Mods` populated by `from_int` from bits 10..15 and emitted by `int()`, or document the truncation as intentional at the C-ABI ingress.
- [ ] **S106-F1** · DISPUTED · `INDIV` · other
  - **What:** Options.from_terminal is a stale seam: takes (event, format, size) instead of (&Terminal, size)
  - **Oracle:** `src/input/mouse_encode.zig:39-50` → **Rustty:** `crates/rustty-vt/src/input/mouse_encode.rs:90-112`
  - **Audit's suggested fix:** Change signature to `from_terminal(t: &crate::terminal::Terminal, size: renderer_size::Size)` reading `t.flags.mouse_event`/`t.flags.mouse_format`, update surface.rs:8140 and drop the seam PORT NOTE.
- [ ] **S106-F2** · CONFIRMED · `LOG` · other
  - **What:** X10 coordinate-overflow `log.info` line is a no-op
  - **Oracle:** `src/input/mouse_encode.zig:123-126` → **Rustty:** `crates/rustty-vt/src/input/mouse_encode.rs:222-225`
  - **Audit's suggested fix:** Wire a crate-level log facade (or the `log` crate) and emit the info line; this is a crate-wide gap, not specific to this file.
