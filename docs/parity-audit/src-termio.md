# Parity audit — src/termio

Audited all 9 oracle units under `src/termio` at ghostty pin `20abdb50a` (Exec.zig, Options.zig, Termio.zig, Thread.zig, backend.zig, mailbox.zig, message.zig, shell_integration.zig, stream_handler.zig) against their rustty counterparts, mostly in `crates/rustty-termio`.
Coverage: **7124 of 7124 oracle lines read (100%)**, no partial units.
Findings: **14 CONFIRMED, 0 DISPUTED, 2 REFUTED, 0 UNVERIFIED** — by corrected severity: **0 high, 4 medium, 10 low**.
The most important gap is **S214-F1**: `colorOperation` drops the oracle's 1024-byte fixed-buffer cap on the OSC color-query response, so an over-long report is emitted in full (and the remaining requests still processed) where the oracle aborts the whole operation and emits nothing — an observable pty byte-stream _and_ terminal-state divergence.
Close behind are the two crash-sentry `thread_state` deferrals (S211-F2, S212-F2) and the collapsed three-way IO-thread startup failure screen (S212-F1).

## Coverage

| Oracle unit (path:lines)                  | Rustty counterpart(s)                                                                                                                                                                                     | Status  | Note                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| ----------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/termio/Exec.zig:1-2309`              | `crates/rustty-termio/src/exec.rs`                                                                                                                                                                        | ported  | 7159-line port covering Exec (init/deinit/initTerminal/threadEnter/threadExit/focusGained/resize/processExitCommon/processExit/flatpakExit/termiosTimer/queueWrite/ttyWrite/getProcessInfo split into getForegroundPid+getTtyName), ThreadData(+Write), Config, Subprocess (init/deinit/start/childPreExec/externalExit/stop/resize/getProcessInfo), Process union, ArgsFormatter, killCommand/killPid/getpgid/killCommandFlatpak, ReadThread (Pipeline + constants + threadMainPosix/gatherMainPosix/clearBridging/setNonblock/setQosClass/threadMainWindows), execCommand, appendEnv/appendEnvAlways. All 10 reference test blocks have counterparts with identical assertion counts and values.                                                                                                                                 |
| `src/termio/Options.zig:1-41`             | `crates/rustty-termio/src/options.rs`                                                                                                                                                                     | ported  | All 9 fields present with identical semantics (size, full_config, config, backend, mailbox, renderer_state, renderer_wakeup, renderer_mailbox, surface_mailbox). Zero reference tests.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `src/termio/Termio.zig:1-817`             | `crates/rustty-termio/src/termio_impl.rs`                                                                                                                                                                 | ported  | All 27 declarations present: MutexState, ThreadEnterState (create/destroy/prepareInput/Input), DerivedConfig (init/deinit incl. the palette-generate break-block), Termio init/deinit/threadEnter/threadExit/queueMessage/queueWrite/changeConfig/resize/sizeReport/sizeReportLocked/resetSynchronizedOutput/clearScreen/scrollViewport/jumpToPrompt/focusGained/processOutput/processOutputLocked/kittyClipboardGrant/colorSchemeReport(+Locked)/visibilityReport, ThreadData(+deinit). `getProcessInfo(comptime info)` is split into `getForegroundPid`/`getTtyName`; both halves present. Lock/unlock ordering, early returns, the 500 ms cursor-reset gate, the inspector slow path, and the alt-screen/prompt clearScreen branches all match. 0 oracle tests.                                                                 |
| `src/termio/Thread.zig:1-545`             | `crates/rustty-termio/src/thread.rs`                                                                                                                                                                      | partial | Struct shape, Coalesce.min_ms=25, sync_reset_ms=1000, selection_scroll_ms=15, flags, init/deinit, threadMain/threadMain_, CallbackData, drainMailbox (all 18 message arms incl. visibility_report and both kitty_clipboard_grant arms), startSynchronizedOutput, handleResize, startScrollTimer/stopScrollTimer and all five callbacks are ported with matching Canceled/else handling and rearm/disarm returns. Two gaps: the three-way startup-error screen collapses to one arm (opaque `crate::Error`), and `crash.sentry.thread_state` is never set. The Thread `alloc` field collapses into Rust ownership (arena only). 0 oracle tests.                                                                                                                                                                                     |
| `src/termio/backend.zig:1-129`            | `crates/rustty-termio/src/backend.rs`                                                                                                                                                                     | ported  | Kind, Config, Backend (deinit/initTerminal/threadEnter/threadExit/focusGained/resize/queueWrite/childExitedAbnormally/getProcessInfo), ThreadData (deinit/changeConfig no-op) and WRITE_REQ_PREALLOC=2^5 all present. `childExitedAbnormally` is `unimplemented!()`, matching the oracle's never-instantiated delegation. 0 oracle tests.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `src/termio/mailbox.zig:1-109`            | `crates/rustty-termio/src/mailbox.rs`                                                                                                                                                                     | ported  | Queue = BlockingQueue<Message,64>, Mailbox::Spsc{queue,wakeup}, initSPSC, deinit, send (fast instant push -> wakeup.notify -> unlock/forever-push/relock slow path) and notify all present with matching control flow; the commented-out `unbounded` variant is preserved as a comment as in the oracle. Explicit `msg.deinit()` frees happen via Rust Drop. 0 oracle tests.                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `src/termio/message.zig:1-146`            | `crates/rustty-termio/src/message.rs`                                                                                                                                                                     | ported  | All 18 union variants present, plus WriteReq=MessageData(u8,38), KittyClipboardGrant, SizeReport alias and writeReq (stable=>unreachable, small, alloc). `Message.deinit` collapses into Drop. Both oracle `test {}` blocks have counterparts; the size test pins 56 (Rust layout) rather than 40.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `src/termio/shell_integration.zig:1-1058` | `crates/rustty-termio/src/shell_integration.rs`, `crates/rustty-global/src/environ.rs`, `crates/rustty-os/src/shell.rs`, `crates/rustty-config/src/command.rs`, `crates/rustty-config/src/value_types.rs` | ported  | 1:1 port (1958 lines). Shell enum (5 variants + tag_name shim), ShellIntegration, setup, detectShell, setupFeatures, setupBash, setupXdgDataDirs, prependEnv (moved in per e8525c0fd), setupNushell, setupZsh, TmpResourcesDir. All 21 oracle test blocks have counterparts with matching assertion counts, plus 2 moved prependEnv tests and 3 extra byte-fidelity discriminators. Use-sites verified at exec.rs:2157/:2221 vs Exec.zig:772/800.                                                                                                                                                                                                                                                                                                                                                                                  |
| `src/termio/stream_handler.zig:1-1970`    | `crates/rustty-termio/src/stream_handler.rs`                                                                                                                                                              | ported  | 1:1 line-annotated port (4181 lines): StreamHandler and all 16 fields with matching defaults, deinit, queueRender, changeConfig, the three message writers, the whole vt/vtFallible action switch (~95 arms, distributed over the `stream::Handler` trait impl, including title_push/title_pop/kitty_dnd no-ops), dcsHook/dcsPut/dcsUnhook/dcsCommand (tmux + xtgettcap + decrqss), apcEnd, bell, tabs, linefeed/index/nextLine/reverseIndex, mode handling, hyperlinks, DA/DSR, decaln, cursor save/restore, enquiry, charsets, fullReset, kitty clipboard + keyboard, xtversion, titles, mouse shape, clipboard, semanticPrompt, reportPwd, colorOperation, notifications, sendSizeReport, kittyColorReport, progressReport. Both oracle tests have counterparts with the same assertions; 4 extra negative-control tests added. |

**Partial units:** one — `src/termio/Thread.zig`, for the two reasons recorded above (collapsed three-way startup-error screen, see S212-F1; missing `crash.sentry.thread_state`, see S212-F2). No other unit was partial and no unit was missing, replaced, or not-applicable.

## Findings

### High severity

None.

### Medium severity

#### S211-F2 — io read thread never sets crash.sentry.thread_state (POSIX and Windows read threads)

- **Status:** CONFIRMED (both lenses failed to refute)
- **Kind:** missing-branch
- **Oracle:** `src/termio/Exec.zig:1411-1417, 1776-1781`
- **Rustty:** `crates/rustty-termio/src/exec.rs:4143-4155, 4815-4817`

**Description.** Both oracle read-thread entry points set `crash.sentry.thread_state = .{ .type = .io, .surface = io.surface_mailbox.surface }` and clear it on exit, so a crash on the io-reader thread is attributed to the io thread and its surface. The port leaves this as a comment-only deferral in `threadMainPosix` and `threadMainWindows`, so a crash there carries no thread state. The seam it waits on already exists and is used elsewhere.

**Evidence.** ghostty Exec.zig:1411-1417 (threadMainPosix) and :1776-1781 (threadMainWindows). rustty exec.rs:4143-4155 is a "DEFERRED (cross-crate crash-sentry seam...)" comment with no call; exec.rs:4815-4817 says "identical to the posix arm". `grep -rn setThreadState crates` shows callers only in `rustty-core/src/surface.rs` and `renderer_seam.rs`, never in rustty-termio. The seam itself is at `crates/rustty-crash/src/sentry.rs:153` (setThreadState) / `:161` (clearThreadState).

**Suggested fix.** Call `crash::sentry::setThreadState(ThreadState{type_: Type::Io, surface: <mailbox surface>})` at the top of both read-thread mains with a Drop guard calling `clearThreadState()`.

#### S212-F1 — IO-thread failure screen loses the OpenptyFailed and InputNotFound/InputFailed messages

- **Status:** CONFIRMED (both lenses failed to refute)
- **Kind:** missing-branch
- **Oracle:** `src/termio/Thread.zig:148-207`
- **Rustty:** `crates/rustty-termio/src/thread.rs:363-400`

**Description.** `Thread.threadMain`'s error handler switches on the error and paints one of THREE distinct screens: a pty-exhaustion message (`error.OpenptyFailed`), a configured-`input`-path message (`error.InputNotFound`/`error.InputFailed`), and the generic `error starting IO thread: {}` message. The Rust port's `crate::Error` (lib.rs:148) is an opaque unit struct carrying no discriminant, so the two special-cased arms are unreachable and every startup failure paints the generic screen; additionally the interpolated `{err}` prints the fixed string "termio operation failed" instead of the Zig error name. The two message bodies exist only inside a comment block (thread.rs:251-279).

**Evidence.** Oracle Thread.zig:144-207 `const Err = @TypeOf(err) || error{OpenptyFailed, InputNotFound, InputFailed}; switch (@as(Err, @errorCast(err))) { error.OpenptyFailed => ... t.printString(str); error.InputNotFound, error.InputFailed => ... ; else => ... }`. rustty thread.rs:368-380 PORT NOTE: "`crate::Error` is the OPAQUE termio seam error (lib.rs:122); it carries NO discriminant, so the reference's error switch cannot dispatch ... Every error collapses to the general `else` arm below." thread.rs:381-390 builds only the generic `format!("error starting IO thread: {err}\n ...")`. Workspace greps for `OpenptyFailed` / "cannot allocate any more pty" hit only comments plus an unrelated `PrepareInputError::InputNotFound` (termio_impl.rs:255/294) that is erased at termio_impl.rs:884 via `map_err(|_| Error)?`.

**Suggested fix.** Give `crate::Error` the real variants (at minimum OpenptyFailed / InputNotFound / InputFailed, produced by exec's openpty path and `Termio::threadEnter`'s prepareInput/queueWrite failures) and restore the three-arm match in `Thread::threadMain`.

#### S212-F2 — IO thread never sets crash.sentry.thread_state

- **Status:** CONFIRMED (both lenses failed to refute)
- **Kind:** behavioral-difference
- **Oracle:** `src/termio/Thread.zig:235-240`
- **Rustty:** `crates/rustty-termio/src/thread.rs:751-764`

**Description.** `threadMain_` sets the per-thread crash metadata `crash.sentry.thread_state = .{ .type = .io, .surface = io.surface_mailbox.surface }` on entry and clears it (`defer ... = null`) on exit, so a crash captured on the writer thread is tagged with the thread type and the owning surface. The Rust `threadMain_` preserves this only as a comment and explicitly defers it ("the `AppMailbox` trait does not yet expose a `.surface` accessor"), so no crash report produced from the io thread carries the thread-state block. `rustty_crash::sentry::ThreadState` exists (crates/rustty-crash/src/sentry.rs:95) but is never populated from termio.

**Evidence.** Oracle Thread.zig:234-240. rustty thread.rs:751-764: `// crash.sentry.thread_state = .{ ... }` / `// defer crash.sentry.thread_state = null;` followed by "DEFERRED (cross-crate crash-sentry seam, not part of B4) ... Preserved verbatim above; wired when core lands." `grep thread_state` in crates/rustty-termio yields only comment lines (435, 439, 753, 757, 760); the only real assignments in the workspace are in rustty-core (surface.rs, renderer_seam.rs).

**Suggested fix.** Add a `surface()` accessor to `AppMailbox` (or a ThreadStateSurface impl in core) and set/clear `rustty_crash::sentry::thread_state` around the loop run in `Thread::threadMain_`, matching the reference's set-on-entry/clear-on-exit.

#### S214-F1 — colorOperation drops the reference's 1024-byte fixed-buffer cap on the OSC color query response

- **Status:** CONFIRMED (both lenses failed to refute)
- **Kind:** behavioral-difference
- **Oracle:** `src/termio/stream_handler.zig:1548-1556, 1764-1772`
- **Rustty:** `crates/rustty-termio/src/stream_handler.rs:2288-2300, 2681-2691`

**Description.** The oracle builds the OSC color-query response into a `std.Io.Writer.Allocating` backed by a 1024-byte `FixedBufferAllocator` (`var buffer: [1024]u8 = undefined; var fba: ... = .init(&buffer);`). Once the accumulated report exceeds 1024 bytes, `try response.writer.print(...)` fails with WriteFailed, `colorOperation` returns the error, `vt` logs it, and NO response is written at all — and the remaining requests in the `color.List` (including their palette/dynamic-color side effects) are never processed. The rustty port replaces the fixed-buffer allocator with an unbounded `Vec<u8>` accumulator, so an over-long report is emitted in full and every remaining request is still processed. `osc::color::List` is an unbounded SegmentedList (ghostty src/terminal/osc/parsers/color.zig:333), so a single OSC 4 carrying ~40+ `?` queries (a 16-bit palette report is ~26 bytes each, ~240 input bytes) crosses the cap and produces a different byte stream on the pty and a different terminal state.

**Evidence.** ghostty src/termio/stream_handler.zig:1548-1556 `var buffer: [1024]u8 = undefined; var fba: std.heap.FixedBufferAllocator = .init(&buffer); const alloc = fba.allocator(); var response: std.Io.Writer.Allocating = .init(alloc);` and every write via `try response.writer.print(...)` / `try response.writer.writeAll(terminator.string())`, with the flush at :1764-1770 never reached on error. rustty crates/rustty-termio/src/stream_handler.rs:2296-2300 `// (PORT NOTE: the fixed-buffer allocator + unmanaged list collapse into the owned response Vec/String accumulator.) let mut response: Vec<u8> = Vec::new();` — all subsequent `write!(VecFmt(&mut response), ...)` calls are infallible against a Vec, so the `map_err(|_| crate::Error)?` arm is dead. Greps for `1024` in that file find no bounded shim at this site.

**Suggested fix.** Bound the response accumulator to 1024 bytes (e.g. reuse the `FixedFmt` shim over a `[u8; 1024]`) and return `crate::Error` from `colorOperation` when a write would overflow, so the whole operation aborts without emitting a message, matching the reference.

### Low severity

#### S211-F1 — ReadThread buffer_count/buffer_capacity halved on non-Darwin, and the cited reference source is wrong

- **Status:** CONFIRMED (both lenses failed to refute; severity corrected medium -> low by the semantic lens)
- **Kind:** different-constant
- **Oracle:** `src/termio/Exec.zig:1303-1318`
- **Rustty:** `crates/rustty-termio/src/exec.rs:3925-3945` (corrected: `:3934-3946`)

**Description.** The oracle at pin 20abdb50a declares `const buffer_count = 4;` and `const buffer_capacity = 64 * 1024;` unconditionally. The port declares those values only for macos/ios and uses `BUFFER_COUNT=2` / `BUFFER_CAPACITY=8*1024` elsewhere, and its verbatim-reference comment claims the oracle reads `if (builtin.os.tag.isDarwin()) 4 else 2` — which the pinned oracle does not say. On Linux/FreeBSD this changes the ring depth (child flow-control backpressure point) and the parse-stage batch size / terminal-lock hold time. Severity was corrected to low because the delivered byte stream and terminal state are identical; it is kept as a finding because a pinned oracle constant differs and the "preserved verbatim" citation misquotes the oracle.

**Evidence.** ghostty/src/termio/Exec.zig:1305 `const buffer_count = 4;` and :1318 `const buffer_capacity = 64 * 1024;`, no isDarwin conditional. rustty exec.rs:3933-3947 `#[cfg(any(target_os="macos",target_os="ios"))] BUFFER_COUNT = 4; #[cfg(not(...))] BUFFER_COUNT = 2;` and the same for BUFFER_CAPACITY, above the false `// const buffer_count = if (builtin.os.tag.isDarwin()) 4 else 2;` citation. Greps of every `*.md` in the repo for `buffer_count`/`BUFFER_COUNT`/`8 * 1024`/`isDarwin()) 4 else 2` return zero hits, so the split is not recorded as an intentional divergence anywhere (unlike KILL_ABANDON/drain, which are in the `.port` DEFERRALS docs).

**Suggested fix.** Restore the pinned oracle values unconditionally and fix the citation comment, or record the split explicitly as an intentional divergence naming the oracle line it departs from.

#### S211-F3 — threadEnter always spawns threadMainPosix; the Windows read-thread selection is missing

- **Status:** CONFIRMED (both lenses failed to refute; severity corrected medium -> low by the presence lens)
- **Kind:** missing-branch
- **Oracle:** `src/termio/Exec.zig:135-140`
- **Rustty:** `crates/rustty-termio/src/exec.rs:495-515`

**Description.** The oracle selects the read-thread entry at comptime (`if (builtin.os.tag == .windows) ReadThread.threadMainWindows else ReadThread.threadMainPosix`). The port unconditionally spawns `ReadThread::threadMainPosix::<M>` with a RawFd-typed `ReadThreadArgs`, while the ported `threadMainWindows` is `#[cfg(windows)]` and has no caller anywhere in the workspace, so on a Windows target the ported Windows read thread is dead code and `threadEnter` (which is itself not cfg-gated) names a cfg'd-out function. Severity was corrected to low because `crates/rustty-termio/src/lib.rs:35-38` explicitly records "the Windows spawn/read arms (exec.rs threadMainWindows + windows Subprocess.start)" as honest residual seams outside the locked macOS-first fork, pointing at DEFERRALS.md.

**Evidence.** ghostty Exec.zig:135-140 (comptime selection at :139). rustty exec.rs:506-514 `let read_args = ReadThreadArgs::<M>(pty_fds.read, io, pipe[0]); ... ReadThread::threadMainPosix::<M>(args.0, args.1, args.2);` with no cfg; `threadMainWindows` is defined at exec.rs:4792 (`#[cfg(windows)]`) and referenced only in comments (exec.rs:279, 498) and doc text. The rest of the file does keep live `#[cfg(windows)]` arms (755, 1652, 3287, 3499, 5142...), so the target is otherwise intended to be reachable.

**Suggested fix.** Cfg-split the read-thread spawn in `threadEnter` (and `ReadThreadArgs`' fd type) so the Windows build spawns `ReadThread::threadMainWindows`.

#### S211-F4 — killPid abandons the child after a 10s deadline; the oracle loops until the child is reaped

- **Status:** CONFIRMED (both lenses failed to refute; severity corrected medium -> low by the presence lens)
- **Kind:** behavioral-difference
- **Oracle:** `src/termio/Exec.zig:1189-1215`
- **Rustty:** `crates/rustty-termio/src/exec.rs:3393, 3540-3600`

**Description.** The oracle's `killPid` loops forever (killpg SIGHUP + waitpid WNOHANG + 10ms sleep) until the child is reaped. The port adds `KILL_ABANDON = 10s` and returns `KillError::Abandoned` past that deadline, leaving the process untracked and its exit status uncollected. Deliberate and documented, but observably different for any teardown exceeding 10s. The finding's zombie claim is partly overstated: exec.rs:3580-3588 documents rustty's xev `.proc` completion as a second reaper, so an abandoned-but-reapable child is still collected there.

**Evidence.** ghostty Exec.zig:1189-1215 — unbounded `while (true) { killpg...; waitpid(pid, null, W.NOHANG); if (res_pid != 0) break; sleep 10ms }`. rustty exec.rs:3393 `const KILL_ABANDON: Duration = Duration::from_secs(10);`, KillError::Abandoned at exec.rs:3300-3304, deadline return at exec.rs:3585-3596 flagged "[R1-DIVERGENCE] hard deadline — the ONLY divergence in this loop", with a divergence banner at 3307-3388. Recorded as accepted in `.port/phase-2/DEFERRALS.md`, `.port/phase-3/DEFERRAL_REAUDIT.md` and `.port/phase-3/assessments/E-os-termio-platform.md`.

**Suggested fix.** Keep listed as an accepted R1 divergence rather than parity; consider upstreaming the bound so both trees agree.

#### S211-F5 — threadEnter error arms perform an extra synchronous pty-master drain not present in the oracle

- **Status:** CONFIRMED (both lenses failed to refute)
- **Kind:** behavioral-difference
- **Oracle:** `src/termio/Exec.zig:98-104`
- **Rustty:** `crates/rustty-termio/src/exec.rs:3430-3480, 404-486`

**Description.** The port adds `drain_pty_master_once` (bounded by `DRAIN_BUDGET = 50ms`) on every `threadEnter` error arm (ProcessNotStarted / ProcessNoPid / xev::Process::init / pipe() / Timer::init / read-thread spawn failure) before `subprocess.stop()`. The oracle has no such call — those arms go straight to `errdefer self.subprocess.stop()`. Bytes already written by the child are consumed and discarded and up to 50ms of extra syscalls run on teardown.

**Evidence.** ghostty Exec.zig:104 is a bare `errdefer self.subprocess.stop();` and the oracle file contains no pty-master drain anywhere. rustty exec.rs:3430 `fn drain_pty_master_once(fd: libc::c_int) -> usize` with DRAIN_BUDGET at exec.rs:3395-3399 and call sites at exec.rs:404, 417/419, 428, 454/457, 486/488, each marked "WS19 T17a [R1-DIVERGENCE]".

**Suggested fix.** Keep as a recorded divergence enumerated alongside the killPid bound so parity audits do not read it as a port bug.

#### S211-F6 — gather-thread spawn failure panics instead of logging and returning

- **Status:** CONFIRMED (both lenses failed to refute)
- **Kind:** missing-error-path
- **Oracle:** `src/termio/Exec.zig:1470-1479`
- **Rustty:** `crates/rustty-termio/src/exec.rs:4232-4245`

**Description.** The oracle handles a failed gather-thread spawn by logging "read thread exiting, failed to spawn gather thread" and returning normally from `threadMainPosix`. The port uses `spawn_scoped(...).expect("spawn io-gather")`, which panics and unwinds out of the read thread instead of returning cleanly, and emits no error log.

**Evidence.** ghostty Exec.zig:1470-1479 `} catch |err| { log.err("read thread exiting, failed to spawn gather thread err={}", .{err}); return; };`. rustty exec.rs:4242-4246 `.spawn_scoped(scope, move || { ... }).expect("spawn io-gather");` with a PORT NOTE at exec.rs:4224-4233 conceding `std::thread::scope` has no fallible spawn and that failure "surfaces as the same abort Rust already uses". No alternate handling elsewhere in the workspace.

**Suggested fix.** Match on the `Builder::spawn_scoped` result and return from `threadMainPosix` on `Err` (the QuitFdCloser/IdlePipeCloser guards already handle cleanup).

#### S211-F7 — Stale module-level PORT STATUS claims threadEnter/threadExit/ReadThread/queueWrite/Subprocess.start are deferred

- **Status:** CONFIRMED (both lenses failed to refute)
- **Kind:** other (documentation accuracy)
- **Oracle:** `src/termio/Exec.zig:1-16`
- **Rustty:** `crates/rustty-termio/src/exec.rs:9-16`

**Description.** The file header still reads "PORT STATUS: ported ... EXCEPT the xev event-loop wiring (threadEnter/threadExit/focusGained/processExit*/termiosTimer/queueWrite/ttyWrite/ReadThread ...) and Subprocess.start". Every one of those is live in this file (several inner headers are explicitly marked "STALE HEADER STRUCK"). The stale header invites phantom-gap reports. Doc-only; no observable behaviour differs.

**Evidence.** rustty exec.rs:9-16 header vs live bodies at exec.rs:345 (threadEnter), 703 (threadExit), 816 (focusGained), 1043 (processExitCommon), 1098 (processExit), 1147 (termiosTimer), 1360 (queueWrite), 1539 (ttyWrite), 2582 (Subprocess::start, real posix+windows body), 3919 (impl ReadThread, exercised by real-pty tests at exec.rs:6560-6771). `grep -c 'unimplemented!()' exec.rs` returns 0. Inner struck-header notes at exec.rs:895 ("[WS20] STALE HEADER STRUCK") and exec.rs:1290 ("[W6 STALE HEADER STRUCK]") — the finding cited 886 and 1330.

**Suggested fix.** Update the header to "ported" and list only what genuinely remains (crash-sentry thread_state seam, Windows read-thread selection).

#### S212-F3 — Backend::childExitedAbnormally is unimplemented!() in production code

- **Status:** CONFIRMED (both lenses failed to refute)
- **Kind:** stub-or-unimplemented
- **Oracle:** `src/termio/backend.zig:88-100`
- **Rustty:** `crates/rustty-termio/src/backend.rs:166-179`

**Description.** The Rust `Backend::childExitedAbnormally` body is `unimplemented!(...)`. This is defensible and matches the oracle's effective behaviour: `termio.Exec` declares no `childExitedAbnormally` at pin 20abdb50a and nothing calls `Backend.childExitedAbnormally` (only Surface.zig has its own same-named private fn), so Zig's lazy compilation never type-checks the delegation. Recorded for completeness: the Rust method is publicly callable and would panic, whereas the Zig one would fail to compile.

**Evidence.** Oracle backend.zig:88-100 delegates `.exec => |*exec| try exec.childExitedAbnormally(gpa, t, exit_code, runtime_ms)`; grep across /Users/gael/projects/ghostty/src finds `childExitedAbnormally` only at Surface.zig:1277/1330 and backend.zig:88/96. rustty backend.rs:171-185 `Backend::Exec(_exec) => unimplemented!("Backend.childExitedAbnormally: uninstantiated in the reference ...")`.

**Suggested fix.** Leave as-is, or gate the method behind `#[doc(hidden)]`/`cfg` so it cannot be reached from a Rust caller before the oracle regrows `Exec.childExitedAbnormally`.

#### S212-F5 — log.warn/log.debug call sites in this area emit nothing

- **Status:** CONFIRMED (both lenses failed to refute)
- **Kind:** behavioral-difference
- **Oracle:** `src/termio/mailbox.zig:73-104` (and the log sites in Termio.zig / Thread.zig)
- **Rustty:** `crates/rustty-termio/src/mailbox.rs:200-210`

**Description.** Every `log.*` call site in Termio.zig, Thread.zig and mailbox.zig (e.g. "failed to wake up writer, data will be dropped", "failed to notify writer, data will be dropped", "failed to open input file=...", "mailbox message={s}", "abrupt io thread exit detected, starting xev to drain mailbox", "error draining mailbox err=...") is ported either as an empty helper function or as a bare comment, so the diagnostics never reach stderr. This is a workspace-wide convention (no logging facade is wired in rustty), so it is cosmetic rather than a logic difference, but the observable stderr output differs from the oracle.

**Evidence.** Oracle mailbox.zig:79-82 and 99-103 `log.warn("failed to wake up writer, data will be dropped err={}", .{err});` / `log.warn("failed to notify writer, ...")`. rustty mailbox.rs:200-210 `fn log_warn_notify_failed(_err: &xev::NotifyError) { // log.warn(...) }` — empty bodies; same pattern at termio_impl.rs:1961-1964 and thread.rs:337, 405-411, 1077. A workspace grep for `log::warn`/`tracing::warn` matches only rustty-util/src/tripwire.rs; rustty-termio has no logging at all (only two `eprintln!` in exec.rs test benches).

**Suggested fix.** When a logging facade is adopted workspace-wide, fill these helper bodies with the verbatim reference messages.

#### S213-F1 — basename() shim does not strip trailing path separators (Zig's std.fs.path.basenamePosix does)

- **Status:** CONFIRMED (both lenses failed to refute)
- **Kind:** behavioral-difference
- **Oracle:** `src/termio/shell_integration.zig:132-138` (use site at :187)
- **Rustty:** `crates/rustty-termio/src/shell_integration.rs:779-786` (corrected: `:782-787`, use site at `:194`)

**Description.** `detectShell()` computes `const exe = std.fs.path.basename(arg0)`. Zig's `basenamePosix` first walks back over trailing `/` characters before locating the last separator, so `basename("/usr/bin/bash/") == "bash"`. The rustty shim is a bare `rposition(sep)` with no trailing-separator strip, so the same input yields an empty slice and the shell is not detected (`setup` returns None, shell integration silently disabled). Zig's `basename` also dispatches to `basenameWindows` on Windows (both separators plus drive prefixes); the shim only tests `MAIN_SEPARATOR`. Note the workspace _does_ contain a faithful port of Zig's basename at `crates/rustty-config/src/path.rs:838-909` (with the trailing-separator strip and Windows dispatch), but it is `pub(crate)` to rustty-config and is not what this use site calls. Empty path, `/` and `//bash` behave identically on both sides, so the blast radius is trailing separators only.

**Evidence.** oracle shell_integration.zig:133/:187 `const exe = std.fs.path.basename(arg0);`. rustty shell_integration.rs:782-787 `fn basename(path: &[u8]) -> &[u8] { let sep = std::path::MAIN_SEPARATOR as u8; match path.iter().rposition(|&b| b == sep) { Some(idx) => &path[idx + 1..], None => path } }`. Input `b"/usr/bin/bash/"`: oracle -> "bash" -> Shell.bash; rustty -> `b""` -> None.

**Suggested fix.** Mirror `basenamePosix`: strip trailing separators before the rposition search (and keep an empty-path early return), matching the Zig std helper — or route this use site at the existing `rustty-config/src/path.rs` implementation.

#### S213-F2 — setupBash's BASH_INJECT accumulator is a fixed 32-byte buffer in the oracle (NoSpaceLeft is reachable); rustty grows unboundedly

- **Status:** CONFIRMED (both lenses failed to refute)
- **Kind:** missing-error-path
- **Oracle:** `src/termio/shell_integration.zig:302-322` (consumed at :386-390)
- **Rustty:** `crates/rustty-termio/src/shell_integration.rs:377-384, 402-410` (corrected: `:382`, consumed at `:492`)

**Description.** The oracle writes the inject flag list into `var buf: [32]u8` via a fixed `std.Io.Writer`; `try inject.writeAll(" --norc")` returns `error.NoSpaceLeft` once the accumulated text exceeds 32 bytes, which propagates out of `setupBash`/`setup` — and `Exec.zig:800 const integration = try shell_integration.setup(...)` has no catch (only an `orelse` null arm), so the oracle hard-errors subprocess init rather than injecting. Rustty replaces the fixed buffer with a growable `String`, so the error path cannot occur and the value keeps growing. Reachable with a repeated flag, e.g. `bash --norc --norc --norc --norc --norc` (1 + 5*7 = 36 > 32): oracle errors out, rustty succeeds and exports a 36-byte inject value.

**Evidence.** oracle shell_integration.zig:302-304 `var buf: [32]u8 = undefined; var inject: std.Io.Writer = .fixed(&buf); try inject.writeAll("1");`, appends at :322/:326, consumed at :386-390 `try env.put("GHOSTTY_BASH_INJECT", buf[0..inject.end]);`. rustty shell_integration.rs:382 `let mut inject = String::from("1");` with the PORT NOTE "(PORT NOTE: the fixed 32-byte writer collapses into a `String`.)", pushes at :405/:407/:411 and `env.put(&env_name("BASH_INJECT"), inject.as_bytes());` at :492 — no 32-byte bound and no error arm.

**Suggested fix.** Either cap the accumulator at 32 bytes and return the equivalent error (`crate::Error` variant) when it overflows, or document/justify the divergence explicitly in the PORT NOTE; the `setupFeatures` buffer note (provably unreachable) does not apply here.

## Performance-only differences (not findings)

- Pipeline buffer storage is heap-allocated and zeroed (`alloc_zeroed` + Box) rather than the oracle's stack-local `undefined` array — identical observable behaviour, done to fit Rust's 2 MiB thread stacks.
- `std::thread::scope` replaces the oracle's spawn + `defer join()` for the gather thread; same join semantics including on unwind.
- `threadExit` moves the JoinHandle out via `mem::replace` with a freshly spawned already-finished no-op thread so `exec` stays fully initialized — one extra trivial thread spawn per teardown, no behavioural effect.
- `Instant::now()` replaces `std.Io.Timestamp.now(io, .awake)/untilNow().toMilliseconds()` for process runtime; the oracle's fallible-now `catch break :runtime null` arm collapses to an infallible `Some(...)`, with the same `orelse 0` value reaching the ChildExited message.
- Explicit EINTR retry loops around read/poll in `gatherMainPosix` and drain replace Zig std's internal EINTR retries in `posix.read`/`posix.poll` — identical results.
- `getProcessInfo(comptime info)` is split into `getForegroundPid`/`getTtyName` (comptime-dispatch flattening); both halves present with identical null-on-missing-pty behaviour (in both Exec.zig and Termio.zig).
- `MemoryPool` owns its child allocator, so the oracle's per-call `create(alloc)`/`deinit(alloc)` allocator parameter collapses; the pooled 64-byte Write nodes and their pointer stability are preserved.
- `Command`'s posix stdin/stdout/stderr are `Option<&File>` rather than the oracle's `std.Io.File{ .handle, .flags = .{ .nonblocking = false } }`; `setupFd` reads only the handle in both trees, so the dropped flags field is not observable.
- `termio_impl.rs ThreadEnterState::prepareInput` clones the `.raw` input bytes (`Input::String(v.clone())`) where the oracle borrows the arena slice — an extra allocation only, taken deliberately to avoid a long-lived borrow into `thread_enter_state` across `queueWrite` (Stacked-Borrows hazard). Observable behaviour identical.
- `termio_impl.rs colorSchemeReportLocked`/`visibilityReportLocked` encode into a `Vec::with_capacity(MAX_..._ENCODE_SIZE)` instead of the oracle's stack `[N]u8` + `std.Io.Writer.fixed`. Since the capacity is the encoder's advertised maximum, the overflow error path is unreachable in both; identical bytes are written. (`sizeReportLocked`/`focusGained` do use the stack `FixedWriter`, matching the oracle including its NoSpaceLeft error.)
- `mailbox.rs` uses `BlockingQueue::push_or_return` (hand-back-on-failure) for the instant push where Zig re-pushes its by-value copy; and relies on Rust `Drop` (queue Box drop, `Vec`/`Box` payload drops) where the oracle calls `Message.deinit()` explicitly in `Mailbox.deinit`, the drain loop, and the failed forever-push. The frees performed are the same set.
- `thread.rs coalesceCallback` uses `Option::take()` to read-and-clear `coalesce_data.resize` where the oracle reads then assigns null — identical ordering and effect.
- `termio_impl.rs init` calls `scrollback_limits(opts.full_config)` twice (once per tuple element) instead of once; both calls are pure `.optional()` reads.
- The Thread `alloc` field collapses into Rust ownership (arena only).
- Arena/Allocator parameters in shell_integration (`alloc_arena`, `stackFallback(4096, ...)`, `ShellCommandBuilder.init(alloc)`) collapse into Rust ownership / owned Vec+String buffers — `cmd.written()` bytes are byte-identical to the Zig builder's.
- `setupFeatures`'s comptime field reflection + comptime `sortUnstable(ascii.orderIgnoreCase)` is materialized as a hardcoded sorted array `[cursor, path, ssh-env, ssh-terminfo, sudo, title]`; verified against Config.zig:8824-8831's field set — same order, same output string.
- Zig `std.Io.Dir.openDirAbsolute(...)` / `openFileAbsolute(...)` probes are `std::fs::read_dir` / `std::fs::File::open` in rustty; the success/failure predicate is the same, only the syscall shape differs.
- `EnvMap` is a Rust `HashMap<Vec<u8>,Vec<u8>>` instead of Zig's ArrayHashMap; `swapRemove` vs `remove` ordering is unobservable (documented at crates/rustty-global/src/environ.rs:64-72) since the map is only iterated to build envp.
- `ArgIterator::deinit()` is a no-op in rustty (buffer freed on Drop); the reference's `defer iter.deinit()` has no observable effect.
- Product-identity renames (`GHOSTTY_*` -> `RUSTTY_*` via rustty_identity::ENV_PREFIX, `ghostty.bash` -> `rustty.bash`, `use ghostty *` -> `use rustty *`) are the locked R2 identity fork, applied consistently in production code and ported tests — not a parity gap.
- `xtgettcapMap()`: the oracle's `comptime terminfo.ghostty.xtgettcapMap()` is realized as a process-lifetime `OnceLock<HashMap<Vec<u8>,Vec<u8>>>` (stream_handler.rs:3620-3627). Same lookups, same 'static response slices.
- The `@branchHint(.likely/.unlikely/.cold)` annotations in `vtFallible` are preserved only as comments (Rust has no stable branch-hint intrinsic); pure codegen hint.
- `vtFallible`'s comptime `switch (action)` is distributed across `impl stream::Handler for StreamHandler` methods; every arm is present with the same body.
- Kitty clipboard read/write request arenas and the OSC 7 stackFallback arena collapse into owned `Box`/`Vec` values carrying the same data across the surface-mailbox boundary.
- `windowTitle` zeroes its 256-byte buffer where the oracle leaves it `undefined`; both write a NUL terminator at the title/pwd length, so the consumed value is identical.

## Refuted candidates

- **S212-F4 — Message size anti-growth test pins 56 instead of the oracle's 40.** Refuted by both lenses: pure Rust-vs-Zig layout consequence with the same single anti-growth assertion and the same observable contract (the `MessageData(u8, 38)` small/alloc switchover point is identical on both sides); no behavioural difference.
- **S214-F2 — Kitty clipboard OOM paths do not re-raise the error after replying.** Refuted by both lenses: the rustty arms issue the same EIO/EINVAL finishes and the same state transitions, and the OOM variants are never constructed under Rust's aborting allocator (rustty-vt clipboard_command.rs / clipboard_write.rs carry explicit "OutOfMemory carried for parity" notes), so the only delta is a log line on an unreachable branch.

**Unverified (over the verification cap):** none — every finding in this area was verified by both lenses.
