//! An allocation-free implementation of the raw LZ4 block format.
//!
//! LZ4 has two relevant layers: the block format describes the compressed
//! bytes, while the frame format adds headers, sizes, checksums, and support
//! for a stream of blocks. Terminal pages already have their own ownership
//! and metadata, so this implements only blocks. In particular, an encoded
//! block does not contain its decompressed size. The caller must store that
//! separately and provide an exactly sized buffer when decoding.
//!
//! A block is a series of sequences. Each non-final sequence has this shape:
//!
//!     token | literal length extensions | literals | offset | match length extensions
//!
//! The token's high nibble contains the literal length and its low nibble
//! contains the match length minus four. A nibble value of 15 means that the
//! length continues in extension bytes at the corresponding point in the
//! sequence. Each extension byte adds to the length; a value of 255 means
//! another byte follows. The literal bytes are copied directly. The two-byte
//! little-endian offset then points backwards in the already decompressed
//! output to the match bytes.
//!
//! The last sequence is special: it contains literals only and ends directly
//! after them. The reference format also requires the last five input bytes to
//! be literals and the final match to begin at least twelve bytes before the
//! end. The compressor observes these restrictions so its output can be
//! consumed by optimized LZ4 decoders which copy in larger units.
//!
//! Compression uses the fast LZ4 strategy: hash each four-byte input sequence
//! and test recent positions as match candidates. Two 16-bit positions fit in
//! each hash-table entry because the format cannot refer further than 64 KiB
//! backwards. Retaining the displaced position recovers useful matches after
//! hash collisions without increasing the fixed 16 KiB workspace. Long runs
//! without matches gradually skip input positions, while matching runs are
//! extended a machine word at a time. The implementation allocates nothing;
//! all input, output, and scratch memory is supplied by the caller.
//!
//! Format reference:
//! https://github.com/lz4/lz4/blob/dev/doc/lz4_Block_format.md
//!
//! PORTS: ghostty src/terminal/compress/lz4.zig (oracle origin/main, final
//! form after b9bb50c83 + 9a4bd2120).
//!
//! PORT NOTE (WS7): this module contains NO `unsafe`. The oracle's blind wide
//! copies are expressed as safe slice reads/writes whose margin conditions are
//! restated in comments exactly as the oracle requires them. See the WS7 plan's
//! Global Constraints for the (measured, not assumed) escalation rule.

/// Maximum input accepted by the reference LZ4 block API. Keeping the same
/// limit means `compress_bound` fits in the integer sizes used by LZ4 callers
/// and gives us the same compatibility boundary as other implementations.
pub const MAX_INPUT_SIZE: usize = 0x7E00_0000;

/// Every LZ4 match represents at least four bytes. The token stores the number
/// of bytes beyond this minimum rather than the full match length.
const MIN_MATCH: usize = 4;

/// Number of bytes at the end of a conforming block which must remain literals.
const LAST_LITERALS: usize = 5;

/// A match may not begin in the final 12 bytes. This leaves enough room for the
/// minimum match and the required five trailing literals.
const MATCH_FIND_LIMIT: usize = 12;

/// We retain one input position for each 12-bit hash. LZ4 refers to this as
/// memory usage 14 because the 4096 entries are four bytes each (16 KiB).
const HASH_LOG: usize = 12;

/// Multiplicative hash used by the reference LZ4 fast compressor. The high
/// `HASH_LOG` bits provide the table index.
const HASH_MULTIPLIER: u32 = 2_654_435_761;

/// Scratch memory used while compressing one block. Each entry packs the low
/// 16 bits of the two most recent input positions for its hash. All-ones marks
/// an empty half; LZ4's 16-bit offset is enough to reconstruct the only useful
/// preceding address from the current position.
/// The table is reset by every call to `compress` and can be reused afterwards.
pub type HashTable = [u32; 1 << HASH_LOG];

/// Errors which can occur while encoding a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressError {
    /// The input exceeds the maximum size supported by the block compressor.
    InputTooLarge,

    /// The provided output buffer cannot hold the encoded block.
    OutputTooSmall,
}

/// Errors which can occur while decoding a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecompressError {
    /// The encoded block ended in the middle of a sequence.
    TruncatedInput,

    /// A match offset was zero or pointed before the produced output.
    InvalidOffset,

    /// A sequence would write beyond the provided output buffer.
    OutputTooSmall,

    /// The block ended before filling the exact-size output buffer.
    OutputSizeMismatch,
}

/// Return the maximum number of bytes needed to encode `input_len` bytes.
///
/// Incompressible input is represented as one literal run. Every 255 literal
/// bytes can require one extension byte. The additional 16-byte margin covers
/// the token and the format's fixed overhead. Callers can allocate this amount
/// once and reuse it for any block no larger than `input_len`.
pub fn compress_bound(input_len: usize) -> Result<usize, CompressError> {
    if input_len > MAX_INPUT_SIZE {
        return Err(CompressError::InputTooLarge);
    }
    Ok(input_len + input_len / 255 + 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PORTS: oracle@main lz4.zig `test "compressBound"`
    #[test]
    fn lz4_compress_bound() {
        assert_eq!(compress_bound(0), Ok(16));
        assert_eq!(compress_bound(255), Ok(272));
        assert_eq!(
            compress_bound(MAX_INPUT_SIZE + 1),
            Err(CompressError::InputTooLarge)
        );
    }
}
