# VAXIS Chunk 3b — the TTY BACKEND + CONCURRENCY (the impure half of the backend) — DESIGN

**Crate:** existing `crates/rustty-vaxis` (Chunks 1a value types, 1b uucode/gwidth/
unicode, 2 rendering, **3a input-parsing** — all landed). This chunk adds the
**impure half of the backend** as three new modules — `queue.rs`, `tty.rs`, `loop_.rs`
(`loop` is a Rust keyword) — wired into `lib.rs`, plus **one new leaf dep** (`libc`, for
the termios/ioctl/sigaction FFI) and two clearly-bounded seams (a `Vaxis` placeholder +
a `LoopEvent` trait) documented in §5/§6.

**Source (read-only):**
`/Users/gael/.cache/zig/p/vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS/src/`
— `queue.zig` (349 LOC, **7 `test{}`**), `tty.zig` (749 LOC, **0 test**), `Loop.zig`
(404 LOC, **1 `test{}`**).

**Scope (R1 faithful):** the thread-safe fixed-size **blocking queue** (Mutex + two
Condvars, the two-mirror ring wraparound, `push`/`pop`/`try_push`/`try_pop`/`poll`/
`drain`, all 7 tests); the **`PosixTty`** FFI (open `/dev/tty`, `makeRaw`'s EXACT
termios flag clears/sets, the SIGWINCH `sigaction` global handler + `handler_mutex` +
handlers array, `read`/`write`, `getWinsize` `TIOCGWINSZ` ioctl, `deinit` termios
restore); the **`WindowsTty`** `#[cfg(windows)]` R5 seam; the **`TestTty`** pipe/mock;
and the **`Loop`** read-thread (construct-then-spawn: `ttyRun` reads the tty, feeds the
chunk-3a `Parser`, routes via `handleEventGeneric`, pushes to the queue; `init`/`start`/
`stop`/`join`, `nextEvent`/`pollEvent`/`tryEvent`/`postEvent`/`tryPostEvent`, the
`winsizeCallback`).

---

## DISPOSITION: was **BLOCKED** (grapheme-text lifetime) — **RESOLVED** in this revision

The adversary found **one genuine blocker**: the naive port queued an event whose
`Key.text` borrowed the read thread's grapheme-cache ring (a `ttyRun` stack local that
WRAPS with no synchronization), so a main-thread pop would read bytes the read thread is
concurrently overwriting — an unsynchronized cross-thread **data race** on `cache.buf`
(UB) **and** a lifetime that cannot be named in safe Rust. This revision **resolves** it
by promoting the queued event's grapheme text to **owned** (`Key::into_owned` -> a
`Vec<u8>` copy at post time), exactly the faithful-and-sound promotion already applied to
`should_quit`->`AtomicBool` and `Paste`->`Vec<u8>`. The `LoopEvent` key constructors and
the `Queue` element type are now **owned/`'static`** (§6), and the mirihar `loopthread_*`
pair now models the cache-text handoff (§8). See §4.5/§6/§8 for the resolution; **the
block clears once the owned-text model + the extended mirihar land as specified below.**

With that resolved, the three structural forks all have an established, crate-precedented
resolution:

1. **The read-thread aliasing** (§4, the CORE ASK) — a construct-then-spawn shape
   **identical to the already-proven `renderthread_*` family** in
   `mirihar-chunk3b`. Faithful model = **raw-self `*mut Loop<T>`** (NOT an `Arc`
   split), pinned, narrow-field-only on both threads; `should_quit` promoted to
   `AtomicBool`; `thread` an interior-mutable `Cell`; **the queued `T` OWNS its grapheme
   text** (no read-thread ring borrow crosses to main — the resolved blocker, §4.5).
   **needs_mirihar = true** — a Loop-specific `loopthread_naive`(UB)/
   `loopthread_shipped`(CLEAN) pair is added, now also covering the owned-text handoff.
2. **The `Vaxis` coupling** (§5) — `Loop.vaxis: *Vaxis` + `handleEventGeneric`'s
   capability-detection body reach deep into `Vaxis` (a **later** chunk). Resolved
   by the **placeholder-leaf-doctrine**: an INHABITED `Vaxis` placeholder struct with
   `unimplemented!()` method leaves, so `handleEventGeneric` ports **live** (its full
   routing control-flow) and only the Vaxis-method leaves defer.
3. **`@hasField(Event, "…")` comptime reflection** (§6) — Zig's `Loop(comptime T)`
   duck-types the user's event union. Faithful Rust encoding = a **`LoopEvent` trait**
   with one `Option<Self>`-returning constructor per event (`Some` ≡ `@hasField`
   true, `None` ≡ the field-absent skip).

The tty FFI (`libc`) is a **leaf dep** — acyclic (§9). tty tests need `/dev/tty`
(absent in the sandbox) ⇒ `#[ignore]`-on-device; `test Loop` needs the real `Vaxis` ⇒
`#[ignore]`; the 7 queue tests are pure ⇒ **Miri-primary**. Proceed.

---

## 1. Per-file mapping (Zig → rustty module)

| Zig source | rustty module | new public items | purity |
|---|---|---|---|
| `queue.zig` | `queue.rs` | `Queue<T, const N: usize>` (blocking ring) | **PURE** (Mutex/Condvar) → Miri |
| `tty.zig` | `tty.rs` | `PosixTty`, `TestTty`, `WindowsTty`(seam), `Tty` alias, `SignalHandler`, `global_tty` | **FFI** (libc) → ASan/native |
| `Loop.zig` | `loop_.rs` | `Loop<T>`, `handle_event_generic`, `LoopEvent` trait + `KeyOwned` (§6) | **impure** (thread) → mirihar |

`lib.rs`: add `pub mod queue; pub mod tty; pub mod loop_;` (+ `pub use loop_::Loop;`
so `vaxis::Loop` matches Zig's `vaxis.Loop`), extend the CHUNK banner / PORT STATUS to
note the tty-backend + concurrency layer landed, and add the **`Vaxis` placeholder**
(§5 — either a new `pub mod vaxis` placeholder module or a `pub struct Vaxis` stub in
`lib.rs` with `unimplemented!()` leaves).

**Cargo.toml:** replace the `# … dep-free R4 leaf` comment; add a `[dependencies]` section
with `libc = "0.2"`. **`libc` is pinned PER-CRATE, not via `[workspace.dependencies]`** —
each of the ~7 crates that use it (`rustty-os`, `rustty-vt`, `rustty-xev`, …) declares its
own `libc = "0.2"` literal, so `rustty-vaxis` does the same (a plain `libc = "0.2"`, **not**
`libc = { workspace = true }`). `libc` is a **leaf** (no rustty edges) ⇒ `rustty-vaxis`
stays acyclic.

---

## 2. `queue.rs` — the blocking ring buffer (PURE, the 7-test pin)

### 2.1 Struct shape

Zig `Queue(comptime T, comptime size)` → **`Queue<T, const N: usize>`**. Zig keeps
`buf`, `read_index`, `write_index`, `mutex`, `not_full`, `not_empty` as sibling fields
all touched under the lock. Faithful Rust: put the lock-guarded mutable state in a
`Mutex<Inner>` and the two Condvars beside it.

```rust
struct Inner<T, const N: usize> {
    buf: [Option<T>; N],      // Zig `buf: [size]T = undefined`; see §2.2
    read_index: usize,        // = 0
    write_index: usize,       // = 0
}

/// Thread safe. Fixed size. Blocking push and pop.
pub struct Queue<T, const N: usize> {
    inner: Mutex<Inner<T, N>>,
    // blocks when the buffer is full
    not_full: Condvar,
    // ...or empty
    not_empty: Condvar,
}
```

`impl<T, const N: usize> Default for Queue<T, N>` builds `buf` with
`core::array::from_fn(|_| None)` (arrays impl `Default` only ≤ 32; `N` is 512 for
Loop). Preserve the `blocks when the buffer is full` / `...or empty` comments and the
`Thread safe. Fixed size. Blocking push and pop.` doc.

### 2.2 THE `buf` DECISION — `[Option<T>; N]` (safe) vs `[MaybeUninit<T>; N]` (Zig-literal)

Zig's `buf: [size]T = undefined` stores `T` by value in undefined slots, occupancy
tracked purely by the ring indices. Two faithful ports:

- **`[Option<T>; N]` (RECOMMENDED)** — fully **safe**, **zero `unsafe`**, Miri-trivially
  clean; push = `slot = Some(item)`, pop = `slot.take().unwrap()`. `T` may be non-`Copy`
  (the Loop stores `Event`-like `T` that can own a `Vec`); `Option` handles drop
  correctly. The only divergence is a redundant `None` tag (the ring indices already
  track occupancy) — behaviorally identical. **This is the best fit for a Miri-primary
  pure module** (no unsafe to audit; queue is the one module here that MUST be Miri-clean).
- `[MaybeUninit<T>; N]` (closer to Zig `undefined`) — needs `assume_init_read` on pop +
  a drop-glue loop; introduces well-scoped `unsafe`. Rejected: no fidelity gain over
  `Option` (occupancy is index-driven either way), and it adds unsafe to the ONE module
  we want unconditionally Miri-clean. PORT-NOTE the `undefined → Option::None` choice.

### 2.3 The two-mirror ring (port EXACTLY)

Indices live in `[0, 2N)` (the two-mirror trick that disambiguates full from empty):

- `mask(i)  = i % N`      (`buf` index)
- `mask2(i) = i % (2*N)`  (index-space wrap)
- `is_empty_lh = write_index == read_index`
- `is_full_lh  = mask2(write_index + N) == read_index`
- `pop_lh`: `let r = buf[mask(read)].take().unwrap(); read = mask2(read + 1); r`
- push: `buf[mask(write)] = Some(item); write = mask2(write + 1);`
- `len` (private, unused by tests, port for R1): `wrap = 2*N * usize::from(write < read); (write + wrap) - read`

`…LH` = "lock held". Preserve every doc: `Returns index modulo the length…`, `…modulo
twice the length…`, the `Used to efficiently drain the queue while the lock is
externally held` on `drain`, etc.

### 2.4 The blocking methods (Condvar `while`-loops — the load-bearing correctness)

```rust
pub fn pop(&self) -> T {                    // Zig `pop(self: *Self) T`
    let mut g = self.inner.lock().unwrap();
    while is_empty(&g) { g = self.not_empty.wait(g).unwrap(); }
    debug_assert!(!is_empty(&g));
    if is_full(&g) { self.not_full.notify_one(); }  // wake a waiting push
    pop_lh(&mut g)
}
pub fn push(&self, item: T) {               // Zig `push(self: *Self, item: T) void`
    let mut g = self.inner.lock().unwrap();
    while is_full(&g) { g = self.not_full.wait(g).unwrap(); }
    debug_assert!(!is_full(&g));
    let was_empty = is_empty(&g);
    /* write slot; bump write_index */
    if was_empty { self.not_empty.notify_one(); }   // wake a waiting pop
}
```

**`&self`, not `&mut self`** — the whole point of the internal Mutex (and load-bearing
for §4: the read thread and main thread both hold only `&Queue`). `signal()` →
`Condvar::notify_one()`; `wait(&mutex)` → `Condvar::wait(guard)` (returns the
re-acquired guard). The **`while` (not `if`) predicate re-check** is exactly what tests
5/6 assert (spurious wakeups) — keep it verbatim.

`try_push`/`try_pop` port the faithful **TOCTOU unlock-then-call**: lock, check
full/empty, **unlock**, then call `push`/`pop` (which re-locks). Zig unlocks between the
check and the blocking call — so `try_push` can still block if another thread fills the
slot first; test 2 ("Fill, wait to push") depends on `try_push` returning `false` when
full. Port exactly (drop the guard, then call the blocking method). `poll` = the
`pop`-style wait loop that returns without popping (used by `Loop.pollEvent`). `lock`/
`unlock`/`drain` (the externally-held-lock drain API, no test exercises it) — port with
a documented guard-shaped signature; PORT-NOTE that Rust RAII makes the split `lock()` +
`unlock()` awkward, so `drain` takes/returns the guard. `is_empty`/`is_full` (public,
lock-then-check) port directly.

### 2.5 The 7 tests (assertion-for-assertion, Miri-clean)

| # | Zig test | shape | Rust port notes |
|---|---|---|---|
| 1 | `Queue: simple push / pop` | single-thread | direct |
| 2 | `Fill, wait to push, pop once in another thread` | 1 bg thread; `tryPush=false` when full | `std::thread::scope` |
| 3 | `Try to pop, fill from another thread` | 1 bg writer, 5 pops | scope |
| 4 | `Fill, block, fill, block` | bg `sleepyPop` + `AtomicU8` state; **spurious signals**; asserts push blocked ≥5ms | scope + `AtomicU8` |
| 5 | `Drain, block, drain, block` | bg `sleepyPush`; spurious signals; the pop-side `while` pin | scope |
| 6 | `2 readers` | 2 reader threads, 1 writer | scope |
| 7 | `2 writers` | 2 writer threads, 1 reader | scope |

**Threading port:** Zig `Thread.spawn(cfg, fn, .{&queue})` + `t.join()` (joined before
`queue` drops) ≡ **`std::thread::scope`** (borrow `&queue` into the closures; `Queue:
Sync` via the Mutex). `atomic.Value(u8)` → `AtomicU8` (`.load(.acquire)`/`.store(_,
.release)` → `Ordering::Acquire`/`Release`). `Thread.yield()` → `std::thread::
yield_now()`; `std.Thread.sleep(10 * ns_per_ms)` → `thread::sleep(Duration::
from_millis(10))`. `std.time.milliTimestamp()` → `Instant::now()` + `.elapsed()`
(assert `> 5ms`). **The spurious-wake tests reach `q.not_full`/`q.not_empty` directly**
(`q.not_full.signal()` from the helper thread) → same-module test access to the `pub(?)`
Condvar fields → `notify_one()`. `expectEqual(1, pop)` → `assert_eq!(pop, 1)`. These 7
are the Miri contract: `cargo +nightly miri test -p rustty-vaxis` exercises the ring
wraparound + the blocking Condvar handoff.

---

## 3. `tty.rs` — the termios FFI (`PosixTty`) + the two seams

### 3.1 `PosixTty` struct + the libc surface

```rust
pub struct PosixTty {
    /// the original state of the terminal, prior to calling makeRaw
    termios: libc::termios,
    /// The file descriptor of the tty
    fd: libc::c_int,                 // Zig posix.fd_t
    /// buffered writer over `fd` (see §3.4)
    tty_writer: TtyWriter,
}
```

FFI calls (all `libc`, `unsafe`): `open`, `read`, `write`, `tcgetattr`, `tcsetattr`,
`ioctl(TIOCGWINSZ)`, `sigaction`, `close`. This is the R6 **ASan/native** surface (a
`MIRI_EXCEPTIONS.md` entry — the FFI can't run under Miri; `/dev/tty` is absent in the
sandbox ⇒ any device test is `#[ignore]`).

### 3.2 `init` + `makeRaw` — the EXACT termios flag manipulation (fidelity core)

`init(buffer)`: `open("/dev/tty", O_RDWR, 0)` (Zig `.ACCMODE = .RDWR`) → `fd`; `makeRaw(fd)`
→ saved `termios`; install the SIGWINCH handler (§3.3); build `self`; set `global_tty`.

`makeRaw(fd) -> termios`: `tcgetattr` → `state`; copy to `raw`; then the **exact** bit
clears/sets (Zig manipulates the named `posix.termios` bitfields; Rust manipulates the
`c_iflag`/`c_oflag`/`c_lflag`/`c_cflag` `tcflag_t` words with `libc` constants). Preserve
the `// see termios(3)` comment.

| Zig field | Rust op |
|---|---|
| `iflag.IGNBRK=false … IXON=false` | `raw.c_iflag &= !(IGNBRK\|BRKINT\|PARMRK\|ISTRIP\|INLCR\|IGNCR\|ICRNL\|IXON)` |
| `oflag.OPOST=false` | `raw.c_oflag &= !OPOST` |
| `lflag.ECHO/ECHONL/ICANON/ISIG/IEXTEN=false` | `raw.c_lflag &= !(ECHO\|ECHONL\|ICANON\|ISIG\|IEXTEN)` |
| `cflag.CSIZE = .CS8` | `raw.c_cflag = (raw.c_cflag & !CSIZE) \| CS8` |
| `cflag.PARENB=false` | `raw.c_cflag &= !PARENB` |
| `cc[V.MIN]=1` | `raw.c_cc[VMIN] = 1` |
| `cc[V.TIME]=0` | `raw.c_cc[VTIME] = 0` |

Then `tcsetattr(fd, TCSAFLUSH, &raw)` (Zig `.FLUSH`) and **return `state`** (the
original, for restore). PORT-NOTE: this is the `cfmakeraw`-equivalent spelled out
flag-by-flag — port the individual bits, do **not** substitute `cfmakeraw()` (it may
differ; the Zig set is the contract).

### 3.3 The SIGWINCH global handler (`handler_mutex` + handlers array)

Zig module-level statics: `handlers: [8]SignalHandler`, `handler_mutex: Mutex`,
`handler_idx: usize`, `handler_installed: bool`. Faithful Rust globals:

```rust
pub struct SignalHandler {
    pub context: *mut c_void,                       // was `*anyopaque`
    pub callback: extern "C" fn(context: *mut c_void),
}
// FFI-FIX (required to compile): a raw `*mut c_void` field makes `SignalHandler`
// neither `Send` nor `Sync`, so it CANNOT be placed in a `static Mutex<HandlerState>`
// (a static requires `Sync`; `Mutex<HandlerState>: Sync` requires `HandlerState: Send`,
// which requires `SignalHandler: Send`). Add an explicit, audited:
//     unsafe impl Send for SignalHandler {}
// (safe here: the pointer is a type-erased `*mut Loop<T>` whose pointee is pinned for
// the handler's whole registered life, and every cross-thread touch of it goes through
// the raw-self narrow-projection discipline of §4). `HandlerState` (the array + idx) is
// then `Send` automatically, so `Mutex<HandlerState>: Sync` and the static compiles.
// global signal handlers
static HANDLERS: Mutex<HandlerState> = …;   // { handlers: [Option<SignalHandler>; 8], idx: 0 }
static HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);
```

`init` installs `handle_winch` via `libc::sigaction(SIGWINCH, &act, null)`. **The
`sigaction` FFI detail (spell it out):** libc's `struct sigaction` exposes the handler as
`sa_sigaction: sighandler_t` (a `usize`-width slot), so an `extern "C" fn(c_int)` must be
put in via a fn-ptr cast — `act.sa_sigaction = handle_winch as usize` (equivalently a
`mem::transmute` to `sighandler_t`) — and **`sa_flags` MUST NOT set `SA_SIGINFO`** on
either macOS or Linux (we use the 1-arg `fn(c_int)` form, not the 3-arg `siginfo`
handler). Set `sa_mask = 0` on macOS / `sigemptyset(&mut act.sa_mask)` elsewhere (Zig's
`switch (os.tag)`), and `sa_flags = 0`. `notify_winsize(h)` locks `HANDLERS`, `if idx == 8
{ return Err(OutOfMemory) }`,
stores, bumps `idx` (max 8 handlers — preserve the doc). `handle_winch(_: c_int)`
(`extern "C"`, the signal handler): lock `HANDLERS`, iterate `0..idx`, call each
`callback(context)`. `reset_signal_handler`: if installed, reinstall `SIG_DFL`.

PORT-NOTE (async-signal-safety): locking a mutex + `ioctl`/queue-push inside a SIGWINCH
handler is **not** async-signal-safe — but it is Zig's exact design and the faithful
port keeps it (this is the impure-backend contract, ASan/native-tested, never Miri).
`SignalHandler.context` is `*mut c_void` (type-erased `*anyopaque`) — load-bearing for
§4 (the whole `Loop` is stored here and recovered by the callback). The globals make
`PosixTty` NOT `Sync` by default; the handler statics are the shared surface, guarded by
`HANDLERS`'s Mutex + the `AtomicBool`.

### 3.4 `read`/`writer`/`getWinsize`/`deinit`

- `read(&self, buf) -> Result<usize>`: `libc::read(self.fd, buf.as_mut_ptr() as *mut
  c_void, buf.len())`; errno on `-1`. Zig `posix.read`.
- `getWinsize(fd) -> Result<Winsize>`: `libc::ioctl(fd, TIOCGWINSZ, &mut ws)` with
  `libc::winsize{ ws_row, ws_col, ws_xpixel, ws_ypixel }`; on success return
  `Winsize{ rows: ws.ws_row, cols: ws.ws_col, x_pixel: ws.ws_xpixel, y_pixel:
  ws.ws_ypixel }` — **note the field rename** `row→rows`, `col→cols`, `xpixel→x_pixel`,
  `ypixel→y_pixel`. Else `Err(IoctlError)`. Preserve `Get the window size from the kernel`.
- `writer(&mut self) -> &mut TtyWriter`: the buffered writer over `fd` (§below).
- `deinit(self)`: `tcsetattr(fd, TCSAFLUSH, &self.termios)` (log-and-continue on err,
  preserve `couldn't restore terminal`); `if os != macos { close(fd) }` — preserve the
  `closing /dev/tty may block indefinitely on macos` comment.

**`tty_writer` / the `buffer` param (a documented sub-decision):** Zig `init(buffer:
[]u8)` hands a caller write buffer to `std.fs.File.Writer` (buffered). The Rust tty must
buffer writes over the **raw `fd`** WITHOUT taking ownership of it (else the writer's
`File` drop double-closes the fd `deinit` closes). RECOMMEND a small `TtyWriter { fd:
c_int, buf: Vec<u8> }` that `impl std::io::Write` via `libc::write(fd, …)` and does NOT
own/close `fd` (crate precedent: buffer-owning things → owned `Vec`; PORT-NOTE the
caller-`buffer` → owned `Vec` divergence). The alternative (a borrowed `&'buf mut [u8]`
matching Zig exactly) threads a `'buf` lifetime through `PosixTty` **and** `Loop<T>`,
complicating the raw-self model in §4 for no runtime gain in 3b (the writer's real
exercise is the Vaxis render chunk). Owned `Vec` chosen.

### 3.5 `WindowsTty` — the `#[cfg(windows)]` R5 seam

Zig `Tty = if is_test TestTty else switch(os){ .windows => WindowsTty, else => PosixTty }`.
Rust:

```rust
#[cfg(not(windows))] pub use PosixTty as Tty;   // (+ TestTty under #[cfg(test)], §3.6)
#[cfg(windows)]      pub use windows_tty::WindowsTty as Tty;
```

`WindowsTty` (the console-mode + `ReadConsoleInputW` + `INPUT_RECORD` key/mouse/resize/
focus decode — ~450 LOC) is a **`#[cfg(windows)]` R5 seam** per the task. Land it as a
`#[cfg(windows)] mod windows_tty` with the struct SHAPE (`stdin`/`stdout`/`initial_*`/
`buf`/`last_mouse_button_press`) + `unimplemented!()` method bodies (placeholder-leaf-
doctrine), NOT compiled/tested on the dev (macOS/Linux) target. DEFERRALS.md entry.
PORT-NOTE: the `winapi`/`INPUT_RECORD` FFI + the eventFromRecord decode land when a
Windows target is in scope; the non-windows build never names it. (Its Zig `read` even
references `self.fd`, a field it lacks — a latent Zig bug; the seam sidesteps it.)

### 3.6 `TestTty` — the pipe/mock (enables the Loop test path)

Zig `TestTty` (compiled under `builtin.is_test`): a `posix.pipe()` pair + an allocating
writer + `getWinsize` returning a **fixed `{rows:40, cols:80, x_pixel:320, y_pixel:640}`**.
Port under `#[cfg(test)]` as `TestTty { fd, pipe_read, pipe_write, tty_writer:
Vec<u8> }`: `init` → `libc::pipe()` (the read end is `fd`); `read` → `libc::read(fd, …)`;
`writer` → the `Vec` sink; `get_winsize(_) -> Winsize{40,80,320,640}` (preserve `40 * 8`
/ `40 * 8 * 2`); `deinit` closes both pipe ends; `next_event` / `reset_signal_handler`
are the API-compat stubs (`next_event` → the Windows-API `SkipZigTest` equivalent =
`unimplemented!()`/ignored). Under `#[cfg(test)]`, `type Tty = TestTty` so the Loop test
+ any device-free path uses the pipe. `libc::pipe` works in the sandbox (unlike
`/dev/tty`), so the pipe-backed Loop scaffolding is exercisable where a real tty is not.

---

## 4. `loop_.rs` — the read thread (THE ALIASING MODEL — the CORE ASK)

### 4.1 Struct

```rust
pub struct Loop<T> {                       // Zig `pub fn Loop(comptime T) type`
    tty: *mut Tty,                          // Zig `tty: *Tty`
    vaxis: *mut Vaxis,                      // Zig `vaxis: *Vaxis` (the §5 placeholder)
    queue: Queue<T, 512>,                  // internally-synced (§2)
    thread: Cell<Option<JoinHandle<()>>>,  // Zig `thread: ?std.Thread` — see §4.4
    should_quit: AtomicBool,               // Zig `should_quit: bool` — see §4.3
}
```

**`T: LoopEvent + Send + 'static`** (§6) — `Send + 'static` is load-bearing for
soundness: the queued `T` is produced on the read thread, consumed on main, and (per the
resolved blocker) must **own** all its bytes, so it may not borrow anything with a
non-`'static` lifetime (no read-thread stack/ring borrow). **`thread` is
`Cell<Option<JoinHandle>>`, not a bare `Option`** — see §4.4 (SOUNDNESS FIX): `start`/
`stop` mutate it through `&self` while the read thread may hold `*mut Loop`, so it needs
interior mutability; writing it through a `*mut` derived from `&self` (a plain field)
would be UB under SB/TB. `Cell` (an `UnsafeCell`) is the sanctioned interior-mutability
channel; it is main-thread-only and the read thread never touches it. (`Cell` is `!Sync`,
which is fine: no `&Loop` ever crosses the thread boundary — the read thread holds a raw
`*mut Loop`, §4.2 — and every mutable field is behind an `UnsafeCell`: `AtomicBool`,
`Queue`'s `Mutex`, and this `Cell`.)

Preserve the `Initialize the event loop. This is an intrusive init
so that we have a stable pointer to register signal callbacks with posix TTYs` doc — it
is the direct justification for the **raw-self, pinned** model below.

### 4.2 THE ALIASING MODEL — raw-self `*mut Loop<T>` (NOT Arc), pinned

The read thread (`ttyRun`) captures the Loop and **holds it for its whole life** while
the main thread concurrently drives `stop`/`nextEvent`. This is **precisely the
construct-then-spawn + userdata-callback-reentry shape already PROVEN** by the
`renderthread_naive`(UB)/`renderthread_shipped`(CLEAN, SB+TB) pair in
`mirihar-chunk3b`. Two candidate models:

- **raw-self `*mut Loop<T>` (CHOSEN, faithful).** The read thread captures `self_ptr:
  *mut Loop<T>` (via a `Send` newtype, e.g. `struct SendLoop<T>(*mut Loop<T>); unsafe
  impl Send`), the main thread keeps the pinned owned Loop. **Why raw-self, not an
  `Arc<Shared>` split:** Zig's `Tty.notifyWinsize` stores the **WHOLE Loop** as a
  **type-erased `*anyopaque` context** in the global handlers array, and the SIGWINCH
  handler re-enters `winsizeCallback(ptr)` through it (`@ptrCast(@alignCast(ptr))` back
  to `*Self`). That is a **userdata round-trip** (`*mut c_void` in/out of a global) an
  `Arc` cannot model — the handler stores an opaque pointer, not a refcount. It is the
  same container_of/userdata pattern the RENDERTHREAD/DISPLAYCB candidates certified.
  The Loop MUST be **pinned** (stable address — Box or the test's stack local held for
  the thread's life), which is exactly what the `intrusive init … stable pointer` doc
  demands.
- `Arc<Shared>` split (REJECTED) — would decouple `queue`+`should_quit` behind an Arc,
  but diverges from the intrusive single-struct + the type-erased winsize-context
  contract; less faithful and can't carry the whole-`Self` callback userdata.

**The discipline (what keeps it SOUND under SB+TB):**

1. **Read thread (`ttyRun`)** holds `self_ptr: *mut Loop<T>` for its whole life and does
   ONLY **narrow field projections** — `(*self_ptr).should_quit` (atomic load),
   `(*self_ptr).tty` (raw → libc `read`/`getWinsize`), `(*self_ptr).vaxis` (raw →
   `handle_event_generic`), `&(*self_ptr).queue` (shared → internally-synced `push`).
   It **NEVER binds a whole `&mut *self_ptr`** (the naive shape's UB).
2. **Winsize callback** fires with `context: *mut c_void = self_ptr` (type-erased),
   casts back, and likewise projects narrow fields only — `&(*self_ptr).vaxis` (read
   `state.in_band_resize`), `(*self_ptr).tty.fd` → `getWinsize`, `&(*self_ptr).queue` →
   `postEvent`. Never a whole `&mut`/`&Loop`. (SIGWINCH ⇒ arbitrary thread — this
   foreign access through the raw ctx is exactly what the mirihar callbacks model.)
3. **Main thread**, post-spawn, NEVER takes a whole `&mut Loop` while the thread is
   live. `nextEvent`/`pollEvent`/`tryEvent`/`postEvent`/`tryPostEvent` take **`&self`**
   and go through the queue's interior mutability (Mutex). `stop` writes `should_quit`
   (`AtomicBool::store`, `&self`), triggers a read (`vaxis.deviceStatusReport(tty.
   writer())` via narrow projections), **then** `join`s — and the `thread:
   Option<JoinHandle>` is touched by main ONLY, via a narrow field access, AFTER the
   store. No whole-`&mut Loop` reborrow ever spans a live read thread.
4. **`queue` is the internally-synchronized channel** (both threads hold only `&Queue`);
   the winsize handlers array is the `Mutex`-guarded static (§3.3).

Soundness: this is structurally `renderthread_shipped` (CLEAN under SB **and** TB in the
recorded mirihar run). It is **non-trivial** — a whole-`&mut Loop` on either thread pops
the other's held raw tag (SB: tag not in stack; TB: protected/`Disabled` violation),
exactly as `renderthread_naive` demonstrates.

### 4.3 `should_quit: AtomicBool` (faithful-and-sound)

Zig `should_quit: bool` is a plain bool **written by main (`stop`) and read by the child
(`ttyRun` loop)** with no synchronization — a latent data race that "works" on the
target arch (and `stop` also pokes the tty to force the blocking `read` to return so the
child re-checks). Rust makes it explicit + sound: **`AtomicBool`** (`store(true,
Relaxed)` in `stop`; `load(Relaxed)` in the read loop). PORT-NOTE (same class as the
parser's saturating-sub / the AtomicBool caps in `handleEventGeneric`). This keeps rule
3's `stop` a pure `&self` op (atomic store, no `&mut`).

### 4.4 The lifecycle methods

- `init(&self)`: non-windows, non-test → build `SignalHandler{ context: self as *mut
  c_void, callback: Self::winsize_callback_trampoline }`, `Tty::notify_winsize(h)?`.
  windows/test → no-op. (Preserve the intrusive-init doc.)
- `start(&self)`: `if self.thread.take_is_some_then_restore() { return }` — concretely,
  peek `self.thread` (the `Cell`) for `Some` and early-return; else spawn
  `ttyRun(SendLoop(self_ptr), paste_allocator)` (§4.2) and `self.thread.set(Some(handle))`.
  PORT-NOTE `system_clipboard_allocator` → dropped (paste owns a `Vec`, §5/3a precedent).
  **SOUNDNESS FIX (adversary):** `start`/`stop` are `&self` while a live read thread may
  hold `*mut Loop`, so `thread` MUST be interior-mutable — it is the `Cell<Option<
  JoinHandle>>` of §4.1, NOT a bare field. Mutating a plain `Option` field through a `*mut`
  derived from `&self` is UB under SB/TB; `Cell::set`/`Cell::take` (via `UnsafeCell`) is
  the sanctioned channel. `thread` is **main-thread-only** — the read thread never reads or
  writes it — and no whole `&mut Loop` is ever formed (rule 3).
- `stop(&self)`: `if self.thread` is `None` return; `should_quit.store(true)`; `vaxis.
  deviceStatusReport(tty.writer())` "// trigger a read" (ignore err); `self.thread.take()`
  the handle, `join()`, leave the `Cell` `None`, `should_quit.store(false)`. Preserve `If
  we don't have a thread…` / `trigger a read`.
- `nextEvent`→`queue.pop()`, `pollEvent`→`queue.poll()`, `tryEvent`→`queue.try_pop()`,
  `postEvent`→`queue.push()`, `tryPostEvent`→`queue.try_push()` (all `&self`, preserve
  each doc: `returns the next available event, blocking…`, `poll + drain architecture`,
  `Non-blocking`, `Will block if there is not capacity`).
- `winsize_callback(self_ptr)`: cast the `*mut c_void` back; `if vaxis.state.
  in_band_resize { return }` (preserve `We will be receiving winsize updates in-band`);
  `getWinsize(tty.fd)` (return on err); if `T` has a winsize variant (`LoopEvent`,
  §6) `postEvent`. An `extern "C"` trampoline adapts the signal-handler ABI.

### 4.5 `ttyRun` (the read loop)

`fn tty_run(loop_ptr: SendLoop<T>, paste_allocator)`: `if is_test return` (preserve
`Return early if we're in test mode to avoid infinite loops`). Non-windows arm:
`getWinsize` → initial winsize `postEvent`; a `Parser`; `buf: [u8; 1024]`,
`read_start = 0`; the **`read_loop`** — `n = tty.read(&mut buf[read_start..])`; inner
`while seq_start < n` calls `parser.parse(&buf[seq_start..n])` (chunk-3a), handles the
`result.n == 0` **overlapping-copy-to-front + `continue :read_loop`** (preserve `we don't
use memcpy because this could be overlapping, and it's also rare`), else advances
`seq_start += result.n` and routes `result.event` via `handle_event_generic`. All `self`
touches are narrow `(*loop_ptr.0).<field>` projections (rule 1). Windows arm = the
`#[cfg(windows)]`/seam path (`tty.nextEvent`).

**RESOLVED BLOCKER — the grapheme text MUST be OWNED (no `GraphemeCache` ring on the
Loop path).** Zig's `handleEventGeneric` does `mut_key.text = cache.put(text)`, where
`cache` is a `ttyRun` **stack-local** `GraphemeCache` and `GraphemeCache.put` returns a
`[]u8` **into an 8 KiB ring that WRAPS** (`grapheme_cache.rs`: `idx = 0` on overflow).
The queued event (`LoopEvent::key_press(mut_key)`) would therefore hold a `Key` whose
`text` borrows the **read thread's stack ring**, popped by the **main** thread while the
read thread keeps overwriting the ring with **no synchronization** — a genuine
unsynchronized cross-thread **data race** (UB) and a lifetime unnameable in safe Rust.
Zig "gets away with it" (8 KiB is usually large enough that a consumer drains before the
ring wraps back over live bytes), but it is a latent bug; the faithful-**and**-sound port
does NOT reproduce it. **Resolution:** `ttyRun` does **not** construct a `GraphemeCache`
and does **not** hand a ring slice across the queue. Instead `handle_event_generic`
promotes the transient `Key.text` to an **owned `Vec<u8>`** via `Key::into_owned` (§6)
**before** posting — the same owned-promotion already used for `Paste([]const u8)` →
`Vec<u8>`. The posted `T` owns every byte it carries; nothing crosses the queue by
reference. The Zig `// initialize a grapheme cache` line and the per-key ring reuse are
replaced by this per-key owned copy (PORT-NOTE: trades one small `Vec` allocation per
key-with-text for soundness — the read-thread-local ring cannot be shared across the
thread boundary soundly; the `GraphemeCache` module itself stays ported, just unused by
the Loop read path). The `// HACK: yuck. there has to be a better way` comment is
preserved over the `into_owned` copy — Rust's "better way" is exactly owning the bytes.

⚠ **Borrow note (3a interplay):** `parser.parse<'a>(&'a mut self, &'a [u8]) ->
ParseResult<'a>` (chunk 3a §2.4) returns an event borrowing `buf` **or** `parser.buf`;
`handle_event_generic` must consume the event **before** the next `parse` — the 3a
lifetime already enforces this (the result keeps `parser` borrowed until dropped). The
`Key::into_owned` copy severs that borrow at post time, so the queued `T` is fully owned
and decoupled from `buf`/`parser.buf` (and from any read-thread stack).

---

## 5. The `Vaxis` coupling — the placeholder-leaf seam (KEY DECISION)

`Loop.vaxis: *Vaxis` and **`handleEventGeneric`** (the ~200-LOC event router in
`Loop.zig`) reach deep into `Vaxis` — a **later** chunk (`lib.rs` PORT STATUS:
"tty/loop/vaxis are later chunks"). `handleEventGeneric` reads/writes `vx.caps.{kitty_
keyboard, kitty_graphics, rgb, sgr_pixels, unicode, color_scheme_updates, multi_cursor,
explicit_width, scaled_text}`, `vx.state.in_band_resize`, `vx.screen.width_method`,
`vx.query_futex` (`Futex.wake`), `vx.queries_done` (atomic), and calls `vx.
translateMouse(mouse)` + (in `stop`) `vx.deviceStatusReport(writer)`; `start` reads
`vx.opts.system_clipboard_allocator`.

**DECISION — port `handleEventGeneric` LIVE against an INHABITED `Vaxis` placeholder**
(placeholder-leaf-doctrine), NOT a single deferred seam:

- Introduce a **`Vaxis` placeholder** (new `pub struct Vaxis` — a `vaxis.rs` module or a
  `lib.rs` stub) with the fields `handleEventGeneric` names (`caps`, `state`, `screen`,
  `query_futex`, `queries_done`, `opts`) as inhabited sub-structs, and the methods
  (`translate_mouse`, `device_status_report`) as **`unimplemented!()` leaves**. The
  capability-flag WRITES (`caps.kitty_keyboard = true`, `queries_done.store(true)`) are
  live (plain field/atomic writes on the placeholder); only the two method-call leaves
  + the futex wake defer.
- Then `handle_event_generic` ports **live** — its full match-on-event control-flow, the
  F3 width/scaled-query detection, the `@hasField` gating (→ `LoopEvent`, §6), the
  postEvent routing, the `winsize` in-band-resize + `Tty::reset_signal_handler` branch,
  the paste-free branch. This preserves R1 line/comment coverage of the ROUTING logic
  (the actual port), deferring only genuine Vaxis leaves.

**Rejected alternative:** defer `handleEventGeneric` wholesale as one `unimplemented!()`
seam (Vaxis-integration belongs to the Vaxis chunk). Lighter, but loses R1 coverage of
~200 LOC of routing/comments that IS this chunk's port and is reviewable now. The
placeholder is later **filled** by the Vaxis chunk (intrusive-ptr-and-placeholder-fill
memory: filling must preserve the trait/field surface; `cargo build --workspace` is the
backstop).

Preserve the `Use return on the self.postEvent's so it can either return error union or
void` comment, the `explicit width capability detected`/`scaled text…`/`kitty keyboard…`/
`… HACK: yuck. there has to be a better way` comments, and the two `builtin.os.tag`
windows/else arms (the else arm is the live one; the windows arm is a `#[cfg(windows)]`/
seam mirror).

---

## 6. `@hasField(Event, "…")` → the `LoopEvent` trait (KEY DECISION)

Zig `Loop(comptime T)` duck-types the user's event union: `handleEventGeneric` does `if
(@hasField(Event, "winsize")) self.postEvent(.{ .winsize = ws })` — post ONLY the
variants the user's `T` actually declares (the test's `T` has `key_press`/`winsize`/
`focus_in`/`foo`, so `mouse`/`paste`/… are silently skipped). Rust has no `@hasField`
reflection and can't construct `T::Winsize` generically. Faithful encoding:

```rust
pub trait LoopEvent: Sized + Send + 'static {
    fn key_press(k: KeyOwned) -> Option<Self> { None }  // None ≡ @hasField == false (skip)
    fn key_release(k: KeyOwned) -> Option<Self> { None }
    fn mouse(m: Mouse) -> Option<Self> { None }
    fn winsize(ws: Winsize) -> Option<Self> { None }
    fn focus_in() -> Option<Self> { None }
    fn focus_out() -> Option<Self> { None }
    fn mouse_leave() -> Option<Self> { None }
    fn paste_start() -> Option<Self> { None }
    fn paste_end() -> Option<Self> { None }
    fn paste(b: Vec<u8>) -> Option<Self> { None }
    fn color_report(r: cell::Report) -> Option<Self> { None }
    fn color_scheme(s: cell::Scheme) -> Option<Self> { None }
    // (no cap_* constructors: caps set vx.caps, never postEvent'd)
}
```

**OWNED-KEY RESOLUTION (the blocker, at the type level).** The chunk-3a `Key<'a>` carries
`text: Option<&'a [u8]>` — a **borrowed** slice into the parser/read buffer (and, in Zig,
the grapheme ring). A queued `T` must not carry that borrow (the §4.5 data race /
unnameable lifetime), so the trait's key constructors take an **owned `KeyOwned`**, not
`Key<'a>`. `KeyOwned` mirrors `Key` field-for-field with **`text: Option<Vec<u8>>`** (all
other `Key` fields — `codepoint`, `shifted_codepoint`, `base_layout_codepoint`, `mods` —
are `Copy`), plus a `Key::into_owned(&self) -> KeyOwned` helper that copies `text` into a
fresh `Vec<u8>`. `handle_event_generic` calls `T::key_press(key.into_owned())` (dropping
Zig's `cache: *GraphemeCache` parameter and its `cache.put`). The supertrait bound
**`Send + 'static`** on `LoopEvent` is what statically forbids a `T` that borrows the read
thread — a borrowed `Key<'a>` in a variant would fail `'static`. The user's event type
therefore stores the owned key, e.g. the Loop test's `Event::KeyPress(KeyOwned)`.

> Rejected alternative: make `Key<'a>.text` a `Cow<'a, [u8]>` (single type, `.into_owned()`
> at post time). Cleaner in principle but it **ripples into chunk 3a** (already landed) and
> every 3a `Key` test (`text: Some(b"a")` etc.). `KeyOwned` is a 3b-local addition that
> leaves 3a's borrowed `Key<'a>` and its tests untouched — chosen for that containment.

A `T` implementing `LoopEvent` overrides only the constructors for the variants it has;
the default `None` ≡ the `@hasField`-false skip. `postEvent(.{ .winsize = ws })` →
`if let Some(ev) = T::winsize(ws) { self.post_event(ev) }`. The **`.paste` else-branch**
(free the text if the event type lacks `paste`) → `T::paste(v)` returning `None` drops
the `Vec` (Rust's owned-drop replaces Zig's `paste_allocator.?.free(text)` — no explicit
free). PORT-NOTE prominently: this trait is the faithful Rust image of Zig's comptime
field-presence optionality; `Some`/`None` is `@hasField` true/false. The Loop test's `T`
(`{key_press, winsize, focus_in, foo}`) impls only those three constructors.

---

## 7. Test strategy (per the task)

- **queue (7 tests) — Miri-primary.** Pure `std::sync` + `thread::scope`; `cargo
  +nightly miri test -p rustty-vaxis` exercises the ring wraparound + the blocking
  Condvar handoff + the two spurious-wake `while`-loop pins. No `unsafe` (§2.2 Option
  buf). These are the fidelity contract.
- **tty (0 tests in Zig) — `#[ignore]`-on-device.** No Zig `test{}` to port. Any smoke
  test (`makeRaw` round-trip, `getWinsize`) needs `/dev/tty` (absent in the sandbox) ⇒
  `#[ignore]` (run on a real device). The FFI is **Miri-excluded** ⇒ a `MIRI_
  EXCEPTIONS.md` entry (the `libc` termios/ioctl/sigaction surface); R6 gate = ASan/
  native. `TestTty` (pipe-backed) covers the device-free path.
- **Loop (1 test: `test Loop`) — `#[ignore]`.** The Zig test needs the real `Vaxis`
  (`vaxis.init`, `enterAltScreen`, `queryTerminal`, `deviceStatusReport` — §5
  placeholder `unimplemented!()` leaves) and a live tty. Port it as `#[ignore]` (unblocks
  when the Vaxis chunk fills the placeholder). Its ttyRun returns early in test mode
  (`if is_test return`), so it only asserts the init/start/stop/query lifecycle compiles
  + runs against `TestTty`. NOTE the test still counts toward `check_test_counts`
  (it's a `#[test]`, just `#[ignore]`d).
- **Loop aliasing — mirihar (needs_mirihar = true).** Add `loopthread_naive`(UB)/
  `loopthread_shipped`(CLEAN) to `.port/phase-2/mirihar-chunk3b` (§8).

---

## 8. mirihar-chunk3b — the `loopthread_*` candidates (needs_mirihar = TRUE)

The `renderthread_*` pair already proves the general construct-then-spawn + userdata-
callback-reentry family CLEAN/UB. The Loop has **distinct field mechanics** worth a
dedicated pin: the produced-to channel is the **internally-synced `Queue` (a Mutex)**
(not renderthread's plain-field mailbox), `should_quit` is an **`AtomicBool`** (not a
plain field), the `thread` handle is a **`Cell` mutated through `&self`** by main while
the child holds `*mut Loop` (§4.4), and the re-entrant callback is the **type-erased
SIGWINCH winsize callback** (`*mut c_void` context). Add (selected by argv, per the
harness convention):

| candidate | models | expect SB | expect TB |
|---|---|---|---|
| `loopthread_naive` | (a) child `ttyRun` binds `let l = &mut *self_ptr` (whole) held across the read loop; the winsize callback reborrows `&mut *self_ptr` (sibling) → pops `l`; next `l.field` = use-after-pop; main `stop`/`nextEvent` also reborrow whole `&mut Loop`; **and (b) the child posts an item that BORROWS a read-thread-local ring buffer** (models `cache.put`), which main reads while the child overwrites the ring → the §4.5 cross-thread data race. | **UB** | **UB** |
| `loopthread_shipped` | child + winsize-cb use raw `self_ptr` with NARROW field access only (`(*sp).should_quit` atomic load, `&(*sp).queue` push, `(*sp).tty`/`(*sp).vaxis` narrow); main = `&self` queue ops + `AtomicBool` store + a narrow `Cell`-based `thread` take/join, never `&mut Loop`; **and the posted item OWNS its bytes** (a `Vec<u8>` copied at post time, modelling `Key::into_owned`) — nothing crosses the queue by reference | **CLEAN** | **CLEAN** |

The model mirrors `renderthread_shipped` but substitutes: a `Mutex`-backed mini-queue
for the mailbox (two threads shared-borrowing the `Sync` queue is fine), an `AtomicBool`
should_quit produced by main + read by child (happens-before via the queue Mutex + the
atomic — no data race to mask the tag verdict), a `Cell<Option<…>>` `thread` slot mutated
by main through `&self` (interior mutability; never a `*mut`-through-`&self` plain-field
write), a type-erased `winsize_cb(ctx: *mut c_void)` recovering `self_ptr` (the SIGWINCH
userdata), **and the queued-element ownership handoff**: `loopthread_shipped`'s posted
item is an OWNED `Vec<u8>` (the read thread keeps mutating a local ring AFTER posting, and
main never observes the race because it holds its own copy), whereas `loopthread_naive`'s
item borrows that ring (the §4.5 blocker, reproduced so Miri flags the race and proves the
owned copy is load-bearing, not decorative). A deterministic mpsc handshake parks the read
thread INSIDE its loop while the winsize callback fires (so the child's borrow context is
live across the callback's foreign access) and lets main pop-then-let-the-child-overwrite
deterministically (so the race window is hit every run). **Pass criterion (record in
README.md):** `loopthread_shipped` CLEAN under BOTH SB and TB; `loopthread_naive` UB under
both (both the whole-`&mut` tag violation AND the borrowed-ring data race) — proving the
raw-self narrow-projection split **and** the owned-text promotion are load-bearing for
Loop's exact field set, not merely inherited from renderthread. Run:

```sh
for c in loopthread_naive loopthread_shipped; do
  echo "== $c =="
  cargo +nightly miri run --quiet -- "$c"                          # Stacked Borrows
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c" # Tree Borrows
done
```

If `loopthread_shipped` is not green under both models, the raw-self discipline in §4 is
wrong and must not ship.

---

## 9. Deps / acyclic (R4) + gates

- **New dep: `libc = "0.2"`** (matching the workspace's existing pin in `rustty-os`/
  `rustty-vt`/`rustty-xev`/…). `libc` is a **leaf** (no rustty edges) ⇒ `rustty-vaxis`
  stays acyclic; `check_acyclic.sh` trivially green. **Update the Cargo.toml `dep-free
  R4 leaf` comment** (it's now `libc`-only, still acyclic). No other new crate deps
  (queue is pure std; Loop consumes intra-crate `parser`/`event`/`grapheme_cache`/`key`/
  `mouse`/`Winsize` + the §5 `Vaxis` placeholder).
- **R1** — every line/comment/`test{}` faithful: the two-mirror ring math, the 7 tests
  (incl. the two spurious-wake `while`-pins + the ≥5ms timing assert), the EXACT
  `makeRaw` flag clears/sets + `// see termios(3)`, the SIGWINCH handler + the 8-handler
  cap + `closing /dev/tty may block indefinitely on macos`, `getWinsize`'s field rename,
  the intrusive-init doc, `handleEventGeneric`'s routing + all its capability-detect
  comments, the ttyRun overlapping-copy note.
- **Byte-slices** — `Key.text` (owned into `KeyOwned.text: Option<Vec<u8>>` via
  `Key::into_owned`, §6 — NOT the shared ring), the `Paste(Vec<u8>)`, the read `buf: [u8;
  1024]` → `&[u8]`/`Vec<u8>`, **never lossy `String`**.
- **R5** — the `WindowsTty` `#[cfg(windows)]` seam (§3.5); the `Vaxis` placeholder's two
  method leaves + futex wake (§5); `TestTty.next_event`. All DEFERRALS.md entries.
- **R6** — queue → **Miri** (primary, no unsafe); tty FFI → **ASan/native** +
  MIRI_EXCEPTIONS entry; Loop read-thread aliasing → **mirihar** `loopthread_*` (§8).
- **`check_test_counts.py`** — `queue.zig`/`tty.zig`/`Loop.zig` **append to the existing
  `"vaxis"` MAP zig-list** (line ~198; the rust glob `{RSVAXIS}/*.rs` auto-picks the new
  `queue.rs` 7 tests + the `loop_.rs` 1 `#[ignore]` test). Zig total for the added files
  = 7 + 0 + 1 = 8; rust ≥ zig holds (the 7 queue tests + the ignored Loop test are all
  `#[test]`s the glob counts).
- **`check_comment_coverage.py --crate rustty-vaxis`** — preserve the distinctive
  (≥5-token) comments enumerated above (queue's `Thread safe. Fixed size. Blocking…`,
  `blocks when the buffer is full`, the ring `modulo twice the length` docs; tty's `see
  termios(3)`, `closing /dev/tty may block indefinitely on macos`, `global tty instance,
  used in case of a panic`, `A maximum of 8 handlers`; Loop's `intrusive init … stable
  pointer`, `trigger a read`, `Return early if we're in test mode`, the overlapping-copy
  note, `HACK: yuck`, the capability-detected lines).
- **Gates:** `cargo build --workspace` · `cargo test -p rustty-vaxis` · `cargo clippy -p
  rustty-vaxis --all-targets -- -D warnings` · `cargo fmt --check` ·
  `bash .port/tools/gates/check_acyclic.sh` · `python3 .port/tools/gates/
  check_test_counts.py` · `python3 .port/tools/gates/check_comment_coverage.py --crate
  rustty-vaxis` · `cargo +nightly miri test -p rustty-vaxis` (queue) · the
  `mirihar-chunk3b` `loopthread_*` SB+TB run (§8).

---

## 10. Fidelity quirks to preserve (port EXACTLY)

1. **`loop` is a Rust keyword** → module `loop_.rs` (`pub use loop_::Loop`).
2. **Queue `buf` = `[Option<T>; N]`** (safe, Miri-clean; §2.2) — `undefined → None`.
3. **The two-mirror ring** (indices in `[0, 2N)`, `mask`/`mask2`) — the full/empty
   disambiguator; port the math exactly incl. the unused `len`.
4. **`while` (not `if`) in `push`/`pop`** — the spurious-wake correctness (tests 4/5).
5. **`try_push`/`try_pop` TOCTOU** — unlock between check and the blocking call (test 2
   depends on `try_push=false` when full).
6. **`makeRaw` spelled flag-by-flag** — do NOT substitute `cfmakeraw()`; the Zig bit set
   is the contract. `TCSAFLUSH` (Zig `.FLUSH`), `VMIN=1`/`VTIME=0`, `CS8`.
7. **SIGWINCH handler locks a mutex + ioctls + pushes a queue** — not async-signal-safe,
   but Zig's exact design; faithful port keeps it (ASan/native, never Miri).
8. **`getWinsize` field rename** `row/col/xpixel/ypixel → rows/cols/x_pixel/y_pixel`.
9. **`deinit` skips `close(fd)` on macOS** (`closing /dev/tty may block indefinitely`).
10. **`should_quit: AtomicBool`** (faithful-and-sound; Zig plain-bool cross-thread race).
11. **raw-self `*mut Loop<T>`, pinned, narrow-projection-only both threads** (§4) — NOT
    an Arc split (the type-erased winsize userdata forbids it). Whole-`&mut Loop` on
    either thread = UB (mirihar `loopthread_naive`).
12. **`LoopEvent` trait** = Zig `@hasField(Event,"…")` (`Option<Self>`: `Some`≡present,
    `None`≡skip); the `.paste` `None`-drop replaces `alloc.free(text)`. Supertrait
    `Send + 'static`; key constructors take an **owned `KeyOwned`** (§6).
12b. **Owned grapheme text (RESOLVED BLOCKER)** — the queued event OWNS its bytes:
    `handle_event_generic` calls `Key::into_owned` (`text` → `Vec<u8>`) instead of Zig's
    `cache.put` (a shared read-thread-local 8 KiB ring that wraps). `ttyRun` constructs
    NO `GraphemeCache`; nothing crosses the queue by reference (§4.5). This eliminates the
    unsynchronized cross-thread race on the ring and the unnameable lifetime. `thread`
    is a `Cell<Option<JoinHandle>>` (interior-mutable through `&self`, §4.1/§4.4).
13. **`Vaxis` inhabited placeholder** (doctrine) — `handleEventGeneric` ports live; only
    `translate_mouse`/`device_status_report`/futex-wake are `unimplemented!()` leaves.
14. **`WindowsTty` `#[cfg(windows)]` R5 seam** — struct shape + `unimplemented!()`
    bodies; the non-windows build never names it (sidesteps its latent `self.fd` bug).
15. **`TestTty` pipe/fixed-40x80** — the device-free path; `libc::pipe` works in the
    sandbox where `/dev/tty` does not.
16. **`log.*` dropped**; **`system_clipboard_allocator` dropped** (paste owns a `Vec`).

---

## 11. Bottom line

Chunk 3b is **READY** (the one genuine adversary blocker is resolved in this revision —
see the top DISPOSITION and §4.5/§6/§8). Port `queue.zig`→`queue.rs` (pure `Mutex`+`Condvar` two-mirror
ring, `[Option<T>; N]` buf, all **7 tests** Miri-clean incl. the spurious-wake pins),
`tty.zig`→`tty.rs` (the `PosixTty` `libc` termios FFI — the EXACT `makeRaw` flag
manipulation, the SIGWINCH `sigaction` global handler, `getWinsize` ioctl, `deinit`
restore; the `WindowsTty` `#[cfg(windows)]` R5 seam; the `TestTty` pipe/mock), and
`Loop.zig`→`loop_.rs` (the construct-then-spawn read thread — **raw-self `*mut Loop<T>`,
pinned, narrow-projection-only both threads**, `should_quit: AtomicBool`,
`thread: Cell<Option<JoinHandle>>` (interior-mutable through `&self`), the type-erased
winsize callback; **the queued `T` OWNS its grapheme text via `Key::into_owned` — no
`GraphemeCache` ring crosses the queue** (the resolved blocker); `handleEventGeneric` live
against the `Vaxis` placeholder; the `LoopEvent` trait (`Send + 'static`, owned `KeyOwned`
keys) for `@hasField`). One new **leaf** dep (`libc = "0.2"`, pinned per-crate, acyclic).
**needs_mirihar = true** — add `loopthread_naive`(UB)/`loopthread_shipped`(CLEAN, SB+TB)
to `mirihar-chunk3b`, now covering **both** the whole-`&mut` tag split **and** the
owned-vs-borrowed queued-text handoff, confirming the `renderthread` discipline carries to
Loop's exact field set (Queue-Mutex + AtomicBool + `Cell` thread + type-erased winsize
userdata + owned-text). Queue → Miri; tty FFI → ASan/native + MIRI_EXCEPTIONS; Loop
aliasing → mirihar. tty tests `#[ignore]`-on-device, `test Loop` `#[ignore]` pending
Vaxis. **Adversary's one genuine blocker (borrowed grapheme text) is RESOLVED in this
revision** (owned-text promotion + extended mirihar); the seven non-blocking fixes
(SignalHandler `unsafe impl Send`, `thread` `Cell`, sigaction fn-ptr/no-`SA_SIGINFO`,
per-crate `libc` pin, and the three confirmed-faithful items) are folded in.
