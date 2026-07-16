export const meta = {
  name: 'perf-fixes',
  description:
    'Implement the ranked PERF_FIX_PLAN fixes: a sequential chain (parser/osc/unicode/pagelist, overlapping files) in the perf-opt worktree in parallel with the render-SoA fix (file-disjoint) in the perf-render worktree; merge, gate, and stash per-fix bench binaries. NO benchmark runs (timing work happens later on an idle machine).',
  phases: [
    { title: 'Implement', detail: 'sequential fix chain || render-SoA worktree' },
    { title: 'MergeGate', detail: 'merge render branch, full workspace gates, final binaries' },
  ],
}

const W = '/Users/gael/projects/rustty/.claude/worktrees/perf-opt'
const WR = '/Users/gael/projects/rustty/.claude/worktrees/perf-render'
const G = '/Users/gael/projects/ghostty'
const PLAN = `${W}/.port/phase-2/PERF_FIX_PLAN.md`

const RULES = `You are implementing ONE performance fix in rustty (full-fidelity Zig->Rust port of Ghostty). Reference oracle (READ-ONLY): ${G}. The complete ranked fix plan with exact files/lines/parity arguments is at ${PLAN} — READ IT FIRST, then implement ONLY your assigned fix, exactly as specified there (the plan's evidence is disassembly-verified; trust its line references but re-verify against the current file state since earlier fixes in the chain may have shifted lines).

NON-NEGOTIABLE RULES:
- Functional parity (R1): observable behavior must be identical to the Zig reference. ALL existing tests must stay green. NEVER change what a test asserts.
- Preserve every existing reference comment (the // <zig code> comments). Your changes ADD code/attributes; they must not delete reference comments. If you restructure (sanctioned reshape), carry every reference comment to the corresponding new location.
- Mark every perf-motivated change per R10: "// PERF(port): <what Zig idiom is being restored / why>" or for structural reshapes "// PORT NOTE (reshape): <reason>" as the plan specifies per fix.
- NO benchmark runs. NO touching ${W}/.perf-bins/rustty-bench-{A-release,B-rf,C-rf-lto} (frozen baselines).
- Zig byte-slices stay &[u8]/Vec<u8>, never lossy String.

PER-FIX GATE (run all, in your fix's worktree):
1. cargo test -p <every crate you touched> (and -p rustty-vt if you touched it)
2. cargo clippy -p <touched crates> --no-deps --all-targets -- -D warnings
3. cargo fmt -p <touched crates> -- --check (run cargo fmt -p <crate> to fix)
If a gate fails and you cannot fix it while keeping parity: git checkout -- <files> (revert your changes), and report disposition 'reverted' with the exact failure. Do not leave the tree broken.

ON SUCCESS: git add <your files> && git commit with message format:
"rustty: perf — <fix-id>: <title>\n\n<2-5 lines: what Zig idiom restored / what overhead removed, plan reference, gates run>\n\nCo-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>\nClaude-Session: https://claude.ai/code/session_01PY4UZsF6TuQWkJwbbA8JgC"
Commit ONLY your own files. Never commit .perf-bins or target.`

const FIX_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['fix_id', 'disposition', 'commit', 'files_changed', 'gates', 'notes'],
  properties: {
    fix_id: { type: 'string' },
    disposition: { type: 'string', description: "'committed' | 'reverted: <why>' | 'skipped: <why>'" },
    commit: { type: 'string', description: 'short SHA or empty' },
    files_changed: { type: 'array', items: { type: 'string' } },
    gates: { type: 'string', description: 'exact gate commands run + results' },
    notes: { type: 'string' },
  },
}

// Sequential chain in perf-opt: overlapping files (parser.rs, osc/, utf8_decoder.rs, unicode, pagelist).
// Order: profile commit first, then quick wins, then M-effort, destage LAST (depends on inline landing).
const CHAIN = [
  {
    id: 'fix-profile-lto',
    prompt: `Fix: TIER-0 build-profile parity, formalized. In ${W}/Cargo.toml, extend [profile.release-fast] (currently inherits release + overflow-checks=false) with:\nlto = "fat"\ncodegen-units = 1\npanic = "abort"\nKeep the existing comment and ADD a comment line explaining: release-fast mirrors Zig ReleaseFast (whole-program LTO, single unit, aborting panics — the Bun-in-Rust cross-language-LTO learning; Goal-2 bench profile only, tests keep [profile.release]'s unwind). Gate: cargo build -p rustty-bench --profile release-fast --bin rustty-bench must succeed. Then commit (just Cargo.toml).`,
  },
  {
    id: 'fix-inline-idioms',
    prompt: `Fix: plan TIER 1 item 1 "fix-inline-idioms" — restore every dropped Zig inline hint. Files: ${W}/crates/rustty-vt/src/parser.rs (#[inline] on next; #[inline(always)] on do_action/collect/clear/stage_to_action), ${W}/crates/rustty-vt/src/osc/mod.rs (#[inline(always)] on Parser::next; #[inline] on ensure_allocator/capture_trailing/Capture::write_byte), ${W}/crates/rustty-vt/src/utf8_decoder.rs (#[inline] -> #[inline(always)] on next), ${W}/crates/rustty-unicode/src (add #[inline] to grapheme_break, props_uucode::get, the symbols_table/symbols_uucode getters, props_table::table). Each site gets the PERF(port) marker citing the Zig inline fn / @call(.always_inline) reference location per the plan. Touched crates: rustty-vt, rustty-unicode.`,
  },
  {
    id: 'fix-utf8-dfa',
    prompt: `Fix: plan TIER 1 item 2 "fix-utf8-dfa" — elide per-byte TRANSITIONS bounds/overflow checks in ${W}/crates/rustty-vt/src/utf8_decoder.rs. (a) Pad TRANSITIONS [u8;108] -> [u8;256], entries 108..=255 = 12 (REJECT), keeping the original 108 entries byte-identical (preserve the reference table comment block exactly). (b) index as self.state as usize + char_class as usize. Add a unit test asserting entries 0..108 equal the reference values (embed the original 108-entry array in the test) and that all pad entries are 12. PERF(port) marker per plan. Touched: rustty-vt.`,
  },
  {
    id: 'fix-bench-assert-tier',
    prompt: `Fix: plan TIER 1 item 4 "fix-bench-assert-tier" — in ${W}/crates/rustty-bench/src/{codepoint_width.rs,is_symbol.rs,grapheme_break.rs}: change the per-byte assert!(consumed) sites to debug_assert!(consumed) (the exact tier of Zig std.debug.assert: live in Debug, absent in release). Do NOT add assert_unchecked (plan's optional extension deferred). Keep the reference comment lines (// assert(consumed);) exactly. PERF(port) marker at each site. Touched: rustty-bench.`,
  },
  {
    id: 'fix-pin-keys-collect',
    prompt: `Fix: plan TIER 1 item 5 "fix-pin-keys-collect" — ${W}/crates/rustty-vt/src/pagelist/access.rs (~line 876, inside the clone chunk loop): drop the per-chunk "let pin_keys: Vec<*mut Pin> = self.tracked_pins.keys().copied().collect()" and iterate self.tracked_pins.keys() directly, matching PageList.zig:866 and the 6 sibling sites (grow.rs/erase.rs) that already iterate directly. Check the plan's note about pin.rs:108-110 too. Borrow-check note: self is &self there and the loop mutates only local pool/remap. PERF(port) marker. Touched: rustty-vt.`,
  },
  {
    id: 'fix-remap-hasher',
    prompt: `Fix: plan TIER 2 item 11 "fix-remap-hasher" — ${W}/crates/rustty-vt/src/pagelist/access.rs:~721: the TrackedPinsRemap map keyed by *mut Pin currently uses default SipHash RandomState; swap to a pointer-identity/integer hasher (BuildHasherDefault over a small local u64-mix hasher — check whether rustty-zigstd already ports Zig AutoHashMap's hash and reuse it if so; otherwise a ~10-line local PtrHasher). Keep the type alias so call sites in screen.rs are untouched. Reference: PageList.zig:778 std.AutoHashMap(*Pin,*Pin). PERF(port) marker. Touched: rustty-vt (maybe rustty-zigstd read-only).`,
  },
  {
    id: 'fix-osc-fixed-capture',
    prompt: `Fix: plan TIER 1 item 3 "fix-osc-fixed-capture" — restore the zero-alloc fixed-writer-over-self.buffer OSC capture. This is the M-effort fix; follow the plan section EXACTLY: ${W}/crates/rustty-vt/src/osc/mod.rs (Capture stores a running len; writes go into the Parser's existing inline buffer: [u8; MAX_BUF] field in Parser::next; len>=MAX_BUF -> state=Invalid exactly as osc.zig's fixed writer overflow; trailing()/trailing_mut() become &self.buffer[..len] accessors threaded through Parser) + update the ~15 osc/parsers/* readers of trailing()/trailing_mut(). CRITICAL parity pins: the change_window_title boundary tests (longer-than/exactly-at/one-short-of buffer) and rxvt_extension's in-place NUL stamping via trailing_mut. Preserve ALL reference comments; PERF(port) marker per plan. Run the FULL rustty-vt test suite. Touched: rustty-vt.`,
  },
  {
    id: 'fix-color-list-prealloc',
    prompt: `Fix: plan TIER 2 item 10 "fix-color-list-prealloc" — ${W}/crates/rustty-vt/src/osc/parsers/color.rs:~417: replace pub type List = Vec<Request> with a 2-slot-inline small-vector (local enum {Inline([Request;2],u8), Heap(Vec<Request>)} with push/iter/len/as_slice API compatible with existing call sites; check first whether the workspace already has a sanctioned SmallVec/SegmentedList port in rustty-datastruct or rustty-zigstd and reuse it if the API fits). Reference rationale: color.zig:332 SegmentedList(Request,2) "large majority of the time only one operation per OSC" — that rationale comment is already carried at osc/mod.rs:91-95. PERF(port) marker. Touched: rustty-vt (+datastruct if reused).`,
  },
  {
    id: 'fix-lut-bounds',
    prompt: `Fix: plan TIER 2 item 8 "fix-lut-bounds" — ${W}/crates/rustty-unicode/src/lut.rs:~226-231: (1) change TablesRef to hold &'static [u16; N] fixed-size array references (the generated props_data.rs constants are already fixed-size arrays — verify and cite sizes) so stage2/stage3 bounds checks vanish; (2) mask the codepoint to the reference's u21 domain (cp & 0x1F_FFFF) before stage1 indexing, exactly matching Zig lut.zig:141-145's u21 parameter type (the Rust port widened to u32 — that widening made the range unprovable). Verify stage1 array length covers the full u21>>8 range (8192 = 0x200000>>8 — confirm). Add/keep a unit test that the getter returns identical values pre/post change for a sweep of codepoints incl. 0x10FFFF and the mask boundary. PERF(port) marker. Touched: rustty-unicode.`,
  },
  {
    id: 'fix-parser-destage',
    prompt: `Fix: plan TIER 2 item 6 "fix-parser-destage" — restore the reference arm structure in ${W}/crates/rustty-vt/src/parser.rs (the Staged/stage_to_action layer). Follow the plan section EXACTLY: build Option<Action> directly in each exit/transition/entry arm; materialize &self.intermediates[..idx]/&self.params[..idx] slices (and owned-Vec copies) ONLY inside CsiDispatch/EscDispatch/DcsHook arms (mirroring Parser.zig:377-384/:398-403/:299-306); do_action returns Option<Action> directly; the deferred self.state write stays where it is; delete the now-dead Staged enum + stage_to_action. CARRY every reference comment to its new arm location (the arm-for-arm mapping). Mark "// PORT NOTE (reshape): restore reference arm structure (Parser.zig:267-310)" + PERF(port) where the plan says. If the borrow checker fights the direct-return shape, use the plan's fallback minimal variant (slice materialization moved behind match staged {CsiDispatch|EscDispatch|DcsHook}). Gate: ALL parser tests (csi_too_many_params / dcs_too_many_params boundary tests especially) + full rustty-vt suite. Touched: rustty-vt.`,
  },
]

phase('Implement')

async function runChain() {
  const results = []
  for (const fix of CHAIN) {
    const r = await agent(
      `${RULES}\n\nWORKTREE: ${W} (branch worktree-perf-opt). Your assigned fix: ${fix.id}.\n\n${fix.prompt}\n\nAfter the gates pass and you commit, ALSO build and stash the two attribution binaries for this stage:\ncd ${W} && cargo build -p rustty-bench --release --bin rustty-bench && cp target/release/rustty-bench .perf-bins/rustty-bench-${fix.id}-A && cargo build -p rustty-bench --profile release-fast --bin rustty-bench && cp target/release-fast/rustty-bench .perf-bins/rustty-bench-${fix.id}-C\n(Neither build output is committed.) Report the schema fields.`,
      { label: `impl:${fix.id}`, phase: 'Implement', agentType: 'general-purpose', schema: FIX_SCHEMA })
    results.push(r)
    if (r) log(`${fix.id}: ${r.disposition} ${r.commit || ''}`)
    else log(`${fix.id}: agent failed/skipped — chain continues`)
  }
  return results
}

const renderFix = () => agent(
  `${RULES}\n\nWORKTREE: ${WR} (branch worktree-perf-render — a SEPARATE worktree so you can work in parallel with the fix chain; do all work there). Your assigned fix: fix-render-soa (plan TIER 2 item 7) — restore the MultiArrayList SoA layout + raw-column memcpy in ${WR}/crates/rustty-vt/src/render.rs. Follow the plan section EXACTLY: split Row cells into parallel Vecs (raw: Vec<page::Cell> plain u64 copy via copy_from_slice; style: Vec<Style>; grapheme as per-row reusable buffer Vec<u32> + (offset,len), .clear() retaining capacity = the arena.reset(.retain_capacity) analogue, render.zig:469-471); style/grapheme written ONLY for flagged cells in the managed-row loop (the reference's undefined-unless-managed discipline, render.zig:491-505); hoist row/cells refs once per row and iterate with zip so no per-cell double indexing; expose a Cell-view accessor so consumers keep an equivalent API. Sweep ALL consumers of row_data/cells (grep the workspace — the plan names the renderer crate; check rustty-renderer, rustty-inspector, vt-c). The carried "~300% faster" reference comment at render.rs:545-551 describes THIS layout — keep it and mark "// PORT NOTE (reshape): restore MultiArrayList SoA (render.zig:187)". Compare against ${G}/src/terminal/render.zig THROUGHOUT. Gates: all ported render tests + cargo test -p rustty-vt + cargo build -p rustty-renderer (and any other consumer crate) + cargo test --workspace if feasible, clippy/fmt on touched crates. Commit in ${WR}. Do NOT build/stash bench binaries (merge happens later).`,
  { label: 'impl:fix-render-soa', phase: 'Implement', agentType: 'general-purpose', schema: FIX_SCHEMA })

const [chainResults, renderResult] = await parallel([runChain, renderFix])

const committed = (chainResults || []).filter(Boolean).filter(r => (r.disposition || '').startsWith('committed'))
log(`Chain: ${committed.length}/${CHAIN.length} committed. Render: ${renderResult ? renderResult.disposition : 'FAILED'}`)

phase('MergeGate')
const M_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['merge_ok', 'gates_green', 'final_commit', 'gate_report', 'binaries'],
  properties: {
    merge_ok: { type: 'boolean' },
    gates_green: { type: 'boolean' },
    final_commit: { type: 'string' },
    gate_report: { type: 'string' },
    binaries: { type: 'array', items: { type: 'string' } },
  },
}
const merge = await agent(
  `${RULES.replace('implementing ONE performance fix', 'finalizing the perf-fix branch')}\n\nMERGE + GATE. In ${W} (branch worktree-perf-opt):\n1. If the render fix committed (check: cd ${WR} && git log --oneline -3 — commits beyond 10cc614 mean yes), merge it: cd ${W} && git merge worktree-perf-render -m "rustty: perf — merge fix-render-soa branch". Resolve conflicts ONLY if trivial (disjoint-file merge expected); if non-trivial conflicts, abort the merge and report merge_ok=false with details.\n2. Full gates in ${W}: cargo build --workspace --release; cargo test --workspace (release profile is fine); cargo clippy -p rustty-vt -p rustty-unicode -p rustty-bench --no-deps --all-targets -- -D warnings; cargo fmt --check on touched crates; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_comment_coverage.py (the vt lane had 2 PRE-EXISTING missing comments at HEAD — only NEW regressions count against this branch; report the count delta).\n3. Build final binaries: cargo build -p rustty-bench --release && cp target/release/rustty-bench .perf-bins/rustty-bench-FINAL-A; cargo build -p rustty-bench --profile release-fast && cp target/release-fast/rustty-bench .perf-bins/rustty-bench-FINAL-C.\n4. Report: merge_ok, gates_green (true only if everything is green modulo the documented pre-existing failures: rustty-z2d clippy manual_div_ceil, xtask fmt, cli_ghostty test-count, vt 2 missing comments), final_commit (git rev-parse --short HEAD), gate_report (verbatim per-gate results), binaries (ls ${W}/.perf-bins).`,
  { label: 'merge+gate', phase: 'MergeGate', agentType: 'general-purpose', schema: M_SCHEMA })

return { chain: chainResults, render: renderResult, merge }
