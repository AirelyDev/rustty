//! PORTS: the part of Zig 0.16 `std.valgrind` the port uses:
//! `runningOnValgrind()` and the `doClientRequest` it is built on
//! (`lib/std/valgrind.zig`), for `renderer/Thread.zig`'s `cursorBlinkInterval`
//! (parity S114-F1).
//!
//! A Valgrind client request is a "magic" instruction sequence that is a no-op
//! on real hardware (the rotations add up to a full turn and the final
//! instruction moves a register onto itself), so natively the request returns
//! `default`; under Valgrind the JIT recognises the sequence, reads the
//! request block that the second register points at, and writes the answer
//! into the result register.
//
// PORT NOTE: only the two architectures the port builds for (x86_64 and
// aarch64) carry the sequence; the reference's other arms (arm, mips, powerpc,
// riscv64, s390x, x86) return `default` here, as its own `else` arm does for
// any architecture it lists none for. The sequences and register constraints
// are copied from the reference's `.x86_64` and `.aarch64` arms.

/// `builtin.valgrind_support`: whether client requests are compiled in.
///
/// MEASURED (zig 0.16.0, `zig build-obj -target <t> -O <mode>` on a
/// `@compileError` of the value): true in `Debug` for x86_64-linux,
/// aarch64-linux, x86_64/aarch64-freebsd, x86_64-windows, arm-linux and
/// riscv64-linux; false in `ReleaseSafe` and `ReleaseFast`; false on
/// aarch64-macos and x86_64-macos in every mode.
//
// Zig `Debug` is the Cargo opt-level 0 (the mode mapping in
// `rustty-build-config`'s `mode_string_for`), passed in by `build.rs`.
// CORRECTION (review T-I F6): this was `cfg!(debug_assertions) && !macos`,
// which made it true under the `release-safe` profile (DG4:
// `debug-assertions = true` at opt-level 3), where Zig ReleaseSafe's is false.
pub const VALGRIND_SUPPORT: bool =
    valgrind_support_for(env!("RUSTTY_ZIGSTD_OPT_LEVEL"), cfg!(target_os = "macos"));

/// The `valgrind_support` decision over its inputs: Debug (opt-level 0) and
/// not macOS.
const fn valgrind_support_for(opt_level: &str, macos: bool) -> bool {
    let opt_level = opt_level.as_bytes();
    opt_level.len() == 1 && opt_level[0] == b'0' && !macos
}

/// `Valgrind.ClientRequest.RunningOnValgrind`.
const RUNNING_ON_VALGRIND: usize = 4097;

// pub fn doClientRequest(default: usize, request: usize, a1: usize, a2: usize,
//     a3: usize, a4: usize, a5: usize) usize {
//     if (!builtin.valgrind_support) {
//         return default;
//     }
//     ...
// }
pub fn do_client_request(
    default: usize,
    request: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> usize {
    if !VALGRIND_SUPPORT {
        return default;
    }
    raw_client_request(default, request, a1, a2, a3, a4, a5)
}

/// The instruction sequence itself, without the `valgrind_support` gate.
fn raw_client_request(
    default: usize,
    request: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> usize {
    // const args = &[_]usize{ request, a1, a2, a3, a4, a5 };
    let args: [usize; 6] = [request, a1, a2, a3, a4, a5];
    #[cfg(all(target_arch = "x86_64", not(miri)))]
    {
        // .x86_64 => asm volatile (
        //     \\ rolq  $3,    %%rdi ; rolq $13, %%rdi
        //     \\ rolq  $61,   %%rdi ; rolq $51, %%rdi
        //     \\ xchgq %%rbx, %%rbx
        //     : [_] "={rdx}" (-> usize),
        //     : [_] "{rax}" (args),
        //       [_] "{rdx}" (default),
        //     : .{ .memory = true }),
        let mut result = default;
        // SAFETY: natively a no-op (rdi is rotated a full 128 bits and rbx is
        // exchanged with itself); under Valgrind it reads the six words at
        // `args`, which live until the asm returns, and writes rdx. No stack
        // use; memory is not marked `nomem`/`readonly`, the `.memory` clobber.
        unsafe {
            core::arch::asm!(
                "rol rdi, 3",
                "rol rdi, 13",
                "rol rdi, 61",
                "rol rdi, 51",
                "xchg rbx, rbx",
                inout("rdx") result,
                in("rax") args.as_ptr(),
                options(nostack),
            );
        }
        result
    }
    #[cfg(all(target_arch = "aarch64", not(miri)))]
    {
        // .aarch64, .aarch64_be => asm volatile (
        //     \\ ror x12, x12, #3  ; ror x12, x12, #13
        //     \\ ror x12, x12, #51 ; ror x12, x12, #61
        //     \\ orr x10, x10, x10
        //     : [_] "={x3}" (-> usize),
        //     : [_] "{x4}" (args),
        //       [_] "{x3}" (default),
        //     : .{ .memory = true }),
        let mut result = default;
        // SAFETY: natively a no-op (x12 is rotated a full 128 bits and x10 is
        // or-ed with itself); under Valgrind it reads the six words at `args`,
        // which live until the asm returns, and writes x3. No stack use; memory
        // is not marked `nomem`/`readonly`, the `.memory` clobber.
        unsafe {
            core::arch::asm!(
                "ror x12, x12, #3",
                "ror x12, x12, #13",
                "ror x12, x12, #51",
                "ror x12, x12, #61",
                "orr x10, x10, x10",
                inout("x3") result,
                in("x4") args.as_ptr(),
                options(nostack),
            );
        }
        result
    }
    // else => default,
    // (Miri cannot run inline assembly; it takes this arm too.)
    #[cfg(any(not(any(target_arch = "x86_64", target_arch = "aarch64")), miri))]
    {
        let _ = args;
        default
    }
}

/// Returns the number of Valgrinds this code is running under.  That
/// is, 0 if running natively, 1 if running under Valgrind, 2 if
/// running under Valgrind which is running under another Valgrind,
/// etc.
// pub fn runningOnValgrind() usize {
//     return doClientRequestExpr(0, .RunningOnValgrind, 0, 0, 0, 0, 0);
// }
pub fn running_on_valgrind() -> usize {
    do_client_request(0, RUNNING_ON_VALGRIND, 0, 0, 0, 0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review T-I F6: `valgrind_support` is true only for Debug (opt-level 0)
    /// off macOS, matching the measured zig 0.16.0 value for each of Debug,
    /// ReleaseSafe (opt-level 1-3 with `debug-assertions`, the `release-safe`
    /// profile), ReleaseFast and ReleaseSmall ("s"/"z").
    #[test]
    fn valgrind_support_is_debug_only_and_never_on_macos() {
        assert!(valgrind_support_for("0", false));
        for opt in ["1", "2", "3", "s", "z"] {
            assert!(!valgrind_support_for(opt, false), "opt-level {opt}");
        }
        for opt in ["0", "1", "2", "3", "s", "z"] {
            assert!(!valgrind_support_for(opt, true), "macOS opt-level {opt}");
        }
        // This build's constant is the same decision over its own inputs.
        assert_eq!(
            valgrind_support_for(env!("RUSTTY_ZIGSTD_OPT_LEVEL"), cfg!(target_os = "macos")),
            VALGRIND_SUPPORT
        );
    }

    /// Natively the magic sequence is a no-op: the request returns its
    /// default. This executes the x86_64/aarch64 sequence on the host even
    /// where `VALGRIND_SUPPORT` is false (macOS), so the asm is exercised.
    #[test]
    #[cfg_attr(miri, ignore = "inline assembly does not run under Miri (R14)")]
    fn client_request_returns_default_natively() {
        assert_eq!(7, raw_client_request(7, RUNNING_ON_VALGRIND, 1, 2, 3, 4, 5));
        assert_eq!(0, raw_client_request(0, RUNNING_ON_VALGRIND, 0, 0, 0, 0, 0));
        // The test binary does not run under Valgrind.
        assert_eq!(0, running_on_valgrind());
        assert_eq!(
            42,
            do_client_request(42, RUNNING_ON_VALGRIND, 0, 0, 0, 0, 0)
        );
    }
}
