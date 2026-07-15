//! base64 — PORTS ghostty src/simd/base64.zig.

use crate::base64_scalar::scalar_decoder;
use rustty_util::quirks::inlineAssert as assert;

// const log = std.log.scoped(.simd_base64);

/// `options.simd` — whether SIMD-accelerated code paths are built. Mirrors the
/// reference's `build_options.simd`, which defaults to `true`.
const SIMD: bool = true;

// PORT NOTE (Miri): Miri cannot execute the C/C++ side of the FFI ("can't
// call foreign function"), so under Miri the two entry points below take the
// scalar lane instead — the exact code the reference builds when
// `build_options.simd` is off, with identical output by construction (see
// `scalar_input`). `cfg!(miri)` is const-false in every real build, so the
// SIMD lane is untouched everywhere else (in-repo precedent: the `cfg(miri)`
// std-heap allocator fallback in rustty-vt pagelist/mod.rs). The SIMD lane
// itself stays covered by this crate's ASan G6 route and `cargo test`.
pub fn max_len(input: &[u8]) -> usize {
    if SIMD && !cfg!(miri) {
        return unsafe { rustty_simd_base64_max_length(input.as_ptr(), input.len()) };
    }

    max_len_scalar(input)
}

fn max_len_scalar(input: &[u8]) -> usize {
    // PARITY: the reference is `catch |err| { log.warn(...); return 0; }`; kept
    // as an explicit match so the (commented) error-logging branch is preserved.
    #[allow(clippy::manual_unwrap_or, clippy::manual_unwrap_or_default)]
    match scalar_decoder.calc_size_for_slice(scalar_input(input)) {
        Ok(v) => v,
        Err(_err) => {
            // log.warn("failed to calculate base64 size for payload: {}", .{err});
            0
        }
    }
}

/// Mirrors Zig's `error{Base64Invalid}`.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Base64Invalid;

pub fn decode<'a>(input: &[u8], output: &'a mut [u8]) -> Result<&'a [u8], Base64Invalid> {
    // See the Miri PORT NOTE on `max_len`.
    if SIMD && !cfg!(miri) {
        let res =
            unsafe { rustty_simd_base64_decode(input.as_ptr(), input.len(), output.as_mut_ptr()) };
        if res < 0 {
            return Err(Base64Invalid);
        }
        return Ok(&output[0..res as usize]);
    }

    decode_scalar(input, output)
}

fn decode_scalar<'a>(input_raw: &[u8], output: &'a mut [u8]) -> Result<&'a [u8], Base64Invalid> {
    let input = scalar_input(input_raw);
    let size = max_len_scalar(input);
    if size == 0 {
        return Ok(b"");
    }
    assert(output.len() >= size);
    scalar_decoder
        .decode(output, scalar_input(input))
        .map_err(|_| Base64Invalid)?;
    Ok(&output[0..size])
}

/// For non-SIMD enabled builds, we trim the padding from the end of the
/// base64 input in order to get identical output with the SIMD version.
fn scalar_input(input: &[u8]) -> &[u8] {
    let mut i: usize = 0;
    while input[input.len() - i - 1] == b'=' {
        i += 1;
    }
    &input[0..input.len() - i]
}

// base64.cpp
extern "C" {
    fn rustty_simd_base64_max_length(input: *const u8, len: usize) -> usize;
    fn rustty_simd_base64_decode(input: *const u8, len: usize, output: *mut u8) -> isize;
}

#[cfg(test)]
mod tests {
    // `base64_maxLen` carries the reference test name `"base64 maxLen"`
    // (camelCase `maxLen` preserved); spaces are not valid in a Rust fn name.
    #![allow(non_snake_case)]
    use super::*;

    #[test]
    fn base64_maxLen() {
        let len = max_len(b"aGVsbG8gd29ybGQ=");
        assert_eq!(11, len);
    }

    #[test]
    fn base64_decode() {
        let input = b"aGVsbG8gd29ybGQ=";
        let len = max_len(input);
        let mut output = vec![0u8; len];
        let str = decode(input, &mut output).unwrap();
        assert_eq!(b"hello world".as_slice(), str);
    }
}
