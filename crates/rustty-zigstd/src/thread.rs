//! PORTS: Zig stdlib `std.Thread.Mutex` AS THE PORT USES IT — the GUARD-LESS
//! manual `lock()`/`tryLock()`/`unlock()` mutex.
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
}
