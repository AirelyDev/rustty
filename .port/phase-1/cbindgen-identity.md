# Phase-1 — cbindgen toolchain + identity dual-profile

**Date:** 2026-06-21
**Scope:** (1) Flesh out `rustty-identity` as the R2 source-of-truth with the
`identity-oracle` parity feature. (2) Stand up the cbindgen toolchain for
`rustty-vt-c` and prove it emits `include/rustty.h` with `rustty_*` symbols.
**Governs:** `.port/PORT_RULES.md` (R1–R5) + the crate-structure design
(`docs/superpowers/specs/2026-06-21-rustty-crate-structure-design.md`, §6) +
`.port/phase-0/plan/PORT_PLAN.md` (crates #3 `rustty-identity`, #12
`rustty-vt-c`).
**Reference oracle (READ-ONLY):** `/Users/gael/projects/ghostty` — used only to
*read* the reference identity values; copied as constants (R3, no Cargo dep).

---

## 1. Identity dual-profile (R2)

### 1.1 Source of truth

`crates/rustty-identity` is the single crate holding every product-identity
value. There is no ghostty source file behind it — it is the additive R2 seam
(R1 allows additive glue that loses nothing). All values are plain `&str`
constants with two compile-time arms selected by the cargo feature
`identity-oracle`:

- **default (feature OFF) → `rustty`** — the published product identity.
- **feature ON → `ghostty`** — the reference values, for the parity /
  differential harness ONLY, so port output matches the oracle byte-for-byte
  (no canonicalization hacks). This feature must never ship in a published
  rustty artifact.

`PROFILE: Profile` resolves to `Profile::Rustty` or `Profile::Ghostty` at compile
time so consumers can branch/log on the active identity.

### 1.2 The values (rustty default vs. ghostty reference)

| Symbol | rustty (default) | ghostty (`identity-oracle`) | Reference source |
|---|---|---|---|
| `APP_NAME` | `rustty` | `ghostty` | `src/build_config.zig` |
| `VERSION` | `0.1.0` | `1.3.2-dev` | `build.zig.zon` (`.version`) |
| `BUNDLE_ID` | `io.airely.rustty` | `com.mitchellh.ghostty` | `src/build_config.zig:58` |
| `TERM` | `xterm-rustty` | `xterm-ghostty` | `src/config/Config.zig:3724` |
| `TERM_PROGRAM` | `rustty` | `ghostty` | `src/termio/Exec.zig:746` |
| `TERMINFO_PRIMARY_NAME` | `xterm-rustty` | `xterm-ghostty` | `src/terminfo/ghostty.zig` (1st `.names`) |
| `TERMINFO_PREFERRED_NAME` | `rustty` | `ghostty` | `src/terminfo/ghostty.zig` ("preferred name") |
| `CONFIG_DIR` | `rustty` | `ghostty` | `src/config/file_load.zig:14` (`.subdir`) |
| `CONFIG_FILE` | `config.rustty` | `config.ghostty` | `src/config/file_load.zig:14,55` |
| `XTVERSION_NAME` | `rustty` | `ghostty` | `src/termio/stream_handler.zig:1001` |
| `LIBVT_XTVERSION_FALLBACK` | `librustty` | `libghostty` | `src/terminal/stream_terminal.zig:376` |

Invariant captured in both arms: `TERMINFO_PRIMARY_NAME == TERM` (the tcell
requirement called out in `src/terminfo/ghostty.zig`).

### 1.3 VT query (XTVERSION) wire strings

Two helpers compose the exact wire bytes (the format strings are copied from the
reference verbatim, only the identity tokens flip per profile):

- `xtversion_response()` → `ESC P > | {XTVERSION_NAME} {VERSION} ESC \`
  - rustty:  `\x1bP>|rustty 0.1.0\x1b\\`
  - ghostty: `\x1bP>|ghostty 1.3.2-dev\x1b\\`
  - Mirrors `src/termio/stream_handler.zig:reportXtversion`
    (`"\x1BP>|{s} {s}\x1B\\"`, args `"ghostty"`, `build_config.version_string`).
- `libvt_xtversion_response()` → `ESC P > | {LIBVT_XTVERSION_FALLBACK} ESC \`
  - rustty:  `\x1bP>|librustty\x1b\\`
  - ghostty: `\x1bP>|libghostty\x1b\\`
  - Mirrors `src/terminal/stream_terminal.zig:reportXtversion`
    (`"\x1BP>|{s}\x1B\\"`, fallback `"libghostty"`).

**Note on Device Attributes (DA1/DA2).** In the reference the DA1/DA2 responses
are *not* a fixed identity string — they are encoded from the
`device_attributes` struct (DA1 default `CSI ? 62 ; ... c` from
`conformance_level=.vt220` + `features=.{.ansi_color}`; DA2 from
`device_type=.vt220`, firmware/ROM = 0). Those numeric responses are
identity-independent (a VT-conformance concern, owned by `rustty-vt`), so they
are **not** flipped by `identity-oracle`. The only DA-family string that is
identity-bearing is the XTVERSION (`CSI > 0 q`) name, captured above. This is
documented here so the boundary is explicit and nothing is silently dropped (R1).

### 1.4 Unit test (passing, both profiles)

`crates/rustty-identity/src/lib.rs` has one feature-gated test per profile:

- `default_profile_is_rustty` (compiled when the feature is OFF) asserts every
  value is the rustty identity, the tcell invariant holds, the two XTVERSION
  wire strings are exact, and no value leaks the substring `ghostty`.
- `ghostty_profile_matches_reference` (compiled when the feature is ON) asserts
  every value equals the reference value read from the oracle, the tcell
  invariant holds, and both XTVERSION wire strings match the reference
  byte-for-byte.

**Results (run live; not fabricated):**

```
$ cargo test -p rustty-identity
running 1 test
test tests::default_profile_is_rustty ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; ...

$ cargo test -p rustty-identity --features identity-oracle
running 1 test
test tests::ghostty_profile_matches_reference ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; ...
```

`cargo clippy -p rustty-identity` and
`cargo clippy -p rustty-identity --features identity-oracle` are both clean
(no warnings) under the workspace lints.

---

## 2. cbindgen toolchain for `rustty-vt-c` (R2)

### 2.1 Goal

Per R2 the published C ABI is `include/rustty.h` (+ a `rustty/vt/*` header
family as the ABI is ported) with all symbols carrying the `rustty_` prefix.
`rustty-vt-c` is currently an empty stub (the real ABI lands with break #4 /
PORT_PLAN crate #12), so the objective here is to **prove the cbindgen pipeline
works end-to-end** and document the exact command.

### 2.2 What was created

- `crates/rustty-vt-c/cbindgen.toml` — cbindgen config: `language = "C"`,
  include guard `RUSTTY_H`, `cpp_compat = true`, doxygen docs, autogen warning,
  `parse.parse_deps = false` (reads only the rustty-vt-c crate — R3, never the
  ghostty tree). No symbol rename prefix is applied because the exported symbols
  are already authored with the `rustty_` prefix in the crate.
- `crates/rustty-vt-c/src/lib.rs` — one toolchain-proof exported symbol:
  `#[no_mangle] pub extern "C" fn rustty_vt_abi_version() -> c_int`. This exists
  ONLY to give cbindgen a `rustty_*` symbol to emit; it is replaced by the
  ported `terminal/c/` ABI when break #4 lands.
- `xtask` `header-gen` subcommand (`xtask/src/main.rs`) — drives the cbindgen
  **CLI** (kept light: shells out, does not link the cbindgen crate) to write
  `include/rustty.h`. Run with `cargo run -p xtask -- header-gen`.
- `include/rustty.h` — the generated header (committed artifact / proof).

### 2.3 Exact command

The xtask subcommand is equivalent to this manual invocation (run from the
workspace root):

```sh
cbindgen \
  --config crates/rustty-vt-c/cbindgen.toml \
  --crate rustty-vt-c \
  --output include/rustty.h
```

(Installed toolchain: `cbindgen 0.29.4`, `cargo 1.93.1`. Install if missing:
`cargo install cbindgen`.)

### 2.4 Generated header (proof it works)

`cbindgen` exited 0 and produced `include/rustty.h`:

```c
/**
 * @file rustty.h
 * librustty-vt - Virtual terminal emulator library (rustty). ...
 * all symbols carry the `rustty_` prefix (R2). Do not edit by hand.
 */
#ifndef RUSTTY_H
#define RUSTTY_H
/* Generated by cbindgen from rustty-vt-c. DO NOT EDIT BY HAND. (R2) */
#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#ifdef __cplusplus
extern "C" {
#endif // __cplusplus
/** Returns the ABI version of the rustty-vt C interface. ... */
int rustty_vt_abi_version(void);
#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus
#endif  /* RUSTTY_H */
```

The `rustty_` prefix, doxygen comments, include guard, and C++ `extern "C"`
guard are all present — the toolchain is proven.

---

## 3. Gate change (R2): from ABI-equivalence to self-consistency + behavioral

Per `.port/PORT_RULES.md` R2, the Parity Constitution's identity-EQUIVALENCE
gates are **DROPPED** for rustty:

- ~~`ghostty.h` ABI match~~ (dropped — rustty publishes its own `rustty.h`)
- ~~Swift-app-links-unchanged~~ (dropped)
- ~~terminfo == ghostty~~ (dropped — rustty publishes terminfo `rustty`)

**The replacement gate for the C header is two-part:**

1. **Header self-consistency** — `cbindgen` succeeds *and* the emitted header
   compiles cleanly as both C and C++ and is idempotent under double-inclusion.
   Verified (run live):
   - `clang -xc  -std=c11   -Wall -Wextra -I include -fsyntax-only <stub>.c`  → OK
   - `clang -xc++ -std=c++17 -Wall -Wextra -I include -fsyntax-only <stub>.cpp` → OK
   - double `#include "rustty.h"` compiles (include guard `RUSTTY_H`) → OK
2. **Behavioral parity via `identity-oracle`** — the differential harness builds
   the port with `--features identity-oracle` so every identity-bearing byte
   (TERM, TERM_PROGRAM, terminfo, XTVERSION, config paths, version) matches the
   reference oracle exactly, and the BEHAVIORAL parity gates (ported assertions,
   differential output, conformance/goldens, fuzz) — all of which remain in full
   force under R5 — run against that build.

All BEHAVIORAL parity gates from the constitution remain invariant (R5);
only the identity-EQUIVALENCE gates are exchanged for the above.

---

## 4. Files created / modified

Created:
- `crates/rustty-vt-c/cbindgen.toml`
- `include/rustty.h` (generated)
- `.port/phase-1/cbindgen-identity.md` (this file)

Modified:
- `crates/rustty-identity/Cargo.toml` — added `[features] identity-oracle = []`.
- `crates/rustty-identity/src/lib.rs` — identity source-of-truth + dual-profile
  values + XTVERSION composers + the two per-profile unit tests.
- `crates/rustty-vt-c/src/lib.rs` — toolchain-proof `rustty_vt_abi_version`
  export (replaced when break #4 lands).
- `xtask/src/main.rs` — `header-gen` subcommand driving the cbindgen CLI.

---

## 5. Follow-ups (when the real crates land)

- Replace `rustty_vt_abi_version` with the ported `terminal/c/` ABI (break #4,
  PORT_PLAN crate #12; carve `include/rustty/vt/*` to mirror the reference's
  `ghostty/vt/*` layout).
- `rustty-terminfo` (crate #22) consumes `rustty-identity`'s
  `TERMINFO_PRIMARY_NAME` / `TERMINFO_PREFERRED_NAME` (and emits the
  `xterm-ghostty`/`ghostty` entry under `identity-oracle` for the harness).
- `rustty-config` / `rustty-os` consume `CONFIG_DIR` / `CONFIG_FILE`; `termio`
  consumes `TERM` / `TERM_PROGRAM`; `rustty-vt` wires `XTVERSION_NAME` /
  `xtversion_response()` into the XTVERSION effect.
- Add a CI gate that runs `cargo run -p xtask -- header-gen` and fails if
  `include/rustty.h` is stale (drift check) plus the C/C++ self-consistency
  compile.
