export const meta = {
  name: 'phase1-workspace-gates',
  description: 'Ghostty Zig->Rust port Phase 1: Cargo workspace, reference FFI bridge, cbindgen ABI gate, full parity-gate harness (PASS/PENDING/FAIL), CI, injected-divergence self-test',
  phases: [
    { title: 'Scaffold', detail: 'Cargo workspace mirroring src/, MODULE_STATUS, libghostty-vt crate, ghostty bin, reference FFI bridge with passing smoke test' },
    { title: 'Gates', detail: 'gate scripts (tests/leaks/diff_vt/fuzz/abi/perf/tables/layout) + gate_all + ci.sh + selftest.sh' },
    { title: 'Prove', detail: 'CI green on empty workspace; selftest injects a divergence per gate -> red -> revert -> green' },
    { title: 'Critique', detail: 'adversarial: CI not hollow, selftest exercises real failure paths, refbridge links reference, cbindgen real, MODULE_STATUS enforces parity' },
  ],
}

const REF = '/Users/gael/projects/ghostty'
const PORT = '/Users/gael/projects/rustty'
const PORTDIR = PORT + '/.port'
const P1 = PORTDIR + '/phase-1'

const FACTS = `
GROUND-TRUTH (verified; do not re-derive):
- Reference (READ-ONLY source): ${REF} @ 4749c4e93731067049bfbf2e4572061cef2bdd17. Zig 0.15.2.
- Prebuilt reference C ABI: ${REF}/zig-out/lib/libghostty-vt.a (+ .dylib), headers ${REF}/zig-out/include/ghostty/vt.h + ghostty/vt/*.h (30 headers), app header ${REF}/include/ghostty.h. The .a is self-contained (statically bundles simdutf etc.); linking it from Rust needs -lc++ and (on macOS) system libs only.
- C ABI surface: exactly 162 exported _ghostty_* symbols in libghostty-vt.0.1.0.dylib (nm -gU); ~164 ghostty_* decls across the headers. Symbols MUST stay ghostty_* (identity invariant) — never rename to rustty_*.
- Port repo (git): ${PORT}. Phase 0 committed at ${PORTDIR}/phase-0/ : PARITY_LEDGER.json, PORT_PLAN.md, module_graph.json, DEFINITION_OF_PARITY.md, TEST_RECONCILIATION.md, BASELINE.json. Canonical baseline copy at ${PORTDIR}/baseline/BASELINE.json. Differential VT harness at ${PORTDIR}/harness/ (oracle.c -> build via: clang -O2 -I ${REF}/zig-out/include ${PORTDIR}/harness/oracle.c ${REF}/zig-out/lib/libghostty-vt.a -o ${PORTDIR}/harness/oracle -lc++ ; note: in THIS env 'cc' may alias to 'claude' so call /usr/bin/clang explicitly). diff_vt.py runs corpus through oracle vs port oracle at ${PORTDIR}/harness/oracle-port (absent => red).
- Toolchain: Rust 1.93.1 / cargo 1.93.1; cbindgen 0.29.4 at /Users/gael/.cargo/bin/cbindgen; Xcode CLT present (libclang available for the bindgen crate). 'hyperfine' NOT installed (use /usr/bin/time -l).
- NAMING RULE (critical): "rustty" names crates/workspace ONLY. User-observable identity stays ghostty: the produced binary is 'ghostty'; the VT lib is 'libghostty-vt' exporting ghostty_* symbols; TERM='xterm-ghostty'; config dir ~/.config/ghostty; version 1.3.2-dev. Do NOT rename these.

PORT_PLAN topo order (leaves first; <root> SPLIT into a leaf util crate ported FIRST and a binary glue crate ported LAST):
  util(leaf) -> datastruct, simd, unicode, terminfo -> os, crash -> terminal (67% critical path) -> font, renderer -> config, input -> termio, apprt -> cli -> synthetic/inspector/benchmark -> libghostty-vt lib -> ghostty binary (glue, last).

THE 21 REFERENCE MODULES and their static test-block counts (sum = 3234; this is the per-module parity contract from PARITY_LEDGER.by_module):
  terminal=2178, input=245, font=178, config=175, cli=108, datastruct=76, os=57, apprt=46, termio=33, <root>=29, renderer=25, simd=23, lib=13, synthetic=12, crash=9, benchmark=8, build=7, unicode=5, terminfo=4, extra=2, inspector=1.

Write all NEW artifacts under ${PORT} (Cargo workspace at repo root) and ${PORTDIR} (gates/reports). DO NOT modify the reference repo. DO NOT git commit (orchestrator commits). DO NOT weaken/skip/xfail any gate to make it pass — that is a critical failure.
`

const SCAFFOLD_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['workspace_builds', 'workspace_tests_pass', 'crate_count', 'crates', 'libghostty_vt_crate', 'ghostty_bin_crate', 'refbridge_smoke_pass', 'module_status_written', 'module_status_sum', 'notes'],
  properties: {
    workspace_builds: { type: 'boolean', description: 'cargo build --workspace exit 0' },
    workspace_tests_pass: { type: 'boolean', description: 'cargo test --workspace exit 0' },
    crate_count: { type: 'integer' },
    crates: { type: 'array', items: { type: 'string' } },
    libghostty_vt_crate: { type: 'string', description: 'crate that produces libghostty-vt (cdylib+staticlib)' },
    ghostty_bin_crate: { type: 'string', description: 'crate that produces the ghostty binary' },
    refbridge_smoke_pass: { type: 'boolean', description: 'a Rust test that calls the reference ghostty_terminal_new/vt_write/format via FFI and gets correct output' },
    module_status_written: { type: 'boolean' },
    module_status_sum: { type: 'integer', description: 'sum of reference_tests across MODULE_STATUS entries; MUST be 3234' },
    notes: { type: 'string' },
  },
}

const GATES_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['scripts_written', 'notes'],
  properties: {
    scripts_written: { type: 'array', items: { type: 'string' }, description: 'absolute paths of gate scripts authored' },
    notes: { type: 'string' },
  },
}

const PROVE_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['ci_exit_code', 'ci_green_on_empty', 'selftest_results', 'all_gates_detect_divergence', 'state_reverted_clean', 'proof_path', 'notes'],
  properties: {
    ci_exit_code: { type: 'integer', description: 'must be 0 on empty workspace' },
    ci_green_on_empty: { type: 'boolean' },
    selftest_results: { type: 'array', items: { type: 'object', additionalProperties: true }, description: 'per gate: {gate, injected_exit (nonzero), reverted_exit (zero)}' },
    all_gates_detect_divergence: { type: 'boolean', description: 'true iff every gate exits non-zero on its injected divergence AND zero after revert' },
    state_reverted_clean: { type: 'boolean', description: 'workspace + reference restored to pre-selftest state' },
    proof_path: { type: 'string' },
    notes: { type: 'string' },
  },
}

const CRITIC_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['check', 'passed', 'blocker', 'findings'],
  properties: {
    check: { type: 'string' },
    passed: { type: 'boolean' },
    blocker: { type: 'boolean' },
    findings: { type: 'array', items: { type: 'string' } },
  },
}

phase('Scaffold')
const scaffold = await agent(`You are the WORKSPACE-SCAFFOLD agent for the Ghostty Zig->Rust port, Phase 1.
${FACTS}

GOAL: Create a compiling Cargo workspace mirroring the reference src/ layout per the PORT_PLAN, a MODULE_STATUS manifest the gates will enforce, the libghostty-vt and ghostty output crates (stubs), and a WORKING reference FFI bridge with a passing smoke test. Everything must build and test green while still EMPTY of ported logic.

STEPS:
1. Create ${PORT}/Cargo.toml as a [workspace] (resolver="2") listing member crates under ${PORT}/crates/*. Create one lib crate per reference module, named with a 'rustty-' prefix, each an empty 'pub' lib that compiles (lib.rs with a //! doc comment naming the reference module, its src path, and its static test count; plus '#![allow(dead_code)]'):
   rustty-util (the leaf utilities split out of <root>: fastmem/quirks/tripwire/math/file_type/build_config/global), rustty-datastruct, rustty-simd, rustty-unicode, rustty-terminfo, rustty-os, rustty-crash, rustty-terminal, rustty-font, rustty-renderer, rustty-config, rustty-input, rustty-termio, rustty-apprt, rustty-cli, rustty-synthetic, rustty-inspector, rustty-benchmark, rustty-extra, rustty-lib (maps reference 'lib' module), rustty-stb.
   Wire each crate's intra-workspace dependencies to match the PORT_PLAN topo edges (e.g. rustty-terminal depends on rustty-datastruct/rustty-simd/rustty-unicode/rustty-util). Empty crates need no deps yet; add the obvious leaf deps so the dependency skeleton is real, but keep it ACYCLIC (the <root> split is what makes it a DAG — util is a pure leaf, the binary glue is the sink).
2. Create the OUTPUT crates:
   - ${PORT}/crates/libghostty-vt/ : crate-type ["cdylib","staticlib","rlib"], lib name 'ghostty_vt' so it emits libghostty-vt (or set [lib] name appropriately to produce libghostty-vt.*). It will export ghostty_* C-ABI fns in Phase 2; for now it is empty (no exports yet). Add a cbindgen.toml configured to generate a C header (header name ghostty/vt.h, prefix none, so symbols are bare ghostty_*).
   - ${PORT}/crates/ghostty/ : a [[bin]] named 'ghostty' (the binary identity must be 'ghostty', NOT rustty). main.rs prints nothing meaningful yet (stub) but compiles; it is the glue crate ported LAST.
3. Create the REFERENCE FFI BRIDGE crate ${PORT}/crates/rustty-refbridge/ : a dev/test bridge that links the reference's prebuilt libghostty-vt.a so the port can call reference behavior for differential testing.
   - build.rs: emit cargo:rustc-link-search=native=${REF}/zig-out/lib and cargo:rustc-link-lib=static=ghostty-vt, plus cargo:rustc-link-lib=c++ and any macOS frameworks needed (try none first; add as link errors demand). Use the 'bindgen' crate (build-dependency) to generate bindings from ${REF}/zig-out/include/ghostty/vt.h (clang arg -I ${REF}/zig-out/include). If bindgen/libclang is unavailable, FALL BACK to hand-written extern "C" decls for just the functions needed by the smoke test (ghostty_terminal_new, ghostty_terminal_vt_write, ghostty_formatter_terminal_new, ghostty_formatter_format_alloc, ghostty_free, ghostty_formatter_free, ghostty_terminal_free) with the correct structs from the headers.
   - Provide a safe-ish Rust wrapper fn that feeds VT bytes to a reference terminal and returns the PLAIN formatted output (mirrors ${PORTDIR}/harness/oracle.c).
   - Add a #[test] that feeds b"ab\\x1b[31mc\\r\\n" and asserts the reference returns "abc" (matching the verified oracle behavior). This test MUST pass (it proves the FFI bridge links and calls the reference correctly).
4. Create ${PORTDIR}/MODULE_STATUS.json: an object mapping each of the 21 reference modules (terminal, input, font, config, cli, datastruct, os, apprt, termio, <root>, renderer, simd, lib, synthetic, crash, benchmark, build, unicode, terminfo, extra, inspector) to {status:"not-started", reference_tests:N (the contract count above), ported_tests:0, crate:"rustty-..."}. The sum of reference_tests MUST equal 3234 (verify and report module_status_sum). Note in the file that gates enforce ported_tests==reference_tests ONLY for modules whose status is "in-progress"/"ported"; "not-started" modules are PENDING (not failures).
5. Verify: 'cd ${PORT} && cargo build --workspace' exits 0; 'cargo test --workspace' exits 0 (the only real test is the refbridge smoke test). Fix until green. Record exact outputs.

Return SCAFFOLD_SCHEMA with real values. Do NOT commit. Do NOT modify the reference repo.`, { label: 'scaffold', phase: 'Scaffold', schema: SCAFFOLD_SCHEMA })

phase('Gates')
const COMMON_GATE = `
${FACTS}

GATE SEMANTICS (uniform across all gates; from DEFINITION_OF_PARITY.md). Every gate script lives in ${PORTDIR}/gates/, is executable, reads ${PORTDIR}/MODULE_STATUS.json, and has THREE outcomes:
  - exit 0 + "PASS: ..."     : applicable and parity holds.
  - exit 0 + "PENDING: ..."  : nothing ported yet to check (e.g. module status not-started, or port artifact absent). HONEST not-yet, NOT a weakened pass. Must be visibly distinct in output.
  - exit 1 + "FAIL: ..."     : a real divergence detected.
  - exit 2 + "ERROR: ..."    : tooling/setup error (missing tool, etc.).
A gate must be written so that the MOMENT real ported code exists it flips from PENDING to a hard PASS/FAIL check — never a permanent skip. Do NOT weaken a gate to make CI green; PENDING is only for genuinely-absent port code.
The scaffold agent has created the Cargo workspace, MODULE_STATUS.json, the libghostty-vt + ghostty crates, and rustty-refbridge. Build on that. Write ONLY your assigned files. Do NOT commit. Do NOT modify the reference repo.
`
const gates = await parallel([
  () => agent(`You are GATES-A (tests + leaks).
${COMMON_GATE}
Write:
1. ${PORTDIR}/gates/gate_tests.sh: run 'cargo test --workspace' (must pass). Then for EACH module in MODULE_STATUS.json with status in {in-progress, ported}: count ported Rust tests for its crate (cargo test -- --list, or parse #[test]/#[cfg(test)] provenance comments '// PARITY: <reffile>:<line> <name>') and require ported_tests == reference_tests; any shortfall => FAIL with the gap. Modules with status not-started => PENDING. On the current empty workspace ALL are not-started => overall PENDING+the refbridge test passes => exit 0. Print a per-module table.
2. ${PORTDIR}/gates/gate_leaks.sh: wire leak detection. Prefer 'cargo +nightly miri test --workspace' if a nightly toolchain + miri are available; else use a tracking global allocator harness (document the mechanism and run 'cargo test' under it). On empty workspace => PASS (no leaks possible) or PENDING-with-reason if miri unavailable (document which). It must FAIL if a future test leaks.
Make both robustly detect their failure conditions (so the selftest can trip them). Return GATES_SCHEMA (scripts_written = the two paths).`, { label: 'gate:tests+leaks', phase: 'Gates', schema: GATES_SCHEMA }),

  () => agent(`You are GATES-B (differential VT + fuzz).
${COMMON_GATE}
Write:
1. ${PORTDIR}/gates/gate_diff_vt.sh: drive the existing ${PORTDIR}/harness/diff_vt.py. The reference oracle is ${PORTDIR}/harness/oracle (rebuild it if missing via the documented clang cmd). The PORT oracle is ${PORTDIR}/harness/oracle-port (built from the Rust libghostty-vt once it exports the C ABI; ABSENT now). If oracle-port absent => PENDING (port VT not implemented). If present => run diff_vt.py; exit 0 only if 0/N diverge, else FAIL. On empty workspace => PENDING.
2. ${PORTDIR}/gates/gate_fuzz.sh: differential fuzz SMOKE. The reference fuzz targets are ${REF}/test/fuzz-libghostty/src/{fuzz_parser,fuzz_osc,fuzz_stream}.zig with corpora under that tree's corpus/. For the smoke gate: if the Rust port exposes equivalent parser/osc/stream entry points (it does not yet) run a small number of corpus inputs through BOTH the reference (via rustty-refbridge / the oracle) and the port, comparing canonical output; else PENDING. Wire it to use the reference corpus files as the shared seed corpus. On empty workspace => PENDING. It must FAIL if a future port output diverges from reference on any fuzz input.
Both must be trippable by the selftest. Return GATES_SCHEMA.`, { label: 'gate:diff+fuzz', phase: 'Gates', schema: GATES_SCHEMA }),

  () => agent(`You are GATES-C (C-ABI via cbindgen + perf).
${COMMON_GATE}
Write:
1. ${PORTDIR}/gates/gate_abi.sh: regenerate the C header from the Rust libghostty-vt crate using cbindgen (/Users/gael/.cargo/bin/cbindgen with the crate's cbindgen.toml) into a temp file, then compare to the reference C ABI. The reference exports exactly 162 _ghostty_* symbols (nm -gU ${REF}/zig-out/lib/libghostty-vt.0.1.0.dylib | grep _ghostty_) and ships headers at ${REF}/zig-out/include/ghostty/vt.h + ghostty/vt/*.h. The gate must: (a) extract the set of ghostty_* function names cbindgen produced from the Rust crate; (b) extract the reference's 162 ghostty_* symbol set; (c) require the Rust set to be a parity match (same names; in Phase 2 also same signatures). On the empty workspace the Rust crate exports 0 ghostty_* => PENDING ("VT C ABI not implemented: 0/162 symbols"). It must FAIL the moment the Rust set diverges from the reference set (missing symbol, extra symbol, or a symbol renamed away from ghostty_*). Also assert the reference baseline of 162 symbols is still 162 (guards against env drift). 
2. ${PORTDIR}/gates/gate_perf.sh: read ${PORTDIR}/baseline/BASELINE.json. If no port benchmark binary exists yet => PENDING ("no port benches"). When present, run the port benches and FAIL if any dimension regresses past the DEFINITION thresholds (throughput >5% slower per-bench / >3% geomean, MaxRSS >5%, binary size hard ratchet, startup >5%). On empty workspace => PENDING. Document the thresholds in the script.
Both must be trippable. Return GATES_SCHEMA.`, { label: 'gate:abi+perf', phase: 'Gates', schema: GATES_SCHEMA }),

  () => agent(`You are GATES-D (comptime tables + packed layouts).
${COMMON_GATE}
Write:
1. ${PORTDIR}/gates/gate_tables.sh: comptime-generated tables must be byte-identical. Reference generated tables: src/unicode/props_table.zig + symbols_table.zig (uucode), src/terminal/parse_table.zig, lut.zig/cache_table.zig. The gate compares the SERIALIZED table byte image / sha256 of the port's generated tables vs the reference's. Until the port generates any table => PENDING. It must FAIL on any byte difference. Document which tables are tracked and how their byte image is obtained.
2. ${PORTDIR}/gates/gate_layout.sh: packed memory layouts must be byte-identical. Reference: terminal/page.zig Row=packed struct(u64), Cell=packed struct(u64); style.zig PackedStyle=packed struct(u128) sizeof==16, Flags=packed struct(u16), palette=packed struct(u24). The gate checks the port's corresponding Rust types have identical size_of (Row=8, Cell=8, PackedStyle=16, Flags=2, palette=3) and identical bit-field positions (via a fixture byte-image: construct a known value, serialize, diff against the reference's byte image). Until the port defines these types => PENDING. It must FAIL on any size/bit mismatch.
Both must be trippable by the selftest. Return GATES_SCHEMA.`, { label: 'gate:tables+layout', phase: 'Gates', schema: GATES_SCHEMA }),

  () => agent(`You are GATES-E (orchestrator + CI + injected-divergence selftest). Your scripts CALL the other gates by these EXACT paths: ${PORTDIR}/gates/{gate_tests,gate_leaks,gate_diff_vt,gate_fuzz,gate_abi,gate_perf,gate_tables,gate_layout}.sh
${COMMON_GATE}
Write:
1. ${PORTDIR}/gates/gate_all.sh: run all 8 gates in sequence, collect each one's status (PASS/PENDING/FAIL/ERROR), print a summary table, and exit non-zero if ANY gate FAILs or ERRORs (PENDING does not fail the suite). This is the parity master gate.
2. ${PORT}/ci.sh (repo-root CI entry): the full local CI pipeline = 'cargo build --workspace' + 'cargo test --workspace' + ${PORTDIR}/gates/gate_all.sh. Exit non-zero on any failure. Print a clear final GREEN/RED banner. This is what "CI" means for the port (local; a .github/workflows/ci.yml that simply runs ./ci.sh is a nice-to-have — add it).
3. ${PORTDIR}/gates/selftest.sh: PROVES each gate's failure-detection works. For EACH of the 8 gates, it must: (a) create a controlled, gate-specific INJECTED DIVERGENCE that the gate is designed to catch (e.g. for gate_abi: temporarily make MODULE_STATUS/abi think a required ghostty_* symbol is missing or add a forbidden symbol; for gate_tests: flip one module's status to "ported" with ported_tests<reference_tests so the count check must fail; for gate_layout: point it at a fixture with a wrong size; for gate_diff_vt: drop a port oracle that emits wrong bytes; for gate_tables: a table with a flipped byte; for gate_perf: a fake port bench result that regresses; for gate_leaks: a leaking test fixture; for gate_fuzz: a port output that differs on one corpus input), (b) assert the gate exits NON-ZERO, (c) REVERT the injection completely, (d) assert the gate returns to exit 0 (PASS/PENDING). It records per-gate {gate, injected_exit, reverted_exit}. At the end it must restore the workspace AND reference to the exact pre-selftest state (git status clean for the port; reference untouched). selftest.sh exits 0 only if ALL 8 gates went non-zero-on-inject AND zero-on-revert.
Return GATES_SCHEMA (scripts_written = gate_all.sh, ci.sh, selftest.sh).`, { label: 'gate:ci+selftest', phase: 'Gates', schema: GATES_SCHEMA }),
])

phase('Prove')
const prove = await agent(`You are the PROVE agent for Phase 1. Demonstrate the gate infrastructure works.
${FACTS}
The scaffold + 8 gate scripts + gate_all.sh + ci.sh + selftest.sh now exist (under ${PORTDIR}/gates/ and ${PORT}/ci.sh). 
STEPS:
1. Ensure the reference oracle exists (rebuild via the documented clang cmd if ${PORTDIR}/harness/oracle is missing).
2. Run 'cd ${PORT} && bash ci.sh' on the empty workspace. It MUST exit 0 (green): cargo build+test pass, all gates PASS or PENDING (none FAIL/ERROR). If it does not, diagnose and FIX the offending gate/scaffold so that the empty-workspace state is honestly green (do NOT weaken a gate — fix real bugs only; a gate that ERRORs due to a tooling bug must be repaired). Capture full output.
3. Run 'bash ${PORTDIR}/gates/selftest.sh'. It must show, for all 8 gates, injected->non-zero and reverted->zero, and exit 0. If any gate fails to detect its injected divergence, FIX that gate's detection logic (the gate is buggy, not the test) and re-run. Capture full output.
4. Confirm final cleanliness: 'cd ${PORT} && git status --short' shows only legitimately new/changed workspace files (no leftover injected divergence), and the reference repo 'git -C ${REF} status --porcelain' is empty (untouched).
5. Tee all output to ${P1}/phase1_proof.txt.
Return PROVE_SCHEMA with REAL measured exit codes and the per-gate selftest_results array. Do NOT commit.`, { label: 'prove', phase: 'Prove', schema: PROVE_SCHEMA })

phase('Critique')
const critics = await parallel([
  () => agent(`Adversarial CI critic, Phase 1. Verify CI is REAL, not hollow.
- Run 'cd ${PORT} && bash ci.sh' yourself; confirm exit 0 and that it ACTUALLY runs 'cargo build --workspace' and 'cargo test --workspace' (not stubbed) and invokes gate_all.sh which runs all 8 gates.
- Confirm the refbridge smoke test is among the tests actually executed (it exercises the reference FFI) — grep the cargo test output for it.
- Confirm no gate is a permanent no-op: each gate file must contain real detection logic, not 'exit 0' unconditionally. Read each gate script.
- Confirm PENDING states are honest (tied to genuinely-absent port code), not used to mask a check that should run.
Any hollowness => BLOCKER. Return CRITIC_SCHEMA. check='CI green & not hollow'.`, { label: 'critic:ci', phase: 'Critique', schema: CRITIC_SCHEMA }),

  () => agent(`Adversarial SELFTEST critic, Phase 1. Verify the injected-divergence proof is real.
- Run 'bash ${PORTDIR}/gates/selftest.sh' yourself and read ${P1}/phase1_proof.txt.
- For at least 3 of the 8 gates, MANUALLY reproduce the injection by hand, run the gate, and confirm it exits non-zero; then revert and confirm exit 0. Pick gate_abi, gate_tests, and gate_layout at minimum.
- Confirm selftest reverts state fully: after running it, 'git -C ${PORT} status --short' has no stray injected files and 'git -C ${REF} status --porcelain' is empty.
- Confirm the injections are MEANINGFUL (actually trigger the gate's real detection path), not trivially detected by an unrelated error.
Any gate that cannot be tripped, or any state not reverted => BLOCKER. Return CRITIC_SCHEMA. check='selftest exercises real failure paths + reverts'.`, { label: 'critic:selftest', phase: 'Critique', schema: CRITIC_SCHEMA }),

  () => agent(`Adversarial FFI-BRIDGE + ABI critic, Phase 1.
- Verify rustty-refbridge actually links the reference: run its smoke test ('cargo test -p rustty-refbridge') and confirm it passes and that it calls the reference C ABI (inspect the build.rs link directives and the bindings; confirm it feeds b"ab\\x1b[31mc\\r\\n" and gets "abc").
- Verify cbindgen is real: run the libghostty-vt crate's cbindgen and confirm it produces a header (even if empty of ghostty_* now). Confirm gate_abi reads the genuine 162-symbol reference set (run: nm -gU ${REF}/zig-out/lib/libghostty-vt.0.1.0.dylib | grep -c _ghostty_  -> expect 162) and that the gate would FAIL if the Rust set diverged.
- Confirm the libghostty-vt crate is configured to emit a lib named 'libghostty-vt' with bare ghostty_* symbols (NOT rustty_*), preserving identity.
Any break => BLOCKER. Return CRITIC_SCHEMA. check='refbridge links reference + cbindgen/ABI gate real + identity preserved'.`, { label: 'critic:ffi+abi', phase: 'Critique', schema: CRITIC_SCHEMA }),

  () => agent(`Adversarial MODULE_STATUS + workspace critic, Phase 1.
- Read ${PORTDIR}/MODULE_STATUS.json: confirm all 21 reference modules present, reference_tests sum == 3234 (re-add them), every entry status="not-started" ported_tests=0 with a crate assignment.
- Confirm the Cargo workspace builds ('cargo build --workspace' exit 0) and the intra-workspace dependency graph is ACYCLIC (cargo would error on cycles; also confirm rustty-util is a pure leaf and the ghostty binary crate is the sink).
- Confirm the binary crate produces a binary named exactly 'ghostty' and the VT lib crate is named to emit 'libghostty-vt' (check Cargo.toml [[bin]] name and [lib] name/crate-type). Identity must be ghostty, not rustty, for outputs.
- PROVE the test-parity gate has teeth: temporarily flip ONE module (e.g. unicode) to status="ported" with ported_tests=0 (reference_tests=5), run gate_tests.sh, confirm it FAILs (exit non-zero) because 0 != 5, then revert MODULE_STATUS.json exactly. Report the observed exit codes.
Any mismatch (sum != 3234, cyclic graph, wrong output names, gate has no teeth) => BLOCKER. Return CRITIC_SCHEMA. check='MODULE_STATUS sums to 3234 + workspace acyclic + identity names + test gate has teeth'.`, { label: 'critic:modstatus', phase: 'Critique', schema: CRITIC_SCHEMA }),
])

const blockers = critics.filter(Boolean).filter(c => c.blocker || !c.passed)
return {
  scaffold,
  gates_written: gates.filter(Boolean).flatMap(g => g.scripts_written || []),
  prove,
  critics,
  blockers,
  gate_green: blockers.length === 0
    && !!scaffold && scaffold.workspace_builds === true && scaffold.workspace_tests_pass === true
    && scaffold.refbridge_smoke_pass === true && scaffold.module_status_sum === 3234
    && !!prove && prove.ci_exit_code === 0 && prove.ci_green_on_empty === true
    && prove.all_gates_detect_divergence === true && prove.state_reverted_clean === true,
}
