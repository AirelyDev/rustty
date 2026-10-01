//! epoll backend tests: the Async watcher, the thread-pool close, and the File
//! watcher.
//!
//! PORTS: libxev src/watcher/async.zig `AsyncTests` and src/watcher/file.zig
//! `FileTests`, monomorphized for epoll, plus the thread-pool `.close` KAT the
//! File watcher rides.

use super::*;

// KAT — Async (eventfd) wakeup: wait on an `Async`, notify it from a spawned
// thread, and confirm the loop's `run(until_done)` returns with the wait
// callback fired. Proves the eventfd poll + drain on the live epoll loop.
//
// PORT NOTE: the reference's `AsyncTests` (`watcher/async.zig`) are generic over
// the backend; this is the "async: notify" shape monomorphized for epoll (the
// twin of `io_uring_async_notify`).
#[test]
fn epoll_async_notify() {
    let mut loop_ = Loop::init(Options::default()).unwrap();
    let mut notifier = Async::init().unwrap();

    let mut wake = false;
    let mut c_wait = Completion::default();
    let cb = |ud: *mut bool,
              _l: *mut Loop,
              _c: *mut Completion,
              r: core::result::Result<(), ReadError>| {
        r.unwrap();
        unsafe { *ud = true };
        CallbackAction::Disarm
    };
    unsafe {
        notifier.wait(
            &mut loop_ as *mut Loop,
            ptr::addr_of_mut!(c_wait),
            ptr::addr_of_mut!(wake),
            cb,
        );
    }

    // Not woken up before the notify.
    loop_.run(RunMode::NoWait).unwrap();
    assert!(!wake);

    // Notify from ANOTHER thread WHILE this one is BLOCKED inside the loop —
    // the whole point of `Async`. `run(until_done)` parks in `epoll_wait`
    // until the eventfd goes readable, so this asserts the real wakeup path.
    // Not racy: if the notify lands first the poll simply completes at once.
    let n = notifier; // `Async` is Copy (an fd handle) and Send (a plain i32).
    let t = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(20));
        n.notify().unwrap();
    });

    // Blocks here until the spawned thread notifies. A hang is a real bug.
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(wake);
    t.join().unwrap();

    // The eventfd counter was drained by the wait callback, so a second wait
    // does NOT fire spuriously.
    let mut wake2 = false;
    let mut c_wait2 = Completion::default();
    let cb2 = |ud: *mut bool,
               _l: *mut Loop,
               _c: *mut Completion,
               r: core::result::Result<(), ReadError>| {
        r.unwrap();
        unsafe { *ud = true };
        CallbackAction::Disarm
    };
    unsafe {
        notifier.wait(
            &mut loop_ as *mut Loop,
            ptr::addr_of_mut!(c_wait2),
            ptr::addr_of_mut!(wake2),
            cb2,
        );
    }
    loop_.run(RunMode::NoWait).unwrap();
    assert!(!wake2);

    notifier.deinit();
    loop_.deinit();
}

// test "async: notify first" (watcher/async.zig) — a notification sent
// BEFORE the wait is registered must still wake the waiter (the eventfd
// counter retains it). Monomorphized for epoll (the macOS instantiation
// lives in async.rs).
#[test]
fn epoll_async_notify_first() {
    let mut loop_ = Loop::init(Options::default()).unwrap();
    let mut notifier = Async::init().unwrap();

    // Send a notification
    notifier.notify().unwrap();

    // Wait
    let mut wake = false;
    let mut c_wait = Completion::default();
    let cb = |ud: *mut bool,
              _l: *mut Loop,
              _c: *mut Completion,
              r: core::result::Result<(), ReadError>| {
        r.unwrap(); // _ = r catch unreachable
        unsafe { *ud = true };
        CallbackAction::Disarm
    };
    unsafe {
        notifier.wait(
            &mut loop_ as *mut Loop,
            ptr::addr_of_mut!(c_wait),
            ptr::addr_of_mut!(wake),
            cb,
        );
    }

    // Wait for wake
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(wake);

    notifier.deinit();
    loop_.deinit();
}

// test "async batches multiple notifications" (watcher/async.zig) — N
// notifications before (5) and after (1) the wait is registered must
// coalesce into EXACTLY ONE callback (the eventfd read drains the counter),
// even across 10 extra `no_wait` ticks with a rearming waiter.
#[test]
fn epoll_async_batches_multiple_notifications() {
    let mut loop_ = Loop::init(Options::default()).unwrap();
    let mut notifier = Async::init().unwrap();

    // Send a notification many times
    notifier.notify().unwrap();
    notifier.notify().unwrap();
    notifier.notify().unwrap();
    notifier.notify().unwrap();
    notifier.notify().unwrap();

    // Wait
    let mut count: u32 = 0;
    let mut c_wait = Completion::default();
    let cb = |ud: *mut u32,
              _l: *mut Loop,
              _c: *mut Completion,
              r: core::result::Result<(), ReadError>| {
        r.unwrap(); // _ = r catch unreachable
        unsafe { *ud += 1 };
        CallbackAction::Rearm
    };
    unsafe {
        notifier.wait(
            &mut loop_ as *mut Loop,
            ptr::addr_of_mut!(c_wait),
            ptr::addr_of_mut!(count),
            cb,
        );
    }

    // Send a notification
    notifier.notify().unwrap();

    // Wait for wake
    loop_.run(RunMode::Once).unwrap();
    for _ in 0..10 {
        loop_.run(RunMode::NoWait).unwrap();
    }
    assert_eq!(count, 1);

    notifier.deinit();
    loop_.deinit();
}

// Task-2 KAT: a `.close` op flagged for the thread pool runs on the pool
// (`thread_schedule` -> worker `thread_perform` closes the fd + pushes onto
// `thread_pool_completions` + wakes the loop) and its callback fires when the
// loop drains the MPSC queue in `tick`. This exercises the whole epoll
// thread-pool path the File watcher (Task 5) rides. With NO pool attached the
// same flagged close yields `ThreadPoolRequired` (asserted in the second half).
#[test]
fn epoll_close_on_thread_pool() {
    use crate::thread_pool::Config;

    struct CloseState {
        fired: bool,
        ok: bool,
    }

    fn close_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
        let st = unsafe { &mut *(ud as *mut CloseState) };
        st.fired = true;
        st.ok = matches!(r, Result::Close(Ok(())));
        CallbackAction::Disarm
    }

    // --- With a real pool: the flagged close runs on the pool. ---
    let mut pool = ThreadPool::init(Config {
        stack_size: 0,
        max_threads: 1,
    });
    let pool_ptr: *mut ThreadPool = &mut pool;

    let mut loop_ = Loop::init(Options {
        thread_pool: Some(pool_ptr),
        ..Options::default()
    })
    .unwrap();

    // A pipe gives us a real fd to close.
    let mut fds = [0 as fd_t; 2];
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    let (read_fd, write_fd) = (fds[0], fds[1]);

    let mut st = CloseState {
        fired: false,
        ok: false,
    };
    let mut c = Completion::new_op(
        Operation::Close { fd: read_fd },
        ptr::addr_of_mut!(st) as *mut c_void,
        close_cb,
    );
    // Stream.close sets this bit; set it directly here to drive the pool path.
    unsafe { Completion::set_threadpool(ptr::addr_of_mut!(c), true) };
    unsafe { loop_.add(ptr::addr_of_mut!(c)) };

    // Blocks in epoll_wait until the pool worker wakes us; a hang is a real bug.
    loop_.run(RunMode::UntilDone).unwrap();

    assert!(
        st.fired,
        "close callback never fired — the thread-pool completion was not drained"
    );
    // `st.ok` (the pool-thread `perform` returned `Result::Close(Ok(()))`) is
    // the meaningful proof the blocking close ran on the pool AND that its
    // `task_result` round-tripped through `thread_pool_completions` intact — a
    // stale/never-written `task_result` would surface here as the default
    // `Noop` (or an `Err`). We deliberately do NOT re-close `read_fd` to prove
    // EBADF: fd numbers are per-process, and a concurrently-running test may
    // have already recycled this integer — the re-close would then race (or,
    // worse, close an unrelated test's fd).
    assert!(
        st.ok,
        "close on the pool did not deliver Result::Close(Ok(())) — task_result \
         was stale or an error"
    );
    let _ = read_fd;

    unsafe { libc::close(write_fd) };
    loop_.deinit();
    pool.shutdown();
    pool.deinit();

    // --- With NO pool: the same flagged close yields ThreadPoolRequired. ---
    let mut loop2 = Loop::init(Options::default()).unwrap();
    let mut fds2 = [0 as fd_t; 2];
    assert_eq!(unsafe { libc::pipe(fds2.as_mut_ptr()) }, 0);

    let mut got_required = false;
    fn required_cb(
        ud: *mut c_void,
        _l: *mut Loop,
        _c: *mut Completion,
        r: Result,
    ) -> CallbackAction {
        let flag = unsafe { &mut *(ud as *mut bool) };
        *flag = matches!(r, Result::Close(Err(CloseError::ThreadPoolRequired)));
        CallbackAction::Disarm
    }
    let mut c2 = Completion::new_op(
        Operation::Close { fd: fds2[0] },
        ptr::addr_of_mut!(got_required) as *mut c_void,
        required_cb,
    );
    unsafe { Completion::set_threadpool(ptr::addr_of_mut!(c2), true) };
    unsafe { loop2.add(ptr::addr_of_mut!(c2)) };
    loop2.run(RunMode::UntilDone).unwrap();
    assert!(
        got_required,
        "flagged close with no pool must yield ThreadPoolRequired"
    );

    unsafe {
        libc::close(fds2[0]);
        libc::close(fds2[1]);
    }
    loop2.deinit();
}

// -----------------------------------------------------------------------
// File watcher tests (watcher/file.zig `FileTests`). On epoll every File
// read/write/pread/pwrite is a BLOCKING op that rides the ThreadPool, so these
// exercise the whole epoll thread-pool round-trip (schedule -> worker
// `thread_perform` -> MPSC drain -> callback) AND the real multi-worker pool
// teardown at the end of each test (the exact path Task 1's deviation hardened
// — `Config::default()` resolves `max_threads = 0` to the host CPU count).
// -----------------------------------------------------------------------

// Creates a fresh empty temp file (read+write, truncated) and returns its raw
// fd + path. The caller closes the fd and removes the path.
fn temp_file() -> (fd_t, std::path::PathBuf) {
    use std::os::unix::io::IntoRawFd;
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "rustty_xev_file_epoll_{}_{}",
        std::process::id(),
        n
    ));
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .unwrap();
    (f.into_raw_fd(), path)
}

// Opens an existing path read-only and returns its raw fd (a fresh fd resets the
// file offset, exactly as the reference reopens `f2` before the read).
fn open_read_fd(path: &std::path::Path) -> fd_t {
    use std::os::unix::io::IntoRawFd;
    std::fs::File::open(path).unwrap().into_raw_fd()
}

// test "read/write" (file.zig:596). write a payload via File.write (on the
// pool), then read it back through a freshly-opened fd.
#[test]
fn epoll_file_read_write() {
    use crate::thread_pool::Config;

    let mut pool = ThreadPool::init(Config::default());
    let pool_ptr: *mut ThreadPool = &mut pool;
    let mut loop_ = Loop::init(Options {
        thread_pool: Some(pool_ptr),
        ..Options::default()
    })
    .unwrap();

    let (wfd, path) = temp_file();
    let file = File::init(wfd);

    let write_buf = [1u8, 1, 2, 3, 5, 8, 13];
    let mut c_write = Completion::default();
    let write_cb = |_ud: *mut (),
                    _l: *mut Loop,
                    _c: *mut Completion,
                    _s: File,
                    _b: WriteBuffer,
                    r: core::result::Result<usize, WriteError>| {
        assert_eq!(r.unwrap(), 7);
        CallbackAction::Disarm
    };
    unsafe {
        file.write(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_write),
            write_slice(&write_buf),
            ptr::null_mut::<()>(),
            write_cb,
        );
    }
    loop_.run(RunMode::UntilDone).unwrap();

    // Make sure the data is on disk.
    assert_eq!(unsafe { libc::fsync(wfd) }, 0);

    let rfd = open_read_fd(&path);
    let file2 = File::init(rfd);
    let mut read_buf = [0u8; 128];
    let mut read_len: usize = 0;
    let read_cb = |ud: *mut usize,
                   _l: *mut Loop,
                   _c: *mut Completion,
                   _s: File,
                   _b: ReadBuffer,
                   r: core::result::Result<usize, ReadError>| {
        unsafe { *ud = r.unwrap() };
        CallbackAction::Disarm
    };
    unsafe {
        file2.read(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_write),
            read_slice(&mut read_buf),
            ptr::addr_of_mut!(read_len),
            read_cb,
        );
    }
    loop_.run(RunMode::UntilDone).unwrap();
    assert_eq!(read_len, write_buf.len());
    assert_eq!(&write_buf, &read_buf[0..read_len]);

    unsafe {
        libc::close(wfd);
        libc::close(rfd);
    }
    loop_.deinit();
    pool.shutdown();
    pool.deinit();
    std::fs::remove_file(&path).ok();
}

// test "pread/pwrite" (file.zig:671). Same as read/write but with explicit
// offset 0 through File.pwrite / File.pread.
#[test]
fn epoll_file_pread_pwrite() {
    use crate::thread_pool::Config;

    let mut pool = ThreadPool::init(Config::default());
    let pool_ptr: *mut ThreadPool = &mut pool;
    let mut loop_ = Loop::init(Options {
        thread_pool: Some(pool_ptr),
        ..Options::default()
    })
    .unwrap();

    let (wfd, path) = temp_file();
    let file = File::init(wfd);

    let write_buf = [1u8, 1, 2, 3, 5, 8, 13];
    let mut c_write = Completion::default();
    let write_cb = |_ud: *mut (),
                    _l: *mut Loop,
                    _c: *mut Completion,
                    _s: File,
                    _b: WriteBuffer,
                    r: core::result::Result<usize, WriteError>| {
        assert_eq!(r.unwrap(), 7);
        CallbackAction::Disarm
    };
    unsafe {
        file.pwrite(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_write),
            write_slice(&write_buf),
            0,
            ptr::null_mut::<()>(),
            write_cb,
        );
    }
    loop_.run(RunMode::UntilDone).unwrap();

    assert_eq!(unsafe { libc::fsync(wfd) }, 0);

    let rfd = open_read_fd(&path);
    let file2 = File::init(rfd);
    let mut read_buf = [0u8; 128];
    let mut read_len: usize = 0;
    let read_cb = |ud: *mut usize,
                   _l: *mut Loop,
                   _c: *mut Completion,
                   _s: File,
                   _b: ReadBuffer,
                   r: core::result::Result<usize, ReadError>| {
        unsafe { *ud = r.unwrap() };
        CallbackAction::Disarm
    };
    unsafe {
        file2.pread(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_write),
            read_slice(&mut read_buf),
            0,
            ptr::addr_of_mut!(read_len),
            read_cb,
        );
    }
    loop_.run(RunMode::UntilDone).unwrap();
    assert_eq!(&write_buf, &read_buf[0..read_len]);

    unsafe {
        libc::close(wfd);
        libc::close(rfd);
    }
    loop_.deinit();
    pool.shutdown();
    pool.deinit();
    std::fs::remove_file(&path).ok();
}

// test "queued writes" (file.zig:744). Two ordered File.queue_write requests
// ("1234" then "5678"); read back "12345678".
#[test]
fn epoll_file_queued_writes() {
    use crate::thread_pool::Config;

    let mut pool = ThreadPool::init(Config::default());
    let pool_ptr: *mut ThreadPool = &mut pool;
    let mut loop_ = Loop::init(Options {
        thread_pool: Some(pool_ptr),
        ..Options::default()
    })
    .unwrap();

    let (wfd, path) = temp_file();
    let file = File::init(wfd);

    let mut write_queue: WriteQueue = WriteQueue::default();
    let mut write_req: [WriteRequest; 2] = [WriteRequest::default(), WriteRequest::default()];
    let write_cb = |_ud: *mut (),
                    _l: *mut Loop,
                    _c: *mut Completion,
                    _s: File,
                    _b: WriteBuffer,
                    r: core::result::Result<usize, WriteError>| {
        r.unwrap();
        CallbackAction::Disarm
    };
    unsafe {
        file.queue_write::<(), _>(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(write_queue),
            ptr::addr_of_mut!(write_req[0]),
            write_slice(b"1234"),
            ptr::null_mut(),
            write_cb,
        );
        file.queue_write::<(), _>(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(write_queue),
            ptr::addr_of_mut!(write_req[1]),
            write_slice(b"5678"),
            ptr::null_mut(),
            write_cb,
        );
    }
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(write_queue.empty());

    assert_eq!(unsafe { libc::fsync(wfd) }, 0);

    let rfd = open_read_fd(&path);
    let file2 = File::init(rfd);
    let mut read_buf = [0u8; 128];
    let mut read_len: usize = 0;
    let mut c_read = Completion::default();
    let read_cb = |ud: *mut usize,
                   _l: *mut Loop,
                   _c: *mut Completion,
                   _s: File,
                   _b: ReadBuffer,
                   r: core::result::Result<usize, ReadError>| {
        unsafe { *ud = r.unwrap() };
        CallbackAction::Disarm
    };
    unsafe {
        file2.read(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_read),
            read_slice(&mut read_buf),
            ptr::addr_of_mut!(read_len),
            read_cb,
        );
    }
    loop_.run(RunMode::UntilDone).unwrap();
    assert_eq!(b"12345678", &read_buf[0..read_len]);

    unsafe {
        libc::close(wfd);
        libc::close(rfd);
    }
    loop_.deinit();
    pool.shutdown();
    pool.deinit();
    std::fs::remove_file(&path).ok();
}

// test "queuePWrite" (current pin file.zig:318; libxev 9ce8e8e6, absent from
// the tracked 34fa5087 oracle — codex W13A-02). Two File.queue_write requests
// ("1234" via req[0], "5678" via req[1]) followed by a File.queue_pwrite of
// "000" at offset 3 that REUSES req[1] — faithfully clobbering the still-queued
// "5678" request node exactly as the oracle does — then read back and pin the
// exact final bytes "123000".
#[test]
fn epoll_file_queue_pwrite_kat() {
    use crate::thread_pool::Config;

    let mut pool = ThreadPool::init(Config::default());
    let pool_ptr: *mut ThreadPool = &mut pool;
    let mut loop_ = Loop::init(Options {
        thread_pool: Some(pool_ptr),
        ..Options::default()
    })
    .unwrap();

    let (wfd, path) = temp_file();
    let file = File::init(wfd);

    let mut write_queue: WriteQueue = WriteQueue::default();
    let mut write_req: [WriteRequest; 2] = [WriteRequest::default(), WriteRequest::default()];
    let write_cb = |_ud: *mut (),
                    _l: *mut Loop,
                    _c: *mut Completion,
                    _s: File,
                    _b: WriteBuffer,
                    r: core::result::Result<usize, WriteError>| {
        r.unwrap();
        CallbackAction::Disarm
    };
    unsafe {
        file.queue_write::<(), _>(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(write_queue),
            ptr::addr_of_mut!(write_req[0]),
            write_slice(b"1234"),
            ptr::null_mut(),
            write_cb,
        );
        file.queue_write::<(), _>(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(write_queue),
            ptr::addr_of_mut!(write_req[1]),
            write_slice(b"5678"),
            ptr::null_mut(),
            write_cb,
        );
        // Oracle reuse: &write_req[1] again — queue_pwrite re-inits the node
        // that is already queued for "5678", so that write never runs.
        file.queue_pwrite::<(), _>(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(write_queue),
            ptr::addr_of_mut!(write_req[1]),
            write_slice(b"000"),
            3,
            ptr::null_mut(),
            write_cb,
        );
    }
    loop_.run(RunMode::UntilDone).unwrap();

    // Make sure the data is on disk.
    assert_eq!(unsafe { libc::fsync(wfd) }, 0);

    let rfd = open_read_fd(&path);
    let file2 = File::init(rfd);
    let mut read_buf = [0u8; 128];
    let mut read_len: usize = 0;
    let mut c_read = Completion::default();
    let read_cb = |ud: *mut usize,
                   _l: *mut Loop,
                   _c: *mut Completion,
                   _s: File,
                   _b: ReadBuffer,
                   r: core::result::Result<usize, ReadError>| {
        unsafe { *ud = r.unwrap() };
        CallbackAction::Disarm
    };
    unsafe {
        file2.read(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_read),
            read_slice(&mut read_buf),
            ptr::addr_of_mut!(read_len),
            read_cb,
        );
    }
    loop_.run(RunMode::UntilDone).unwrap();
    assert_eq!(b"123000", &read_buf[0..read_len]);

    unsafe {
        libc::close(wfd);
        libc::close(rfd);
    }
    loop_.deinit();
    pool.shutdown();
    pool.deinit();
    std::fs::remove_file(&path).ok();
}

// test "poll" (file.zig:557). A pipe with a byte already written is poll-read
// ready. File.poll does NOT ride the pool (it registers with epoll directly),
// so no ThreadPool is attached here.
#[test]
fn epoll_file_poll() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

    // Create our pipe and write to it so it's ready to be read.
    let mut fds = [0 as fd_t; 2];
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    let (read_fd, write_fd) = (fds[0], fds[1]);
    assert_eq!(unsafe { libc::write(write_fd, b"x".as_ptr().cast(), 1) }, 1);

    let file = File::init_fd(read_fd);
    let mut c = Completion::default();
    let mut ready = false;
    let poll_cb = |ud: *mut bool,
                   _l: *mut Loop,
                   _c: *mut Completion,
                   _s: File,
                   r: core::result::Result<PollEvent, PollError>| {
        r.unwrap();
        unsafe { *ud = true };
        CallbackAction::Disarm
    };
    unsafe {
        file.poll(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c),
            PollEvent::Read,
            ptr::addr_of_mut!(ready),
            poll_cb,
        );
    }
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(ready);

    unsafe {
        libc::close(read_fd);
        libc::close(write_fd);
    }
    loop_.deinit();
}
