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
//! PORT NOTE (WS7): fenced as `text` so rustdoc renders the oracle's format
//! diagram verbatim instead of compiling an indented block as a doctest.
//!
//! ```text
//! token | literal length extensions | literals | offset | match length extensions
//! ```
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
pub type HashTable = [u32; HASH_TABLE_LEN];

/// The element count of a [`HashTable`].
///
/// PORT NOTE (WS7 B3): the oracle declares scratch tables as
/// `var table: lz4.HashTable = undefined;`. Rust has no `undefined`, so callers
/// write `[0; lz4::HASH_TABLE_LEN]`; the table's contents are irrelevant
/// because `compress` resets it on entry.
pub const HASH_TABLE_LEN: usize = 1 << HASH_LOG;

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

/// Compress `input` into a raw LZ4 block in `output`.
///
/// Returns the initialized length of `output`. The input and output buffers
/// must not overlap. `table` is scratch space and does not need to be
/// initialized by the caller; it is reset before use.
pub fn compress(
    input: &[u8],
    output: &mut [u8],
    table: &mut HashTable,
) -> Result<usize, CompressError> {
    if input.len() > MAX_INPUT_SIZE {
        return Err(CompressError::InputTooLarge);
    }

    // All-ones in either packed half means "no previous position".
    table.fill(u32::MAX);

    // `ip` is the current input position, `anchor` is the first literal not yet
    // emitted, and `op` is the next output position. A successful match emits
    // input[anchor..ip] as literals followed by the match, then moves both input
    // positions to the end of that match.
    let mut op: usize = 0;
    let mut anchor: usize = 0;

    // LZ4's format leaves the final five input bytes as literals and starts
    // the final match at least twelve bytes before the end. This is not
    // required by our safe decoder, but makes blocks compatible with fast
    // decoders that rely on the standard format restrictions. Inputs too
    // short for any match are emitted below as one literal-only sequence.
    if input.len() >= MATCH_FIND_LIMIT {
        let search_end = input.len() - MATCH_FIND_LIMIT;
        let match_end_limit = input.len() - LAST_LITERALS;
        let mut ip: usize = 0;
        let mut search_attempts: usize = 0;

        while ip <= search_end {
            // Hash the next four bytes and replace the table entry
            // immediately. Hash collisions are expected, so equality is
            // checked below before accepting a saved position as a match.
            let sequence = read_u32(input, ip);
            let hash = hash_sequence(sequence);
            let candidates = table[hash];
            remember_position(table, hash, ip);

            let mut match_pos = match candidate_position(ip, candidates as u16)
                .or_else(|| candidate_position(ip, (candidates >> 16) as u16))
            {
                Some(p) => p,
                None => {
                    advance_search(&mut ip, anchor, &mut search_attempts);
                    continue;
                }
            };

            if read_u32(input, match_pos) != sequence {
                // The nearest candidate collided. Fall back to the older
                // one, which must additionally match one byte beyond the
                // minimum so collision-prone minimum-length matches from
                // the stale half are not emitted.
                match candidate_position(ip, (candidates >> 16) as u16).filter(|&older| {
                    read_u32(input, older) == sequence
                        && input[older + MIN_MATCH] == input[ip + MIN_MATCH]
                }) {
                    Some(older) => match_pos = older,
                    None => {
                        advance_search(&mut ip, anchor, &mut search_attempts);
                        continue;
                    }
                }
            }

            // Pull the match backwards into the current literal run. This is
            // particularly helpful around aligned cell records. As with
            // forward extension, compare words before locating the first
            // differing byte.
            let (begin_position, begin_candidate) = match_begin(input, ip, match_pos, anchor);
            ip = begin_position;
            match_pos = begin_candidate;

            // We already compared the first four bytes. Continue up to the
            // point where the required last five literals begin. `match_end_position`
            // compares a machine word at a time before locating the first
            // differing byte.
            let match_end = match_end_position(
                input,
                ip + MIN_MATCH,
                match_pos + MIN_MATCH,
                match_end_limit,
            );

            // Restores the oracle's `@intCast` trap semantics under R1: the
            // offset must fit in u16, so assert rather than silently truncate.
            debug_assert!(ip - match_pos <= u16::MAX as usize);
            emit_sequence(
                output,
                &mut op,
                &input[anchor..ip],
                (ip - match_pos) as u16,
                match_end - ip,
            )?;

            // The main loop jumps over the matched bytes rather than hashing
            // every position within them. Seed one position near the end so
            // an adjacent repeated record can still refer back into this
            // match. The next loop iteration will then seed `match_end`
            // normally.
            if match_end >= 2 && match_end - 2 + MIN_MATCH <= input.len() {
                let seed = match_end - 2;
                remember_position(table, hash_sequence(read_u32(input, seed)), seed);
            }

            ip = match_end;
            anchor = ip;
            search_attempts = 0;
        }
    }

    // Whatever remains after the last match is the terminal literal-only
    // sequence. For short inputs this is also the only sequence in the block.
    emit_last_literals(output, &mut op, &input[anchor..])?;
    Ok(op)
}

/// Map a four-byte input sequence to its scratch-table slot.
#[inline(always)]
fn hash_sequence(sequence: u32) -> usize {
    (sequence.wrapping_mul(HASH_MULTIPLIER) >> (32 - HASH_LOG as u32)) as usize
}

/// Add a position to one hash slot and shift the previous newest position into
/// the fallback half. A position whose low bits are 0xFFFF conflicts with the
/// sentinel and is simply not stored.
#[inline(always)]
fn remember_position(table: &mut HashTable, hash: usize, position: usize) {
    let low = position as u16;
    if low == u16::MAX {
        return;
    }
    table[hash] = (table[hash] << 16) | (low as u32);
}

/// Recover the nearest preceding position from a stored low half. Modular
/// subtraction directly produces the LZ4 offset within the current window.
#[inline(always)]
fn candidate_position(ip: usize, stored: u16) -> Option<usize> {
    if stored == u16::MAX {
        return None;
    }
    let distance = (ip as u16).wrapping_sub(stored);
    if distance == 0 {
        return None;
    }
    Some(ip - distance as usize)
}

/// Advance through a literal run. The first KiB inspects every byte without
/// maintaining a counter, which keeps ordinary terminal-page searches cheap.
/// Longer runs enable gradually increasing steps for incompressible data.
#[inline(always)]
fn advance_search(ip: &mut usize, anchor: usize, attempts: &mut usize) {
    if *attempts == 0 {
        *ip += 1;
        if *ip - anchor == 1024 {
            *attempts = 1;
        }
        return;
    }

    *attempts += 1;
    *ip += 1 + *attempts / 64;
}

/// Extend a match backwards without crossing the current literal anchor or the
/// beginning of the candidate. The returned positions preserve their offset.
fn match_begin(input: &[u8], position_: usize, candidate_: usize, anchor: usize) -> (usize, usize) {
    let mut position = position_;
    let mut candidate = candidate_;

    while core::cmp::min(position - anchor, candidate) >= 8 {
        let position_word = position - 8;
        let candidate_word = candidate - 8;
        let difference = read_u64(input, position_word) ^ read_u64(input, candidate_word);
        if difference != 0 {
            // Little-endian reads make the MOST-significant differing bit map
            // to the LAST byte in memory, so leading zeros count equal trailing
            // bytes of the two words.
            let equal_bytes = (difference.leading_zeros() / 8) as usize;
            return (position - equal_bytes, candidate - equal_bytes);
        }

        position = position_word;
        candidate = candidate_word;
    }

    while position > anchor && candidate > 0 && input[position - 1] == input[candidate - 1] {
        position -= 1;
        candidate -= 1;
    }
    (position, candidate)
}

/// Return the first input position where two matching runs differ, or `limit`
/// when they remain equal. Both positions are known to have matched through
/// `MIN_MATCH` before this is called.
///
/// PORT NOTE: named `match_end_position` because `match_end` shadows a local in
/// `compress`; the oracle's function is `matchEnd`.
fn match_end_position(input: &[u8], position_: usize, candidate_: usize, limit: usize) -> usize {
    let mut position = position_;
    let mut candidate = candidate_;

    // The oracle relies on `position <= limit` and computes `limit - position`
    // directly, trapping on underflow; assert the invariant here since the
    // saturating form below would otherwise absorb a would-be invariant break.
    debug_assert!(position <= limit);

    // Reading as little endian makes the least-significant differing bit map
    // to the earliest byte in memory on every target.
    while limit.saturating_sub(position) >= 8 {
        let difference = read_u64(input, position) ^ read_u64(input, candidate);
        if difference != 0 {
            return position + (difference.trailing_zeros() / 8) as usize;
        }

        position += 8;
        candidate += 8;
    }

    while position < limit && input[position] == input[candidate] {
        position += 1;
        candidate += 1;
    }
    position
}

/// Decompress a raw LZ4 block into an exact-size output buffer.
///
/// Returns `output.len()` on success. Both consuming all input and filling all
/// output are required. Raw LZ4 blocks do not carry their decoded size, so this
/// exact-size contract validates the size metadata maintained by the caller.
/// The input and output buffers must not overlap (guaranteed by `&[u8]` +
/// `&mut [u8]` in Rust).
pub fn decompress(input: &[u8], output: &mut [u8]) -> Result<usize, DecompressError> {
    // `ip` and `op` always identify the next unread input byte and the next
    // unwritten output byte respectively.
    //
    // The decoder is written around one observation: almost every sequence
    // in real blocks has a short literal run and a short match. Both fast
    // paths below copy a fixed number of bytes blindly and let the length
    // arithmetic sort out how many of them were meaningful. Writing past a
    // run's logical end is safe within the output buffer because decoding
    // is strictly in order: every byte past `op` is either rewritten by a
    // later copy before anything can read it, or lies beyond the block's
    // final length and is never part of the result. The margin conditions
    // on the fast paths also subsume the exact bounds checks they replace,
    // which keeps decoding of malformed blocks memory-safe.
    let mut ip: usize = 0;
    let mut op: usize = 0;

    loop {
        // A normal block ends after the literal bytes of its final sequence.
        // This also accepts the empty block produced by our compressor, which
        // consists of a zero token and no literals.
        if ip == input.len() {
            if op != output.len() {
                return Err(DecompressError::OutputSizeMismatch);
            }
            return Ok(op);
        }

        let token = input[ip];
        ip += 1;

        // The high nibble and any extension bytes describe the literal run.
        // The literals are copied as a side effect of computing the length.
        let literal_len: usize = {
            let nibble = (token >> 4) as usize;
            if nibble != 15 && core::cmp::min(input.len() - ip, output.len() - op) >= 16 {
                // A run below the extension threshold is at most 14 bytes,
                // so with a 16-byte margin on both buffers one wide copy
                // covers it.
                output[op..op + 16].copy_from_slice(&input[ip..ip + 16]);
                nibble
            } else {
                // Extended or margin-poor runs take the checked path. Bounds are
                // verified before copying so malformed blocks never cause a
                // partial read or write.
                let len = decode_length(input, &mut ip, nibble)?;
                if len > input.len() - ip {
                    return Err(DecompressError::TruncatedInput);
                }
                if len > output.len() - op {
                    return Err(DecompressError::OutputTooSmall);
                }
                output[op..op + len].copy_from_slice(&input[ip..ip + len]);
                len
            }
        };
        ip += literal_len;
        op += literal_len;

        // Ending immediately after the literals marks the final sequence. Any
        // non-final sequence must continue with an offset and match length.
        if ip == input.len() {
            if op != output.len() {
                return Err(DecompressError::OutputSizeMismatch);
            }
            return Ok(op);
        }

        if input.len() - ip < 2 {
            return Err(DecompressError::TruncatedInput);
        }
        let offset = read_u16(input, ip) as usize;
        ip += 2;
        if offset == 0 || offset > op {
            return Err(DecompressError::InvalidOffset);
        }

        // The token stores the match length minus the four-byte minimum. As
        // with literals, a low nibble of 15 is extended by following bytes.
        let match_nibble = (token & 0x0F) as usize;

        // A match whose length fits its nibble spans at most 18 bytes, so
        // three blind copies always cover it. They are overlap-safe when the
        // offset is at least a word: each load lies a full word behind the
        // store which could observe it, so repeating patterns propagate
        // correctly.
        if match_nibble != 15 && offset >= 8 && output.len() - op >= 18 {
            let m = op - offset;
            copy_u64_within(output, op, m);
            copy_u64_within(output, op + 8, m + 8);
            copy_u16_within(output, op + 16, m + 16);
            op += match_nibble + MIN_MATCH;
            continue;
        }

        let encoded_match_len = decode_length(input, &mut ip, match_nibble)?;
        let match_len = encoded_match_len
            .checked_add(MIN_MATCH)
            .ok_or(DecompressError::OutputTooSmall)?;
        if match_len > output.len() - op {
            return Err(DecompressError::OutputTooSmall);
        }

        // Match copies may overlap, so this cannot always be one copy.
        // `copy_match` expands the common small repeating periods into wide
        // stores and uses word copies for larger offsets.
        copy_match(output, op, offset, match_len);
        op += match_len;
    }
}

/// Decode a length from its token nibble and any following extension bytes.
/// `ip` is advanced past every consumed extension byte.
fn decode_length(input: &[u8], ip: &mut usize, nibble: usize) -> Result<usize, DecompressError> {
    let mut length: usize = nibble;
    if nibble != 15 {
        return Ok(length);
    }

    loop {
        if *ip >= input.len() {
            return Err(DecompressError::TruncatedInput);
        }
        let value = input[*ip];
        *ip += 1;
        length = length
            .checked_add(value as usize)
            .ok_or(DecompressError::TruncatedInput)?;
        if value != 255 {
            return Ok(length);
        }
    }
}

/// Copy one decoded match from `offset` bytes behind `op`.
///
/// The caller has validated that the match fits: `op + match_len` never
/// exceeds `output.len()`. Wide copies may write a few scratch bytes past the
/// match's logical end; as described in `decompress`, that is safe anywhere
/// the write stays inside the output buffer. Every path therefore bounds its
/// wide stores by both the match end and the buffer end, and the bytewise
/// loop at the bottom finishes whatever remains.
fn copy_match(output: &mut [u8], op_: usize, offset: usize, match_len: usize) {
    let mut op = op_;
    let end = op_ + match_len;

    // A source which ends behind the copy can never overlap it. Long
    // distant matches are common in structured pages (repeated rows and
    // whole blank regions), and one exact copy moves them in cache-line
    // units. Shorter matches are not worth the call overhead.
    if offset >= match_len && match_len >= 64 {
        output.copy_within(op - offset..op - offset + match_len, op);
        return;
    }

    match offset {
        // A period which divides the word size expands into one repeated
        // pattern word. Long runs (blank lines, repeated cells) then become
        // independent stores, with no load waiting on a preceding store.
        // Stores advance by whole words from `op`, which preserves the
        // pattern's phase.
        1 | 2 | 4 | 8 => {
            let pattern: u64 = match offset {
                // PORT NOTE: none of these products can overflow `u64` —
                // the maximum factor of each pattern width times its
                // repetition constant is exactly `u64::MAX`
                // (0xFF * 0x0101…01, 0xFFFF * 0x0001…01, 0xFFFF_FFFF *
                // 0x0000_0001_0000_0001). Plain `*` therefore matches the
                // oracle under rustty's `overflow-checks = true` profiles.
                1 => (output[op - 1] as u64) * 0x0101_0101_0101_0101,
                2 => (read_u16(output, op - 2) as u64) * 0x0001_0001_0001_0001,
                4 => (read_u32(output, op - 4) as u64) * 0x0000_0001_0000_0001,
                8 => read_u64(output, op - 8),
                _ => unreachable!(),
            };

            let limit = core::cmp::min(end, output.len().saturating_sub(7));
            while op < limit {
                output[op..op + 8].copy_from_slice(&pattern.to_le_bytes());
                op += 8;
            }
        }

        // Wide copies are overlap-safe for the remaining offsets of at
        // least a copy unit: each load lies a full unit behind the store
        // which could observe it. Offsets 3, 5, 6, and 7 fall through to
        // the bytewise loop; they are rare in real data and word tricks
        // for them cost more in complexity than they return.
        _ => {
            if offset >= 16 {
                let limit = core::cmp::min(end, output.len().saturating_sub(15));
                while op < limit {
                    copy_u128_within(output, op, op - offset);
                    op += 16;
                }
            } else if offset >= 8 {
                let limit = core::cmp::min(end, output.len().saturating_sub(7));
                while op < limit {
                    copy_u64_within(output, op, op - offset);
                    op += 8;
                }
            }
        }
    }

    while op < end {
        output[op] = output[op - offset];
        op += 1;
    }
}

/// Emit one non-final sequence.
///
/// A sequence starts with a token, followed by optional literal length bytes,
/// the literals themselves, the two-byte offset, and optional match length
/// bytes. This function computes the complete size first so `OutputTooSmall`
/// is reported without partially writing a sequence.
fn emit_sequence(
    output: &mut [u8],
    op: &mut usize,
    literals: &[u8],
    offset: u16,
    match_len: usize,
) -> Result<(), CompressError> {
    debug_assert!(match_len >= MIN_MATCH);
    debug_assert!(offset > 0);

    let encoded_match_len = match_len - MIN_MATCH;

    // One byte is always needed for the token and two for the offset. Each
    // length may additionally need extension bytes after its token nibble.
    let required = 1
        + encoded_length_bytes(literals.len())
        + literals.len()
        + 2
        + encoded_length_bytes(encoded_match_len);
    if required > output.len() - *op {
        return Err(CompressError::OutputTooSmall);
    }

    let token_pos = *op;
    *op += 1;

    // Lengths below 15 fit directly in their nibble. Larger values put 15 in
    // the nibble and encode the remainder immediately after the token.
    output[token_pos] = ((core::cmp::min(literals.len(), 15) as u8) << 4)
        | (core::cmp::min(encoded_match_len, 15) as u8);

    // Literal length extensions precede the literals they describe.
    if literals.len() >= 15 {
        write_length(output, op, literals.len() - 15);
    }
    output[*op..*op + literals.len()].copy_from_slice(literals);
    *op += literals.len();

    // Match length extensions follow the offset because this is where the
    // decoder expects them in an LZ4 sequence.
    write_u16_at(output, *op, offset);
    *op += 2;
    if encoded_match_len >= 15 {
        write_length(output, op, encoded_match_len - 15);
    }

    Ok(())
}

/// Emit the literal-only sequence which terminates every block.
///
/// There is no offset or match length after these bytes. As with
/// `emit_sequence`, capacity is checked before modifying the output.
fn emit_last_literals(
    output: &mut [u8],
    op: &mut usize,
    literals: &[u8],
) -> Result<(), CompressError> {
    let required = 1 + encoded_length_bytes(literals.len()) + literals.len();
    if required > output.len() - *op {
        return Err(CompressError::OutputTooSmall);
    }

    output[*op] = (core::cmp::min(literals.len(), 15) as u8) << 4;
    *op += 1;
    if literals.len() >= 15 {
        write_length(output, op, literals.len() - 15);
    }
    output[*op..*op + literals.len()].copy_from_slice(literals);
    *op += literals.len();

    Ok(())
}

/// Return the number of extension bytes needed when a length is represented by
/// a token nibble plus zero or more bytes. An extended length always ends with
/// a byte below 255, so an exact multiple of 255 requires a final zero byte.
fn encoded_length_bytes(encoded_len: usize) -> usize {
    if encoded_len < 15 {
        return 0;
    }
    (encoded_len - 15) / 255 + 1
}

/// Write the portion of a length which did not fit in the token nibble.
///
/// Each 255 byte means "add 255 and continue". The final byte is always less
/// than 255 and may be zero.
fn write_length(output: &mut [u8], op: &mut usize, length_: usize) {
    let mut length = length_;
    while length >= 255 {
        output[*op] = 255;
        *op += 1;
        length -= 255;
    }
    output[*op] = length as u8;
    *op += 1;
}

/// Write an unaligned little-endian `u16` at `position`.
#[inline(always)]
fn write_u16_at(output: &mut [u8], position: usize, value: u16) {
    output[position..position + 2].copy_from_slice(&value.to_le_bytes());
}

// -- Unaligned little-endian reads / intra-buffer wide copies --------------
//
// PORT NOTE: the oracle uses `std.mem.readInt(..., .little)` on a fixed-size
// slice plus `copyIntAt` to move one integer between byte ranges. The Rust
// equivalents below are pure safe slice operations. `try_into().unwrap()` on a
// fixed-length subslice is a compile-time-sized conversion that LLVM lowers to
// an unaligned load; the `unwrap` is provably infallible.

#[inline(always)]
fn read_u16(buf: &[u8], pos: usize) -> u16 {
    u16::from_le_bytes(buf[pos..pos + 2].try_into().unwrap())
}

#[inline(always)]
fn read_u32(buf: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap())
}

#[inline(always)]
fn read_u64(buf: &[u8], pos: usize) -> u64 {
    u64::from_le_bytes(buf[pos..pos + 8].try_into().unwrap())
}

#[inline(always)]
fn copy_u16_within(buf: &mut [u8], dst: usize, src: usize) {
    let v = read_u16(buf, src);
    buf[dst..dst + 2].copy_from_slice(&v.to_le_bytes());
}

#[inline(always)]
fn copy_u64_within(buf: &mut [u8], dst: usize, src: usize) {
    let v = read_u64(buf, src);
    buf[dst..dst + 8].copy_from_slice(&v.to_le_bytes());
}

#[inline(always)]
fn copy_u128_within(buf: &mut [u8], dst: usize, src: usize) {
    let v = u128::from_le_bytes(buf[src..src + 16].try_into().unwrap());
    buf[dst..dst + 16].copy_from_slice(&v.to_le_bytes());
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

    /// PORTS: oracle@main lz4.zig `test "literal-only compatibility vectors"`
    #[test]
    fn lz4_literal_only_compatibility_vectors() {
        let mut empty: [u8; 0] = [];
        assert_eq!(decompress(&[0], &mut empty), Ok(0));

        let mut hello = [0u8; 5];
        assert_eq!(
            decompress(&[0x50, b'h', b'e', b'l', b'l', b'o'], &mut hello),
            Ok(5)
        );
        assert_eq!(&hello, b"hello");

        let mut fifteen = [0u8; 15];
        let mut encoded = [b'x'; 17];
        encoded[0] = 0xF0;
        encoded[1] = 0;
        decompress(&encoded, &mut fifteen).unwrap();
        assert!(fifteen.iter().all(|&b| b == b'x'));
    }

    /// PORTS: oracle@main lz4.zig `test "overlapping match compatibility vector"`
    ///
    /// One literal 'a', followed by a four-byte match at distance one.
    #[test]
    fn lz4_overlapping_match_compatibility_vector() {
        let mut output = [0u8; 5];
        assert_eq!(decompress(&[0x10, b'a', 0x01, 0x00], &mut output), Ok(5));
        assert_eq!(&output, b"aaaaa");
    }

    /// PORTS: oracle@main lz4.zig `test "extended overlapping match compatibility vector"`
    ///
    /// One literal followed by a 274-byte match. The match extension is
    /// encoded as 255 + 0 after the low token nibble's initial 15 bytes.
    #[test]
    fn lz4_extended_overlapping_match_compatibility_vector() {
        let mut output = [0u8; 275];
        assert_eq!(
            decompress(&[0x1F, b'a', 0x01, 0x00, 0xFF, 0x00], &mut output),
            Ok(275)
        );
        assert!(output.iter().all(|&b| b == b'a'));
    }

    /// PORTS: oracle@main lz4.zig `test "short offset compatibility vectors"`
    ///
    /// These blocks end immediately after their match. Besides covering the
    /// repeating-pattern paths, they verify that the decoder uses exact copies
    /// when the block does not provide the standard trailing-literal margin.
    #[test]
    fn lz4_short_offset_compatibility_vectors() {
        let mut offset_two = [0u8; 6];
        decompress(&[0x20, b'a', b'b', 0x02, 0x00], &mut offset_two).unwrap();
        assert_eq!(&offset_two, b"ababab");

        let mut offset_three = [0u8; 9];
        decompress(&[0x32, b'a', b'b', b'c', 0x03, 0x00], &mut offset_three).unwrap();
        assert_eq!(&offset_three, b"abcabcabc");

        let mut offset_four = [0u8; 8];
        decompress(
            &[0x40, b'a', b'b', b'c', b'd', 0x04, 0x00],
            &mut offset_four,
        )
        .unwrap();
        assert_eq!(&offset_four, b"abcdabcd");
    }

    /// PORTS: oracle@main lz4.zig
    /// `test "bounded wild copies are overwritten by final literals"`
    #[test]
    fn lz4_bounded_wild_copies_are_overwritten_by_final_literals() {
        // The first sequence's nine-byte match leaves the five final literals
        // required by the LZ4 block format. Its logical one-byte tail is copied
        // as a word and the following literal sequence overwrites the extra
        // bytes.
        let mut repeated_byte = [0u8; 15];
        decompress(
            &[0x15, b'a', 0x01, 0x00, 0x50, b'1', b'2', b'3', b'4', b'5'],
            &mut repeated_byte,
        )
        .unwrap();
        assert_eq!(&repeated_byte, b"aaaaaaaaaa12345");

        // Exercise the same bounded tail copy with a non-overlapping offset.
        let mut word_offset = [0u8; 22];
        decompress(
            &[
                0x85, b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h', 0x08, 0x00, 0x50, b'1', b'2',
                b'3', b'4', b'5',
            ],
            &mut word_offset,
        )
        .unwrap();
        assert_eq!(&word_offset, b"abcdefghabcdefgha12345");
    }

    /// PORTS: oracle@main lz4.zig `test "decompress rejects malformed blocks"`
    #[test]
    fn lz4_decompress_rejects_malformed_blocks() {
        let mut output = [0u8; 32];
        assert_eq!(
            decompress(&[0xF0], &mut output),
            Err(DecompressError::TruncatedInput)
        );
        assert_eq!(
            decompress(&[0x10, b'a', 1], &mut output[0..5]),
            Err(DecompressError::TruncatedInput)
        );
        assert_eq!(
            decompress(&[0x10, b'a', 0, 0], &mut output[0..5]),
            Err(DecompressError::InvalidOffset)
        );
        assert_eq!(
            decompress(&[0x10, b'a', 2, 0], &mut output[0..5]),
            Err(DecompressError::InvalidOffset)
        );
        assert_eq!(
            decompress(&[0x10, b'a', 1, 0], &mut output[0..4]),
            Err(DecompressError::OutputTooSmall)
        );
        assert_eq!(
            decompress(&[0], &mut output[0..1]),
            Err(DecompressError::OutputSizeMismatch)
        );
    }

    /// PORTS: oracle@main lz4.zig `test "maximum match offset compatibility vector"`
    ///
    /// Builds a maximum-offset block using the encoder's own `write_length`,
    /// then decodes it. This is the encoder/decoder handshake at the format's
    /// 16-bit offset boundary.
    #[test]
    fn lz4_maximum_match_offset_compatibility_vector() {
        let literal_len: usize = u16::MAX as usize;
        let extension_len = (literal_len - 15) / 255 + 1;
        let mut encoded = vec![0u8; 1 + extension_len + literal_len + 2];
        let mut output = vec![0u8; literal_len + MIN_MATCH];

        let mut op: usize = 0;
        encoded[op] = 0xF0;
        op += 1;
        write_length(&mut encoded, &mut op, literal_len - 15);
        for (i, byte) in encoded[op..op + literal_len].iter_mut().enumerate() {
            *byte = i as u8;
        }
        op += literal_len;
        write_u16_at(&mut encoded, op, u16::MAX);
        op += 2;

        assert_eq!(encoded.len(), op);
        assert_eq!(decompress(&encoded, &mut output), Ok(output.len()));
        assert_eq!(
            &encoded[1 + extension_len..1 + extension_len + 4],
            &output[literal_len..]
        );
    }

    /// Direct unit coverage for the extension-byte length encoding rule: an
    /// exact multiple of 255 needs a trailing zero byte.
    #[test]
    fn lz4_encoded_length_bytes_boundaries() {
        assert_eq!(encoded_length_bytes(0), 0);
        assert_eq!(encoded_length_bytes(14), 0);
        assert_eq!(encoded_length_bytes(15), 1);
        assert_eq!(encoded_length_bytes(15 + 254), 1);
        assert_eq!(encoded_length_bytes(15 + 255), 2);
    }

    /// Shared round-trip assertion used by the corpus-style tests below.
    /// PORTS: oracle@main lz4.zig `fn expectRoundTrip`
    fn expect_round_trip(input: &[u8]) {
        let bound = compress_bound(input.len()).unwrap();
        let mut encoded = vec![0u8; bound];
        let mut decoded = vec![0u8; input.len()];

        let mut table: HashTable = [0; 1 << HASH_LOG];
        let encoded_len = compress(input, &mut encoded, &mut table).unwrap();
        assert_eq!(
            decompress(&encoded[..encoded_len], &mut decoded),
            Ok(input.len())
        );
        assert_eq!(&decoded[..], input);
    }

    /// PORTS: oracle@main lz4.zig `test "round trips boundary-sized inputs"`
    #[test]
    fn lz4_round_trips_boundary_sized_inputs() {
        const LENGTHS: [usize; 22] = [
            0, 1, 3, 4, 5, 12, 15, 16, 19, 20, 254, 255, 256, 269, 270, 271, 510, 511, 512, 65_535,
            65_536, 65_537,
        ];
        for len in LENGTHS {
            let buf: Vec<u8> = (0..len).map(|i| (i.wrapping_mul(31)) as u8).collect();
            expect_round_trip(&buf);
        }
    }

    /// PORTS: oracle@main lz4.zig `test "round trips compressible page-sized inputs"`
    #[test]
    fn lz4_round_trips_compressible_page_sized_inputs() {
        const PAGE_LEN: usize = 400 * 1024;

        let zeros = vec![0u8; PAGE_LEN];
        expect_round_trip(&zeros);

        let mut structured = vec![0u8; PAGE_LEN];
        for i in 0..PAGE_LEN / 8 {
            structured[i * 8] = (b' ' as usize + i % 95) as u8;
            structured[i * 8 + 4] = ((i / 80) % 16) as u8;
        }
        expect_round_trip(&structured);
    }

    /// PORTS: oracle@main lz4.zig `test "round trips deterministic random inputs"`
    ///
    /// PORT NOTE: the oracle uses `std.Random.DefaultPrng` (xoshiro256++). The
    /// exact stream does not matter — the property is round-trip identity over
    /// incompressible data — so this uses a small deterministic xorshift64*.
    #[test]
    fn lz4_round_trips_deterministic_random_inputs() {
        let mut state: u64 = 0x4C5A_3401;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..256 {
            let len = (next() % (32 * 1024)) as usize;
            let input: Vec<u8> = (0..len).map(|_| next() as u8).collect();
            expect_round_trip(&input);
        }
    }

    /// PORTS: oracle@main lz4.zig `test "compress reports short output"`
    #[test]
    fn lz4_compress_reports_short_output() {
        let input = b"a terminal page needs enough output space";
        let mut table: HashTable = [0; 1 << HASH_LOG];
        let mut output = [0u8; 4];
        assert_eq!(
            compress(input, &mut output, &mut table),
            Err(CompressError::OutputTooSmall)
        );
    }
}
