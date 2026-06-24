export const meta = {
  name: 'alloc-pagelist-faithful',
  description: 'Phase 2 of audit task #7: replace the hand-rolled PagePool with the faithful zigstd MemoryPoolAligned-over-arena on the rustty-os pageAllocator, threaded per the reference MemoryPool.init(gen_alloc, page_alloc). Makes page-buffer alloc FALLIBLE (#1), removes the eager ~360KiB memset via lazy mmap (#2), restores O(log N) arena batching (#3), fixes PAGE_SIZE_MIN (#4), + hygiene (#9/#16). Verified by 205 native + Miri + ASan (all 205) + adversarial faithfulness.',
  phases: [
    { title: 'PagePool', detail: 'PagePool -> MemoryPoolAligned over pageAllocator; fallible OOM; faithful zeroing (lazy fresh)' },
    { title: 'PageSizeMin', detail: 'fix PAGE_SIZE_MIN (4096->target 16384) + false PORT NOTE (#4)' },
    { title: 'Hygiene', detail: '#9 arena node_layout graceful OOM, #16 deinit-order PORT NOTE' },
    { title: 'Verify', detail: '205 + Miri + ASan + faithfulness; confirm no eager memset' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const ML = R + '/crates/rustty-vt/src/pagelist/mod.rs'
const SAN = 'bash ' + R + '/.port/tools/gates/sanitizers.sh rustty-vt pagelist address'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. Stay CLOSE to the Zig reference ' + G + '/src/terminal/PageList.zig + Zig std.heap.MemoryPool/MemoryPoolAligned. '
  + 'CONTEXT (phase-1 landed, commits 5bcb426/b7199d9/812cf7b): rustty-zigstd ArenaAllocator + MemoryPool/MemoryPoolAligned now take an explicit `A: allocator_api2::alloc::Allocator` child (faithful to Zig child_allocator). rustty-os has a faithful `page_allocator()` -> an allocator_api2 Allocator (mmap, page-aligned, LAZILY OS-zeroed). The 205 PageList tests, zigstd Miri, ASan-on-205 are all green NOW. '
  + 'Run gates FOREGROUND (capture+inspect). Native `cargo test -p rustty-vt pagelist` (205) on every step; Miri on the tractable subset + ASan via `' + SAN + '` at verify. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_pass: { type: 'integer' }, miri_clean: { type: 'boolean' }, no_eager_memset: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const REVIEW = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['FAITHFUL', 'DIVERGENT'] }, fallible_oom: { type: 'boolean' }, lazy_zero: { type: 'boolean' }, arena_backed: { type: 'boolean' }, page_size_min_correct: { type: 'boolean' }, divergences: { type: 'array', items: { type: 'string' } }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('PagePool')
const pp = await robust(RULES +
  'TASK: replace the hand-rolled PagePool (' + ML + ' ~212-353: `arena: Vec<NonNull<u8>>` + per-page alloc_zeroed + handle_alloc_error) with the FAITHFUL reference model. Reference: `PagePool = std.heap.MemoryPoolAligned([std_size]u8, page_align)` (PageList.zig:65) backed by the page allocator; `MemoryPool.init(gen_alloc, page_alloc)` (PageList.zig:34-50) threads gen_alloc to Node/Pin pools and page_alloc to the PagePool. STEPS: '
  + '(1) Make the standard page pool a `rustty_zigstd::MemoryPoolAligned<[u8; STD_SIZE], PAGE_ALIGN, PageAllocator>` (or the closest faithful mapping) constructed over `rustty_os::page_allocator::page_allocator()`. Nodes/Pins keep Global (gen_alloc). Mirror the reference MemoryPool.init(gen_alloc, page_alloc) signature/wiring. '
  + '(2) NON-STANDARD pages (layout.total_size > std_size): allocate/free DIRECTLY via the page_allocator (the reference uses `pages.arena.child_allocator` for these — bypassing the pool), matching createPageExt / destroyNodeExt. '
  + '(3) FALLIBLE OOM (#1): remove `handle_alloc_error`; the pool create() and direct page_alloc.allocate() return errors; propagate `OutOfMemory` through create_page/create_page_ext/grow/etc. (those signatures already exist). The reference threads Allocator.Error and callers retry — restore that. '
  + '(4) ZEROING — faithful + the #2 optimization: FRESH page memory from the arena/page_allocator is LAZILY OS-zeroed (mmap) — do NOT eagerly memset it (this removes the ~360KiB-per-page eager RSS commit). But REUSED (free-list) and RETAINED-on-reset buffers must still be zeroed to match the reference (@memset(0) on retained mmap memory). Read PageList.zig createPage/initPages/reset/destroyNode + the prior PORT NOTE (mod.rs:187-211) and reproduce the EXACT zero semantics: handed-out pages are always observably zero, but fresh ones get there lazily. '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt pagelist = 205 pass; cargo +nightly miri test -p rustty-vt -- --exact pagelist::tests::pagelist_reset pagelist::tests::pagelist_clone_partial_trimmed_left_reclaims_styles pagelist::tests::pagelist_active_after_grow --test-threads=1 (clean); clippy -D; fmt. Add/keep a test asserting a freshly handed-out page buffer reads as all-zero. Commit "rustty: PageList PagePool -> faithful MemoryPoolAligned/arena + fallible OOM + lazy-zero (audit #1/#2/#3)". Return ok+tests_pass+miri_clean+no_eager_memset+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'pagepool', phase: 'PagePool' })

phase('PageSizeMin')
const psm = await robust(RULES +
  'TASK (#4 fidelity bug): PAGE_SIZE_MIN is hardcoded 4096 at crates/rustty-vt/src/page.rs:163, but aarch64-macos uses 16384 (std.heap.page_size_min). Derive it from the target (e.g. a const per target_arch/target_os, or query the OS page size faithfully to the reference) so non-standard page total_size matches the reference on Apple Silicon. Correct the FALSE PORT NOTE at page.rs:158-162. Add/strengthen a parity test asserting Page.layout(reduced_capacity).total_size matches the reference value on this target. Gates: cargo build + cargo test -p rustty-vt pagelist (205) + clippy -D + fmt. Commit "rustty: derive PAGE_SIZE_MIN from target (audit #4 fidelity)". Return ok+tests_pass+commit+blockers. If changing it breaks a test that encoded the wrong 4096, fix the test to the correct reference value (it was wrong), not weaken it.',
  { schema: STEP, effort: 'high', label: 'pagesizemin', phase: 'PageSizeMin' })

phase('Hygiene')
const hy = await robust(RULES +
  'TASK (low-sev hygiene): (#9) crates/rustty-zigstd/src/arena_allocator.rs node_layout `.expect()` -> graceful `Layout::from_size_align(...).ok()?` threading `?` (mirror Zig `orelse return null`) if still present after phase-1. (#16) Fix the misleading deinit-order PORT NOTE at ' + ML + ' ~431-434 (pools are self-contained; comment claims pages-first drop). No functional change. Gates: cargo build + cargo test -p rustty-zigstd + -p rustty-vt pagelist (205) + zigstd Miri + clippy -D + fmt. Commit "rustty: alloc hygiene — arena graceful OOM + deinit-order PORT NOTE (audit #9/#16)". Return ok+tests_pass+commit+blockers.',
  { schema: STEP, effort: 'medium', label: 'hygiene', phase: 'Hygiene' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify + adversarial faithfulness — default DIVERGENT if unsure). Run FOREGROUND: cargo build --workspace; cargo test -p rustty-vt pagelist (205 pass) + -p rustty-zigstd + -p rustty-os; cargo +nightly miri test -p rustty-vt on ~6 tractable pagelist tests (reset/clone/grow/erase/pin/scroll small — 0 UB/leak); `' + SAN + '` (ASan on all 205 = 0 findings). FAITHFULNESS vs ' + G + '/src/terminal/PageList.zig + Zig std.heap.MemoryPoolAligned: (1) is the PagePool now MemoryPoolAligned-over-arena on the pageAllocator, threaded like MemoryPool.init(gen_alloc,page_alloc)? arena_backed. (2) is page-buffer OOM now FALLIBLE (no handle_alloc_error on the buffer path; OutOfMemory propagates like the reference)? fallible_oom. (3) is fresh-page zeroing LAZY (no eager memset; mmap) while reused/retained still zeroed to match the reference? lazy_zero + grep that no eager `write_bytes(0)`/`alloc_zeroed` remains on the fresh page path. (4) is PAGE_SIZE_MIN target-correct (16384 on aarch64-macos)? page_size_min_correct. Verdict FAITHFUL only if all gates green AND all four true. List divergences.',
  { schema: REVIEW, effort: 'high', label: 'verify', phase: 'Verify' })
return { pp, psm, hy, verify }
