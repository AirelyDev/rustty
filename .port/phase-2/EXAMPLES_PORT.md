# Port of ghostty's `example/` suite to rustty

**Goal:** 1:1 rustty counterparts of ghostty's `example/*` programs, each verified
against its ghostty twin with **exact side-by-side output parity**. Decided with
the user (2026-07-15): port the ENTIRE suite, wasm first; each example is
**self-contained + 1:1 with ghostty** (symbol/URL swaps only, same UI/behavior/
README), so it diffs cleanly against upstream.

## Foundation (done)
- `rustty-vt-c` builds freestanding (`wasm32-unknown-unknown`, pure-Rust via the
  scalar-simd / no-zlib wasm cfg): `target/wasm32-unknown-unknown/release/
  rustty_vt_c.wasm` — **906K, ZERO imports, 0 wasi** — the `ghostty-vt.wasm`
  analog. All exports the wasm examples need are present (terminal/formatter/sgr/
  key_encoder + `rustty_wasm_alloc_*` helpers + `type_json`). Build: `.port/tools/
  build-vt-wasm.sh` (→ this .wasm; the wasm examples load it, mirroring how all 3
  ghostty wasm examples load one `ghostty-vt.wasm`).

## Translation rules (per example)
- `ghostty_*` → `rustty_*` symbol names; `GHOSTTY_*` JS consts → `RUSTTY_*`;
  `Ghostty…` type-json keys → `Rustty…`; `ghostty-vt.wasm` → `rustty_vt_c.wasm`.
- Product-name prose: `Ghostty` → `Rustty` (capital); keep UI/layout/logic identical.
- Copy the `README.md` too (same translation).
- **Parity gate:** drive the rustty example and its ghostty twin with the SAME
  input; the output (formatted text / parsed attributes / encoded bytes) must be
  byte-identical. Verified in real Chrome, side by side (servers: rustty :8731,
  ghostty :8732).

## Phases
### Phase A — wasm examples (IN PROGRESS)  ·  all load `rustty_vt_c.wasm`
- [x] `wasm-vt` — **DONE, BYTE-EXACT PARITY (2026-07-15).** 1:1 symbol-swap of
  ghostty's; both modules: 111-byte input → identical 59-byte output
  (`Hello, World!\nGreen Bold and Underline\nLine 3: Overwritten!`). Verified
  side-by-side in Chrome + byte-diff via `.port/tools/wasm-parity.mjs`.
  **Surfaced + fixed a real fidelity bug:** rustty's `type_json` hardcoded `"u64"`
  for `usize` fields; on wasm32 `usize` is 4 bytes so it must narrow to `"u32"`
  (ghostty derives the label from `@bitSizeOf`). Fixed centrally in
  `types.rs::resolve_ty` (native output unchanged). All 24 example-touched fields
  now match ghostty's `type_json` exactly.
- [x] `wasm-sgr` — **DONE, PARITY CONFIRMED (2026-07-15).** 1:1 swap; same SGR sequence (`4:3;38;2;51;51;51;48;2;170;170;170;58;2;255;97;136`) → identical 4-attribute output in both (Underline curly / RESET_STRIKETHROUGH / FG RGB(170,170,170) / RESET_UNDERLINE). Verified side-by-side in Chrome; the SGR tag enum values match ghostty.
- [x] `wasm-key-encode` — **DONE, PARITY CONFIRMED (2026-07-15).** 1:1 swap; Ctrl+A encodes to an identical 7-byte Kitty sequence in both (Hex `1b 5b 39 37 3b 35 75` = `\x1b[97;5u`). Verified side-by-side in Chrome; the key/mods/action enum maps + key_encoder/key_event C-ABI match ghostty (set_utf8 name matches too).

**Phase A COMPLETE** — all 3 ghostty wasm examples ported 1:1 and parity-verified against their ghostty twins.

### Phase B — c-vt examples (~23)  ·  C programs vs `librustty` static lib + `include/rustty.h`
c-vt, -build-info, -cmake{,-cross,-static}, -colors, -effects, -encode-{focus,key,mouse},
-formatter, -grid-ref-tracked, -grid-traverse, -kitty-graphics, -modes, -paste, -render,
-selection{,-gesture}, -sgr, -size-report, -static, -stream. Build via `cc` against the
staticlib; parity = same stdout as the ghostty C example. (Separate build track, no browser.)

### Phase C — zig / cpp / swift
zig-vt, zig-vt-stream, zig-formatter, cpp-vt-stream, swift-vt-xcframework — port as the
respective toolchains allow (Swift reuses the RusttyKit xcframework path).

## Status ledger
(append per example as landed: `wasm-vt: DONE — parity vs ghostty confirmed (input X → output Y identical)`)
