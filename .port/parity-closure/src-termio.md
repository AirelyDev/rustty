# Parity closure checklist — src/termio

14 open finding(s): 0 high, 4 medium, 10 low. Full evidence per row is in [`docs/parity-audit/src-termio.md`](../../docs/parity-audit/src-termio.md).

A row is CLOSED only when its fix has a test that fails without it (or a written, reviewed justification for why no test is possible).


## Medium (4)

- [ ] **S211-F2** · CONFIRMED · `INDIV` · missing-branch
  - **What:** io read thread never sets crash.sentry.thread_state (POSIX and Windows read threads)
  - **Oracle:** `src/termio/Exec.zig:1411-1417, 1776-1781` → **Rustty:** `crates/rustty-termio/src/exec.rs:4143-4155, 4815-4817`
  - **Audit's suggested fix:** Call crash::sentry::setThreadState(ThreadState{type_: Type::Io, surface: <mailbox surface>}) at the top of both read-thread mains with a Drop guard calling clearThreadState().
- [ ] **S212-F1** · CONFIRMED · `ABI` · missing-branch
  - **What:** IO-thread failure screen loses the OpenptyFailed and InputNotFound/InputFailed messages
  - **Oracle:** `src/termio/Thread.zig:148-207` → **Rustty:** `crates/rustty-termio/src/thread.rs:363-400`
  - **Audit's suggested fix:** Give `crate::Error` the real variants (at minimum OpenptyFailed / InputNotFound / InputFailed, produced by exec's openpty path and `Termio::threadEnter`'s prepareInput/queueWrite failures) and restore the three-arm match in `Thread::threadMain`.
- [ ] **S212-F2** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** IO thread never sets crash.sentry.thread_state
  - **Oracle:** `src/termio/Thread.zig:235-240` → **Rustty:** `crates/rustty-termio/src/thread.rs:751-764`
  - **Audit's suggested fix:** Add a `surface()` accessor to `AppMailbox` (or a ThreadStateSurface impl in core) and set/clear `rustty_crash::sentry::thread_state` around the loop run in `Thread::threadMain_`, matching the reference's set-on-entry/clear-on-exit.
- [ ] **S214-F1** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** colorOperation drops the reference's 1024-byte fixed-buffer cap on the OSC color query response
  - **Oracle:** `src/termio/stream_handler.zig:1548-1556, 1764-1772` → **Rustty:** `crates/rustty-termio/src/stream_handler.rs:2288-2300, 2681-2691`
  - **Audit's suggested fix:** Bound the response accumulator to 1024 bytes (e.g. reuse the `FixedFmt` shim over a `[u8; 1024]`) and return `crate::Error` from `colorOperation` when a write would overflow, so the whole operation aborts without emitting a message, matching the reference.

## Low (10)

- [ ] **S211-F1** · CONFIRMED · `INDIV` · different-constant
  - **What:** ReadThread buffer_count/buffer_capacity halved on non-Darwin, and the cited reference source is wrong
  - **Oracle:** `src/termio/Exec.zig:1303-1318` → **Rustty:** `crates/rustty-termio/src/exec.rs:3925-3945`
  - **Audit's suggested fix:** Restore the pinned oracle values unconditionally and fix the citation comment, or record the split explicitly as an intentional divergence naming the oracle line it departs from.
- [ ] **S211-F3** · CONFIRMED · `INDIV` · missing-branch
  - **What:** threadEnter always spawns threadMainPosix; the Windows read-thread selection is missing
  - **Oracle:** `src/termio/Exec.zig:135-140` → **Rustty:** `crates/rustty-termio/src/exec.rs:495-515`
  - **Audit's suggested fix:** Cfg-split the read-thread spawn in threadEnter (and ReadThreadArgs' fd type) so the Windows build spawns ReadThread::threadMainWindows.
- [ ] **S211-F4** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** killPid abandons the child after a 10s deadline; the oracle loops until the child is reaped
  - **Oracle:** `src/termio/Exec.zig:1189-1215` → **Rustty:** `crates/rustty-termio/src/exec.rs:3393, 3540-3600`
  - **Audit's suggested fix:** Keep listed as an accepted R1 divergence rather than parity; consider upstreaming the bound so both trees agree.
- [ ] **S211-F5** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** threadEnter error arms perform an extra synchronous pty-master drain not present in the oracle
  - **Oracle:** `src/termio/Exec.zig:98-104` → **Rustty:** `crates/rustty-termio/src/exec.rs:3430-3480, 404-486`
  - **Audit's suggested fix:** Keep as a recorded divergence enumerated alongside the killPid bound so parity audits do not read it as a port bug.
- [ ] **S211-F6** · CONFIRMED · `INDIV` · missing-error-path
  - **What:** gather-thread spawn failure panics instead of logging and returning
  - **Oracle:** `src/termio/Exec.zig:1470-1479` → **Rustty:** `crates/rustty-termio/src/exec.rs:4232-4245`
  - **Audit's suggested fix:** Match on the Builder::spawn_scoped result and return from threadMainPosix on Err (the QuitFdCloser/IdlePipeCloser guards already handle cleanup).
- [x] **S211-F7** · CONFIRMED · `DOC` · other
  - **What:** Stale module-level PORT STATUS claims threadEnter/threadExit/ReadThread/queueWrite/Subprocess.start are deferred
  - **Oracle:** `src/termio/Exec.zig:1-16` → **Rustty:** `crates/rustty-termio/src/exec.rs:9-16`
  - **Audit's suggested fix:** Update the header to 'ported' and list only what genuinely remains (crash-sentry thread_state seam, Windows read-thread selection).
- [ ] **S212-F3** · CONFIRMED · `INDIV` · stub-or-unimplemented
  - **What:** Backend::childExitedAbnormally is unimplemented!() in production code
  - **Oracle:** `src/termio/backend.zig:88-100` → **Rustty:** `crates/rustty-termio/src/backend.rs:166-179`
  - **Audit's suggested fix:** Leave as-is, or gate the method behind `#[doc(hidden)]`/`cfg` so it cannot be reached from a Rust caller before the oracle regrows `Exec.childExitedAbnormally`.
- [ ] **S212-F5** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** log.warn/log.debug call sites in this area emit nothing
  - **Oracle:** `src/termio/mailbox.zig:73-104` → **Rustty:** `crates/rustty-termio/src/mailbox.rs:200-210`
  - **Audit's suggested fix:** When a logging facade is adopted workspace-wide, fill these helper bodies with the verbatim reference messages.
- [ ] **S213-F1** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** basename() shim does not strip trailing path separators (Zig's std.fs.path.basenamePosix does)
  - **Oracle:** `src/termio/shell_integration.zig:132-138` → **Rustty:** `crates/rustty-termio/src/shell_integration.rs:779-786`
  - **Audit's suggested fix:** Mirror basenamePosix: strip trailing separators before the rposition search (and keep an empty-path early return), matching the Zig std helper.
- [ ] **S213-F2** · CONFIRMED · `INDIV` · missing-error-path
  - **What:** setupBash's GHOSTTY_BASH_INJECT accumulator is a fixed 32-byte buffer in the oracle (NoSpaceLeft is reachable); rustty grows unboundedly
  - **Oracle:** `src/termio/shell_integration.zig:302-322` → **Rustty:** `crates/rustty-termio/src/shell_integration.rs:377-384, 402-410`
  - **Audit's suggested fix:** Either cap the accumulator at 32 bytes and return the equivalent error (Error variant) when it overflows, or document/justify the divergence as intentional in the PORT NOTE; the setupFeatures buffer note (provably unreachable) does not apply here.
