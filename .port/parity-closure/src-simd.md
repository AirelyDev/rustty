# Parity closure checklist — src/simd

2 open finding(s): 0 high, 1 medium, 1 low. Full evidence per row is in [`docs/parity-audit/src-simd.md`](../../docs/parity-audit/src-simd.md).

A row is CLOSED only when its fix has a test that fails without it (or a written, reviewed justification for why no test is possible).


## Medium (1)

- [ ] **S120-F1** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** Scalar (wasm32) codepointWidth returns 1 for cp <= 0xFF where oracle's simd=false lane returns uucode width (0 for C0/C1 controls)
  - **Oracle:** `src/simd/codepoint_width.zig:7-12` → **Rustty:** `crates/rustty-simd/src/codepoint_width.rs:22-44`
  - **Audit's suggested fix:** Move the `cp <= 0xFF` early-out inside the `#[cfg(not(target_arch = "wasm32"))] if SIMD` block (it is only a hoist of the kernel's first statement), so the scalar lane keeps `if cp > MAX_CODE_POINT {1} else {props_uucode::get(cp).width}` unchanged.

## Low (1)

- [ ] **S120-F2** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** maxLenScalar drops the `log.warn("failed to calculate base64 size for payload")` on error
  - **Oracle:** `src/simd/base64.zig:18-23` → **Rustty:** `crates/rustty-simd/src/base64.rs:38-49`
  - **Audit's suggested fix:** Route through the workspace log facility (log::warn! with target "simd_base64") as other ported modules do.
