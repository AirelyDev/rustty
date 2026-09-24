//! Kitty clipboard protocol (OSC 5522) session password grants:
//! requests carrying a granted password skip the permission prompt, and
//! paste events mint one-time passwords.
//!
//! PORTS: ghostty src/terminal/kitty/clipboard_grants.zig (B5/B7 end state:
//! `6f007e767` grants, `a5bb22e23` generateOtp, `dda8e6f31` sys
//! random_secure override)

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const Allocator = std.mem.Allocator;
//   const clipboard_command = @import("clipboard_command.zig");
//   const sys = @import("../sys.zig");
// `Allocator` is the runtime allocator handle (parity tail S7-4): the entries
// list and every owned password are allocated through the allocator the
// caller passes (the terminal's `gpa()`), fallibly.
#![allow(non_upper_case_globals)]

use crate::alloc::{self as valloc, AVec, Allocator, Bytes, OutOfMemory, TryPushExt};
use crate::kitty::clipboard_command;
use crate::sys;

// const max_pw_len = clipboard_command.max_pw_len;
use clipboard_command::max_pw_len;

/// The direction a grant covers.
// PORT NOTE: the reference nests this as `Grants.Direction`; Rust enums cannot
// be declared inside a struct, so it is flattened to module scope.
// pub const Direction = enum { read, write };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Read,
    Write,
}

/// Session password grants, used to skip permission prompts for
/// requests carrying a known pw. Callers can choose to scope these
/// however they want, e.g. Kitty does it per window and the spec
/// doesn't demand anything.
// PORT NOTE (parity tail S7-4): `.empty` holds no allocator; the list binds to
// the allocator of the first growing call (`alloc::bind`) and every later
// `alloc` argument is debug-asserted to be that one, the reference's contract
// for an unmanaged list.
pub struct Grants {
    // entries: std.ArrayListUnmanaged(Entry) = .empty,
    entries: AVec<Entry>,
}

impl Default for Grants {
    // .{} (`entries = .empty`): allocates nothing.
    fn default() -> Grants {
        Grants {
            entries: AVec::new_in(valloc::Global),
        }
    }
}

// const Entry = struct { ... };
struct Entry {
    /// Owned by the allocator passed to grant.
    pw: Bytes,
    read: bool,
    write: bool,
    one_time: bool,
}

impl Grants {
    // const max_entries = 32;
    pub(crate) const max_entries: usize = 32;

    // pub fn deinit(self: *Grants, alloc: Allocator) void {
    //     for (self.entries.items) |entry| alloc.free(entry.pw);
    //     self.entries.deinit(alloc);
    // }
    // PORT NOTE: each `pw` and the list free through the allocator they were
    // allocated with when dropped; `deinit` leaves the list `.empty`.
    pub fn deinit(&mut self, alloc: Allocator) {
        if self.entries.capacity() != 0 {
            self.entries.allocator().debug_assert_same(alloc);
        }
        self.entries = AVec::new_in(valloc::Global);
    }

    /// Record a grant for pw. An existing grant for the same password
    /// gains the new direction.
    // pub fn grant(self: *Grants, alloc: Allocator, pw: []const u8, dir: Direction,
    //     one_time: bool) Allocator.Error!void {
    pub fn grant(
        &mut self,
        alloc: Allocator,
        pw: &[u8],
        dir: Direction,
        one_time: bool,
    ) -> Result<(), OutOfMemory> {
        if pw.is_empty() || pw.len() > max_pw_len {
            return Ok(());
        }

        // const entry: *Entry = entry: { ... };
        let idx: usize = match self.find_index(pw) {
            Some(idx) => {
                let entry = &mut self.entries[idx];
                entry.one_time = entry.one_time && one_time;
                idx
            }
            None => {
                valloc::bind(&mut self.entries, alloc);

                // Evict the oldest grant once full.
                if self.entries.len() >= Grants::max_entries {
                    // const oldest = self.entries.orderedRemove(0);
                    // alloc.free(oldest.pw);
                    self.entries.remove(0);
                }

                // const owned = try alloc.dupe(u8, pw);
                // errdefer alloc.free(owned);
                let owned = valloc::try_dupe(alloc, pw)?;
                // try self.entries.append(alloc, .{ .pw = owned, .one_time = one_time });
                self.entries.try_push(Entry {
                    pw: owned,
                    read: false,
                    write: false,
                    one_time,
                })?;
                self.entries.len() - 1
            }
        };

        let entry = &mut self.entries[idx];
        match dir {
            Direction::Read => entry.read = true,
            Direction::Write => entry.write = true,
        }
        Ok(())
    }

    /// Check whether pw grants the given direction. A one-time grant is
    /// consumed by this check even when the direction doesn't match,
    /// matching kitty's pop-on-check behavior.
    // pub fn use(self: *Grants, alloc: Allocator, pw: []const u8, dir: Direction) bool {
    // PORT NOTE: `use` is a Rust keyword; the trailing underscore follows the
    // crate's keyword-collision precedent (opengl `Program::use_`).
    pub fn use_(&mut self, alloc: Allocator, pw: &[u8], dir: Direction) -> bool {
        if pw.is_empty() {
            return false;
        }
        let idx = match self.find_index(pw) {
            Some(idx) => idx,
            None => return false,
        };
        let entry = &self.entries[idx];
        let allowed = match dir {
            Direction::Read => entry.read,
            Direction::Write => entry.write,
        };
        if entry.one_time {
            // const removed = self.entries.swapRemove(idx);
            // alloc.free(removed.pw);
            self.entries.allocator().debug_assert_same(alloc);
            self.entries.swap_remove(idx);
        }
        allowed
    }

    fn find_index(&self, pw: &[u8]) -> Option<usize> {
        // for (self.entries.items, 0..) |*entry, idx| { ... }
        self.entries
            .iter()
            .position(|entry| entry.pw.as_slice() == pw)
    }

    /// The number of recorded grants.
    // PORT NOTE (rustty-only accessor): the reference tests and consumers read
    // `grants.entries.items.len` directly (Zig has no field privacy); the
    // ported `entries` Vec is private, so this is the sanctioned equivalent.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no grants are recorded (`entries.items.len == 0`).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The length of a one-time password generated for paste events.
pub const otp_len: usize = 22;

/// The one-time password alphabet. This matches kitty (alphanumeric
/// without easily-confused characters), but the spec doesn't demand
/// this.
pub const otp_alphabet: &[u8] = b"23456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ";

/// Generate a one-time password for a paste event.
///
/// The password is a secret: a program that learns it can read the
/// clipboard without a prompt. Entropy comes from `sys.random_secure`
/// if set, otherwise from the Io; see `sys.randomSecure`.
// pub fn generateOtp(io: std.Io) std.Io.RandomSecureError![otp_len]u8 {
// PORT NOTE: `io: std.Io` is the sys.rs `Io` stand-in (the one capability
// these callers consume — `randomSecure`; see the NON_PORTS TinyIo re-read
// note there), and `std.Io.RandomSecureError` collapses into
// `sys::RandomSecureError`.
pub fn generate_otp(io: sys::Io) -> Result<[u8; otp_len], sys::RandomSecureError> {
    let mut result = [0u8; otp_len];
    let mut len: usize = 0;
    while len < result.len() {
        // var raw: [2 * otp_len]u8 = undefined;
        let mut raw = [0u8; 2 * otp_len];
        // try sys.randomSecure(io, &raw);
        sys::random_secure_fill(io, &mut raw)?;
        // const limit = (std.math.maxInt(u8) + 1) / otp_alphabet.len * otp_alphabet.len;
        let limit = (u8::MAX as usize + 1) / otp_alphabet.len() * otp_alphabet.len();
        for &byte in raw.iter() {
            if byte as usize >= limit {
                continue;
            }
            result[len] = otp_alphabet[byte as usize % otp_alphabet.len()];
            len += 1;
            if len == result.len() {
                break;
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "grants: basic grant and use"
    #[test]
    fn grants_basic_grant_and_use() {
        let alloc = crate::alloc::Global;
        let mut grants = Grants::default();
        assert!(!grants.use_(alloc, b"pw1", Direction::Read));

        grants.grant(alloc, b"pw1", Direction::Read, false).unwrap();
        assert!(grants.use_(alloc, b"pw1", Direction::Read));
        // Persistent grants survive use.
        assert!(grants.use_(alloc, b"pw1", Direction::Read));
        assert!(!grants.use_(alloc, b"pw1", Direction::Write));
    }

    // test "grants: one-time consumed on check"
    #[test]
    fn grants_one_time_consumed_on_check() {
        let alloc = crate::alloc::Global;
        let mut grants = Grants::default();
        grants.grant(alloc, b"otp", Direction::Read, true).unwrap();
        assert!(grants.use_(alloc, b"otp", Direction::Read));
        assert!(!grants.use_(alloc, b"otp", Direction::Read));
    }

    // test "grants: one-time consumed even on direction mismatch"
    #[test]
    fn grants_one_time_consumed_even_on_direction_mismatch() {
        let alloc = crate::alloc::Global;
        let mut grants = Grants::default();
        grants.grant(alloc, b"otp", Direction::Read, true).unwrap();
        assert!(!grants.use_(alloc, b"otp", Direction::Write));
        assert!(!grants.use_(alloc, b"otp", Direction::Read));
    }

    // test "grants: directions are independent"
    #[test]
    fn grants_directions_are_independent() {
        let alloc = crate::alloc::Global;
        let mut grants = Grants::default();
        grants.grant(alloc, b"pw", Direction::Read, false).unwrap();
        grants.grant(alloc, b"pw", Direction::Write, false).unwrap();
        assert!(grants.use_(alloc, b"pw", Direction::Read));
        assert!(grants.use_(alloc, b"pw", Direction::Write));
    }

    // test "grants: capacity evicts the oldest"
    #[test]
    fn grants_capacity_evicts_the_oldest() {
        let alloc = crate::alloc::Global;
        let mut grants = Grants::default();

        // var buf: [8]u8 = undefined;
        for i in 0..Grants::max_entries + 1 {
            // const pw = try std.fmt.bufPrint(&buf, "pw{}", .{i});
            let pw = format!("pw{}", i);
            grants
                .grant(alloc, pw.as_bytes(), Direction::Read, false)
                .unwrap();
        }

        // The oldest grant was evicted; the newest survives.
        assert!(!grants.use_(alloc, b"pw0", Direction::Read));
        let newest = format!("pw{}", Grants::max_entries);
        assert!(grants.use_(alloc, newest.as_bytes(), Direction::Read));
    }

    // test "generateOtp: length and alphabet with a real Io"
    // PORT NOTE: the reference's `testing.io` (a real Io with entropy) maps to
    // `sys::io_native`, which calls the OS CSPRNG — a foreign call, hence the
    // Miri ignore (crate precedent: stream.rs FFI-backed tests).
    #[test]
    #[cfg_attr(miri, ignore)]
    fn generate_otp_length_and_alphabet_with_a_real_io() {
        // rustty-only: the swappable `random_secure` slot is process-global and
        // `cargo test` is parallel; hold the slot lock (see sys.rs).
        let _lock = sys::random_secure_test_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let otp = generate_otp(sys::io_native).unwrap();
        assert_eq!(otp_len, otp.len());
        for c in otp {
            assert!(otp_alphabet.contains(&c));
        }

        // Two passwords don't collide (a repeat would mean no entropy).
        let other = generate_otp(sys::io_native).unwrap();
        assert!(otp != other);
    }

    // test "generateOtp: no entropy is an error, never a weak password"
    #[test]
    fn generate_otp_no_entropy_is_an_error_never_a_weak_password() {
        let _lock = sys::random_secure_test_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        // try testing.expectError(error.EntropyUnavailable, generateOtp(std.Io.failing));
        assert_eq!(
            Err(sys::RandomSecureError::EntropyUnavailable),
            generate_otp(sys::io_failing),
        );
    }

    // test "generateOtp: sys override supplies entropy without an Io source"
    #[test]
    fn generate_otp_sys_override_supplies_entropy_without_an_io_source() {
        let _lock = sys::random_secure_test_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        // const S = struct {
        //     var counter: u8 = 0;
        //     fn fill(buffer: []u8) sys.RandomSecureError!void { ... }
        // };
        // PORT NOTE: the reference's container-scoped `var counter` becomes a
        // static (the fill slot is a plain fn pointer, so the state cannot be
        // captured); the slot lock above serializes access.
        static COUNTER: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
        fn fill(buffer: &mut [u8]) -> Result<(), sys::RandomSecureError> {
            for b in buffer.iter_mut() {
                *b = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            Ok(())
        }
        // sys.random_secure = &S.fill;
        // defer sys.random_secure = null;
        *sys::random_secure.write().unwrap() = Some(fill);

        let otp = generate_otp(sys::io_failing).unwrap();
        for c in otp {
            assert!(otp_alphabet.contains(&c));
        }

        // (the reference's `defer sys.random_secure = null`)
        *sys::random_secure.write().unwrap() = None;
    }
}
