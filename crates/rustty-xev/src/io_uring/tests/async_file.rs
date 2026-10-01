//! io_uring backend tests: the Async and File watchers.
//!
//! PORTS: libxev src/watcher/async.zig `AsyncTests` and src/watcher/file.zig
//! `FileTests`, monomorphized for io_uring.

use super::*;

// KAT — Async (eventfd) wakeup: wait on an `Async`, notify it from a spawned
// thread, and confirm the loop's `run(until_done)` returns with the wait
// callback fired. Proves the eventfd poll + drain on the live io_uring ring.
//
// PORT NOTE: the reference's `AsyncTests` (`watcher/async.zig`) are generic over
// the backend; this is the "async: notify" shape monomorphized for io_uring.
#[test]
fn io_uring_async_notify() {
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
    // the whole point of `Async`. `run(until_done)` parks in `io_uring_enter`
    // until the eventfd goes readable, so this asserts the real wakeup path
    // (joining the thread BEFORE running would only prove that an
    // already-readable eventfd completes, which the poll would do anyway).
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
// counter retains it). Monomorphized for io_uring (the macOS instantiation
// lives in async.rs).
#[test]
fn io_uring_async_notify_first() {
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
fn io_uring_async_batches_multiple_notifications() {
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

// -----------------------------------------------------------------------
// File watcher tests (watcher/file.zig `FileTests`). On io_uring every File
// read/write/pread/pwrite is a NATIVE SQE (no thread pool involved), so these
// are plain regular-file round-trips on the live ring — the io_uring twin of
// epoll.rs's pool-backed File tests.
// -----------------------------------------------------------------------

fn temp_file() -> (fd_t, std::path::PathBuf) {
    use std::os::unix::io::IntoRawFd;
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "rustty_xev_file_iouring_{}_{}",
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

fn open_read_fd(path: &std::path::Path) -> fd_t {
    use std::os::unix::io::IntoRawFd;
    std::fs::File::open(path).unwrap().into_raw_fd()
}

// test "read/write" (file.zig:596).
#[test]
fn io_uring_file_read_write() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

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
    std::fs::remove_file(&path).ok();
}

// test "pread/pwrite" (file.zig:671).
#[test]
fn io_uring_file_pread_pwrite() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

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
    std::fs::remove_file(&path).ok();
}

// test "queued writes" (file.zig:744).
#[test]
fn io_uring_file_queued_writes() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

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
    std::fs::remove_file(&path).ok();
}

// test "queuePWrite" (current pin file.zig:318; libxev 9ce8e8e6, absent from
// the tracked 34fa5087 oracle — codex W13A-02). Two File.queue_write requests
// ("1234" via req[0], "5678" via req[1]) followed by a File.queue_pwrite of
// "000" at offset 3 that REUSES req[1] — faithfully clobbering the still-queued
// "5678" request node exactly as the oracle does — then read back and pin the
// exact final bytes "123000".
#[test]
fn io_uring_file_queue_pwrite_kat() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

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
    std::fs::remove_file(&path).ok();
}

// test "poll" (file.zig:557). A pipe with a byte already written is poll-read
// ready.
#[test]
fn io_uring_file_poll() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

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
