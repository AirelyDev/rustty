# R2 §2.9: an unknown-OSC callback sets UNKNOWN_MAX_BYTES while it holds its content

An isolated, **non-FFI** model of the seam P8-W5 T3 ships (fix round 1, IMPORTANT 2). It follows the
memories `rust-mut-args-strongly-protected` and `shaper-hook-sb-clean-interleave` and the precedent
`../mirihar-n452`.

## The hazard

The vt-c terminal's unknown-OSC trampoline hands the C callback content that borrows the OSC parser's
**inline** buffer (`osc::Parser.buffer: [u8; MAX_BUF]`, inside `wrapper.stream.parser.osc_parser`). The
callback may legally call `rustty_terminal_set(UNKNOWN_MAX_BYTES)`, which upstream implements as a
write to `wrapper.stream.parser.osc_parser.unknown_max_bytes`. If the port formed `&mut osc::Parser`
(or `&mut Stream`/`&mut Parser`) for that write, the `Unique` retag would cover the buffer bytes the
callback's content still borrows.

## The model (`src/main.rs`)

`Wrapper { stream: Stream { handler, parser: Parser { osc_parser: OscParser { buffer, len,
unknown_max_bytes }, state } }, live_handler, .. }` in a `Box::into_raw` allocation whose provenance is
exposed once (vt-c `new_`). `vt_write` drives `next_slice_raw(addr_of_mut!((*w).stream))` (N452);
the stream ends the OSC with a temporary `&mut OscParser` whose result borrows the inline buffer and
dispatches it with `&mut (*this).handler`. The trampoline recovers the wrapper from the handler
address (the D3 `offset_of!` seam), publishes `live_handler`, and calls the callback with a C
`{ ptr, len }`. The callback sets the limit (APC half through `handler_ptr`, OSC half as the
candidate says) and then reads the content.

## Results (`bash run-all.sh`; logs in `outputs/`, table in `outputs-table.txt`)

| candidate | OSC limit write | callback's wrapper pointer | SB | TB |
|---|---|---|---|---|
| `mut_parser_strict` | `&mut (*w).stream.parser.osc_parser` | the handle's own provenance | **UB** | CLEAN |
| `raw_place_strict` | `(*w).stream.parser.osc_parser.unknown_max_bytes = v` (shipped) | the handle's own provenance | CLEAN | CLEAN |
| `mut_parser_exposed` | `&mut` as above | `with_exposed_provenance_mut` (shipped) | **UB** | CLEAN |
| `raw_place_exposed` | raw place (shipped) | `with_exposed_provenance_mut` (shipped) | CLEAN | CLEAN |

The SB report for the `&mut` write (`outputs/mut_parser_exposed.sb.log`): "not granting access to
tag <wildcard> because that would remove [SharedReadOnly for <…>] which is strongly protected", at
`let p: &mut OscParser = &mut (*w).stream.parser.osc_parser;`. The protected item is the content
borrow the trampoline holds.

**Tree Borrows does not separate the two writes.** TB tracks each byte on its own, and the `&mut`
write touches only the limit's bytes; the retag reads the buffer bytes but does not write them, so
the content stays readable. The SB result is the one the port must satisfy (the N441/N522
precedent).

**The FFI tests do not separate them either.** `reentrant_callback_unknown_osc` (rustty-vt-c) runs
clean with the shipped write, and also with the `&mut osc::Parser` write (checked in P8-W5 T3). The
likely reason (not proven here): in the shipped trampoline the content reaches the trampoline inside
the `UnknownSequence<'_>` value rather than as a bare `&[u8]` argument, so no protector covers it.
This harness, where the trampoline takes the content as a reference argument, is the evidence for
the shape.

Toolchain: miri 0.1.0 (6f72b5dd5f 2026-07-22), rustc 1.99.0-nightly (6f72b5dd5 2026-07-22); run 2026-10-08.
