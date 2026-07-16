export const meta = {
  name: 'perf-round5-broad',
  description:
    'Round 5 — push EVERY bench much further, including the ones Rust already wins. 4 parallel file-disjoint lanes in separate worktrees (parser/stream, decode family, osc, clone-chain), each: deep hunt -> implement -> gate. All absolute-timing runs serialized via an atomic mkdir lock. Merge lanes into perf-opt at the end. Official benches happen later (parent, idle-gated).',
  phases: [
    { title: 'Lanes', detail: '4 parallel hunt+implement lanes in own worktrees' },
    { title: 'Merge', detail: 'sequential merges into perf-opt + full gates' },
  ],
}

const W = '/Users/gael/projects/rustty/.claude/worktrees/perf-opt'
const G = '/Users/gael/projects/ghostty'
const R = '/Users/gael/projects/rustty'
const COR = `${R}/.port/phase-0/baseline/corpora`
const LOCK = '/Users/gael/.claude/jobs/22556231/tmp/bench.lock'

const BASE = `rustty perf round 5 — broad improvement hunt. Base branch: worktree-perf-opt (may advance while you work; you branch from its CURRENT head when you create your worktree). Oracle (READ-ONLY): ${G}. History (MUST read first): ${W}/.port/phase-2/PERF_LOG.md — 3 rounds already landed; current official standings: 13 wins / 1 parity / 0 losses vs Zig, suite -11.1%.

MISSION: the user wants MUCH more improvement on your lane's benches even though Rust already wins most. Target: >=10% further improvement vs the current rust numbers on at least one of your benches, without regressing ANY other bench. Structural rustty-only optimizations Zig does not do are ENCOURAGED (sanctioned space: internal reshapes with byte-identical observable behavior for ALL inputs, marked PORT NOTE (reshape)/PERF(port); public-surface changes need a full consumer sweep). If after honest profiling your lane is at a true floor, return NONE-SAFE with instruction-level evidence — do not force noise-level or risky changes.

HARD RULES: functional parity NON-NEGOTIABLE (all tests green; never change what a test asserts; reference comments preserved/carried). The BENCH crate code (crates/rustty-bench) is itself a faithful port — do NOT reshape bench loops/semantics; optimize the LIBRARY code it calls. Global LTO banned (measured pessimization, PERF_LOG). No unsafe without compiler-provable bounds or a machine-checked invariant test. Zig byte-slices stay &[u8]/Vec<u8>.

BENCHMARK SYNCHRONIZATION (multiple lanes run concurrently): before ANY absolute-timing run (spot timings, interleaved validation), acquire the global lock:
  while ! mkdir ${LOCK} 2>/dev/null; do sleep 5; done
  # ... your timing runs, keep the critical section SHORT (<3 min) ...
  rmdir ${LOCK}
ALWAYS release (trap 'rmdir ${LOCK} 2>/dev/null' EXIT in the same shell). Sampling profiles (xctrace/sample) measure relative shares and do NOT need the lock. NO official N=7 protocol runs — the parent does those later.

WORKTREE PROTOCOL: create your own worktree FIRST: cd ${W} && git worktree add ${R}/.claude/worktrees/<your-lane-name> -b <your-branch-name> && cd <worktree>. ALL edits/commits there. Gates before each commit: cargo test -p <touched crates> (rustty-vt also with --features alloc-bench), clippy --no-deps --all-targets -- -D warnings, fmt --check. Commit message "rustty: perf — <fix-id>: <title>" + trailer "\\n\\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>\\nClaude-Session: https://claude.ai/code/session_01PY4UZsF6TuQWkJwbbA8JgC". Build validation binaries inside YOUR worktree (cargo build -p rustty-bench --profile release-fast) and spot-time (locked!) against ${W}/.perf-bins/rustty-bench-r3-SHIP-RF (pre-round baseline) and the Zig binary ${R}/.port/phase-0/baseline/bench-releasefast/bin/ghostty-bench. Corpora: ${COR}/. Leave your worktree committed and clean.`

const L_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['lane', 'worktree', 'branch', 'commits', 'disposition', 'spot_timing', 'opportunities_evaluated', 'notes'],
  properties: {
    lane: { type: 'string' },
    worktree: { type: 'string' },
    branch: { type: 'string' },
    commits: { type: 'array', items: { type: 'string' } },
    disposition: { type: 'string', description: "'landed' | 'none-safe: <evidence>' | 'reverted: <why>'" },
    spot_timing: { type: 'string', description: 'locked interleaved medians: zig vs pre vs post per bench' },
    opportunities_evaluated: { type: 'array', items: { type: 'string' } },
    notes: { type: 'string' },
  },
}

phase('Lanes')
const lanes = await parallel([
  () => agent(
    `${BASE}\n\nLANE A — parser + stream (worktree perf-lane-parser, branch worktree-perf-lane-parser). Benches: terminal-parser (zig 266.0 / rust 210.6, -20.8%) and terminal-stream (zig 566.2 / rust 460.8, -18.6%). Push both further. Landed already: inline idioms, destage, boxed Action + InlineVec + state hoist (read the PERF_LOG r2 entry + parser.rs as-is). Candidate directions (profile first, add your own): (1) fused transition table — precompute a single [u16; 256*16] (or [u8;4096]) table keyed by (state<<8)|byte yielding packed (next_state, action) so next() does ONE load instead of class-lookup + transition-lookup + unpack; measure table-cache pressure honestly. (2) branchless Ground print fast-path inside next() (most bytes: table says print/none — collapse to early return before the match dispatch). (3) terminal-stream: profile where its time goes NOW (stream scan? Terminal::print? width lookup? grapheme state machine?) — the print path calls codepoint width + grapheme break per char; check for redundant per-char work vs ghostty stream.zig/Terminal.print (e.g. style/dirty bookkeeping per char that Zig batches). (4) check ghostty's src/simd usage on the stream path (simdvt): does the reference SIMD-scan plain-text runs where rustty scans scalar? If rustty downgraded a SIMD idiom, restoring it is R10-sanctioned and potentially huge. Files likely: crates/rustty-vt/src/{parser.rs,stream.rs,terminal.rs}.`,
    { label: 'lane:parser-stream', phase: 'Lanes', agentType: 'general-purpose', schema: L_SCHEMA }),

  () => agent(
    `${BASE}\n\nLANE B — decode family (worktree perf-lane-unicode, branch worktree-perf-lane-unicode). Benches (7 lanes, all currently -4.5..-15.1% vs zig): codepoint-width.{noop,table,simd,wcwidth}, grapheme-break.{noop,table}, is-symbol.{table,uucode}. ALL share UTF8Decoder::next per byte + a property lookup per codepoint; the decode-only noop lanes bound the shared cost. Candidate directions: (1) fuse the DFA: precompute a single [u8; 256*8] table keyed by (state_index<<8)|byte -> next_state, eliminating the two-load class+transition chain (states are multiples of 12 today — compact to 0..7 indices internally, keep the public (Option<u32>, bool) contract bit-identical incl. REJECT behavior; exhaustive test vs the old DFA over all (state,byte) pairs). (2) branchless codepoint accumulation (mask-select instead of the class-dependent branch, if any remains). (3) property tables: check stage-table element widths/locality vs zig lut.zig; consider fusing stage1+stage2 for the hot BMP range (cp<=0xFFFF direct table? memory cost analysis required — only if <=256KiB and measured faster). (4) is-symbol/grapheme-break: the per-cp call chain post-#[inline] — confirm fully inlined in release-fast disasm; kill any residual outlined calls. Files: crates/rustty-vt/src/utf8_decoder.rs, crates/rustty-unicode/src/{lut.rs,props_table.rs,grapheme.rs,symbols_table.rs}. Do NOT touch the vendored Highway FFI (.simd lane inner call is verbatim-identical both sides — verified).`,
    { label: 'lane:decode', phase: 'Lanes', agentType: 'general-purpose', schema: L_SCHEMA }),

  () => agent(
    `${BASE}\n\nLANE C — osc-parser (worktree perf-lane-osc, branch worktree-perf-lane-osc). Bench: zig 57.4 / rust 48.5 (-15.6%). Push toward -25%+. Landed already: zero-alloc fixed capture, writeByte fall-through, inline attrs (read PERF_LOG). The bench replays length-prefixed OSC records: per record ~ Parser::reset + next(c) per byte + end(BEL). Candidate directions (profile first): (1) reset() cost per record — what does it clear? any O(buffer) work that could be O(1) (generation counter instead of memset, if observably identical)? (2) the state-machine dispatch in next(): first-byte command-number parsing, memcmp-style prefix matching — any per-byte scalar compares that could table-dispatch? (3) end() -> Command construction: remaining copies/temporaries; the parsers/* modules' trailing() consumers. (4) the capture write path: current bounds+store per byte — could the common capture state write via a raw cursor (provable bounds from MAX_BUF invariant + unit test)? Files: crates/rustty-vt/src/osc/mod.rs + osc/parsers/*.`,
    { label: 'lane:osc', phase: 'Lanes', agentType: 'general-purpose', schema: L_SCHEMA }),

  () => agent(
    `${BASE}\n\nLANE D — clone chain (worktree perf-lane-clone, branch worktree-perf-lane-clone). Bench: screen-clone.clone zig 51.6 / rust 50.5 (-2.1%, the thinnest win) + screen-clone.noop. Goal: decisive margin (-10%+). The round-3 profile documented a residual floor: (a) Rust marshals the 28-byte Style BY VALUE per RefCountedSet::add_with_id call (stack q-register traffic) where Zig passes by const reference; (b) caller-saved constant rematerialization across that call. THE FLOOR IS NOT LOCKED: the reference's PUBLIC surface add_with_id(value: T) must stay, but an INTERNAL by-ref fast path is a sanctioned reshape — e.g. #[inline] pub fn add_with_id(&mut self, base, value: T, id) { self.add_with_id_ref(base, &value, id) } with the real body taking &T; same for lookup/eql chain. Verify codegen actually stops the marshal (disasm). Also: (c) the same treatment on hyperlink_set add path; (d) profile what ELSE remains per styled cell in clone_partial_row_from_inner post-r3 (the outlined cold arms landed; the hot style arm inline body); (e) page-level: clone_from's region memcpys — widths/granularity vs zig fastmem.copy. Files: crates/rustty-vt/src/{ref_counted_set.rs,page_impl.rs,hash_map.rs?}. CAUTION: ref_counted_set is used by style AND hyperlink sets — full vt suite + Miri-sensitive raw-ptr discipline (read the file's PORT NOTEs; no new aliasing).`,
    { label: 'lane:clone', phase: 'Lanes', agentType: 'general-purpose', schema: L_SCHEMA }),
])

const good = lanes.filter(Boolean)
log(`Lanes done: ${good.map(l => `${l.lane}=${l.disposition.split(':')[0]}`).join(' ')}`)

phase('Merge')
const M_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['merged', 'gates_green', 'final_commit', 'gate_report'],
  properties: {
    merged: { type: 'array', items: { type: 'string' } },
    gates_green: { type: 'boolean' },
    final_commit: { type: 'string' },
    gate_report: { type: 'string' },
  },
}
const merge = await agent(
  `${BASE}\n\nMERGE + GATE (no new optimization work). In ${W} (branch worktree-perf-opt):\n1. Lane results: ${JSON.stringify(good.map(l => ({ lane: l.lane, branch: l.branch, disposition: l.disposition, commits: l.commits })))}\n2. For each lane with disposition 'landed': git merge <branch> -m "rustty: perf — merge <lane> lane (round 5)" (+ trailer). Merge ONE AT A TIME, running cargo test -p rustty-vt -p rustty-unicode between merges; resolve only trivial conflicts (disjoint files expected; if two lanes touched the same hunk, prefer keeping BOTH changes if compatible, else abort that merge and report it unmerged).\n3. Full gates after all merges: cargo build --workspace --release; cargo test --workspace (debug lane); cargo test -p rustty-vt --features alloc-bench; clippy on all touched crates (--no-deps -D warnings); fmt --check; check_acyclic.sh; check_comment_coverage.py (baseline cli 2 / inspector 22 / vt 2).\n4. Build + stash ${W}/.perf-bins/rustty-bench-r5-SHIP-RF (release-fast) and r5-SHIP-A (release).\n5. Remove merged lane worktrees: git worktree remove <path> --force for each successfully merged lane.\nReport verbatim per-gate results.`,
  { label: 'merge+gate', phase: 'Merge', agentType: 'general-purpose', schema: M_SCHEMA })

return { lanes: good, merge }
