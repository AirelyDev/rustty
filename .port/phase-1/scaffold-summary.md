# rustty — Phase-1 Workspace Scaffold Summary

**Date:** 2026-06-21
**Toolchain:** rustc 1.93.1 (01f6ddf75 2026-02-11) / cargo 1.93.1 (pinned in `rust-toolchain.toml`)
**Governs:** `.port/PORT_RULES.md` (R1–R5), the crate-structure design spec,
`.port/phase-0/plan/PORT_PLAN.md`, `DEFINITION_OF_PARITY.md`.

This phase creates the empty, GREEN Cargo workspace. **No reference logic is
ported yet** — every crate is a documented stub. The scaffold encodes the
file→crate partition, the topological order, and the dependency DAG from
PORT_PLAN so the acyclicity invariant (R4) is structurally true from day one.

---

## 1. What was created

- **`/Users/gael/projects/rustty/Cargo.toml`** — virtual workspace. Members
  listed in the strict leaves-first topological order from PORT_PLAN §1:
  the 29 parity crates under `crates/`, the 13 `-sys` crates under
  `crates-sys/`, and `xtask`. Includes `[workspace.package]` (version, edition
  2021, rust-version 1.93, license, repo, authors), `[workspace.dependencies]`
  (all internal crates by path so members declare each other via
  `dep.workspace = true`), and `[workspace.lints]` (rust + clippy).
- **29 `crates/` members** — each a `Cargo.toml` (name = crate IDENTITY, NOT the
  published artifact name; artifacts get the `rustty` identity later per R2) +
  `src/lib.rs` (or `src/main.rs` for the bin) carrying a module doc comment that
  records which ghostty `src/` dir/files it ports (from PORT_PLAN) and a
  `//! PORT STATUS: stub` line.
  - `rustty` is the **bin** crate (`src/main.rs`, `[[bin]] name = "rustty"`).
  - `rustty-vt-c` is `crate-type = ["cdylib", "staticlib", "rlib"]` (the C ABI layer).
- **13 `crates-sys/` members** — each a `Cargo.toml` with `links = "<lib>"` +
  `build = "build.rs"`, a `build.rs` **stub** (documented R3 TODO: vendor copied
  C source, compile via `cc`, bind via `bindgen`; **no C compiled yet**), and a
  stub `src/lib.rs`. Build scripts are intentionally empty so the workspace
  builds with zero native-toolchain requirements.
- **`xtask/`** — build-orchestration bin stub (replaces ghostty `src/build/`).
- **`rust-toolchain.toml`** — pins channel `1.93.1` + rustfmt/clippy.
- **`.gitignore`** — ignores `target/`, `**/*.rlib`, `examples-out/`, the
  harness `**/bin/` + compiled harness binaries (`.o/.a/.so/.dylib/.dSYM`), and
  the reference-oracle BUILD outputs under `baseline/` (the Zig compile cache
  `bench-rf-cache/` and the compiled install tree `bench-releasefast/`). KEEPS
  all `.port` docs/scripts/inventory/baseline data/corpora goldens and gate tools.
- **`.gitattributes`** — `text=auto` + explicit eol=lf for sources; `.vt`/
  `.golden`/`.bin` and compiled artifacts marked `binary` (byte-exact corpora
  must round-trip exactly).

## 2. Crate inventory (topological order — matches PORT_PLAN §1)

```
crates/ (29):
 1 rustty-util          2 rustty-build-config  3 rustty-identity
 4 rustty-datastruct    5 rustty-unicode       6 rustty-simd
 7 rustty-lib           8 rustty-global        9 rustty-render-types
10 rustty-os           11 rustty-vt (crown)   12 rustty-vt-c
13 rustty-crash        14 rustty-cfg-fmt      15 rustty-args
16 rustty-apprt-api    17 rustty-font         18 rustty-input
19 rustty-inspector    20 rustty-renderer     21 rustty-config
22 rustty-terminfo     23 rustty-termio       24 rustty-core
25 rustty-apprt        26 rustty-cli          27 rustty (bin)
28 rustty-bench        29 rustty-synthetic

crates-sys/ (13): freetype-sys harfbuzz-sys fontconfig-sys oniguruma-sys
  simdutf-sys highway-sys glslang-sys spirv-cross-sys wuffs-sys zlib-sys
  libpng-sys sentry-sys dcimgui-sys

xtask
```

Inter-crate path dependencies are wired per the PORT_PLAN §5 edge set, so the
DAG is real (not just declared). The graph is acyclic by construction — Cargo
itself would reject a cycle, and the order above is a valid topological sort.

## 3. Gate results (exact, not fabricated)

- `cargo build --workspace` → **GREEN**. All 43 crates compiled (29 + 13 + xtask),
  `Finished dev profile in ~4.2s`.
- `cargo test --workspace` → **GREEN**. 84 `test result: ok` blocks (unit + doc
  tests across all crates), **0 passed / 0 failed** (empty stubs), 0 ignored.
  No failures, no compile errors.

## 4. R-rule compliance notes

- **R1:** stubs add nothing and remove nothing; doc comments name the exact
  ghostty source each crate will port. Real lines/comments/`test{}` land as each
  crate is carved.
- **R2:** crate names are identities, not artifact names. `rustty-terminfo`,
  `rustty-identity`, etc. carry doc notes about the `identity-ghostty` feature
  that flips identity for the differential harness only.
- **R3:** zero ghostty Cargo dependency anywhere. `-sys` crates document
  vendored-C / cc+bindgen TODOs; no C is compiled yet.
- **R4:** members are in topological order; dependency edges encode the DAG;
  acyclic by Cargo construction (cycle_check.py remains the standing gate).
- **R5:** nothing weakened. Green is genuine (empty workspace).

## 5. Known follow-ups (not blockers)

- `-sys` build scripts are empty stubs — vendoring real C source is later work.
- The 256M of benchmark `corpora/*.bin` goldens are committed per the task's
  KEEP directive; consider Git LFS if repo size becomes a concern.
- Ambiguities A1–A6 from PORT_PLAN §4 are reflected in crate doc comments and
  resolved as each break lands.
