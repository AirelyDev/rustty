export const meta = {
  name: 'alloc-refactor-foundation',
  description: 'Phase 1 of the faithful allocator re-architecture (audit task #7). Thread an explicit Allocator (allocator-api2, stable) through rustty-zigstd ArenaAllocator + MemoryPool faithfully mirroring Zig std.heap (child_allocator), and port the reference mmap pageAllocator. BEHAVIOR-PRESERVING: every current caller passes the Global allocator, so the 205 PageList tests + Miri stay green; the page-specific switch (lazy-zero + fallible) is phase 2. Per-step commits + Miri + adversarial faithfulness review.',
  phases: [
    { title: 'Deps+Arena', detail: 'add allocator-api2; thread Allocator through ArenaAllocator (callers pass Global)' },
    { title: 'MemPool', detail: 'thread Allocator through MemoryPool/MemoryPoolAligned' },
    { title: 'pageAllocator', detail: 'port reference pageAllocator (mmap, page-aligned, lazily zeroed)' },
    { title: 'Verify', detail: 'workspace build + zigstd/os tests + Miri + faithfulness review' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const ZS = R + '/crates/rustty-zigstd/src'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. Stay CLOSE to the Zig reference: Ghostty ' + G + ' + the Zig 0.15.2 stdlib (std.heap.ArenaAllocator, std.heap.MemoryPool/MemoryPoolAligned, std.heap.page_allocator — find via `zig env` lib path or the existing PORT NOTES). '
  + 'Use the allocator-api2 crate (stable polyfill of core::alloc::Allocator) as the explicit allocator trait — the faithful map of Zig std.mem.Allocator. Its `Allocator::allocate(Layout)->Result<NonNull<[u8]>,AllocError>` / `deallocate` map to Zig rawAlloc/rawFree. '
  + 'CRITICAL — BEHAVIOR-PRESERVING in THIS workflow: thread the allocator as a real parameter (faithful: Zig ArenaAllocator.init(child_allocator) ALWAYS takes a child), but make EVERY existing caller pass `allocator_api2::alloc::Global` so the observable behavior is identical (Global == the current std::alloc path). Do NOT change page zeroing or fallibility semantics here — that is phase 2. The 205 PageList tests, all rustty-zigstd tests, clippy, fmt, and Miri MUST stay green. '
  + 'Run gates FOREGROUND (capture+inspect; for Miri use `cargo +nightly miri test -p <crate>` on the small zigstd/os crates — they are fast). Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, builds: { type: 'boolean' }, tests_pass: { type: 'boolean' }, miri_clean: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const REVIEW = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['FAITHFUL', 'DIVERGENT'] }, divergences: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Deps+Arena')
const arena = await robust(RULES +
  'TASK: (1) add `allocator-api2` (latest 0.2.x) to the workspace deps (root Cargo.toml [workspace.dependencies]) + rustty-zigstd Cargo.toml. (2) Thread a child allocator `A: allocator_api2::alloc::Allocator` through ' + ZS + '/arena_allocator.rs faithfully to Zig std.heap.ArenaAllocator: store `child: A` (make ArenaAllocator generic over A, default A = Global to minimize churn if helpful, but the field is real); `init(child: A)`; `raw_alloc_node`/`create_node` allocate the backing node via `child.allocate(Layout)` (was std::alloc); free backing nodes via `child.deallocate` in reset/Drop. Preserve the EXACT growth algorithm (BufNode 16B/align-8, 1.5x geometric, LIFO, reset-collapse) — only the alloc SOURCE changes from std::alloc to child.allocate. (3) Update the 4 arena tests + ALL callers (MemoryPool, anything constructing ArenaAllocator) to pass `allocator_api2::alloc::Global` so behavior is unchanged. Gates FOREGROUND: cargo build (workspace), cargo test -p rustty-zigstd, cargo +nightly miri test -p rustty-zigstd (0 UB/leak), clippy -D, fmt. Commit "rustty: zigstd ArenaAllocator threads explicit Allocator (allocator-api2)". Return ok+builds+tests_pass+miri_clean+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'arena', phase: 'Deps+Arena' })

phase('MemPool')
const mempool = await robust(RULES +
  'TASK: thread the same `A: Allocator` through ' + ZS + '/memory_pool.rs (MemoryPoolExtra/MemoryPool/MemoryPoolAligned) faithfully to Zig std.heap.MemoryPool — it owns an ArenaAllocator, so `init(child: A)` / `init_preheated(child, n)` forward `child` to the arena. Mirror Zig MemoryPool(T).init(child)/initPreheated(child, n). Update all callers (the NodePool/PinPool/etc. in rustty-vt pagelist, and zigstd tests) to pass `Global` — behavior unchanged. Gates FOREGROUND: cargo build (workspace) + cargo test -p rustty-zigstd + cargo +nightly miri test -p rustty-zigstd + cargo test -p rustty-vt pagelist (205 still pass) + clippy -D + fmt. Commit "rustty: zigstd MemoryPool threads explicit Allocator (allocator-api2)". Return ok+builds+tests_pass+miri_clean+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'mempool', phase: 'MemPool' })

phase('pageAllocator')
const pa = await robust(RULES +
  'TASK: port the reference page allocator as an `allocator_api2::alloc::Allocator` impl. REFERENCE: ' + G + '/src/terminal/PageList.zig `pageAllocator()` (~line 322, darwin-specific) + Zig std.heap.page_allocator. It must: allocate PAGE-ALIGNED memory via mmap (libc mmap MAP_ANON|MAP_PRIVATE on unix / VirtualAlloc on windows) so memory is LAZILY OS-zeroed (NOT eagerly memset — this is the whole point: fixes the audit eager-zero RSS regression); deallocate via munmap. Put it in rustty-os (e.g. crates/rustty-os/src/page_allocator.rs) since it is OS-specific, faithfully mirroring the reference darwin path + the generic page_allocator fallback. Add a test: allocating a page-sized region returns page-aligned, readable, zero-initialized memory; freeing works; Miri may not run mmap (note that — gate it with #[cfg] or skip under Miri). Gates FOREGROUND: cargo build + cargo test -p rustty-os + clippy -D + fmt. Commit "rustty: port reference pageAllocator (mmap, lazily-zeroed) — rustty-os". Return ok+builds+tests_pass+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'pagealloc', phase: 'pageAllocator' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify + adversarial faithfulness review — default DIVERGENT if unsure). Confirm FOREGROUND: cargo build (whole workspace) GREEN; cargo test -p rustty-zigstd + -p rustty-os + (cargo test -p rustty-vt pagelist = 205 pass); cargo +nightly miri test -p rustty-zigstd (0 UB/leak); clippy --all-targets -D + fmt --check. FAITHFULNESS: read the new ArenaAllocator + MemoryPool + pageAllocator vs the Zig stdlib originals — is the Allocator threading a faithful map of Zig child_allocator (allocate/deallocate <-> rawAlloc/rawFree)? Is behavior truly unchanged (all current callers pass Global)? Is the pageAllocator a faithful mmap lazily-zeroed port (no eager memset)? List any divergence. Verdict FAITHFUL only if all gates green AND threading+pageAllocator match the reference AND behavior is preserved.',
  { schema: REVIEW, effort: 'high', label: 'verify', phase: 'Verify' })
return { arena, mempool, pageAllocator: pa, verify }
