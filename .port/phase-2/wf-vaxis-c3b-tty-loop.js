export const meta = {
  name: 'vaxis-c3b-tty-loop',
  description: 'vaxis chunk 3b — port the tty backend (termios FFI) + queue (concurrency) + Loop (read-thread): design -> adversary -> fold -> port -> [mirihar] -> verify',
  phases: [
    { title: 'Design', detail: 'faithful-port design; tty FFI (libc), queue Miri, Loop read-thread aliasing + test strategy' },
    { title: 'Adversary', detail: 'attack the FFI soundness + read-thread aliasing + test honesty' },
    { title: 'Fold', detail: 'fold fixes' },
    { title: 'Port', detail: 'queue + tty + Loop' },
    { title: 'Mirihar', detail: 'if the read-thread aliasing is non-trivial, prove naive=UB/shipped=CLEAN under SB+TB' },
    { title: 'Verify', detail: 'gates + R1 fidelity + Miri(queue) + ASan-note(tty FFI)' },
  ],
}

const R = '/Users/gael/projects/rustty'
const VX = '/Users/gael/.cache/zig/p/vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS'
const DESIGN = `${R}/.port/phase-2/VAXIS_C3B_DESIGN.md`
const MIRIHAR = `${R}/.port/phase-2/mirihar-chunk3b`

const rules = `
Full-fidelity Zig->Rust port (rustty). vaxis chunk 3b: the tty backend + concurrency (the impure half of the backend). Source (read-only): vaxis 0.5.1 ${VX}/src/{queue,tty,Loop}.zig. Target: crates/rustty-vaxis (chunks 1a/1b/2/3a landed). rustty-vaxis is currently a dep-free leaf; chunk 3b adds libc (for termios) — a leaf dep, acyclic (update the 'dep-free' Cargo.toml comment). Chunk 3b uses chunk-3a parser + chunk-1a Key/Mouse + event.
SCOPE:
- queue.zig (349 LOC, 7 test — a thread-safe fixed-size BLOCKING queue: Mutex + not_full/not_empty Condvar, push/pop/tryPush/tryPop). Pure Rust (std::sync::Mutex/Condvar) -> Miri-testable, port all 7 test{} assertion-for-assertion.
- tty.zig (749 LOC, 0 test — PosixTty: open /dev/tty, makeRaw (tcgetattr/cfmakeraw-equivalent via the exact termios flag clears/sets, tcsetattr), sigaction WINCH global handler + handler_mutex, read/write, getWinsize (TIOCGWINSZ ioctl), deinit restores termios). FFI via libc (tcgetattr/tcsetattr/ioctl/sigaction/open/read/write) — port the EXACT termios flag manipulations (makeRaw clears/sets specific c_iflag/c_oflag/c_lflag/c_cflag bits + VMIN/VTIME). The WindowsTty arm = #[cfg(windows)] R5 seam. FFI -> Miri-excluded/ASan; tty tests need /dev/tty (absent in sandbox) so any are #[ignore] on-device.
- Loop.zig (404 LOC, 1 test — spawns a read thread (ttyRun) that reads the tty in a loop, feeds bytes to the chunk-3a Parser, and pushes parsed events to the queue; start/stop/join, postEvent, tryEvent/nextEvent). The read-thread holds the Loop/tty for its life = construct-then-spawn aliasing (like the render/io/search threads): the read thread touches tty(read)+queue(push); the main thread touches queue(pop)+the join handle. Model faithfully with the raw-self/Arc discipline (queue is internally-synced; NO &mut *self whole-struct reborrow while the thread is live). If non-trivial, PROVE on ${MIRIHAR} (loopthread_naive=UB/loopthread_shipped=CLEAN, SB+TB).
R1: EVERY line/comment/test{} ported; byte-slices -> &[u8]/Vec<u8>, never lossy String. R4 acyclic (libc is the only new dep). R6: queue -> Miri; tty FFI -> ASan/native (MIRI_EXCEPTIONS entry); Loop read-thread aliasing -> mirihar. Gates: build/test/clippy -D/fmt/check_acyclic.sh/check_test_counts.py/check_comment_coverage.py + Miri.`

const D_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','needs_mirihar'], properties:{
  doc_path:{type:'string'}, disposition:{type:'string',description:"'ready' or 'blocked: <reason>'"}, summary:{type:'string'},
  aliasing_model:{type:'string',description:'the Loop read-thread aliasing model + why it is sound (keep terse)'}, needs_mirihar:{type:'boolean'},
  key_decisions:{type:'array',items:{type:'string'}}, risks:{type:'array',items:{type:'string'}} } }

phase('Design')
const design = await agent(
  `${rules}\n\nDESIGN PHASE. Write ${DESIGN}. Read ${VX}/src/{queue,tty,Loop}.zig in full. Map -> queue.rs/tty.rs/loop_.rs (loop is a Rust keyword). Pin: queue (Mutex+Condvar, the exact blocking/wraparound semantics + 7 tests); tty (the libc termios FFI surface + the EXACT makeRaw flag manipulations + the WINCH sigaction + getWinsize ioctl; the WindowsTty #[cfg(windows)] R5 seam); Loop (the read-thread construct-then-spawn — fill aliasing_model: how the read thread's tty+queue access and the main thread's queue+join access stay non-aliasing/sound; whether a raw-self *Loop or an Arc<shared> split is faithful; set needs_mirihar if non-trivial). Test strategy: queue 7 tests Miri-clean; tty tests #[ignore]-on-device (no /dev/tty in sandbox); Loop test via a pipe/mock or #[ignore]. If a genuine blocker, disposition 'blocked' + STOP. Else 'ready'. Return the object.`,
  { label:'design:vaxis-c3b', phase:'Design', agentType:'general-purpose', schema:D_SCHEMA })

if (!design || (design.disposition||'').toLowerCase().startsWith('blocked')) {
  log(`VAXIS-C3b DESIGN BLOCKED: ${design ? design.disposition : 'design failed'}`)
  return { blocked:true, design }
}

phase('Adversary')
const ADV_SCHEMA = { type:'object', additionalProperties:false, required:['verdict','fixes','blocking_issues'], properties:{
  verdict:{enum:['SOUND','SOUND_WITH_FIXES','UNSOUND']}, fixes:{type:'array',items:{type:'string'}}, blocking_issues:{type:'array',items:{type:'string'}} } }
const adversary = await agent(
  `${rules}\n\nADVERSARY PHASE. Design at ${DESIGN}; aliasing_model: "${design.aliasing_model || '(see the design doc §Loop)'}". BREAK it: (1) tty makeRaw — are the EXACT termios flag clears/sets (c_iflag IGNBRK/BRKINT/PARMRK/ISTRIP/INLCR/IGNCR/ICRNL/IXON; c_oflag OPOST; c_lflag ECHO/ECHONL/ICANON/ISIG/IEXTEN; c_cflag CSIZE/PARENB->CS8; VMIN=1/VTIME=0) faithful to tty.zig (a wrong flag silently breaks raw input)? (2) is the libc FFI sound (correct struct/fn signatures, error handling, the sigaction WINCH global + handler_mutex)? (3) the Loop read-thread aliasing — is it ACTUALLY sound (no &mut *self while the thread is live; the queue Mutex genuinely the only shared mutable state; join before deinit)? does the mirihar model (if needs_mirihar) match the real shape? (4) queue Mutex+Condvar — the exact blocking semantics (spurious wakeup loop, not_full/not_empty signalling) faithful? (5) is the test honesty right (tty/Loop #[ignore]-on-device clearly justified, queue really Miri-covered)? (6) no lossy String? List every fix + blocker. verdict SOUND only if the FFI + aliasing are provably sound.`,
  { label:'adversary:vaxis-c3b', phase:'Adversary', agentType:'Explore', schema:ADV_SCHEMA })

const needsFold = adversary && ((adversary.fixes && adversary.fixes.length) || (adversary.blocking_issues && adversary.blocking_issues.length))
if (needsFold) {
  phase('Fold')
  await agent(
    `${rules}\n\nFOLD PHASE. Adversary issues on ${DESIGN}. Fixes: ${JSON.stringify(adversary.fixes)}. Blocking: ${JSON.stringify(adversary.blocking_issues)}. Update ${DESIGN} IN PLACE to fix every one (re-read tty/Loop/queue as needed). If a blocker is genuine add a top 'DISPOSITION: blocked' line. Keep needs_mirihar accurate. Return a short summary.`,
    { label:'fold:vaxis-c3b', phase:'Fold', agentType:'general-purpose' })
}

phase('Port')
const P_SCHEMA = { type:'object', additionalProperties:false, required:['disposition','files_changed','build_ok','tests_ported','miri_clean','notes'], properties:{
  disposition:{type:'string'}, files_changed:{type:'array',items:{type:'string'}}, build_ok:{type:'boolean'}, tests_ported:{type:'number'}, miri_clean:{type:'boolean'}, notes:{type:'string'} } }
const port = await agent(
  `${rules}\n\nPORT PHASE. Implement per ${DESIGN} — if 'DISPOSITION: blocked', STOP. Port queue/tty/Loop into crates/rustty-vaxis/src/{queue,tty,loop_}.rs (+ mod wiring + libc dep in Cargo.toml + update the dep-free comment), preserving every comment (R1). Port queue's 7 test{} (Miri-clean) + any tty/Loop test #[ignore]-on-device per the design. WindowsTty = #[cfg(windows)] R5 seam. Update lib.rs PORT STATUS. Build \`cargo build -p rustty-vaxis\`, \`cargo test -p rustty-vaxis\` (queue + non-ignored), \`cargo +nightly miri test -p rustty-vaxis\` (the FFI tty tests must NOT run under Miri — gate them). Add a MIRI_EXCEPTIONS entry for the tty FFI. Report files_changed, build_ok, tests_ported, miri_clean, notes (incl. the aliasing approach + which tests are on-device). Do NOT git commit.`,
  { label:'port:vaxis-c3b', phase:'Port', agentType:'general-purpose', schema:P_SCHEMA })

if (!port || (port.disposition||'').toLowerCase().startsWith('stopped')) {
  log(`VAXIS-C3b PORT STOPPED`)
  return { design, adversary, port, mirihar:null, verify:null }
}

let mirihar = null
if (design.needs_mirihar) {
  phase('Mirihar')
  const M_SCHEMA = { type:'object', additionalProperties:false, required:['naive_is_ub','shipped_is_clean','sb_ran','tb_ran','evidence'], properties:{
    naive_is_ub:{type:'boolean'}, shipped_is_clean:{type:'boolean'}, sb_ran:{type:'boolean'}, tb_ran:{type:'boolean'}, evidence:{type:'string'} } }
  mirihar = await agent(
    `${rules}\n\nMIRIHAR PHASE. Per ${DESIGN}, add loopthread_naive (UB) + loopthread_shipped (CLEAN) candidates to ${MIRIHAR}/src/main.rs modelling the Loop read-thread construct-then-spawn (a struct with a queue-analogue + a spawned thread holding raw self reading+pushing while main pops+joins; tty stubbed). Run BOTH under SB (default) AND -Zmiri-tree-borrows; confirm shipped CLEAN under both, naive UB. Update the harness README + MIRI_EXCEPTIONS. Report naive_is_ub/shipped_is_clean/sb_ran/tb_ran + evidence (actual miri output).`,
    { label:'mirihar:vaxis-c3b', phase:'Mirihar', agentType:'general-purpose', schema:M_SCHEMA })
}

phase('Verify')
const V_SCHEMA = { type:'object', additionalProperties:false, required:['dimension','verdict','findings'], properties:{
  dimension:{type:'string'}, verdict:{enum:['PASS','FAIL']}, findings:{type:'array',items:{type:'string'}} } }
const verify = await parallel([
  () => agent(`${rules}\n\nVERIFY (GATES+MIRI). Independently re-run + confirm GREEN: cargo build --workspace; cargo test -p rustty-vaxis; cargo +nightly miri test -p rustty-vaxis (the tty FFI tests must be gated OUT of Miri, not failing it); cargo clippy -p rustty-vaxis --all-targets -- -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh; python3 .port/tools/gates/check_test_counts.py; python3 .port/tools/gates/check_comment_coverage.py. Report each. verdict PASS only if ALL green.`,
    { label:'verify:gates', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
  () => agent(`${rules}\n\nVERIFY (R1 + FFI/ALIASING). Diff rustty-vaxis chunk-3b vs ${VX}/src/{queue,tty,Loop}.zig: (1) every comment verbatim? (2) queue's 7 test{} + the exact Mutex/Condvar blocking semantics faithful + Miri-clean? (3) tty makeRaw termios flag manipulations EXACT (spot-check the c_iflag/c_lflag clears + VMIN/VTIME), the WINCH sigaction + getWinsize ioctl faithful, WindowsTty an R5 seam? (4) the Loop read-thread aliasing sound (no &mut *self while live; join before deinit; mirihar naive=UB/shipped=CLEAN under SB+TB if run)? (5) tty FFI tests correctly #[ignore]-on-device + in MIRI_EXCEPTIONS? (6) no lossy String? R4 acyclic (only libc added)? Report findings; verdict PASS only if R1 + FFI + aliasing sound.`,
    { label:'verify:fidelity', phase:'Verify', agentType:'Explore', schema:V_SCHEMA }),
])

return { design, adversary, port, mirihar, verify }
