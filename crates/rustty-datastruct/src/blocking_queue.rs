//! Blocking queue implementation aimed primarily for message passing
//! between threads.
//
// PORT NOTE: The Zig reference is `fn BlockingQueue(T, capacity) type`. We make
// `T` a generic and `capacity` a const generic. `std.Thread.Mutex` ->
// `std.sync::Mutex`-style state is reproduced with `std::sync::Mutex` guarding
// the queue state, and `std.Thread.Condition` -> `std::sync::Condvar` for the
// not-full waiters. Because the Zig API does manual `lock()`/`unlock()` around
// fields, we move all guarded fields into an inner `State` behind the mutex and
// drive the condvar exactly as the reference does. `create`/`destroy` map to
// heap allocation (`Box`) — Rust's ownership handles the free.

use std::sync::{Condvar, Mutex};
use std::time::Duration;

/// The type we use for queue size types. We can optimize this
/// in the future to be the correct bit-size for our preallocated
/// size for this queue.
pub type Size = u32;

/// Specifies the timeout for an operation.
pub enum Timeout {
    /// Fail instantly (non-blocking).
    Instant,

    /// Run forever or until interrupted
    Forever,

    /// Nanoseconds
    Ns(u64),
}

// The guarded state of the queue. In the Zig reference these are bare fields
// protected by the big mutex; in Rust we group them so the `Mutex` can own
// them.
struct State<T, const CAPACITY: usize> {
    /// Our data. The values are undefined until they are written.
    data: Vec<Option<T>>,

    /// The next location to write (next empty loc) and next location
    /// to read (next non-empty loc). The number of written elements.
    write: Size,
    read: Size,
    len: Size,

    not_full_waiters: usize,
}

/// Returns a blocking queue implementation for type T.
///
/// This is tailor made for ghostty usage so it isn't meant to be maximally
/// generic, but I'm happy to make it more generic over time. Traits of this
/// queue that are specific to our usage:
///
///   - Fixed size. We expect our queue to quickly drain and also not be
///     too large so we prefer a fixed size queue for now.
///   - No blocking pop. We use an external event loop mechanism such as
///     eventfd to notify our waiter that there is no data available so
///     we don't need to implement a blocking pop.
///   - Drain function. Most queues usually pop one at a time. We have
///     a mechanism for draining since on every IO loop our TTY drains
///     the full queue so we can get rid of the overhead of a ton of
///     locks and bounds checking and do a one-time drain.
///
/// One key usage pattern is that our blocking queues are single producer
/// single consumer (SPSC). This should let us do some interesting optimizations
/// in the future. At the time of writing this, the blocking queue implementation
/// is purposely naive to build something quickly, but we should benchmark
/// and make this more optimized as necessary.
pub struct BlockingQueue<T, const CAPACITY: usize> {
    /// The big mutex that must be held to read/write.
    mutex: Mutex<State<T, CAPACITY>>,

    /// A CV for being notified when the queue is no longer full. This is
    /// used for writing. Note we DON'T have a CV for waiting on the
    /// queue not being EMPTY because we use external notifiers for that.
    cond_not_full: Condvar,
}

impl<T, const CAPACITY: usize> BlockingQueue<T, CAPACITY> {
    // The bounds of this queue. We recast this to Size so we can do math.
    const BOUNDS: Size = CAPACITY as Size;

    /// Allocate the blocking queue on the heap.
    pub fn create() -> Box<Self> {
        let mut data: Vec<Option<T>> = Vec::with_capacity(CAPACITY);
        for _ in 0..CAPACITY {
            data.push(None);
        }
        Box::new(BlockingQueue {
            mutex: Mutex::new(State {
                data,
                len: 0,
                write: 0,
                read: 0,
                not_full_waiters: 0,
            }),
            cond_not_full: Condvar::new(),
        })
    }

    /// Free all the resources for this queue. This should only be
    /// called once all producers and consumers have quit.
    pub fn destroy(self: Box<Self>) {
        // self.* = undefined; alloc.destroy(self);
        // In Rust, dropping the Box frees the queue.
        drop(self);
    }

    /// Push a value to the queue. This returns the total size of the
    /// queue (unread items) after the push. A return value of zero
    /// means that the push failed.
    pub fn push(&self, value: T, timeout: Timeout) -> Size {
        let mut state = self.mutex.lock().unwrap();

        // The
        if Self::full(&state) {
            match timeout {
                // If we're not waiting, then we failed to write.
                Timeout::Instant => return 0,

                Timeout::Forever => {
                    state.not_full_waiters += 1;
                    state = self.cond_not_full.wait(state).unwrap();
                    state.not_full_waiters -= 1;
                }

                Timeout::Ns(ns) => {
                    state.not_full_waiters += 1;
                    let (s, res) = self
                        .cond_not_full
                        .wait_timeout(state, Duration::from_nanos(ns))
                        .unwrap();
                    state = s;
                    state.not_full_waiters -= 1;
                    if res.timed_out() {
                        return 0;
                    }
                }
            }

            // If we're still full, then we failed to write. This can
            // happen in situations where we are interrupted.
            if Self::full(&state) {
                return 0;
            }
        }

        // Add our data and update our accounting
        let w = state.write as usize;
        state.data[w] = Some(value);
        state.write += 1;
        if state.write >= Self::BOUNDS {
            state.write -= Self::BOUNDS;
        }
        state.len += 1;

        state.len
    }

    /// Pop a value from the queue without blocking.
    pub fn pop(&self) -> Option<T> {
        let mut state = self.mutex.lock().unwrap();

        // If we're empty we have nothing
        if state.len == 0 {
            return None;
        }

        // Get the index we're going to read data from and do some
        // accounting. We don't copy the value here to avoid copying twice.
        let n = state.read as usize;
        state.read += 1;
        if state.read >= Self::BOUNDS {
            state.read -= Self::BOUNDS;
        }
        state.len -= 1;

        // If we have consumers waiting on a full queue, notify.
        if state.not_full_waiters > 0 {
            self.cond_not_full.notify_one();
        }

        state.data[n].take()
    }

    /// Pop all values from the queue. This will hold the big mutex
    /// until `deinit` is called on the return value. This is used if
    /// you know you're going to "pop" and utilize all the values
    /// quickly to avoid many locks, bounds checks, and cv signals.
    pub fn drain(&self) -> DrainIterator<'_, T, CAPACITY> {
        let guard = self.mutex.lock().unwrap();
        DrainIterator {
            queue: self,
            state: Some(guard),
        }
    }

    /// Returns true if the queue is full. This is not public because
    /// it requires the lock to be held.
    #[inline]
    fn full(state: &State<T, CAPACITY>) -> bool {
        state.len == Self::BOUNDS
    }
}

/// Drain iterator over the queue (see `BlockingQueue::drain`).
pub struct DrainIterator<'a, T, const CAPACITY: usize> {
    queue: &'a BlockingQueue<T, CAPACITY>,
    state: Option<std::sync::MutexGuard<'a, State<T, CAPACITY>>>,
}

impl<'a, T, const CAPACITY: usize> DrainIterator<'a, T, CAPACITY> {
    // PARITY: the reference `blocking_queue.zig:171` exposes an inherent,
    // iterator-like `next()` on `DrainIterator`. We keep that public API shape
    // rather than reshaping it into `std::iter::Iterator`, so we silence
    // `should_implement_trait`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<T> {
        let state = self.state.as_mut().unwrap();
        if state.len == 0 {
            return None;
        }

        // Read and account
        let n = state.read as usize;
        state.read += 1;
        if state.read >= BlockingQueue::<T, CAPACITY>::BOUNDS {
            state.read -= BlockingQueue::<T, CAPACITY>::BOUNDS;
        }
        state.len -= 1;

        state.data[n].take()
    }

    pub fn deinit(mut self) {
        // If we have consumers waiting on a full queue, notify.
        let waiters = self.state.as_ref().unwrap().not_full_waiters;
        if waiters > 0 {
            self.queue.cond_not_full.notify_one();
        }

        // Unlock
        self.state = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_push_and_pop() {
        type Q = BlockingQueue<u64, 4>;
        let q = Q::create();

        // Should have no values
        assert!(q.pop().is_none());

        // Push until we're full
        assert_eq!(1 as Size, q.push(1, Timeout::Instant));
        assert_eq!(2 as Size, q.push(2, Timeout::Instant));
        assert_eq!(3 as Size, q.push(3, Timeout::Instant));
        assert_eq!(4 as Size, q.push(4, Timeout::Instant));
        assert_eq!(0 as Size, q.push(5, Timeout::Instant));

        // Pop!
        assert!(q.pop().unwrap() == 1);
        assert!(q.pop().unwrap() == 2);
        assert!(q.pop().unwrap() == 3);
        assert!(q.pop().unwrap() == 4);
        assert!(q.pop().is_none());

        // Drain does nothing
        let mut it = q.drain();
        assert!(it.next().is_none());
        it.deinit();

        // Verify we can still push
        assert_eq!(1 as Size, q.push(1, Timeout::Instant));

        q.destroy();
    }

    #[test]
    fn timed_push() {
        type Q = BlockingQueue<u64, 1>;
        let q = Q::create();

        // Push
        assert_eq!(1 as Size, q.push(1, Timeout::Instant));
        assert_eq!(0 as Size, q.push(2, Timeout::Instant));

        // Timed push should fail
        assert_eq!(0 as Size, q.push(2, Timeout::Ns(1000)));

        q.destroy();
    }
}
