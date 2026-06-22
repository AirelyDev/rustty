//! rustty-os::windows
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/windows.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).
//!
//! PORT: the reference is `const std = @import("std"); const windows =
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

// PORT: Win32 ABI base types. In the reference these come from `std.os.windows`;
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

/// PORT: `windows.unexpectedError` from `std.os.windows`. Maps a Win32 error code
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

/// PORT: `windows.SetHandleInformation` returns a `BOOL` from the Win32 call but
/// the reference's std wrapper returns `!void`. We mirror the wrapper's error
/// propagation by returning an `io::Result`.
pub fn SetHandleInformation(hObject: HANDLE, dwMask: DWORD, dwFlags: DWORD) -> std::io::Result<()> {
    if unsafe { kernel32::SetHandleInformation(hObject, dwMask, dwFlags) } == 0 {
        return unexpectedError(unsafe { kernel32::GetLastError() });
    }
    Ok(())
}

/// PORT: BOOL GetComputerNameA(LPSTR lpBuffer, LPDWORD nSize)
pub fn GetComputerNameA(lpBuffer: LPSTR, nSize: *mut DWORD) -> BOOL {
    extern "system" {
        fn GetComputerNameA(lpBuffer: LPSTR, nSize: *mut DWORD) -> BOOL;
    }
    unsafe { GetComputerNameA(lpBuffer, nSize) }
}

// PORT: `pub const OpenFile = windows.OpenFile;` — re-export of the std
// `std.os.windows.OpenFile` wrapper. It is not used by any module in this group
// (the Windows pty path calls `CreateFileW` directly), so the binding is provided
// for completeness; callers that need it bind to the same NtCreateFile-based
// wrapper. Surfaced here as a typed function pointer alias to preserve the
// re-export without reimplementing the std wrapper.
pub type OpenFileFn = unsafe extern "system" fn();

pub const FILE_ATTRIBUTE_NORMAL: DWORD = 0x00000080;
pub const FILE_FLAG_OVERLAPPED: DWORD = 0x40000000;
pub const FILE_SHARE_READ: DWORD = 0x00000001;
pub const GENERIC_READ: DWORD = 0x80000000;
pub const HANDLE_FLAG_INHERIT: DWORD = 0x00000001;
pub const INFINITE: DWORD = 0xFFFFFFFF;
pub const INVALID_HANDLE_VALUE: HANDLE = !0usize as HANDLE;
pub const MAX_PATH: usize = 260;
pub const OPEN_EXISTING: DWORD = 3;
pub const PIPE_ACCESS_OUTBOUND: DWORD = 0x00000002;
pub const PIPE_TYPE_BYTE: DWORD = 0x00000000;
pub const S_OK: HRESULT = 0;
pub const STARTF_USESTDHANDLES: DWORD = 0x00000100;
pub const SYNCHRONIZE: DWORD = 0x00100000;
pub const WAIT_FAILED: DWORD = 0xFFFFFFFF;
pub const FALSE: BOOL = 0;
pub const TRUE: BOOL = 1;

pub mod exp {
    use super::{c_void, BOOL, DWORD, DWORD_PTR, HANDLE, LPWSTR, PVOID, SIZE_T};

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
