export const meta = {
  name: 'perf-round3',
  description:
    'Round 3 — flip the last 2 losses: screen-clone.clone (zig 51.3 vs rust 56.7, +10.5%) and screen-clone.render (zig 15.7 vs rust 17.0, +8.6%). Profile the r2 ship binary, implement targeted parity-safe fixes, gate. NO official benches (parent runs them idle-gated).',
  phases: [
    { title: 'Profile', detail: '2 parallel hotspot hunts on r2-SHIP-RF' },
    { title: 'Implement', detail: 'sequential targeted fixes' },
    { title: 'Gate', detail: 'full gates + ship binaries' },
  ],
}

const W = '/Users/gael/projects/rustty/.claude/worktrees/perf-opt'
const G = '/Users/gael/projects/ghostty'
const R = '/Users/gael/projects/rustty'
const COR = `${R}/.port/phase-0/baseline/corpora`

const CTX = `rustty perf round 3 (final losses). Worktree: ${W} branch worktree-perf-opt @ 3f9f91c. Reference (READ-ONLY): ${G}. Read ${W}/.port/phase-2/PERF_LOG.md for rounds 1-2 history (what already landed: SoA render + no-re-zero columns, packed Style::eql + coalesced cell copy in clone, boxed Action, osc fixed capture, inline idioms...).

OFFICIAL STANDINGS after round 2 (idle, N=7): 12 WINS / 2 LOSSES vs Zig; suite -10.0%.
Remaining losses (both screen-clone family, ascii-small corpus, 1000 iterations per step):
- screen-clone.clone: zig 51.3ms vs rust 56.7ms (+10.5%) — round 2 took it 81->57 (packed Style::eql + coalesced copy). ~5.4ms/1000-clones gap left.
- screen-clone.render: zig 15.7ms vs rust 17.0ms (+8.6%) — rounds 1-2 took it 31.4->17.0 (SoA + no-re-zero). ~1.3ms/1000-updates gap left.

RULES: functional parity NON-NEGOTIABLE (R1; tests unchanged; reference comments preserved; PERF(port)/PORT NOTE markers). Global LTO banned. unsafe only with provable bounds or machine-checked invariant tests. NO official benches — profiling + interleaved non-official spot-timings allowed (mark them). Profilers: /usr/bin/xctrace (Time Profiler), /usr/bin/sample. Binary: ${W}/.perf-bins/rustty-bench-r2-SHIP-RF. Zig comparison binary: ${R}/.port/phase-0/baseline/bench-releasefast/bin/ghostty-bench. Invocations: +screen-clone --mode=clone|render --data=${COR}/ascii-small.bin.
IMPORTANT calibration: the remaining gaps are SMALL (5.4ms and 1.3ms per 1000 iterations = 5.4us and 1.3us per op). Profile BOTH rust and zig binaries and diff the per-symbol time — the fix must target where rust spends time zig does not. If after honest profiling the remaining gap is structural-but-parity-locked (e.g. a locked decision like Command's owned Vecs, or allocator-call asymmetry that geometry rules forbid touching), SAY SO explicitly in the findings with the evidence — do not force a risky change.`

const P_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['area', 'hotspots', 'fix_spec', 'confidence'],
  properties: {
    area: { type: 'string' },
    hotspots: { type: 'array', items: { type: 'string' } },
    fix_spec: { type: 'string', description: "concrete spec, or 'NONE-SAFE: <why>' if no parity-safe fix remains" },
    confidence: { type: 'string' },
  },
}

phase('Profile')
const profs = await parallel([
  () => agent(
    `${CTX}\n\nAREA: screen-clone.clone (+10.5%, 5.4us/clone). Profile rust r2-SHIP-RF AND zig ghostty-bench on '+screen-clone --mode=clone' (loop externally for sampling; xctrace both). Diff per-symbol self-time. Round 2 already fixed Style::eql (packed compare) + the cell copy loop. Remaining suspects from prior analysis: RefCountedSet add_with_id chain (hash + probe per styled cell — compare hash fn + probe codegen vs Zig's), per-page mmap/pool ops per clone (clone creates its own pool: preheat behavior vs Zig MemoryPool), hyperlink/grapheme adoption checks per cell, the tracked_pins remap walk, memcpy width. Deliver fix_spec with parity argument, or NONE-SAFE with evidence.`,
    { label: 'prof3:clone', phase: 'Profile', agentType: 'general-purpose', schema: P_SCHEMA }),
  () => agent(
    `${CTX}\n\nAREA: screen-clone.render (+8.6%, 1.3us/update). Profile rust r2-SHIP-RF AND zig on '+screen-clone --mode=render'. Diff per-symbol self-time. Already landed: SoA columns, no per-rebuild re-zero, raw copy_from_slice per row, managed-row-only style/grapheme writes. Remaining suspects: per-row Vec len/resize bookkeeping vs Zig MultiArrayList.resize (setCapacity path), the style map lookup per managed cell (RefCountedSet get vs Zig), dirty-bitset iteration, the update() entry overhead (RenderState fields reset), bounds checks in the row loop the hoisting missed. Deliver fix_spec with parity argument, or NONE-SAFE with evidence.`,
    { label: 'prof3:render', phase: 'Profile', agentType: 'general-purpose', schema: P_SCHEMA }),
])

const good = profs.filter(Boolean)
log(`Profile: ${good.map(p => `${p.area.split(' ')[0]}: ${p.fix_spec.startsWith('NONE-SAFE') ? 'NONE-SAFE' : 'fix found'} (${p.confidence.split(' ')[0]})`).join(' | ')}`)

phase('Implement')
const FIX_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['fix_id', 'disposition', 'commit', 'files_changed', 'gates', 'notes'],
  properties: {
    fix_id: { type: 'string' }, disposition: { type: 'string' }, commit: { type: 'string' },
    files_changed: { type: 'array', items: { type: 'string' } }, gates: { type: 'string' }, notes: { type: 'string' },
  },
}
const IMPL = `${CTX}\n\nIMPLEMENT exactly your assigned fix in ${W}. Gates: cargo test -p <touched> (+rustty-vt suite when touching vt; also --features alloc-bench lane); clippy --no-deps -D warnings; fmt --check. Revert your files if parity or gates can't hold ('reverted: <why>'). Commit "rustty: perf — <fix-id>: <title>" + trailer "\\n\\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>\\nClaude-Session: https://claude.ai/code/session_01PY4UZsF6TuQWkJwbbA8JgC". Then stash: cargo build -p rustty-bench --profile release-fast --bin rustty-bench && cp target/release-fast/rustty-bench .perf-bins/rustty-bench-r3-<fix-id>. Spot-time interleaved vs .perf-bins/rustty-bench-r2-SHIP-RF (non-official) and report the numbers.`

const results = []
for (const p of good) {
  if (p.fix_spec.startsWith('NONE-SAFE')) { log(`skip (none-safe): ${p.area.split(' ')[0]}`); results.push({ fix_id: p.area, disposition: 'none-safe', commit: '', files_changed: [], gates: '', notes: p.fix_spec }); continue }
  const id = p.area.toLowerCase().includes('render') ? 'r3-render' : 'r3-clone'
  const r = await agent(`${IMPL}\n\nYOUR FIX: ${id}.\nHOTSPOTS:\n${p.hotspots.join('\n')}\n\nSPEC:\n${p.fix_spec}`,
    { label: `impl:${id}`, phase: 'Implement', agentType: 'general-purpose', schema: FIX_SCHEMA })
  results.push(r)
  log(`${id}: ${r ? r.disposition + ' ' + (r.commit || '') : 'failed'}`)
}

phase('Gate')
const M_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['gates_green', 'final_commit', 'gate_report'],
  properties: { gates_green: { type: 'boolean' }, final_commit: { type: 'string' }, gate_report: { type: 'string' } },
}
const gate = await agent(
  `${IMPL}\n\nFINAL GATE (no fix; verify + build). In ${W}: cargo build --workspace --release; cargo test --workspace (debug lane); cargo test -p rustty-vt --features alloc-bench; clippy (touched crates, --no-deps -D warnings); fmt --check (touched); check_acyclic.sh; check_comment_coverage.py (baseline cli 2 / inspector 22 / vt 2 — only NEW counts). Build + stash .perf-bins/rustty-bench-r3-SHIP-RF and r3-SHIP-A (release profile). Report.`,
  { label: 'gate', phase: 'Gate', agentType: 'general-purpose', schema: M_SCHEMA })

return { profiles: good, fixes: results, gate }
