//! epoll backend tests: the socket family.
//!
//! PORTS: libxev src/backend/epoll.zig — the socket accept/connect/send/recv/close
//! `test{}` block, plus the TCP and UDP watcher tests (src/watcher/tcp.zig
//! `TCPTests`, src/watcher/udp.zig `UDPTests`).

use super::*;

// -----------------------------------------------------------------------
// Task 4 live KATs (socket / process / stream). These run REAL syscalls on
// the live epoll loop (Miri can't), mirroring the kqueue KATs 3/4.
// -----------------------------------------------------------------------

use crate::linux::net;
use std::sync::Mutex;

// Serializes the fixed-port socket test against rapid cargo re-runs.
static SOCKET_TEST_LOCK: Mutex<()> = Mutex::new(());

// PORT NOTE: `posix.socket(family, ty, flags)`; the socket test creates a
// blocking listener + a NONBLOCK client (SOCK.CLOEXEC on both).
unsafe fn new_socket(family: libc::c_int, nonblock: bool) -> socket_t {
    let mut ty = libc::SOCK_STREAM | libc::SOCK_CLOEXEC;
    if nonblock {
        ty |= libc::SOCK_NONBLOCK;
    }
    let fd = unsafe { libc::socket(family, ty, 0) };
    assert!(fd >= 0);
    fd
}

// test "epoll: socket accept/connect/send/recv/close" (epoll.zig:1672)
#[test]
fn epoll_socket_roundtrip() {
    let _guard = SOCKET_TEST_LOCK.lock().unwrap();
    let mut loop_ = Loop::init(Options::default()).unwrap();

    // Create a TCP server socket
    let address = net::Address::parse_ip4(b"127.0.0.1", 3131).unwrap();
    let kernel_backlog = 1;
    let family = unsafe { address.any.family } as libc::c_int;
    let ln = unsafe { new_socket(family, false) };
    let one: libc::c_int = 1;
    unsafe {
        libc::setsockopt(
            ln,
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            ptr::addr_of!(one).cast(),
            size_of::<libc::c_int>() as net::socklen_t,
        );
        libc::setsockopt(
            ln,
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            ptr::addr_of!(one).cast(),
            size_of::<libc::c_int>() as net::socklen_t,
        );
        assert!(
            libc::bind(
                ln,
                ptr::addr_of!(address.any) as *const libc::sockaddr,
                address.get_os_sock_len()
            ) == 0
        );
        assert!(libc::listen(ln, kernel_backlog) == 0);
    }

    // Create a TCP client socket
    let mut client_conn = unsafe { new_socket(family, true) };

    // Accept
    let mut server_conn: socket_t = 0;
    fn accept_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
        let conn = ud as *mut socket_t;
        match r {
            Result::Accept(Ok(v)) => unsafe { *conn = v },
            _ => unreachable!(),
        }
        CallbackAction::Disarm
    }
    let mut c_accept = Completion {
        op: Operation::Accept {
            socket: ln,
            addr: unsafe { core::mem::zeroed() },
            addr_size: size_of::<net::sockaddr::Any>() as net::socklen_t,
            flags: SOCK_CLOEXEC,
        },
        userdata: ptr::addr_of_mut!(server_conn).cast::<c_void>(),
        callback: accept_cb,
        ..Completion::default()
    };
    unsafe { loop_.add(ptr::addr_of_mut!(c_accept)) };

    // Connect
    let mut connected = false;
    fn connect_cb(
        ud: *mut c_void,
        _l: *mut Loop,
        _c: *mut Completion,
        r: Result,
    ) -> CallbackAction {
        match r {
            Result::Connect(Ok(())) => {}
            _ => unreachable!(),
        }
        unsafe { *(ud as *mut bool) = true };
        CallbackAction::Disarm
    }
    let mut c_connect = Completion {
        op: Operation::Connect {
            socket: client_conn,
            addr: address,
        },
        userdata: ptr::addr_of_mut!(connected).cast::<c_void>(),
        callback: connect_cb,
        ..Completion::default()
    };
    unsafe { loop_.add(ptr::addr_of_mut!(c_connect)) };

    // Wait for the connection to be established
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(server_conn > 0);
    assert!(connected);

    // Send
    let send_buf = [1u8, 1, 2, 3, 5, 8, 13];
    fn send_cb(_ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
        match r {
            Result::Send(Ok(_)) => {}
            _ => unreachable!(),
        }
        CallbackAction::Disarm
    }
    let mut c_send = Completion {
        op: Operation::Send {
            fd: client_conn,
            buffer: write_slice(&send_buf),
        },
        callback: send_cb,
        ..Completion::default()
    };
    unsafe { loop_.add(ptr::addr_of_mut!(c_send)) };

    // Receive
    let mut recv_buf = [0u8; 128];
    let mut recv_len: usize = 0;
    fn recv_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
        let ptr = ud as *mut usize;
        match r {
            Result::Recv(Ok(n)) => unsafe { *ptr = n },
            _ => unreachable!(),
        }
        CallbackAction::Disarm
    }
    let mut c_recv = Completion {
        op: Operation::Recv {
            fd: server_conn,
            buffer: read_slice(&mut recv_buf),
        },
        userdata: ptr::addr_of_mut!(recv_len).cast::<c_void>(),
        callback: recv_cb,
        ..Completion::default()
    };
    unsafe { loop_.add(ptr::addr_of_mut!(c_recv)) };

    // Wait for the send/receive
    loop_.run(RunMode::UntilDone).unwrap();
    assert_eq!(send_buf.as_slice(), &recv_buf[0..recv_len]);

    // Shutdown
    let mut shutdown = false;
    fn shutdown_cb(
        ud: *mut c_void,
        _l: *mut Loop,
        _c: *mut Completion,
        r: Result,
    ) -> CallbackAction {
        match r {
            Result::Shutdown(Ok(())) => {}
            _ => unreachable!(),
        }
        unsafe { *(ud as *mut bool) = true };
        CallbackAction::Disarm
    }
    let mut c_client_shutdown = Completion {
        op: Operation::Shutdown {
            socket: client_conn,
            how: ShutdownHow::Both,
        },
        userdata: ptr::addr_of_mut!(shutdown).cast::<c_void>(),
        callback: shutdown_cb,
        ..Completion::default()
    };
    unsafe { loop_.add(ptr::addr_of_mut!(c_client_shutdown)) };
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(shutdown);

    // Read should be EOF
    let mut eof: Option<bool> = None;
    fn eof_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
        let ptr = ud as *mut Option<bool>;
        let v = match r {
            Result::Recv(Ok(_)) => false,
            Result::Recv(Err(ReadError::EOF)) => true,
            Result::Recv(Err(_)) => false,
            _ => unreachable!(),
        };
        unsafe { *ptr = Some(v) };
        CallbackAction::Disarm
    }
    c_recv = Completion {
        op: Operation::Recv {
            fd: server_conn,
            buffer: read_slice(&mut recv_buf),
        },
        userdata: ptr::addr_of_mut!(eof).cast::<c_void>(),
        callback: eof_cb,
        ..Completion::default()
    };
    unsafe { loop_.add(ptr::addr_of_mut!(c_recv)) };

    loop_.run(RunMode::UntilDone).unwrap();
    assert!(eof.unwrap());

    // Close
    fn close_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
        match r {
            Result::Close(Ok(())) => {}
            _ => unreachable!(),
        }
        unsafe { *(ud as *mut socket_t) = 0 };
        CallbackAction::Disarm
    }
    let mut c_client_close = Completion {
        op: Operation::Close { fd: client_conn },
        userdata: ptr::addr_of_mut!(client_conn).cast::<c_void>(),
        callback: close_cb,
        ..Completion::default()
    };
    unsafe { loop_.add(ptr::addr_of_mut!(c_client_close)) };

    let mut ln_var = ln;
    let mut c_server_close = Completion {
        op: Operation::Close { fd: ln },
        userdata: ptr::addr_of_mut!(ln_var).cast::<c_void>(),
        callback: close_cb,
        ..Completion::default()
    };
    unsafe { loop_.add(ptr::addr_of_mut!(c_server_close)) };

    // Wait for the sockets to close
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(ln_var == 0);
    assert!(client_conn == 0);

    loop_.deinit();
}

// -----------------------------------------------------------------------
// TCP watcher tests (watcher/tcp.zig `TCPTests`). Ported for the epoll
// backend instantiation. `TCPTests` runs against `TCPStream(xev)` +
// `TCPDynamic(xev)`; the per-backend split gives that coverage here (epoll)
// and in `io_uring.rs` (io_uring), with the Dynamic wrapper covered by the
// crate-level `dynamic` tests. The `test "TCP: Stream decls"` reflection test
// (tcp.zig:506) is a Zig comptime `@hasDecl` sweep with no Rust runtime
// analog — it is enforced at COMPILE time by `TCP` carrying every
// `read`/`write`/`queue_write`/`close`/`poll` method, so it is not a
// separate runtime test here.
// -----------------------------------------------------------------------

// test "TCP: accept/connect/send/recv/close" (tcp.zig:518). The epoll TCP
// `close` rides the thread pool (the `.threadpool` flag), so the loop is
// built with one — exactly as the reference builds `Loop.init(.{
// .thread_pool = &tpool })`.
#[test]
fn epoll_tcp_accept_connect_send_recv_close() {
    use crate::thread_pool::{Config, ThreadPool};

    let mut pool = ThreadPool::init(Config::default());
    let pool_ptr: *mut ThreadPool = &mut pool;
    let mut loop_ = Loop::init(Options {
        thread_pool: Some(pool_ptr),
        ..Options::default()
    })
    .unwrap();

    // Choose random available port (Zig #14907)
    let mut address = net::Address::parse_ip4(b"127.0.0.1", 0).unwrap();
    let server = TCP::init(address).unwrap();

    // Bind and listen
    server.bind(address).unwrap();
    server.listen(1).unwrap();

    // Retrieve bound port and initialize client
    let mut sock_len = address.get_os_sock_len();
    unsafe {
        libc::getsockname(
            server.fd(),
            ptr::addr_of_mut!(address.any) as *mut libc::sockaddr,
            ptr::addr_of_mut!(sock_len),
        );
    }
    let client = TCP::init(address).unwrap();

    let mut c_accept = Completion::default();
    let mut c_connect = Completion::default();

    // Accept
    let mut server_conn: Option<TCP> = None;
    unsafe {
        server.accept(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_accept),
            ptr::addr_of_mut!(server_conn),
            |ud: *mut Option<TCP>,
             _l: *mut Loop,
             _c: *mut Completion,
             r: core::result::Result<TCP, AcceptError>| {
                *ud = Some(r.unwrap());
                CallbackAction::Disarm
            },
        );
    }

    // Connect
    let mut connected = false;
    unsafe {
        client.connect(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_connect),
            address,
            ptr::addr_of_mut!(connected),
            |ud: *mut bool,
             _l: *mut Loop,
             _c: *mut Completion,
             _s: TCP,
             r: core::result::Result<(), ConnectError>| {
                r.unwrap();
                *ud = true;
                CallbackAction::Disarm
            },
        );
    }

    // Wait for the connection to be established
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(server_conn.is_some());
    assert!(connected);

    // Close the server
    let mut server_closed = false;
    unsafe {
        server.close(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_accept),
            ptr::addr_of_mut!(server_closed),
            |ud: *mut bool,
             _l: *mut Loop,
             _c: *mut Completion,
             _s: TCP,
             r: core::result::Result<(), CloseError>| {
                r.unwrap();
                *ud = true;
                CallbackAction::Disarm
            },
        );
    }
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(server_closed);

    // Send
    let send_buf = [1u8, 1, 2, 3, 5, 8, 13];
    unsafe {
        client.write(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_connect),
            write_slice(&send_buf),
            ptr::null_mut::<()>(),
            |_ud: *mut (),
             _l: *mut Loop,
             _c: *mut Completion,
             _s: TCP,
             _b: WriteBuffer,
             r: core::result::Result<usize, WriteError>| {
                r.unwrap();
                CallbackAction::Disarm
            },
        );
    }

    // Receive
    let mut recv_buf = [0u8; 128];
    let mut recv_len: usize = 0;
    let server_conn_tcp = server_conn.unwrap();
    unsafe {
        server_conn_tcp.read(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_accept),
            read_slice(&mut recv_buf),
            ptr::addr_of_mut!(recv_len),
            |ud: *mut usize,
             _l: *mut Loop,
             _c: *mut Completion,
             _s: TCP,
             _b: ReadBuffer,
             r: core::result::Result<usize, ReadError>| {
                *ud = r.unwrap();
                CallbackAction::Disarm
            },
        );
    }

    // Wait for the send/receive
    loop_.run(RunMode::UntilDone).unwrap();
    assert_eq!(send_buf.as_slice(), &recv_buf[0..recv_len]);

    // Close
    unsafe {
        server_conn_tcp.close(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_accept),
            ptr::addr_of_mut!(server_conn),
            |ud: *mut Option<TCP>,
             _l: *mut Loop,
             _c: *mut Completion,
             _s: TCP,
             r: core::result::Result<(), CloseError>| {
                r.unwrap();
                *ud = None;
                CallbackAction::Disarm
            },
        );
        client.close(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_connect),
            ptr::addr_of_mut!(connected),
            |ud: *mut bool,
             _l: *mut Loop,
             _c: *mut Completion,
             _s: TCP,
             r: core::result::Result<(), CloseError>| {
                r.unwrap();
                *ud = false;
                CallbackAction::Disarm
            },
        );
    }

    loop_.run(RunMode::UntilDone).unwrap();
    assert!(server_conn.is_none());
    assert!(!connected);
    assert!(server_closed);

    loop_.deinit();
    pool.shutdown();
    pool.deinit();
}

// test "TCP: Queued writes" (tcp.zig:697). Limits SO_SNDBUF to force a
// partial unqueued write, then drains the remainder through the ordered-
// write engine (`queue_write`), shutting the writer down inside the queued
// callback; a re-arming receiver reads until the full 1 MB is collected.
//
// Potentially flaky (per the reference): could hang if the sender can't
// flush or the receiver over-reads.
#[test]
fn epoll_tcp_queued_writes() {
    use crate::thread_pool::{Config, ThreadPool};

    // send_buf = [_]u8{ 1..9, 0 } ** 100_000 (1 MB).
    const PATTERN: [u8; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 0];
    const SEND_LEN: usize = PATTERN.len() * 100_000;
    let mut send_buf: Vec<u8> = Vec::with_capacity(SEND_LEN);
    for _ in 0..100_000 {
        send_buf.extend_from_slice(&PATTERN);
    }

    let mut pool = ThreadPool::init(Config::default());
    let pool_ptr: *mut ThreadPool = &mut pool;
    let mut loop_ = Loop::init(Options {
        thread_pool: Some(pool_ptr),
        ..Options::default()
    })
    .unwrap();

    let mut address = net::Address::parse_ip4(b"127.0.0.1", 0).unwrap();
    let server = TCP::init(address).unwrap();
    server.bind(address).unwrap();
    server.listen(1).unwrap();

    let mut sock_len = address.get_os_sock_len();
    unsafe {
        libc::getsockname(
            server.fd(),
            ptr::addr_of_mut!(address.any) as *mut libc::sockaddr,
            ptr::addr_of_mut!(sock_len),
        );
    }
    let client = TCP::init(address).unwrap();

    let mut c_accept = Completion::default();
    let mut c_connect = Completion::default();

    // Accept
    let mut server_conn: Option<TCP> = None;
    unsafe {
        server.accept(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_accept),
            ptr::addr_of_mut!(server_conn),
            |ud: *mut Option<TCP>,
             _l: *mut Loop,
             _c: *mut Completion,
             r: core::result::Result<TCP, AcceptError>| {
                *ud = Some(r.unwrap());
                CallbackAction::Disarm
            },
        );
    }

    // Connect
    let mut connected = false;
    unsafe {
        client.connect(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_connect),
            address,
            ptr::addr_of_mut!(connected),
            |ud: *mut bool,
             _l: *mut Loop,
             _c: *mut Completion,
             _s: TCP,
             r: core::result::Result<(), ConnectError>| {
                r.unwrap();
                *ud = true;
                CallbackAction::Disarm
            },
        );
    }

    loop_.run(RunMode::UntilDone).unwrap();
    assert!(server_conn.is_some());
    assert!(connected);

    // Close the server (listener)
    let mut server_closed = false;
    unsafe {
        server.close(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_accept),
            ptr::addr_of_mut!(server_closed),
            |ud: *mut bool,
             _l: *mut Loop,
             _c: *mut Completion,
             _s: TCP,
             r: core::result::Result<(), CloseError>| {
                r.unwrap();
                *ud = true;
                CallbackAction::Disarm
            },
        );
    }
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(server_closed);

    // Unqueued send - Limit send buffer to 8kB, this should force partial writes.
    let sndbuf: libc::c_int = 8192;
    unsafe {
        libc::setsockopt(
            client.fd(),
            libc::SOL_SOCKET,
            libc::SO_SNDBUF,
            ptr::addr_of!(sndbuf).cast(),
            size_of::<libc::c_int>() as net::socklen_t,
        );
    }

    // First we try to send the whole 1MB buffer in one write operation, this
    // _should_ result in a partial write.
    let mut sent_unqueued: usize = 0;
    unsafe {
        client.write(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_connect),
            write_slice(&send_buf),
            ptr::addr_of_mut!(sent_unqueued),
            |ud: *mut usize,
             _l: *mut Loop,
             _c: *mut Completion,
             _s: TCP,
             _b: WriteBuffer,
             r: core::result::Result<usize, WriteError>| {
                *ud = r.unwrap();
                CallbackAction::Disarm
            },
        );
    }

    // Make sure that we sent a small fraction of the buffer.
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(sent_unqueued < (send_buf.len() / 10));

    // Set up queued write for the remainder; shutdown() the socket afterwards.
    let mut w_queue = WriteQueue::default();
    let mut wr_send = WriteRequest::default();
    let mut sent_queued: usize = 0;
    let queued_slice = write_slice(&send_buf[sent_unqueued..]);
    unsafe {
        client.queue_write(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(w_queue),
            ptr::addr_of_mut!(wr_send),
            queued_slice,
            ptr::addr_of_mut!(sent_queued),
            |ud: *mut usize,
             l: *mut Loop,
             c: *mut Completion,
             tcp: TCP,
             _b: WriteBuffer,
             r: core::result::Result<usize, WriteError>| {
                *ud = r.unwrap();
                tcp.shutdown(
                    l,
                    c,
                    ptr::null_mut::<()>(),
                    |_ud: *mut (),
                     _l: *mut Loop,
                     _c: *mut Completion,
                     _s: TCP,
                     _r: core::result::Result<(), ShutdownError>| {
                        CallbackAction::Disarm
                    },
                );
                CallbackAction::Disarm
            },
        );
    }

    // Set up receiver which keeps reading until it reads the full send buffer.
    struct Receiver {
        loop_: *mut Loop,
        conn: TCP,
        completion: Completion,
        buf: Vec<u8>,
        bytes_read: usize,
    }
    impl Receiver {
        fn read(&mut self) {
            if self.bytes_read == self.buf.len() {
                return;
            }
            let start = self.bytes_read;
            let len = self.buf.len() - start;
            let read_buf = ReadBuffer::Slice(ptr::slice_from_raw_parts_mut(
                unsafe { self.buf.as_mut_ptr().add(start) },
                len,
            ));
            let loop_ = self.loop_;
            let comp = ptr::addr_of_mut!(self.completion);
            let self_ptr = self as *mut Receiver;
            unsafe {
                self.conn
                    .read(loop_, comp, read_buf, self_ptr, Self::read_cb);
            }
        }

        fn read_cb(
            ud: *mut Receiver,
            _l: *mut Loop,
            _c: *mut Completion,
            _s: TCP,
            _b: ReadBuffer,
            r: core::result::Result<usize, ReadError>,
        ) -> CallbackAction {
            let receiver = unsafe { &mut *ud };
            let n_bytes = r.unwrap();
            receiver.bytes_read += n_bytes;
            if receiver.bytes_read < receiver.buf.len() {
                receiver.read();
            }
            CallbackAction::Disarm
        }
    }
    let mut receiver = Receiver {
        loop_: ptr::addr_of_mut!(loop_),
        conn: server_conn.unwrap(),
        completion: Completion::default(),
        buf: vec![0u8; SEND_LEN],
        bytes_read: 0,
    };
    receiver.read();

    // Wait for the send/receive.
    loop_.run(RunMode::UntilDone).unwrap();
    assert_eq!(&send_buf[..], &receiver.buf[0..receiver.bytes_read]);
    assert_eq!(send_buf.len(), sent_unqueued + sent_queued);

    // Close
    let server_conn_tcp = server_conn.unwrap();
    unsafe {
        server_conn_tcp.close(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_accept),
            ptr::addr_of_mut!(server_conn),
            |ud: *mut Option<TCP>,
             _l: *mut Loop,
             _c: *mut Completion,
             _s: TCP,
             r: core::result::Result<(), CloseError>| {
                r.unwrap();
                *ud = None;
                CallbackAction::Disarm
            },
        );
        client.close(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_connect),
            ptr::addr_of_mut!(connected),
            |ud: *mut bool,
             _l: *mut Loop,
             _c: *mut Completion,
             _s: TCP,
             r: core::result::Result<(), CloseError>| {
                r.unwrap();
                *ud = false;
                CallbackAction::Disarm
            },
        );
    }

    loop_.run(RunMode::UntilDone).unwrap();
    assert!(server_conn.is_none());
    assert!(!connected);
    assert!(server_closed);

    loop_.deinit();
    pool.shutdown();
    pool.deinit();
}

// -----------------------------------------------------------------------
// UDP watcher tests (watcher/udp.zig `UDPTests`). Ported for the epoll
// backend instantiation. The `test "UDP: Stream decls"` reflection test
// (udp.zig:902) is a Zig comptime `@hasDecl` sweep with no Rust runtime
// analog — it is enforced at COMPILE time by `UDP` carrying every
// `read`/`write`/`close`/`poll` method, so it is not a separate runtime test.
// Like the epoll `TCP` tests, `UDP::close` rides the thread pool, so the loop
// is built with one (the reference builds `Loop.init(.{ .thread_pool })`).
// -----------------------------------------------------------------------

// test "UDP: read/write" (udp.zig:914). Distinct port from the TCP/socket
// tests so parallel cargo runs cannot cross-route datagrams (SO_REUSEPORT).
//
// Named in the const-registry style (mirrors io_uring.rs's `PORT_*` block)
// so the loopback port is greppable and non-colliding: 3131 is this module's
// `epoll_socket_roundtrip`; 3132-3134/3136 are io_uring.rs's; 3137-3139 are
// dynamic.rs's. 3135 is THIS test — was a bare literal, now registered here.
const PORT_UDP_READ_WRITE: u16 = 3135;

#[test]
fn epoll_udp_read_write() {
    use crate::thread_pool::{Config, ThreadPool};

    let mut pool = ThreadPool::init(Config::default());
    let pool_ptr: *mut ThreadPool = &mut pool;
    let mut loop_ = Loop::init(Options {
        thread_pool: Some(pool_ptr),
        ..Options::default()
    })
    .unwrap();

    let address = net::Address::parse_ip4(b"127.0.0.1", PORT_UDP_READ_WRITE).unwrap();
    let server = UDP::init(address).unwrap();
    let client = UDP::init(address).unwrap();

    // Bind / Recv
    server.bind(address).unwrap();
    let mut c_read = Completion::default();
    let mut s_read = UdpState::undefined();
    let mut recv_buf = [0u8; 128];
    let mut recv_len: usize = 0;
    unsafe {
        server.read(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_read),
            ptr::addr_of_mut!(s_read),
            read_slice(&mut recv_buf),
            ptr::addr_of_mut!(recv_len),
            |ud: *mut usize,
             _l: *mut Loop,
             _c: *mut Completion,
             _s: *mut UdpState,
             _addr: net::Address,
             _sock: UDP,
             _b: ReadBuffer,
             r: core::result::Result<usize, ReadError>| {
                *ud = r.unwrap();
                CallbackAction::Disarm
            },
        );
    }

    // Send
    let send_buf = [1u8, 1, 2, 3, 5, 8, 13];
    let mut c_write = Completion::default();
    let mut s_write = UdpState::undefined();
    unsafe {
        client.write(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_write),
            ptr::addr_of_mut!(s_write),
            address,
            write_slice(&send_buf),
            ptr::null_mut::<()>(),
            |_ud: *mut (),
             _l: *mut Loop,
             _c: *mut Completion,
             _s: *mut UdpState,
             _sock: UDP,
             _b: WriteBuffer,
             r: core::result::Result<usize, WriteError>| {
                r.unwrap();
                CallbackAction::Disarm
            },
        );
    }

    // Wait for the send/receive
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(recv_len > 0);
    assert_eq!(send_buf.as_slice(), &recv_buf[0..recv_len]);

    // Close
    unsafe {
        server.close(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_read),
            ptr::null_mut::<()>(),
            |_ud: *mut (),
             _l: *mut Loop,
             _c: *mut Completion,
             _s: UDP,
             r: core::result::Result<(), CloseError>| {
                r.unwrap();
                CallbackAction::Disarm
            },
        );
        client.close(
            ptr::addr_of_mut!(loop_),
            ptr::addr_of_mut!(c_write),
            ptr::null_mut::<()>(),
            |_ud: *mut (),
             _l: *mut Loop,
             _c: *mut Completion,
             _s: UDP,
             r: core::result::Result<(), CloseError>| {
                r.unwrap();
                CallbackAction::Disarm
            },
        );
    }

    loop_.run(RunMode::UntilDone).unwrap();

    loop_.deinit();
    pool.shutdown();
    pool.deinit();
}
