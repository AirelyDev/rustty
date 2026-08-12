//! rustty-os::windows
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/windows.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).
//!
//! NOTE: The Windows part of the Zig stdlib is currently in the process of
//! having most of its features removed, with the ultimate goal of switching to
//! serve as a support for higher-level functionality offered in places like
//! `std.Io` only. As such this file serves as a "bridge" between type
//! information (mostly coming from stdlib) and manually-defined constants and
//! external functions.
//!
//! PORT NOTE (`e8525c0fd`, `.port/phase-3/NON_PORTS.md` §E25): that upstream
//! note describes a migration rustty was born on the far side of — Rust has no
//! `std.os.windows` to lean on, so this module has always declared its own
//! constants and `extern`s. The rewrite is a CONVERGENCE, not a delta. The
//! constants and `extern`s upstream added for symbols no rustty code calls
//! (`FILE_NON_DIRECTORY_FILE`, the `PROC_THREAD_*` numeric set, and the `ntdll`
//! `NtCreateFile` path) are deliberately not transcribed: they would be dead
//! surface a `dead_code` sweep removes again.
//!
//! PORT NOTE (W2D_WINDOWS_OS WI-5): `MEM_COMMIT`/`MEM_RELEASE`/`MEM_RESERVE`
//! and `PAGE_READWRITE` (`src/os/windows.zig:84-88`) plus
//! `exp.kernel32.VirtualAlloc`/`VirtualFree` (`:205-216`) used to be covered by
//! that "dead surface" carve-out. That rationale is now OBSOLETE: the port of
//! `terminal/page.zig`'s `AllocWindows` (`rustty-vt/src/page.rs`) is their live
//! caller — the same caller the reference has (`page.zig:62,76`), reaching them
//! through this module exactly as the reference reaches
//! `windows.exp.kernel32.VirtualAlloc`. They are transcribed below.
//!
//! PORT NOTE: the reference is `const std = @import("std"); const windows =
//! std.os.windows;` followed by a set of re-exports of Win32 constants, types,
//! and functions "so we can just import one file". Zig's `std.os.windows`
//! provides those declarations; Rust has no equivalent std module, so we bind
//! directly to the SAME Win32 system DLLs (`kernel32`) and define the Win32 ABI
//! type aliases here (R3: bindings to the same system library, not a
//! reimplementation). The whole module is Windows-only — matching the
//! reference, which is only referenced from Windows code paths.
#![cfg(windows)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(dead_code)]

// PORT NOTE: Win32 ABI base types. In the reference these come from `std.os.windows`;
// here they are local aliases to the same C ABI types so the bindings below have
// matching signatures.
use std::os::raw::{c_int, c_void};

pub type BOOL = c_int;
pub type DWORD = u32;
pub type DWORD_PTR = usize;
pub type SIZE_T = usize;
pub type WORD = u16;
pub type HANDLE = *mut c_void;
pub type HRESULT = i32;
pub type LPVOID = *mut c_void;
pub type PVOID = *mut c_void;
pub type LPSTR = *mut u8;
pub type LPWSTR = *mut u16;
pub type WCHAR = u16;
pub type CHAR = u8;
pub type ULONG_PTR = usize;
/// PORT NOTE (WS20): the Win32 `UINT` (`TerminateProcess`'s exit code).
pub type UINT = u32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct COORD {
    pub X: i16,
    pub Y: i16,
}

#[repr(C)]
pub struct SECURITY_ATTRIBUTES {
    pub nLength: DWORD,
    pub lpSecurityDescriptor: *mut c_void,
    pub bInheritHandle: BOOL,
}

#[repr(C)]
pub struct STARTUPINFOW {
    pub cb: DWORD,
    pub lpReserved: LPWSTR,
    pub lpDesktop: LPWSTR,
    pub lpTitle: LPWSTR,
    pub dwX: DWORD,
    pub dwY: DWORD,
    pub dwXSize: DWORD,
    pub dwYSize: DWORD,
    pub dwXCountChars: DWORD,
    pub dwYCountChars: DWORD,
    pub dwFillAttribute: DWORD,
    pub dwFlags: DWORD,
    pub wShowWindow: WORD,
    pub cbReserved2: WORD,
    pub lpReserved2: *mut u8,
    pub hStdInput: HANDLE,
    pub hStdOutput: HANDLE,
    pub hStdError: HANDLE,
}

#[repr(C)]
pub struct PROCESS_INFORMATION {
    pub hProcess: HANDLE,
    pub hThread: HANDLE,
    pub dwProcessId: DWORD,
    pub dwThreadId: DWORD,
}

// Export any constants or functions we need from the Windows API so
// we can just import one file.
pub mod kernel32 {
    use super::{BOOL, DWORD, HANDLE, LPWSTR};
    extern "system" {
        pub fn GetLastError() -> DWORD;
        pub fn CreateNamedPipeW(
            lpName: *const u16,
            dwOpenMode: DWORD,
            dwPipeMode: DWORD,
            nMaxInstances: DWORD,
            nOutBufferSize: DWORD,
            nInBufferSize: DWORD,
            nDefaultTimeOut: DWORD,
            lpSecurityAttributes: *const super::SECURITY_ATTRIBUTES,
        ) -> HANDLE;
        pub fn CreateFileW(
            lpFileName: *const u16,
            dwDesiredAccess: DWORD,
            dwShareMode: DWORD,
            lpSecurityAttributes: *mut super::SECURITY_ATTRIBUTES,
            dwCreationDisposition: DWORD,
            dwFlagsAndAttributes: DWORD,
            hTemplateFile: HANDLE,
        ) -> HANDLE;
        pub fn SetHandleInformation(hObject: HANDLE, dwMask: DWORD, dwFlags: DWORD) -> BOOL;
        #[allow(unused)]
        pub fn unused_lpwstr(_p: LPWSTR);
    }
}

/// PORT NOTE: `windows.unexpectedError` from `std.os.windows`. Maps a Win32 error code
/// to the same kind of unexpected-error result the reference returns. We return
/// an `io::Error` so the bytes-for-bytes error path is preserved.
pub fn unexpectedError<T>(err: DWORD) -> std::io::Result<T> {
    Err(std::io::Error::from_raw_os_error(err as i32))
}

pub fn CloseHandle(hObject: HANDLE) -> BOOL {
    extern "system" {
        fn CloseHandle(hObject: HANDLE) -> BOOL;
    }
    unsafe { CloseHandle(hObject) }
}

pub fn GetCurrentProcessId() -> DWORD {
    extern "system" {
        fn GetCurrentProcessId() -> DWORD;
    }
    unsafe { GetCurrentProcessId() }
}

/// PORT NOTE: `windows.SetHandleInformation` returns a `BOOL` from the Win32 call but
/// the reference's std wrapper returns `!void`. We mirror the wrapper's error
/// propagation by returning an `io::Result`.
pub fn SetHandleInformation(hObject: HANDLE, dwMask: DWORD, dwFlags: DWORD) -> std::io::Result<()> {
    if unsafe { kernel32::SetHandleInformation(hObject, dwMask, dwFlags) } == 0 {
        return unexpectedError(unsafe { kernel32::GetLastError() });
    }
    Ok(())
}

/// PORT NOTE: BOOL GetComputerNameA(LPSTR lpBuffer, LPDWORD nSize)
pub fn GetComputerNameA(lpBuffer: LPSTR, nSize: *mut DWORD) -> BOOL {
    extern "system" {
        fn GetComputerNameA(lpBuffer: LPSTR, nSize: *mut DWORD) -> BOOL;
    }
    unsafe { GetComputerNameA(lpBuffer, nSize) }
}

// PORT NOTE: `pub const OpenFile = windows.OpenFile;` — re-export of the std
// `std.os.windows.OpenFile` wrapper. It is not used by any module in this group
// (the Windows pty path calls `CreateFileW` directly), so the binding is provided
// for completeness; callers that need it bind to the same NtCreateFile-based
// wrapper. Surfaced here as a typed function pointer alias to preserve the
// re-export without reimplementing the std wrapper.
pub type OpenFileFn = unsafe extern "system" fn();

// Bit-field and enum constant values
pub const FILE_ATTRIBUTE_NORMAL: DWORD = 0x00000080;
pub const FILE_FLAG_OVERLAPPED: DWORD = 0x40000000;
pub const FILE_SHARE_READ: DWORD = 0x00000001;
pub const GENERIC_READ: DWORD = 0x80000000;
pub const HANDLE_FLAG_INHERIT: DWORD = 0x00000001;
pub const INFINITE: DWORD = 0xFFFFFFFF;
pub const INVALID_HANDLE_VALUE: HANDLE = !0usize as HANDLE;
pub const MAX_PATH: usize = 260;
pub const MEM_COMMIT: DWORD = 0x1000;
pub const MEM_RELEASE: DWORD = 0x8000;
pub const MEM_RESERVE: DWORD = 0x2000;
pub const OPEN_EXISTING: DWORD = 3; // Known as FILE_OPEN in Windows docs
pub const PAGE_READWRITE: DWORD = 0x04;
pub const PIPE_ACCESS_OUTBOUND: DWORD = 0x00000002;
pub const PIPE_TYPE_BYTE: DWORD = 0x00000000;
pub const S_OK: HRESULT = 0;
pub const STARTF_USESTDHANDLES: DWORD = 0x00000100;
pub const SYNCHRONIZE: DWORD = 0x00100000;
pub const WAIT_FAILED: DWORD = 0xFFFFFFFF;
pub const FALSE: BOOL = 0;
pub const TRUE: BOOL = 1;

// Types needed for ntdll calls
//
// PORT NOTE (§E25): upstream re-exports `ACCESS_MASK`, `IO_STATUS_BLOCK`,
// `NTSTATUS` and `OBJECT_ATTRIBUTES` from `std.os.windows` for its new
// `ntdll.NtCreateFile` binding. rustty's Windows pty path calls `CreateFileW`
// directly (see `OpenFileFn` above), so no `ntdll` call site exists and the
// four types would be dead surface.

pub mod exp {
    // Only the names `exp`'s OWN items use: `exp::kernel32` below has its own
    // import list, and `HPCON`/`STARTUPINFOEX` name their types through
    // `super::`.
    use super::{c_void, DWORD};

    pub type HPCON = super::LPVOID;

    pub const CREATE_UNICODE_ENVIRONMENT: DWORD = 0x00000400;
    pub const EXTENDED_STARTUPINFO_PRESENT: DWORD = 0x00080000;
    pub type LPPROC_THREAD_ATTRIBUTE_LIST = *mut c_void;
    pub const FILE_FLAG_FIRST_PIPE_INSTANCE: DWORD = 0x00080000;

    pub const STATUS_PENDING: DWORD = 0x00000103;
    pub const STILL_ACTIVE: DWORD = STATUS_PENDING;

    #[repr(C)]
    pub struct STARTUPINFOEX {
        pub StartupInfo: super::STARTUPINFOW,
        pub lpAttributeList: LPPROC_THREAD_ATTRIBUTE_LIST,
    }

    pub mod kernel32 {
        use super::super::{
            BOOL, COORD, DWORD, HANDLE, HRESULT, LPWSTR, PROCESS_INFORMATION, SECURITY_ATTRIBUTES,
        };
        use super::super::{DWORD_PTR, PVOID, SIZE_T};
        use super::{HPCON, LPPROC_THREAD_ATTRIBUTE_LIST};
        extern "system" {
            pub fn CreatePipe(
                hReadPipe: *mut HANDLE,
                hWritePipe: *mut HANDLE,
                lpPipeAttributes: *const SECURITY_ATTRIBUTES,
                nSize: DWORD,
            ) -> BOOL;
            pub fn CreatePseudoConsole(
                size: COORD,
                hInput: HANDLE,
                hOutput: HANDLE,
                dwFlags: DWORD,
                phPC: *mut HPCON,
            ) -> HRESULT;
            pub fn ResizePseudoConsole(hPC: HPCON, size: COORD) -> HRESULT;
            pub fn ClosePseudoConsole(hPC: HPCON);
            pub fn InitializeProcThreadAttributeList(
                lpAttributeList: LPPROC_THREAD_ATTRIBUTE_LIST,
                dwAttributeCount: DWORD,
                dwFlags: DWORD,
                lpSize: *mut SIZE_T,
            ) -> BOOL;
            pub fn UpdateProcThreadAttribute(
                lpAttributeList: LPPROC_THREAD_ATTRIBUTE_LIST,
                dwFlags: DWORD,
                Attribute: DWORD_PTR,
                lpValue: PVOID,
                cbSize: SIZE_T,
                lpPreviousValue: PVOID,
                lpReturnSize: *mut SIZE_T,
            ) -> BOOL;
            pub fn PeekNamedPipe(
                hNamedPipe: HANDLE,
                lpBuffer: super::super::LPVOID,
                nBufferSize: DWORD,
                lpBytesRead: *mut DWORD,
                lpTotalBytesAvail: *mut DWORD,
                lpBytesLeftThisMessage: *mut DWORD,
            ) -> BOOL;
            // PORT NOTE (WS20): the reference forwards `std.os.windows.kernel32`
            // wholesale (`pub const kernel32 = windows.kernel32;`), so every entry
            // point termio's Windows paths call is reachable as
            // `internal_os.windows.(exp.)kernel32.*`. These three are the ones
            // `termio.Exec` names — `ReadThread.threadMainWindows`'s `ReadFile`,
            // `Exec.threadExit`'s `CancelIoEx`, and `killCommand`'s
            // `TerminateProcess` — and they were missing from this hand-rolled
            // forwarding set.
            pub fn ReadFile(
                hFile: HANDLE,
                lpBuffer: super::super::LPVOID,
                nNumberOfBytesToRead: DWORD,
                lpNumberOfBytesRead: *mut DWORD,
                lpOverlapped: super::super::LPVOID,
            ) -> BOOL;
            pub fn CancelIoEx(hFile: HANDLE, lpOverlapped: super::super::LPVOID) -> BOOL;
            // The reference declares these two right after `CloseHandle` in
            // `exp.kernel32` (`src/os/windows.zig:205-216`). Their live caller
            // is `terminal/page.zig`'s `AllocWindows`, which reaches them as
            // `windows.exp.kernel32.VirtualAlloc/VirtualFree` — i.e. THROUGH
            // this module, exactly as the port's `rustty-vt/src/page.rs`
            // windows arm does. The reference's `?LPVOID` optionals are Rust
            // raw pointers, which are already nullable.
            pub fn VirtualAlloc(
                lpAddress: super::super::LPVOID,
                dwSize: SIZE_T,
                flAllocationType: DWORD,
                flProtect: DWORD,
            ) -> super::super::LPVOID;
            pub fn VirtualFree(
                lpAddress: super::super::LPVOID,
                dwSize: SIZE_T,
                dwFreeType: DWORD,
            ) -> BOOL;
            pub fn TerminateProcess(hProcess: HANDLE, uExitCode: super::super::UINT) -> BOOL;
            // Duplicated here because lpCommandLine is not marked optional in zig std
            pub fn CreateProcessW(
                lpApplicationName: LPWSTR,
                lpCommandLine: LPWSTR,
                lpProcessAttributes: *mut SECURITY_ATTRIBUTES,
                lpThreadAttributes: *mut SECURITY_ATTRIBUTES,
                bInheritHandles: BOOL,
                dwCreationFlags: DWORD,
                lpEnvironment: *mut super::super::c_void,
                lpCurrentDirectory: LPWSTR,
                lpStartupInfo: *mut super::super::STARTUPINFOW,
                lpProcessInformation: *mut PROCESS_INFORMATION,
            ) -> BOOL;
            /// https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getcomputernamea
            pub fn GetComputerNameA(lpBuffer: super::super::LPSTR, nSize: *mut DWORD) -> BOOL;
            /// https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-gettemppathw
            pub fn GetTempPathW(nBufferLength: DWORD, lpBuffer: LPWSTR) -> DWORD;
        }
    }

    pub const PROC_THREAD_ATTRIBUTE_NUMBER: DWORD = 0x0000FFFF;
    pub const PROC_THREAD_ATTRIBUTE_THREAD: DWORD = 0x00010000;
    pub const PROC_THREAD_ATTRIBUTE_INPUT: DWORD = 0x00020000;
    pub const PROC_THREAD_ATTRIBUTE_ADDITIVE: DWORD = 0x00040000;

    #[repr(u32)]
    #[derive(Clone, Copy)]
    pub enum ProcThreadAttributeNumber {
        ProcThreadAttributePseudoConsole = 22,
    }

    /// Corresponds to the ProcThreadAttributeValue define in WinBase.h
    pub const fn ProcThreadAttributeValue(
        attribute: ProcThreadAttributeNumber,
        thread: bool,
        input: bool,
        additive: bool,
    ) -> DWORD {
        (attribute as DWORD & PROC_THREAD_ATTRIBUTE_NUMBER)
            | (if thread {
                PROC_THREAD_ATTRIBUTE_THREAD
            } else {
                0
            })
            | (if input {
                PROC_THREAD_ATTRIBUTE_INPUT
            } else {
                0
            })
            | (if additive {
                PROC_THREAD_ATTRIBUTE_ADDITIVE
            } else {
                0
            })
    }

    pub const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: DWORD = ProcThreadAttributeValue(
        ProcThreadAttributeNumber::ProcThreadAttributePseudoConsole,
        false,
        true,
        false,
    );
}
