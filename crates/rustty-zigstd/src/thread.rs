//! PORTS: Zig stdlib `std.Thread.Mutex` AS THE PORT USES IT — the GUARD-LESS
//! manual `lock()`/`tryLock()`/`unlock()` mutex — and `std.Thread.Semaphore`,
//! the counting semaphore `generic.zig`'s `SwapChain` uses for triple-buffer
//! GPU-frame backpressure (`frame_sema: std.Thread.Semaphore = .{ .permits =
//! buf_count }`).
//!
//! Ghostty's core tier is riddled with manual lock/unlock patterns that Rust's
//! guard-based `std::sync::Mutex` cannot express from the borrowing frame
//! (CORE_BLUEPRINT §3): `Surface.imePoint` locks and unlocks without a defer;
//! `StreamHandler.surfaceMessageWriter` / `termio.Mailbox.send` UNLOCK a
//! caller-held mutex, block on a queue push, and relock (`m.unlock(); defer
//! m.lock();`); `mouseRefreshLinks` is documented "must be called with the
//! renderer mutex held" across function boundaries. The mutex-model decision
//! (made ONCE, in core chunk C0) is this Zig-shaped mutex: no RAII guard, no
//! poisoning (Zig has no unwind-poisoning concept — a Zig panic aborts), plain
//! `lock`/`try_lock`/`unlock` calls that pair verbatim with the reference
//! lines. Renderer `State.mutex` is `*mut thread::Mutex` of this type.
//!
//! Built on `std::sync::Mutex<bool>` + `Condvar` (zero new deps): the inner
//! std mutex guards only the `locked` flag and is held only for the flag
//! flip/wait, never across user code — so the OUTER (Zig-visible) lock can be
//! released by ANY thread/frame, exactly like Zig's futex-based
//! `std.Thread.Mutex` (which has no owner tracking in release mode).
//!
//! Like the rest of rustty-zigstd this is ADDITIVE INFRASTRUCTURE GLUE (R1
//! allows necessary additions): `std.Thread.Mutex` is Zig stdlib, not ghostty
//! src/, so it is NOT in the parity test contract; the faithfulness target is
//! the exact observable semantics of the Zig API as ghostty uses it.

use std::sync::{Condvar, Mutex as StdMutex, MutexGuard};

/// `std.Thread.SpawnConfig.default_stack_size` (Zig 0.16 `std/Thread.zig`:
/// `16 * 1024 * 1024`): the stack every `std.Thread.spawn(.{}, ..)` thread
/// gets. Rust's `std::thread` default is 2 MiB (parity tail S7-4b, N278).
pub const DEFAULT_STACK_SIZE: usize = 16 * 1024 * 1024;

/// `std.Thread.spawn(.{}, ..)`'s `SpawnConfig{}` as a `std::thread::Builder`:
/// a [`DEFAULT_STACK_SIZE`] stack. Every thread the reference spawns with the
/// default config is built from this.
pub fn spawn_config() -> std::thread::Builder {
    std::thread::Builder::new().stack_size(DEFAULT_STACK_SIZE)
}
use std::time::{Duration, Instant};

/// Zig `std.Thread.Mutex`: a guard-less mutual-exclusion lock.
///
/// Semantics (matching Zig):
/// - `lock` blocks until the lock is acquired; recursive locking from the
///   same thread DEADLOCKS (Zig: "It is undefined behavior if the mutex is
///   already held by the caller" — the futex impl blocks forever, and so do
///   we; we never grant re-entry).
/// - `try_lock` acquires without blocking, returning whether it succeeded.
/// - `unlock` releases the lock; callable from any frame or thread that the
///   holder hands control to (the `m.unlock(); ...; defer m.lock()` inversion
///   in `termio.Mailbox.send`). Unlocking a mutex that is not locked is UB in
///   Zig; here it is a debug assertion.
//
// PORT NOTE: `= .{}` default-initialization in Zig maps to `new()`/`Default`.
pub struct Mutex {
    /// Whether the outer (Zig-visible) lock is currently held. The std mutex
    /// guards ONLY this flag; it is never held across user code.
    locked: StdMutex<bool>,

    /// Signaled when the outer lock is released so `lock()` waiters can race
    /// to acquire it.
    cond: Condvar,
}

impl Mutex {
    /// Zig `Mutex = .{}` — a new, unlocked mutex.
    pub const fn new() -> Mutex {
        Mutex {
            locked: StdMutex::new(false),
            cond: Condvar::new(),
        }
    }

    /// Acquire the inner flag guard, IGNORING std poisoning: Zig has no
    /// poisoning (a Zig panic aborts the process), and the flag state is
    /// always valid (the guard is never held across user code).
    #[inline]
    fn flag(&self) -> MutexGuard<'_, bool> {
        match self.locked.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Zig `Mutex.lock`: "Acquires the mutex, blocking the caller's thread
    /// until it can." No guard is returned — release with [`Mutex::unlock`].
    pub fn lock(&self) {
        let mut locked = self.flag();
        while *locked {
            locked = match self.cond.wait(locked) {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
        }
        *locked = true;
    }

    /// Zig `Mutex.tryLock`: "Tries to acquire the mutex without blocking the
    /// caller's thread. Returns `false` if the calling thread would have to
    /// block to acquire it," otherwise acquires and returns `true`.
    pub fn try_lock(&self) -> bool {
        let mut locked = self.flag();
        if *locked {
            false
        } else {
            *locked = true;
            true
        }
    }

    /// Zig `Mutex.unlock`: "Releases the mutex which was previously acquired
    /// with `lock` or `tryLock`. It is undefined behavior if the mutex wasn't
    /// locked" — asserted in debug builds here.
    pub fn unlock(&self) {
        let mut locked = self.flag();
        debug_assert!(*locked, "unlock of an unlocked zigstd thread::Mutex");
        *locked = false;
        // Drop the flag guard before signalling so the woken waiter can take
        // it immediately.
        drop(locked);
        self.cond.notify_one();
    }
}

impl Default for Mutex {
    fn default() -> Mutex {
        Mutex::new()
    }
}

/// Zig `std.Thread.Semaphore.timedWait` error set `error{Timeout}` — the wait
/// expired before a permit became available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeout;

/// Zig `std.Thread.Semaphore`: "A semaphore is an unsigned integer that blocks
/// the kernel thread if the number would become negative. This API supports
/// static initialization and does not require deinitialization."
///
/// Ghostty's `generic.zig` `SwapChain` holds one as `frame_sema:
/// std.Thread.Semaphore = .{ .permits = buf_count }` for triple-buffer
/// backpressure: `nextFrame` [`wait`](Semaphore::wait)s a permit before handing
/// out a frame, and `releaseFrame`/`deinit` [`post`](Semaphore::post) it back —
/// so at most `buf_count` frames are ever in flight (the GPU draw path's flow
/// control).
///
/// Backing (mirroring the sibling [`Mutex`]): a `std::sync::Mutex<usize>`
/// guarding the `permits` count plus a `Condvar`. Zig's struct is `mutex + cond
/// + permits` with the mutex guarding the plain `permits` field; folding
/// `permits` INTO the std mutex is the faithful mapping — the std mutex plays
/// the role of Zig's `mutex`, held only for the count check/update, never across
/// user code. Like [`Mutex`] this is Zig STDLIB glue, not ghostty src/, so it is
/// NOT in the parity test contract; the faithfulness target is the exact
/// observable semantics of `std.Thread.Semaphore` as ghostty uses it.
///
/// Example (the Zig source's runnable consumer/producer example, translated —
/// Zig shares a `var s` global; Rust shares via `Arc`):
///
/// ```
/// use rustty_zigstd::thread::Semaphore;
/// use std::sync::Arc;
///
/// let s = Arc::new(Semaphore::new());
///
/// // fn producer() { s.post(); }
/// let producer = {
///     let s = Arc::clone(&s);
///     std::thread::spawn(move || s.post())
/// };
///
/// // fn consumer() { s.wait(); }  — blocks until the producer posts.
/// s.wait();
///
/// producer.join().unwrap();
/// ```
pub struct Semaphore {
    /// The permit count. Zig: `permits: usize = 0` guarded by `mutex`; here the
    /// std mutex IS that guard. "It is OK to initialize this field to any value."
    permits: StdMutex<usize>,

    /// Zig: `cond: Condition = .{}` — signalled on [`post`](Semaphore::post) and
    /// on the permit-handoff chain in [`wait`](Semaphore::wait) so blocked
    /// waiters wake.
    cond: Condvar,
}

impl Semaphore {
    /// Zig `Semaphore{}` — a new semaphore with `permits = 0`.
    pub const fn new() -> Semaphore {
        Semaphore::with_permits(0)
    }

    /// Zig `Semaphore{ .permits = n }` — static initialization to `n` permits
    /// (`SwapChain` uses `.{ .permits = buf_count }`).
    pub const fn with_permits(permits: usize) -> Semaphore {
        Semaphore {
            permits: StdMutex::new(permits),
            cond: Condvar::new(),
        }
    }

    /// Acquire the count guard, IGNORING std poisoning: Zig has no poisoning (a
    /// Zig panic aborts the process), and the count is always valid — the guard
    /// is never held across user code. Mirrors [`Mutex::flag`].
    #[inline]
    fn count(&self) -> MutexGuard<'_, usize> {
        match self.permits.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Zig `Semaphore.wait`: blocks the caller until a permit is available, then
    /// consumes one. When a surplus remains, hands the signal on so another
    /// blocked waiter can wake (`if (sem.permits > 0) sem.cond.signal();`).
    pub fn wait(&self) {
        let mut permits = self.count();

        while *permits == 0 {
            permits = match self.cond.wait(permits) {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
        }

        *permits -= 1;
        if *permits > 0 {
            self.cond.notify_one();
        }
    }

    /// Zig `Semaphore.timedWait`: like [`wait`](Semaphore::wait) but returns
    /// [`Timeout`] if no permit becomes available within `timeout_ns`
    /// nanoseconds. (Unused by ghostty's `SwapChain`, which only `wait`/`post`s;
    /// ported for fidelity to the stdlib type.)
    pub fn timed_wait(&self, timeout_ns: u64) -> Result<(), Timeout> {
        // `std.time.Timer.start()` — a monotonic stopwatch started now.
        let timeout_timer = Instant::now();

        let mut permits = self.count();

        while *permits == 0 {
            let elapsed = timeout_timer.elapsed().as_nanos();
            if elapsed > u128::from(timeout_ns) {
                return Err(Timeout);
            }

            // `elapsed <= timeout_ns` here, so the subtraction and the cast back
            // to u64 cannot overflow.
            let local_timeout_ns = timeout_ns - elapsed as u64;
            let (guard, res) = match self
                .cond
                .wait_timeout(permits, Duration::from_nanos(local_timeout_ns))
            {
                Ok(pair) => pair,
                Err(poisoned) => poisoned.into_inner(),
            };
            permits = guard;
            // Zig `try sem.cond.timedWait(...)` propagates `error.Timeout` on
            // expiry; a spurious/real wake falls through to re-check `permits`.
            if res.timed_out() {
                return Err(Timeout);
            }
        }

        *permits -= 1;
        if *permits > 0 {
            self.cond.notify_one();
        }
        Ok(())
    }

    /// Zig `Semaphore.post`: releases a permit and signals one waiter. Signalled
    /// while the count guard is held (Zig signals under `defer mutex.unlock()`).
    pub fn post(&self) {
        let mut permits = self.count();
        *permits += 1;
        self.cond.notify_one();
    }
}

impl Default for Semaphore {
    fn default() -> Semaphore {
        Semaphore::new()
    }
}

// rustty-only unit tests (this type is Zig STDLIB glue, not part of the parity
// test contract; CORE_BLUEPRINT §7 assigns it 2-3 rustty-only tests).
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::UnsafeCell;
    use std::sync::Arc;

    /// A value protected by the guard-less mutex the way ghostty protects the
    /// renderer state: the Mutex and the data are separate; holding the lock
    /// is a protocol, not a type-level borrow.
    struct Shared {
        mutex: Mutex,
        value: UnsafeCell<u64>,
    }
    // SAFETY (test-only): all access to `value` happens between
    // `mutex.lock()`/`unlock()` below — the mutual-exclusion protocol under test.
    unsafe impl Sync for Shared {}

    // rustty-only: lock/unlock provide mutual exclusion across threads (the
    // canonical racing-increment check).
    #[test]
    fn lock_unlock_across_threads() {
        const THREADS: usize = 4;
        const INCREMENTS: u64 = 10_000;

        let shared = Arc::new(Shared {
            mutex: Mutex::new(),
            value: UnsafeCell::new(0),
        });

        let handles: Vec<_> = (0..THREADS)
            .map(|_| {
                let shared = Arc::clone(&shared);
                std::thread::spawn(move || {
                    for _ in 0..INCREMENTS {
                        shared.mutex.lock();
                        // SAFETY: the lock is held.
                        unsafe { *shared.value.get() += 1 };
                        shared.mutex.unlock();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        shared.mutex.lock();
        // SAFETY: the lock is held.
        let total = unsafe { *shared.value.get() };
        shared.mutex.unlock();
        assert_eq!(total, THREADS as u64 * INCREMENTS);
    }

    // rustty-only: try_lock fails while held (even by the same thread — no
    // re-entry, matching Zig) and succeeds after unlock.
    #[test]
    fn try_lock() {
        let m = Mutex::new();
        assert!(m.try_lock());
        // Held: a second try_lock must fail without blocking.
        assert!(!m.try_lock());
        m.unlock();
        // Released: acquirable again.
        assert!(m.try_lock());
        m.unlock();
    }

    // rustty-only: the unlock-from-callee inversion (`m.unlock(); ...; defer
    // m.lock()`) that motivated the guard-less design — termio.Mailbox.send's
    // slow path releases the CALLER-held renderer mutex so another thread can
    // acquire it, then relocks before returning.
    #[test]
    fn unlock_from_callee() {
        // The callee frame: releases the caller-held lock, waits for the other
        // thread to take and release it, then relocks (the `defer m.lock()`).
        fn callee(m: &Mutex, other: std::thread::JoinHandle<()>) {
            m.unlock();
            // The other thread's `lock()` can only complete inside this
            // window; joining proves it acquired the lock we released.
            other.join().unwrap();
            m.lock();
        }

        let m = Arc::new(Mutex::new());
        m.lock(); // "caller" holds the lock

        let other = {
            let m = Arc::clone(&m);
            std::thread::spawn(move || {
                // Blocks until the callee releases the caller-held lock.
                m.lock();
                m.unlock();
            })
        };

        callee(&m, other);
        // Back from the callee with the lock re-held, exactly as the
        // reference's `defer` guarantees.
        assert!(!m.try_lock());
        m.unlock();
    }

    // Worker context for the ported `test Semaphore`: a semaphore + a shared
    // non-atomic counter (Zig's `sem: *Semaphore, n: *i32`). The permits=1
    // semaphore serialises the increments, so the raw `*n += 1` is race-free.
    struct SemCtx {
        sem: Semaphore,
        n: UnsafeCell<i32>,
    }
    // SAFETY (test-only): every access to `n` happens while a permit is held —
    // the mutual exclusion under test.
    unsafe impl Sync for SemCtx {}

    // Ported from Zig std `test Semaphore` (the `single_threaded` skip is N/A —
    // rustty is always multi-threaded): three workers each `wait` a permit,
    // increment the shared counter, then `post`; after they join, the main
    // thread's `wait` proves all three ran.
    #[test]
    fn semaphore() {
        const NUM_THREADS: usize = 3;
        let ctx = Arc::new(SemCtx {
            sem: Semaphore::with_permits(1),
            n: UnsafeCell::new(0),
        });

        let threads: Vec<_> = (0..NUM_THREADS)
            .map(|_| {
                let ctx = Arc::clone(&ctx);
                std::thread::spawn(move || {
                    ctx.sem.wait();
                    // SAFETY: the permit is held → exclusive access to `n`.
                    unsafe { *ctx.n.get() += 1 };
                    ctx.sem.post();
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        ctx.sem.wait();
        // SAFETY: the final permit is held and every worker has joined.
        let n = unsafe { *ctx.n.get() };
        assert_eq!(n, NUM_THREADS as i32);
    }

    // Ported from Zig std `test timedWait`: a fresh (0-permit) semaphore times
    // out; after a `post` the permit is taken without blocking.
    #[test]
    fn timed_wait() {
        let sem = Semaphore::new();
        assert_eq!(*sem.count(), 0);

        assert_eq!(sem.timed_wait(1), Err(Timeout));

        sem.post();
        assert_eq!(*sem.count(), 1);

        assert_eq!(sem.timed_wait(1), Ok(()));
        assert_eq!(*sem.count(), 0);
    }
}
