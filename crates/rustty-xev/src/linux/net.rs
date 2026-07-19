//! linux/net.rs — the `std.net.Address` subset libxev's epoll `Operation` uses
//! (XEV_BLUEPRINT §1 Linux port, task L1 LT4).
//!
//! `backend/epoll.zig` stores `addr: std.net.Address` in its `connect` operation
//! and the loop's `connect` `start` arm calls `posix.connect(v.socket,
//! &v.addr.any, v.addr.getOsSockLen())`; the "socket accept/connect/send/recv/
//! close" test builds one with `net.Address.parseIp4("127.0.0.1", 3131)`.
//! `std.net.Address` is Zig std (not a libxev source file), so — faithful to the
//! port's "reproduce what libxev's Linux build compiles" rule — only the EXACT
//! subset the epoll backend and its test reach is ported here: the `extern union
//! Address` (any/in/in6/un), `Ip4Address`/`Ip6Address`, `parseIp4`/`initIp4`,
//! `getOsSockLen`, and the `posix.sockaddr.{in,in6,un}` extern structs they wrap.
//! Ported byte-faithfully from Zig `std/net.zig` and the LINUX
//! `std.os.linux.sockaddr` arm (NOT the macOS `std.c` arm the crate's macOS-gated
//! `net.rs` twins).
//
// PORT NOTE (R5): the crate's `net.rs` is macOS-gated (it twins the darwin
// `sockaddr` arm: an 8-bit `sa_family_t` and a leading `len` byte). Linux's ABI
// differs — `sa_family_t` is `u16` with NO `len` byte and `sun_path` is 108 bytes
// — so this is a SEPARATE Linux twin of the same `std.net.Address` subset, not a
// shared import. Zig field names are preserved; the `in` keyword → `in_`.
//
// PORT NOTE: `posix.socklen_t`/`sa_family_t`/`in_port_t` are the platform C ints.
// On Linux `sa_family_t` is `u16` (`std.os.linux.sa_family_t`); `posix.AF.{INET,
// INET6,UNIX}` are the address-family selectors, bound from `libc`.

#![allow(non_camel_case_types)]

use core::mem::size_of;

/// PORT NOTE: `posix.socklen_t` (Linux `u32`).
pub type socklen_t = libc::socklen_t;
/// PORT NOTE: `posix.sa_family_t` (Linux `u16`).
pub type sa_family_t = u16;
/// PORT NOTE: `posix.in_port_t` (network-order port, `u16`).
pub type in_port_t = u16;

/// PORT NOTE: `posix.AF.INET`.
pub const AF_INET: sa_family_t = libc::AF_INET as sa_family_t;
/// PORT NOTE: `posix.AF.INET6`.
pub const AF_INET6: sa_family_t = libc::AF_INET6 as sa_family_t;
/// PORT NOTE: `posix.AF.UNIX`.
pub const AF_UNIX: sa_family_t = libc::AF_UNIX as sa_family_t;

/// PORT NOTE: the `std.os.linux` `posix.sockaddr` family — the extern structs the
/// `Address` union overlays. Field names + defaults mirror the Zig `extern
/// struct`s verbatim (`family = AF.*`); Rust has no per-field defaults, so
/// `Ip4Address::{parse,init}` set `family`/`zero` explicitly at construction
/// (exactly the values the Zig defaults supply). Unlike the darwin arm, there is
/// NO leading `len` byte and `sa_family_t` is `u16`.
pub mod sockaddr {
    use super::{in_port_t, sa_family_t};

    /// PORT NOTE: `posix.sockaddr` (the generic `any`): `{ family, data: [14]u8 }`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Any {
        pub family: sa_family_t,
        pub data: [u8; 14],
    }

    /// PORT NOTE: `posix.sockaddr.in`: `{ family = AF.INET, port, addr,
    /// zero: [8]u8 = {0}*8 }`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct In {
        pub family: sa_family_t,
        pub port: in_port_t,
        pub addr: u32,
        pub zero: [u8; 8],
    }

    /// PORT NOTE: `posix.sockaddr.in6`: `{ family = AF.INET6, port, flowinfo,
    /// addr: [16]u8, scope_id }`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct In6 {
        pub family: sa_family_t,
        pub port: in_port_t,
        pub flowinfo: u32,
        pub addr: [u8; 16],
        pub scope_id: u32,
    }

    /// PORT NOTE: `posix.sockaddr.un` (UNIX domain socket): `{ family = AF.UNIX,
    /// path: [108]u8 }` (Linux `sun_path` is 108 bytes).
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Un {
        pub family: sa_family_t,
        pub path: [u8; 108],
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
                // family/zero take the Zig `sockaddr.in` field defaults.
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
/// the struct + `getOsSockLen` are reached by the epoll backend (the INET6 arm of
/// `Address.getOsSockLen`; the tests are IPv4-only). `Ip6Address.parse` is unused
/// by the epoll backend and is not ported.
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
/// access is `unsafe` in Rust (exactly Zig's active-field discipline); the epoll
/// backend reads `.any.family` (dispatch) + `&.any` (the connect syscall) and
/// writes via `.in`. `#[derive(Copy)]`: all members are POD `extern struct`s (Zig
/// copies `Address` by value into the `Operation`).
//
// PORT NOTE: Zig field `in` is a Rust keyword → renamed `in_` (the only rename).
// `un` is `posix.sockaddr.un` on Linux (`has_unix_sockets`).
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

    /// PORT NOTE: `Address.initPosix(addr: *align(4) const sockaddr) Address` —
    /// `return .{ .any = addr.* };`. Reinterprets a filled `sockaddr` (e.g. the
    /// `msg_name` a `recvmsg` wrote) as an `Address` by copying its `any` overlay.
    ///
    /// # Safety
    /// `addr` must point at a readable `sockaddr` (at least `sockaddr::Any` bytes).
    pub unsafe fn init_posix(addr: *const sockaddr::Any) -> Address {
        Address {
            any: unsafe { *addr },
        }
    }

    /// PORT NOTE: `Address.getOsSockLen(self)`.
    //
    // PORT NOTE: the `else => unreachable` (non-IP/UNIX family) ports as a Rust
    // `unreachable!()`; `has_unix_sockets` is always true on Linux so the
    // `if (!has_unix_sockets) unreachable` guard collapses to the live UNIX arm.
    pub fn get_os_sock_len(self) -> socklen_t {
        // SAFETY: reading the `any.family` byte pair is valid for any active union
        // member (all overlay `family` as their first two bytes).
        match unsafe { self.any.family } {
            AF_INET => unsafe { self.in_.get_os_sock_len() },
            AF_INET6 => unsafe { self.in6.get_os_sock_len() },
            AF_UNIX => size_of::<sockaddr::Un>() as socklen_t,
            _ => unreachable!(),
        }
    }
}
