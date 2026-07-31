# Cross-review: shared runtime and test infrastructure

Audit date: 2026-07-31

Inputs reviewed:

- `.port/codex/agents/shared-runtime.md`
- `.port/codex/agents/test-infrastructure-cross-review.md`
- Rustty `5243303fbb2f2cfd0832edfb8fa49d72ce322d81`
- Ghostty `6ad1fe7d8cbda36c77b337a96c9bea8a77883699`

The review independently re-ran available checks, traced each target contract in
both repositories, challenged severity and product scope, and compared roots
against `.port/codex/agents/platform-targets.md`. All four proposed root causes
withstand review, but one must be merged as a duplicate, one has overlapping
target-level impact, and the test-infrastructure report needs a path correction
and two important scope qualifications.

## Final cross-review verdict

| Candidate | Verdict after falsification | Final severity | Catalog treatment |
|---|---|---:|---|
| SR-001 Windows `rustty-os` compile failure | Verified | P1 | Exact root-cause duplicate of PT-001; merge, do not count twice. Use SR-001's broader shared-runtime consequence and PT-001's VT-library reachability/control evidence. |
| SR-002 FreeBSD OpenGL cfg/export contradiction | Verified | P1 | Keep as a distinct root cause. Its FreeBSD outage overlaps PT-002's missing GTK backend, but each independently blocks the target and requires a different fix. |
| TI-001 stale VT differential driver ABI | Verified infrastructure defect | P3 | Keep. It is not a product behavior divergence; it invalidates reproducibility and current-oracle provenance of a hard parity gate. |
| TI-002 missing Rust coverage-guided/differential fuzz target | Verified test-contract weakness | P3 | Keep with qualifications: G5 is honestly labeled `PROGRESS`, and Ghostty CI builds but does not run a fuzz campaign. |

## SR-001 — verified, but duplicate of PT-001

### Evidence that withstands review

Rustty declares `libc` only for Unix (`crates/rustty-os/Cargo.toml:67-71`) but
compiles locale, mach, passwd, path, and PTY on every non-WASM target
(`crates/rustty-os/src/lib.rs:76-101`). The Windows page allocator emits two
explicit compile errors (`crates/rustty-os/src/page_allocator.rs:350-366`), the
passwd module emits another (`crates/rustty-os/src/passwd.rs:182-188`), locale,
mach, and path name the absent `libc`, and `WindowsPtyError` is boxed as
`dyn Error` without implementing the trait (`crates/rustty-os/src/pty.rs:525-533`,
`crates/rustty-os/src/pty.rs:638-649`).

A fresh independent rerun of

```text
cargo check -q -p rustty-os --target x86_64-pc-windows-msvc --offline
```

exited 101 with the same 25 Rust errors. The control
`cargo check -q -p rustty-xev --target x86_64-pc-windows-msvc --offline` exited
0, so the installed target and Rustty's Windows event-loop dependency are not
blanket blockers.

Ghostty's contract is broader than a type existing in source: native Windows CI
tests/builds lib-vt and static linking
(`ghostty/.github/workflows/test.yml:728-747`) and separately builds the full
headless library with GNU ABI (`ghostty/.github/workflows/test.yml:749-761`). Its
termio implementation passes ConPTY to the child and reads its pipes on Windows
(`ghostty/src/termio/Exec.zig:1013-1033`,
`ghostty/src/termio/Exec.zig:1075-1085`,
`ghostty/src/termio/Exec.zig:1770-1817`).

### Merge correction

SR-001 and PT-001 are the same `rustty-os` target-gating root, observed through
two downstream products. They must become one P1 finding:

- VT C ABI consequence: `rustty-vt-c` transitively reaches `rustty-os` and
  cannot produce Ghostty's Windows lib-vt artifact.
- Shared-runtime consequence: termio/core cannot reach the already-written
  ConPTY path, and an `app-runtime=none`-style Windows library cannot compile.

Reporting separate P1s would double-count one remediation boundary. The native
C/C++ dependency failures seen in whole-library cross-checking remain an
environmental lead because this host lacks a Windows SDK; they are not required
to prove this Rust source failure.

## SR-002 — verified as a distinct P1 blocker

### Evidence that withstands review

Both projects select OpenGL for every non-WASM, non-Darwin target:

- Rustty: `crates/rustty-build-config/src/renderer.rs:18-30`.
- Ghostty: `ghostty/src/renderer/backend.zig:10-21`, with the selected OpenGL
  renderer instantiated at `ghostty/src/renderer.zig:36-42`.

Rustty nevertheless declares the OpenGL module only on Linux
(`crates/rustty-renderer/src/lib.rs:98-106`), exports the concrete generic
renderer only on Linux (`crates/rustty-renderer/src/lib.rs:181-189`), and gates
the implementation module the same way
(`crates/rustty-renderer/src/generic.rs:7604-7612`). Its GL/EGL dependencies are
also under the Linux-only target table
(`crates/rustty-renderer/Cargo.toml:205-220`). There is no FreeBSD renderer alias.
At the same time, core names `rustty_renderer::Renderer` on every non-macOS
target (`crates/rustty-core/src/renderer_seam.rs:242-253`).

`rustc --print cfg --target x86_64-unknown-freebsd` freshly confirmed
`target_os="freebsd"`. A repo-wide alias/gate search found only macOS Metal,
Linux OpenGL, and wasm WebGL exports, which falsifies the possibility of a
platform-neutral or alternate FreeBSD renderer alias.

### Severity and overlap correction

P1 withstands review: a target that Ghostty selects and builds as OpenGL lacks
the public renderer type that Rustty core unconditionally consumes. No runtime
reproducer is needed to decide this deterministic cfg/name-resolution failure,
although the missing FreeBSD Rust standard library prevents capturing the final
compiler diagnostic.

This is not a duplicate of PT-002. PT-002 is a GTK application-runtime gate;
SR-002 is a renderer/core gate. Fixing either alone still leaves FreeBSD broken.
For executive summaries, however, they should be presented as two independent
blockers under one FreeBSD-target outage, not as two separate affected-target
counts. The same missing renderer alias is a later-stage Windows blocker after
the merged SR-001/PT-001 failure.

## TI-001 — verified P3: hard G4 gate uses stale binaries

### Evidence that withstands review

The shared C driver still constructs `GhosttyTerminalOptions` and calls the old
three-argument constructor
(`.port/phase-0/harness/ref_driver/vt_driver.c:123-130`). The checked build script
prefers Rustty `target/include`, then Ghostty's current source header, then
Ghostty's generated header
(`.port/phase-0/harness/build_port_driver.sh:75-89`) and compiles that shared
source at `.port/phase-0/harness/build_port_driver.sh:165-176`.

The first include candidate is absent. Ghostty's preferred live header instead
declares the current four-argument constructor
(`ghostty/include/ghostty/vt/terminal.h:1397-1400`), as does Rustty
(`include/rustty/vt/terminal.h:1372-1375`). An independent syntax check against
the live Ghostty include tree exited 1 with:

```text
unknown type name 'GhosttyTerminalOptions'
too few arguments to function call, expected 4, have 3
```

The same syntax check against `ghostty/zig-out/include` exited 0, confirming that
the generated header is stale enough to accept the old driver rather than
restoring a current-oracle build.

The checked binaries remain executable and mutually equal. More importantly, a
fresh `bash .port/tools/gates/run_differential.sh` exited 0 and printed
`GREEN: byte-exact` across all 12 fixtures and 21 C-VT golden cases. Thus the
hard G4 result can look current even though the source cannot regenerate its
oracle driver against the header the build script actually selects.

### Severity and scope correction

P3 is appropriate. This is stronger than a cosmetic stale comment because it
breaks reproducibility and current-oracle provenance of a hard parity gate. It
is weaker than P2 because no terminal output divergence or non-macOS product
failure was demonstrated; the current Rustty and Ghostty public constructor
ABIs agree. Existing binary comparisons may still catch many behavioral
differences, but cannot establish comparison against the pinned current ABI.

There is no overlap with the platform findings: this is platform-neutral test
infrastructure consumed as evidence for all targets.

## TI-002 — verified P3 with citation and status corrections

### Evidence that withstands review

The G5 runner names a future Cargo fuzz target at
`crates/rustty-vt/fuzz/` (`.port/tools/gates/run_fuzz_smoke.sh:2-17`). It enters
the real fuzz lane only when cargo-fuzz and that directory exist
(`.port/tools/gates/run_fuzz_smoke.sh:81-105`); otherwise it runs a progress
self-test and exits 0 (`.port/tools/gates/run_fuzz_smoke.sh:108-117`). The
self-test checks only the inventory, synthetic-crate directory, and plain Cargo
(`.port/tools/gates/run_fuzz_smoke.sh:48-67`), not the advertised seed-corpus
directory or cargo-fuzz itself.

Fresh observations matched the report:

- `cargo fuzz --version`: exit 0, `cargo-fuzz 0.13.2`.
- `test -d crates/rustty-vt/fuzz`: exit 1.
- Searches for Rust coverage-guided/property fuzz entry points in production
  crates returned no matches.
- `bash .port/tools/gates/run_fuzz_smoke.sh`: exit 0, explicitly reported
  `PROGRESS mode`, `fuzz dir ... exists: no`, and `FUZZ-WIRED`.

Ghostty has three instrumented AFL++ targets—OSC, parser, and terminal stream—at
`ghostty/test/fuzz-libghostty/build.zig:19-23` and
`ghostty/test/fuzz-libghostty/build.zig:36-74`. The stream target constructs a
real terminal and exercises slice/scalar ingestion, including target-aware
Windows argv setup
(`ghostty/test/fuzz-libghostty/src/fuzz_stream.zig:17-65`). Ghostty CI installs
the fuzz toolchain and builds these harnesses
(`ghostty/.github/workflows/test.yml:1531-1574`).

### Explicit corrections and qualifications

1. The source paths in `test-infrastructure-cross-review.md` are incomplete when
   read from the repository root. Replace `src/fuzz_osc.zig`,
   `src/fuzz_parser.zig`, and `src/fuzz_stream.zig` with
   `test/fuzz-libghostty/src/fuzz_osc.zig`,
   `test/fuzz-libghostty/src/fuzz_parser.zig`, and
   `test/fuzz-libghostty/src/fuzz_stream.zig` respectively. The cited line
   content is otherwise correct.
2. G5 is not laundering a missing fuzz target as a completed green gate:
   `.port/tools/ci.sh:441` labels it `PROGRESS`, and the standalone runner prints
   `FUZZ-WIRED`, not fuzz-clean `GREEN`. The verified defect is absent coverage
   and a weak progress self-test.
3. Ghostty CI proves the three per-side fuzz harnesses build; it does not run a
   time-budgeted `afl-fuzz` campaign. Rustty's missing `vt_diff` is also a local
   declared differential contract, while the direct Ghostty comparison is the
   absence of buildable coverage-guided parser/OSC/stream counterparts. These
   narrower statements support P3, not P2.

Randomized Rustty unit tests and `rustty-synthetic` mitigate the gap but do not
provide input-evolving, coverage-guided execution or a live current-oracle
comparison. TI-001 further prevents the checked driver binaries from supplying
current-ABI provenance. TI-001 and TI-002 are related evidence weaknesses but
remain distinct roots: updating the C driver would not create a fuzz target,
and adding a fuzz target would not refresh the fixed-corpus oracle binaries.

## Consolidation guidance

The findings catalog should represent these reviewed claims as:

1. **One P1 Windows target-gating finding** combining PT-001 and SR-001.
2. **One P1 FreeBSD renderer/core finding** from SR-002, alongside—but not merged
   with—the independent PT-002 FreeBSD GTK launch finding.
3. **One P3 stale differential-oracle finding** from TI-001.
4. **One P3 missing coverage-guided/differential fuzz finding** from TI-002.

No severity increase is supported for TI-001/TI-002 without a behavioral
counterexample. No severity downgrade is supported for the two target compile
failures because each satisfies the audit plan's P1 target-unbuildable criterion.

## Verification limits

- No FreeBSD standard library or runtime was available, so SR-002 was verified
  by deterministic cfg/name resolution rather than a full target compile.
- A Windows SDK was unavailable, but SR-001 was reproduced in Rust source before
  any SDK-dependent conclusion.
- The current G4 hard gate and G5 progress gate were executed locally. No fuzz
  campaign was run because the declared Rust fuzz target/directory is absent.
- No production source was edited. This cross-review authored only this report;
  existing audit evidence and gate output remain beneath `.port/codex/`.
