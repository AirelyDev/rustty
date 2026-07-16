export const meta = {
  name: 'perf-explore',
  description:
    'Parallel static diagnosis of the 4 hot Rust-vs-Zig benchmark gaps (terminal-parser 2.9x, osc-parser 3.3x, screen-clone 2-2.5x, unicode family +20-30%) plus the pagelist allocator RSS gap and a PERF(port) marker audit; synthesizes a ranked fix plan. NO benchmark runs (machine busy).',
  phases: [
    { title: 'Diagnose', detail: '6 parallel code-level investigations vs the Zig oracle' },
    { title: 'Synthesize', detail: 'rank fixes into an implementation plan' },
  ],
}

const W = '/Users/gael/projects/rustty/.claude/worktrees/perf-opt'
const G = '/Users/gael/projects/ghostty'
const PLAN = `${W}/.port/phase-2/PERF_FIX_PLAN.md`

const RULES = `You are diagnosing PERFORMANCE gaps in rustty, a full-fidelity Zig->Rust port of Ghostty. Reference oracle (READ-ONLY): ${G} (Zig 0.15.2). Working tree: ${W} (a worktree; branch worktree-perf-opt). Do NOT run any benchmarks (the machine is busy with unrelated load; timing would be garbage). Do NOT edit any files. Static analysis only: read code, compare Rust vs Zig, reason about machine behavior (bounds checks, overflow checks, allocation, indirection, inlining, memory layout, dispatch).

MEASURED FACTS (idle machine, N=7 medians, identical corpora, cargo --release profile which has overflow-checks=true):
- terminal-parser/utf8: Zig 286ms vs Rust 834ms (+191%, 2.9x) — raw Parser::next per byte over 64MiB
- osc-parser: Zig 63ms vs Rust 204ms (+226%, 3.3x) — length-prefixed OSC record replay
- screen-clone.clone: Zig 58ms vs Rust 118ms (+104%) — 1000x Screen::clone; .render: Zig 20ms vs Rust 50ms (+150%) — 1000x RenderState::update
- codepoint-width.table +23%, .simd +18%, .wcwidth +27%; grapheme-break.noop +31%, .table +30%; is-symbol.table +24%, .uucode +23% — all decode 64MiB utf8 via UTF8Decoder byte loop
- terminal-stream/ascii: only +1.3% (statistical tie!) — the Stream print path is FINE. Whatever is slow is NOT shared with terminal-stream's per-byte path, OR is amortized there.
- Allocator study (.port/phase-2/ALLOC_BENCH.md): rustty PageList workload peak RSS 11.4MB vs Zig 7.7MB (+47%); arena backing allocs 7 vs Zig 3 mmaps; rustty latency actually BEATS the Zig standalone bench there.
KNOWN GLOBAL FACTOR (do not re-derive, but DO note where it would matter most): the benched profile had overflow-checks=true, no LTO, codegen-units=16, panic=unwind; Zig is ReleaseFast (no overflow checks, whole-program opt, aborting panics). Profile-parity variants are being built separately. Your job is what REMAINS beyond profile flags — but flag any site where overflow checks in a per-byte loop are plausibly the dominant cost.

CONSTRAINT for proposed fixes: functional parity is NON-NEGOTIABLE (R1: observable behavior identical to the Zig reference; all existing tests must stay green; reference comments preserved). Structural divergence for perf is allowed per PORT_RULES R10 if marked "// PERF(port): <reason>" or "// PORT NOTE: <reshape reason>". Prefer fixes in this order: (1) restore a Zig perf idiom the port downgraded, (2) remove Rust-only overhead (bounds checks via iterator/get_unchecked-with-proof, needless Option/Result wrapping, by-value copies, allocation), (3) algorithmic reshape with parity proof.

Return findings as the schema requires. Cite EXACT file:line evidence on both sides. Estimate impact honestly (high/medium/low + why). Note parity risk per fix.`

const F_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['area', 'root_causes', 'summary'],
  properties: {
    area: { type: 'string' },
    summary: { type: 'string' },
    root_causes: {
      type: 'array',
      items: {
        type: 'object', additionalProperties: false,
        required: ['finding', 'rust_evidence', 'zig_evidence', 'proposed_fix', 'expected_impact', 'parity_risk'],
        properties: {
          finding: { type: 'string' },
          rust_evidence: { type: 'string', description: 'file:line + what the code does' },
          zig_evidence: { type: 'string', description: 'file:line + what the reference does differently' },
          proposed_fix: { type: 'string' },
          expected_impact: { type: 'string', description: 'high|medium|low + reasoning' },
          parity_risk: { type: 'string', description: 'none|low|medium|high + what could diverge' },
        },
      },
    },
  },
}

phase('Diagnose')
const findings = await parallel([
  () => agent(
    `${RULES}\n\nAREA: terminal-parser (2.9x slow). The bench (crates/rustty-bench/src/terminal_parser.rs) reads 4096-byte chunks and calls rustty_vt Parser::next(c) per byte, discarding the [Option<Action>;3]. Read ${W}/crates/rustty-vt/src/parser.rs FULLY (esp. next(), the state-transition table lookup, Action construction) and ${G}/src/terminal/Parser.zig side by side. Investigate: (1) the transition-table representation — Zig uses a comptime-built packed table indexed [state][byte]; what does Rust do (2D array? match? bounds checks per lookup)? (2) the [Option<Action>;3] return — size in bytes vs Zig's [3]?Action (niche optimization? is Action large — does it contain a full CSI params array by value? measure with a size_of note); Zig copies the same struct — compare actual sizes. (3) per-byte function-call overhead: is next() #[inline]? does the hot path allocate (params ArrayList)? (4) any u8->usize casts with bounds checks in the table lookup. (5) intermediate_buf/params handling vs Zig fixed arrays. Also check what clear()/collect do per byte. Rank the top 3 candidate causes for a 2.9x with the overflow-check caveat in mind.`,
    { label: 'diag:terminal-parser', phase: 'Diagnose', agentType: 'general-purpose', schema: F_SCHEMA }),

  () => agent(
    `${RULES}\n\nAREA: osc-parser (3.3x slow). The bench (crates/rustty-bench/src/osc_parser.rs) replays length-prefixed OSC records: parser.next(c) per byte then parser.end(BEL). Read ${W}/crates/rustty-vt/src/osc/mod.rs FULLY (Parser::init/reset/next/end, Command storage, the buf handling) vs ${G}/src/terminal/osc.zig. Investigate: (1) does Rust Parser::next allocate or copy where Zig writes into a fixed buf? (2) the state machine dispatch — match-per-byte vs Zig switch; any string comparisons per byte? (3) reset() cost per record — does it free/realloc (Zig retains capacity)? (4) Command construction in end() — clones vs Zig slices into the parser buf. (5) the ZigAllocatorRef indirection: rustty's Parser::init(Some(alloc_ref)) — is every buffer op going through a dyn-style allocator vtable where Zig monomorphizes? (6) any per-byte UTF-8 validation Rust does that Zig doesn't. Note: the bench calls next() via a plain call where Zig uses @call(.always_inline) — check if rustty-vt's next is #[inline]. Rank top 3 causes for 3.3x.`,
    { label: 'diag:osc-parser', phase: 'Diagnose', agentType: 'general-purpose', schema: F_SCHEMA }),

  () => agent(
    `${RULES}\n\nAREA: screen-clone.clone (+104%) and .render (+150%). The bench (crates/rustty-bench/src/screen_clone.rs) does 1000x Screen::clone(alloc, viewport-top, None) / 1000x RenderState::update. Read ${W}/crates/rustty-vt/src/screen.rs (clone, ~line 828), the PageList clone path (${W}/crates/rustty-vt/src/pagelist/mod.rs clone), and ${W}/crates/rustty-vt/src/render.rs (RenderState::update) vs ${G}/src/terminal/Screen.zig, PageList.zig (clone), and the Zig render state equivalent. Investigate: (1) per-clone allocations: does Rust clone create a fresh pool/arena per clone where Zig reuses (MemoryPool preheat)? Count allocation calls per clone on both sides. (2) page copy mechanics: memcpy of whole pages vs row-by-row cell copies; any per-cell work (style/grapheme rehashing) Rust does that Zig memcpys? (3) hash maps in Screen (style set, grapheme map, hyperlink map) — Rust std HashMap SipHash vs Zig's cheaper hashes? cloned per iteration? (4) render.rs update: what gets rebuilt per update — full cell grid? any Vec growth without with_capacity? (5) the 589824B std page geometry difference (rustty ~586KB, 4KiB min vs Zig 16KiB min pages) affecting copy counts. Rank top 3 causes each for clone and render.`,
    { label: 'diag:screen-clone', phase: 'Diagnose', agentType: 'general-purpose', schema: F_SCHEMA }),

  () => agent(
    `${RULES}\n\nAREA: pagelist allocator — RSS +47% vs Zig (11.4MB vs 7.7MB) and 7 arena backing allocs vs Zig 3 mmaps (see ${W}/.port/phase-2/ALLOC_BENCH.md for the full study — READ IT FIRST). Read ${W}/crates/rustty-vt/src/pagelist/mod.rs (the DynamicMemoryPoolAligned<PageAllocator> + pageAllocator mmap path) vs ${G}/src/terminal/PageList.zig + Zig std.heap.MemoryPool/page_allocator (mach.taggedPageAllocator on Darwin). Investigate: (1) rustty page geometry: std page size & min page size vs Zig's 16KiB-hw-page-aligned 589824B std page — does rustty over-allocate or fragment? (2) the arena doubling policy: first block size, growth factor, preheat (Zig preheats 4 std pages in 2 mmaps) — why 7 blocks vs 3? (3) does rustty's mmap path use MAP_ANON lazy zero + does reset() retain (retain_with_limit) or munmap? (4) RSS delta: what extra 3.7MB does rustty touch — eager zeroing? metadata? overshoot from doubling? (5) propose concrete parameter/geometry changes to match Zig (preheat count, initial block size, 16KiB alignment) with parity analysis (allocation geometry is NOT observable behavior — low parity risk, but confirm no test asserts page counts). Rank fixes by RSS and alloc-count impact.`,
    { label: 'diag:pagelist-alloc', phase: 'Diagnose', agentType: 'general-purpose', schema: F_SCHEMA }),

  () => agent(
    `${RULES}\n\nAREA: unicode-property family (+20-30%: codepoint-width table/simd/wcwidth, grapheme-break noop/table, is-symbol table/uucode). ALL of these share one hot loop: UTF8Decoder::next(byte) over 64MiB then a cheap property lookup. Since even 'noop' modes (decode-only) show +31-43%, THE DECODER IS THE PRIME SUSPECT. Read ${W}/crates/rustty-vt/src/utf8_decoder.rs vs ${G}/src/terminal/UTF8Decoder.zig. Investigate: (1) the DFA table lookup — Zig uses a packed u8 table; Rust representation and bounds checks? (2) the (Option<u32>, bool) tuple return vs Zig's struct — extra branches? (3) #[inline] on next()? (4) overflow-checks impact on the accumulate step (cp = cp << 6 | ...). Then the lookups: rustty_unicode::table lut get(cp) vs Zig's 2-stage table — same stages/types? bounds checks per get? rustty_simd::codepoint_width — is it actually SIMD on aarch64 or a scalar fallback (check the crate for neon intrinsics vs a ported-scalar path; the Zig side links a C++ Highway library!)? is-symbol table().get. Also compare the BENCH loops themselves (crates/rustty-bench/src/{codepoint_width,grapheme_break,is_symbol}.rs read_slice_short chunking) vs the Zig benchmark loops — any per-chunk overhead difference. Rank top 3 causes.`,
    { label: 'diag:unicode-family', phase: 'Diagnose', agentType: 'general-purpose', schema: F_SCHEMA }),

  () => agent(
    `${RULES}\n\nAREA: PERF(port) debt audit. PORT_RULES R10 mandates: every Zig perf idiom downgraded during porting is marked "// PERF(port): <zig idiom>". Run: grep -rn "PERF(port)" ${W}/crates/rustty-vt/src ${W}/crates/rustty-unicode/src ${W}/crates/rustty-simd/src ${W}/crates/rustty-datastruct/src ${W}/crates/rustty-zigstd/src ${W}/crates/rustty-bench/src (and any other crate on the bench paths). For EACH marker found: read its context, classify whether it lies on one of the 4 hot bench paths (terminal-parser per-byte, osc per-byte, screen-clone/render, utf8-decode+property-lookup) or the pagelist alloc path, and rate expected impact if resolved. Common downgrades to look for: appendAssumeCapacity->push (capacity re-check), assumeCapacity patterns, @setRuntimeSafety off blocks ported with checks on, fixed-buffer->Vec, comptime-unrolled->runtime loop, packed-struct bit tricks->safe accessors. Also grep "PORT NOTE" in the same hot files for sanctioned reshapes that may have perf cost. Return every hot-path marker with file:line, the downgraded idiom, the restoration fix, impact, parity risk. If there are more than ~30 markers total, focus exclusively on the hot paths and say how many you skipped.`,
    { label: 'diag:perf-port-audit', phase: 'Diagnose', agentType: 'general-purpose', schema: F_SCHEMA }),
])

const good = findings.filter(Boolean)
log(`Diagnose done: ${good.length}/6 areas returned. Total root causes: ${good.reduce((n, f) => n + f.root_causes.length, 0)}`)

phase('Synthesize')
const S_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['doc_path', 'fix_sequence', 'summary'],
  properties: {
    doc_path: { type: 'string' },
    summary: { type: 'string' },
    fix_sequence: {
      type: 'array',
      items: {
        type: 'object', additionalProperties: false,
        required: ['rank', 'id', 'title', 'targets', 'expected_impact', 'parity_risk', 'effort'],
        properties: {
          rank: { type: 'number' },
          id: { type: 'string', description: 'short slug, e.g. fix-parser-table' },
          title: { type: 'string' },
          targets: { type: 'array', items: { type: 'string' }, description: 'which bench ids this should move' },
          expected_impact: { type: 'string' },
          parity_risk: { type: 'string' },
          effort: { type: 'string', description: 'S|M|L' },
        },
      },
    },
  },
}

const synth = await agent(
  `${RULES}\n\nSYNTHESIZE. Here are the structured findings from 6 parallel diagnoses:\n\n${JSON.stringify(good, null, 2)}\n\nWrite ${PLAN}: a ranked, dependency-aware fix plan. Rules for ranking: (impact x confidence) / effort, with parity_risk as a tie-breaker (lower risk first). Group into: TIER 0 = build-profile parity (already in flight separately — document expected coverage: which gaps it plausibly closes fully/partially, per the findings); TIER 1 = low-risk high-impact code fixes; TIER 2 = medium; TIER 3 = speculative/algorithmic (only if TIER 1+2 leave a gap). For each fix: exact files, the change, the parity argument (why observable behavior is unchanged), which tests/gates prove it, and which bench ids must move. Include a "measurement protocol" section: bench variants A(release)/B(release-fast)/C(rf+LTO) already built, then one bench run per accepted fix, sequential, idle machine only. Also include a DO-NOT list (fixes rejected for parity risk). Return doc_path, summary, and the fix_sequence array.`,
  { label: 'synthesize', phase: 'Synthesize', agentType: 'general-purpose', schema: S_SCHEMA })

return { findings: good, plan: synth }
