//! Isolated, **non-FFI** Miri harness that faithfully reproduces the SB-class
//! raw-pointer borrow structure of libxev `ThreadPool.zig`'s two lock-free
//! intrusive containers — `Node.Queue` (the Vyukov-style swap-based unbounded
//! MPSC queue) and `Node.Buffer` (the bounded single-producer / multi-consumer
//! work-stealing ring) — and hammers `push`/`pop`/`consume`/`steal` across
//! threads to PROVE the port is clean under BOTH Stacked Borrows and Tree
//! Borrows before the same primitives ship in `crates/rustty-xev/src/thread_pool.rs`.
//!
//! Why a harness (mirihar doctrine, `.port/PORT_RULES.md:209`,
//! memory `shaper-hook-sb-clean-interleave`): the real `ThreadPool` spawns/detaches
//! OS threads and (in the epoll File task) routes blocking pread/pwrite/close
//! through the pool — none of which Miri can execute. This harness strips the
//! syscalls and models ONLY the raw-pointer aliasing so Miri can interpret every
//! executed path.
//!
//! The load-bearing rules this run proves clean:
//!  1. Node linkage (`Node.next`) is ONLY ever touched through a raw `*mut Node`
//!     (`(*node).next`), NEVER through a `&Node`/`&mut Node`. Producers write it
//!     (push / migrate) and one consumer reads it (pop); the atomic
//!     stack/head/tail carry the Acquire/Release happens-before, so there is no
//!     data race and no reference tag to pop.
//!  2. The queue's `cache` (`?*Node`, mutated only by the single consumer holding
//!     `IS_CONSUMING`) lives in an `UnsafeCell`, so every container method takes
//!     `&self` (shared, interior-mutable via atomics + the cell) — the faithful
//!     Rust translation of Zig's `*Queue`/`*Buffer`, and the shape that lets the
//!     containers be shared across threads without a `&mut` uniqueness claim that
//!     concurrent stealers would violate.
//!  3. The `Node`s live in their OWN heap allocations (`Box::into_raw`), so a
//!     shared `&Shared` never freezes their bytes — the raw `(*node).next` writes
//!     are writes to a sibling allocation, not interior mutation of a frozen `&`.
//!
//! Buffer `capacity` is modelled at 8 (the real port uses 256): a smaller power
//! of two exercises the identical push-overflow/migrate + steal-half code paths
//! with far fewer nodes so Miri can finish. This is a borrow-STRUCTURE proof, not
//! a throughput/capacity proof.
//!
//! Run (from this dir):
//! ```sh
//! cargo +nightly miri test                                          # Stacked Borrows (default)
//! MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri test            # Tree Borrows
//! # (or `cargo +nightly miri run` to drive the same hammer via main())
//! ```

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicUsize};
use std::sync::atomic::Ordering::{Acquire, AcqRel, Relaxed, Release};
use std::cell::UnsafeCell;

// ---------------------------------------------------------------------------
// Node — the intrusive linked-list node embedded in every Task (Zap `Node`).
// ---------------------------------------------------------------------------
#[derive(Default)]
struct Node {
    next: Option<*mut Node>,
}

/// `@ptrFromInt(addr)` into a `?*Node`: address 0 is Zig's null pointer.
#[inline]
fn node_from_addr(addr: usize) -> Option<*mut Node> {
    if addr == 0 {
        None
    } else {
        Some(core::ptr::with_exposed_provenance_mut::<Node>(addr))
    }
}

/// A linked list of Nodes (Zig `Node.List`).
#[derive(Clone, Copy)]
struct List {
    head: *mut Node,
    tail: *mut Node,
}

// ---------------------------------------------------------------------------
// Node.Queue — an unbounded multi-producer-(non blocking)-multi-consumer queue
// of Node pointers.
// ---------------------------------------------------------------------------
struct Queue {
    stack: AtomicUsize,
    // Zig `cache: ?*Node = null` — mutated only by the single consumer holding
    // IS_CONSUMING. UnsafeCell so `&self` methods can write it (no data race:
    // the consumer bit serialises all cache access).
    cache: UnsafeCell<Option<*mut Node>>,
}

unsafe impl Sync for Queue {}

impl Default for Queue {
    fn default() -> Self {
        Queue { stack: AtomicUsize::new(0), cache: UnsafeCell::new(None) }
    }
}

const HAS_CACHE: usize = 0b01;
const IS_CONSUMING: usize = 0b10;
const PTR_MASK: usize = !(HAS_CACHE | IS_CONSUMING);

/// `error{ Empty, Contended }`
enum AcquireError {
    Empty,
    Contended,
}

impl Queue {
    fn push(&self, list: List) {
        let mut stack = self.stack.load(Relaxed);
        loop {
            // Attach the list to the stack (pt. 1)
            unsafe { (*list.tail).next = node_from_addr(stack & PTR_MASK) };

            // Update the stack with the list (pt. 2).
            // Don't change the HAS_CACHE and IS_CONSUMING bits of the consumer.
            let mut new_stack = list.head.expose_provenance();
            debug_assert_eq!(new_stack & !PTR_MASK, 0);
            new_stack |= stack & !PTR_MASK;

            // Push to the stack with a release barrier for the consumer to see
            // the proper list links.
            match self.stack.compare_exchange_weak(stack, new_stack, Release, Relaxed) {
                Ok(_) => break,
                Err(e) => stack = e,
            }
        }
    }

    fn try_acquire_consumer(&self) -> Result<Option<*mut Node>, AcquireError> {
        let mut stack = self.stack.load(Relaxed);
        loop {
            if stack & IS_CONSUMING != 0 {
                return Err(AcquireError::Contended); // The queue already has a consumer.
            }
            if stack & (HAS_CACHE | PTR_MASK) == 0 {
                return Err(AcquireError::Empty); // Nothing cached and nothing in the stack.
            }

            // When we acquire the consumer, also consume the pushed stack if the
            // cache is empty.
            let mut new_stack = stack | HAS_CACHE | IS_CONSUMING;
            if stack & HAS_CACHE == 0 {
                debug_assert!(stack & PTR_MASK != 0);
                new_stack &= !PTR_MASK;
            }

            // Acquire barrier on getting the consumer to see cache/Node updates
            // done by previous consumers.
            match self.stack.compare_exchange_weak(stack, new_stack, Acquire, Relaxed) {
                Ok(_) => {
                    let cache = unsafe { *self.cache.get() };
                    return Ok(cache.or_else(|| node_from_addr(stack & PTR_MASK)));
                }
                Err(e) => stack = e,
            }
        }
    }

    fn release_consumer(&self, consumer: Option<*mut Node>) {
        // Stop consuming and remove the HAS_CACHE bit as well if the consumer's
        // cache is empty.
        let mut remove = IS_CONSUMING;
        if consumer.is_none() {
            remove |= HAS_CACHE;
        }

        // Release the consumer with a release barrier to ensure cache/node
        // accesses happen before the consumer was released.
        unsafe { *self.cache.get() = consumer };
        let stack = self.stack.fetch_sub(remove, Release);
        debug_assert!(stack & remove != 0);
    }

    fn pop(&self, consumer_ref: &mut Option<*mut Node>) -> Option<*mut Node> {
        // Check the consumer cache (fast path)
        if let Some(node) = *consumer_ref {
            *consumer_ref = unsafe { (*node).next };
            return Some(node);
        }

        // Load the stack to see if there was anything pushed that we could grab.
        let mut stack = self.stack.load(Relaxed);
        debug_assert!(stack & IS_CONSUMING != 0);
        if stack & PTR_MASK == 0 {
            return None;
        }

        // Nodes have been pushed to the stack, grab them with an Acquire barrier
        // to see the Node links.
        stack = self.stack.swap(HAS_CACHE | IS_CONSUMING, Acquire);
        debug_assert!(stack & IS_CONSUMING != 0);
        debug_assert!(stack & PTR_MASK != 0);

        let node: *mut Node = core::ptr::with_exposed_provenance_mut(stack & PTR_MASK);
        *consumer_ref = unsafe { (*node).next };
        Some(node)
    }
}

// ---------------------------------------------------------------------------
// Node.Buffer — a bounded single-producer, multi-consumer ring for node ptrs.
// ---------------------------------------------------------------------------
type Index = u32;
const CAPACITY: usize = 8; // harness model of the real port's 256 (power of two)

struct Buffer {
    head: AtomicU32,
    tail: AtomicU32,
    array: [AtomicPtr<Node>; CAPACITY],
}

unsafe impl Sync for Buffer {}

impl Default for Buffer {
    fn default() -> Self {
        Buffer {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            array: [const { AtomicPtr::new(core::ptr::null_mut()) }; CAPACITY],
        }
    }
}

struct Stole {
    node: *mut Node,
    pushed: bool,
}

/// `error{Overflow}`
struct Overflow;

impl Buffer {
    fn push(&self, list: &mut List) -> Result<(), Overflow> {
        let mut head = self.head.load(Relaxed);
        let mut tail = self.tail.load(Relaxed); // we're the only thread that can change this

        loop {
            let mut size = tail.wrapping_sub(head);
            debug_assert!(size as usize <= CAPACITY);

            // Push nodes from the list to the buffer if it's not empty..
            if (size as usize) < CAPACITY {
                let mut nodes: Option<*mut Node> = Some(list.head);
                while (size as usize) < CAPACITY {
                    let node = match nodes {
                        Some(n) => n,
                        None => break,
                    };
                    nodes = unsafe { (*node).next };

                    // Array written atomically with weakest ordering since it
                    // could be getting atomically read by steal().
                    self.array[(tail as usize) % CAPACITY].store(node, Relaxed);
                    tail = tail.wrapping_add(1);
                    size = size.wrapping_add(1);
                }

                // Release barrier synchronizes with Acquire loads for steal()ers.
                self.tail.store(tail, Release);

                // Update the list with the nodes we pushed and try again if more.
                match nodes {
                    None => return Ok(()),
                    Some(h) => list.head = h,
                }
                std::hint::spin_loop();
                head = self.head.load(Relaxed);
                continue;
            }

            // Try to steal/overflow half of the tasks in the buffer to make room.
            // Acquire barrier to ensure the linked-list creation after the steal
            // only happens after we successfully steal.
            let mut migrate = size / 2;
            match self.head.compare_exchange_weak(head, head.wrapping_add(migrate), Acquire, Relaxed) {
                Ok(_) => {
                    // Link the migrated Nodes together
                    let first = self.array[(head as usize) % CAPACITY].load(Relaxed);
                    while migrate > 0 {
                        let prev = self.array[(head as usize) % CAPACITY].load(Relaxed);
                        head = head.wrapping_add(1);
                        unsafe {
                            (*prev).next = Some(self.array[(head as usize) % CAPACITY].load(Relaxed))
                        };
                        migrate -= 1;
                    }

                    // Append the list that was supposed to be pushed to the end
                    // of the migrated Nodes.
                    let last = self.array[(head.wrapping_sub(1) as usize) % CAPACITY].load(Relaxed);
                    unsafe { (*last).next = Some(list.head) };
                    unsafe { (*list.tail).next = None };

                    // Return the migrated nodes + the original list as overflowed.
                    list.head = first;
                    return Err(Overflow);
                }
                Err(e) => head = e,
            }
        }
    }

    fn pop(&self) -> Option<*mut Node> {
        let mut head = self.head.load(Relaxed);
        let tail = self.tail.load(Relaxed); // we're the only thread that can change this

        loop {
            // Quick sanity check and return null when not empty
            let size = tail.wrapping_sub(head);
            debug_assert!(size as usize <= CAPACITY);
            if size == 0 {
                return None;
            }

            // Dequeue with an acquire barrier to ensure any writes done to the
            // Node only happen after we successfully claim it from the array.
            match self.head.compare_exchange_weak(head, head.wrapping_add(1), Acquire, Relaxed) {
                Ok(_) => return Some(self.array[(head as usize) % CAPACITY].load(Relaxed)),
                Err(e) => head = e,
            }
        }
    }

    fn consume(&self, queue: &Queue) -> Option<Stole> {
        let mut consumer = match queue.try_acquire_consumer() {
            Ok(c) => c,
            Err(_) => return None,
        };
        // NOTE: Zig `defer queue.releaseConsumer(consumer)` runs with the FINAL
        // `consumer` value at each return; we call it explicitly before returning.

        let head = self.head.load(Relaxed);
        let tail = self.tail.load(Relaxed); // we're the only thread that can change this

        let size = tail.wrapping_sub(head);
        debug_assert!(size as usize <= CAPACITY);
        debug_assert!(size == 0); // we should only be consuming if our array is empty

        // Pop nodes from the queue and push them to our array.
        let mut pushed: Index = 0;
        while (pushed as usize) < CAPACITY {
            let node = match queue.pop(&mut consumer) {
                Some(n) => n,
                None => break,
            };
            self.array[(tail.wrapping_add(pushed) as usize) % CAPACITY].store(node, Relaxed);
            pushed += 1;
        }

        // We will be returning one node that we stole from the queue. Get an
        // extra, and if that's not possible, take one from our array.
        let node = match queue.pop(&mut consumer) {
            Some(n) => n,
            None => {
                if pushed == 0 {
                    queue.release_consumer(consumer);
                    return None;
                }
                pushed -= 1;
                self.array[(tail.wrapping_add(pushed) as usize) % CAPACITY].load(Relaxed)
            }
        };

        // Update the array tail with the nodes we pushed to it.
        if pushed > 0 {
            self.tail.store(tail.wrapping_add(pushed), Release);
        }
        queue.release_consumer(consumer);
        Some(Stole { node, pushed: pushed > 0 })
    }

    fn steal(&self, buffer: &Buffer) -> Option<Stole> {
        let head = self.head.load(Relaxed);
        let tail = self.tail.load(Relaxed); // we're the only thread that can change this

        let size = tail.wrapping_sub(head);
        debug_assert!(size as usize <= CAPACITY);
        debug_assert!(size == 0); // we should only be stealing if our array is empty

        loop {
            let buffer_head = buffer.head.load(Acquire);
            let buffer_tail = buffer.tail.load(Acquire);

            // Overly large size => the tail was updated a lot after the head was
            // loaded. Reload both and try again.
            let buffer_size = buffer_tail.wrapping_sub(buffer_head);
            if buffer_size as usize > CAPACITY {
                std::hint::spin_loop();
                continue;
            }

            // Try to steal half (divCeil) to amortize the cost of stealing.
            let steal_size = buffer_size - (buffer_size / 2);
            if steal_size == 0 {
                return None;
            }

            // Copy the nodes we will steal from the target's array to our own.
            let mut i: Index = 0;
            while i < steal_size {
                let node = buffer.array[(buffer_head.wrapping_add(i) as usize) % CAPACITY].load(Relaxed);
                self.array[(tail.wrapping_add(i) as usize) % CAPACITY].store(node, Relaxed);
                i += 1;
            }

            // Try to commit the steal from the target buffer.
            match buffer.head.compare_exchange(
                buffer_head,
                buffer_head.wrapping_add(steal_size),
                AcqRel,
                Relaxed,
            ) {
                Ok(_) => {
                    // Pop one from the nodes we stole as we'll be returning it.
                    let pushed = steal_size - 1;
                    let node = self.array[(tail.wrapping_add(pushed) as usize) % CAPACITY].load(Relaxed);

                    // Update the array tail with the nodes we pushed to it.
                    if pushed > 0 {
                        self.tail.store(tail.wrapping_add(pushed), Release);
                    }
                    return Some(Stole { node, pushed: pushed > 0 });
                }
                Err(_) => {
                    std::hint::spin_loop();
                    continue;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The multi-threaded hammer: K worker threads, each owning a Buffer + a Queue,
// plus one global Queue. Each worker seeds its own buffer with PER > CAPACITY
// nodes (forcing push overflow -> its queue), then all workers run the exact
// `Thread.pop` work-stealing loop (local buffer pop -> local queue consume ->
// global queue consume -> steal peers' queues/buffers) until every node has been
// dequeued exactly once.
// ---------------------------------------------------------------------------
struct Shared {
    global: Queue,
    buffers: Vec<Buffer>,
    queues: Vec<Queue>,
    nodes: Vec<*mut Node>,
    processed: Vec<AtomicU32>,
    index_map: HashMap<usize, usize>,
    processed_total: AtomicUsize,
    k: usize,
    per: usize,
}

unsafe impl Sync for Shared {}

fn run_hammer(k: usize, per: usize) {
    let n = k * per;

    // Nodes get their OWN heap allocations so a shared &Shared never freezes
    // their bytes (rule 3 above).
    let nodes: Vec<*mut Node> = (0..n)
        .map(|_| Box::into_raw(Box::new(Node::default())))
        .collect();
    let mut index_map: HashMap<usize, usize> = HashMap::with_capacity(n);
    for (idx, &p) in nodes.iter().enumerate() {
        index_map.insert(p as usize, idx);
    }
    let processed: Vec<AtomicU32> = (0..n).map(|_| AtomicU32::new(0)).collect();

    let shared = Shared {
        global: Queue::default(),
        buffers: (0..k).map(|_| Buffer::default()).collect(),
        queues: (0..k).map(|_| Queue::default()).collect(),
        nodes,
        processed,
        index_map,
        processed_total: AtomicUsize::new(0),
        k,
        per,
    };

    std::thread::scope(|scope| {
        for i in 0..k {
            let shared = &shared;
            scope.spawn(move || worker(shared, i, n));
        }
    });

    // Verify every node dequeued exactly once.
    for idx in 0..n {
        let c = shared.processed[idx].load(Relaxed);
        assert_eq!(c, 1, "node {idx} dequeued {c} times (expected exactly 1)");
    }
    assert_eq!(shared.processed_total.load(Relaxed), n);

    // Free the heap nodes.
    for &p in &shared.nodes {
        unsafe { drop(Box::from_raw(p)) };
    }
}

fn worker(shared: &Shared, i: usize, n: usize) {
    // --- Seed: schedule this worker's PER nodes onto its own buffer, spilling
    // the overflow (migrated + remaining) to its own queue — exactly like
    // ThreadPool.schedule's `run_buffer.push(&list) catch run_queue.push(list)`.
    let base = i * shared.per;
    for j in 0..shared.per {
        let node = shared.nodes[base + j];
        let next = if j + 1 < shared.per {
            Some(shared.nodes[base + j + 1])
        } else {
            None
        };
        unsafe { (*node).next = next };
    }
    let mut list = List {
        head: shared.nodes[base],
        tail: shared.nodes[base + shared.per - 1],
    };
    if shared.buffers[i].push(&mut list).is_err() {
        shared.queues[i].push(list);
    }

    // --- Drain: the faithful `Thread.pop` interleave.
    let mut guard: u64 = 0;
    loop {
        if shared.processed_total.load(Relaxed) >= n {
            break;
        }
        guard += 1;
        assert!(guard < 50_000_000, "worker {i} spun without draining — lost node / deadlock");

        let got: Option<*mut Node> = if let Some(node) = shared.buffers[i].pop() {
            Some(node)
        } else if let Some(s) = shared.buffers[i].consume(&shared.queues[i]) {
            Some(s.node)
        } else if let Some(s) = shared.buffers[i].consume(&shared.global) {
            Some(s.node)
        } else {
            let mut found = None;
            for off in 1..shared.k {
                let target = (i + off) % shared.k;
                // Steal from the peer's queue first (cheaper, like the pool).
                if let Some(s) = shared.buffers[i].consume(&shared.queues[target]) {
                    found = Some(s.node);
                    break;
                }
                // Then steal from the peer's buffer as a last resort.
                if let Some(s) = shared.buffers[i].steal(&shared.buffers[target]) {
                    found = Some(s.node);
                    break;
                }
            }
            found
        };

        match got {
            Some(node) => {
                let idx = *shared
                    .index_map
                    .get(&(node as usize))
                    .expect("dequeued an unknown node pointer");
                shared.processed[idx].fetch_add(1, Relaxed);
                shared.processed_total.fetch_add(1, Relaxed);
            }
            None => std::hint::spin_loop(),
        }
    }
}

fn main() {
    // A slightly larger drive for `cargo miri run` / native runs.
    run_hammer(3, 12);
    println!("mirihar-threadpool: push/pop/consume/steal hammer OK");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hammer_push_pop_steal() {
        // Kept small so Miri (default Stacked Borrows and -Zmiri-tree-borrows)
        // can finish: K=3 workers, PER=10 (>CAPACITY=8 => forces push overflow).
        run_hammer(3, 10);
    }

    #[test]
    fn single_thread_queue_roundtrip() {
        // A tiny deterministic sanity check of the queue FIFO-ish contract.
        run_hammer(1, 20);
    }
}
