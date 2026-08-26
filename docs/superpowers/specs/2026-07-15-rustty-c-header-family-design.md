# rustty curated C header family + ABI-fidelity gate — design

**Date:** 2026-07-15
**Status:** approved (design), pending spec review → writing-plans

## Problem

ghostty's public C API is a **hand-curated header family**: `include/ghostty/vt.h`
(umbrella) → `include/ghostty/vt/*.h` (29 per-subsystem headers), with named enums
(`GHOSTTY_BUILD_INFO_SIMD`), typedef'd structs (`GhosttyString`), `GHOSTTY_API`
prototypes, and doc comments. ghostty's `example/c-vt-*` programs `#include
<ghostty/vt.h>` and rely on those curated names.

rustty instead ships a single **cbindgen-generated `include/rustty.h`**, where the
same ABI appears in a different _shape_ — e.g. `int rustty_build_info(int data,
void *out)` with the enum as a bare `int` and no named constants, no per-subsystem
split, different struct organization. So the c-vt examples cannot cleanly
symbol-swap onto rustty's header.

## Decisions (locked with the user)

1. **Adopt ghostty's curated header family as rustty's canonical public C API.**
   Token-rename all VT headers → `include/rustty/vt.h` + `include/rustty/vt/*.h`;
   this becomes _the_ public header. The cbindgen `rustty.h` steps back to an
   internal, code-derived ABI ground-truth (verification only). Precedent:
   `include/rustty-embed.h` (the token-renamed ghostty embedding umbrella).
2. **Mismatch policy = fix rustty to match ghostty by default; escalate genuine
   intentional divergences.** When a token-renamed header declaration doesn't
   match rustty's _actual_ exported ABI, treat it as a rustty fidelity bug and
   correct the Rust/C-ABI. If the mismatch is a deliberate, documented divergence
   (e.g. a faithful-but-safe one), make the header reflect it and flag it. The
   header adoption thus doubles as an ABI-fidelity audit that hardens rustty.

## Scope

VT family ONLY: `ghostty/vt.h` + `ghostty/vt/*.h` (30 headers). The top-level
embedding `include/ghostty.h` is already covered by `include/rustty-embed.h` and
is OUT of scope (YAGNI). Ports of the c-vt _examples_ themselves are a SEPARATE
follow-on plan (Phase B) built on top of this header foundation.

## Design

### Component 1 — the header family

`include/rustty/vt.h` (umbrella) + `include/rustty/vt/*.h` (29 subsystem headers),
token-renamed from ghostty's, applying the [[rename-casing-fidelity]] discipline:

- `GHOSTTY_`→`RUSTTY_`, `ghostty_`→`rustty_`, `Ghostty`→`Rustty`,
  `GHOSTTY_API`→`RUSTTY_API`, `<ghostty/…>`→`<rustty/…>`, include guards
  `GHOSTTY_*_H`→`RUSTTY_*_H`.
- doc-comment prose product name `Ghostty`→`Rustty` (capital); genuine
  oracle-path references (`src/…`, upstream file names) stay lowercase `ghostty`.
- Port ghostty's export/visibility macro (`GHOSTTY_API`→`RUSTTY_API`) and any
  platform bits (`vt/sys.h`).

### Component 2 — ABI-fidelity gate

New `.port/tools/gates/check_rustty_vt_headers.sh`, three checks:

- **(a) Compiles clean:** `cc -fsyntax-only -Iinclude` a TU that `#include
<rustty/vt.h>` (transitively pulls every subheader). No warnings/errors.
- **(b) Symbols real:** every `rustty_*` prototype declared in the curated headers
  is (i) an actual exported symbol in `librustty_vt_c` (`nm`), and (ii) present in
  the cbindgen-regenerated `rustty.h` with a compatible signature. Also flag the
  reverse (curated header declaring a symbol rustty does not export).
- **(c) Struct layouts match:** every curated struct's `sizeof`/field `offsetof`
  matches rustty's `#[repr(C)]` layout — cross-checked against
  `rustty_type_json()` (self-describing; this caught the wasm `usize` bug) AND a
  compile-time `static_assert(sizeof(...)==N && offsetof(...)==M)` probe compiled
  against the curated headers.

Enum discriminants are verified two ways: a static probe program that prints the
`build_info`/`key`/`mods`/etc. enum values and diffs them against ghostty's header
values, plus end-to-end via the Phase-B example parity (a wrong discriminant
changes an example's stdout).

Any mismatch → fix rustty (policy A); genuine divergence → header reflects it +
flag. Every rustty _code_ change gets a native test.

### Component 3 — cbindgen `rustty.h` role

Kept as the code-derived ABI ground truth consumed by check (b) — NOT the shipped
public API. The gate asserts the curated header's symbol/struct set agrees with
the cbindgen output, so the public header can never silently drift from what
rustty actually exports. (The existing `check_rustty_h_abi.sh` — cbindgen vs the
frozen ghostty oracle — is unaffected and complementary.)

### Execution

1. Token-rename the 30 headers (mechanical; parallelizable).
2. Run the gate; fix every mismatch per policy A (each code fix + native test).
3. Commit the header family + gate.
   Follow-on (separate plan): Phase B c-vt example ports build on this — each `.c`
   symbol-swapped, compiled `cc src/*.c -Iinclude -Ltarget/release -lrustty_vt_c`,
   run, stdout parity-diffed vs ghostty's `zig build run`. Parallelized via workflow.

## Testing

- The 3-check gate (`check_rustty_vt_headers.sh`) is the standing regression net.
- The Phase-B example-parity suite is the functional backstop for enum values +
  runtime behavior.
- Native `cargo build --workspace` + native tests stay green for every code fix.

## Non-goals

- Porting the c-vt/cpp/zig examples (separate Phase B/C plans).
- Touching the embedding `rustty-embed.h` / apprt header.
- Changing the cbindgen pipeline beyond using it as a verification input.
