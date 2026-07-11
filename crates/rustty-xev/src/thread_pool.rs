//! thread_pool.rs — the TYPE SURFACE of libxev `ThreadPool.zig` (D6,
//! XEV_BLUEPRINT §1/§6 chunk C3).
//!
//! Thread pool copied almost directly from Zap[1]. In @kprotty's own words:
//! lock-free, allocation-free* (excluding spawning threads), supports batch
//! scheduling, and dynamically spawns threads while handling thread spawn
//! failure. I highly recommend reading @kprotty's incredible blog post[2] on
//! this topic.
//!
//! The original file in Zap is licensed under the MIT license, and the
//! license and copyright is reproduced below. The libxev project is also
//! MIT licensed so the entire project (including this file) are equally
//! licensed. This is just a convenience note for any OSS users, contributors,
//! etc.
//!
//! MIT License
//!
//! Copyright (c) 2021 kprotty
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.
//!
//! [1]: https://github.com/kprotty/zap
//! [2]: https://zig.news/kprotty/resource-efficient-thread-pools-with-zig-3291
//
// PORT NOTE: SCOPE [ADV-FIX / XEV_BLUEPRINT §1/§6]: ghostty calls `Loop.init(.{})` with
// NO `thread_pool` (termio/Thread.zig, renderer/Thread.zig) — the pool is
// reachable only via `flags.threadpool = true` (xev.File / explicit) which
// ghostty never sets. So ONLY the TYPE SURFACE (`Task`, `Batch.from`, `schedule`,
// `init`/`deinit`/`shutdown` signatures) that `kqueue.rs` names is ported here;
// the work-stealing BODY is deferred (DEFERRALS.md). The kqueue thread-pool
// branch is a faithfully-ported DEAD branch (never taken while the pool is None).

#![allow(dead_code)]

/// PORT NOTE: the `ThreadPool = @This()` root struct. Fields are the work-stealing
/// pool's internal state; the deferred BODY owns them, so they are held as an
/// opaque zero-information placeholder here (only the type NAME + the method
/// signatures kqueue.rs references are load-bearing in C3).
pub struct ThreadPool {
    // DEFERRED BODY (§6): stack_size/max_threads/sync/idle_event/join_event/
    // run_queue/threads — the Zap pool state. Ported when a consumer needs the
    // pool (xev.File); ghostty passes no pool.
    _deferred: (),
}

/// Configuration options for the thread pool.
/// TODO: add CPU core affinity?
#[derive(Default)]
pub struct Config {
    pub stack_size: u32,
    pub max_threads: u32,
}

impl ThreadPool {
    /// Statically initialize the thread pool using the configuration.
    //
    // PORT NOTE (type surface only): the reference computes `stack_size`/`max_threads`
    // defaults and returns the pool. The BODY is deferred (§6) — this signature
    // exists so the kqueue "file IO on thread pool" skip-stub test names it.
    pub fn init(_config: Config) -> ThreadPool {
        unimplemented!(
            "xev ThreadPool body deferred (XEV_BLUEPRINT §1/§6): type surface only in C3"
        )
    }

    /// Wait for a thread to call shutdown() on the thread pool and kill the
    /// worker threads.
    pub fn deinit(&mut self) {
        unimplemented!(
            "xev ThreadPool body deferred (XEV_BLUEPRINT §1/§6): type surface only in C3"
        )
    }

    /// Marks the thread pool as shutdown.
    pub fn shutdown(&mut self) {
        unimplemented!(
            "xev ThreadPool body deferred (XEV_BLUEPRINT §1/§6): type surface only in C3"
        )
    }

    /// Schedule a batch of tasks to be executed by some thread on the thread
    /// pool.
    pub fn schedule(&mut self, _batch: Batch) {
        unimplemented!(
            "xev ThreadPool body deferred (XEV_BLUEPRINT §1/§6): type surface only in C3"
        )
    }
}

/// A Task represents the unit of Work / Job / Execution that the ThreadPool
/// schedules. The user provides a `callback` which is invoked when the *Task can
/// run on a thread.
//
// PORT NOTE: `node: Node = .{}` (the Zap intrusive queue node) collapses to a
// deferred placeholder; `callback: *const fn (*Task) void` is the fn-pointer
// `kqueue.rs`'s `thread_perform` installs (in the dead thread-pool branch).
pub struct Task {
    pub node: Node,
    pub callback: fn(*mut Task),
}

// PORT NOTE: Rust glue — `Completion`'s `task: ThreadPool.Task` field is Zig
// `undefined` (set only when scheduled on the pool, which never happens while the
// pool is deferred). Rust has no `undefined`, so `Completion::default` needs a
// `Task::default`: a null-ish node + a noop callback that is never invoked (the
// pool body is deferred; the kqueue thread-pool branch is dead).
fn task_noop(_: *mut Task) {}

impl Default for Task {
    fn default() -> Self {
        Task {
            node: Node::default(),
            callback: task_noop,
        }
    }
}

/// The intrusive queue node embedded in every `Task` (Zap `Node`). Type surface
/// only — the deferred BODY owns its `next` linkage.
#[derive(Default)]
pub struct Node {
    _deferred: (),
}

/// An unordered collection of Tasks which can be submitted for scheduling as a
/// group.
pub struct Batch {
    pub len: usize,
    pub head: *mut Task,
    pub tail: *mut Task,
}

impl Batch {
    /// Create a batch from a single task.
    pub fn from(task: *mut Task) -> Batch {
        Batch {
            len: 1,
            head: task,
            tail: task,
        }
    }
}
