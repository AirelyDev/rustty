# Parity closure checklist — src/datastruct

5 open finding(s): 0 high, 0 medium, 5 low. Full evidence per row is in [`docs/parity-audit/src-datastruct.md`](../../docs/parity-audit/src-datastruct.md).

A row is CLOSED only when its fix has a test that fails without it (or a written, reviewed justification for why no test is possible).


## Low (5)

- [ ] **S80-F2** · CONFIRMED · `INDIV` · other
  - **What:** Mismatch diagnostics (`print(...)`) of expectApproxEqual are silenced in the Rust port
  - **Oracle:** `src/datastruct/comparison.zig:113,128,137-139,192-199` → **Rustty:** `crates/rustty-datastruct/src/comparison.rs:165,215,308-317`
  - **Audit's suggested fix:** Emit the same messages with eprintln! (gated on cfg(test) or an equivalent of testing.backend_can_print) in the f32/f64 and [T;N] failure paths.
- [x] **S81-F1** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** Use-site: `log.warn("error loading regular entry={d} err={}", it.index - 1, err)` dropped at the SegmentedList iterator consumer in Collection.autoItalicize
  - **Oracle:** `src/font/Collection.zig:343-351` → **Rustty:** `crates/rustty-font-shaper/src/collection.rs:470-482`
  - **Audit's suggested fix:** Emit `log::warn!("error loading regular entry={} err={:?}", index, err)` (index equals it.index - 1 at that point) before `continue`.
- [ ] **S83-F1** · DISPUTED · `INDIV` · other
  - **What:** Vim file Maintainer URL is not the mechanical brand rename
  - **Oracle:** `src/extra/vim.zig:9, 19, 48, 82` → **Rustty:** `crates/rustty-extra/src/vim.rs:22, 32, 61, 80`
  - **Audit's suggested fix:** None required if the AirelyDev URL is the intended identity; otherwise apply the per-token rename consistently (bash.rs keeps the ghostty-org URL).
- [ ] **S83-F3** · DISPUTED · `DEBUGONLY` · missing-test
  - **What:** Tripwire fail-point tests are `ignore`d in release-mode test runs
  - **Oracle:** `src/font/Atlas.zig:826-889` → **Rustty:** `crates/rustty-font/src/atlas.rs:1649, 1683, 1715`
  - **Audit's suggested fix:** Gate tripwire enablement on `cfg!(test)` (or a `tripwire` test feature) rather than `debug_assertions` so the tests run in release test builds.
- [ ] **S83-F6** · CONFIRMED · `DOC` · other
  - **What:** Stale PORT NOTE claims the +gen-* consumers are DEFERRED although xtask consumes the generators
  - **Oracle:** `src/extra/bash.zig:17-31` → **Rustty:** `crates/rustty-extra/src/lib.rs:21-25`
  - **Audit's suggested fix:** Update the lib.rs PORT NOTE to point at xtask/src/main_build_data.rs.
