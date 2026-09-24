# Parity closure checklist — src/stb

2 open finding(s): 0 high, 1 medium, 1 low. Full evidence per row is in [`docs/parity-audit/src-stb.md`](../../docs/parity-audit/src-stb.md).

A row is CLOSED only when its fix has a test that fails without it (or a written, reviewed justification for why no test is possible).


## Medium (1)

- [ ] **S121-F1** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** Kitty image id drawn as u64 instead of u32 — same-seed output differs from oracle
  - **Oracle:** `src/synthetic/Kitty.zig:49-53` → **Rustty:** `crates/rustty-synthetic/src/kitty.rs:82-88`
  - **Audit's suggested fix:** Add `impl_uint_prim!(u32, 32, 4);` in rand.rs and change kitty.rs:88 to `self.rand.int_range_at_most::<u32>(1, 1_000_000)`; add a golden test pinning the first generated sequence for a fixed seed against the Zig output.

## Low (1)

- [x] **S121-F2** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** Args::String is whitespace-split instead of ArgIteratorGeneral quote/backslash parsing
  - **Oracle:** `src/synthetic/cli.zig:80-87` → **Rustty:** `crates/rustty-synthetic/src/cli.rs:396-413`
  - **Audit's suggested fix:** Port ArgIteratorGeneral's quote/backslash state machine (or share the one rustty-bench should also have) and use it for Args::String in parse_opts.
