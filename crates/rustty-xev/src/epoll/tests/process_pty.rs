//! epoll backend tests: the process and pty live KATs.
//!
//! PORTS: libxev src/watcher/process.zig and src/watcher/stream.zig — the process
//! wait and pty stream tests, monomorphized for epoll, with the test-only `Pty`
//! helper.

use super::*;

// KAT — Process spawn+exit: posix_spawn `/bin/true`; Process.wait;
// run(until_done); assert exit status 0. Proves the pidfd poll + waitid reap
// on the live epoll loop.
#[test]
#[allow(clippy::zombie_processes)]
fn epoll_process_spawn_exit() {
    // posix_spawn /bin/true (a real process that exits 0).
    let mut pid: libc::pid_t = 0;
    let path = b"/bin/true\0";
    let mut argv: [*mut libc::c_char; 2] = [path.as_ptr() as *mut libc::c_char, ptr::null_mut()];
    let mut envp: [*mut libc::c_char; 1] = [ptr::null_mut()];
    let rc = unsafe {
        libc::posix_spawn(
            &mut pid,
            path.as_ptr() as *const libc::c_char,
            ptr::null(),
            ptr::null(),
            argv.as_mut_ptr() as *const *mut libc::c_char,
            envp.as_mut_ptr() as *const *mut libc::c_char,
        )
    };
    assert_eq!(rc, 0, "posix_spawn /bin/true");

    let mut loop_ = Loop::init(Options::default()).unwrap();
    let mut p = Process::init(pid).unwrap();

    // Wait
    let mut code: Option<u32> = None;
    let mut c_wait = Completion::default();
    let cb = |ud: *mut Option<u32>,
              _l: *mut Loop,
              _c: *mut Completion,
              r: core::result::Result<u32, ProcessWaitError>| {
        unsafe { *ud = Some(r.unwrap()) };
        CallbackAction::Disarm
    };
    unsafe {
        p.wait(
            &mut loop_ as *mut Loop,
            ptr::addr_of_mut!(c_wait),
            ptr::addr_of_mut!(code),
            cb,
        );
    }

    loop_.run(RunMode::UntilDone).unwrap();
    assert_eq!(code, Some(0), "exit status 0");

    p.deinit();
    loop_.deinit();
}

// test "process wait with non-zero exit code" (watcher/process.zig) — the
// reference instantiates `ProcessTests` inside `ProcessPidFd`, so this
// generic test runs per Linux backend upstream; monomorphized here for
// epoll. Asserts the pidfd waitid reap surfaces the REAL `si_status`, not
// just zero.
#[test]
#[allow(clippy::zombie_processes)]
fn epoll_process_wait_with_non_zero_exit_code() {
    // posix_spawn `sh -c "exit 42"` (the reference's argv_42).
    let mut pid: libc::pid_t = 0;
    let path = b"/bin/sh\0";
    let arg_c = b"-c\0";
    let arg_script = b"exit 42\0";
    let mut argv: [*mut libc::c_char; 4] = [
        path.as_ptr() as *mut libc::c_char,
        arg_c.as_ptr() as *mut libc::c_char,
        arg_script.as_ptr() as *mut libc::c_char,
        ptr::null_mut(),
    ];
    let mut envp: [*mut libc::c_char; 1] = [ptr::null_mut()];
    let rc = unsafe {
        libc::posix_spawn(
            &mut pid,
            path.as_ptr() as *const libc::c_char,
            ptr::null(),
            ptr::null(),
            argv.as_mut_ptr() as *const *mut libc::c_char,
            envp.as_mut_ptr() as *const *mut libc::c_char,
        )
    };
    assert_eq!(rc, 0, "posix_spawn sh -c 'exit 42'");

    let mut loop_ = Loop::init(Options::default()).unwrap();
    let mut p = Process::init(pid).unwrap();

    // Wait
    let mut code: Option<u32> = None;
    let mut c_wait = Completion::default();
    let cb = |ud: *mut Option<u32>,
              _l: *mut Loop,
              _c: *mut Completion,
              r: core::result::Result<u32, ProcessWaitError>| {
        unsafe { *ud = Some(r.unwrap()) }; // ud.?.* = r catch unreachable
        CallbackAction::Disarm
    };
    unsafe {
        p.wait(
            &mut loop_ as *mut Loop,
            ptr::addr_of_mut!(c_wait),
            ptr::addr_of_mut!(code),
            cb,
        );
    }

    loop_.run(RunMode::UntilDone).unwrap();
    assert_eq!(code, Some(42));

    p.deinit();
    loop_.deinit();
}

// test "process wait on a process that already exited" (watcher/process.zig)
// — Process::init FIRST (the pidfd must be opened while the child exists),
// then the child is reaped OUT FROM UNDER the watcher via waitpid; the
// subsequent loop wait exercises the ECHILD → `InvalidChild` path
// (`r catch 0` in the reference).
#[test]
fn epoll_process_wait_on_a_process_that_already_exited() {
    // posix_spawn /bin/true (the reference's argv_0 equivalent).
    let mut pid: libc::pid_t = 0;
    let path = b"/bin/true\0";
    let mut argv: [*mut libc::c_char; 2] = [path.as_ptr() as *mut libc::c_char, ptr::null_mut()];
    let mut envp: [*mut libc::c_char; 1] = [ptr::null_mut()];
    let rc = unsafe {
        libc::posix_spawn(
            &mut pid,
            path.as_ptr() as *const libc::c_char,
            ptr::null(),
            ptr::null(),
            argv.as_mut_ptr() as *const *mut libc::c_char,
            envp.as_mut_ptr() as *const *mut libc::c_char,
        )
    };
    assert_eq!(rc, 0, "posix_spawn /bin/true");

    let mut loop_ = Loop::init(Options::default()).unwrap();
    let mut p = Process::init(pid).unwrap();

    // _ = try child.wait();  — reap the child before the watcher does.
    let mut status: libc::c_int = 0;
    let reaped = unsafe { libc::waitpid(pid, &mut status, 0) };
    assert_eq!(reaped, pid, "waitpid must reap the child");

    // Wait
    let mut code: Option<u32> = None;
    let mut c_wait = Completion::default();
    let cb = |ud: *mut Option<u32>,
              _l: *mut Loop,
              _c: *mut Completion,
              r: core::result::Result<u32, ProcessWaitError>| {
        unsafe { *ud = Some(r.unwrap_or(0)) }; // ud.?.* = r catch 0
        CallbackAction::Disarm
    };
    unsafe {
        p.wait(
            &mut loop_ as *mut Loop,
            ptr::addr_of_mut!(c_wait),
            ptr::addr_of_mut!(code),
            cb,
        );
    }

    loop_.run(RunMode::UntilDone).unwrap();
    assert_eq!(code, Some(0));

    p.deinit();
    loop_.deinit();
}

// KAT — Pty write round-trip: openpty; Stream.init_fd(write side = parent);
// queue_write a payload; read it back on the other end (child); assert bytes +
// WriteQueue drains. Proves the ordered-write engine on the live epoll loop.
#[test]
fn epoll_pty_write_roundtrip() {
    let mut pty = Pty::init().unwrap();
    let mut loop_ = Loop::init(Options::default()).unwrap();

    let parent = Stream::init_fd(pty.parent); // write side
    let child = Stream::init_fd(pty.child); // read side

    // Read back on the child end.
    let mut read_buf = [0u8; 128];
    let mut read_len: Option<usize> = None;
    let mut c_read = Completion::default();
    let read_cb = |ud: *mut Option<usize>,
                   _l: *mut Loop,
                   _c: *mut Completion,
                   _s: Stream,
                   _b: ReadBuffer,
                   r: core::result::Result<usize, ReadError>| {
        unsafe { *ud = Some(r.unwrap()) };
        CallbackAction::Disarm
    };
    unsafe {
        child.read(
            &mut loop_ as *mut Loop,
            ptr::addr_of_mut!(c_read),
            read_slice(&mut read_buf),
            ptr::addr_of_mut!(read_len),
            read_cb,
        );
    }
    loop_.run(RunMode::NoWait).unwrap();
    assert!(read_len.is_none());

    // queue_write the payload on the parent (write) end.
    let mut write_queue: WriteQueue = WriteQueue::default();
    let mut write_req = WriteRequest::default();
    let payload = b"KAT payload\n";
    let write_cb = |_ud: *mut (),
                    _l: *mut Loop,
                    _c: *mut Completion,
                    _s: Stream,
                    _b: WriteBuffer,
                    r: core::result::Result<usize, WriteError>| {
        assert_eq!(r.unwrap(), b"KAT payload\n".len());
        CallbackAction::Disarm
    };
    unsafe {
        parent.queue_write::<(), _>(
            &mut loop_ as *mut Loop,
            ptr::addr_of_mut!(write_queue),
            ptr::addr_of_mut!(write_req),
            write_slice(payload),
            ptr::null_mut(),
            write_cb,
        );
    }

    // The queue is non-empty until the write drains it.
    assert!(!write_queue.empty());

    loop_.run(RunMode::UntilDone).unwrap();
    assert!(read_len.is_some());
    assert_eq!(payload, &read_buf[0..read_len.unwrap()]);

    // The WriteQueue drains (the single request was popped by the trampoline).
    assert!(write_queue.empty());

    pty.deinit();
    loop_.deinit();
}

// test "pty: child to parent" (watcher/stream.zig) — direct `Stream::write`
// on the child end, read on the parent end. The reference's `StreamTests`
// are generic over the backend; monomorphized here for epoll (the macOS
// instantiation lives in stream.rs).
#[test]
fn epoll_pty_child_to_parent() {
    // Create the pty parent/child side.
    let mut pty = Pty::init().unwrap();
    let mut loop_ = Loop::init(Options::default()).unwrap();

    let parent = Stream::init_fd(pty.parent);
    let child = Stream::init_fd(pty.child);

    // Read
    let mut read_buf = [0u8; 128];
    let mut read_len: Option<usize> = None;
    let mut c_read = Completion::default();
    let read_cb = |ud: *mut Option<usize>,
                   _l: *mut Loop,
                   _c: *mut Completion,
                   _s: Stream,
                   _b: ReadBuffer,
                   r: core::result::Result<usize, ReadError>| {
        unsafe { *ud = Some(r.unwrap()) }; // ud.?.* = r catch unreachable
        CallbackAction::Disarm
    };
    unsafe {
        parent.read(
            &mut loop_ as *mut Loop,
            ptr::addr_of_mut!(c_read),
            read_slice(&mut read_buf),
            ptr::addr_of_mut!(read_len),
            read_cb,
        );
    }

    // This should not block!
    loop_.run(RunMode::NoWait).unwrap();
    assert!(read_len.is_none());

    // Send
    let send_buf = b"hello, world!";
    let mut c_write = Completion::default();
    let write_cb = |_ud: *mut (),
                    _l: *mut Loop,
                    _c: *mut Completion,
                    _s: Stream,
                    _b: WriteBuffer,
                    r: core::result::Result<usize, WriteError>| {
        r.unwrap(); // _ = r catch unreachable
        CallbackAction::Disarm
    };
    unsafe {
        child.write::<(), _>(
            &mut loop_ as *mut Loop,
            ptr::addr_of_mut!(c_write),
            write_slice(send_buf),
            ptr::null_mut(),
            write_cb,
        );
    }

    // The write and read should trigger
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(read_len.is_some());
    assert_eq!(send_buf, &read_buf[0..read_len.unwrap()]);

    pty.deinit();
    loop_.deinit();
}

// test "pty: parent to child" (watcher/stream.zig) — direct `Stream::write`
// on the parent end, read on the child end.
#[test]
fn epoll_pty_parent_to_child() {
    // Create the pty parent/child side.
    let mut pty = Pty::init().unwrap();
    let mut loop_ = Loop::init(Options::default()).unwrap();

    let parent = Stream::init_fd(pty.parent);
    let child = Stream::init_fd(pty.child);

    // Read
    let mut read_buf = [0u8; 128];
    let mut read_len: Option<usize> = None;
    let mut c_read = Completion::default();
    let read_cb = |ud: *mut Option<usize>,
                   _l: *mut Loop,
                   _c: *mut Completion,
                   _s: Stream,
                   _b: ReadBuffer,
                   r: core::result::Result<usize, ReadError>| {
        unsafe { *ud = Some(r.unwrap()) };
        CallbackAction::Disarm
    };
    unsafe {
        child.read(
            &mut loop_ as *mut Loop,
            ptr::addr_of_mut!(c_read),
            read_slice(&mut read_buf),
            ptr::addr_of_mut!(read_len),
            read_cb,
        );
    }

    // This should not block!
    loop_.run(RunMode::NoWait).unwrap();
    assert!(read_len.is_none());

    // Send (note the newline at the end of the buf is important
    // since we're in cooked mode)
    let send_buf = b"hello, world!\n";
    let mut c_write = Completion::default();
    let write_cb = |_ud: *mut (),
                    _l: *mut Loop,
                    _c: *mut Completion,
                    _s: Stream,
                    _b: WriteBuffer,
                    r: core::result::Result<usize, WriteError>| {
        r.unwrap();
        CallbackAction::Disarm
    };
    unsafe {
        parent.write::<(), _>(
            &mut loop_ as *mut Loop,
            ptr::addr_of_mut!(c_write),
            write_slice(send_buf),
            ptr::null_mut(),
            write_cb,
        );
    }

    // The write and read should trigger
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(read_len.is_some());
    assert_eq!(send_buf, &read_buf[0..read_len.unwrap()]);

    pty.deinit();
    loop_.deinit();
}

// test "pty: queued writes" (watcher/stream.zig) — TWO `queue_write`
// requests must drain FIFO ("hello, " then "world!\n" concatenate), and the
// completion handed to the second callback must map back to ITS
// `WriteRequest` (`WriteRequest.from(c_result) == &write_req[1]`; the
// reference guards that assert with `!xev.dynamic`, which holds for this
// concrete-backend instantiation).
#[test]
fn epoll_pty_queued_writes() {
    // Create the pty parent/child side.
    let mut pty = Pty::init().unwrap();
    let mut loop_ = Loop::init(Options::default()).unwrap();

    let parent = Stream::init_fd(pty.parent);
    let child = Stream::init_fd(pty.child);

    // Read
    let mut read_buf = [0u8; 128];
    let mut read_len: Option<usize> = None;
    let mut c_read = Completion::default();
    let read_cb = |ud: *mut Option<usize>,
                   _l: *mut Loop,
                   _c: *mut Completion,
                   _s: Stream,
                   _b: ReadBuffer,
                   r: core::result::Result<usize, ReadError>| {
        unsafe { *ud = Some(r.unwrap()) };
        CallbackAction::Disarm
    };
    unsafe {
        child.read(
            &mut loop_ as *mut Loop,
            ptr::addr_of_mut!(c_read),
            read_slice(&mut read_buf),
            ptr::addr_of_mut!(read_len),
            read_cb,
        );
    }

    // This should not block!
    loop_.run(RunMode::NoWait).unwrap();
    assert!(read_len.is_none());

    let mut write_queue: WriteQueue = WriteQueue::default();
    let mut write_req: [WriteRequest; 2] = [WriteRequest::default(), WriteRequest::default()];

    // Send (note the newline at the end of the buf is important
    // since we're in cooked mode)
    let hello_cb = |_ud: *mut (),
                    _l: *mut Loop,
                    _c: *mut Completion,
                    _s: Stream,
                    _b: WriteBuffer,
                    r: core::result::Result<usize, WriteError>| {
        r.unwrap();
        CallbackAction::Disarm
    };
    unsafe {
        parent.queue_write::<(), _>(
            &mut loop_ as *mut Loop,
            ptr::addr_of_mut!(write_queue),
            ptr::addr_of_mut!(write_req[0]),
            write_slice(b"hello, "),
            ptr::null_mut(),
            hello_cb,
        );
    }

    let mut c_result: Option<*mut Completion> = None;
    let world_cb = |ud: *mut Option<*mut Completion>,
                    _l: *mut Loop,
                    c: *mut Completion,
                    _s: Stream,
                    _b: WriteBuffer,
                    r: core::result::Result<usize, WriteError>| {
        r.unwrap();
        unsafe { *ud = Some(c) }; // ud.?.* = c
        CallbackAction::Disarm
    };
    unsafe {
        parent.queue_write(
            &mut loop_ as *mut Loop,
            ptr::addr_of_mut!(write_queue),
            ptr::addr_of_mut!(write_req[1]),
            write_slice(b"world!\n"),
            ptr::addr_of_mut!(c_result),
            world_cb,
        );
    }

    // The write and read should trigger
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(read_len.is_some());
    assert_eq!(b"hello, world!\n", &read_buf[0..read_len.unwrap()]);

    // Verify our completion is equal to our request
    // (`!xev.dynamic` holds here.)
    assert_eq!(
        WriteRequest::from(c_result.unwrap()),
        ptr::addr_of_mut!(write_req[1])
    );

    pty.deinit();
    loop_.deinit();
}

// Helper to open a pty (test-only), mirroring watcher/stream.zig's `Pty`.
struct Pty {
    parent: fd_t,
    child: fd_t,
}

#[repr(C)]
struct Winsize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

extern "C" {
    fn openpty(
        parent: *mut fd_t,
        child: *mut fd_t,
        name: *mut u8,
        termios: *const c_void,
        winsize: *const Winsize,
    ) -> libc::c_int;
}

impl Pty {
    fn init() -> core::result::Result<Pty, ()> {
        let size = Winsize {
            ws_row: 80,
            ws_col: 80,
            ws_xpixel: 800,
            ws_ypixel: 600,
        };
        let mut parent_fd: fd_t = 0;
        let mut child_fd: fd_t = 0;
        if unsafe {
            openpty(
                ptr::addr_of_mut!(parent_fd),
                ptr::addr_of_mut!(child_fd),
                ptr::null_mut(),
                ptr::null(),
                &size,
            )
        } < 0
        {
            return Err(());
        }
        Ok(Pty {
            parent: parent_fd,
            child: child_fd,
        })
    }

    fn deinit(&mut self) {
        unsafe {
            libc::close(self.parent);
            libc::close(self.child);
        }
    }
}
