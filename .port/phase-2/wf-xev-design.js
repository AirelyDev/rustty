export const meta = {
  name: 'xev-design',
  description: 'Design blueprint for D6: faithful Zig->Rust port of libxev (the async event loop) into rustty-xev, unblocking the live shell (termio/renderer/app event loops). Architect + adversarial verification.',
  phases: [ { title: 'Design' }, { title: 'Verify' } ],
}

const LIBXEV = '/Users/gael/.cache/zig/p/libxev-0.0.0-86vtc4IcEwCqEYxEYoN_3KXmc6A9VLcm22aVImfvecYs/src'

const CONTEXT = [
  'Project: rustty, a faithful Zig->Rust port of ghostty. Reference READ-ONLY: ghostty at /Users/gael/projects/ghostty (Zig 0.15.2) and its libxev dependency source at ' + LIBXEV + ' (mitchellh/libxev, the exact pinned commit ghostty uses). Port at /Users/gael/projects/rustty. Rules: /Users/gael/projects/rustty/.port/PORT_RULES.md (R1: every libxev test{} ported, test COUNT is a contract; R3: byte-faithful every line and comment, byte-slices become and-u8 slices or str never lossy String; R5: never weaken). Gates: check_acyclic.sh, check_test_counts.py, scoped-G3b. Precedent for porting a non-ghostty Zig LIBRARY faithfully: rustty-z2d (z2d ported pixel-exact). libxev is the SAME class (a Zig library dependency, ported like z2d, NOT a C -sys binding).',
  '',
  'THIS IS D6. rustty-xev currently is a 127-loc PLACEHOLDER (10 unimplemented! seams: Loop/Async/Timer/Completion/etc.) that rustty-termio re-exports via `pub use rustty_xev`. Filling it with the real libxev port un-defers the whole D6 family across termio (Thread run loop + callbacks, Termio init/threadEnter/processOutput, Exec Subprocess.start + ReadThread), renderer (Thread event loop), and core/App (the app event loop) — i.e. it unblocks a LIVE shell (termio can actually pump a real pty; the app loop runs). This is the single biggest functional unlock remaining.',
  '',
  'GHOSTTY USES from xev (the API surface that MUST be ported, with usage counts): Completion(31), Timer(23), Loop(21), Async(18), CallbackAction(15), Process(5), Stream(3), WriteRequest(1), WriteQueue(1), WriteError(1), WriteBuffer(1). The dev/CI box is macOS (Darwin) -> the kqueue backend is what unblocks the live shell here; Linux epoll/io_uring and Windows iocp are lower priority.',
  '',
  'LANDED BASE (all gate-green): the ENTIRE terminal + app + libghostty-C-ABI + CLI is ported; termio (44t) and core (15t) hold the xev-deferred D6 seams (documented, verbatim bodies) waiting for this. rustty-os has the posix layer (fd/pipe/pty/kqueue syscalls likely reachable via libc).',
].join('\n')

phase('Design')
const blueprint = await agent(CONTEXT + '\n\n' + [
  'You are the ARCHITECT. Produce a CONCISE, decision-oriented implementation blueprint (write it to /Users/gael/projects/rustty/.port/phase-2/XEV_BLUEPRINT.md, do NOT commit; also return as final text) for porting libxev into rustty-xev. Ground EVERY claim by READING the libxev source at ' + LIBXEV + ' (main.zig, backend/*.zig, watcher/*.zig, the Loop/Completion/Callback core) and how ghostty uses it (termio Thread/Termio/Exec, renderer Thread, App). Cover:',
  '1. SCOPE decision: libxev is cross-platform (kqueue/epoll/io_uring/iocp/wasi backends). What is the MINIMAL faithful port that unblocks the live shell on macOS: the core (Loop, Completion, Callback, CallbackAction, ThreadPool if used) + the KQUEUE backend + the watchers ghostty actually uses (Async, Timer, Process, Stream/File, WriteQueue/WriteRequest). Which backends/watchers to DEFER (epoll/io_uring/iocp/wasi; TCP/UDP if ghostty does not use them). Give the honest file-by-file in-scope vs deferred list with loc + test counts.',
  '2. The COMPLETION / callback model (the make-or-break faithfulness question). libxev uses INTRUSIVE completions: the caller owns the Completion memory and passes it by pointer; callbacks are comptime fn pointers with an untyped userdata pointer + CallbackAction (disarm/rearm) return. This is raw-pointer-heavy and self-referential (completions live in caller structs, the Loop holds pointers into them). Design the faithful Rust model: raw pointers (NOT lifetimes/&mut across the loop — cite the rust-mut-args-strongly-protected discipline), how the comptime-generic callback becomes a Rust fn pointer + *mut c_void userdata + a CallbackAction enum, how intrusive queues (the completion linked lists) port. Show the core Loop.run/tick + Completion struct signatures.',
  '3. The kqueue backend: how libxev backend/kqueue.zig maps kqueue syscalls (via rustty-os libc or a new syscall layer) — kevent/EV_ADD/EV_DELETE, the changelist/eventlist, timers, EVFILT_*, process (EVFILT_PROC), async (self-pipe or EVFILT_USER). What rustty-os already provides vs new syscall bindings needed.',
  '4. Watchers: Async (wakeup), Timer (deadline + reset), Process (pidfd/EVFILT_PROC wait), Stream/File (read/write/pread/pwrite + WriteQueue). Faithful Rust struct + method signatures for each, mapping to Completions.',
  '5. Crate placement + deps + acyclicity: rustty-xev is a low leaf (currently deps rustty-global only). What does the real port need (rustty-os for syscalls? libc directly? rustty-datastruct for the intrusive queue?). Confirm acyclic (rustty-xev must stay BELOW termio/renderer/core which depend on it).',
  '6. The D6 UN-DEFER plan: once rustty-xev is real, which termio/renderer/core deferred seams get un-deferred, and is that in THIS milestone or a follow-up? (Recommend: land real rustty-xev + its own tests FIRST as a self-contained milestone; the termio/renderer/core un-defer + a live-shell KAT is the NEXT milestone. Justify.)',
  '7. Test accounting: libxev has its own test{} blocks (grep them per file) — R1 requires porting them. Count per in-scope file. Plus recommend a rustty-only live KAT (e.g. an Async wakeup round-trip, a Timer firing, a Process spawn+exit, a pipe read) proving the loop actually runs on kqueue.',
  '8. Chunked execution plan (the port is large) + risks (the raw-pointer/intrusive-completion UB surface -> Miri where feasible though kqueue syscalls may not run under Miri; comptime->generic/fn-ptr translation; the self-pipe/EVFILT_USER async; platform cfg-gating).',
  'Be decisive and tight. This is a large, faithfulness-critical port; the blueprint is the quality gate.',
].join('\n'), { label: 'architect', phase: 'Design' })

phase('Verify')
const verdict = await agent(CONTEXT + '\n\n' + [
  'You are the ADVERSARIAL REVIEWER. The blueprint is at /Users/gael/projects/rustty/.port/phase-2/XEV_BLUEPRINT.md (read it). Break it against the actual libxev source + ghostty usage + the repo:',
  '1. API completeness: grep ghostty for EVERY xev. usage yourself; does the in-scope list cover all of Loop/Completion/Timer/Async/Process/Stream/WriteQueue members ghostty calls? Anything deferred that ghostty actually needs for termio/renderer/app to compile+run?',
  '2. Completion/callback faithfulness: is the proposed Rust model actually sound for libxev intrusive completions (caller-owned, self-referential, comptime callbacks)? Name any aliasing/lifetime/Send/pinning landmine. Does the CallbackAction (disarm/rearm) + userdata round-trip typecheck? Compare to how vt-c / core handled raw-pointer self-reference.',
  '3. kqueue backend correctness: read libxev backend/kqueue.zig; does the blueprint faithfully map the changelist/eventlist, EVFILT_*, timers, EVFILT_PROC/USER? Any syscall the blueprint hand-waves? Does rustty-os actually expose what is claimed (verify) or is new libc binding needed?',
  '4. Cycle traps: does the proposed rustty-xev dep set create a cycle (rustty-xev must stay below termio/renderer/core)? If it needs rustty-os, is rustty-os below rustty-xev in the real graph?',
  '5. Test count: grep test{} in each in-scope libxev file yourself; verify the blueprint counts + that the R1 contract (port every test) is honored, and that deferred backends do not silently drop tests that ghostty-relevant code needs.',
  '6. Scope honesty: is the macOS-kqueue-first split real, or does some ghostty path need a Linux backend / a watcher marked deferred? Is the two-milestone split (rustty-xev first, then the termio/core un-defer) sound, or does landing rustty-xev without wiring leave a false-green?',
  '7. Anything else that sinks it.',
  'Return numbered findings (CONFIRMED-ISSUE with file:line evidence / FALSE-ALARM / RECOMMENDATION) + overall verdict: ACCEPT / ACCEPT-WITH-FIXES (list fixes) / REDESIGN. Do NOT edit the blueprint.',
].join('\n'), { label: 'adversary', phase: 'Verify' })

return { blueprint_len: (blueprint || '').length, verdict }
