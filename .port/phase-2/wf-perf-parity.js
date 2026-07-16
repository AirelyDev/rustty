export const meta = {
  name: 'perf-parity',
  description:
    'Make rustty-bench + rustty-synthetic runnable binaries (faithful port of main_bench.zig / main_gen.zig + the synthetic CLI glue), validate byte-parity of the Rust generator vs ghostty-gen, then run the Rust-vs-Zig benchmark head-to-head on identical corpora.',
  phases: [
    { title: 'Design', detail: 'synthetic-CLI parity design + measurement protocol' },
    { title: 'Implement', detail: 'bench bin || synthetic parity (disjoint crates)' },
    { title: 'Verify', detail: 'gates || R1 fidelity || generator byte-equivalence' },
    { title: 'Measure', detail: 'serial Rust-vs-Zig bench on identical corpora' },
  ],
}

const R = '/Users/gael/projects/rustty'
const G = '/Users/gael/projects/ghostty'
const COR = `${R}/.port/phase-0/baseline/corpora`
const ZBENCH = `${R}/.port/phase-0/baseline/bench-releasefast/bin/ghostty-bench`
const ZGEN = `${R}/.port/phase-0/baseline/bench-releasefast/bin/ghostty-gen`
const HARNESS = `${R}/.port/phase-0/baseline/run_benchmarks.py`
const SYNTH_DOC = `${R}/.port/phase-2/SYNTHETIC_CLI_PARITY.md`
const MEAS_DOC = `${R}/.port/phase-2/PERF_COMPARISON_PLAN.md`
const RESULT_DOC = `${R}/.port/phase-2/PERF_COMPARISON.md`

const RULES = `Full-fidelity Zig->Rust port (rustty). Reference oracle (READ-ONLY): ghostty at ${G} (Zig 0.15.2). Target repo: ${R}. Governed by .port/PORT_RULES.md: R1 (EVERY line/comment/test{} ported assertion-for-assertion; Zig byte-slices -> &[u8]/Vec<u8>, NEVER lossy String), R2 (identity rename ghostty->rustty), R3 (no ghostty Cargo dep), R4 (acyclic crate graph), R5 (gates invariant). Do NOT git commit. Edit files in place in ${R}.

CONTEXT (already established, do not re-derive):
- The Zig reference bench+gen binaries are BUILT and working at ${ZBENCH} and ${ZGEN}. Corpora are regenerated at ${COR}/ (utf8.bin seed=42 64MiB, ascii.bin 64MiB, osc-parser.bin 64MiB, ascii-small.bin 256KiB).
- rustty-bench (crates/rustty-bench): the benchmark LOGIC is fully ported and real. crates/rustty-bench/src/cli.rs already has a working \`pub fn main() -> Result<(), cli::Error>\` (detect_args + main_action + main_action_impl). The crate is a LIBRARY with NO [[bin]] target. The Zig reference src/main_bench.zig is exactly 3 lines: \`const benchmark = @import("benchmark/main.zig"); pub const main = benchmark.cli.main;\`.
- rustty-synthetic (crates/rustty-synthetic): the CORE generators are fully ported and real (src/{utf8.rs,osc.rs,bytes.rs,rand.rs,generator.rs}, and per-action src/cli/{ascii.rs,osc.rs,utf8.rs} each with real Options/create/run/destroy). The ONLY stubs are 4 \`unimplemented!()\` in src/cli.rs: \`main()\` and \`main_action_impl_{ascii,osc,utf8}\`. The crate does NOT yet depend on rustty-args and its Options structs have NO \`rustty_args::args::Target\` impl.
- rustty-args (crate #15, earlier than both #28 bench / #29 synthetic -> depending on it is ACYCLIC) provides: \`args::parse(&mut opts, &mut iter)\`, \`args::args_iterator()\`, \`args::slice_iterator(&[&[u8]])\`, and field helpers \`parse_field_string\`, \`parse_field_bool\`, \`parse_field_int::<T>\`, \`parse_field_float::<T>\`, \`parse_field_enum(value, from_name)\`. The \`Target\` trait's method is \`fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), args::ParseError>\`. EXEMPLAR to copy: crates/rustty-bench/src/codepoint_width.rs (its \`impl rustty_args::args::Target for Options\`).
- Per-action Rust signatures (NOTE: no alloc arg, unlike the Zig create(alloc,opts)): ascii \`Ascii::create(Options)->Result<Ascii,WriteFailed>\` (Options is EMPTY \`{}\`), osc \`Osc::create(Options)->Result<Osc,WriteFailed>\`, utf8 \`Utf8::create(Options)->Result<Utf8,InvalidValue>\`. All: \`run<W: Writer<Error=WriteFailed>>(&self, writer:&mut W, rand: crate::rand::Rng)->Result<(),WriteFailed>\`, \`destroy(self)\`. crates/rustty-synthetic/src/io.rs re-exports \`rustty_os::string_encoding::{FixedWriter, WriteFailed, Writer}\` and adds \`WriterExt\`.
- Reference glue to port faithfully: ${G}/src/synthetic/cli.zig \`mainActionImpl\` (parse opts; seed = @truncate(nanoTimestamp) -> DefaultPrng::init -> rand; stdout via a 2048-byte-buffered writer; create; run(writer, rand); flush) and \`main\` (cli.action.detectArgs -> mainAction). ${G}/src/main_gen.zig is 3 lines: \`const synthetic = @import("synthetic/main.zig"); pub const main = synthetic.cli.main;\`. The bench crate ALREADY solved the identical \`detect_args\`/\`main\` local-replica problem in crates/rustty-bench/src/cli.rs (detectArgs lives in rustty-cli, a cycle; replicate locally over std::env::args_os) — MIRROR that exact approach for rustty-synthetic.`

// ---------------- Phase 1: Design ----------------
phase('Design')

const D_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['doc_path', 'disposition', 'summary', 'key_points'],
  properties: {
    doc_path: { type: 'string' },
    disposition: { type: 'string', description: "'ready' or 'blocked: <reason>'" },
    summary: { type: 'string' },
    key_points: { type: 'array', items: { type: 'string' } },
  },
}

const designs = await parallel([
  () => agent(
    `${RULES}\n\nDESIGN (synthetic CLI parity). Read the reference ${G}/src/synthetic/cli.zig, ${G}/src/synthetic/cli/{Ascii,Osc,Utf8}.zig, ${G}/src/main_gen.zig, and the current crates/rustty-synthetic/src/{cli.rs,io.rs,rand.rs,cli/{ascii,osc,utf8}.rs} + Cargo.toml. Write ${SYNTH_DOC}: a concise faithful-port design covering (1) the EXACT reference-Options-field -> CLI-key map for each of ascii/osc/utf8 (kebab keys, Rust type, which parse_field_* helper, default), so \`Target::parse_into_field\` can be implemented per action; ascii Options is empty so its Target rejects all keys. (2) The stdout Writer: a buffered writer newtype impl'ing \`rustty_os::string_encoding::Writer<Error=WriteFailed>\` over std::io::stdout (2048-byte buffer, flush at end; a write error / BrokenPipe just ends generation Ok, matching each action's run loop which returns on WriteFailed). Cite the exact Writer trait method(s) the impl must provide (read rustty_os::string_encoding). (3) The time seed: @truncate(nanoTimestamp) -> DefaultPrng::init(seed).random(); use std::time::SystemTime UNIX_EPOCH nanos truncated to u64 (document the faithful mapping). (4) The main_action_impl glue per action (parse opts -> create -> run(writer,rand) -> flush; map the differing InvalidValue/WriteFailed create errors). (5) The main()/detect_args local replica mirroring crates/rustty-bench/src/cli.rs. (6) The [[bin]] wrapper: crates/rustty-synthetic/src/main_gen.rs (faithful 3-line-equivalent calling crate::cli::main()), [[bin]] name = "rustty-synthetic" path = "src/main_gen.rs" so \`cargo run -p rustty-synthetic\` works. (7) R1: preserve every reference comment; keep the existing ref_all_actions test. If a genuine blocker, disposition 'blocked' + STOP; else 'ready'. Return the object.`,
    { label: 'design:synthetic', phase: 'Design', agentType: 'general-purpose', schema: D_SCHEMA }),

  () => agent(
    `${RULES}\n\nDESIGN (measurement protocol). Read ${HARNESS} (the existing single-process monotonic timing harness), ${R}/.port/phase-0/baseline/METHODOLOGY.md, ${R}/crates/rustty-bench/AGENTS.md, and crates/rustty-bench/src/cli.rs (the argv/action/--data/--mode CLI shape). Write ${MEAS_DOC}: the exact protocol to run the Rust rustty-bench binary head-to-head vs the Zig ${ZBENCH} on the SAME corpora at ${COR}. Cover: (a) that run_benchmarks.py is BENCH-generic (\`argv = [BENCH] + args + ["--data="+corpus]\`, actions use the leading "+"), so it can drive BOTH binaries unchanged — the Rust bin path is target/release/rustty-bench after \`cargo build -p rustty-bench --release\`. (b) The 14 bench configs it runs (list them). (c) N=7 timed + 1 warmup, median headline, stdout/stderr -> DEVNULL, never run benches in parallel, isolate (no other heavy work concurrently). (d) The comparison table schema: per config -> zig_median_ms, rust_median_ms, delta_pct (rust vs zig, + = Rust slower), throughput where applicable, clones/s for screen-clone. (e) That the Rust GENERATOR byte-equivalence is validated SEPARATELY (not used as bench input) so a generator diff cannot confound the bench numbers. (f) Note the process-spawn/setup floor (~5-9ms, visible as codepoint-width.noop) and that work = median - noop-floor roughly. disposition 'ready'. Return the object.`,
    { label: 'design:measure', phase: 'Design', agentType: 'general-purpose', schema: D_SCHEMA }),
])

const synthDesign = designs[0]
if (!synthDesign || (synthDesign.disposition || '').toLowerCase().startsWith('blocked')) {
  log(`SYNTHETIC DESIGN BLOCKED: ${synthDesign ? synthDesign.disposition : 'design agent failed'}`)
  return { blocked: true, designs }
}
log(`Design done. synthetic=${synthDesign.disposition} measure=${designs[1] ? designs[1].disposition : 'n/a'}`)

// ---------------- Phase 2: Implement (disjoint crates -> safe parallel) ----------------
phase('Implement')

const IMPL_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['crate', 'build_ok', 'bin_name', 'files_changed', 'smoke_ok', 'smoke_output', 'notes'],
  properties: {
    crate: { type: 'string' },
    build_ok: { type: 'boolean' },
    bin_name: { type: 'string' },
    files_changed: { type: 'array', items: { type: 'string' } },
    smoke_ok: { type: 'boolean' },
    smoke_output: { type: 'string' },
    notes: { type: 'string' },
  },
}

const impls = await parallel([
  () => agent(
    `${RULES}\n\nIMPLEMENT (rustty-bench binary). Faithful port of ${G}/src/main_bench.zig ("const benchmark=@import(\\"benchmark/main.zig\\"); pub const main = benchmark.cli.main;"). Create crates/rustty-bench/src/main_bench.rs: a thin bin entry that calls \`rustty_bench::cli::main()\`. Because \`cli::Error\` is large, do NOT return it from fn main (clippy result_large_err); instead: \`fn main() -> std::process::ExitCode { match rustty_bench::cli::main() { Ok(())=>ExitCode::SUCCESS, Err(e)=>{ eprintln!("error: {e:?}"); ExitCode::FAILURE } } }\`. Put the reference 2 lines as a header comment (R1). Add to crates/rustty-bench/Cargo.toml: \`[[bin]]\\nname = "rustty-bench"\\npath = "src/main_bench.rs"\` (a lib+bin of the same name is allowed). Then: \`cargo build -p rustty-bench --release\` (must be clean, no warnings — workspace lints deny warnings), \`cargo clippy -p rustty-bench --all-targets -- -D warnings\`. SMOKE TEST: \`target/release/rustty-bench +codepoint-width --mode=noop --data=${COR}/utf8.bin\` (should exit 0, print nothing) and a no-arg run (should error NoAction to stderr, exit non-zero). Report build_ok, bin_name, files_changed, smoke_ok, smoke_output (the exact commands + exit codes), notes. Do NOT commit.`,
    { label: 'impl:bench-bin', phase: 'Implement', agentType: 'general-purpose', schema: IMPL_SCHEMA }),

  () => agent(
    `${RULES}\n\nIMPLEMENT (rustty-synthetic parity). Follow ${SYNTH_DOC} exactly. Steps: (1) crates/rustty-synthetic/Cargo.toml: add \`rustty-args = { workspace = true }\` to [dependencies] (acyclic: #15 < #29). (2) Add the buffered stdout Writer per the design (impl rustty_os::string_encoding::Writer<Error=WriteFailed> over std::io::stdout, 2048 buffer, flush; write-error ends Ok). (3) In crates/rustty-synthetic/src/cli/{ascii,osc,utf8}.rs add \`impl rustty_args::args::Target for Options\` mapping the design's CLI keys (ascii: empty -> all keys InvalidField; osc/utf8: real fields via parse_field_*). Preserve all comments. (4) In crates/rustty-synthetic/src/cli.rs replace the 4 unimplemented!() : port main_action_impl_{ascii,osc,utf8} (parse opts via rustty_args over args_iterator (Cli) / slice_iterator (String) exactly like crates/rustty-bench/src/cli.rs; seed DefaultPrng from truncated SystemTime nanos; create impl; run into the stdout writer; flush; map InvalidValue/WriteFailed) and port main()+detect_args as a local replica mirroring rustty-bench's cli.rs. Keep every reference comment verbatim (R1) and the ref_all_actions test. (5) Create crates/rustty-synthetic/src/main_gen.rs (faithful port of main_gen.zig; ExitCode pattern like the bench bin) + Cargo.toml \`[[bin]]\\nname = "rustty-synthetic"\\npath = "src/main_gen.rs"\`. Then build+lint: \`cargo build -p rustty-synthetic --release\` clean, \`cargo clippy -p rustty-synthetic --all-targets -- -D warnings\`, \`cargo test -p rustty-synthetic\`. SMOKE TEST (set -o pipefail): \`target/release/rustty-synthetic +ascii 2>/dev/null | head -c 64 | wc -c\` (=64), \`target/release/rustty-synthetic +utf8 --seed=42 2>/dev/null | head -c 64 | xxd | head -2\`, \`target/release/rustty-synthetic +osc --style=parser --p-valid=0.5 2>/dev/null | head -c 64 | wc -c\`. Report build_ok, bin_name, files_changed, smoke_ok, smoke_output (commands + outputs + exit codes), notes (the field maps + writer + seed model + any deviation). Do NOT commit.`,
    { label: 'impl:synthetic', phase: 'Implement', agentType: 'general-purpose', schema: IMPL_SCHEMA }),
])

const benchImpl = impls[0], synthImpl = impls[1]
log(`Implement done. bench.build=${benchImpl && benchImpl.build_ok} synthetic.build=${synthImpl && synthImpl.build_ok}`)
if (!benchImpl || !benchImpl.build_ok) {
  log('BENCH BIN BUILD FAILED — cannot measure; stopping after reporting.')
  return { designs, impls, verify: null, measure: null }
}

// ---------------- Phase 3: Verify (independent dimensions) ----------------
phase('Verify')

const V_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['dimension', 'verdict', 'findings'],
  properties: {
    dimension: { type: 'string' },
    verdict: { enum: ['PASS', 'FAIL'] },
    findings: { type: 'array', items: { type: 'string' } },
  },
}

const verify = await parallel([
  () => agent(
    `${RULES}\n\nVERIFY (gates). Re-run and report each, GREEN required: \`cargo build --workspace --release\`; \`cargo clippy --workspace --all-targets -- -D warnings\`; \`cargo fmt --check\`; \`bash .port/tools/gates/check_acyclic.sh\`; \`python3 .port/tools/gates/check_test_counts.py\`; \`python3 .port/tools/gates/check_comment_coverage.py\`. (These gate scripts may be under .port/tools/ — locate them if the path differs.) verdict PASS only if ALL green; list any failure verbatim in findings.`,
    { label: 'verify:gates', phase: 'Verify', agentType: 'general-purpose', schema: V_SCHEMA }),

  () => agent(
    `${RULES}\n\nVERIFY (R1 fidelity). Diff the newly added/edited files vs their Zig references: crates/rustty-bench/src/main_bench.rs vs ${G}/src/main_bench.zig; crates/rustty-synthetic/src/main_gen.rs vs ${G}/src/main_gen.zig; crates/rustty-synthetic/src/cli.rs main()/main_action_impl_* vs ${G}/src/synthetic/cli.zig; the 3 Target impls vs the reference Options field sets in ${G}/src/synthetic/cli/{Ascii,Osc,Utf8}.zig. Confirm: every reference comment preserved; the seed/stdout/flush semantics faithful; CLI keys exactly match the Zig field names (kebab); no lossy String on byte data; the ref_all_actions test intact. verdict PASS only if R1 intact; findings list any divergence.`,
    { label: 'verify:fidelity', phase: 'Verify', agentType: 'Explore', schema: V_SCHEMA }),

  () => agent(
    `${RULES}\n\nVERIFY (generator byte-equivalence). The Rust generator must faithfully reproduce ghostty-gen for the SEEDED stream. Run (set -o pipefail): \`target/release/rustty-synthetic +utf8 --seed=42 2>/dev/null | head -c 1048576 > /tmp/rgen_utf8.bin\` and \`${ZGEN} +utf8 --seed=42 2>/dev/null | head -c 1048576 > /tmp/zgen_utf8.bin\`, then \`cmp /tmp/rgen_utf8.bin /tmp/zgen_utf8.bin\`. Report BYTE-IDENTICAL or the first differing offset (\`cmp\` output) — a divergence is a real RNG/generator fidelity finding, report it precisely (do NOT fail the whole workflow over it; it does not affect the bench comparison which uses ghostty-gen corpora). Also sanity-check ascii and osc produce valid non-empty 64-byte streams (\`+ascii\` printable ASCII; \`+osc --style=parser --p-valid=0.5\` non-empty). verdict PASS if utf8 is byte-identical AND ascii/osc are valid; else FAIL with the exact cmp/first-diff details in findings (this verdict is informational — the measure phase proceeds regardless).`,
    { label: 'verify:byte-equiv', phase: 'Verify', agentType: 'general-purpose', schema: V_SCHEMA }),
])
log(`Verify done. ${verify.filter(Boolean).map(v => `${v.dimension}=${v.verdict}`).join(' ')}`)

// ---------------- Phase 4: Measure (SERIAL, alone — benches never run in parallel) ----------------
phase('Measure')

const M_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['doc_path', 'summary', 'table_md', 'rows'],
  properties: {
    doc_path: { type: 'string' },
    summary: { type: 'string' },
    table_md: { type: 'string' },
    rows: {
      type: 'array',
      items: {
        type: 'object', additionalProperties: false,
        required: ['id', 'zig_median_ms', 'rust_median_ms', 'delta_pct'],
        properties: {
          id: { type: 'string' },
          zig_median_ms: { type: 'number' },
          rust_median_ms: { type: 'number' },
          delta_pct: { type: 'number' },
        },
      },
    },
  },
}

const measure = await agent(
  `${RULES}\n\nMEASURE (Rust vs Zig, SERIAL — you are the only agent running; do NOT background anything heavy; never run two benches at once). Follow ${MEAS_DOC}. The Rust bench binary is target/release/rustty-bench (built in Implement); the Zig one is ${ZBENCH}. Corpora at ${COR}. Use the existing harness ${HARNESS} which is BENCH-generic: run it TWICE with N=7 — \`python3 ${HARNESS} ${ZBENCH} /tmp/perf_zig.jsonl 7\` and \`python3 ${HARNESS} target/release/rustty-bench /tmp/perf_rust.jsonl 7\`. (If rustty-bench rejects an arg the harness passes, note it and, if needed, run those configs directly with the same flags.) Then join the two JSONL by id and compute delta_pct = (rust_median - zig_median)/zig_median*100 (+ = Rust slower). Write ${RESULT_DOC}: machine/date, methodology pointer, and a per-config table (zig_median_ms, rust_median_ms, delta_pct, throughput/clones-s where relevant), plus an honest summary (which benches Rust wins/loses, the noop process-spawn floor caveat, and that corpora are identical ghostty-gen files). Return doc_path, summary, table_md (the markdown table), and rows[]. Report numbers faithfully — if a Rust bench is slower, say so plainly.`,
  { label: 'measure', phase: 'Measure', agentType: 'general-purpose', schema: M_SCHEMA })

log(`Measure done -> ${measure ? measure.doc_path : 'FAILED'}`)
return { designs, impls, verify, measure }
