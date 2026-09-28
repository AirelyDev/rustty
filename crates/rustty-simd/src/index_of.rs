//! index_of — PORTS ghostty src/simd/index_of.zig.

/// `options.simd` — whether SIMD-accelerated code paths are built. Mirrors the
/// reference's `build_options.simd`, which defaults to `true`.
// WASM DIVERGENCE: on `wasm32` the SIMD branch is gated OFF (no C++ kernel on
// freestanding wasm); the scalar `index_of_scalar` lane below is always taken.
#[cfg_attr(any(target_arch = "wasm32", not(feature = "simd")), allow(dead_code))]
const SIMD: bool = crate::SIMD;

// index_of.cpp — gated OFF on `wasm32` (no C++ kernel on freestanding wasm).
#[cfg(all(feature = "simd", not(target_arch = "wasm32")))]
extern "C" {
    fn rustty_simd_index_of(needle: u8, input: *const u8, count: usize) -> usize;
}

// PORT NOTE (Miri): see `base64.rs` / `vt.rs` — Miri cannot execute the C/C++
// kernel, so it takes the pure-Rust `index_of_scalar` lane, which is identical
// output by construction (`std.mem.indexOfScalar`). `cfg!(miri)` is const-false
// in every real build; the SIMD lane stays covered by the ASan G6 route.
#[allow(non_snake_case)]
pub fn indexOf(input: &[u8], needle: u8) -> Option<usize> {
    #[cfg(all(feature = "simd", not(target_arch = "wasm32")))]
    if SIMD && !cfg!(miri) {
        let result = unsafe { rustty_simd_index_of(needle, input.as_ptr(), input.len()) };
        return if result == input.len() {
            None
        } else {
            Some(result)
        };
    }

    index_of_scalar(input, needle)
}

fn index_of_scalar(input: &[u8], needle: u8) -> Option<usize> {
    // std.mem.indexOfScalar(u8, input, needle)
    input.iter().position(|&b| b == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // PARITY: `== None` mirrors the reference's `== null` comparison.
    #[allow(clippy::partialeq_to_none)]
    fn index_of() {
        assert!(indexOf(b"hello", b' ') == None);
        assert_eq!(2_usize, indexOf(b"hi lo", b' ').unwrap());
        assert_eq!(
            5_usize,
            indexOf(
                concat!(
                    "XXXXX XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
                    "\n",
                    "XXXXXXXXXXXX XXXXXXXXXXX XXXXXXXXXXXXXXX"
                )
                .as_bytes(),
                b' '
            )
            .unwrap()
        );
        assert_eq!(
            53_usize,
            indexOf(
                concat!(
                    "XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
                    "\n",
                    "XXXXXXXXXXXX XXXXXXXXXXX XXXXXXXXXXXXXXX"
                )
                .as_bytes(),
                b' '
            )
            .unwrap()
        );
    }
}
