//! Port of `libvaxis` `queue.zig` (0.6.0: 376 LOC, 7 `test{}`) — a thread-safe, fixed-size,
//! BLOCKING ring buffer (the concurrency channel between the Loop read thread and the
//! main thread). Faithful Rust: the lock-guarded mutable state lives in a
//! `Mutex<Inner>` and the two condition variables sit beside it.
//!
//! PORT NOTE (`undefined` → `Option::None`, §2.2): Zig `buf: [size]T = undefined`
//! stores `T` by value in undefined slots, occupancy tracked purely by the ring
//! indices. The faithful-and-safe port stores `[Option<T>; N]` — push = `Some(item)`,
//! pop = `.take().unwrap()`. This is fully SAFE (zero `unsafe`), so the ONE module we
//! want unconditionally Miri-clean has no unsafe to audit. The only divergence is a
//! redundant `None` tag (the ring indices already track occupancy) — behaviorally
//! identical.
//!
//! PORT NOTE (`&self`, not `&mut self`): every method takes `&self` — the whole point
//! of the internal `Mutex` (load-bearing for the Loop, where the read thread and the
//! main thread both hold only `&Queue`). `signal()` → `Condvar::notify_one()`;
//! `wait(&mutex)` → `Condvar::wait(guard)` (returns the re-acquired guard). The
//! `while` (not `if`) predicate re-check is exactly what tests 5/6 assert (spurious
//! wakeups) — kept verbatim.
//!
//! PORT NOTE (`lock`/`unlock`/`drain`): Rust RAII makes Zig's split `lock()` +
//! `unlock()` awkward, so `drain` takes/returns the guard (the externally-held-lock
//! drain API).
//!
//! PORT NOTE (libvaxis 0.6.0, `std.Io`): the queue now holds an `io: std.Io` and uses
//! `std.Io.Mutex` / `std.Io.Condition`, so `pop`/`push`/`tryPush`/`tryPop`/`poll`/`lock`/
//! `isEmpty`/`isFull` return `error.Canceled` when the calling `std.Io` task is canceled.
//! The port keeps `std::sync` and plain `std::thread`s, which have no cancelation, so these
//! stay infallible and `init(io)` is `Default`. The 0.6.0 behaviour changes ARE ported:
//! `tryPush`/`tryPop` check and act under one lock, `pop`/`push` signal through
//! `popAndSignalLH`/`pushAndSignalLH`, and `drain` now signals `not_full`.

use std::sync::{Condvar, Mutex, MutexGuard};

/// The lock-guarded mutable state of the ring. Held under the `Mutex`; the two
/// `Condvar`s sit beside it in [`Queue`]. Its fields are private — the type name is
/// public only so the guard-shaped `lock`/`unlock`/`drain` API can name it.
pub struct Inner<T, const N: usize> {
    buf: [Option<T>; N], // Zig `buf: [size]T = undefined`; see the module PORT NOTE
    read_index: usize,   // = 0
    write_index: usize,  // = 0
}

impl<T, const N: usize> Inner<T, N> {
    fn is_empty_lh(&self) -> bool {
        self.write_index == self.read_index
    }

    fn is_full_lh(&self) -> bool {
        self.mask2(self.write_index + N) == self.read_index
    }

    /// Returns the length
    #[allow(dead_code)] // unused by the tests; ported for R1 fidelity
    fn len(&self) -> usize {
        let wrap_offset = 2 * N * usize::from(self.write_index < self.read_index);
        let adjusted_write_index = self.write_index + wrap_offset;
        adjusted_write_index - self.read_index
    }

    /// Returns `index` modulo the length of the backing slice.
    fn mask(&self, index: usize) -> usize {
        index % N
    }

    /// Returns `index` modulo twice the length of the backing slice.
    fn mask2(&self, index: usize) -> usize {
        index % (2 * N)
    }

    fn pop_lh(&mut self) -> T {
        let result = self.buf[self.mask(self.read_index)].take().unwrap();
        self.read_index = self.mask2(self.read_index + 1);
        result
    }
}

/// Thread safe. Fixed size. Blocking push and pop.
pub struct Queue<T, const N: usize> {
    inner: Mutex<Inner<T, N>>,
    // blocks when the buffer is full
    not_full: Condvar,
    // ...or empty
    not_empty: Condvar,
}

impl<T, const N: usize> Default for Queue<T, N> {
    fn default() -> Self {
        Queue {
            inner: Mutex::new(Inner {
                // arrays impl `Default` only <= 32; build `buf` element-by-element
                buf: core::array::from_fn(|_| None),
                read_index: 0,
                write_index: 0,
            }),
            not_full: Condvar::new(),
            not_empty: Condvar::new(),
        }
    }
}

impl<T, const N: usize> Queue<T, N> {
    /// Pop an item from the queue. Blocks until an item is available.
    pub fn pop(&self) -> T {
        let mut g = self.inner.lock().unwrap();
        while g.is_empty_lh() {
            g = self.not_empty.wait(g).unwrap();
        }
        debug_assert!(!g.is_empty_lh());
        self.pop_and_signal_lh(&mut g)
    }

    /// Push an item into the queue. Blocks until an item has been
    /// put in the queue.
    pub fn push(&self, item: T) {
        let mut g = self.inner.lock().unwrap();
        while g.is_full_lh() {
            g = self.not_full.wait(g).unwrap();
        }
        debug_assert!(!g.is_full_lh());
        self.push_and_signal_lh(&mut g, item);
    }

    /// Push an item into the queue. Returns true when the item
    /// was successfully placed in the queue, false if the queue
    /// was full.
    ///
    /// libvaxis 0.6.0 holds the lock across the check and the push (0.5.1 unlocked
    /// after the check and called the blocking `push`, which could then block).
    pub fn try_push(&self, item: T) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.is_full_lh() {
            return false;
        }
        self.push_and_signal_lh(&mut g, item);
        true
    }

    /// Pop an item from the queue. Returns null when no item is
    /// available.
    ///
    /// libvaxis 0.6.0 holds the lock across the check and the pop (see `try_push`).
    pub fn try_pop(&self) -> Option<T> {
        let mut g = self.inner.lock().unwrap();
        if g.is_empty_lh() {
            return None;
        }
        Some(self.pop_and_signal_lh(&mut g))
    }

    /// Poll the queue. This call blocks until events are in the queue
    pub fn poll(&self) {
        let mut g = self.inner.lock().unwrap();
        while g.is_empty_lh() {
            g = self.not_empty.wait(g).unwrap();
        }
        debug_assert!(!g.is_empty_lh());
    }

    pub fn lock(&self) -> MutexGuard<'_, Inner<T, N>> {
        self.inner.lock().unwrap()
    }

    pub fn unlock(&self, guard: MutexGuard<'_, Inner<T, N>>) {
        drop(guard);
    }

    /// Used to efficiently drain the queue while the lock is externally held
    pub fn drain(&self, guard: &mut MutexGuard<'_, Inner<T, N>>) -> Option<T> {
        if guard.is_empty_lh() {
            return None;
        }
        // Preserve queue push wakeups when draining under external lock.
        // If the queue was full before this pop, a producer may be blocked
        // waiting on not_full.
        let was_full = guard.is_full_lh();
        let item = guard.pop_lh();
        if was_full {
            self.not_full.notify_one();
        }
        Some(item)
    }

    /// Returns `true` if the queue is empty and `false` otherwise.
    pub fn is_empty(&self) -> bool {
        let g = self.inner.lock().unwrap();
        g.is_empty_lh()
    }

    /// Returns `true` if the queue is full and `false` otherwise.
    pub fn is_full(&self) -> bool {
        let g = self.inner.lock().unwrap();
        g.is_full_lh()
    }

    // Edge-only signalling (`not_empty` only when the queue was empty, `not_full` only
    // when it was full) is the reference's, kept faithfully; with two waiters at once it
    // can leave the second waiting (UPSTREAM-DEFECTS UD267, kind: latent).
    fn push_and_signal_lh(&self, g: &mut MutexGuard<'_, Inner<T, N>>, item: T) {
        let was_empty = g.is_empty_lh();
        let write_index = g.write_index;
        let slot = g.mask(write_index);
        g.buf[slot] = Some(item);
        g.write_index = g.mask2(write_index + 1);
        if was_empty {
            self.not_empty.notify_one();
        }
    }

    fn pop_and_signal_lh(&self, g: &mut MutexGuard<'_, Inner<T, N>>) -> T {
        let was_full = g.is_full_lh();
        let result = g.pop_lh();
        if was_full {
            self.not_full.notify_one();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::time::{Duration, Instant};

    #[test]
    fn queue_simple_push_pop() {
        let queue: Queue<u8, 16> = Queue::default();
        queue.push(1);
        queue.push(2);
        let pop = queue.pop();
        assert_eq!(pop, 1);
        assert_eq!(queue.pop(), 2);
    }

    #[test]
    fn fill_wait_to_push_pop_once_in_another_thread() {
        let queue: Queue<u8, 2> = Queue::default();
        queue.push(1);
        queue.push(2);
        std::thread::scope(|s| {
            // testPushPop: push 3 (blocks until space), then pop == 2.
            let t = s.spawn(|| {
                queue.push(3);
                assert_eq!(queue.pop(), 2);
            });
            assert!(!queue.try_push(3));
            assert_eq!(queue.pop(), 1);
            t.join().unwrap();
            assert_eq!(queue.pop(), 3);
            assert_eq!(queue.try_pop(), None);
        });
    }

    #[test]
    fn try_to_pop_fill_from_another_thread() {
        let queue: Queue<u8, 2> = Queue::default();
        std::thread::scope(|s| {
            // testPush
            s.spawn(|| {
                queue.push(0);
                queue.push(1);
                queue.push(2);
                queue.push(3);
                queue.push(4);
            });
            for idx in 0..5u8 {
                assert_eq!(queue.pop(), idx);
            }
        });
    }

    // Fill the queue, block while trying to write another item, have
    // a background thread unblock us, then block while trying to
    // write yet another thing. Have the background thread unblock
    // that too (after some time) then drain the queue. This test
    // fails if the while loop in `push` is turned into an `if`.
    #[test]
    fn fill_block_fill_block() {
        let queue: Queue<u8, 2> = Queue::default();
        let state = AtomicU8::new(0);
        std::thread::scope(|s| {
            // sleepyPop
            s.spawn(|| {
                // First we wait for the queue to be full.
                while state.load(Ordering::Acquire) < 1 {
                    std::thread::yield_now();
                }

                // Then we spuriously wake it up, because that's a thing that can
                // happen.
                queue.not_full.notify_one();
                queue.not_empty.notify_one();

                // Then give the other thread a good chance of waking up. It's not
                // clear that yield guarantees the other thread will be scheduled,
                // so we'll throw a sleep in here just to be sure. The queue is
                // still full and the push in the other thread is still blocked
                // waiting for space.
                std::thread::yield_now();
                std::thread::sleep(Duration::from_millis(10));
                // Finally, let that other thread go.
                assert_eq!(queue.pop(), 1);

                // Wait for the other thread to signal it's ready for second push
                while state.load(Ordering::Acquire) < 2 {
                    std::thread::yield_now();
                }
                // But we want to ensure that there's a second push waiting, so
                // here's another sleep.
                std::thread::sleep(Duration::from_millis(10));

                // Another spurious wake...
                queue.not_full.notify_one();
                queue.not_empty.notify_one();
                // And another chance for the other thread to see that it's
                // spurious and go back to sleep.
                std::thread::yield_now();
                std::thread::sleep(Duration::from_millis(10));

                // Pop that thing and we're done.
                assert_eq!(queue.pop(), 2);
            });

            queue.push(1);
            queue.push(2);
            state.store(1, Ordering::Release);
            let now = Instant::now();
            queue.push(3); // This one should block.
            let then = now.elapsed();

            // Just to make sure the sleeps are yielding to this thread, make
            // sure it took at least 5ms to do the push.
            assert!(then > Duration::from_millis(5));

            state.store(2, Ordering::Release);
            // This should block again, waiting for the other thread.
            queue.push(4);
        });

        // And once that push has gone through, the other thread's done.
        assert_eq!(queue.pop(), 3);
        assert_eq!(queue.pop(), 4);
    }

    // This is like fill/block/fill/block, but on the pop end. This
    // test should fail if the `while` loop in `pop` is turned into an
    // `if`.
    #[test]
    fn drain_block_drain_block() {
        let queue: Queue<u8, 1> = Queue::default();
        let state = AtomicU8::new(0);
        std::thread::scope(|s| {
            // sleepyPush
            s.spawn(|| {
                // Try to ensure the other thread has already started trying to pop.
                std::thread::yield_now();
                std::thread::sleep(Duration::from_millis(10));

                // Spurious wake
                queue.not_full.notify_one();
                queue.not_empty.notify_one();

                std::thread::yield_now();
                std::thread::sleep(Duration::from_millis(10));

                // Stick something in the queue so it can be popped.
                queue.push(1);
                // Ensure it's been popped.
                while state.load(Ordering::Acquire) < 1 {
                    std::thread::yield_now();
                }
                // Give the other thread time to block again.
                std::thread::yield_now();
                std::thread::sleep(Duration::from_millis(10));

                // Spurious wake
                queue.not_full.notify_one();
                queue.not_empty.notify_one();

                queue.push(2);
            });

            assert_eq!(queue.pop(), 1);
            state.store(1, Ordering::Release);
            assert_eq!(queue.pop(), 2);
        });
    }

    /// libvaxis 0.6.0 `drain`: "Preserve queue push wakeups when draining under external
    /// lock. If the queue was full before this pop, a producer may be blocked waiting on
    /// not_full." (0.5.1's `drain` did not signal, so that producer stayed blocked.)
    #[test]
    fn drain_wakes_a_blocked_producer() {
        let queue: Queue<u8, 1> = Queue::default();
        queue.push(1);
        let pushed = std::sync::atomic::AtomicBool::new(false);
        std::thread::scope(|s| {
            s.spawn(|| {
                queue.push(2); // blocks: the queue is full
                pushed.store(true, Ordering::Release);
            });
            // Give the producer time to block on `not_full`.
            std::thread::sleep(Duration::from_millis(20));
            assert!(!pushed.load(Ordering::Acquire));

            let mut g = queue.lock();
            assert_eq!(queue.drain(&mut g), Some(1));
            queue.unlock(g);

            let deadline = Instant::now() + Duration::from_secs(2);
            while !pushed.load(Ordering::Acquire) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            let woke = pushed.load(Ordering::Acquire);
            if !woke {
                // Unblock the producer so the scope can end, then fail.
                let _ = queue.try_pop();
                queue.not_full.notify_one();
            }
            assert!(
                woke,
                "drain did not wake the producer blocked on a full queue"
            );
        });
        assert_eq!(queue.pop(), 2);
    }

    #[test]
    fn two_readers() {
        // 2 threads read, one thread writes
        let queue: Queue<u8, 1> = Queue::default();
        std::thread::scope(|s| {
            // readerThread x2
            s.spawn(|| assert_eq!(queue.pop(), 1));
            s.spawn(|| assert_eq!(queue.pop(), 1));
            std::thread::yield_now();
            std::thread::sleep(Duration::from_millis(10));
            queue.push(1);
            queue.push(1);
        });
    }

    #[test]
    fn two_writers() {
        let queue: Queue<u8, 1> = Queue::default();
        std::thread::scope(|s| {
            // writerThread x2
            s.spawn(|| queue.push(1));
            s.spawn(|| queue.push(1));
            assert_eq!(queue.pop(), 1);
            assert_eq!(queue.pop(), 1);
        });
    }
}
