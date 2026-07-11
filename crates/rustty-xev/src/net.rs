//! net.rs — the `std.net.Address` subset libxev's kqueue `Operation` uses (D6,
//! XEV_BLUEPRINT §2.1 chunk C3).
//!
//! `backend/kqueue.zig` stores `addr: std.net.Address` in its `connect`/`sendto`/
//! `recvfrom` operations and the loop's `connect` `start` arm calls
//! `posix.system.connect(v.socket, &v.addr.any, v.addr.getOsSockLen())`; the
//! "socket accept/connect/send/recv/close" + "socket accept/cancel" tests build
//! one with `net.Address.parseIp4("127.0.0.1", 3131)`. `std.net.Address` is Zig
//! std (not a libxev source file), so — faithful to the port's "reproduce what
//! libxev's macOS build compiles" rule — only the EXACT subset the kqueue backend
//! and its C3 tests reach is ported here: the `extern union Address`
//! (any/in/in6/un), `Ip4Address`/`Ip6Address`, `parseIp4`/`initIp4`,
//! `getOsSockLen`, and the `posix.sockaddr.{in,in6,un}` extern structs they wrap.
//! Ported byte-faithfully from Zig 0.15 `std/net.zig` and `std/c.zig` (the macOS
//! `sockaddr` arm), with Zig field names preserved. Module is macOS-gated in
//! lib.rs, like the whole kqueue port.
//
// PORT NOTE: `posix.socklen_t`/`sa_family_t`/`in_port_t` are the platform C ints;
// `posix.AF.{INET,INET6,UNIX}` are the address-family selectors. On macOS
// `sa_family_t` is `u8` (the `sockaddr.len`/`.family` byte pair), matching the
// `std.c` darwin `sockaddr` arm. These are bound from `libc` where present.

// (module is `#[cfg(target_os = "macos")]`-gated in lib.rs)
#![allow(non_camel_case_types)]

use core::mem::size_of;

/// PORT NOTE: `posix.socklen_t` (macOS `u32`).
pub type socklen_t = libc::socklen_t;
/// PORT NOTE: `posix.sa_family_t` (the darwin `sockaddr` `family` byte, `u8`).
pub type sa_family_t = u8;
/// PORT NOTE: `posix.in_port_t` (network-order port, `u16`).
pub type in_port_t = u16;

/// PORT NOTE: `posix.AF.INET` (`std.c` darwin `AF.INET`).
pub const AF_INET: sa_family_t = libc::AF_INET as sa_family_t;
/// PORT NOTE: `posix.AF.INET6`.
pub const AF_INET6: sa_family_t = libc::AF_INET6 as sa_family_t;
/// PORT NOTE: `posix.AF.UNIX`.
pub const AF_UNIX: sa_family_t = libc::AF_UNIX as sa_family_t;

/// PORT NOTE: the `std.c` (darwin) `posix.sockaddr` family — the extern structs the
/// `Address` union overlays. Field names + defaults mirror the Zig `extern
/// struct`s verbatim (`len = @sizeOf(...)`, `family = AF.*`); Rust has no
/// per-field defaults, so `Ip4Address::{parse,init}` set `len`/`family`/`zero`
/// explicitly at construction (exactly the values the Zig defaults supply).
pub mod sockaddr {
    use super::{in_port_t, sa_family_t};

    /// PORT NOTE: `posix.sockaddr` (the generic `any`): `{ len, family, data: [14]u8 }`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Any {
        pub len: u8,
        pub family: sa_family_t,
        pub data: [u8; 14],
    }

    /// PORT NOTE: `posix.sockaddr.in`: `{ len = @sizeOf(in), family = AF.INET, port,
    /// addr, zero: [8]u8 = {0}*8 }`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct In {
        pub len: u8,
        pub family: sa_family_t,
        pub port: in_port_t,
        pub addr: u32,
        pub zero: [u8; 8],
    }

    /// PORT NOTE: `posix.sockaddr.in6`: `{ len = @sizeOf(in6), family = AF.INET6, port,
    /// flowinfo, addr: [16]u8, scope_id }`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct In6 {
        pub len: u8,
        pub family: sa_family_t,
        pub port: in_port_t,
        pub flowinfo: u32,
        pub addr: [u8; 16],
        pub scope_id: u32,
    }

    /// PORT NOTE: `posix.sockaddr.un` (UNIX domain socket): `{ len = @sizeOf(un),
    /// family = AF.UNIX, path: [104]u8 }`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Un {
        pub len: u8,
        pub family: sa_family_t,
        pub path: [u8; 104],
    }
}

/// PORT NOTE: `std.net.IParseError` subset — the `Ip4Address.parse` error set
/// (`IPv4ParseError = IPParseError || error{NonCanonical}`; the `std.math`
/// overflow surfaces as `Overflow`). Only the variants `parse` returns are
/// modeled; consumers `catch` them or (in the tests) never hit them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IPv4ParseError {
    Overflow,
    InvalidEnd,
    InvalidCharacter,
    Incomplete,
    NonCanonical,
}

/// PORT NOTE: `std.net.Ip4Address = extern struct { sa: posix.sockaddr.in }`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Ip4Address {
    pub sa: sockaddr::In,
}

impl Ip4Address {
    /// PORT NOTE: `Ip4Address.parse(buf, port)`.
    pub fn parse(buf: &[u8], port: u16) -> core::result::Result<Ip4Address, IPv4ParseError> {
        let mut result: Ip4Address = Ip4Address {
            sa: sockaddr::In {
                // .port = mem.nativeToBig(u16, port)
                port: port.to_be(),
                // .addr = undefined (filled by out_ptr below)
                addr: 0,
                // len/family/zero take the Zig `sockaddr.in` field defaults.
                len: size_of::<sockaddr::In>() as u8,
                family: AF_INET,
                zero: [0, 0, 0, 0, 0, 0, 0, 0],
            },
        };
        // const out_ptr = mem.asBytes(&result.sa.addr);
        let out_ptr: &mut [u8; 4] =
            unsafe { &mut *(core::ptr::addr_of_mut!(result.sa.addr).cast::<[u8; 4]>()) };

        let mut x: u8 = 0;
        let mut index: u8 = 0;
        let mut saw_any_digits = false;
        let mut has_zero_prefix = false;
        for &c in buf {
            if c == b'.' {
                if !saw_any_digits {
                    return Err(IPv4ParseError::InvalidCharacter);
                }
                if index == 3 {
                    return Err(IPv4ParseError::InvalidEnd);
                }
                out_ptr[index as usize] = x;
                index += 1;
                x = 0;
                saw_any_digits = false;
                has_zero_prefix = false;
            } else if c.is_ascii_digit() {
                if c == b'0' && !saw_any_digits {
                    has_zero_prefix = true;
                } else if has_zero_prefix {
                    return Err(IPv4ParseError::NonCanonical);
                }
                saw_any_digits = true;
                // x = try std.math.mul(u8, x, 10);
                x = x.checked_mul(10).ok_or(IPv4ParseError::Overflow)?;
                // x = try std.math.add(u8, x, c - '0');
                x = x.checked_add(c - b'0').ok_or(IPv4ParseError::Overflow)?;
            } else {
                return Err(IPv4ParseError::InvalidCharacter);
            }
        }
        if index == 3 && saw_any_digits {
            out_ptr[index as usize] = x;
            return Ok(result);
        }

        Err(IPv4ParseError::Incomplete)
    }

    /// PORT NOTE: `Ip4Address.init(addr, port)`.
    pub fn init(addr: [u8; 4], port: u16) -> Ip4Address {
        Ip4Address {
            sa: sockaddr::In {
                // .port = mem.nativeToBig(u16, port)
                port: port.to_be(),
                // .addr = @as(*align(1) const u32, @ptrCast(&addr)).*
                addr: u32::from_ne_bytes(addr),
                len: size_of::<sockaddr::In>() as u8,
                family: AF_INET,
                zero: [0, 0, 0, 0, 0, 0, 0, 0],
            },
        }
    }

    /// PORT NOTE: `Ip4Address.getOsSockLen(self)`.
    pub fn get_os_sock_len(self) -> socklen_t {
        size_of::<sockaddr::In>() as socklen_t
    }
}

/// PORT NOTE: `std.net.Ip6Address = extern struct { sa: posix.sockaddr.in6 }`. Only
/// the struct + `getOsSockLen` are reached by the kqueue backend (the INET6 arm
/// of `Address.getOsSockLen`; the tests are IPv4-only). `Ip6Address.parse` is
/// unused by the kqueue backend and is not ported.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Ip6Address {
    pub sa: sockaddr::In6,
}

impl Ip6Address {
    /// PORT NOTE: `Ip6Address.getOsSockLen(self)`.
    pub fn get_os_sock_len(self) -> socklen_t {
        size_of::<sockaddr::In6>() as socklen_t
    }
}

/// PORT NOTE: `std.net.Address = extern union { any, in, in6, un }`. Union field
/// access is `unsafe` in Rust (exactly Zig's active-field discipline); the
/// kqueue backend reads `.any.family` (dispatch) + `&.any` (the connect
/// syscall) and writes via `.in`. `#[derive(Copy)]`: all members are POD
/// `extern struct`s (Zig copies `Address` by value into the `Operation`).
//
// PORT NOTE: Zig field `in` is a Rust keyword → renamed `in_` (the only rename;
// R2 identifier-mapping). `un` is `void` on platforms without unix sockets; on
// macOS (`has_unix_sockets`) it is `posix.sockaddr.un`.
#[repr(C)]
#[derive(Clone, Copy)]
pub union Address {
    pub any: sockaddr::Any,
    pub in_: Ip4Address,
    pub in6: Ip6Address,
    pub un: sockaddr::Un,
}

impl Address {
    /// PORT NOTE: `Address.parseIp4(buf, port)`.
    pub fn parse_ip4(buf: &[u8], port: u16) -> core::result::Result<Address, IPv4ParseError> {
        Ok(Address {
            in_: Ip4Address::parse(buf, port)?,
        })
    }

    /// PORT NOTE: `Address.initIp4(addr, port)`.
    pub fn init_ip4(addr: [u8; 4], port: u16) -> Address {
        Address {
            in_: Ip4Address::init(addr, port),
        }
    }

    /// PORT NOTE: `Address.getOsSockLen(self)`.
    //
    // PORT NOTE: the `else => unreachable` (non-IP/UNIX family) ports as a Rust
    // `unreachable!()`; `has_unix_sockets` is always true on macOS so the
    // `if (!has_unix_sockets) unreachable` guard collapses to the live UNIX arm.
    pub fn get_os_sock_len(self) -> socklen_t {
        // SAFETY: reading the `any.family` byte is valid for any active union
        // member (all overlay `len`/`family` as their first two bytes).
        match unsafe { self.any.family } {
            AF_INET => unsafe { self.in_.get_os_sock_len() },
            AF_INET6 => unsafe { self.in6.get_os_sock_len() },
            AF_UNIX => {
                // Using the full length of the structure here is more portable than returning
                // the number of bytes actually used by the currently stored path.
                // This also is correct regardless if we are passing a socket address to the kernel
                // (e.g. in bind, connect, sendto) since we ensure the path is 0 terminated in
                // initUnix() or if we are receiving a socket address from the kernel and must
                // provide the full buffer size (e.g. getsockname, getpeername, recvfrom, accept).
                //
                // To access the path, std.mem.sliceTo(&address.un.path, 0) should be used.
                size_of::<sockaddr::Un>() as socklen_t
            }
            _ => unreachable!(),
        }
    }
}
