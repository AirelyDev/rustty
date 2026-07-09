export const meta = {
  name: 'benchmark-harness',
  description: 'Sub-project #3 — port ghostty src/benchmark/** (Benchmark runner + 7 benchmarks + cli/CApi/options/main, 1441 LOC, ~9 test{}) into rustty-bench: design -> port -> verify(+Miri)',
  phases: [
    { title: 'Design', detail: 'brief faithful-port design + platform-gating + STOP-guard' },
    { title: 'Port', detail: 'port all benchmark files + tests into rustty-bench' },
    { title: 'Verify', detail: 'gates + R1 fidelity + Miri (pure logic)' },
  ],
}

const R = '/Users/gael/projects/rustty'
const G = '/Users/gael/projects/ghostty'
const DESIGN = `${R}/.port/phase-2/BENCHMARK_DESIGN.md`

const rules = `
Full-fidelity Zig->Rust port (rustty). Reference (read-only): ghostty at ${G}. Target: ${R} (crate crates/rustty-bench; currently a scaffold). All benchmarked subsystems are already ported: rustty-vt (terminal/osc/UTF8Decoder/Screen/stream/Parser), rustty-unicode (symbols_table + grapheme/width), rustty-simd, rustty-global, rustty-build-config, rustty-args (cli). rustty-bench may dep all of these (it is a top-ish consumer; check_acyclic must stay green).
R1: EVERY line/comment/test{} ported assertion-for-assertion; Zig byte-slices -> &[u8]/Vec<u8>/CString, NEVER lossy String. R4 acyclic. R6: rustty-bench logic is PURE Rust -> Miri primary (the CApi C-ABI export is extern "C", Miri-excluded/ASan). Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py + Miri.
SCOPE (ghostty src/benchmark/): Benchmark.zig (178,1 — the runner: run/setup/warmup + std.time timing + the terminal.rs rusage/perf-counter path), options.zig (20), main.zig (13,1), cli.zig (100 — the bench CLI dispatch), CApi.zig (34 — ghostty_benchmark_cli extern export), and the 7 benchmarks: CodepointWidth (207,1), GraphemeBreak (152,1), IsSymbol (151,1), OscParser (118,1), ScreenClone (193,0), TerminalParser (114,1), TerminalStream (161,1). ~9 test{} — the Phase-0 count contract.
PLATFORM: the Benchmark runner may use Linux perf_event / getrusage; gate non-portable bits #[cfg(target_os=...)] faithfully (macOS uses the std.time.Timer path), preserving the verbatim reference for the gated-out arm. The C-ABI export stays a thin extern "C" (name it rustty_benchmark_cli per R2, with a ghostty_benchmark_cli alias deferred to sub-project #2's alias layer — note in DEFERRALS).`

const D_SCHEMA = { type:'object', additionalProperties:false, required:['doc_path','disposition','summary','key_decisions','risks'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason>'"}, summary:{type:'string'}, key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN} — a concise faithful-port design for rustty-bench. Read ALL of ${G}/src/benchmark/*.zig first + confirm the current crates/rustty-bench scaffold + Cargo.toml deps. Map each Zig file -> a rustty module; pin the timing/perf-counter model (std.time.Timer -> std::time::Instant; any getrusage/perf_event -> #[cfg]-gated with verbatim preserved), the Benchmark runner's fn-ptr vtable (setup/run -> a trait or raw-ptr trampoline, Miri-clean), each benchmark's exact subsystem calls (against the ported rustty-vt/unicode/simd APIs — cite real signatures), and the ~9 test{}. Decide the C-ABI export name (rustty_benchmark_cli). If a genuine blocker (a benchmarked API not actually ported), disposition 'blocked' + STOP. Else 'ready'. Return the object.`,
  { label:'design:bench', phase:'Design', agentType:'general-purpose', schema:D_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`BENCH DESIGN BLOCKED: ${design ? design.disposition : 'design failed'}`)
  return { blocked:true, design }
}

phase('Port')
const P_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','tests_ported','miri_clean','notes'], properties:{
  disposition:{type:'string'}, files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'}, tests_ported:{type:'number'}, miri_clean:{type:'boolean'}, notes:{type:'string'} } }
const port = await agent(
  `${rules}\n\nPORT PHASE. Implement per ${DESIGN} — if blocked, STOP. Port EVERY benchmark file into crates/rustty-bench/src/ (module per Zig file), preserving every comment (R1) and porting all ~9 test{} assertion-for-assertion. Wire the module tree + set 'PORT STATUS: done'. Platform-gate the perf-counter arm faithfully. Build \`cargo build -p rustty-bench\`, run \`cargo test -p rustty-bench\`, and \`cargo +nightly miri test -p rustty-bench\` (pure logic -> Miri-clean; if a benchmark's run is too slow under Miri, mark it #[cfg_attr(miri, ignore)] with a note but keep the correctness tests Miri-covered). Report files_changed, build_ok, tests_ported, miri_clean, notes (incl. the timing/vtable model + any #[cfg] gating). Do NOT git commit.`,
  { label:'port:bench', phase:'Port', agentType:'general-purpose', schema:P_SCHEMA })

if (!port || (port.disposition||'').toLowerCase().startsWith('stopped')) {
  log(`BENCH PORT STOPPED`)
  return { design, port, verify:null }
}

phase('Verify')
const V_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES+MIRI). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-bench; cargo +nightly miri test -p rustty-bench; cargo clippy -p rustty-bench --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green.`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 FIDELITY). Diff each ported rustty-bench module vs its ghostty src/benchmark/*.zig: (1) every comment verbatim? (2) all ~9 test{} ported assertion-for-assertion? (3) each benchmark's subsystem calls faithful (right ported API, right args, right loop counts/iterations)? (4) the timing + any perf-counter arm faithfully #[cfg]-gated with verbatim preserved? (5) the runner vtable Miri-clean + faithful to the Zig fn-ptr? (6) C-ABI export named rustty_benchmark_cli (ghostty_ alias deferred + noted)? (7) no lossy String? Report findings; verdict PASS only if R1 intact.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
])

return { design, port, verify }
