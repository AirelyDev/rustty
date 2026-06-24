export const meta = {
  name: 'page-pool-tracking-allocator',
  description: 'Task #8: a test-only counting Allocator (allocator-api2) wrapping the PageList page allocator, asserting "all pages returned" (no leak) — the near-1:1 scoped port of Zig std.testing.allocator value, at the one place it matters (page buffers). Gives native-speed leak detection covering the 23 Miri-intractable heavy tests (macOS has no LSan). Adversarially validated by injecting a deliberate leak and confirming the assertion fires.',
  phases: [
    { title: 'Implement', detail: 'TrackingAllocator wrapper + wire into PageList tests + assert all-returned' },
    { title: 'Verify', detail: 'inject a leak -> assertion fires (then revert); 206 + Miri + ASan green' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const SAN = 'bash ' + R + '/.port/tools/gates/sanitizers.sh rustty-vt pagelist address'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. CONTEXT: the allocator stack is now faithful (task #7): rustty-zigstd MemoryPool/ArenaAllocator take an explicit allocator_api2 Allocator; the PageList page pool is a DynamicMemoryPoolAligned over rustty_os::page_allocator::page_allocator() (mmap), threaded via MemoryPool::init(gen_alloc=Global, page_alloc). 206 PageList tests pass; ASan-all-206 + Miri-tractable clean. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_pass: { type: 'integer' }, leak_assert_fires: { type: 'boolean' }, miri_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Implement')
const impl = await robust(RULES +
  'TASK: add a TEST-ONLY tracking (counting) allocator and wire it onto the PageList page pool, as the scoped faithful equivalent of Zig std.testing.allocator (which fails a test if any allocation leaks). '
  + 'STEPS: (1) implement `TrackingAllocator<A: allocator_api2::alloc::Allocator>` (e.g. in rustty-zigstd under #[cfg(any(test, feature="tracking"))], or a shared test-util module): wraps a child `A`, and on each allocate/deallocate atomically updates outstanding byte + allocation counters; exposes `outstanding_bytes()`/`outstanding_allocs()` and a `assert_no_leak()` that panics with a clear message if either is non-zero. It must be a faithful counting wrapper (every allocate increments, every deallocate decrements by the SAME layout), not change behavior. '
  + '(2) Make PageList constructible over a custom page allocator in tests (the MemoryPool::init already threads page_alloc — expose a test ctor or use the existing seam) so a test can build a PageList whose page_alloc is `TrackingAllocator<PageAllocator>`. '
  + '(3) Add test(s) that exercise representative page churn (grow scrollback to several pages, scroll, clone, resize/reflow, then deinit/reset) and assert the tracker reports ZERO outstanding page allocations after deinit — i.e. all pages returned. Include at least one heavy path (a resize-reflow or clone over many pages) so this gives leak coverage where Miri times out. '
  + 'GATES FOREGROUND: cargo build; cargo test -p rustty-vt pagelist (>=206 pass, incl. the new tracking tests); cargo +nightly miri test -p rustty-vt on the new tracking test (clean) + cargo test -p rustty-zigstd; `' + SAN + '`; clippy -D; fmt. Commit "rustty: page-pool tracking allocator (scoped std.testing.allocator equivalent)". Return ok+tests_pass+miri_clean+asan_clean+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'tracking', phase: 'Implement' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (adversarial — prove the tracker actually CATCHES a leak, then prove the real code is clean). (1) Temporarily inject a deliberate page leak (e.g. allocate a page via the tracked page_alloc and skip its free, or comment out one destroy in a scratch copy of a tracking test) and confirm the tracking assertion (assert_no_leak / outstanding != 0) FIRES the test failure. Then REVERT the injection (leave the tree exactly as committed — git diff must be empty after). (2) Confirm the real tracking tests pass (outstanding == 0 = no leak) and that cargo test -p rustty-vt pagelist is green (>=206), Miri clean on the new test, `' + SAN + '` clean. Report leak_assert_fires (did the injected leak get caught) + tests_pass + the post-verify git status (must be clean). Verdict in notes: is the tracker a sound, faithful, leak-catching scoped equivalent of std.testing.allocator?',
  { schema: STEP, effort: 'high', label: 'verify', phase: 'Verify' })
return { impl, verify }
