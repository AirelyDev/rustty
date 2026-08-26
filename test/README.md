# Tests

PORTS: ghostty `test/`.

Manual and out-of-band test programs. The automated suite lives elsewhere —
`cargo test --workspace` for the port's own tests, and `.port/tools/ci.sh` for
the gate suite.

| Path             | What it is                                                                            |
| ---------------- | ------------------------------------------------------------------------------------- |
| `pkgconfig/`     | The C smoke program `G-CCONSUMER` compiles against the installed pkg-config metadata. |
| `windows/`       | `test_dll_init.c` — loads `rustty_apprt_c.dll` and calls across the C ABI.            |
| `ucs-detect.sh`  | Runs [ucs-detect](https://github.com/jquast/ucs-detect) with pinned settings.         |
| `wasm-alloc.mjs` | Allocator shim for the wasm examples.                                                 |

## Where the fuzzers live

The reference keeps its AFL++ harnesses in `test/fuzz-libghostty/`. rustty's
are cargo-fuzz targets and live with the crate they fuzz, at
`crates/rustty-vt/fuzz/fuzz_targets/` — `fuzz_osc`, `fuzz_parser` and
`fuzz_stream` (the ports of the reference's three) plus `vt_diff` (rustty's own
differential target against the G4 harness). Their seed corpora are
`.port/phase-0/corpora/cvt/{osc,parser,stream}-initial`.

All four are run by gate `G5` (`.port/tools/gates/run_fuzz_smoke.sh`).
