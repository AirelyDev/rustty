# rustty-xev libxev Completion (File / TCP / UDP watchers + ThreadPool on Linux) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development (or a dynamic Workflow). Steps use checkbox (`- [ ]`) syntax. This is an R1 faithful port — each task cites the exact oracle file + LOC; the "code" is the faithful Rust port of that oracle (literal reproduction in-plan would duplicate ~5k LOC — same precedent as `2026-07-14-inspector-port.md`).

**Goal:** Complete `rustty-xev`'s libxev surface on Linux — port the `ThreadPool` and the `xev.File`/`xev.TCP`/`xev.UDP` watchers (currently deferred, "zero ghostty uses"), so `rustty-xev` is a 1:1 port of libxev's API for external reuse.

**Architecture:** Dependency-ordered port. (1) `ThreadPool` — the lock-free work-stealing pool (self-contained; unblocks File-on-epoll + the epoll `Loop` fields); proven SB-clean on an isolated Miri harness per the port's mirihar doctrine. (2) Un-gate the already-ported `stream.rs` + `net.rs` for Linux (the watchers monomorphize over `Stream(...)`). (3) `TCP` (all ops already on both backends). (4) epoll `SendMsg`/`RecvMsg` ops + `UDP`. (5) `File` (blocking pread/pwrite/close route through `ThreadPool` on epoll). (6) `lib.rs` Linux exports + the full oracle test suite.

**Tech Stack:** Rust, `rustty-xev` (Rust port of mitchellh/libxev), the epoll + io_uring Linux backends, `libc`, Miri (Stacked/Tree Borrows) for the lock-free pool.

## Global Constraints

- **Oracle:** mitchellh/libxev @ `34fa50878aec6e5fa8f532867001ab3c36fae23e` — vendored READ-ONLY at `/home/gael/tmp/libxev-oracle/libxev-34fa50878aec6e5fa8f532867001ab3c36fae23e/src/` for this session (re-fetch: `curl -fsSL https://deps.files.ghostty.org/libxev-34fa50878aec6e5fa8f532867001ab3c36fae23e.tar.gz | tar xz`). This is the SAME commit ghostty pins (`ghostty/build.zig.zon:10-13`).
- **R1 fidelity** — port every line of each oracle `.zig`; preserve its comments as ported comments; port its `test{}` blocks as `#[cfg(test)]` tests (file=8, tcp=6, udp=6).
- **SB-class lock-free code PROVEN on Miri** — the `ThreadPool` intrusive `Node.Queue`/`Node.List` (raw-pointer atomic queue) is proven SB/TB-clean on an isolated non-FFI `.port/phase-2/mirihar-threadpool` harness BEFORE shipping (the in-repo mirihar doctrine, `PORT_RULES.md:209`; precedent: `mirihar-chunk3b`, `mirihar-inspector-pin`).
- **Symmetry** — the goal is that the Linux and macOS `rustty-xev` surfaces export the SAME set (`File`/`TCP`/`UDP`/`ThreadPool`). macOS already forwards these types; this plan makes Linux match.
- **panic=abort profile** — assert-class failures abort (Zig-trap parity).
- **Build gate** — `cargo build -p rustty-xev` + `cargo test -p rustty-xev` green after every task (on Linux). `cargo build --workspace` green at phase ends.
- **Git** — explicit paths; retry on `index.lock` (sleep 3, ≤10×); never push/reset/stash.

## File Structure

- **Modify:** `crates/rustty-xev/src/thread_pool.rs` — inhabit the `ThreadPool` body (currently `unimplemented!()` stub) + un-gate for Linux.
- **Modify:** `crates/rustty-xev/src/lib.rs` — un-gate `mod stream`, `mod net`, `mod thread_pool` for Linux (currently `#[cfg(target_os="macos")]`); add `mod file/tcp/udp` + export `File`/`TCP`/`UDP` on Linux (the `dynamic.rs`/`main.rs` forwarding the header comment at `:184,304` says is deferred).
- **Modify:** `crates/rustty-xev/src/epoll.rs` — add `thread_pool` + `thread_pool_completions` fields to `Loop` (`:155`), the `Operation::{SendMsg,RecvMsg}` arms (msghdr ops; io_uring already has them at `:1732`), and route blocking `Close`/pread/pwrite through the pool (replacing the `CloseError::ThreadPoolRequired` sentinel at `:882-892`).
- **Modify:** `crates/rustty-xev/src/linux.rs` — change `Options.thread_pool` (`:44-66`) from the opaque `Option<*mut c_void>` to the real `Option<*mut ThreadPool>`.
- **Modify:** `crates/rustty-xev/src/net.rs` — un-gate for Linux (the `std.net.Address` subset TCP/UDP need).
- **Create:** `crates/rustty-xev/src/watcher/` → `file.rs`, `tcp.rs`, `udp.rs` (or flat `file.rs`/`tcp.rs`/`udp.rs` per the crate's existing convention — match how `stream.rs` sits).
- **Create:** `.port/phase-2/mirihar-threadpool/` — the isolated Miri harness for the lock-free `Node.Queue`.
- **Reused as-is:** `stream.rs` (the `Stream(xev, T, options)` generic — DONE, `lib.rs:223`), the epoll/io_uring `Operation::{Read,Write,Send,Recv,Accept,Connect,Shutdown}` arms + `Result` types.

---

## Task 1: `ThreadPool` — the lock-free work-stealing pool

**Files:** Modify `crates/rustty-xev/src/thread_pool.rs` (inhabit the stub, un-gate for Linux) + `crates/rustty-xev/src/lib.rs` (un-gate `mod thread_pool` for Linux). Create `.port/phase-2/mirihar-threadpool/`.
**Oracle:** `.../src/ThreadPool.zig` (827 LOC). Structure: `run_queue: Node.Queue`, packed atomic `Sync` (idle:u14/spawned:u14/state:enum), `Config`, `init`/`deinit`, `Task{node,callback}`, `Batch{head,tail,push,from}`, `schedule`, the `Node.Queue`/`Node.List`/`Node.Stack` intrusive lock-free queues, `join`/`register`/`unregister`/`wait`/`notify`, the worker `run` loop with work-stealing.

**Interfaces:**
- Produces: `ThreadPool` (`init(Config) -> ThreadPool`, `deinit(&mut)`, `shutdown(&mut)`, `schedule(&self, Batch)`), `Task { node: Node, callback: fn(*mut Task) }`, `Batch { from(*mut Task) -> Batch, push(&mut, Batch) }`. Consumed by Task 5 (File) + Task 3/5 epoll blocking ops.

- [ ] **Step 1: Read the oracle fully.** Read `ThreadPool.zig` end-to-end (all 827 lines) — it's a copy of kprotty's Zap work-stealing pool. Note the atomics: `run_queue` is a Vyukov-style intrusive MPSC `Node.Queue` (a swap-based unbounded queue); `Sync` is a `packed struct` in an `Atomic(u32)` mutated via CAS loops; per-thread `Node.Buffer` (bounded work-stealing deque) + `Node.Stack`. Preserve every comment.
- [ ] **Step 2: The mirihar Miri proof FIRST (SB-class).** Create `.port/phase-2/mirihar-threadpool/` (mirror `mirihar-chunk3b` layout: Cargo.toml + src + README). Port ONLY the raw-pointer `Node.Queue` (push/pop swap-based MPSC) + `Node.Buffer` (steal/push/pop) in isolation, and write a multi-threaded `#[test]` that hammers push/pop/steal across threads. Run `cargo +nightly miri test` under Stacked AND Tree Borrows → must be clean (no raw-alias UB). Only once green, port the same primitives into `thread_pool.rs`. Document the SB-clean interleave (per [[shaper-hook-sb-clean-interleave]] doctrine).
- [ ] **Step 3: Port the full `ThreadPool`** into `thread_pool.rs`, replacing the `_deferred: ()` + `unimplemented!()` bodies. Faithful: the `Sync` CAS state machine, dynamic thread spawn (`std.Thread` → `std::thread`), `schedule`/`run`/`join`/`wait`/`notify`. Un-gate: change `lib.rs:146` `#[cfg(target_os="macos")] mod thread_pool;` to compile on Linux too (`#[cfg(any(target_os="macos", target_os="linux"))]` or unconditional).
- [ ] **Step 4: Port the oracle's exercise.** ThreadPool.zig has no `test{}` of its own, but port a schedule/execute smoke test (spawn N tasks across the pool, assert all callbacks ran, `deinit` joins cleanly) as `#[cfg(test)]`. Run `cargo test -p rustty-xev thread_pool` green.
- [ ] **Step 5: Verify + commit.** `cargo build -p rustty-xev` (Linux) green; `cargo +nightly miri test` (the mirihar harness) green. Commit (`xev: port ThreadPool (lock-free work-stealing pool) + mirihar Miri proof (Task 1)`).

## Task 2: Un-gate `stream.rs` + `net.rs` for Linux + wire epoll `Loop` thread_pool fields

**Files:** Modify `crates/rustty-xev/src/lib.rs` (un-gate `mod stream`, `mod net`), `crates/rustty-xev/src/epoll.rs` (add `thread_pool` + `thread_pool_completions` to `Loop` at `:155`; route `Close`/pread/pwrite through the pool at `:882-892`), `crates/rustty-xev/src/linux.rs` (`Options.thread_pool` → `Option<*mut ThreadPool>` at `:44-66`).
**Oracle:** `stream.zig` (the `Stream(xev, T, options)` generic — already ported, just Linux-gate it), the epoll backend's thread-pool integration in libxev's `src/backend/epoll.zig` (the `thread_pool`/`thread_schedule` path).

**Interfaces:**
- Consumes: `ThreadPool` (Task 1).
- Produces: `xev::Stream`/`WriteQueue`/`WriteRequest` compiled on Linux; `epoll::Loop` with a live `thread_pool` field; blocking `Close`/pread/pwrite that schedule onto the pool (no longer return `ThreadPoolRequired` when a pool is present).

- [ ] **Step 1: Un-gate stream + net.** In `lib.rs`, change the `#[cfg(target_os="macos")]` on `mod stream;` (`:144`) and `mod net;` (`:131`) to include Linux. Build `cargo build -p rustty-xev` (Linux) — fix any macOS-specific code (e.g. kqueue-only refs) so `stream.rs`/`net.rs` compile on Linux against the epoll/io_uring backend surface (which already exposes the same `Operation`/`Result`/`Completion` types the Stream generic needs — Read/Write/Close/Send/Recv/Accept/Connect/Shutdown).
- [ ] **Step 2: Wire the epoll `Loop` pool fields.** Add `thread_pool: ?*ThreadPool` + `thread_pool_completions: TaskCompletionQueue` to `epoll::Loop` (`:155`), populated from `linux::Options.thread_pool` (change its type from `Option<*mut c_void>` to `Option<*mut ThreadPool>`, `linux.rs:44-66`). Port libxev epoll's `thread_schedule`/`thread_perform` path so blocking ops (`Close`, and File's pread/pwrite) enqueue a `Task` onto the pool and drain completions — replacing the `CloseError::ThreadPoolRequired` sentinel (`epoll.rs:882-892`) with real execution when `thread_pool != null`. (The kqueue side already staged the `thread_pool_completions` aliasing fix, DEFERRALS 4396-4398 — mirror it for epoll.)
- [ ] **Step 3: Verify + commit.** `cargo build -p rustty-xev` (Linux) green; existing xev tests still green (`cargo test -p rustty-xev`). Commit (`xev: un-gate stream/net for Linux + wire epoll Loop thread_pool (Task 2)`).

## Task 3: `TCP` watcher

**Files:** Create `crates/rustty-xev/src/watcher/tcp.rs` (or `tcp.rs` per crate convention); modify `lib.rs` (add `mod tcp` + `pub use tcp::TCP` on Linux).
**Oracle:** `.../src/watcher/tcp.zig` (935 LOC, 6 tests). `xev.TCP` = a socket over `Stream(xev, TCP, .{.close,.read,.write})` + `Operation::{Accept,Connect,Send,Recv,Shutdown}`. Methods: `init`/`initFd`/`bind`/`listen`/`accept`/`connect`/`read`/`write`/`shutdown`/`close`.

**Interfaces:**
- Consumes: `Stream` (Task 2), `net::Address` (Task 2), the epoll/io_uring `Accept/Connect/Send/Recv/Shutdown` ops (already present on both backends).
- Produces: `xev::TCP`.

- [ ] **Step 1: Port `tcp.zig`** faithfully into `tcp.rs` — the `TCP` struct (an fd), monomorphizing `read`/`write`/`close` over rustty's `Stream(...)` generic (the same way the macOS-side would); `bind`/`listen` are plain libc; `accept`/`connect`/`shutdown` submit the existing `Operation` arms. Port the 6 `test{}` blocks (loopback accept/connect/send/recv round-trips) as `#[cfg(test)]`.
- [ ] **Step 2: Export + verify.** `lib.rs`: `mod tcp;` + `pub use tcp::TCP;` on Linux. `cargo test -p rustty-xev tcp` → the 6 tests green (real loopback sockets on the epoll + io_uring backends). Commit (`xev: port TCP watcher (Task 3)`).

## Task 4: epoll `SendMsg`/`RecvMsg` ops + `UDP` watcher

**Files:** Modify `crates/rustty-xev/src/epoll.rs` (add `Operation::{SendMsg,RecvMsg}` start+perform arms — io_uring already has them at `:1732`, `prep_sendmsg`/`prep_recvmsg` at `:1018/1030`); create `crates/rustty-xev/src/watcher/udp.rs`; modify `lib.rs`.
**Oracle:** `.../src/watcher/udp.zig` (1003 LOC, 6 tests) + libxev's epoll `SendMsg`/`RecvMsg` (msghdr recvmsg/sendmsg). `xev.UDP` = a datagram socket using `Operation::{SendMsg,RecvMsg}` + `msghdr`.

**Interfaces:**
- Consumes: `net::Address` (Task 2), the io_uring SendMsg/RecvMsg (present), the NEW epoll SendMsg/RecvMsg (this task).
- Produces: `xev::UDP`.

- [ ] **Step 1: Add epoll `SendMsg`/`RecvMsg`.** Mirror the existing epoll `Send`/`Recv` arms (`epoll.rs:858-870`) but with `libc::sendmsg`/`recvmsg` over a `*mut libc::msghdr` (matching the io_uring `Operation::SendMsg` payload shape at `:1732`). Add the `Result::{SendMsg,RecvMsg}` if not already shared.
- [ ] **Step 2: Port `udp.zig`** faithfully into `udp.rs` — the `UDP` struct (fd), `init`/`bind`/`read`(RecvMsg)/`write`(SendMsg)/`close`, building the `msghdr` + iovec. Port the 6 `test{}` blocks (loopback datagram send/recv) as `#[cfg(test)]`.
- [ ] **Step 3: Export + verify.** `lib.rs`: `mod udp;` + `pub use udp::UDP;` on Linux. `cargo test -p rustty-xev udp` → 6 tests green on BOTH backends. Commit (`xev: epoll SendMsg/RecvMsg + UDP watcher (Task 4)`).

## Task 5: `File` watcher

**Files:** Create `crates/rustty-xev/src/watcher/file.rs`; modify `lib.rs`.
**Oracle:** `.../src/watcher/file.zig` (848 LOC, 8 tests). `xev.File` = a regular-fd watcher over `Stream(xev, File, .{.close,.read,.write})`; on the epoll backend `pread`/`pwrite`/`close` are BLOCKING and route through the `ThreadPool` (io_uring does them natively).

**Interfaces:**
- Consumes: `Stream` (Task 2), `ThreadPool` (Task 1), the epoll pool-routing (Task 2).
- Produces: `xev::File`.

- [ ] **Step 1: Port `file.zig`** faithfully into `file.rs` — `File` struct (fd), `init`/`initFd`/`read`/`write`/`pread`/`pwrite`/`close`, monomorphizing over `Stream(...)`. On epoll the pread/pwrite/close submit through the `thread_pool` path (Task 2); on io_uring they use the native ops. Port the 8 `test{}` blocks as `#[cfg(test)]`.
- [ ] **Step 2: Export + verify.** `lib.rs`: `mod file;` + `pub use file::File;` on Linux. `cargo test -p rustty-xev file` → 8 tests green on BOTH backends (the epoll ones exercise the ThreadPool path — this is the real integration test for Task 1's pool). Commit (`xev: port File watcher + epoll ThreadPool integration (Task 5)`).

## Task 6: Full Linux export symmetry + suite + ledger

**Files:** Modify `crates/rustty-xev/src/lib.rs` (the top-level `dynamic.rs`/`main.rs` forwarding at `:184,304` — export `File`/`TCP`/`UDP`/`ThreadPool` on Linux, matching macOS). Modify `.port/phase-2/DEFERRALS.md` (mark the four items RESOLVED at `:3285-3343`).

- [ ] **Step 1: Export symmetry.** Ensure `lib.rs` exports `File`/`TCP`/`UDP`/`ThreadPool` on Linux identically to macOS (update the `:184` "File/TCP/UDP deferred" + `:304` comments). Confirm `dynamic.rs` (the runtime-selected backend) forwards them.
- [ ] **Step 2: Full suite.** `cargo test -p rustty-xev` → ALL tests green on Linux (the 8+6+6 = 20 new watcher tests + ThreadPool smoke + the existing suite), on both epoll and io_uring where the tests parameterize. `cargo build --workspace` green. `cargo +nightly miri test` (mirihar-threadpool) green.
- [ ] **Step 3: Ledger + commit.** Update DEFERRALS.md (the "STILL DEFERRED on Linux" list `:3285-3343` → RESOLVED) + the `lib.rs` header comments. Update the parity memory. Commit (`xev: File/TCP/UDP/ThreadPool complete on Linux — libxev surface parity (Task 6)`).

---

## Self-Review

**1. Spec coverage:** ThreadPool → Task 1 (+ Miri). stream/net un-gate + epoll pool wiring → Task 2. TCP → Task 3. epoll msghdr + UDP → Task 4. File (+ pool integration) → Task 5. Export symmetry + suite → Task 6. ✅

**2. Placeholder scan:** R1 port — each task cites the exact oracle file + LOC + the structures/hazards; the code is the faithful port (literal reproduction would duplicate ~5k LOC; same precedent as the inspector plan). The one design-heavy task (ThreadPool lock-free) names the exact SB hazard (raw-pointer `Node.Queue`) + the Miri-proof-first gate. No "TBD"/"handle edge cases". Acceptable for a port plan.

**3. Type consistency:** `ThreadPool`/`Task`/`Batch` (Task 1 → used by Task 2/5), `Stream` (Task 2 → used by Task 3/5), `net::Address` (Task 2 → Task 3/4), `Operation::{SendMsg,RecvMsg}` (Task 4), `File`/`TCP`/`UDP` exports (Tasks 3/4/5 → Task 6). Consistent.

**Sequencing for execution:** STRICTLY sequential by dependency — T1 (ThreadPool, Miri-gated) → T2 (un-gate stream/net + epoll pool, needs T1) → T3 (TCP, needs T2) → T4 (UDP, needs T2 + adds epoll msghdr) → T5 (File, needs T1+T2) → T6 (exports + suite, needs all). T3/T4 could parallelize after T2 (distinct files) but both touch lib.rs exports — sequence to avoid conflicts. The ThreadPool (T1) is the critical-path risk; its Miri gate is mandatory before T2 builds on it.
