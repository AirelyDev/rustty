export const meta = {
  name: 'xev-milestone-b-design',
  description: 'Design blueprint for D6 Milestone B: wire termio (Thread/Termio/Exec) to the real rustty-xev loop + resolve the self-referential subprocess spawn + a live-shell KAT (first running terminal). Architect + adversarial verification.',
  phases: [ { title: 'Design' }, { title: 'Verify' } ],
}

const CONTEXT = [
  'Project: rustty, a faithful Zig->Rust port of ghostty. Reference READ-ONLY at /Users/gael/projects/ghostty (Zig 0.15.2); port at /Users/gael/projects/rustty. Rules: /Users/gael/projects/rustty/.port/PORT_RULES.md (R1 test-count contract; R3 byte-faithful every line and comment; R5 never weaken). Gates: check_acyclic.sh, check_test_counts.py, scoped-G3b.',
  '',
  'D6 MILESTONE A JUST LANDED (HEAD c8ebe97): rustty-xev is now a REAL, working kqueue event loop (9550 loc, 42 tests incl. 4 live KATs proving cross-thread Async wakeup, Timer fire, Process spawn/exit, and pty Stream write-round-trip all run on real syscalls). Public API surface (flat kqueue Api()): Loop{init,deinit,stop,stopped,add,submit,run,tick,now,update_now,timer,timer_reset}, Completion (Default, Send), Callback/CallbackAction, Async{init,deinit,wait,notify} (mach-port, Copy/Send), Timer{init,run,reset,cancel}, Process{init,wait}, Stream{init_fd,read,write,close,poll,queue_write}, WriteQueue/WriteRequest (Default). Raw-ptr intrusive model end-to-end; unsafe impl Send on Loop/Completion/WriteQueue-bearing types.',
  '',
  'THIS IS MILESTONE B — wire TERMIO ONLY to the real loop (renderer.Thread is GPU-AND-xev blocked, renderer_seam.rs:381 = Phase-3; App.zig has no xev loop; both are OUT per the reviewed XEV_BLUEPRINT §6). The termio D6-deferred seams already exist as VERBATIM-comment bodies waiting for this (documented in DEFERRALS.md): termio Thread.run loop + callbacks (thread.rs), Termio.init/threadEnter/processOutput/queueWrite (termio_impl.rs), Exec Subprocess.start + ReadThread + queueWrite + the xev-driven exec callbacks (exec.rs), the mailbox::send caller-held-mutex pattern, stream_handler surfaceMessageWriter/rendererMessageWriter. Plus the core Async::notify() leaves that termio threads poke.',
  '',
  'KNOWN HARD SPOT (from the termio C-T deferral): Exec Subprocess.start was deferred at "the stored fork_exec Command arena-pointer self-reference (Rust UB per the strongly-protected-reference rule; modeling decided WITH the D6 live-thread wiring)". This milestone must resolve that self-referential spawn (heap-box/pin discipline, like the embedded Surface pinning). termio currently = 44 tests, must stay green; the un-deferred paths become LIVE.',
].join('\n')

phase('Design')
const design = await agent(CONTEXT + '\n\n' + [
  'You are the ARCHITECT. Produce a CONCISE blueprint (write to /Users/gael/projects/rustty/.port/phase-2/XEV_MILESTONE_B_BLUEPRINT.md; also return as final text) for wiring termio to the real rustty-xev loop, ending in a live shell. Ground EVERY claim by READING: the real rustty-xev public API (crates/rustty-xev/src/{lib,loop,async,timer,process,stream}.rs), the termio deferred seams (crates/rustty-termio/src/{thread,termio_impl,exec,mailbox,stream_handler}.rs — find every D6/xev unimplemented!()/deferred-verbatim body), the reference (src/termio/{Thread,Termio,Exec}.zig, the xev usage), and the core Async::notify leaves (surface.rs). Cover:',
  '1. The exact un-defer list: every termio (+ core notify) deferred seam this milestone lands, mapped to the real rustty-xev API call that replaces its verbatim-comment body. Confirm each maps cleanly to the landed API (signatures match) — flag any termio body that needs an xev API not in Milestone A.',
  '2. The subprocess-spawn self-reference resolution (the hard spot). Read Exec.zig Subprocess.start + the rustty-termio exec.rs deferral. Design the faithful Rust model for the fork_exec Command + its arena-pointer self-reference: heap-box + pin (embedded-Surface precedent) vs a restructure. Show the ownership/lifetime + why it is sound (raw-ptr discipline, no long-lived &mut).',
  '3. The thread lifecycle + Send. termio Thread.init runs on the parent, threadMain on the child; the Loop is MOVED onto the io thread; Async.notify() is called cross-thread. Map to the rustty-xev Send impls + std::thread::spawn with the documented Send-wrapper (embedded/core precedent). The ReadThread (raw pty read, NOT xev) + the xev-driven write path.',
  '4. The mailbox/mutex un-defer: mailbox::send caller-held-mutex pattern + stream_handler surfaceMessageWriter/rendererMessageWriter — now that the loop is real, do these land here or stay partly deferred (renderer mailbox is real; the core Timeout is resolvable)? Justify.',
  '5. The LIVE-SHELL KAT (the milestone proof). Design an integration test: spawn a real shell (/bin/sh) via the termio Exec path in a pty, pump the rustty-xev loop, write a command (e.g. echo), assert the terminal Screen contains the expected output (round-trip through pty -> xev read -> terminal.Stream -> Screen). What crate does it live in (termio integration test)? How to make it deterministic (no hangs — bounded loop ticks / timeout).',
  '6. Test accounting: which termio deferred tests (if any) un-defer + become live; does termio stay 44 or grow; the live-shell KAT counting. Acyclicity (termio already deps rustty-xev).',
  '7. Chunked execution plan + risks (the self-ref spawn UB; thread-lifecycle races; the parallel-cargo-test isolation lesson for a pty-spawning KAT; determinism/hang-avoidance).',
  'Be decisive. This is the culmination — the first time the port runs a real terminal.',
].join('\n'), { label: 'architect', phase: 'Design' })

phase('Verify')
const verdict = await agent(CONTEXT + '\n\n' + [
  'You are the ADVERSARIAL REVIEWER. The blueprint is at /Users/gael/projects/rustty/.port/phase-2/XEV_MILESTONE_B_BLUEPRINT.md (read it). Break it against the real rustty-xev API + the termio deferred bodies + the reference:',
  '1. API-match: does EVERY termio deferred body the blueprint un-defers actually map to a rustty-xev API that EXISTS (grep the real crates/rustty-xev/src public items)? Any body needing an xev call that Milestone A deferred (e.g. a Stream/Timer variant not ported)?',
  '2. The self-ref spawn: is the proposed resolution actually sound (no dangling arena ptr, no long-lived &mut across an integrity-asserting call, no move-after-pin)? Compare to how embedded Surface / core init handled self-reference. Name any UB.',
  '3. Send/thread-lifecycle: does moving the Loop onto the io thread actually typecheck with the landed Send impls? Any !Send field still in the moved struct? Race between ReadThread + the xev write path?',
  '4. The live-shell KAT: is it genuinely a proof the loop RUNS (real pty + real shell + real round-trip), or does it stub/mock the shell? Is it deterministic (can it hang)? Does it isolate global/pty state under parallel cargo test (the onig_initialize lesson)?',
  '5. Scope honesty: is this really termio-only, or does a body pull renderer.Thread/core-app-loop (the false-green trap)? Any deferred seam the blueprint claims lands but actually needs Phase-3?',
  '6. Test-count + acyclic + anything else that sinks it.',
  'Return numbered findings (CONFIRMED-ISSUE with file:line evidence / FALSE-ALARM / RECOMMENDATION) + verdict: ACCEPT / ACCEPT-WITH-FIXES (list fixes) / REDESIGN. Do NOT edit the blueprint.',
].join('\n'), { label: 'adversary', phase: 'Verify' })

return { design_len: (design || '').length, verdict }
