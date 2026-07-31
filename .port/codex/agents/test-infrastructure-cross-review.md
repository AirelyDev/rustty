# Test-infrastructure cross-review

**Audit date:** 2026-07-31  
**Rustty revision:** `5243303fbb2f2cfd0832edfb8fa49d72ce322d81`  
**Ghostty revision:** `6ad1fe7d8cbda36c77b337a96c9bea8a77883699`  
**Scope:** independent review of two proposed root causes only. No production or
prior-report file was modified. Raw observations are under
`.port/codex/evidence/test-infrastructure-cross-review/`.

## Verdict summary

| ID | Candidate | Verdict | Severity | What is actually wrong |
|---|---|---|---|---|
| TI-001 | A: stale differential driver source/build path | **Verified** | **P3**, high confidence | The checked-in executables can still report equality, but the checked-in source cannot compile against the current reference header selected by the build script. This is oracle/harness drift after a synchronized Ghostty/Rustty ABI change, not a demonstrated product-parity gap. |
| TI-002 | B: fuzz smoke succeeds without a Rust fuzz target | **Verified** | **P3**, high confidence | G5 explicitly takes its progress path and exits 0 with cargo-fuzz installed but no `crates/rustty-vt/fuzz/`. Rustty has useful deterministic/randomized tests and corpus tooling, but no coverage-guided Rust fuzz harness and no live differential fuzz target. |

Both fit the audit plan's P3 definition: a meaningful test-contract weakness
without a demonstrated common-path failure (`.port/codex/AUDIT_PLAN.md:129-138`).
Neither supports P1/P2 product severity on the evidence observed here.

## TI-001 — current harness source cannot reproduce the passing binaries

**Verdict:** verified infrastructure defect; **not** a Rustty product gap.  
**Severity/confidence:** P3 / high.

### Contract and exact source drift

The shared driver still constructs a removed options aggregate and makes the old
three-argument call:

- `.port/phase-0/harness/ref_driver/vt_driver.c:123-130` declares
  `GhosttyTerminalOptions`, fills `cols`, `rows`, and `max_scrollback`, then calls
  `ghostty_terminal_new(NULL, &terminal, opts)`.
- `.port/phase-0/harness/build_port_driver.sh:75-89` selects the first existing
  header tree in this order: Rustty `target/include`, current Ghostty source
  `include`, then Ghostty `zig-out/include`.
- `.port/phase-0/harness/build_port_driver.sh:165-176` compiles that same
  `vt_driver.c` against the selected header and Rustty static library.

At the audited revisions, the live products agree on the replacement ABI:

- Ghostty `include/ghostty/vt/terminal.h:1397-1400` takes allocator, terminal,
  `uint16_t cols`, and `uint16_t rows`. Its current options enum is the singular
  `GhosttyTerminalOption` at `:1029-1031`; there is no current
  `GhosttyTerminalOptions` aggregate.
- Rustty `include/rustty/vt/terminal.h:1372-1375` has the same four-argument
  shape under the Rustty symbol identity.
- `crates/rustty-vt-c/src/terminal.rs:868-873` records that the options struct was
  removed and scrollback moved to terminal-set options 27/28.
- `crates/rustty-vt-c/src/lib.rs:2476-2483` implements the current Rust ABI with
  `cols` and `rows` as separate arguments.

Thus there is no present Ghostty-versus-Rustty constructor mismatch. The oracle
and port advanced together; the differential driver did not.

### Reproduction without repository mutation

I compiled the driver only to the syntax-check stage and wrote diagnostics under
`.port/codex/`. The effective probes were:

```sh
SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"
clang -std=c11 -fsyntax-only -isysroot "$SDKROOT" \
  -I/Users/gael/projects/ghostty/include \
  .port/phase-0/harness/ref_driver/vt_driver.c

clang -std=c11 -fsyntax-only -isysroot "$SDKROOT" \
  -I/Users/gael/projects/ghostty/zig-out/include \
  .port/phase-0/harness/ref_driver/vt_driver.c
```

Observed results:

- Current source header: exit 1. Clang reports unknown type
  `GhosttyTerminalOptions` at driver line 125 and "expected 4, have 3" at line
  130. Full output:
  `.port/codex/evidence/test-infrastructure-cross-review/candidate-a-current-header.log`.
- Stale generated header: exit 0. It still contains the old aggregate and
  three-argument declaration. Output and return-code record:
  `candidate-a-zig-out-header.log` and `candidate-a-return-codes.log`.

This is also the build script's default failure path in the present checkout:
`target/include/ghostty/vt.h` is absent, while both Ghostty trees exist, so lines
83-88 choose the incompatible current source header before the stale generated
one. That selection and file provenance are captured in
`candidate-a-provenance.log`.

### Why an old gate result can still look healthy

I ran the two already-checked-in executables against the existing basic fixture,
placing outputs only in the evidence directory:

```sh
.port/phase-0/harness/bin/vt_driver_ref \
  .port/phase-0/harness/fixtures/basic_text.vt \
  > .port/codex/evidence/test-infrastructure-cross-review/candidate-a-ref-old.out
.port/phase-0/harness/bin/vt_driver_port \
  .port/phase-0/harness/fixtures/basic_text.vt \
  > .port/codex/evidence/test-infrastructure-cross-review/candidate-a-port-old.out
cmp -s \
  .port/codex/evidence/test-infrastructure-cross-review/candidate-a-ref-old.out \
  .port/codex/evidence/test-infrastructure-cross-review/candidate-a-port-old.out
```

Both drivers exited 0; `cmp` exited 0; each output is 54 bytes with SHA-256
`e9b3727e5397c4fbac05679697fda0a94944aa8c243057b673df4e01b10d9a66`.
See `candidate-a-old-binaries.log` and the two `.out` files. This proves the
narrow claim that a checked-in binary comparison can pass even though the
current source gate cannot be regenerated. It does **not** prove that the old
binaries are behaviorally wrong.

The timestamps are consistent with staleness rather than a fresh current-ABI
build: `vt_driver_ref` is dated June 21, `vt_driver_port` July 10, while the
Ghostty constructor change is commit `03d5fa268902...` dated July 27 and the
Rustty port is `ede15d533b35...` dated July 30
(`candidate-a-provenance.log`). Timestamps are supporting provenance; the
current-header compile error is the decisive reproduction.

### Adversarial counter-checks

1. **Could a generated header make the candidate false?** It can make the C
   source syntax-check, but only because Ghostty's `zig-out/include` is itself
   stale. The checked build script prefers the live source header, so this does
   not restore a reproducible current-oracle gate.
2. **Could this be a Rustty ABI lag instead of test drift?** No. The current
   Ghostty header, Rustty header, and Rust implementation all take separate
   `cols`/`rows`; only the harness retains the removed struct.
3. **Do the checked binaries fail immediately?** No. The basic fixture comparison
   passes exactly. That counterevidence limits the finding to reproducibility and
   false reassurance from stale artifacts; no output divergence was observed.
4. **Does this prove every differential result is invalid?** No. It proves that
   the present checked-in source/build contract cannot reproduce the executables
   against current headers. Existing binary behavior may remain useful for some
   inputs, but its ABI/oracle provenance is no longer current.

### Non-macOS effect and severity

The observed executables and build recipe are macOS-hosted (the link command at
`build_port_driver.sh:167-176` includes Apple frameworks and ld64 alias glue), so
this is not evidence of a Linux/FreeBSD/Windows application failure. It remains
in non-macOS audit scope because the constructor and terminal C ABI are
platform-neutral, and this differential oracle is intended to validate shared VT
behavior consumed by non-macOS targets. The consequence is lost trustworthy
coverage of that shared behavior, hence P3 rather than a product P1/P2.

## TI-002 — G5 has no Rust fuzz target or live differential smoke

**Verdict:** verified test-coverage defect, with an important status-label
qualification.  
**Severity/confidence:** P3 / high.

### Observed progress success

`.port/tools/gates/run_fuzz_smoke.sh:2-8` describes G5 as differential fuzz clean
and names a future cargo-fuzz target `vt_diff` in `crates/rustty-vt/fuzz/`.
Its actual branching is explicit:

- `:81-86` recognizes cargo-fuzz and enters the real lane only when the fuzz
  directory also exists.
- `:86-105` is the only path that runs `cargo +nightly fuzz run vt_diff`.
- `:108-114` otherwise runs the progress self-test and exits 0.
- The self-test at `:48-67` checks the inventory file, synthetic crate, and plain
  `cargo`. Despite the promise at `:15-17`, it does not check cargo-fuzz or the
  seed corpus directory.

Exact observed commands:

```sh
bash .port/tools/gates/run_fuzz_smoke.sh
bash .port/tools/gates/run_fuzz_smoke.sh --self-test
cargo fuzz --version
test -d crates/rustty-vt/fuzz
```

The auto runner exited 0 after printing:

```text
PROGRESS mode: cargo-fuzz target 'vt_diff' not built yet
fuzz dir: .../crates/rustty-vt/fuzz (exists: no)
cargo fuzz available: yes
SELF-TEST: PASS
FUZZ-WIRED (port empty) ... G5 in progress mode
```

The self-test also exited 0; installed cargo-fuzz is `0.13.2`; the directory is
absent. See `candidate-b-auto.log`, `candidate-b-self-test.log`, and
`candidate-b-environment.log`.

There is no evidence this is being silently promoted to a completed hard gate:
`.port/tools/ci.sh:441` labels G5 `PROGRESS`, and `.port/tools/CI.md:43,55-59`
documents the exit-0 `FUZZ-WIRED` behavior. This qualification prevents the
stronger claim that aggregate CI mislabels the lane green. It does not supply the
missing fuzz coverage.

### Ghostty's current fuzz baseline

Ghostty has three platform-neutral coverage-guided targets:

- `test/fuzz-libghostty/build.zig:19-23` lists `osc`, `parser`, and `stream`;
  `:36-74` instruments and installs each through AFL++.
- `src/fuzz_osc.zig:15-44` feeds arbitrary payloads to the OSC parser and selects
  BEL/ST/missing terminators.
- `src/fuzz_parser.zig:8-14` drives the raw parser byte by byte.
- `src/fuzz_stream.zig:17-65` constructs a real terminal/stream and uses the first
  byte to choose slice or scalar ingestion. Its argv setup explicitly handles a
  Windows target at `:28-39`.
- `.github/workflows/test.yml:1531-1574` installs AFL++/LLVM and builds the fuzz
  harness. This CI job establishes buildability, not a time-budgeted fuzz
  campaign; the comparison should not overstate Ghostty's CI coverage.

The local inventory reaches the same design conclusion. It records the three
reference targets at `.port/phase-0/harness/FUZZ-INVENTORY.md:10-29`, describes
per-side and differential modes at `:70-116`, and defers mirror Rust harnesses to
"Phase 1+" at `:122-129`. Its claim at `:151-158` that differential mode is fully
operational is now undermined independently by TI-001: the comparator's checked
drivers are stale against the current constructor ABI.

### Search for a Rust substitute

The following production-tree searches returned no matches and no `fuzz/`
directory:

```sh
rg -n 'fuzz_target!|libfuzzer_sys|cargo-fuzz|afl::fuzz!|honggfuzz|bolero' \
  crates include macos --glob '!target/**'
rg -n 'proptest|quickcheck|arbitrary::|derive\(Arbitrary\)|kani::|cargo-afl' \
  crates include macos --glob '!target/**'
find crates -type d -name fuzz -print
```

This does not mean Rustty has no fuzz-derived safety net:

- `crates/rustty-vt/src/terminal.rs:16757-16775` has a deterministic randomized
  differential test comparing slice and scalar print paths over four geometries.
- Other VT unit tests retain AFL/fuzz regression inputs and randomized differential
  checks (enumerated by `candidate-b-rust-fuzz-search.log`).
- `crates/rustty-synthetic/src/lib.rs:1-17` provides corpus generation, but its
  own documentation distinguishes generating a corpus from directly fuzzing.
- G4's drivers provide fixed-corpus cross-implementation replay when their
  provenance is current; fixed replay is not a coverage-guided `vt_diff` smoke.

These are meaningful mitigations, but none is an executable, coverage-guided
Rust per-side target matching Ghostty's `osc`/`parser`/`stream`, and none evolves
inputs while comparing current Ghostty and Rustty behavior. Candidate B therefore
survives the alternate-harness challenge.

### Adversarial counter-checks

1. **Is exit 0 inherently deceptive?** Not in the aggregate runner: the lane is
   deliberately and visibly labeled `PROGRESS`, and the script prints
   `FUZZ-WIRED`, not `GREEN`. The defect is missing coverage and a weak progress
   self-test, not proven status laundering.
2. **Does `rustty-synthetic` count as a fuzzer?** No. It generates potential
   corpus data and explicitly says it does not directly fuzz.
3. **Do randomized/differential unit tests make the candidate false?** No. They
   exercise selected invariants with fixed seeds; they are not coverage-guided
   harnesses, do not mirror all three reference entry points, and do not run a
   live Ghostty-versus-Rustty comparator.
4. **Does Ghostty continuously fuzz in CI?** The cited workflow builds the
   instrumented harness but does not run `afl-fuzz`. Rustty's gap is therefore
   accurately stated as lacking even a buildable mirror/live smoke, not lacking a
   CI campaign that Ghostty demonstrably has.

### Non-macOS effect and severity

This is shared-core test coverage, not a macOS-only concern. The parser, OSC, and
terminal stream are consumed by Linux, FreeBSD, Windows, browser/WASM, and C-ABI
surfaces; Ghostty's stream harness itself contains target-aware Windows setup.
However, no non-macOS behavioral failure was produced. P3 is appropriate for the
absence of the parity-critical fuzz contract and for an exit-0 progress check
that validates less wiring than advertised; elevating it to P2 would require an
actual protocol/behavior divergence.

## Files produced

- Report: `.port/codex/agents/test-infrastructure-cross-review.md`
- Raw logs and small outputs:
  `.port/codex/evidence/test-infrastructure-cross-review/`
