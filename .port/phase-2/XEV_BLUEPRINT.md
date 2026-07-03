# XEV_BLUEPRINT — porting libxev into `rustty-xev` (D6)

Architect blueprint. Every claim is grounded in the pinned libxev source
(`…/libxev-0.0.0-86vtc4…/src`) and ghostty's usage. Scope = the **minimal
faithful port that boots a live shell on macOS/kqueue**. Decision-oriented;
not a line-by-line spec.

**Adversarially reviewed: ACCEPT-WITH-FIXES (2026-07-03) — folded in below** (marked
`[ADV-FIX n]`). The core architecture (flat kqueue types, raw-ptr intrusive model, darwin
mach-port wakeup, in-crate queue/mpsc/heap, libc-direct no-cycle, two-milestone split) survived;
Milestone A is executable as described once these land:
- **[F8, BLOCKING for Milestone B] Milestone B narrows to TERMIO ONLY.** `renderer.Thread` is
  GPU-*and*-xev-blocked (core `renderer_seam.rs:381` = "Phase-3 R-d + D6 xev"), so landing
  rustty-xev does NOT unblock it — listing it would false-green. `App.zig` has no xev loop. The
  genuinely-xev-unblockable live milestone is termio alone (Thread run loop + Termio.init/
  threadEnter/processOutput + Exec Subprocess.start/ReadThread/queueWrite → a live PTY shell). Move
  renderer.Thread to the Phase-3 GPU tier; reframe "core app loop" as core `Async::notify()` leaves
  + render-thread *construction* only. (§6 fixed.)
- **[F2, BLOCKING] `Send`/`Sync` discipline** (compile-time blocker, silent in the draft). The
  intrusive raw-ptr types (`Loop`, `Completion`, `WriteQueue`) are auto-`!Send`, yet ghostty MOVES
  a `Loop` onto a spawned thread (termio Thread) and shares `Async` across threads (`notify()`).
  Add explicit `unsafe impl Send` for `Loop`/`Completion`/`WriteQueue`-bearing types with a
  thread-affinity safety note; `Async{port:u32}` is auto-`Send`. Needed by KATs 3/4 + all of
  Milestone B. (§2 fixed.)
- **[F3, downstream typecheck] The placeholder derives are LOAD-BEARING** and can't survive as
  `#[derive]`. termio's `xev_seam.rs` is `pub use rustty_xev::*`, and downstream default-constructs
  completions: `Completion: Default` (termio `c_xev=.{}`; core renderer_seam ~8 completion fields;
  flatpak), `WriteQueue`/`WriteRequest: Default`, `Async: Copy`, `NotifyError:
  Debug+Clone+Copy+PartialEq+Eq` (core `surface::Error` embeds it). The real `Completion` has fn-ptr
  + raw-ptr fields → hand-write `impl Default` (callback=noop, next=null, op=noop, state=dead;
  faithful to Zig field defaults), NOT `#[derive(Default)]`. Any missing trait breaks termio/core
  compilation the moment the placeholder is filled. (§2.1 fixed.)
- **[F1] Loop public surface fix:** add `add`/`stop`/`stopped`/`now` (ghostty calls `loop.stop`×3 +
  `loop.stopped`×2; `add` is the central dispatch); DROP `stop_completion` — it doesn't exist
  (cancellation is the `.cancel` op + `process_cancellations`). (§2.3/C3 fixed.)
- **[F4] `kevent64`/`kevent64_s` ARE in the `libc` crate** (libc-0.2.186 apple/mod.rs) — use them,
  no new extern needed; the "new binding" claim + its risk are dropped. (§3 fixed.)
- **[F6] Do NOT make `rustty_global::xev` re-export rustty-xev's types** (creates
  `global→xev→global` cycle). Resolve flatpak (`os/flatpak.rs`) + vt-search's xev refs via direct
  `os→xev` / `vt→xev` edges (both acyclic, xev deps neither). (§5 note added.)
- **[F7, hard prereq for Milestone A R1-green] the count-gate cannot see libxev** (content-addressed
  zig-cache path, not `ghostty/src`). Add a `rustty-xev` manifest/mapping to `check_test_counts.py`
  (a libxev base path or an in-repo manifest of the in-scope counts) — this is REQUIRED, not
  optional, for R1 to be enforced. Contract numbers must match the gate's `zig_count` exactly. (§7.)

---

## 0. The one fact that shapes everything: on macOS the Dynamic layer is comptime-elided

ghostty does `pub const xev = @import("xev").Dynamic;` (`global.zig:57`) and
`Dynamic = DynamicXev(Backend.candidates())` (`main.zig:68`).

- `Backend.candidates()` on macOS returns `&.{.kqueue}` — **a single element**
  (`backend.zig:31`).
- `dynamic.zig:34-39`: `pub fn Xev(bes)` → **`if (bes.len == 1) return bes[0].Api();`**

Therefore on macOS `xev.Dynamic` **is** `Kqueue.Api()` = the static
`Xev(.kqueue, kqueue.zig)` struct. `Kqueue` has `pub const dynamic = false`
(`api.zig:20`), so `global.zig:123`'s `if (comptime xev.dynamic) xev.detect()`
is **dead-compiled-out**. The whole of `dynamic.zig` (the `Union`/`DynamicCompletion`/
`detect`/`prefer`/`backend`-global machinery) never instantiates on macOS.

**Consequences (all decisive):**
1. `rustty-xev` exposes **flat** handle types (`xev::Loop`, `xev::Async`,
   `xev::Completion`, …) = the kqueue static Api(). This is exactly what the
   current placeholder already committed to (flat `Loop`/`Async`/…), so **no
   downstream `use` path changes.**
2. **`dynamic.zig` is DEFERRED, not weakened** — we faithfully reproduce what the
   Zig compiler emits for a single-candidate target (the backend's Api()
   directly). R5-safe: nothing is dropped that the macOS build ever compiles.
3. The `xev.dynamic` / `xev.detect` / `xev.prefer` / `xev.backend` seams in
   `global.zig` stay dead exactly as in the reference (they already are TODO
   comments in `rustty-global`).
4. The `rustty_global::xev` seam stays as the "package" anchor; `rustty-xev`
   remains the concrete home of the types.

---

## 1. SCOPE — file-by-file IN / DEFER (with loc + test counts)

`loc` = libxev source lines. `tests` = **macOS/kqueue-live** `test{}` that run
for this target (the R1 contract number — see §7 for how comptime-generic test
files collapse to one instantiation).

### IN SCOPE (the live-shell core)

| file | loc | tests (macOS-live) | why |
|---|---|---|---|
| `main.zig` | 127 | (aggregator) | package root: re-exports the kqueue Api() decls. Port as the crate's `lib.rs` re-export surface, NOT the Backend/Dynamic switch. |
| `api.zig` | 88 | 2 (`refAllDecls`, "completion is zero-able") | the `Xev(be,T)` decl-forwarding struct. Collapses to a fixed re-export module for kqueue. |
| `backend.zig` | 52 | 0 | `Backend` enum + `default()`/`candidates()`. Port the enum; `default()==kqueue` is the only live arm. |
| `loop.zig` | 104 | 0 | `Options`, `RunMode`, `Callback`, `NoopCallback`, `CallbackAction`, `CompletionState`. **The callback ABI — §2.** |
| `watcher/common.zig` | 7 | 0 | `userdataValue`. Trivial. |
| `queue.zig` | 101 | 1 | intrusive singly-linked FIFO of `Completion`. **§2.** |
| `queue_mpsc.zig` | 116 | 1 | Vyukov MPSC (thread-pool completions). Needed for the `Completion` type refs even with pool deferred. |
| `heap.zig` | 379 | 6 | intrusive pairing heap = the **timer heap**. §4. |
| `darwin.zig` | 376 | 0 | mach-port / mach_msg bindings + `KernE`/`MachMsgE`. Needed by `Async`(mach) + loop `Wakeup`. §3. |
| `backend/kqueue.zig` | 2777 | **12** | the whole loop: `Loop`, `Completion`, `Operation`, `Result`, `Wakeup`, `Timer`, `kevent_syscall`. **§2/§3.** All 12 tests are an R1 package deal. |
| `watcher/async.zig` | 828 | 3 | `AsyncMachPort` (Darwin) — the wakeup handle. Other variants cfg-gated out (§7). |
| `watcher/timer.zig` | 641 | 5 | `TimerLoop` (run/reset/cancel). |
| `watcher/process.zig` | 572 | 3 | `ProcessKqueue` (EVFILT_PROC + `waitpid` reap). |
| `watcher/stream.zig` | 1443 | 4 | `Shared` (`WriteQueue`/`WriteRequest`/`PollError`/`PollEvent`) + `GenericStream`(read/write/close/poll) + the `Writeable.queueWrite` engine. §4. |

**In-scope R1 test total ≈ 35** (see §7 tally) + rustty-only KATs.

### DEFERRED (documented in DEFERRALS.md; not needed to boot a shell on macOS)

| file | loc | tests | reason to defer |
|---|---|---|---|
| `ThreadPool.zig` | ~1000 | 0 | ghostty calls `Loop.init(.{})` with **no `thread_pool`** (`termio/Thread.zig:95`, `renderer/Thread.zig:134`). The pool is reachable only via `flags.threadpool=true` (xev.File / explicit) which ghostty never sets. **Port the referenced TYPE surface only** (`Task`, `Batch.from`, `schedule`, `init/deinit/shutdown` sigs) so kqueue.zig compiles; defer the work-stealing body. §6/§8. |
| `dynamic.zig` | 525 | 5 | comptime-elided on single-candidate macOS (§0). N/A, not weakened. |
| `c_api.zig` | ~380 | 1 | libxev's C ABI. rustty consumers use the Rust API; not built. |
| `watcher/file.zig` | 848 | 8 | `xev.File` — **zero** ghostty uses. (Read side of the pty is a raw `ReadThread`, not xev.) |
| `watcher/tcp.zig` | 935 | 5 | `xev.TCP` unused by ghostty. |
| `watcher/udp.zig` | 1003 | 6 | `xev.UDP` unused. |
| `backend/epoll.zig` | 2061 | 12 | Linux. Follow-up platform. |
| `backend/io_uring.zig` | 1787 | 13 | Linux. |
| `backend/iocp.zig` | 2376 | 14 | Windows. |
| `backend/wasi_poll.zig` | 1648 | 9 | WASI. |
| `linux/timerfd.zig`, `windows.zig`, `debug.zig`, `bench/*` | — | — | platform/aux; port only if a referenced decl forces it. |

**Deferring these ~13k loc / ~78 tests is the single biggest scope lever.** Each
is a whole distinct backend/watcher ghostty's macOS build never compiles.

---

## 2. The Completion / callback model (the make-or-break faithfulness question)

libxev is **intrusive**: the caller owns every `Completion`; the `Loop` holds raw
pointers *into* caller memory; callbacks are comptime fn-pointers with untyped
userdata + a `CallbackAction` return. This is precisely the shape the
`rust-mut-args-strongly-protected` memory note governs: **do NOT model loop⇄
completion links as `&mut`/lifetimes** — a `&mut Completion` re-read across a
loop tick (during which the same completion is pushed onto `submissions`, handed
to a callback that may `rearm` it, etc.) is Stacked-Borrows UB. Zig pointers are
unprotected; the faithful Rust model is **raw pointers**.

### 2.1 Core types — Rust signatures

```rust
// loop.zig
#[repr(C)] pub enum RunMode { NoWait = 0, Once = 1, UntilDone = 2 }
#[repr(C)] pub enum CallbackAction { Disarm = 0, Rearm = 1 }
#[repr(C)] pub enum CompletionState { Dead = 0, Active = 1 }

// The callback ABI. `*const fn(...)` in Zig → an `extern fn` pointer in Rust.
pub type Callback = fn(
    userdata: *mut c_void,      // ?*anyopaque  (null when Userdata==void)
    loop_: *mut Loop,           // *Loop        (raw — NOT &mut Loop)
    c: *mut Completion,         // *Completion  (raw — the intrusive self-ptr)
    result: Result,
) -> CallbackAction;

pub fn noop_callback(_:*mut c_void,_:*mut Loop,_:*mut Completion,_:Result)
    -> CallbackAction { CallbackAction::Disarm }
```

`Completion` (kqueue.zig:971; `@sizeOf==256`, comptime-asserted at :1866 — keep
the assert as a Rust `const _: () = assert!(size_of::<Completion>()==256)` **iff
we choose to honor the layout**; recommend keeping it as a fidelity tripwire):

```rust
pub struct Completion {
    pub op: Operation,               // union(OperationType), default .noop
    pub userdata: *mut c_void,       // ?*anyopaque
    pub callback: Callback,          // default noop_callback

    // ---- internal ----
    next: *mut Completion,           // ?*Completion — intrusive queue link (raw!)
    result: Option<Result>,          // ?Result
    flags: CompletionFlags,          // packed { state: State(u3), threadpool: bool }
    task: ThreadPoolTask,            // undefined until scheduled (pool deferred)
    task_loop: *mut Loop,            // undefined until scheduled
}
// [ADV-FIX F3] Completion has fn-ptr + raw-ptr fields → hand-write impl Default
// (callback=noop_callback, op=Operation::Noop, userdata/next/task_loop=null,
// result=None, state=Dead) — NOT #[derive(Default)]. Load-bearing: termio's
// xev_seam re-exports these and default-constructs `Completion`/`WriteQueue`/
// `WriteRequest`; `Async: Copy`; `NotifyError: Debug+Clone+Copy+PartialEq+Eq`
// (core surface::Error embeds it). Missing any of these breaks termio/core compile.
```

**[ADV-FIX F2] `Send`/`Sync` discipline (compile-time REQUIRED, not optional).** The intrusive
raw-pointer types are auto-`!Send`; ghostty moves a `Loop` onto the termio thread and shares
`Async` across threads. Add `unsafe impl Send` (thread-affinity: a `Loop`/its `Completion`s are
touched by exactly one thread at a time — the owning event-loop thread — after the move; `Async`'s
mach `port: u32` is the cross-thread `notify()` handle and is auto-`Send`). Document the
single-thread-affinity invariant at each `unsafe impl Send`. This is exercised by KAT 3/4 (spawn a
thread + pump the loop) and all of Milestone B — design it in, do not discover it during wiring.

- `next: *mut Completion` is the intrusive link. **`queue::Intrusive<T>` ports as
  a raw-pointer FIFO** (head/tail `*mut T`, `push`/`pop`/`empty`), byte-faithful
  to `queue.zig`. The `assert(v.next == null)` on push ports as `debug_assert`.
- `Operation`/`Result` are tag-parallel `union(OperationType)` (kqueue.zig:1527/
  1616). Port as Rust `enum`s with the **same 17 variants** (`noop, accept,
  connect, read, write, pread, pwrite, send, recv, sendto, recvfrom, close,
  shutdown, timer, cancel, machport, proc`). The buffer members are
  `ReadBuffer`/`WriteBuffer` unions (`slice: &mut [u8]` / `array: [u8;32]`) — R3:
  byte-slices stay slices, never `String`.

### 2.2 The comptime-generic callback → Rust

Watchers wrap a **typed** user callback in an **untyped** trampoline (see
`async.zig:335`, `timer.zig:62`). Zig does this with a comptime `struct{fn
callback}`. Rust equivalent: a **generic function that returns a monomorphized
`extern`/plain fn item**, or a generic wrapper `fn`. The typed→untyped bridge is
`common.userdataValue::<Userdata>(ud)` = `if TypeId==void {null} else
ud as *mut Userdata`. Faithful Rust: a generic `wait::<UD>(…, cb: fn(*mut UD, …))`
that installs a monomorphized trampoline coercing `*mut c_void`↔`*mut UD`. No
`dyn`, no boxing — matches Zig's zero-alloc comptime dispatch.

### 2.3 Loop.run / tick — the heart (kqueue.zig:299-565)

```rust
pub fn run(&mut self, mode: RunMode) -> Result<(), Error> {
    match mode {
        RunMode::NoWait   => self.tick(0),
        RunMode::Once     => self.tick(1),
        RunMode::UntilDone=> { while !self.done() { self.tick(1)?; } Ok(()) }
    }
}
pub fn tick(&mut self, wait: u32) -> Result<(), Error>;
```

`tick` faithfully reproduces the exact ordering that makes libxev correct:
1. `process_cancellations()` — drain `cancellations`, flip target states.
2. `submit()` — batch `submissions` into a `[Kevent;256]` changelist, `kevent`
   with **zero eventlist** (the "memory corruption" note at :240 is preserved
   verbatim as a comment — R3).
3. loop: `update_now`; the exact "hideous boolean" break condition (:372); fire
   expired timers off the heap (invoking callbacks, honoring `rearm`); migrate
   thread-pool MPSC completions (dead branch while pool deferred, still ported);
   drain `completions` invoking callbacks and staging `EV.DELETE` disarms;
   compute the timer-derived `timeout`; `kevent_syscall`; dispatch returned
   events (`perform` → callback → disarm/rearm).

**Self-reference discipline:** `tick`/`submit`/`add`/`start` take `&mut self`
(the Loop) — [ADV-FIX F1] the real public surface is `{init, deinit, stop,
stopped, add, submit, run, tick, now, update_now, timer, timer_reset}` (there is
NO `stop_completion`; cancellation is the `.cancel` op + `process_cancellations`;
`loop.stop`/`loop.stopped` are called directly by ghostty) — but **completions
are touched through `*mut Completion`**
(popped from a raw-ptr queue), never through borrows that alias the loop. The
callback receives `*mut Loop` + `*mut Completion` (raw), so a callback may
legally re-add the same completion (`rearm`) or add siblings — exactly libxev's
contract. Model `active`, `submissions`, `cancellations`, `completions`,
`timers`, `thread_pool_completions`, `cached_now`, `flags` as plain fields.

---

## 3. The kqueue backend — syscalls (rustty-os vs new bindings)

libxev calls `std.posix` / `std.c` directly (it depends on **no** ghostty os
layer). Faithful mapping: `rustty-xev` binds **`libc` directly** (as `rustty-os`
itself does), keeping the vendored library self-contained.

| libxev call | Rust binding | provided by |
|---|---|---|
| `posix.kqueue()` | `libc::kqueue()` | libc ✓ |
| `posix.kevent` / `posix.system.kevent64` | `libc::kevent` / `libc::kevent64` | kevent ✓; [ADV-FIX F4] **`kevent64` + `kevent64_s` ARE in the libc crate** (libc-0.2.186 apple/mod.rs) — use them, no new extern. macOS uses `kevent64_s` (kqueue.zig:1802). |
| `EVFILT.{READ,WRITE,PROC,MACHPORT,USER}`, `EV.{ADD,ENABLE,DELETE,ERROR}`, `NOTE.{EXIT,EXITSTATUS}` | `libc::{EVFILT_*, EV_*, NOTE_*}` | libc ✓ |
| `posix.{read,write,pread,pwrite,accept,connect,send,recv,sendto,recvfrom,shutdown,close,waitpid,clock_gettime}` | `libc::*` | libc ✓ (the `Operation.perform` arms) |
| mach: `mach_port_allocate`, `mach_port_insert_right`, `mach_port_set_attributes`, `mach_port_destroy`, `mach_task_self`, `mach_msg` | **`darwin.rs` `extern "C"`** (some in libc, `mach_msg`/`mach_port_set_attributes` declared) + `mach_msg_header_t`, `KernE`, `MachMsgE` | **NEW — port `darwin.zig` verbatim.** |

`kevent_syscall` (kqueue.zig:1810) ports 1:1: on macOS take the `kevent64`
branch (the `Kevent==posix.Kevent` fast path is the FreeBSD arm, cfg-gated).
`kevent_init` (:1852) fills `.ext={0,0}`.

**Async wakeup mechanism:** Darwin uses a **mach port**, not a self-pipe/EVFILT_USER.
`Wakeup` (kqueue.zig:1434) wraps an `Async`(mach) and registers an
`EVFILT.MACHPORT` filter with `udata=0` (internal). `notify()` sends an empty
mach message (`darwin.mach_msg` with `MACH_SEND_MSG|MACH_SEND_TIMEOUT`). Timers
are **not** EVFILT_TIMER — libxev keeps its own pairing heap and derives the
`kevent` timeout (kqueue.zig:464-483). EVFILT_PROC drives `Process`.

`rustty-os` today provides only `pipe()` + `fd_t`; it does **not** wrap
kqueue/kevent/mach — so those are the new bindings, and they live in `rustty-xev`
(faithful: libxev owns them, not ghostty's os).

---

## 4. Watchers — Rust struct + method signatures

All take `*mut Loop` / `*mut Completion` (raw). Generic over `Userdata` with a
monomorphized trampoline (§2.2).

### Async — `AsyncMachPort` (async.zig:221)
```rust
pub struct Async { port: mach_port_name_t }
impl Async {
    pub fn init() -> Result<Async, Error>;      // allocate RECV port + SEND right + qlimit=1
    pub fn deinit(&mut self);                    // mach_port_destroy
    pub fn wait<UD>(self, loop_: *mut Loop, c: *mut Completion,
        ud: *mut UD, cb: fn(*mut UD,*mut Loop,*mut Completion, Result<(),MachPortError>)->CallbackAction);
    pub fn notify(&self) -> Result<(), Error>;   // empty mach_msg (COPY_SEND)
}
```
`wait` sets `op = .machport{ port, buffer:.array }`; its trampoline `drain()`s the
port (coalesces N notifies → 1 wakeup) then calls the user cb.

### Timer — `TimerLoop` (timer.zig:15)
```rust
pub struct Timer;                                 // zero-sized
impl Timer {
    pub fn init() -> Result<Timer, Error>; pub fn deinit(&self);
    pub fn run<UD>(self, loop_:*mut Loop, c:*mut Completion, next_ms:u64,
        ud:*mut UD, cb: fn(*mut UD,*mut Loop,*mut Completion, Result<(),RunError>)->CallbackAction);
    pub fn reset<UD>(self, loop_:*mut Loop, c:*mut Completion, c_cancel:*mut Completion,
        next_ms:u64, ud:*mut UD, cb: …);
    pub fn cancel<UD>(self, loop_:*mut Loop, c_timer:*mut Completion, c_cancel:*mut Completion,
        ud:*mut UD, cb: fn(…, Result<(),CancelError>)->CallbackAction);
}
```
Backed by `Loop::timer` / `timer_reset` (kqueue.zig:601/622) + the `Timer` heap
node (`next: timespec`, `reset: Option<timespec>`, `heap: IntrusiveField`,
`c: *mut Completion`). `less` compares `ns()`. The `reset` path uses a paired
cancel completion — port the `.active`/`.adding`/`.dead` state machine verbatim.

### Process — `ProcessKqueue` (process.zig:145)
```rust
pub struct Process { pid: pid_t }
impl Process {
    pub fn init(pid: pid_t) -> Result<Process, Error>; pub fn deinit(&mut self);
    pub fn wait<UD>(self, loop_:*mut Loop, c:*mut Completion,
        ud:*mut UD, cb: fn(*mut UD,*mut Loop,*mut Completion, Result<u32,WaitError>)->CallbackAction);
}
```
`op = .proc{ pid, flags: NOTE_EXIT|NOTE_EXITSTATUS }`; trampoline reaps with
`waitpid(pid,0)` then reports exit status. Exit status decoded in
`Completion.perform.proc` via `W.IFEXITED/EXITSTATUS`.

### Stream / WriteQueue / WriteRequest (stream.zig)
```rust
pub struct Stream { fd: fd_t }                    // GenericStream, non-dynamic branch
impl Stream {
    pub fn init_fd(fd: fd_t) -> Stream; pub fn deinit(&self);
    pub fn queue_write<UD>(self, loop_:*mut Loop, q:*mut WriteQueue, req:*mut WriteRequest,
        buf: WriteBuffer, ud:*mut UD,
        cb: fn(*mut UD,*mut Loop,*mut Completion, Stream, WriteBuffer, Result<usize,WriteError>)->CallbackAction);
    // read/write/close/poll also ported (one instantiation), exercised by the pty tests
}
pub struct WriteRequest { completion: Completion, userdata:*mut c_void,
                          full_write_buffer: WriteBuffer, next:*mut WriteRequest }
pub type WriteQueue = queue::Intrusive<WriteRequest>;   // intrusive FIFO
```
`queue_write` (stream.zig:780) is the **ordered-write engine** ghostty's pty
writes ride: it chains completions through the `WriteQueue`, and its trampoline
handles **partial writes** by re-`writeInit`-ing the remainder (byte-exact:
`writeBufferLength`/`writeBufferRemainder`, the `@memcpy` of the array tail).
`WriteRequest::from(c)` recovers the request via `@fieldParentPtr("completion")`
→ Rust `container_of` (offset-of), the **same faithful pattern** as the
`vtc-effects-trampoline-seam` note. ghostty pools these:
`SegmentedPool<WriteRequest>` + `WriteQueue{}` default (Exec.zig:513/519) — both
already exist as rustty-datastruct + the placeholder `WriteRequest:Default`.

---

## 5. Crate placement + deps + acyclicity

`rustty-xev` stays a **low leaf**, below `termio`/`core`/`renderer` (which
already `workspace`-depend on it — confirmed in their Cargo.toml).

**New dep set:**
```toml
[dependencies]
rustty-global = { workspace = true }   # existing seam anchor (unchanged)
libc = "0.2"                           # NEW: kqueue/kevent/mach/posix (as rustty-os binds)
[dev-dependencies]
# pty/socket test helpers (openpty, socketpair) via libc
```
- **No `rustty-os` dep** (keep the vendored library self-contained, mirroring
  that libxev depends on `std` only, never ghostty's os layer). If a helper is
  ever genuinely shared, `rustty-os` is a peer leaf — safe either way (os does
  not depend on xev), but default to libc-direct for fidelity.
- **`queue.zig` / `queue_mpsc.zig` / `heap.zig` are ported INTO `rustty-xev`**
  (as `queue.rs` / `queue_mpsc.rs` / `heap.rs`), **not** reused from
  `rustty-datastruct`. They are libxev *source files* (R3 byte-faithful, R1 test
  counts) and differ from datastruct's `DoublyLinkedList` (singly-linked FIFO /
  Vyukov MPSC / pairing heap). Reuse would compromise fidelity and add coupling.
- **Acyclic:** `rustty-xev → {rustty-global, libc}` only. No edge to
  termio/core/renderer/os. `check_acyclic.sh` stays green.

---

## 6. The D6 un-defer plan — two milestones (recommended)

**Milestone A (THIS one): land a real, self-contained `rustty-xev` + its own
tests.** Fill the placeholder types in place (downstream `use` paths unchanged),
port the §1 in-scope files, port the ~35 R1 tests, add rustty-only kqueue KATs
(§7). **Do NOT touch termio/core/renderer** yet. Gate-green in isolation.

**Milestone B (NEXT): un-defer TERMIO + a live-shell KAT. [ADV-FIX F8]** With
`rustty-xev` real, un-defer ONLY the termio D6 seams: `Thread.run` loop +
callbacks, `Termio.init/threadEnter/processOutput`, `Exec` `Subprocess.start` +
`ReadThread` + `queueWrite`, **plus the core `Async::notify()` leaves that termio's
threads poke** (not a core "app loop" — `App.zig` has NO xev loop). Cap with a
**live-shell KAT** (spawn a shell in a pty, pump the loop, assert output
round-trips) — this needs neither the render thread nor a core app loop.
**NOT in Milestone B:** `renderer.Thread` is GPU-*and*-xev-blocked
(`renderer_seam.rs:381` = "Phase-3 R-d + D6 xev") — landing rustty-xev does not make
it compile or run; it stays in the **Phase-3 GPU tier** (its drawFrame needs the
Metal/OpenGL renderer). Listing it here would false-green.

**Why split:** (1) `rustty-xev` is the highest-UB-surface code in the whole port
(raw intrusive self-referential pointers + real kqueue/mach syscalls); it
deserves to be proven **in isolation** before anything depends on its behavior.
(2) The termio/core/renderer un-defer is a large, *independent* body of wiring
(the deferred verbatim bodies already exist) — coupling it to the xev port makes
a mega-PR that can't bisect. (3) A green isolated `rustty-xev` de-risks Milestone
B: if the live shell misbehaves, the loop is already unit-proven, so the bug is
in the wiring, not the loop. This matches the landed cadence (each tier landed
gate-green before the next depended on it).

---

## 7. Test accounting (R1) + rustty KATs

**R1 = every `test{}` in an in-scope file gets a Rust `#[test]`.** For the
comptime-generic watcher files, only the **macOS/kqueue instantiation** compiles;
the cfg-gated-out variants (eventfd/iocp/wasi/dynamic `Async`, pidfd/iocp
`Process`, dynamic `Stream`) are ported as `#[cfg]`-gated code **without** live
tests (precedent: rustty-font-shaper's cfg-matrixed face backends). The
`refAllDecls`/`test{}` aggregator blocks do not map 1:1 — the concrete named
tests do.

| module | R1 tests | names |
|---|---|---|
| queue | 1 | Intrusive |
| queue_mpsc | 1 | Intrusive |
| heap | 6 | heap; remove root; remove w/ children; equal values; million values; dangling next |
| kqueue | 12 | loop time; stop; timer; timer reset; …before tick; …after trigger; timer cancellation; canceling a completed op; socket accept/connect/send/recv/close; **file IO on thread pool**; mach port; socket accept/cancel decrements active |
| async | 3 | async; notify first; batches multiple |
| timer | 5 | timer; reset; reset before tick; reset after trigger; cancel |
| process | 3 | wait; wait non-zero exit; wait already-exited |
| stream | 4 | Stream decls; pty child→parent; pty parent→child; pty queued writes |
| **total** | **35** | |

**The one honest snag — `kqueue: file IO on thread pool`.** This test builds a
real `ThreadPool` and asserts thread-pool file IO round-trips. With ThreadPool
deferred (§1), port it as a **documented skip-stub `#[test]`** (established
precedent: the gate's own docstring — font-shaper's fontconfig/web_canvas skips
"ported as SkipZigTest-style skip-stub #[test]s so the count still holds"). The
count stays 12; DEFERRALS.md records it. Milestone-A honest, R5-safe (it defers
genuinely-unused functionality, it does not weaken a live path). *(Alternative:
fully port `ThreadPool.zig` — §8 risk.)*

The socket tests (9, 12) and "canceling a completed op" (8) drive **raw
completions** with socket ops (via `socketpair`/`std.net`), not the TCP watcher —
so they need only the accept/connect/send/recv/close arms **already inside**
kqueue.zig. No `tcp.zig` needed.

**check_test_counts.py:** add `rustty-xev` mapping entries (GH libxev src →
`crates/rustty-xev/src`). Note the gate maps ghostty `src/` today; libxev lives
in the zig cache — extend the gate with a libxev base path, or record the
in-scope counts in a `rustty-xev` manifest the gate reads.

**rustty-only live KATs (recommend, `#[cfg(target_os="macos")]`):** these prove
the loop actually runs on kqueue (Miri can't — real syscalls):
1. **Async wakeup round-trip** — `Async.init`; `wait` (rearm); `notify` from
   another thread; `run(.once)`; assert woke exactly once.
2. **Timer fires** — `Timer.run(1ms)`; `run(.until_done)`; assert callback fired,
   `now()` advanced.
3. **Process spawn+exit** — `posix_spawn` `/bin/true`; `Process.wait`;
   `run(.until_done)`; assert exit status 0.
4. **Pty write round-trip** — `openpty`; `Stream.init_fd(write)`; `queue_write`
   a payload; read it back on the other end; assert bytes + WriteQueue drains.

---

## 8. Chunked execution plan + risks

**Chunks (each gate-green before the next):**
1. **C1 — data structures:** `queue.rs`, `queue_mpsc.rs`, `heap.rs` (+8 tests).
   Pure, Miri-clean, no syscalls. Establishes the raw-ptr intrusive discipline.
2. **C2 — syscall/darwin layer:** `darwin.rs` (mach bindings, `KernE`/`MachMsgE`),
   `kevent64`/`kevent64_s` extern, EVFILT/EV/NOTE consts. No tests; compile gate.
3. **C3 — loop core (no watchers):** `loop.rs` (Callback/CallbackAction/…),
   `Completion`/`Operation`/`Result`, `Loop.{init,deinit,stop,stopped,add,submit,
   run,tick,now,update_now,timer,timer_reset}` [ADV-FIX F1: real surface, no
   stop_completion], the `Send` impls [ADV-FIX F2] + hand-written `Default`
   [ADV-FIX F3], `Wakeup`, `kevent_syscall`. Land kqueue
   tests that use only timers/cancel/raw ops (loop time, stop, timers×5, cancel
   ×2, sockets×2 with raw completions) + the ThreadPool **type surface** (deferred
   body). KAT 2.
4. **C4 — Async (mach):** `async.rs` `AsyncMachPort` + the loop `machport` op +
   `Wakeup` wiring. kqueue "mach port" test + async×3 + KAT 1.
5. **C5 — Process:** `process.rs` `ProcessKqueue` + EVFILT_PROC perform + waitpid.
   process×3 + KAT 3.
6. **C6 — Stream/WriteQueue:** `stream.rs` `Shared` + `GenericStream` +
   `Writeable.queueWrite` (+read/close/poll). stream×4 + KAT 4.
7. **C7 — package surface + gates:** `lib.rs` re-exports = kqueue Api(); wire
   `check_test_counts` mapping; DEFERRALS.md (dynamic/ThreadPool-body/file/tcp/
   udp/other-backends + the one skip-stub). Fill placeholder types in place.

**Risks + mitigations:**
- **Raw-ptr / intrusive-completion UB** (the big one). The loop stores `*mut
  Completion` into caller structs and re-enters them across ticks — classic
  Stacked-Borrows aliasing hazard. *Mitigation:* raw pointers end-to-end (never
  `&mut` across the loop, per `rust-mut-args-strongly-protected`); **Miri on C1
  + the pure heap/queue tests** (they don't syscall); document a MIRI_EXCEPTIONS
  entry for C3–C6 (kqueue/mach syscalls don't run under Miri) and lean on the
  live KATs there. Keep the `@sizeOf(Completion)==256` assert as a layout
  tripwire.
- **comptime-generic → fn-ptr translation.** Zig's `struct{fn callback}`
  trampolines + `Userdata==void` erasure. *Mitigation:* monomorphized generic
  wrapper `fn`s returning plain fn items; `userdataValue` = null-vs-cast; no
  `dyn`/Box (preserve zero-alloc dispatch).
- **mach-port async correctness** (coalescing, qlimit=1, `drain` on wait,
  `SEND_TIMED_OUT`/`SEND_NO_BUFFER` treated as success). *Mitigation:* port
  `darwin.zig` + `AsyncMachPort` byte-faithfully; KAT 1 asserts the "wakes once"
  coalescing contract (matches `async batches multiple notifications`).
- **`kevent64_s` vs `kevent`.** libc lacks the 64 variant. *Mitigation:* declare
  `kevent64_s` + `kevent64` extern (darwin.zig precedent); macOS always takes the
  64 branch; keep the FreeBSD `posix.Kevent` fast-path cfg-gated for later.
- **ThreadPool deferral.** If a future consumer needs the pool (xev.File), it's a
  clean follow-up: `ThreadPool.zig` is self-contained (0 tests) — port then. The
  deferred body is isolated behind the already-ported type surface.
- **cfg-gating discipline.** Everything is `#[cfg(target_os="macos")]` (or a
  `kqueue` feature). Linux/Windows arms are ported later as whole backends, not
  interleaved — avoids a cfg maze in the loop core.

---

### TL;DR decisions
1. macOS `xev.Dynamic` = static kqueue Api() — **flat types, dynamic.zig deferred,
   not weakened** (comptime-elided at source).
2. In scope: core + kqueue backend + darwin + Async(mach)/Timer/Process/Stream +
   queue/mpsc/heap. **~35 R1 tests.**
3. Defer: all other backends, file/tcp/udp, c_api, and the **ThreadPool body**
   (type surface only; ghostty passes no pool) — one documented skip-stub test.
4. Raw pointers end-to-end (not `&mut`/lifetimes); comptime callbacks →
   monomorphized fn-ptr trampolines; intrusive queues → raw-ptr FIFO/MPSC/heap.
5. `rustty-xev → {rustty-global, libc}`, acyclic, low leaf.
6. **Two milestones:** land isolated `rustty-xev` first; un-defer termio/core/
   renderer + live-shell KAT next.
