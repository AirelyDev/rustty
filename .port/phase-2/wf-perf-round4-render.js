export const meta = {
  name: 'perf-round4-render',
  description:
    'Round 4 — make screen-clone.render a CLEAR win (currently statistical parity: zig 15.7 vs rust 16.1 official, +1.2% at N=21). Deep structural hunt beyond the ABI-locked floor, including rustty-only internal reshapes (caching/memoization) that Zig does not exploit. Implement, gate. No official benches (parent runs idle-gated).',
  phases: [
    { title: 'Hunt', detail: 'deep profile + structural opportunity enumeration' },
    { title: 'Implement', detail: 'top 1-2 opportunities, sequential' },
    { title: 'Gate', detail: 'full gates + ship binary' },
  ],
}

const W = '/Users/gael/projects/rustty/.claude/worktrees/perf-opt'
const G = '/Users/gael/projects/ghostty'
const R = '/Users/gael/projects/rustty'
const COR = `${R}/.port/phase-0/baseline/corpora`

const CTX = `rustty perf round 4 — screen-clone.render ONLY. Worktree: ${W} branch worktree-perf-opt @ e320dcb. Oracle (READ-ONLY): ${G}. History: ${W}/.port/phase-2/PERF_LOG.md — READ the Round 2-3 sections first. Already landed on the render path: SoA columns (fix-render-soa), no per-rebuild re-zero (fix-render-undefined-cols), per-row raw copy_from_slice, managed-row-only style/grapheme writes, hoisted mutable column borrows (r3-render). Official: zig 15.7ms vs rust 16.1ms per 1000 updates (+2.0%); N=21 interleaved +1.2%, ranges fully overlap. Round-3 documented the residual as parity-locked ONLY for the current loop structure (caller-saved constant remat + by-value ABI on the RefCountedSet get path).

THE MANDATE CHANGED: statistical parity is NOT enough — the user wants a decisive WIN (target: >= 5% faster than Zig official). Since the current structure is at its floor, find STRUCTURAL opportunities — including optimizations Zig itself does NOT do. Sanctioned space per PORT_RULES R10: internal reshapes with byte-identical observable behavior, marked "// PORT NOTE (reshape)" / "PERF(port)"; pure caching/memoization of internal lookups is allowed (no observable difference); changing RenderState's PUBLIC read contract needs a consumer sweep (renderer crate) and stronger justification. FORBIDDEN: LTO (banned, measured), unsafe without machine-checked invariants, any observable behavior change, skipping work the reference semantically requires (results must be identical for ANY input, not just the bench corpus).

BENCH SHAPE (so you optimize the right thing): crates/rustty-bench/src/screen_clone.rs step_render does 1000x { state.screen = ScreenType::Alternate (alternates); state.update(alloc, &mut terminal) }. Terminal is 80x120(?) with EVERY cell styled (bg SGR) by setup. So per update: full viewport rebuild, all rows managed, every cell takes the styled path. Real-world signal: long runs of identical style_id are the COMMON case in real terminals (whole lines share one SGR), so same-style memoization is not bench-gaming — but it must also not REGRESS the mixed-style case.

CANDIDATE LIST (evaluate honestly, profile first, add your own):
1. Last-style memoization in the managed loop: cache (last_style_id, resolved Style/ptr) per row walk; on style_id == last, reuse — skips the RefCountedSet get + 28B copy per cell. Zig re-fetches per cell (render.zig:507-509). Pure internal cache, zero observable change. Expected: large on same-style runs, neutral-to-tiny-cost on alternating styles (one compare).
2. Styled-span batching: detect runs of cells with identical style_id and fill cells_style[a..b] with one resolved value (slice::fill or copy pattern); pairs with 1.
3. Fuse the raw copy + managed walk: currently copy_from_slice then re-walk page_cells for flags. A single pass may lose the memcpy advantage — measure, don't assume; possibly keep memcpy + walk a PRECOMPUTED bitset of flagged cells if the Row tracks one (check Row flags: does the reference maintain styled/grapheme row flags that bound which cells need the slow path? does rust check cell.style_id != DEFAULT per cell where a row-level early-out exists in zig or vice versa?).
4. The update() outer structure: what work repeats per update that is invariant across updates of the same screen (row_data sizing, page traversal setup)? Any Vec len churn per update? RenderState is reused across the 1000 iterations — confirm capacity is retained everywhere (row_data, grapheme buffers) and nothing is dropped/rebuilt per update.
5. The style RESOLUTION itself: RefCountedSet::get by id — is it a direct index (memory offset) or hash probe in rust? Compare codegen vs zig's styles.get. If rust pays an extra indirection/bounds check per get, hoist the set's base pointer once per page (like r3 hoisted the columns).
6. Prefetch/layout: cells_style stride (28B Style) vs zig — confirm equal; check whether writing style via ptr::write vs indexed store changed codegen.
Profile FIRST (xctrace Time Profiler + leaf-PC histogram diff on BOTH binaries: ${W}/.perf-bins/rustty-bench-r3-SHIP-RF and ${R}/.port/phase-0/baseline/bench-releasefast/bin/ghostty-bench, args: +screen-clone --mode=render --data=${COR}/ascii-small.bin), THEN pick. Non-official interleaved spot-timings for candidate validation are required before declaring a winner (report medians of >=7 interleaved pairs).`

const H_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['hotspots', 'opportunities', 'chosen', 'confidence'],
  properties: {
    hotspots: { type: 'array', items: { type: 'string' } },
    opportunities: { type: 'array', items: { type: 'string' }, description: 'ALL evaluated, with per-item verdict + expected gain' },
    chosen: { type: 'string', description: 'the 1-2 implementations to do, full spec with parity argument' },
    confidence: { type: 'string' },
  },
}

phase('Hunt')
const hunt = await agent(
  `${CTX}\n\nHUNT. Deliver: hotspots (instruction-level, both binaries, with sample shares), opportunities (each candidate above + your own, with measured/disasm evidence and an expected-gain estimate — kill candidates honestly), chosen (the spec for the top 1-2, with parity argument, exact files/functions, and the validation numbers you already collected if you prototyped). You MAY prototype in the worktree but leave the tree CLEAN (git checkout) before returning; report prototype numbers in 'chosen'.`,
  { label: 'hunt:render', phase: 'Hunt', agentType: 'general-purpose', schema: H_SCHEMA })

if (!hunt) { log('hunt failed'); return { hunt: null } }
log(`Hunt done. chosen: ${hunt.chosen.slice(0, 140)}...`)

phase('Implement')
const FIX_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['fix_id', 'disposition', 'commit', 'files_changed', 'gates', 'spot_timing', 'notes'],
  properties: {
    fix_id: { type: 'string' }, disposition: { type: 'string' }, commit: { type: 'string' },
    files_changed: { type: 'array', items: { type: 'string' } }, gates: { type: 'string' },
    spot_timing: { type: 'string', description: 'non-official interleaved medians: zig vs pre vs post' },
    notes: { type: 'string' },
  },
}
const impl = await agent(
  `${CTX}\n\nIMPLEMENT the hunt's chosen spec in ${W}:\n\n${hunt.chosen}\n\n(Full opportunity context: ${JSON.stringify(hunt.opportunities).slice(0, 3000)})\n\nRules: one commit (fix id r4-render-win). Preserve/carry all reference comments; PORT NOTE (reshape)/PERF(port) markers. MUST include a mixed-style regression guard: a unit test (or extend an existing render test) where adjacent cells have DIFFERENT style_ids and one where styles alternate per cell — outputs must be identical pre/post (write the test against the CURRENT code first if practical). Gates: cargo test -p rustty-vt (default + --features alloc-bench), cargo test -p rustty-renderer if RenderState surface touched, clippy --no-deps -D warnings, fmt --check. Validation: interleaved non-official spot-timing >=7 pairs zig vs pre (r3-SHIP-RF) vs post; report medians. If post does not beat pre beyond noise, 'reverted: no-gain' and leave the tree clean. Then stash: cargo build -p rustty-bench --profile release-fast --bin rustty-bench && cp target/release-fast/rustty-bench .perf-bins/rustty-bench-r4-render. Commit trailer:\n"\\n\\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>\\nClaude-Session: https://claude.ai/code/session_01PY4UZsF6TuQWkJwbbA8JgC"`,
  { label: 'impl:r4-render', phase: 'Implement', agentType: 'general-purpose', schema: FIX_SCHEMA })
log(`impl: ${impl ? impl.disposition + ' | spot: ' + (impl.spot_timing || '').slice(0, 120) : 'failed'}`)

phase('Gate')
const M_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['gates_green', 'final_commit', 'gate_report'],
  properties: { gates_green: { type: 'boolean' }, final_commit: { type: 'string' }, gate_report: { type: 'string' } },
}
const gate = await agent(
  `${CTX}\n\nFINAL GATE. In ${W}: cargo build --workspace --release; cargo test --workspace (debug lane canonical); cargo test -p rustty-vt --features alloc-bench; clippy on touched crates (--no-deps -D warnings); fmt --check; check_acyclic.sh; check_comment_coverage.py (baseline: cli 2 / inspector 22 / vt 2 — only NEW misses count). Rebuild + stash .perf-bins/rustty-bench-r4-SHIP-RF. Report verbatim.`,
  { label: 'gate', phase: 'Gate', agentType: 'general-purpose', schema: M_SCHEMA })

return { hunt, impl, gate }
