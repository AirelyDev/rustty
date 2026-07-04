export const meta = {
  name: 'phase0-oracle-baseline',
  description: 'Ghostty Zig->Rust port Phase 0: oracle, differential harness, baseline, port-plan, parity-definition, adversarial critics',
  phases: [
    { title: 'Analyze', detail: 'module graph + PORT_PLAN, test reconciliation, DEFINITION_OF_PARITY, build VT diff harness' },
    { title: 'Baseline', detail: 'SOLO heavy measurement: bench tools + throughput + startup + sizes -> BASELINE.json' },
    { title: 'Critique', detail: 'adversarial skeptics: count==3234, harness truly red, topo valid, reconciliation complete' },
  ],
}

const REF = '/Users/gael/projects/ghostty'
const PORT = '/Users/gael/projects/rustty'
const PORTDIR = PORT + '/.port'
const P0 = PORTDIR + '/phase-0'

const FACTS = `
GROUND-TRUTH FACTS (verified by the orchestrator; do not re-derive unless told):
- Reference repo (READ-ONLY, never modify its source/examples): ${REF} @ commit 4749c4e93731067049bfbf2e4572061cef2bdd17. Zig 0.15.2.
- Port repo (git, currently an EMPTY Rust workspace): ${PORT}. Write all artifacts under ${PORTDIR}.
- STATIC TEST CONTRACT = 3234 test blocks total: 3113 named (test "x" {), 76 anonymous (test {), 45 declref (test ident {). Inventory already at ${P0}/PARITY_LEDGER.json (array 'tests' of {file,line,kind,name,body_lines,assert_count,assert_kinds}). Per-module counts are in its 'by_module'.
- macOS 'zig build test' (step ghostty-test) RUNS 2857 tests: 2842 passed + 15 skipped, exit 0, MaxRSS 232M, ~1 min. Log: /tmp/rustty_ref_test_run.log.
- 'zig build test-lib-vt' RUNS 4493 across TWO configs (2384 + 2077, +32 skipped), exit 0, MaxRSS 94M. Log: /tmp/rustty_ref_libvt_test.log. Same source blocks compile in multiple configs, so runtime counts are config-multiplied and EXCEED the static 3234.
- Prebuilt oracle: ${REF}/zig-out/lib/libghostty-vt.a (12065920 B) and libghostty-vt.0.1.0.dylib (5742200 B). Generated C-ABI headers at ${REF}/zig-out/include/ (ghostty/vt.h + ghostty/vt/*.h). Source headers at ${REF}/include/.
- Binary sizes (universal x86_64+arm64): ghostty macho 140361712 B; app bundle 162746368 B; libghostty-vt.dylib 5742200 B; .a 12065920 B.
- Machine: Apple M4 Pro (Mac16,7), arm64, 14 cores, 48 GB (51539607552 B), macOS 26.5.1 (25F80).
- Bench build: cd ${REF} && zig build -Demit-bench -Doptimize=ReleaseFast -Demit-macos-app=false  -> ${REF}/zig-out/bin/ghostty-bench and ghostty-gen. RULE: never run multiple benchmarks in parallel.
- Top-level src modules: terminal(2178 blocks), input(245), font(178), config(175), cli(108), datastruct(76), os(57), apprt(46), termio(33), renderer(25), simd(23), lib(13), synthetic(12), crash(9), build(7), unicode(5), terminfo(4), extra(2), benchmark(8), inspector(1), and 29 root-level src/*.zig.
- DO NOT git commit; the orchestrator commits. DO NOT modify the reference repo. Write artifacts only under ${PORTDIR}.
`

const GRAPH_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['topo_order', 'cycles', 'edge_count', 'wrote_port_plan', 'wrote_graph_json', 'notes'],
  properties: {
    topo_order: { type: 'array', items: { type: 'string' }, description: 'modules leaves-first' },
    cycles: { type: 'array', items: { type: 'array', items: { type: 'string' } } },
    edge_count: { type: 'integer' },
    wrote_port_plan: { type: 'boolean' },
    wrote_graph_json: { type: 'boolean' },
    notes: { type: 'string' },
  },
}

const RECON_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['static_total', 'accounts_for_all', 'buckets', 'wrote_md', 'wrote_json', 'notes'],
  properties: {
    static_total: { type: 'integer' },
    accounts_for_all: { type: 'boolean', description: 'true iff every one of the 3234 static blocks is assigned to exactly one accounting bucket and the sum == 3234' },
    buckets: { type: 'object', description: 'bucket name -> count; sum must equal static_total', additionalProperties: { type: 'integer' } },
    wrote_md: { type: 'boolean' },
    wrote_json: { type: 'boolean' },
    notes: { type: 'string' },
  },
}

const DEF_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote', 'global_gate_count', 'per_module_gate_count', 'notes'],
  properties: {
    wrote: { type: 'boolean' },
    global_gate_count: { type: 'integer' },
    per_module_gate_count: { type: 'integer' },
    notes: { type: 'string' },
  },
}

const HARNESS_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['oracle_built', 'oracle_path', 'corpus_count', 'total_inputs', 'divergence_count', 'harness_exit_code', 'self_consistency_zero_divergence', 'proof_path', 'notes'],
  properties: {
    oracle_built: { type: 'boolean' },
    oracle_path: { type: 'string' },
    corpus_count: { type: 'integer' },
    total_inputs: { type: 'integer' },
    divergence_count: { type: 'integer', description: 'how many corpus inputs diverged port-vs-reference (must be ~all since port is empty)' },
    harness_exit_code: { type: 'integer', description: 'must be NON-ZERO (divergence detected)' },
    self_consistency_zero_divergence: { type: 'boolean', description: 'true iff feeding the reference oracle output as BOTH sides yields 0 divergence (proves the comparator is not always-red)' },
    proof_path: { type: 'string' },
    notes: { type: 'string' },
  },
}

const BASELINE_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['wrote_baseline_json', 'benchmarks', 'startup_recorded', 'sizes_recorded', 'methodology_recorded', 'blockers', 'notes'],
  properties: {
    wrote_baseline_json: { type: 'boolean' },
    benchmarks: { type: 'array', items: { type: 'object', additionalProperties: true } },
    startup_recorded: { type: 'boolean' },
    sizes_recorded: { type: 'boolean' },
    methodology_recorded: { type: 'boolean' },
    blockers: { type: 'array', items: { type: 'string' } },
    notes: { type: 'string' },
  },
}

const CRITIC_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['check', 'passed', 'blocker', 'findings'],
  properties: {
    check: { type: 'string' },
    passed: { type: 'boolean' },
    blocker: { type: 'boolean', description: 'true if this is a Phase-0 gate blocker' },
    findings: { type: 'array', items: { type: 'string' } },
  },
}

phase('Analyze')

const analyze = await parallel([
  // --- module dependency graph + PORT_PLAN.md ---
  () => agent(`You are the MODULE-GRAPH agent for the Ghostty Zig->Rust port, Phase 0.
${FACTS}

TASK: Build the module dependency graph of the reference and emit a strict topological port order (leaves first).

METHOD (be mechanical):
1. Write a Python script ${PORTDIR}/tools/build_graph.py that scans ${REF}/src/**/*.zig for @import("...") statements. Resolve each import: a relative path import (e.g. @import("../terminal/Parser.zig") or @import("color.zig")) resolves to a file inside src/; map that file to its top-level module = first path component under src/ (e.g. src/terminal/X -> 'terminal'; a root file src/Surface.zig -> '<root>'). Ignore std/builtin and external package imports (anything not resolving to a path inside src/). Build module-level directed edges A->B when a file in module A imports a file in module B (A != B). Count occurrences per edge.
2. Compute a topological order leaves-first (modules with no outgoing intra-src deps first). Detect cycles (use Tarjan/Kahn). If cycles exist between top-level modules, report each SCC and propose a break order with rationale (leaf-most member first).
3. Write ${PORTDIR}/tools/build_graph.py, ${P0}/module_graph.json ({nodes:[...], edges:[{from,to,count}], topo_order:[...], sccs:[[...]]}), and ${P0}/PORT_PLAN.md.

PORT_PLAN.md must contain: (a) the strict topological port order as a numbered list, leaves first, ending at apprt/shells; (b) for EACH module: its direct intra-src dependencies, its static test-block count (from PARITY_LEDGER.json by_module), and a one-line port note; (c) an explicit "cycles & how to break them" section; (d) a note that 'terminal' (2178 blocks, 68%) is the critical-path module and should be sub-divided. Cross-check: leaves like datastruct/simd/unicode must precede terminal; terminal must precede renderer/termio/font-shaping/config/input that depend on it; apprt last.

Return the schema. Do NOT commit.`, { label: 'graph', phase: 'Analyze', schema: GRAPH_SCHEMA }),

  // --- static<->runtime test reconciliation ---
  () => agent(`You are the TEST-RECONCILIATION agent for the Ghostty Zig->Rust port, Phase 0.
${FACTS}

TASK: Account for ALL 3234 static test blocks against what the reference actually compiles/runs, so "ported all tests" becomes verifiable. The runtime counts (2857 in ghostty-test; 4493 across two lib-vt configs) differ from 3234 because (i) the same source blocks compile in MULTIPLE configs/targets (lib-vt runs the VT subset twice), (ii) platform gating (apprt/gtk tests only compile on Linux; apprt/macos only on macOS), (iii) comptime gating (if (builtin.os.tag==...) / if (builtin.target...) around tests or their containing files), (iv) separate build steps cover different/overlapping file sets.

METHOD:
1. Reuse logs /tmp/rustty_ref_test_run.log and /tmp/rustty_ref_libvt_test.log (already produced). You MAY re-run 'cd ${REF} && zig build test --summary all' if needed.
2. Determine which top-level modules/files the macOS ghostty-test root pulls in vs not. Identify platform-gated tests: grep ${REF}/src for 'builtin.os.tag', 'builtin.target', 'comptime', and OS-conditional blocks near test declarations; count apprt/gtk vs apprt/macos test blocks (from PARITY_LEDGER.json file paths: src/apprt/gtk/* and src/apprt/gtk-ng/* are Linux-only; src/apprt/macos/* macOS-only). Identify which static blocks belong to the libghostty-vt surface (read ${REF}/src/lib_vt.zig to see what the VT lib exports/imports).
3. Build an accounting that assigns each of the 3234 static blocks to exactly ONE bucket so the sum == 3234. Suggested buckets: runs_in_ghostty_test_macos, only_other_platform (linux/gtk/windows-gated), only_comptime_gated_off_here, lib_vt_only (in VT lib but not ghostty-test root), wasm_only, benchmark_only, other_unaccounted. Reconcile the per-bucket counts with the observed runtime numbers (explain how 3234 static maps to 2857 macOS-run, and how lib-vt's 2384+2077 arises from re-compiling the VT subset twice). 'other_unaccounted' MUST be 0 for accounts_for_all=true.
4. Write ${P0}/TEST_RECONCILIATION.md (human) and ${P0}/reconciliation.json (machine, with per-bucket counts and the file lists for gated buckets).

Return the schema (buckets sum MUST equal 3234). Do NOT commit.`, { label: 'reconcile', phase: 'Analyze', schema: RECON_SCHEMA }),

  // --- DEFINITION_OF_PARITY.md ---
  () => agent(`You are the PARITY-DEFINITION agent for the Ghostty Zig->Rust port, Phase 0.
${FACTS}

TASK: Write ${P0}/DEFINITION_OF_PARITY.md: the machine-checkable global + per-module gates that govern the whole port. This is the contract every later phase is graded against. Be precise and checkable; no fluff.

Include these sections:
1. GLOBAL GATES (each as: name, what it checks, the exact pass condition, how it is mechanically verified, exit-nonzero condition):
   - Test parity: all 3234 static reference test blocks ported to Rust tests, assertion-for-assertion; ported count per module == reference per-module count (cite the numbers from PARITY_LEDGER by_module). No skipped/xfail/loosened tests (reference's own 15+32 documented skips may be replicated with the SAME condition only).
   - Differential VT: feeding identical input corpus to reference oracle and Rust port yields byte-identical canonical formatter output (the harness in ${PORTDIR}/harness).
   - Differential fuzz: ported fuzz targets (terminal/Parser, terminal/sgr, terminal/Terminal, osc, and any others) run on shared corpus + generated inputs; zero behavioral divergence ref-vs-port.
   - C ABI: cbindgen-regenerated headers are ABI-equivalent to ${REF}/include/ghostty.h and include/ghostty/vt/*; exported symbols stay ghostty_* (byte-identical symbol set & signatures).
   - Comptime artifacts bit-identical: generated tables (unicode props, parse tables, uucode tables) diff byte-for-byte against reference outputs.
   - Serialized layouts byte-identical: packed Cell/Row/Style 64-bit layouts match reference (assert sizeof + bit positions).
   - Leak detection: tests run under a tracking allocator / Miri; zero leaks (matching std.testing.allocator semantics).
   - Identity invariants: TERM/TERM_PROGRAM, terminfo entry, config dir ~/.config/ghostty, binary names, version strings byte-identical to reference.
   - Performance/memory/size/startup: NOT worse than BASELINE.json on every dimension and platform (define the regression threshold, e.g. >X% slower throughput, >X% higher RSS, larger binary, slower startup => red). Render: perceptual diff within a stated tolerance; non-GPU output exact.
2. PER-MODULE GATE TEMPLATE: for module M -> (ported test count == reference[M]; module differential clean; module fuzz clean if it has fuzz targets; coverage >= reference[M]; micro-bench not worse than BASELINE[M]).
3. GATE SCRIPTS SPEC: enumerate the gate scripts to build in Phase 1 (gate_tests.sh, gate_diff_vt.sh, gate_fuzz.sh, gate_abi.sh, gate_tables.sh, gate_layout.sh, gate_leaks.sh, gate_perf.sh) — for each: inputs, comparison, exit code semantics.
4. CONTRACT NUMBERS: embed the full per-module static test counts table (from PARITY_LEDGER by_module, summing to 3234).

Return the schema. Do NOT commit.`, { label: 'definition', phase: 'Analyze', schema: DEF_SCHEMA }),

  // --- differential VT harness (GATE-critical) ---
  () => agent(`You are the DIFFERENTIAL-HARNESS agent for the Ghostty Zig->Rust port, Phase 0. This is GATE-CRITICAL.
${FACTS}

GOAL: Stand up a real VT differential harness and PROVE it works by showing it reports ~100% divergence against the still-empty Rust port, while exiting NON-ZERO. Also prove it is not trivially always-red (self-consistency check).

The canonical VT oracle output uses the libghostty-vt formatter C API. Reference example (DO NOT modify it; it is read-only) ${REF}/example/c-vt-formatter/src/main.c shows the exact calls:
  GhosttyTerminalOptions opts = { .cols=80, .rows=24, .max_scrollback=0 };
  ghostty_terminal_new(NULL, &terminal, opts);
  ghostty_terminal_vt_write(terminal, (const uint8_t*)bytes, len);   // feed VT input
  GhosttyFormatterTerminalOptions f = GHOSTTY_INIT_SIZED(GhosttyFormatterTerminalOptions); f.emit = GHOSTTY_FORMATTER_FORMAT_PLAIN; f.trim = true;
  ghostty_formatter_terminal_new(NULL, &formatter, terminal, f);
  uint8_t *buf=NULL; size_t n=0; ghostty_formatter_format_alloc(formatter, NULL, &buf, &n);
  fwrite(buf,1,n,stdout);  // canonical output
  ghostty_free(NULL,buf,n); ghostty_formatter_free(formatter); ghostty_terminal_free(terminal);

STEPS:
1. Write ${PORTDIR}/harness/oracle.c: read ALL bytes from stdin into a buffer, run the sequence above (80x24, scrollback 0, PLAIN+trim), print the formatter output to stdout. Header is at ${REF}/zig-out/include (so -I ${REF}/zig-out/include and #include <ghostty/vt.h>).
2. Build it. PRIMARY (fast) path: link the prebuilt static lib:
     cc -O2 -I ${REF}/zig-out/include ${PORTDIR}/harness/oracle.c ${REF}/zig-out/lib/libghostty-vt.a -o ${PORTDIR}/harness/oracle -lc++
   If undefined symbols, add macOS frameworks iteratively until it links: -framework CoreFoundation -framework CoreText -framework CoreGraphics -framework Foundation -framework AppKit. Also try the dylib: cc ... -I ${REF}/zig-out/include ${PORTDIR}/harness/oracle.c -L ${REF}/zig-out/lib -lghostty-vt -Wl,-rpath,${REF}/zig-out/lib -o ${PORTDIR}/harness/oracle .
   FALLBACK (robust) path if cc linking is intractable: copy ${REF}/example/c-vt-formatter into ${PORTDIR}/harness/example-oracle/ (a COPY, never edit the original), edit its src/main.c to read stdin, set its build.zig.zon 'ghostty' dependency to { .path = "${REF}" } (lazy), run 'zig build' there, and use the produced executable as the oracle. (Network is available if a url dep must fetch.)
   Verify the oracle works: echo a test string with SGR/cursor sequences through it and confirm sensible canonical output.
3. Build a VT input corpus under ${PORTDIR}/corpora/vt/ : at least 15 files (*.vt, raw bytes incl. ESC=0x1b) covering: plain ASCII text; line wrap at 80 cols; \\r\\n vs \\n; SGR bold/underline/italic/reverse + 16/256/truecolor; cursor moves CUP/CUU/CUD/CUF/CUB; erase EL/ED variants; insert/delete line+char (IL/DL/ICH/DCH); scroll region DECSTBM + scroll; tabs + tab stops; autowrap on/off (DECAWM); UTF-8 multibyte + a combining/grapheme cluster + a wide CJK char; OSC 0/2 title; DEC modes (DECCKM etc.); reverse index/index (RI/IND); save/restore cursor (DECSC/DECRC). Keep each small and deterministic.
4. Write ${PORTDIR}/harness/diff_vt.py: for each corpus file, run the REFERENCE oracle (${PORTDIR}/harness/oracle) on it -> ref_out; attempt to run the PORT oracle at ${PORTDIR}/harness/oracle-port (which DOES NOT EXIST yet) -> if missing/non-runnable, treat port_out as a sentinel <PORT-NOT-IMPLEMENTED>; compare ref_out vs port_out; count divergences; print a per-file table and a summary 'N/M inputs diverged'; EXIT NON-ZERO if divergences>0, EXIT 0 only if all match. Also implement a --self-check mode that uses the reference oracle output as BOTH sides and must report 0 divergence and exit 0 (proves the comparator is correct, not always-red).
5. Run: (a) 'python3 ${PORTDIR}/harness/diff_vt.py' against the empty port -> MUST report ~100% divergence and exit non-zero; (b) 'python3 ${PORTDIR}/harness/diff_vt.py --self-check' -> MUST report 0 divergence and exit 0. Tee both outputs to ${P0}/harness_proof.txt.

Return the schema with REAL measured values (oracle_path, corpus_count, total_inputs, divergence_count, harness_exit_code from step5a, self_consistency_zero_divergence from step5b, proof_path). Do NOT commit. Do NOT modify the reference repo.`, { label: 'harness', phase: 'Analyze', schema: HARNESS_SCHEMA }),
])

const [graph, reconcile, definition, harness] = analyze

phase('Baseline')
// SOLO heavy measurement — runs alone (benchmarks must never run concurrently).
const baseline = await agent(`You are the BASELINE agent for the Ghostty Zig->Rust port, Phase 0. GATE-CRITICAL. You run SOLO; nothing else heavy is running.
${FACTS}

GOAL: Produce ${P0}/BASELINE.json capturing the reference performance/size/memory/startup baseline on THIS machine, with a reproducible methodology. This is the yardstick all future perf gates compare against.

STEPS:
1. Build benchmark tools (long ReleaseFast compile; be patient, no timeout):
     cd ${REF} && zig build -Demit-bench -Doptimize=ReleaseFast -Demit-macos-app=false
   Confirm ${REF}/zig-out/bin/ghostty-bench and ghostty-gen exist.
2. Discover benchmarks: read ${REF}/src/benchmark/cli.zig, options.zig, main.zig and run 'zig-out/bin/ghostty-bench --help' (and ghostty-gen --help). Enumerate every benchmark kind (TerminalStream, TerminalParser, OscParser, CodepointWidth, GraphemeBreak, IsSymbol, ScreenClone, and any others).
3. For each benchmark: generate deterministic input with ghostty-gen (fixed seed if supported) to a /tmp file, then run 'ghostty-bench <kind> --data <file>' (consult --help for exact flags). Run each 3+ times SEQUENTIALLY (NEVER in parallel) and record the median throughput / time and the units exactly as the tool reports them. If hyperfine is available use it; otherwise capture the tool's own timing output.
4. Startup time: measure the app/CLI startup, e.g. time 'zig-out/Ghostty.app/Contents/MacOS/ghostty --version' (or +show-config / --help) over several runs; record median ms. Note this is CLI startup (GUI cold-start needs a display; record as N/A-with-reason if it cannot run headless).
5. Binary sizes: stat and record ghostty macho (expect 140361712, universal x86_64+arm64), app bundle (162746368), libghostty-vt.dylib (5742200), .a (12065920). Note the universal-binary caveat (arm64-only would be ~half) as a methodology detail so the port compares like-for-like.
6. Memory: record peak RSS from the test run (232M) and the MaxRSS the benchmark runs report.
7. Frame timing: if no benchmark exercises the renderer headlessly, record frame-timing as N/A with reason (GUI/GPU session required) — do NOT silently omit; it becomes a Phase-3 TODO.
8. Assemble ${P0}/BASELINE.json with keys: reference_commit, machine {model,arch,cores,ram_bytes,os}, build {bench_cmd, optimize}, benchmarks [{name, metric, value, unit, runs, raw:[...]}], startup_ms {value, method, runs}, binary_sizes {ghostty_macho, app_bundle, libghostty_vt_dylib, libghostty_vt_a, notes}, memory {test_peak_rss_mb, bench_max_rss}, frame_timing, methodology (exact commands + iteration counts), date_iso:"2026-06-21".

Return the schema with the actual benchmark list and any blockers (e.g., a benchmark that would not build/run). A benchmark that genuinely cannot run is logged in blockers, not silently dropped. Do NOT commit. Do NOT modify the reference repo.`, { label: 'baseline', phase: 'Baseline', schema: BASELINE_SCHEMA })

phase('Critique')
const critics = await parallel([
  () => agent(`You are an adversarial COUNT critic for Phase 0. Independently RE-VERIFY the static test contract using a DIFFERENT method than the Python extractor (use ripgrep/awk/grep). 
- Count, across ${REF}/src/**/*.zig: named tests (^\\s*test\\s+"), anonymous (^\\s*test\\s*\\{ with nothing but optional ws+brace), and declref (^\\s*test\\s+[A-Za-z_][A-Za-z0-9_.]*\\s*\\{). Confirm 3113 + 76 + 45 = 3234.
- Cross-check against ${P0}/PARITY_LEDGER.json total_tests and by_module sums (must total 3234).
- Hunt for any test form the extractor could MISS (e.g. 'test{' no space; multi-line where 'test' and name/brace span lines; tests inside @import-ed files not under src/; tests in generated code). Report any discrepancy as a BLOCKER with file:line evidence.
Return CRITIC_SCHEMA. check='static test count == 3234'.`, { label: 'critic:count', phase: 'Critique', schema: CRITIC_SCHEMA }),

  () => agent(`You are an adversarial HARNESS critic for Phase 0. Verify the differential VT harness is REAL and correct, not faked or always-red.
- Read ${P0}/harness_proof.txt and inspect ${PORTDIR}/harness/oracle.c and diff_vt.py.
- Confirm the oracle binary exists, is executable, and actually links libghostty-vt: run it yourself on a crafted input (e.g. printf 'ab\\033[31mc\\r\\n' | ${PORTDIR}/harness/oracle) and confirm non-empty canonical output.
- Confirm 'python3 ${PORTDIR}/harness/diff_vt.py' exits NON-ZERO and reports ~100% divergence against the empty port.
- Confirm 'python3 ${PORTDIR}/harness/diff_vt.py --self-check' exits 0 with 0 divergence (proves the comparator distinguishes match from mismatch).
- Confirm the harness will swap in a real port oracle at ${PORTDIR}/harness/oracle-port later (the path is wired, not hardcoded-red).
Any failure => BLOCKER. Return CRITIC_SCHEMA. check='diff harness valid + red on empty port + green on self'.`, { label: 'critic:harness', phase: 'Critique', schema: CRITIC_SCHEMA }),

  () => agent(`You are an adversarial PORT-PLAN critic for Phase 0. Verify topological validity.
- Read ${P0}/module_graph.json and PORT_PLAN.md.
- Verify the topo_order is consistent with edges: for every edge A->B (A depends on B), B must appear BEFORE A in the order, UNLESS A,B are in a reported cycle/SCC. Recompute independently and diff.
- Verify leaves (datastruct, simd, unicode) precede terminal; terminal precedes renderer, termio, config, input, font-shaping consumers; apprt is last.
- Verify every top-level src module appears exactly once in the order.
- Report cycles not disclosed in PORT_PLAN.md as findings. Any ordering contradiction not covered by a disclosed cycle => BLOCKER.
Return CRITIC_SCHEMA. check='PORT_PLAN topological validity'.`, { label: 'critic:plan', phase: 'Critique', schema: CRITIC_SCHEMA }),

  () => agent(`You are an adversarial RECONCILIATION critic for Phase 0.
- Read ${P0}/reconciliation.json and TEST_RECONCILIATION.md.
- Verify the buckets sum EXACTLY to 3234 and accounts_for_all is justified (other_unaccounted == 0).
- Spot-check gated buckets: count src/apprt/gtk + src/apprt/gtk-ng test blocks via ripgrep and confirm they match the 'only_other_platform' contribution from apprt; confirm src/apprt/macos blocks are placed in the macOS-run bucket. Sanity-check that the claimed macOS-run count is consistent with the observed 2857 runtime (2842+15).
- If the reconciliation hand-waves any block or the sum != 3234 => BLOCKER.
Return CRITIC_SCHEMA. check='reconciliation accounts for all 3234'.`, { label: 'critic:reconcile', phase: 'Critique', schema: CRITIC_SCHEMA }),

  () => agent(`You are an adversarial BASELINE critic for Phase 0.
- Read ${P0}/BASELINE.json.
- Verify it contains: >=4 real benchmark measurements with units, startup_ms, all four binary sizes, machine spec matching M4 Pro/14-core/48GB/macOS 26.5.1, and a reproducible methodology with exact commands.
- Sanity-check the binary sizes against the known facts (ghostty 140361712, app 162746368, dylib 5742200, .a 12065920).
- Flag any dimension that is missing WITHOUT a documented reason (silent gaps are not allowed; N/A-with-reason is acceptable).
- If BASELINE.json is absent, empty, or has fabricated/placeholder numbers => BLOCKER.
Return CRITIC_SCHEMA. check='BASELINE.json complete + reproducible'.`, { label: 'critic:baseline', phase: 'Critique', schema: CRITIC_SCHEMA }),
])

const blockers = critics.filter(Boolean).filter(c => c.blocker || !c.passed)
return {
  contract_total: 3234,
  analyze: { graph, reconcile, definition, harness },
  baseline,
  critics,
  blockers,
  gate_green: blockers.length === 0
    && !!harness && harness.harness_exit_code !== 0 && harness.self_consistency_zero_divergence === true
    && !!baseline && baseline.wrote_baseline_json === true
    && !!reconcile && reconcile.accounts_for_all === true,
}
