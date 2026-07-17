# L1 — rustty-xev Linux event loop (epoll + io_uring + Dynamic) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port libxev's two Linux event-loop backends (`epoll`, `io_uring`) plus the live `Dynamic` runtime selector into `crates/rustty-xev`, so `termio`/`core` run a live PTY shell on Linux exactly as they already do on macOS/kqueue.

**Architecture:** Mirror the proven kqueue lane (`XEV_BLUEPRINT.md`, chunks C1–C7): raw-pointer intrusive `Completion`/`Loop`, monomorphized fn-ptr callback trampolines, shared `queue`/`queue_mpsc`/`heap` reused as-is. Two NEW backend modules (`epoll.rs`, `io_uring.rs`) sit beside `kqueue.rs`; the watcher modules (`async`/`timer`/`process`/`stream`) gain per-backend arms (currently kqueue-only). The NEW piece with no 1:1 reference is `dynamic.rs`: because Rust has no comptime `@Type`/`@unionInit` reflection, the `Dynamic` layer libxev generates is **hand-authored** as concrete 2-variant (`IoUring`/`Epoll`) enums that dispatch on a global backend selector. `lib.rs` re-exports the flat kqueue `Api()` on macOS (unchanged) and the `Dynamic` types on Linux — both under the same public names, so `termio`'s `pub use rustty_xev::*` seam is untouched.

**Tech Stack:** Rust, `libc` (epoll/eventfd/timerfd/pidfd/io_uring raw syscalls — no new crate deps), the existing `rustty-xev` intrusive primitives. Reference oracle = pinned libxev `34fa50878aec6e5fa8f532867001ab3c36fae23e` (fetched to `scratchpad/libxev/`).

## Global Constraints

- **R1 fidelity:** every `test{}` in an in-scope libxev file gets a Rust `#[test]`; port bodies verbatim in behavior. Contract counts: **epoll 12, io_uring 13, dynamic 5** (+ rustty-only live KATs). Names are enumerated per task.
- **R3 self-contained:** `rustty-xev → {rustty-global, libc}` ONLY. Do NOT add the `io-uring` / `nix` / `mio` crates — hand-roll io_uring over raw `libc` syscalls (`io_uring_setup`/`io_uring_enter` + `mmap` rings), mirroring how Zig's `std.os.linux.IoUring` is itself a thin syscall wrapper. Keep the crate acyclic (`check_acyclic.sh` green); no edge to termio/core/os.
- **R5 gates invariant:** macOS/kqueue path must remain byte-behavior-identical — every Linux addition is `#[cfg(target_os = "linux")]`-gated, mirroring the existing `#[cfg(target_os = "macos")]` gating at `crates/rustty-xev/src/lib.rs:60–205`.
- **Raw-ptr discipline:** never model loop⇄completion links as `&mut`/lifetimes (Stacked-Borrows UB); raw `*mut Completion`/`*mut Loop` end-to-end, per `rust-mut-args-strongly-protected`. io_uring SQ/CQ ring memory (kernel-shared `mmap`) is an isolated-Miri/ASan candidate.
- **Trait surface (load-bearing, [ADV-FIX F3]):** the Linux `Async`/`Completion`/`WriteQueue`/`WriteRequest`/`NotifyError` re-exports MUST satisfy the same bounds the kqueue types do — `Async: Copy`, `Completion/WriteQueue/WriteRequest: Default` (hand-written, not derived), `NotifyError: Debug+Clone+Copy+PartialEq+Eq`. termio (`xev_seam.rs: pub use rustty_xev::*`) and core default-construct and embed these; a missing bound breaks their compile.
- **Reference pin:** all `libxev …:NNN` citations resolve against `scratchpad/libxev/src/` (pin `34fa5087`). All `rustty-xev …:NNN` citations resolve against the current crate.

## File structure

| File | Responsibility |
|---|---|
| `crates/rustty-xev/src/epoll.rs` (**create**) | epoll `Loop`/`Completion`/`Operation`/`Result`/`submit`/`tick`; port of libxev `backend/epoll.zig` (2061 loc). Mirrors `kqueue.rs` structure. |
| `crates/rustty-xev/src/io_uring.rs` (**create**) | io_uring `Loop`/`Completion`/`Operation`/`Result` + SQ/CQ ring engine; port of `backend/io_uring.zig` (1787 loc) + a small `IoUring` ring binding (libc syscalls). |
| `crates/rustty-xev/src/linux/timerfd.rs` (**create**) | `timerfd`-backed timer primitive; port of libxev `linux/timerfd.zig` (89 loc). |
| `crates/rustty-xev/src/dynamic.rs` (**create**) | hand-authored `Dynamic` union layer (`Loop`/`Completion`/`Async`/`Timer`/`Process`/`Stream`/`WriteQueue`/`WriteRequest`/error sets) over `{IoUring, Epoll}` + the global `backend` selector + `detect`/`prefer`. Port of `dynamic.zig` (524 loc) WITHOUT comptime metaprogramming (concrete 2-variant). |
| `crates/rustty-xev/src/async.rs` (**modify**) | add the `io_uring`/`epoll` eventfd arms of `Async` (currently kqueue mach-port only). |
| `crates/rustty-xev/src/timer.rs` (**modify**) | add the epoll(timerfd)/io_uring(IORING_OP_TIMEOUT) arms. |
| `crates/rustty-xev/src/process.rs` (**modify**) | add the epoll(pidfd/waitpid)/io_uring arms of `Process`. |
| `crates/rustty-xev/src/stream.rs` (**modify**) | add the epoll/io_uring read/write/poll/`queueWrite` arms. |
| `crates/rustty-xev/src/backend.rs` (**modify**) | `candidates()`/`default()`/`Api` Linux arms (`[io_uring, epoll]`, default `io_uring`). |
| `crates/rustty-xev/src/lib.rs` (**modify**) | per-target re-export: Linux → `dynamic::*`; macOS → kqueue flat (unchanged). |
| `crates/rustty-termio/tests/live_shell.rs` (**modify**) | widen the KAT gate from `#[cfg(target_os="macos")]` to `#[cfg(unix)]`. |
| `.port/phase-2/DEFERRALS.md` + `check_test_counts.py` (**modify**) | record io_uring/epoll/dynamic counts; note deferred watcher variants (file/tcp/udp still off). |

---

### Task 1: Linux backend enum + gated module skeleton

**Files:**
- Modify: `crates/rustty-xev/src/backend.rs` (add Linux `candidates`/`default`/`Api` arms)
- Modify: `crates/rustty-xev/src/lib.rs:35–205` (add `#[cfg(target_os="linux")] mod epoll; mod io_uring; mod dynamic; mod linux;` skeletons)
- Create: `crates/rustty-xev/src/epoll.rs`, `io_uring.rs`, `dynamic.rs`, `linux/timerfd.rs` (empty stub bodies that compile)

**Interfaces:**
- Consumes: existing `Backend` enum (`backend.rs`).
- Produces: `Backend::{IoUring, Epoll}` variants; `Backend::default() == IoUring` on Linux; `Backend::candidates() == &[IoUring, Epoll]` on Linux (ref `backend.zig:19–41`). Empty `epoll`/`io_uring`/`dynamic` modules gated `#[cfg(target_os="linux")]`.

- [ ] **Step 1: Write the failing test** — in `backend.rs` add
  ```rust
  #[cfg(target_os = "linux")]
  #[test]
  fn linux_candidates_and_default() {
      assert_eq!(Backend::default(), Backend::IoUring);
      assert_eq!(Backend::candidates(), &[Backend::IoUring, Backend::Epoll]);
  }
  ```
- [ ] **Step 2: Run to verify it fails** — `cargo test -p rustty-xev --target aarch64-unknown-linux-gnu linux_candidates_and_default` → FAIL (variants/arms missing).
- [ ] **Step 3: Implement** — add `IoUring`/`Epoll` to the `Backend` enum; port the `.linux => &.{.io_uring, .epoll}` candidate arm and `.linux => .io_uring` default arm from `backend.zig:19–41`; add empty `#[cfg(target_os="linux")] pub(crate) mod epoll;` etc. in `lib.rs` with stub files containing only `//! stub` + a `#![allow(dead_code)]`.
- [ ] **Step 4: Run to verify it passes** — same command → PASS; `cargo build -p rustty-xev --target aarch64-unknown-linux-gnu` compiles.
- [ ] **Step 5: Commit** — `git commit -m "rustty-xev: linux backend enum + gated module skeleton (L1 LT1)"`

---

### Task 2: `linux/timerfd.rs` + epoll `Async` (eventfd) primitive

**Files:**
- Create: `crates/rustty-xev/src/linux/timerfd.rs` (port `linux/timerfd.zig`, 89 loc)
- Modify: `crates/rustty-xev/src/async.rs` (add the `#[cfg(target_os="linux")]` eventfd `Async` arm; ref `watcher/async.zig` `.io_uring, .epoll => …` eventfd variant at `async.zig:85,199`)

**Interfaces:**
- Consumes: `libc::{timerfd_create, timerfd_settime, eventfd, read, write, close}`.
- Produces: `timerfd::Timerfd { fd }` with `set`/`get`/`deinit`; `epoll::Async { fd: RawFd }` (eventfd-backed) with `init() -> Result<Async, Error>`, `deinit`, `notify() -> Result<(), Error>`, `wait::<UD>(…)`. NOTE: on epoll the loop's own wakeup IS an `Async` (eventfd) per `epoll.zig:37–41,78`. `Async` stays `Copy` (it wraps a raw fd) — required by the trait surface.

- [ ] **Step 1: Write the failing tests** — port `epoll: timerfd` (`epoll.zig:1626`) as `#[test] fn epoll_timerfd()` and the eventfd notify round-trip as a live KAT `#[test] fn epoll_async_wakeup()` (mirror the kqueue KAT 1 in `async.rs`).
- [ ] **Step 2: Run to verify they fail** — `cargo test -p rustty-xev --target aarch64-unknown-linux-gnu epoll_timerfd epoll_async_wakeup` → FAIL.
- [ ] **Step 3: Implement** — port `timerfd.zig` verbatim (create/settime with `CLOCK_MONOTONIC`, `TFD_NONBLOCK|TFD_CLOEXEC`); implement the eventfd `Async` arm (`eventfd(0, EFD_CLOEXEC|EFD_NONBLOCK)`, `notify` = `write(fd, &1u64)`, `wait` sets `op = .read`/poll on the eventfd; drain coalesces per `async.zig` eventfd `drain`). Hand-write `impl Copy for Async` + `impl Default` if the trait surface needs it.
- [ ] **Step 4: Run to verify they pass** — same command → PASS.
- [ ] **Step 5: Commit** — `git commit -m "rustty-xev: linux timerfd + epoll eventfd Async (L1 LT2)"`

---

### Task 3: epoll `Loop` core — timers, stop, cancellation

**Files:**
- Modify: `crates/rustty-xev/src/epoll.rs` (the `Loop`/`Completion`/`Operation`/`Result`, `submit`/`tick`/`run`, `epoll_create1` + `epoll_ctl`/`epoll_wait`, the timer heap wiring)

**Interfaces:**
- Consumes: `heap.rs` (the shared pairing heap, reused as-is), `queue.rs` (intrusive FIFO), `linux/timerfd.rs`, epoll `Async` (Task 2), `loop.rs` (`RunMode`/`CallbackAction`/`CompletionState`/`Callback`).
- Produces: `epoll::Loop` with the [ADV-FIX F1] public surface `{init, deinit, stop, stopped, add, submit, run, tick, now, update_now, timer, timer_reset}`; `epoll::Completion` (hand-written `Default`, `const _: () = assert!(size_of::<Completion>()==<ref>)` tripwire from `epoll.zig:1380` "Completion size"); `epoll::Operation`/`Result` (same 17-variant tag set as kqueue, ref `epoll.zig`); `unsafe impl Send for Loop` (thread-affinity note, [ADV-FIX F2]).

- [ ] **Step 1: Write the failing tests** — port these 8 from `epoll.zig`: `Completion size` (:1380), `epoll: default completion` (:1387), `epoll: loop time` (:1403), `epoll: stop` (:1417), `epoll: timer` (:1446), `epoll: timer reset` (:1502), `epoll: timer reset before tick` (:1544), `epoll: timer reset after trigger` (:1582), plus `epoll: timer cancellation` (:1941) and `epoll: canceling a completed operation` (:2002) as `#[test] fn epoll_<snake_name>()`.
- [ ] **Step 2: Run to verify they fail** — `cargo test -p rustty-xev --target aarch64-unknown-linux-gnu epoll_` → FAIL (compile/assert).
- [ ] **Step 3: Implement** — port the epoll `Loop` from `epoll.zig` mirroring `kqueue.rs`'s `Loop`: `epoll_create1(EPOLL_CLOEXEC)` + the always-registered eventfd (`epoll.zig:78–110`), the `submit`/`tick` ordering (process_cancellations → submit via `epoll_ctl` → fire expired heap timers → drain completions → compute timerfd/derived timeout → `epoll_wait` → dispatch `perform`), `RunMode` dispatch (`run` → `tick(0/1/until-done)`). Keep raw-ptr discipline. Hand-write `Default`/`Send`.
- [ ] **Step 4: Run to verify they pass** — same command → 10 epoll tests PASS.
- [ ] **Step 5: Commit** — `git commit -m "rustty-xev: epoll Loop core + timers/cancellation (L1 LT3)"`

---

### Task 4: epoll socket ops + `Process`/`Stream` arms

**Files:**
- Modify: `crates/rustty-xev/src/epoll.rs` (the `Operation.perform` socket arms: accept/connect/send/recv/close/read/write/poll)
- Modify: `crates/rustty-xev/src/process.rs` (epoll `Process` arm — `pidfd_open` + `epoll` readable, `waitpid` reap; ref `watcher/process.zig` linux arm)
- Modify: `crates/rustty-xev/src/stream.rs` (epoll read/write/close/poll + `queueWrite` arms)

**Interfaces:**
- Consumes: `epoll::Loop` (Task 3), `queue.rs` (`WriteQueue`).
- Produces: `epoll::Process { pid }` (`init`/`deinit`/`wait`), `epoll` `Stream`/`WriteQueue`/`WriteRequest` arms; all satisfy the shared watcher signatures in `stream.rs`/`process.rs`.

- [ ] **Step 1: Write the failing tests** — port `epoll: socket accept/connect/send/recv/close` (`epoll.zig:1672`) as `#[test] fn epoll_socket_roundtrip()`; add live KATs `#[test] fn epoll_process_spawn_exit()` (spawn `/bin/true`, `Process.wait`, assert status 0) and `#[test] fn epoll_pty_write_roundtrip()` (openpty + `queue_write` + read back), mirroring kqueue KATs 3/4.
- [ ] **Step 2: Run to verify they fail** — `cargo test -p rustty-xev --target aarch64-unknown-linux-gnu epoll_socket epoll_process epoll_pty` → FAIL.
- [ ] **Step 3: Implement** — port the socket `perform` arms from `epoll.zig`; the epoll `Process` via `pidfd_open(pid, 0)` registered readable → `waitpid(pid, WNOHANG)` reap (fallback to SIGCHLD path if `pidfd_open` ENOSYS); the epoll `Stream` read/write/poll + the `queueWrite` ordered-write engine (partial-write remainder re-init, `container_of` `WriteRequest::from`, byte-faithful to `stream.zig:780`).
- [ ] **Step 4: Run to verify they pass** — same command → PASS.
- [ ] **Step 5: Commit** — `git commit -m "rustty-xev: epoll socket/process/stream arms + live KATs (L1 LT4)"`

**Gate after Task 4:** `cargo test -p rustty-xev --target aarch64-unknown-linux-gnu` runs all 12 epoll `#[test]` + 3 epoll live KATs green; macOS `cargo test -p rustty-xev` unaffected.

---

### Task 5: io_uring ring binding + `Loop` core

**Files:**
- Modify: `crates/rustty-xev/src/io_uring.rs` (a minimal `IoUring` ring over libc syscalls, then `Loop`/`Completion`/`Operation`/`Result`/`submit`/`tick`)

**Interfaces:**
- Consumes: `libc::{syscall, mmap, munmap}` + the `__NR_io_uring_setup`/`__NR_io_uring_enter` numbers, `heap.rs`, `queue.rs`, `loop.rs`.
- Produces: `io_uring::IoUring` (`init(entries) -> Result<Self,Error>`, `get_sqe`, `submit`, `copy_cqes`, `deinit`) — a faithful subset of Zig's `std.os.linux.IoUring` used by `io_uring.zig:32,68`; `io_uring::Loop` with the same [ADV-FIX F1] public surface; `io_uring::Completion` (`Default`, size-assert from `io_uring.zig:1117`); `unsafe impl Send`.

**Decision baked in (R3):** hand-roll the ring via raw syscalls + `mmap` of the SQ/CQ regions (io_uring is submission-based: fill SQEs, `io_uring_enter`, drain CQEs). Do NOT add the `io-uring` crate. This mirrors `std.os.linux.IoUring` (itself a thin wrapper) and keeps `rustty-xev → {rustty-global, libc}`.

- [ ] **Step 1: Write the failing tests** — port `io_uring: available` (:1124), `io_uring: overflow entries count` (:1128), `Completion size` (:1117), `io_uring: default completion` (:1143), `io_uring: loop time` (:1343), `io_uring: stop` (:1314) as `#[test] fn io_uring_<snake>()`. Guard the whole module `#[cfg(target_os="linux")]`; `io_uring: available` returns false on kernels without io_uring (the `Loop` falls back — tested in dynamic Task 8).
- [ ] **Step 2: Run to verify they fail** — `cargo test -p rustty-xev --target aarch64-unknown-linux-gnu io_uring_` → FAIL.
- [ ] **Step 3: Implement** — port the `IoUring` ring (setup/enter/mmap SQ+CQ, `get_sqe`/`submit`/`copy_cqes`) and the `Loop` core from `io_uring.zig` (submit = fill SQEs from `submissions`; `tick` = `submit_and_wait` then drain CQEs → `perform` → callback). Keep the pairing-heap timers derived into an `IORING_OP_TIMEOUT` (or timerfd) per `io_uring.zig`. Ring memory is `mmap`'d kernel-shared — annotate the raw-ptr lifetime + add a MIRI_EXCEPTIONS note.
- [ ] **Step 4: Run to verify they pass** — same command → PASS.
- [ ] **Step 5: Commit** — `git commit -m "rustty-xev: io_uring ring binding + Loop core (L1 LT5)"`

---

### Task 6: io_uring timers, socket ops + watcher arms

**Files:**
- Modify: `crates/rustty-xev/src/io_uring.rs` (timer + socket `perform` arms), `async.rs`/`timer.rs`/`process.rs`/`stream.rs` (io_uring arms)

**Interfaces:**
- Consumes: `io_uring::Loop`/`IoUring` (Task 5).
- Produces: io_uring `Async` (eventfd via `IORING_OP_POLL_ADD`/read), `Timer` (`IORING_OP_TIMEOUT`), `Process`, `Stream`/`WriteQueue` arms.

- [ ] **Step 1: Write the failing tests** — port the remaining io_uring tests: `io_uring: timerfd` (:1173), `io_uring: timer` (:1226), `io_uring: timer reset` (:1272), `io_uring: timer remove` (:1357), `io_uring: socket accept/connect/send/recv/close` (:1401), `io_uring: sendmsg/recvmsg` (:1626), `io_uring: socket read cancellation` (:1725) as `#[test] fn io_uring_<snake>()`; add live KATs `io_uring_process_spawn_exit` + `io_uring_pty_write_roundtrip`.
- [ ] **Step 2: Run to verify they fail** — `cargo test -p rustty-xev --target aarch64-unknown-linux-gnu io_uring_` → FAIL.
- [ ] **Step 3: Implement** — port the timer (`IORING_OP_TIMEOUT`/`TIMEOUT_REMOVE`) and socket/sendmsg/recvmsg/cancel `perform` arms; add the io_uring arms to the watcher files behind `#[cfg(target_os="linux")]` + the backend match.
- [ ] **Step 4: Run to verify they pass** — same command → PASS.
- [ ] **Step 5: Commit** — `git commit -m "rustty-xev: io_uring timers/sockets/watcher arms + KATs (L1 LT6)"`

**Gate after Task 6:** all 13 io_uring `#[test]` + 2 io_uring live KATs green on Linux; macOS unaffected.

---

### Task 7: `dynamic.rs` — the hand-authored 2-backend union selector

**Files:**
- Modify: `crates/rustty-xev/src/dynamic.rs` (the full `Dynamic` layer)

**Interfaces:**
- Consumes: `epoll::*` (Tasks 2–4), `io_uring::*` (Tasks 5–6), `backend.rs`.
- Produces (the Linux public surface `lib.rs` re-exports): all under `#[cfg(target_os="linux")]`. Concrete hand-authored analogues of `dynamic.zig` (NO comptime `@Type`; a fixed 2-variant enum per type):
  ```rust
  pub static BACKEND: AtomicU8 = AtomicU8::new(Backend::Epoll as u8); // last candidate default (dynamic.zig:91)
  pub fn detect() -> Result<(), Error>;   // io_uring.available() ? IoUring : Epoll (dynamic.zig:96)
  pub fn prefer(b: Backend) -> bool;      // dynamic.zig:109

  pub struct Loop { backend: LoopUnion }  // dynamic.zig:122
  enum LoopUnion { IoUring(io_uring::Loop), Epoll(epoll::Loop) }
  impl Loop { pub fn init(o: Options)->Result<Loop,Error>; deinit; stop; stopped; add; submit;
              run(RunMode)->Result<(),Error>; tick; now; update_now; timer; timer_reset }

  #[derive(Clone, Copy)]                  // DynamicCompletion is a TAGGED union (dynamic.zig:240)
  pub enum Completion { IoUring(io_uring::Completion), Epoll(epoll::Completion) }
  impl Default for Completion { /* Epoll variant zero-init — candidates[last] (dynamic.zig:251) */ }
  impl Completion { pub fn state(&self)->CompletionState; pub fn ensure_tag(&mut self, Backend); }

  #[derive(Clone, Copy)] pub enum Async { IoUring(io_uring::Async), Epoll(epoll::Async) } // : Copy REQUIRED
  pub enum Process { IoUring(io_uring::Process), Epoll(epoll::Process) }
  pub struct Timer;  pub struct Stream { … }
  pub struct WriteQueue { … } pub struct WriteRequest { … }  // Default REQUIRED
  pub type Error = …; pub enum NotifyError { … }  // Debug+Clone+Copy+PartialEq+Eq REQUIRED — merged ErrorSet (dynamic.zig:512)
  ```
  Every method dispatches `match self.backend { LoopUnion::IoUring(l)=>l.method(), LoopUnion::Epoll(l)=>l.method() }`, reading `BACKEND` for constructors (`Loop::init`, `Completion::init`) exactly as `dynamic.zig`'s `switch (backend)`.

- [ ] **Step 1: Write the failing tests** — port the 5 `dynamic.zig` tests: `refAllDecls` (as a `dynamic_ref_all_decls` compile-touch), `completion is zero-able` (:202 → `dynamic_completion_zero_able`), `detect` (:207), `prefer` (:218), `loop basics` (:229 → `dynamic_loop_basics`).
- [ ] **Step 2: Run to verify they fail** — `cargo test -p rustty-xev --target aarch64-unknown-linux-gnu dynamic_` → FAIL.
- [ ] **Step 3: Implement** — hand-author the union types + `BACKEND` selector + `detect`/`prefer` per the signatures above. `Completion`/`WriteQueue` default to the **last-candidate (Epoll)** variant (`dynamic.zig:251,377`); `ensure_tag` re-tags on mismatch (`dynamic.zig:283`). Merge each backend's error set into `NotifyError`/`Error` (`ErrorSet`, `dynamic.zig:512`). Assert the trait bounds with `const _: fn() = || { fn needs<T: Copy>(){} needs::<Async>(); };`-style compile checks.
- [ ] **Step 4: Run to verify they pass** — same command → 5 dynamic tests PASS.
- [ ] **Step 5: Commit** — `git commit -m "rustty-xev: hand-authored Dynamic io_uring/epoll selector (L1 LT7)"`

---

### Task 8: `lib.rs` per-target re-export + gate wiring

**Files:**
- Modify: `crates/rustty-xev/src/lib.rs:156–205` (Linux re-export block), `backend.rs`
- Modify: `.port/tools/check_test_counts.py` (or its manifest) — add epoll/io_uring/dynamic count rows
- Modify: `.port/phase-2/DEFERRALS.md` (record: `file/tcp/udp` watchers still deferred on Linux too; ThreadPool body still deferred; any skip-stub)

**Interfaces:**
- Consumes: `dynamic::*` (Task 7), `epoll`/`io_uring` (as `dynamic`'s internals).
- Produces: on Linux, `pub use dynamic::{Loop, Completion, Async, Timer, Process, Stream, WriteQueue, WriteRequest, Error, NotifyError, …}` under the SAME names the macOS block exports — so `rustty_xev::Loop` etc. resolve to `Dynamic` on Linux, flat kqueue on macOS. `Backend`/`RunMode`/`CallbackAction` stay shared.

- [ ] **Step 1: Write the failing test** — add `crates/rustty-xev/tests/surface_parity.rs`:
  ```rust
  // Compiles only if the public names resolve on BOTH targets with the required bounds.
  fn _assert_surface() {
      fn needs_copy<T: Copy>() {} fn needs_default<T: Default>() {}
      needs_copy::<rustty_xev::Async>();
      needs_default::<rustty_xev::Completion>();
      needs_default::<rustty_xev::WriteQueue>();
      let _l: fn(rustty_xev::Options) -> Result<rustty_xev::Loop, rustty_xev::Error> = rustty_xev::Loop::init;
  }
  ```
- [ ] **Step 2: Run to verify it fails** — `cargo test -p rustty-xev --target aarch64-unknown-linux-gnu --no-run` → FAIL (names not re-exported on Linux).
- [ ] **Step 3: Implement** — add the `#[cfg(target_os="linux")] pub use dynamic::…` block mirroring the macOS block; wire `check_test_counts.py` rows (epoll 12 / io_uring 13 / dynamic 5) with a libxev base-path or in-repo manifest ([ADV-FIX F7] precedent); add the DEFERRALS entries.
- [ ] **Step 4: Run to verify it passes** — `--no-run` compiles on Linux AND macOS; `cargo test -p rustty-xev` green on both; `python3 .port/tools/check_test_counts.py` green; `bash .port/tools/check_acyclic.sh` green (xev still `→ {global, libc}`).
- [ ] **Step 5: Commit** — `git commit -m "rustty-xev: per-target Dynamic re-export + gate wiring (L1 LT8)"`

**Gate after Task 8:** `rustty-xev` is a self-contained, gate-green Linux+macOS crate (Milestone-A-equivalent for Linux). No downstream crate touched yet.

---

### Task 9: downstream un-defer — termio live shell on Linux

**Files:**
- Modify: `crates/rustty-termio/tests/live_shell.rs:1` (`#[cfg(target_os="macos")]` → `#[cfg(unix)]`)
- Modify: any termio/core site that `#[cfg(target_os="macos")]`-gated an xev-dependent path that should be `#[cfg(unix)]` (grep `xev` + `cfg(target_os = "macos")` in termio/core; the mailbox/exec/thread wiring is already target-neutral via `xev_seam`).

**Interfaces:**
- Consumes: the Linux `rustty_xev::*` Dynamic surface (Task 8) through `termio::xev_seam` (`pub use rustty_xev::*`).
- Produces: a live PTY shell on Linux — the existing `live_shell.rs` KAT (spawn `/bin/sh -c 'echo …'`, pump `run(NoWait)` under a deadline+tick cap, assert the echo round-trips into the `Screen`) passing on Linux.

- [ ] **Step 1: Widen the KAT gate** — change `live_shell.rs` guard to `#[cfg(unix)]`; call `rustty_xev::detect()` at test start (Linux selects io_uring→epoll; on macOS `detect` is the no-op single-candidate path).
- [ ] **Step 2: Run to verify current state** — `cargo test -p rustty-termio --target aarch64-unknown-linux-gnu --test live_shell` → observe first failure (compile or runtime); triage per systematic-debugging.
- [ ] **Step 3: Fix the wiring** — resolve any `cfg(target_os="macos")` that should be `cfg(unix)` on the xev-dependent termio/core launch path so `Termio::init`/`threadEnter`/`Subprocess::start`/`ReadThread` run under the Dynamic loop on Linux. (No new logic — these bodies exist; only the target gates and the `xev::Loop`/`Async` types differ, now supplied by Dynamic.)
- [ ] **Step 4: Run to verify it passes** — `cargo test -p rustty-termio --target aarch64-unknown-linux-gnu --test live_shell` → PASS (the port spawns a live shell on Linux); macOS `--test live_shell` still green.
- [ ] **Step 5: Commit** — `git commit -m "rustty-termio: live shell on Linux over the xev Dynamic loop (L1 LT9)"`

**Final gate:** `cargo test -p rustty-xev -p rustty-termio` green on Linux AND macOS; `cargo build --workspace` green on Linux; clippy `-D warnings` / fmt / `check_acyclic.sh` / `check_test_counts.py` green; DEFERRALS.md updated. **The port runs a live shell on Linux.**

---

## Deferred (out of L1 scope, documented)
- `watcher/file.zig`/`tcp.zig`/`udp.zig` — unused by ghostty (deferred on macOS too); keep deferred on Linux.
- `ThreadPool` body — type surface only; ghostty passes no pool.
- `io_uring` advanced ops beyond what the shell path exercises (multishot, registered buffers) — port only if a reference decl forces it.
- `renderer.Thread` event loop — GPU+xev blocked; that's L4/L5, not L1.

## Self-review notes (spec coverage)
- epoll 12 tests → Tasks 3–4. io_uring 13 → Tasks 5–6. dynamic 5 → Task 7. ✓
- The `Dynamic` "no comptime" divergence → Task 7 (hand-authored concrete enums). ✓
- Trait-surface bounds ([ADV-FIX F3]) → Task 8 `surface_parity.rs` compile-gate. ✓
- Per-target re-export (macOS unchanged) → Task 8. ✓
- Live-shell proof → Task 9. ✓
- R3 io_uring self-containment (no new crate) → Global Constraints + Task 5 decision. ✓
