//! io_uring backend tests: the socket family.
//!
//! PORTS: libxev src/backend/io_uring.zig — the socket accept/connect/send/recv/
//! close, sendmsg/recvmsg and read-cancellation `test{}` blocks, plus the TCP and
//! UDP watcher tests (src/watcher/tcp.zig, src/watcher/udp.zig).

use super::*;

// -----------------------------------------------------------------------
// L1 LT6: the socket family (io_uring.zig:1401/1626/1725) + the Linux live
// KATs. These run REAL syscalls on the live io_uring ring (Miri can't).
// -----------------------------------------------------------------------

use crate::linux::net;

// PORT NOTE (test-harness divergence, ports): every reference socket test binds
// the FIXED port 3131. Zig's test runner executes a file's tests SERIALLY, so
// the reference's three socket tests never collide. Rust's harness runs the
// WHOLE CRATE's tests in PARALLEL threads of one process — so io_uring's socket
// tests would race BOTH each other AND `epoll.rs`'s `epoll_socket_roundtrip`
// (which also binds TCP 3131 with SO_REUSEADDR|SO_REUSEPORT: a second listener
// would bind successfully and steal connections). Each io_uring socket test
// therefore gets its own dedicated port. Nothing else about the tests changes.
const PORT_SOCKET_ROUNDTRIP: u16 = 3132;
const PORT_SENDMSG_RECVMSG: u16 = 3133;
const PORT_READ_CANCELLATION: u16 = 3134;
const PORT_UDP_READ_WRITE: u16 = 3136;

// PORT NOTE: `posix.socket(family, ty, flags)`; the socket test creates a
// blocking listener + a NONBLOCK client (SOCK.CLOEXEC on both).
unsafe fn new_socket(family: libc::c_int, ty: libc::c_int) -> socket_t {
    let fd = unsafe { libc::socket(family, ty, 0) };
    assert!(fd >= 0);
    fd
}

unsafe fn set_reuse(fd: socket_t, opt: libc::c_int) {
    let one: libc::c_int = 1;
    let rc = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            opt,
            ptr::addr_of!(one).cast(),
            size_of::<libc::c_int>() as net::socklen_t,
        )
    };
    assert_eq!(rc, 0);
}

// test "io_uring: socket accept/connect/send/recv/close" (io_uring.zig:1401)
#[test]
fn io_uring_socket_roundtrip() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

    // Create a TCP server socket
    let address = net::Address::parse_ip4(b"127.0.0.1", PORT_SOCKET_ROUNDTRIP).unwrap();
    let kernel_backlog = 1;
    let family = unsafe { address.any.family } as libc::c_int;
    let ln = unsafe { new_socket(family, libc::SOCK_STREAM | libc::SOCK_CLOEXEC) };
    unsafe {
        set_reuse(ln, libc::SO_REUSEADDR);
        assert_eq!(
            libc::bind(
                ln,
                ptr::addr_of!(address.any) as *const libc::sockaddr,
                address.get_os_sock_len()
            ),
            0
        );
        assert_eq!(libc::listen(ln, kernel_backlog), 0);
    }

    // Create a TCP client socket
    let mut client_conn = unsafe { new_socket(family, libc::SOCK_STREAM | libc::SOCK_CLOEXEC) };

    // Accept
    let mut server_conn: socket_t = 0;
    fn accept_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
        let conn = ud as *mut socket_t;
        // conn.* = r.accept catch unreachable;
        match r {
            Result::Accept(Ok(v)) => unsafe { *conn = v },
            _ => unreachable!(),
        }
        CallbackAction::Disarm
    }
    let mut c_accept = Completion {
        op: Operation::Accept {
            socket: ln,
            // The reference relies on the Zig field defaults
            // (`addr = undefined`, `addr_size = @sizeOf(posix.sockaddr)`,
            // `flags = posix.SOCK.CLOEXEC`); spelled at the call site here.
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
        // _ = r.connect catch unreachable;
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
        // _ = r.send catch unreachable;
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
        let p = ud as *mut usize;
        // ptr.* = r.recv catch unreachable;
        match r {
            Result::Recv(Ok(n)) => unsafe { *p = n },
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
        // _ = r.shutdown catch unreachable;
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
            // Zig field default: `how = .both`.
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
        let p = ud as *mut Option<bool>;
        let v = match r {
            Result::Recv(Ok(_)) => false,
            Result::Recv(Err(ReadError::EOF)) => true,
            Result::Recv(Err(_)) => false,
            _ => unreachable!(),
        };
        unsafe { *p = Some(v) };
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
        // _ = r.close catch unreachable;
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

    // The accepted server connection is not closed by the reference test
    // either; close it so the crate's (long-lived, many-test) process doesn't
    // leak the fd.
    unsafe { libc::close(server_conn) };

    loop_.deinit();
}

// -----------------------------------------------------------------------
// TCP watcher tests (watcher/tcp.zig `TCPTests`). Ported for the io_uring
// backend instantiation (mirrors epoll.rs's). Unlike epoll, io_uring's TCP
// `close` is a real SQE (no `.threadpool` flag), so the loop needs NO thread
// pool. The `test "TCP: Stream decls"` reflection test (tcp.zig:506) is a Zig
// comptime `@hasDecl` sweep with no Rust runtime analog (enforced at COMPILE
// time by `TCP` carrying every method), so it is not a separate runtime test.
// -----------------------------------------------------------------------

// test "TCP: accept/connect/send/recv/close" (tcp.zig:518).
#[test]
fn io_uring_tcp_accept_connect_send_recv_close() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

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
}

// test "TCP: Queued writes" (tcp.zig:697).
#[test]
fn io_uring_tcp_queued_writes() {
    const PATTERN: [u8; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 0];
    const SEND_LEN: usize = PATTERN.len() * 100_000;
    let mut send_buf: Vec<u8> = Vec::with_capacity(SEND_LEN);
    for _ in 0..100_000 {
        send_buf.extend_from_slice(&PATTERN);
    }

    let mut loop_ = Loop::init(Options::default()).unwrap();

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
}

// -----------------------------------------------------------------------
// UDP watcher tests (watcher/udp.zig `UDPTests`). Ported for the io_uring
// backend instantiation. The `test "UDP: Stream decls"` reflection test
// (udp.zig:902) is a Zig comptime `@hasDecl` sweep with no Rust runtime
// analog (enforced at COMPILE time by `UDP` carrying every method). Unlike
// epoll, io_uring's `UDP::close` is a real SQE (no `.threadpool` flag), so the
// loop needs NO thread pool.
// -----------------------------------------------------------------------

// test "UDP: read/write" (udp.zig:914). Dedicated port so parallel cargo runs
// cannot cross-route datagrams (SO_REUSEPORT).
#[test]
fn io_uring_udp_read_write() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

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
}

// test "io_uring: sendmsg/recvmsg" (io_uring.zig:1626)
#[test]
fn io_uring_sendmsg_recvmsg() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

    // Create a UDP server socket
    let address = net::Address::parse_ip4(b"127.0.0.1", PORT_SENDMSG_RECVMSG).unwrap();
    let family = unsafe { address.any.family } as libc::c_int;
    let server = unsafe { new_socket(family, libc::SOCK_DGRAM) };
    unsafe {
        set_reuse(server, libc::SO_REUSEPORT);
        set_reuse(server, libc::SO_REUSEADDR);
        assert_eq!(
            libc::bind(
                server,
                ptr::addr_of!(address.any) as *const libc::sockaddr,
                address.get_os_sock_len()
            ),
            0
        );
    }

    let client = unsafe { new_socket(family, libc::SOCK_DGRAM) };

    // Send
    //
    // PORT NOTE: the reference's send-side buffers are `const` and ride the
    // `posix.msghdr_const` type. `libc` has no const twin of `msghdr` (its
    // fields are all `*mut`), so the `as *mut` casts below are pure type
    // plumbing: `IORING_OP_SENDMSG` only ever READS `msg_name`/`msg_iov`/the
    // iovec buffers, so nothing is ever written through these pointers and the
    // shared-ref provenance is never violated. (Contrast `msg_recv` below,
    // whose `msg_name` the kernel really does write.)
    let buffer_send = [42u8; 128];
    let iovecs_send = [libc::iovec {
        iov_base: buffer_send.as_ptr() as *mut libc::c_void,
        iov_len: buffer_send.len(),
    }];
    let mut msg_send = libc::msghdr {
        msg_name: ptr::addr_of!(address.any) as *mut libc::c_void,
        msg_namelen: address.get_os_sock_len(),
        msg_iov: iovecs_send.as_ptr() as *mut libc::iovec,
        msg_iovlen: 1,
        msg_control: ptr::null_mut(),
        msg_controllen: 0,
        msg_flags: 0,
    };
    fn sendmsg_cb(
        _ud: *mut c_void,
        _l: *mut Loop,
        _c: *mut Completion,
        r: Result,
    ) -> CallbackAction {
        // _ = r.sendmsg catch unreachable;
        match r {
            Result::SendMsg(Ok(_)) => {}
            _ => unreachable!(),
        }
        CallbackAction::Disarm
    }
    let mut c_sendmsg = Completion {
        op: Operation::SendMsg {
            fd: client,
            msghdr: ptr::addr_of_mut!(msg_send),
            // Zig field defaults: `buffer = null`, `iov = undefined`.
            buffer: None,
            iov: [libc::iovec {
                iov_base: ptr::null_mut(),
                iov_len: 0,
            }],
        },
        callback: sendmsg_cb,
        ..Completion::default()
    };
    unsafe { loop_.add(ptr::addr_of_mut!(c_sendmsg)) };

    // Recv
    let mut buffer_recv = [0u8; 128];
    let mut iovecs_recv = [libc::iovec {
        iov_base: buffer_recv.as_mut_ptr() as *mut libc::c_void,
        iov_len: buffer_recv.len(),
    }];
    // `var address_recv` — MUTABLE on purpose: `recvmsg` WRITES the datagram's
    // source address back through `msg_name`, so the pointer handed to the
    // kernel must be derived from a mutable place (a `*mut` laundered out of
    // `addr_of!` would be writing through a shared-ref provenance).
    let mut address_recv = net::Address::init_ip4([0, 0, 0, 0], 0);
    let mut msg_recv = libc::msghdr {
        msg_name: ptr::addr_of_mut!(address_recv.any).cast::<libc::c_void>(),
        msg_namelen: address_recv.get_os_sock_len(),
        msg_iov: iovecs_recv.as_mut_ptr(),
        msg_iovlen: 1,
        msg_control: ptr::null_mut(),
        msg_controllen: 0,
        msg_flags: 0,
    };
    let mut recv_size: usize = 0;
    fn recvmsg_cb(
        ud: *mut c_void,
        _l: *mut Loop,
        _c: *mut Completion,
        r: Result,
    ) -> CallbackAction {
        let p = ud as *mut usize;
        // ptr.* = r.recvmsg catch unreachable;
        match r {
            Result::RecvMsg(Ok(n)) => unsafe { *p = n },
            _ => unreachable!(),
        }
        CallbackAction::Disarm
    }
    let mut c_recvmsg = Completion {
        op: Operation::RecvMsg {
            fd: server,
            msghdr: ptr::addr_of_mut!(msg_recv),
        },
        userdata: ptr::addr_of_mut!(recv_size).cast::<c_void>(),
        callback: recvmsg_cb,
        ..Completion::default()
    };
    unsafe { loop_.add(ptr::addr_of_mut!(c_recvmsg)) };

    // Wait for the sockets to close
    loop_.run(RunMode::UntilDone).unwrap();
    assert_eq!(recv_size, buffer_recv.len());
    assert_eq!(&buffer_send[0..buffer_recv.len()], &buffer_recv[..]);

    unsafe {
        libc::close(client);
        libc::close(server);
    }
    loop_.deinit();
}

// test "io_uring: socket read cancellation" (io_uring.zig:1725)
#[test]
fn io_uring_socket_read_cancellation() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

    // Create a UDP server socket
    let address = net::Address::parse_ip4(b"127.0.0.1", PORT_READ_CANCELLATION).unwrap();
    let family = unsafe { address.any.family } as libc::c_int;
    let socket = unsafe { new_socket(family, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC) };
    unsafe {
        set_reuse(socket, libc::SO_REUSEADDR);
        assert_eq!(
            libc::bind(
                socket,
                ptr::addr_of!(address.any) as *const libc::sockaddr,
                address.get_os_sock_len()
            ),
            0
        );
    }

    // Read
    let mut read_result: Option<Result> = None;
    fn read_result_cb(
        ud: *mut c_void,
        _l: *mut Loop,
        _c: *mut Completion,
        r: Result,
    ) -> CallbackAction {
        let p = ud as *mut Option<Result>;
        unsafe { *p = Some(r) };
        CallbackAction::Disarm
    }
    let mut c_read = Completion {
        op: Operation::Read {
            fd: socket,
            buffer: ReadBuffer::Array([0; 32]),
        },
        userdata: ptr::addr_of_mut!(read_result).cast::<c_void>(),
        callback: read_result_cb,
        ..Completion::default()
    };
    unsafe { loop_.add(ptr::addr_of_mut!(c_read)) };

    // Cancellation
    let mut c_read_cancel = Completion::default();
    let cancel_cb = |_ud: *mut (),
                     _l: *mut Loop,
                     _c: *mut Completion,
                     r: core::result::Result<(), CancelError>| {
        // r catch unreachable;
        r.unwrap();
        CallbackAction::Disarm
    };
    unsafe {
        loop_.cancel::<(), _>(
            ptr::addr_of_mut!(c_read),
            ptr::addr_of_mut!(c_read_cancel),
            ptr::null_mut(),
            cancel_cb,
        );
    }

    // loop.add(&c_read_cancel);
    //
    // PORT NOTE (a REFERENCE QUIRK, preserved deliberately): `Loop::cancel`
    // ALREADY added `c_read_cancel` (io_uring.zig:592, `self.add(c_cancel)`),
    // so this second `add` submits a SECOND, redundant `IORING_OP_ASYNC_CANCEL`
    // SQE for the same target — and the cancel callback therefore fires TWICE,
    // both times asserting `r catch unreachable`. It is preserved because it is
    // what the reference test does (R1); it is benign because the kernel
    // reports success for BOTH: verified empirically on this host (Linux
    // 6.19.10, aarch64) by instrumenting `cqe.res`, which is `0` for every
    // repeated cancel of the same in-flight read, not the `-ENOENT` one might
    // expect for the second. See the report's concern about kernels that may
    // instead return `-ENOENT` here.
    unsafe { loop_.add(ptr::addr_of_mut!(c_read_cancel)) };

    // Wait for the read to be cancelled
    loop_.run(RunMode::UntilDone).unwrap();
    // try testing.expectEqual(OperationType.read, @as(OperationType, read_result));
    // try testing.expectError(error.Canceled, read_result.read);
    assert_eq!(read_result, Some(Result::Read(Err(ReadError::Canceled))));

    // The reference only `errdefer`s this close (so a passing run leaks it);
    // the crate's test process is long-lived and runs many tests, so close it.
    unsafe { libc::close(socket) };
    loop_.deinit();
}
