export const meta = {
  name: 'perf-round2',
  description:
    'Round 2 — beat Zig on the remaining 4 benches (t-parser +22.7%, sc.clone +60%, sc.render +58%, osc tie): profile-guided diagnosis, then sequential fix chain + parallel alloc-benchmark worktree. NO official benches (idle-gated later); profiling/spot-timing allowed for attribution only.',
  phases: [
    { title: 'Profile', detail: '4 parallel hotspot attributions on the ship binary' },
    { title: 'Implement', detail: 'sequential fix chain || alloc-bench worktree' },
    { title: 'MergeGate', detail: 'merge, full gates, ship binaries' },
  ],
}

const W = '/Users/gael/projects/rustty/.claude/worktrees/perf-opt'
const WA = '/Users/gael/projects/rustty/.claude/worktrees/perf-alloc'
const G = '/Users/gael/projects/ghostty'
const R = '/Users/gael/projects/rustty'
const COR = `${R}/.port/phase-0/baseline/corpora`
const PLAN = `${W}/.port/phase-2/PERF_FIX_PLAN.md`
const LOG = `${W}/.port/phase-2/PERF_LOG.md`

const CTX = `rustty = full-fidelity Zig->Rust port of Ghostty. Reference oracle (READ-ONLY): ${G}. Working worktree: ${W}, branch worktree-perf-opt @ 1e96603 (11 perf commits already landed this session — read ${LOG} for what was done and measured, and ${PLAN} for the original diagnosis).

CURRENT STANDINGS (SHIP-RF = release-fast at HEAD; same-session Zig, N=7 medians):
- terminal-parser/utf8: zig 262.9 vs rust 322.7 (+22.7%) — LOSS. Already landed: inline idioms, destage reshape. Release-A and RF nearly equal (328/322); fat LTO makes it WORSE (531). Leading candidate: [Option<Action>;3] per-byte return still carries Vec drop glue (CSI/ESC/DCS payload Vec<u8>/Vec<u16>, OSC Command clone) — PLAN item t3-action-arrayvec. MAX_INTERMEDIATE=4, MAX_PARAMS=24 are hard invariants.
- screen-clone.clone/small: zig 49.7 vs rust 79.8 (+60%) — LOSS, untouched by round 1 (pin-keys/hasher were noise). Candidates: PLAN fix 9 cell-copy-coalesce (masked u64 store vs 3 RMW bitfield setters per cell in page_impl.rs:~1105-1119; Style::eql 16-bit compare vs 9-shift to_u16 repack), PLUS: fat LTO uniquely improved this bench 84->70 (-16%) — find WHAT LTO inlines on the clone chain and replicate with targeted #[inline]/#[inline(always)] attrs.
- screen-clone.render/small: zig 15.6 vs rust 24.7 (+58%) — LOSS. SoA reshape already landed (31.4->24.7). The remaining gap needs fresh profiling: suspect the managed-row loop (per-cell style/grapheme handling) or remaining per-cell branches vs Zig render.zig's column-slice loops.
- osc-parser: zig 57.3 vs rust 57.5 (tie) — user wants a WIN. Fat LTO got 48.4 (-16% vs zig) so the win EXISTS in the code; global LTO is banned (it pessimizes t-parser 60%). Find what LTO inlines/DCEs on the osc path (bench osc_parser.rs step loop -> vt osc::Parser::next/end) and capture it with targeted attrs or local reshape.

HARD RULES:
- Functional parity NON-NEGOTIABLE (R1): observable behavior identical to the Zig reference; ALL existing tests stay green; never change what a test asserts; reference comments preserved (restructures carry them along). Zig byte-slices stay &[u8]/Vec<u8>.
- Perf changes marked "// PERF(port): <what>" or "// PORT NOTE (reshape): <reason>" (R10).
- Global LTO is BANNED (measured pessimization, see Cargo.toml comment + ${LOG}). Targeted #[inline]/#[inline(always)] is allowed and encouraged where the reference inlines.
- unsafe/get_unchecked ONLY with a compiler-provable bound or a unit test machine-checking the invariant + debug_assert (PLAN DO-NOT #3).
- NO official benchmarks (the machine is NOT idle — another session is running). You MAY run the bench binary for PROFILING/attribution (xctrace Time Profiler, /usr/bin/sample, or quick relative spot-timings) — mark any timing as non-official. Profilers available: /usr/bin/xctrace (Time Profiler template), /usr/bin/sample. Bench binary: ${W}/.perf-bins/rustty-bench-SHIP-RF (and per-fix variants there). Corpora: ${COR}/{utf8.bin,ascii.bin,osc-parser.bin,ascii-small.bin}. Invocations: +terminal-parser --data=utf8.bin | +screen-clone --mode=clone|render --data=ascii-small.bin | +osc-parser --data=osc-parser.bin. For xctrace: xctrace record --template 'Time Profiler' --output /tmp/<name>.trace --launch -- <bin> <args>; then xctrace export --input /tmp/<name>.trace --xpath '//trace-toc' or use 'xctrace export ... --har' alternatives; if export is unwieldy, run the binary in a loop and use 'sample <pid> 2 -mayDie' from a second shell. A pragmatic alternative: objdump/otool disassembly + differential micro-builds (that method found round 1's wins).`

const P_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['area', 'hotspots', 'fix_spec', 'confidence'],
  properties: {
    area: { type: 'string' },
    hotspots: { type: 'array', items: { type: 'string' }, description: 'ranked: symbol/loop -> % share or evidence' },
    fix_spec: { type: 'string', description: 'concrete implementation spec: files, exact changes, parity argument, gates' },
    confidence: { type: 'string', description: 'high|medium|low + why' },
  },
}

phase('Profile')
const profiles = await parallel([
  () => agent(
    `${CTX}\n\nPROFILE AREA: terminal-parser residual (+22.7%). Attribute where the remaining ~60ms/64MiB goes in the SHIP-RF binary vs Zig. Specifically test the drop-glue hypothesis: disassemble the bench parse_all loop (crates/rustty-bench/src/terminal_parser.rs) in .perf-bins/rustty-bench-SHIP-RF — is there a per-byte call to core::ptr::drop_in_place or Vec drop glue on the discarded [Option<Action>;3]? Profile with xctrace/sample to get the loop's instruction mix. Then write the fix_spec for t3-action-arrayvec: replace Vec<u8>/Vec<u16>/heap payloads in Action's CSI/ESC/DCS variants (parser.rs Action enum) with inline fixed-capacity arrays (([u8;4],len)/([u16;24],len) — MAX_INTERMEDIATE=4/MAX_PARAMS=24 hard invariants, Zig Parser.zig:80-84 fixed arrays) exposing as_slice()/len(); sweep ALL consumers (stream.rs handlers, vt-c mirrors, tests that pattern-match payloads — they keep asserting identical contents via as_slice). Also assess the OSC Command in Action (does the OscDispatch arm still clone/carry heap? note what t3-osc-command-move would add). Confirm the edit surface precisely (grep every Action::CSI/EscDispatch/DcsHook constructor + matcher in the workspace incl. rustty-vt-c).`,
    { label: 'prof:t-parser', phase: 'Profile', agentType: 'general-purpose', schema: P_SCHEMA }),

  () => agent(
    `${CTX}\n\nPROFILE AREA: screen-clone.clone (+60%). Profile 'rustty-bench-SHIP-RF +screen-clone --mode=clone --data=${COR}/ascii-small.bin' (xctrace/sample; the step loops 1000 clones internally so one invocation is ~80ms — loop it externally if the profiler needs longer). Identify the top self-time symbols in the clone chain (Screen::clone -> PageList::clone -> page clone/copy -> style/grapheme/hyperlink adoption). Compare against ${G}/src/terminal/{Screen.zig,PageList.zig,page.zig} clone paths. Deliver fix_spec covering: (1) PLAN fix 9 cell-copy-coalesce exactly (page_impl.rs ~1105-1119 masked u64 op + equivalence unit test over all flag combos; Style::eql via packed u16 compare if the profile shows Style::eql/to_u16 hot — keep packed_style_layout test as gate); (2) the LTO-win replication: diff what fat LTO inlines (compare .perf-bins/rustty-bench-C-rf-lto disasm vs SHIP-RF on the hot symbols) and list the exact #[inline] attrs to add; (3) anything else the profile surfaces (e.g. hashmap ops, memcpy granularity, per-page overhead).`,
    { label: 'prof:sc-clone', phase: 'Profile', agentType: 'general-purpose', schema: P_SCHEMA }),

  () => agent(
    `${CTX}\n\nPROFILE AREA: screen-clone.render (+58% post-SoA). Profile 'rustty-bench-SHIP-RF +screen-clone --mode=render --data=${COR}/ascii-small.bin'. The SoA reshape already landed (render.rs) — find what still costs 24.7ms vs Zig 15.6ms per 1000 updates. Read the CURRENT render.rs (post-9789788) vs ${G}/src/terminal/render.zig line by line on the update path: row loop, managed-row detection, per-cell style/grapheme writes, dirty handling, any allocation per update (Vec growth? style map lookups? RefCell/bounds overhead?). Check: does Zig skip unmanaged rows entirely via a bitset where Rust still walks cells? Is the raw copy actually a single memcpy per row in the built binary (disasm check) or did the zip loop compile to per-cell copies? Deliver a concrete fix_spec with parity argument.`,
    { label: 'prof:sc-render', phase: 'Profile', agentType: 'general-purpose', schema: P_SCHEMA }),

  () => agent(
    `${CTX}\n\nPROFILE AREA: osc-parser tie->win (-16% is proven available: fat-LTO binary .perf-bins/rustty-bench-C-rf-lto scores 48.4 vs SHIP-RF 57.5 on identical code). Find the delta: profile BOTH binaries on '+osc-parser --data=${COR}/osc-parser.bin' (each ~60-200ms; loop for sampling), and/or disassemble the hot path in both (bench step loop in crates/rustty-bench/src/osc_parser.rs -> rustty-vt osc::Parser::{next,end} -> parsers/*). What does LTO inline or DCE that the non-LTO build does not? Candidates: the bench->vt cross-crate step loop boundary; the record-framing read helpers (options.rs read_exact); Parser::end -> per-command parser dispatch; memchr/scan helpers. Deliver fix_spec: the exact #[inline]/#[inline(always)] attrs (or small local reshapes) that replicate the LTO win WITHOUT global LTO, with the t-parser guard (the fix must not re-create the monster-inline pessimization — keep attrs scoped to the osc path).`,
    { label: 'prof:osc', phase: 'Profile', agentType: 'general-purpose', schema: P_SCHEMA }),
])

const good = profiles.filter(Boolean)
log(`Profile done: ${good.length}/4. Confidences: ${good.map(p => `${p.area.split(' ')[0]}=${p.confidence.split(' ')[0]}`).join(' ')}`)

phase('Implement')
const FIX_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['fix_id', 'disposition', 'commit', 'files_changed', 'gates', 'notes'],
  properties: {
    fix_id: { type: 'string' },
    disposition: { type: 'string' },
    commit: { type: 'string' },
    files_changed: { type: 'array', items: { type: 'string' } },
    gates: { type: 'string' },
    notes: { type: 'string' },
  },
}

const IMPL_RULES = `${CTX}\n\nIMPLEMENTATION RULES: implement EXACTLY ONE fix per your assignment in ${W} (branch worktree-perf-opt). Gates before commit: cargo test -p <touched crates> (+ cargo test -p rustty-vt when touching vt); cargo clippy -p <touched> --no-deps --all-targets -- -D warnings; cargo fmt -p <touched> -- --check. If gates cannot pass with parity intact: git checkout your files, disposition 'reverted: <why>'. Commit message: "rustty: perf — <fix-id>: <title>" + body + "\\n\\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>\\nClaude-Session: https://claude.ai/code/session_01PY4UZsF6TuQWkJwbbA8JgC". After commit, stash attribution binary: cargo build -p rustty-bench --profile release-fast --bin rustty-bench && cp target/release-fast/rustty-bench .perf-bins/rustty-bench-r2-<fix-id>. Do NOT touch other .perf-bins entries. NO official benches; profiling/spot-checks allowed (non-official).`

async function runChain() {
  const chain = [
    { id: 'r2-action-arrayvec', spec: good.find(p => p.area.toLowerCase().includes('parser')) },
    { id: 'r2-clone', spec: good.find(p => p.area.toLowerCase().includes('clone') && !p.area.toLowerCase().includes('render')) },
    { id: 'r2-render', spec: good.find(p => p.area.toLowerCase().includes('render')) },
    { id: 'r2-osc-win', spec: good.find(p => p.area.toLowerCase().includes('osc')) },
  ]
  const results = []
  for (const f of chain) {
    if (!f.spec) { log(`${f.id}: no profile spec — skipping`); results.push(null); continue }
    const r = await agent(
      `${IMPL_RULES}\n\nYOUR FIX: ${f.id}. Implement this profiling-phase spec (verify its file:line refs against current HEAD first; earlier chain fixes may have shifted things):\n\nHOTSPOTS:\n${f.spec.hotspots.join('\n')}\n\nFIX SPEC:\n${f.spec.fix_spec}\n\nConfidence from profiling: ${f.spec.confidence}. If the spec is multi-part, land the parts as ONE commit for this fix id. If a part is too risky for parity, do the safe parts and note what you dropped.`,
      { label: `impl:${f.id}`, phase: 'Implement', agentType: 'general-purpose', schema: FIX_SCHEMA })
    results.push(r)
    log(`${f.id}: ${r ? r.disposition + ' ' + (r.commit || '') : 'agent failed'}`)
  }
  return results
}

const allocAgent = () => agent(
  `${CTX}\n\nSEPARATE WORKTREE TASK — memory-allocation benchmarks (PLAN §T0-B corrections + fix 12). Create your worktree FIRST: cd ${W} && git worktree add ${WA} -b worktree-perf-alloc && cd ${WA} — do ALL work there.\n\nDeliverables:\n1. Move the pagelist allocation-tracking seam off #[cfg(test)] onto a real off-by-default cargo feature 'alloc-bench' in rustty-vt (Cargo.toml [features] + cfg(any(test, feature = "alloc-bench")) at the seam sites in pagelist/mod.rs — keep test behavior identical). Ensure production (no-feature) builds are byte-identical code (feature off = today's non-test build).\n2. Standalone equal-scope alloc bench: a rustty-vt example (examples/alloc_bench.rs, gated on the feature) replicating EXACTLY the workload of the Zig standalone bench at ${R}/.port/phase-1/bench-zig-pagelist (READ its source + RESULTS.txt): init(80,24,max) -> grow to >=2000 rows -> scroll -> full clone -> resize reflow 80->{120,200}->40->80 -> reset -> deinit, reporting: page-alloc count, peak outstanding bytes, peak RSS (getrusage ru_maxrss — BYTES on macOS), and median latency of 9 runs (1 warmup). Match the Zig bench's scope precisely (same max_size if possible — read what it uses; document any forced divergence). Note: it must run WITHOUT cargo-test overhead (cargo run -p rustty-vt --example alloc_bench --features alloc-bench --profile release-fast).\n3. PLAN fix 12 arena-resize-seam: wire in-place resize from ArenaAllocator::reset (rustty-zigstd arena_allocator.rs:~386-411 manual-realloc fallback) to the ALREADY-PORTED TaggedPageAllocator::resize/realloc (rustty-os/src/mach.rs:99-236) via a ResizeInPlace trait defaulting false (Global stays false); implement for PageAllocator (rustty-vt pagelist page_allocator.rs). CRITICAL: the TrackingAllocator (rustty-zigstd tracking_allocator.rs) must forward resize AND adjust recorded sizes or R6 leak gates see phantom leaks. Parity: allocation geometry is not observable; no test asserts backing-node counts; in-place shrink only on end_index=0 nodes (the Zig invariant, arena_allocator.zig:154).\n4. std_size golden test: assert Page std_size()==589824 on aarch64-macos (pagelist geometry drift gate). If it FAILS, do NOT change geometry — report the actual value + layout diff vs page.zig:1700-1760 as a finding.\n5. RUN the alloc bench (single runs are fine for alloc COUNTS/bytes — they are deterministic; note latency numbers as non-official since the machine is busy) and, if ${R}/.port/phase-1/bench-zig-pagelist has a runnable binary, re-run it for fresh Zig numbers (else use RESULTS.txt). Write ${WA}/.port/phase-2/ALLOC_BENCH_V2.md: methodology (equal scope!), results table (allocs, peak bytes, RSS, latency-nonofficial), delta vs the old flawed ALLOC_BENCH.md numbers, and the resize-seam before/after alloc count.\n\nGates: cargo test -p rustty-vt -p rustty-zigstd -p rustty-os (default features AND --features alloc-bench for vt); clippy/fmt on touched crates; the R6-style leak checks = the existing pagelist tracking tests must stay green. Commit everything in ${WA} (branch worktree-perf-alloc) with the standard trailer. Report via schema (fix_id='alloc-bench-suite').`,
  { label: 'impl:alloc-suite', phase: 'Implement', agentType: 'general-purpose', schema: FIX_SCHEMA })

const [chainResults, allocResult] = await parallel([runChain, allocAgent])
log(`Chain: ${(chainResults || []).filter(Boolean).filter(r => (r.disposition || '').startsWith('committed')).length}/4 committed. Alloc: ${allocResult ? allocResult.disposition : 'FAILED'}`)

phase('MergeGate')
const M_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['merge_ok', 'gates_green', 'final_commit', 'gate_report'],
  properties: {
    merge_ok: { type: 'boolean' },
    gates_green: { type: 'boolean' },
    final_commit: { type: 'string' },
    gate_report: { type: 'string' },
  },
}
const merge = await agent(
  `${IMPL_RULES}\n\nMERGE + GATE (you are finalizing, not implementing a fix). In ${W}:\n1. If branch worktree-perf-alloc has commits (cd ${WA} 2>/dev/null && git log --oneline -5), merge: cd ${W} && git merge worktree-perf-alloc -m "rustty: perf — merge alloc-bench suite branch" (+ trailer). Trivial conflicts only; else abort + report.\n2. Full gates: cargo build --workspace --release; cargo test --workspace (debug lane is canonical); cargo test -p rustty-vt --features alloc-bench; clippy on all round-2-touched crates (--no-deps -D warnings); fmt --check on touched crates; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_comment_coverage.py (delta vs the documented baseline counts: cli 2, inspector 22, vt 2 — only NEW missing count against us).\n3. Build ship binaries: cargo build -p rustty-bench --release --bin rustty-bench && cp target/release/rustty-bench .perf-bins/rustty-bench-r2-SHIP-A; cargo build -p rustty-bench --profile release-fast --bin rustty-bench && cp target/release-fast/rustty-bench .perf-bins/rustty-bench-r2-SHIP-RF.\n4. Report schema fields (gate_report verbatim per gate; gates_green=true only modulo the documented pre-existing failures).`,
  { label: 'merge+gate', phase: 'MergeGate', agentType: 'general-purpose', schema: M_SCHEMA })

return { profiles: good, chain: chainResults, alloc: allocResult, merge }
