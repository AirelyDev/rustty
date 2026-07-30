//! windows.rs — faithful port of libxev's own `src/windows.zig`
//! ("Namespace containing missing utils from std"), PLUS byte-faithful TWINS of
//! `loop.rs`'s pure, backend-agnostic ABI enums — exactly the shape `linux.rs`
//! has for the two Linux backends.
//!
//! PORT NOTE: the reference re-exports a set of Win32 constants/types/functions
//! from `std.os.windows` "so we can just import one file", then adds an `exp`
//! namespace for the declarations Zig's std lacks. Rust has no `std.os.windows`
//! equivalent, so — exactly as `crates/rustty-os/src/windows.rs` already does in
//! this workspace — the Win32 ABI types are declared locally and the functions
//! bind directly to the SAME system DLLs (`kernel32`, `ws2_32`). That is a
//! binding to the same system library, not a reimplementation (R3).
//!
//! PORT NOTE (R4/no new dependency): `libc` on Windows carries none of these
//! APIs and the crate convention is hand-rolled `extern "system"` blocks (memory
//! `right-tool-for-the-job`; the same convention `rustty-os::windows` uses). No
//! crate was added for this file.

#![allow(dead_code)]
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(clippy::upper_case_acronyms)]

use core::ffi::c_void;

// ---------------------------------------------------------------------------
// The `loop.zig` ABI twins (see `linux.rs` for why these are twins rather than a
// shared import: `loop.rs` is macOS-gated and its `Callback`/`noop_callback` are
// coupled to the concrete kqueue types; the four decls below are the PURE,
// backend-agnostic slice of that file).
// ---------------------------------------------------------------------------

/// TWIN of `loop.rs`'s `Options` (byte-faithful).
//
// PORT NOTE: `loop.zig`'s field is `thread_pool: ?*xev.ThreadPool`. The `iocp`
// backend never reads it — `Loop.init` is literally `_ = options;`
// (iocp.zig:72-74) — and the crate's `ThreadPool` body (kprotty's Zap pool) is
// currently built only where its futex primitive has an arm (macOS/Linux). The
// Windows arm of that primitive is REMAINING PORT WORK tracked in
// `.port/phase-2/DEFERRALS.md` (WS20), NOT a scope exclusion; until it lands the
// field is carried as the opaque caller-owned handle iocp would pass through
// untouched, so no caller loses the ability to name/populate it.
pub struct Options {
    /// The number of queued completions that can be in flight before
    /// requiring interaction with the kernel.
    ///
    /// Backends: io_uring
    pub entries: u32,

    /// A thread pool to use for blocking operations. If the backend doesn't
    /// need to perform any blocking operations then no threads will ever be
    /// spawned. If the backend does need to perform blocking operations on a
    /// thread and no thread pool is provided, the operations will simply
    /// fail. Unless you're trying to really optimize for space, it is
    /// recommended you provide a thread pool.
    ///
    /// Backends: epoll, kqueue
    pub thread_pool: Option<*mut c_void>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            entries: 256,
            thread_pool: None,
        }
    }
}

/// TWIN of `loop.rs`'s `RunMode` (byte-faithful).
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// Run the event loop once. If there are no blocking operations ready,
    /// return immediately.
    NoWait = 0,

    /// Run the event loop once, waiting for at least one blocking operation
    /// to complete.
    Once = 1,

    /// Run the event loop until it is "done".
    UntilDone = 2,
}

/// TWIN of `loop.rs`'s `CallbackAction` (byte-faithful).
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackAction {
    /// The request is complete and is not repeated.
    Disarm = 0,

    /// Requeue the same operation request with the same parameters
    /// with the event loop.
    Rearm = 1,
}

/// TWIN of `loop.rs`'s `CompletionState` (byte-faithful).
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionState {
    /// The completion is not being used and is ready to be configured
    /// for new work.
    Dead = 0,

    /// The completion is part of an event loop.
    Active = 1,
}

// ---------------------------------------------------------------------------
// The `std.net.Address` subset the iocp `Operation` uses (the twin of the
// macOS-gated `net.rs` and of `linux::net`, on the WINDOWS `sockaddr` layout —
// Windows has no leading `len` byte, so the darwin subset in `net.rs` cannot be
// reused). `backend/iocp.zig` stores `addr: std.net.Address` in its
// `connect`/`sendto`/`recvfrom` ops and reaches `.any` + `getOsSockLen()`.
// ---------------------------------------------------------------------------
pub mod net {
    use core::mem::size_of;

    /// PORT NOTE: `posix.socklen_t` on Windows (`i32`).
    pub type socklen_t = i32;
    /// PORT NOTE: `posix.sa_family_t` on Windows (`u16` — no `len` byte).
    pub type sa_family_t = u16;
    /// PORT NOTE: `posix.in_port_t`.
    pub type in_port_t = u16;

    /// PORT NOTE: `posix.AF.INET`.
    pub const AF_INET: sa_family_t = 2;
    /// PORT NOTE: `posix.AF.INET6`.
    pub const AF_INET6: sa_family_t = 23;

    /// PORT NOTE: `posix.sockaddr` (the generic `any`) on Windows.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct SockaddrAny {
        pub family: sa_family_t,
        pub data: [u8; 14],
    }

    /// PORT NOTE: `posix.sockaddr.in` on Windows.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct SockaddrIn {
        pub family: sa_family_t,
        pub port: in_port_t,
        pub addr: u32,
        pub zero: [u8; 8],
    }

    /// PORT NOTE: `posix.sockaddr.in6` on Windows.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct SockaddrIn6 {
        pub family: sa_family_t,
        pub port: in_port_t,
        pub flowinfo: u32,
        pub addr: [u8; 16],
        pub scope_id: u32,
    }

    /// PORT NOTE: `std.net.Ip4Address`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Ip4Address {
        pub sa: SockaddrIn,
    }

    /// PORT NOTE: `std.net.Address` — the `extern union { any, in, in6 }`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union Address {
        pub any: SockaddrAny,
        pub in_: SockaddrIn,
        pub in6: SockaddrIn6,
    }

    impl Default for Address {
        fn default() -> Self {
            // SAFETY: an all-zero `sockaddr` is a valid (AF_UNSPEC) value.
            unsafe { core::mem::zeroed() }
        }
    }

    impl Address {
        /// PORT NOTE: `std.net.Address.initIp4(addr, port)`.
        pub fn init_ip4(addr: [u8; 4], port: u16) -> Address {
            Address {
                in_: SockaddrIn {
                    family: AF_INET,
                    port: port.to_be(),
                    addr: u32::from_ne_bytes(addr),
                    zero: [0; 8],
                },
            }
        }

        /// PORT NOTE: `std.net.Address.parseIp4(name, port)` — the dotted-quad
        /// subset the reference tests use (`"127.0.0.1"`).
        pub fn parse_ip4(name: &[u8], port: u16) -> Result<Address, ()> {
            let mut out = [0u8; 4];
            let mut idx = 0usize;
            let mut cur: u32 = 0;
            let mut saw_digit = false;
            for &b in name {
                match b {
                    b'0'..=b'9' => {
                        cur = cur * 10 + (b - b'0') as u32;
                        if cur > 255 {
                            return Err(());
                        }
                        saw_digit = true;
                    }
                    b'.' => {
                        if !saw_digit || idx >= 4 {
                            return Err(());
                        }
                        out[idx] = cur as u8;
                        idx += 1;
                        cur = 0;
                        saw_digit = false;
                    }
                    _ => return Err(()),
                }
            }
            if !saw_digit || idx != 3 {
                return Err(());
            }
            out[3] = cur as u8;
            Ok(Address::init_ip4(out, port))
        }

        /// PORT NOTE: `std.net.Address.getOsSockLen()`.
        pub fn get_os_sock_len(&self) -> socklen_t {
            // SAFETY: reading the common `family` prefix of the union.
            match unsafe { self.any.family } {
                AF_INET => size_of::<SockaddrIn>() as socklen_t,
                AF_INET6 => size_of::<SockaddrIn6>() as socklen_t,
                _ => size_of::<SockaddrAny>() as socklen_t,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Forwarded declarations of std.os.windows (windows.zig:8-35).
// ---------------------------------------------------------------------------

pub type BOOL = i32;
pub type BYTE = u8;
pub type DWORD = u32;
pub type WORD = u16;
pub type LONG = i32;
pub type LARGE_INTEGER = i64;
pub type ULONGLONG = u64;
pub type ULONG_PTR = usize;
pub type SIZE_T = usize;
pub type HANDLE = *mut c_void;
pub type LPVOID = *mut c_void;
pub type LPCSTR = *const u8;
pub type SOCKET = usize;

pub const FALSE: BOOL = 0;
pub const TRUE: BOOL = 1;
pub const INFINITE: DWORD = 0xFFFF_FFFF;
pub const INVALID_HANDLE_VALUE: HANDLE = usize::MAX as HANDLE;
pub const INVALID_SOCKET: SOCKET = usize::MAX;

pub const DUPLICATE_SAME_ACCESS: DWORD = 0x0000_0002;
pub const GENERIC_READ: DWORD = 0x8000_0000;
pub const GENERIC_WRITE: DWORD = 0x4000_0000;
pub const OPEN_ALWAYS: DWORD = 4;
pub const FILE_FLAG_OVERLAPPED: DWORD = 0x4000_0000;

/// PORT NOTE: `windows.Win32Error` — only the codes the iocp backend switches on
/// are named; the type is the raw `DWORD` the API returns (Zig's is a non-exhaustive
/// enum over the same values).
pub mod Win32Error {
    use super::DWORD;
    pub const SUCCESS: DWORD = 0;
    pub const FILE_NOT_FOUND: DWORD = 2;
    pub const ACCESS_DENIED: DWORD = 5;
    pub const INVALID_PARAMETER: DWORD = 87;
    pub const ALREADY_EXISTS: DWORD = 183;
    pub const IO_PENDING: DWORD = 997;
    pub const OPERATION_ABORTED: DWORD = 995;
    pub const HANDLE_EOF: DWORD = 38;
    pub const BROKEN_PIPE: DWORD = 109;
}

/// PORT NOTE: `windows.OVERLAPPED`. Zig spells the anonymous union/struct
/// `DUMMYUNIONNAME.DUMMYSTRUCTNAME`; Rust has no anonymous members, so the union
/// is a named `#[repr(C)] union` with the SAME layout (a `{Offset, OffsetHigh}`
/// pair overlapping a `Pointer`), and `iocp.rs` writes it through the same path.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OVERLAPPED {
    pub Internal: ULONG_PTR,
    pub InternalHigh: ULONG_PTR,
    pub DUMMYUNIONNAME: OVERLAPPED_UNION,
    pub hEvent: HANDLE,
}

/// PORT NOTE: the `OVERLAPPED` anonymous union.
#[repr(C)]
#[derive(Clone, Copy)]
pub union OVERLAPPED_UNION {
    pub DUMMYSTRUCTNAME: OVERLAPPED_OFFSETS,
    pub Pointer: *mut c_void,
}

/// PORT NOTE: the `OVERLAPPED` anonymous struct (`Offset`/`OffsetHigh`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OVERLAPPED_OFFSETS {
    pub Offset: DWORD,
    pub OffsetHigh: DWORD,
}

impl Default for OVERLAPPED {
    /// PORT NOTE: the reference's literal default
    /// `.{ .Internal = 0, .InternalHigh = 0, .DUMMYUNIONNAME = .{ .Pointer = null }, .hEvent = null }`.
    fn default() -> Self {
        Self {
            Internal: 0,
            InternalHigh: 0,
            DUMMYUNIONNAME: OVERLAPPED_UNION {
                Pointer: core::ptr::null_mut(),
            },
            hEvent: core::ptr::null_mut(),
        }
    }
}

/// PORT NOTE: `windows.OVERLAPPED_ENTRY`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OVERLAPPED_ENTRY {
    pub lpCompletionKey: ULONG_PTR,
    pub lpOverlapped: *mut OVERLAPPED,
    pub Internal: ULONG_PTR,
    pub dwNumberOfBytesTransferred: DWORD,
}

/// PORT NOTE: `windows.ReadFileError` — the `std.os.windows.ReadFile` error set
/// (`OperationAborted`/`BrokenPipe`/`Unexpected`), collapsed to the variants the
/// iocp backend actually maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadFileError {
    OperationAborted,
    BrokenPipe,
    Unexpected,
}

/// PORT NOTE: `windows.WriteFileError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteFileError {
    OperationAborted,
    BrokenPipe,
    Unexpected,
}

/// PORT NOTE: `posix.UnexpectedError` (`error{Unexpected}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnexpectedError;

/// PORT NOTE: `windows.unexpectedError(err)` — logs in debug and returns
/// `error.Unexpected`.
pub fn unexpectedError(_err: DWORD) -> UnexpectedError {
    UnexpectedError
}

/// PORT NOTE: `windows.unexpectedWSAError(err)`.
pub fn unexpectedWSAError(_err: i32) -> UnexpectedError {
    UnexpectedError
}

// ---------------------------------------------------------------------------
// `windows.kernel32` (the std namespace the reference forwards).
// ---------------------------------------------------------------------------
pub mod kernel32 {
    use super::{
        c_void, BOOL, DWORD, HANDLE, LARGE_INTEGER, OVERLAPPED, OVERLAPPED_ENTRY, ULONG_PTR,
    };

    extern "system" {
        pub fn GetLastError() -> DWORD;
        pub fn CloseHandle(hObject: HANDLE) -> BOOL;
        // WS20: named by `watcher/process.zig`'s `ProcessIocp` — `windows.kernel32
        // .{GetCurrentProcess,DuplicateHandle,GetExitCodeProcess}`. In the
        // reference these come from `std.os.windows.kernel32`, which the
        // reference's own `windows.zig` re-exports wholesale.
        pub fn GetCurrentProcess() -> HANDLE;
        pub fn DuplicateHandle(
            hSourceProcessHandle: HANDLE,
            hSourceHandle: HANDLE,
            hTargetProcessHandle: HANDLE,
            lpTargetHandle: *mut HANDLE,
            dwDesiredAccess: DWORD,
            bInheritHandle: BOOL,
            dwOptions: DWORD,
        ) -> BOOL;
        pub fn GetExitCodeProcess(hProcess: HANDLE, lpExitCode: *mut DWORD) -> BOOL;
        pub fn CreateIoCompletionPort(
            FileHandle: HANDLE,
            ExistingCompletionPort: HANDLE,
            CompletionKey: ULONG_PTR,
            NumberOfConcurrentThreads: DWORD,
        ) -> HANDLE;
        pub fn GetQueuedCompletionStatusEx(
            CompletionPort: HANDLE,
            lpCompletionPortEntries: *mut OVERLAPPED_ENTRY,
            ulCount: DWORD,
            ulNumEntriesRemoved: *mut DWORD,
            dwMilliseconds: DWORD,
            fAlertable: BOOL,
        ) -> BOOL;
        pub fn PostQueuedCompletionStatus(
            CompletionPort: HANDLE,
            dwNumberOfBytesTransferred: DWORD,
            dwCompletionKey: ULONG_PTR,
            lpOverlapped: *mut OVERLAPPED,
        ) -> BOOL;
        pub fn GetOverlappedResult(
            hFile: HANDLE,
            lpOverlapped: *mut OVERLAPPED,
            lpNumberOfBytesTransferred: *mut DWORD,
            bWait: BOOL,
        ) -> BOOL;
        pub fn CancelIoEx(hFile: HANDLE, lpOverlapped: *mut OVERLAPPED) -> BOOL;
        pub fn ReadFile(
            hFile: HANDLE,
            lpBuffer: *mut c_void,
            nNumberOfBytesToRead: DWORD,
            lpNumberOfBytesRead: *mut DWORD,
            lpOverlapped: *mut OVERLAPPED,
        ) -> BOOL;
        pub fn WriteFile(
            hFile: HANDLE,
            lpBuffer: *const c_void,
            nNumberOfBytesToWrite: DWORD,
            lpNumberOfBytesWritten: *mut DWORD,
            lpOverlapped: *mut OVERLAPPED,
        ) -> BOOL;
        pub fn CreateFileW(
            lpFileName: *const u16,
            dwDesiredAccess: DWORD,
            dwShareMode: DWORD,
            lpSecurityAttributes: *mut c_void,
            dwCreationDisposition: DWORD,
            dwFlagsAndAttributes: DWORD,
            hTemplateFile: HANDLE,
        ) -> HANDLE;
        pub fn QueryPerformanceCounter(lpPerformanceCount: *mut LARGE_INTEGER) -> BOOL;
        pub fn QueryPerformanceFrequency(lpFrequency: *mut LARGE_INTEGER) -> BOOL;
    }
}

// PORT NOTE: `pub extern "kernel32" fn DeleteFileW(...) callconv(.winapi) BOOL;`
// — the reference declares this at file scope (std lacks it).
extern "system" {
    pub fn DeleteFileW(lpFileName: *const u16) -> BOOL;
}

// ---------------------------------------------------------------------------
// `windows.ws2_32` (the Winsock namespace the reference forwards).
// ---------------------------------------------------------------------------
pub mod ws2_32 {
    use super::{c_void, BOOL, DWORD, OVERLAPPED, SOCKET};

    /// PORT NOTE: `ws2_32.WSABUF`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct WSABUF {
        pub len: u32,
        pub buf: *mut u8,
    }

    impl Default for WSABUF {
        fn default() -> Self {
            Self {
                len: 0,
                buf: core::ptr::null_mut(),
            }
        }
    }

    /// PORT NOTE: `ws2_32.WinsockError` — the subset the iocp backend switches on.
    pub mod WinsockError {
        pub const WSA_IO_PENDING: i32 = 997;
        pub const WSA_OPERATION_ABORTED: i32 = 995;
        pub const WSAECONNABORTED: i32 = 10053;
        pub const WSAECONNRESET: i32 = 10054;
        pub const WSAENETRESET: i32 = 10052;
    }

    pub const WSA_FLAG_OVERLAPPED: DWORD = 0x01;

    /// PORT NOTE: `posix.SOL.SOCKET` / `posix.SO.TYPE` / `posix.SOCK.STREAM` on
    /// Windows (the values the `recv` EOF probe uses).
    pub const SOL_SOCKET: i32 = 0xffff;
    pub const SO_TYPE: i32 = 0x1008;
    pub const SOCK_STREAM: i32 = 1;

    /// PORT NOTE: `posix.sockaddr` (the generic 16-byte header).
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct sockaddr {
        pub family: u16,
        pub data: [u8; 14],
    }

    /// PORT NOTE: `posix.sockaddr.storage` (128 bytes, 8-aligned).
    #[repr(C, align(8))]
    #[derive(Clone, Copy)]
    pub struct sockaddr_storage {
        pub family: u16,
        pub padding: [u8; 126],
    }

    extern "system" {
        pub fn WSAGetLastError() -> i32;
        pub fn getsockname(s: SOCKET, name: *mut sockaddr, namelen: *mut i32) -> i32;
        pub fn getsockopt(
            s: SOCKET,
            level: i32,
            optname: i32,
            optval: *mut u8,
            optlen: *mut i32,
        ) -> i32;
        pub fn connect(s: SOCKET, name: *const sockaddr, namelen: i32) -> i32;
        pub fn shutdown(s: SOCKET, how: i32) -> i32;
        pub fn WSASocketW(
            af: i32,
            type_: i32,
            protocol: i32,
            lpProtocolInfo: *mut c_void,
            g: u32,
            dwFlags: DWORD,
        ) -> SOCKET;
        pub fn WSASend(
            s: SOCKET,
            lpBuffers: *mut WSABUF,
            dwBufferCount: DWORD,
            lpNumberOfBytesSent: *mut DWORD,
            dwFlags: DWORD,
            lpOverlapped: *mut OVERLAPPED,
            lpCompletionRoutine: *mut c_void,
        ) -> i32;
        pub fn WSARecv(
            s: SOCKET,
            lpBuffers: *mut WSABUF,
            dwBufferCount: DWORD,
            lpNumberOfBytesRecvd: *mut DWORD,
            lpFlags: *mut DWORD,
            lpOverlapped: *mut OVERLAPPED,
            lpCompletionRoutine: *mut c_void,
        ) -> i32;
        pub fn WSASendTo(
            s: SOCKET,
            lpBuffers: *mut WSABUF,
            dwBufferCount: DWORD,
            lpNumberOfBytesSent: *mut DWORD,
            dwFlags: DWORD,
            lpTo: *const sockaddr,
            iToLen: i32,
            lpOverlapped: *mut OVERLAPPED,
            lpCompletionRoutine: *mut c_void,
        ) -> i32;
        pub fn WSARecvFrom(
            s: SOCKET,
            lpBuffers: *mut WSABUF,
            dwBufferCount: DWORD,
            lpNumberOfBytesRecvd: *mut DWORD,
            lpFlags: *mut DWORD,
            lpFrom: *mut sockaddr,
            lpFromlen: *mut i32,
            lpOverlapped: *mut OVERLAPPED,
            lpCompletionRoutine: *mut c_void,
        ) -> i32;
        pub fn WSAGetOverlappedResult(
            s: SOCKET,
            lpOverlapped: *mut OVERLAPPED,
            lpcbTransfer: *mut DWORD,
            fWait: BOOL,
            lpdwFlags: *mut DWORD,
        ) -> BOOL;
    }

    // PORT NOTE: `ws2_32.AcceptEx` — in the real Winsock this is reached through
    // `WSAIoctl(SIO_GET_EXTENSION_FUNCTION_POINTER)`, but `mswsock.lib` exports
    // it directly and Zig's std declares it as a plain extern, which is what the
    // reference calls. Same declaration here.
    extern "system" {
        pub fn AcceptEx(
            sListenSocket: SOCKET,
            sAcceptSocket: SOCKET,
            lpOutputBuffer: *mut c_void,
            dwReceiveDataLength: DWORD,
            dwLocalAddressLength: DWORD,
            dwRemoteAddressLength: DWORD,
            lpdwBytesReceived: *mut DWORD,
            lpOverlapped: *mut OVERLAPPED,
        ) -> BOOL;
    }
}

// ---------------------------------------------------------------------------
// The std wrappers the reference forwards (windows.zig:29-35). Zig's std
// implements these on top of the raw kernel32 calls; the same wrappers live here
// because Rust's std exposes none of them.
// ---------------------------------------------------------------------------

/// PORT NOTE: `windows.CloseHandle` — std's wrapper asserts success and returns
/// `void`; the iocp `close` op ports its result as `CloseError!void`.
///
/// # Safety
/// `handle` must be a live Win32 handle owned by the caller.
pub unsafe fn CloseHandle(handle: HANDLE) -> Result<(), UnexpectedError> {
    if unsafe { kernel32::CloseHandle(handle) } == FALSE {
        return Err(unexpectedError(unsafe { kernel32::GetLastError() }));
    }
    Ok(())
}

/// PORT NOTE: `windows.QueryPerformanceCounter()`.
pub fn QueryPerformanceCounter() -> u64 {
    let mut result: LARGE_INTEGER = 0;
    // SAFETY: QPC never fails on XP+ (std asserts the same).
    unsafe { kernel32::QueryPerformanceCounter(&mut result) };
    result as u64
}

/// PORT NOTE: `windows.QueryPerformanceFrequency()`.
pub fn QueryPerformanceFrequency() -> u64 {
    let mut result: LARGE_INTEGER = 0;
    // SAFETY: QPF never fails on XP+ (std asserts the same).
    unsafe { kernel32::QueryPerformanceFrequency(&mut result) };
    result as u64
}

/// PORT NOTE: `windows.CreateIoCompletionPort`.
///
/// # Safety
/// `file_handle`/`existing_port` must be valid handles (or the documented
/// sentinels).
pub unsafe fn CreateIoCompletionPort(
    file_handle: HANDLE,
    existing_port: HANDLE,
    completion_key: ULONG_PTR,
    concurrent_thread_count: DWORD,
) -> Result<HANDLE, UnexpectedError> {
    let handle = unsafe {
        kernel32::CreateIoCompletionPort(
            file_handle,
            existing_port,
            completion_key,
            concurrent_thread_count,
        )
    };
    if handle.is_null() {
        return Err(unexpectedError(unsafe { kernel32::GetLastError() }));
    }
    Ok(handle)
}

/// PORT NOTE: `windows.PostQueuedCompletionStatus`.
///
/// # Safety
/// `completion_port` must be a live completion port.
pub unsafe fn PostQueuedCompletionStatus(
    completion_port: HANDLE,
    bytes_transferred_count: DWORD,
    completion_key: ULONG_PTR,
    lp_overlapped: *mut OVERLAPPED,
) -> Result<(), UnexpectedError> {
    if unsafe {
        kernel32::PostQueuedCompletionStatus(
            completion_port,
            bytes_transferred_count,
            completion_key,
            lp_overlapped,
        )
    } == FALSE
    {
        return Err(unexpectedError(unsafe { kernel32::GetLastError() }));
    }
    Ok(())
}

/// PORT NOTE: `windows.GetQueuedCompletionStatusEx`'s error set — the iocp `tick`
/// switches on `error.Timeout` and propagates everything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetQueuedCompletionStatusError {
    Aborted,
    Cancelled,
    EOF,
    Timeout,
    Unexpected,
}

/// PORT NOTE: `windows.GetQueuedCompletionStatusEx(port, entries, timeout, alertable)`
/// — returns the number of entries removed.
///
/// # Safety
/// `completion_port` must be a live completion port and `entries` a valid buffer.
pub unsafe fn GetQueuedCompletionStatusEx(
    completion_port: HANDLE,
    entries: &mut [OVERLAPPED_ENTRY],
    timeout_ms: Option<DWORD>,
    alertable: bool,
) -> Result<u32, GetQueuedCompletionStatusError> {
    let mut num_entries_removed: DWORD = 0;
    let ok = unsafe {
        kernel32::GetQueuedCompletionStatusEx(
            completion_port,
            entries.as_mut_ptr(),
            entries.len() as DWORD,
            &mut num_entries_removed,
            timeout_ms.unwrap_or(INFINITE),
            if alertable { TRUE } else { FALSE },
        )
    };
    if ok == FALSE {
        return Err(match unsafe { kernel32::GetLastError() } {
            Win32Error::ACCESS_DENIED => GetQueuedCompletionStatusError::Aborted,
            Win32Error::OPERATION_ABORTED => GetQueuedCompletionStatusError::Cancelled,
            Win32Error::HANDLE_EOF => GetQueuedCompletionStatusError::EOF,
            // WAIT_TIMEOUT
            258 => GetQueuedCompletionStatusError::Timeout,
            _ => GetQueuedCompletionStatusError::Unexpected,
        });
    }
    Ok(num_entries_removed)
}

/// PORT NOTE: `windows.WSASocketW`.
///
/// # Safety
/// Winsock must be initialized.
pub unsafe fn WSASocketW(
    af: i32,
    socket_type: i32,
    protocol: i32,
    protocol_info: *mut c_void,
    g: u32,
    dw_flags: DWORD,
) -> Result<HANDLE, UnexpectedError> {
    let s = unsafe { ws2_32::WSASocketW(af, socket_type, protocol, protocol_info, g, dw_flags) };
    if s == INVALID_SOCKET {
        return Err(unexpectedWSAError(unsafe { ws2_32::WSAGetLastError() }));
    }
    Ok(s as HANDLE)
}

// ---------------------------------------------------------------------------
// `pub const exp = struct { ... }` — windows.zig:39-251, the declarations Zig's
// std lacks.
// ---------------------------------------------------------------------------
pub mod exp {
    use super::{
        c_void, kernel32 as sys_kernel32, unexpectedError, ReadFileError, UnexpectedError,
        Win32Error, WriteFileError, DWORD, FALSE, HANDLE, LARGE_INTEGER, LPCSTR, OVERLAPPED,
        SIZE_T, ULONGLONG, ULONG_PTR,
    };

    pub const STATUS_PENDING: DWORD = 0x0000_0103;
    pub const STILL_ACTIVE: DWORD = STATUS_PENDING;

    /// PORT NOTE: `exp.JOBOBJECT_ASSOCIATE_COMPLETION_PORT`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct JOBOBJECT_ASSOCIATE_COMPLETION_PORT {
        pub CompletionKey: ULONG_PTR,
        pub CompletionPort: HANDLE,
    }

    /// PORT NOTE: `exp.JOBOBJECT_BASIC_LIMIT_INFORMATION`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
        pub PerProcessUserTimeLimit: LARGE_INTEGER,
        pub PerJobUserTimeLimit: LARGE_INTEGER,
        pub LimitFlags: DWORD,
        pub MinimumWorkingSetSize: SIZE_T,
        pub MaximumWorkingSetSize: SIZE_T,
        pub ActiveProcessLimit: DWORD,
        pub Affinity: ULONG_PTR,
        pub PriorityClass: DWORD,
        pub SchedulingClass: DWORD,
    }

    /// PORT NOTE: `exp.IO_COUNTERS`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct IO_COUNTERS {
        pub ReadOperationCount: ULONGLONG,
        pub WriteOperationCount: ULONGLONG,
        pub OtherOperationCount: ULONGLONG,
        pub ReadTransferCount: ULONGLONG,
        pub WriteTransferCount: ULONGLONG,
        pub OtherTransferCount: ULONGLONG,
    }

    /// PORT NOTE: `exp.JOBOBJECT_EXTENDED_LIMIT_INFORMATION`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        pub BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION,
        pub IoInfo: IO_COUNTERS,
        pub ProcessMemoryLimit: SIZE_T,
        pub JobMemoryLimit: SIZE_T,
        pub PeakProcessMemoryUsed: SIZE_T,
        pub PeakJobMemoryUsed: SIZE_T,
    }

    pub const JOB_OBJECT_LIMIT_ACTIVE_PROCESS: DWORD = 0x0000_0008;
    pub const JOB_OBJECT_LIMIT_AFFINITY: DWORD = 0x0000_0010;
    pub const JOB_OBJECT_LIMIT_BREAKAWAY_OK: DWORD = 0x0000_0800;
    pub const JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION: DWORD = 0x0000_0400;
    pub const JOB_OBJECT_LIMIT_JOB_MEMORY: DWORD = 0x0000_0200;
    pub const JOB_OBJECT_LIMIT_JOB_TIME: DWORD = 0x0000_0004;
    pub const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: DWORD = 0x0000_2000;
    pub const JOB_OBJECT_LIMIT_PRESERVE_JOB_TIME: DWORD = 0x0000_0004;
    pub const JOB_OBJECT_LIMIT_PRIORITY_CLASS: DWORD = 0x0000_0020;
    pub const JOB_OBJECT_LIMIT_PROCESS_MEMORY: DWORD = 0x0000_0100;
    pub const JOB_OBJECT_LIMIT_PROCESS_TIME: DWORD = 0x0000_0002;
    pub const JOB_OBJECT_LIMIT_SCHEDULING_CLASS: DWORD = 0x0000_0080;
    pub const JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK: DWORD = 0x0000_1000;
    pub const JOB_OBJECT_LIMIT_SUBSET_AFFINITY: DWORD = 0x0000_4000;
    pub const JOB_OBJECT_LIMIT_WORKINGSET: DWORD = 0x0000_0001;

    /// PORT NOTE: `exp.JOBOBJECT_INFORMATION_CLASS = enum(c_int) { ... }`.
    #[repr(i32)]
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum JOBOBJECT_INFORMATION_CLASS {
        JobObjectAssociateCompletionPortInformation = 7,
        JobObjectBasicLimitInformation = 2,
        JobObjectBasicUIRestrictions = 4,
        JobObjectCpuRateControlInformation = 15,
        JobObjectEndOfJobTimeInformation = 6,
        JobObjectExtendedLimitInformation = 9,
        JobObjectGroupInformation = 11,
        JobObjectGroupInformationEx = 14,
        JobObjectLimitViolationInformation2 = 34,
        JobObjectNetRateControlInformation = 32,
        JobObjectNotificationLimitInformation = 12,
        JobObjectNotificationLimitInformation2 = 33,
        JobObjectSecurityLimitInformation = 5,
    }

    /// PORT NOTE: `exp.JOB_OBJECT_MSG_TYPE = enum(DWORD) { ..., _ }` — a Zig
    /// NON-EXHAUSTIVE enum (the trailing `_`), so this is a newtype over the raw
    /// `DWORD` with the named values as associated constants (the faithful Rust
    /// spelling of an open enum; the same shape WS5's `WriteResult` FFI uses).
    #[repr(transparent)]
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct JOB_OBJECT_MSG_TYPE(pub DWORD);

    impl JOB_OBJECT_MSG_TYPE {
        pub const JOB_OBJECT_MSG_END_OF_JOB_TIME: Self = Self(1);
        pub const JOB_OBJECT_MSG_END_OF_PROCESS_TIME: Self = Self(2);
        pub const JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT: Self = Self(3);
        pub const JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO: Self = Self(4);
        pub const JOB_OBJECT_MSG_NEW_PROCESS: Self = Self(6);
        pub const JOB_OBJECT_MSG_EXIT_PROCESS: Self = Self(7);
        pub const JOB_OBJECT_MSG_ABNORMAL_EXIT_PROCESS: Self = Self(8);
        pub const JOB_OBJECT_MSG_PROCESS_MEMORY_LIMIT: Self = Self(9);
        pub const JOB_OBJECT_MSG_JOB_MEMORY_LIMIT: Self = Self(10);
        pub const JOB_OBJECT_MSG_NOTIFICATION_LIMIT: Self = Self(11);
        pub const JOB_OBJECT_MSG_JOB_CYCLE_TIME_LIMIT: Self = Self(12);
        pub const JOB_OBJECT_MSG_SILO_TERMINATED: Self = Self(13);
    }

    /// PORT NOTE: `exp.kernel32` — the four job-object entry points std lacks.
    pub mod kernel32 {
        use super::super::{BOOL, DWORD, HANDLE, LPCSTR, LPVOID};
        use super::JOBOBJECT_INFORMATION_CLASS;

        extern "system" {
            pub fn GetProcessId(Process: HANDLE) -> DWORD;
            pub fn CreateJobObjectA(
                lpSecurityAttributes: *mut super::super::c_void,
                lpName: LPCSTR,
            ) -> HANDLE;
            pub fn AssignProcessToJobObject(hJob: HANDLE, hProcess: HANDLE) -> BOOL;
            pub fn SetInformationJobObject(
                hJob: HANDLE,
                JobObjectInformationClass: JOBOBJECT_INFORMATION_CLASS,
                lpJobObjectInformation: LPVOID,
                cbJobObjectInformationLength: DWORD,
            ) -> BOOL;
        }
    }

    /// PORT NOTE: `exp.CreateFileError = error{} || posix.UnexpectedError`.
    pub type CreateFileError = UnexpectedError;

    /// PORT NOTE: `exp.CreateFile(...)`.
    ///
    /// # Safety
    /// `lpFileName` must be a NUL-terminated UTF-16 path.
    pub unsafe fn CreateFile(
        lpFileName: *const u16,
        dwDesiredAccess: DWORD,
        dwShareMode: DWORD,
        lpSecurityAttributes: *mut c_void,
        dwCreationDisposition: DWORD,
        dwFlagsAndAttributes: DWORD,
        hTemplateFile: HANDLE,
    ) -> Result<HANDLE, CreateFileError> {
        let handle = unsafe {
            sys_kernel32::CreateFileW(
                lpFileName,
                dwDesiredAccess,
                dwShareMode,
                lpSecurityAttributes,
                dwCreationDisposition,
                dwFlagsAndAttributes,
                hTemplateFile,
            )
        };
        if handle == super::INVALID_HANDLE_VALUE {
            return Err(unexpectedError(unsafe { sys_kernel32::GetLastError() }));
        }
        Ok(handle)
    }

    /// PORT NOTE: `exp.ReadFile(handle, buffer, overlapped) ReadFileError!?usize`
    /// — `null` means IO_PENDING (the async submit succeeded).
    ///
    /// # Safety
    /// `handle` must be live and `buffer`/`overlapped` valid for the whole call
    /// (and, when pending, until the completion fires).
    pub unsafe fn ReadFile(
        handle: HANDLE,
        buffer: *mut [u8],
        overlapped: *mut OVERLAPPED,
    ) -> Result<Option<usize>, ReadFileError> {
        let mut read: DWORD = 0;
        let result = unsafe {
            sys_kernel32::ReadFile(
                handle,
                buffer.cast::<c_void>(),
                buffer.len() as DWORD,
                &mut read,
                overlapped,
            )
        };
        if result == FALSE {
            return match unsafe { sys_kernel32::GetLastError() } {
                Win32Error::IO_PENDING => Ok(None),
                Win32Error::OPERATION_ABORTED => Err(ReadFileError::OperationAborted),
                Win32Error::BROKEN_PIPE => Err(ReadFileError::BrokenPipe),
                _ => Err(ReadFileError::Unexpected),
            };
        }

        Ok(Some(read as usize))
    }

    /// PORT NOTE: `exp.WriteFile(handle, buffer, overlapped) WriteFileError!?usize`.
    ///
    /// # Safety
    /// As `ReadFile`.
    pub unsafe fn WriteFile(
        handle: HANDLE,
        buffer: *const [u8],
        overlapped: *mut OVERLAPPED,
    ) -> Result<Option<usize>, WriteFileError> {
        let mut written: DWORD = 0;
        let result = unsafe {
            sys_kernel32::WriteFile(
                handle,
                buffer.cast::<c_void>(),
                buffer.len() as DWORD,
                &mut written,
                overlapped,
            )
        };
        if result == FALSE {
            return match unsafe { sys_kernel32::GetLastError() } {
                Win32Error::IO_PENDING => Ok(None),
                Win32Error::OPERATION_ABORTED => Err(WriteFileError::OperationAborted),
                Win32Error::BROKEN_PIPE => Err(WriteFileError::BrokenPipe),
                _ => Err(WriteFileError::Unexpected),
            };
        }

        Ok(Some(written as usize))
    }

    /// PORT NOTE: `exp.DeleteFileError = error{} || posix.UnexpectedError`.
    pub type DeleteFileError = UnexpectedError;

    /// PORT NOTE: `exp.DeleteFile(name)`.
    ///
    /// # Safety
    /// `name` must be a NUL-terminated UTF-16 path.
    pub unsafe fn DeleteFile(name: *const u16) -> Result<(), DeleteFileError> {
        if unsafe { super::DeleteFileW(name) } == FALSE {
            return Err(unexpectedError(unsafe { sys_kernel32::GetLastError() }));
        }
        Ok(())
    }

    /// PORT NOTE: `exp.CreateJobObjectError = error{AlreadyExists} || posix.UnexpectedError`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CreateJobObjectError {
        AlreadyExists,
        Unexpected,
    }

    /// PORT NOTE: `exp.CreateJobObject(lpSecurityAttributes, lpName)`.
    ///
    /// # Safety
    /// The attribute/name pointers must be valid or null.
    pub unsafe fn CreateJobObject(
        lpSecurityAttributes: *mut c_void,
        lpName: LPCSTR,
    ) -> Result<HANDLE, CreateJobObjectError> {
        let handle = unsafe { kernel32::CreateJobObjectA(lpSecurityAttributes, lpName) };
        match unsafe { sys_kernel32::GetLastError() } {
            Win32Error::SUCCESS => Ok(handle),
            Win32Error::ALREADY_EXISTS => Err(CreateJobObjectError::AlreadyExists),
            _ => Err(CreateJobObjectError::Unexpected),
        }
    }

    /// PORT NOTE: `exp.AssignProcessToJobObject(hJob, hProcess)`.
    ///
    /// # Safety
    /// Both handles must be live.
    pub unsafe fn AssignProcessToJobObject(
        hJob: HANDLE,
        hProcess: HANDLE,
    ) -> Result<(), UnexpectedError> {
        if unsafe { kernel32::AssignProcessToJobObject(hJob, hProcess) } == FALSE {
            return Err(unexpectedError(unsafe { sys_kernel32::GetLastError() }));
        }
        Ok(())
    }

    /// PORT NOTE: `exp.SetInformationJobObject(...)`.
    ///
    /// # Safety
    /// `hJob` must be live and `lpJobObjectInformation` must point at a struct of
    /// the class named by `JobObjectInformationClass`, of at least
    /// `cbJobObjectInformationLength` bytes.
    pub unsafe fn SetInformationJobObject(
        hJob: HANDLE,
        JobObjectInformationClass: JOBOBJECT_INFORMATION_CLASS,
        lpJobObjectInformation: *mut c_void,
        cbJobObjectInformationLength: DWORD,
    ) -> Result<(), UnexpectedError> {
        if unsafe {
            kernel32::SetInformationJobObject(
                hJob,
                JobObjectInformationClass,
                lpJobObjectInformation,
                cbJobObjectInformationLength,
            )
        } == FALSE
        {
            return Err(unexpectedError(unsafe { sys_kernel32::GetLastError() }));
        }
        Ok(())
    }
}
