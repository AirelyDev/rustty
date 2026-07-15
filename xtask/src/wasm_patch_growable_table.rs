//! PORTS: ghostty `src/build/wasm_patch_growable_table.zig`.
//!
//! Build tool that patches a WASM binary to make the function table
//! growable by removing its maximum size limit.
//!
//! Zig's WASM linker doesn't support `--growable-table`, so the table
//! is emitted with max == min. This tool finds the table section (id 4)
//! and changes the limits flag from 0x01 (has max) to 0x00 (no max),
//! removing the max field.
//!
//! Usage: wasm_growable_table <input.wasm> <output.wasm>

use std::borrow::Cow;

// PORT: `const std = @import("std");` — the Rust std slice/Vec primitives are
// used directly. `testing` / `Allocator` have no standalone Rust analog: Rust's
// growable byte buffer is `Vec<u8>` (no explicit allocator threaded through) and
// the tests live in the `#[cfg(test)] mod tests` block below.

// PORT: the reference `error{InvalidWasm} || std.Io.Writer.Error` error set.
// Writing into a `Vec<u8>` is infallible in Rust (no `Writer.Error`), so the
// only reachable variant is `InvalidWasm`.
#[derive(Debug, PartialEq, Eq)]
pub enum PatchError {
    InvalidWasm,
}

// PORT: `pub fn main() !void { ... }` — the process entry point. In rustty this
// is driven from the xtask `wasm-patch-growable-table <input> <output>`
// subcommand (see main.rs); the argv parsing, read, patch, and write live
// there. The one-off leak-on-purpose GPA maps to Rust's default allocator with
// the buffers simply dropped/handed off. The core logic is `patch_table_growable`
// below, ported verbatim.

/// Patch the WASM binary's table section to remove the maximum size
/// limit, making the table growable. If the table already has no max
/// or no table section is found, the input is returned unchanged.
///
/// The WASM table section (id=4) encodes table limits as:
///   - flags=0x00, min (LEB128)          — no max, growable
///   - flags=0x01, min (LEB128), max (LEB128) — bounded, not growable
///
/// This function rewrites the section to use flags=0x00, dropping the
/// max field entirely.
// PORT: the "returned unchanged" cases return the input pointer verbatim in
// Zig (the tests assert `result.ptr == input.ptr`). The faithful Rust encoding
// is `Cow<'a, [u8]>`: `Cow::Borrowed(input)` for the unchanged cases (same
// backing bytes, no copy) and `Cow::Owned(Vec)` for the rewritten section.
pub fn patch_table_growable(input: &[u8]) -> Result<Cow<'_, [u8]>, PatchError> {
    if input.len() < 8 {
        return Err(PatchError::InvalidWasm);
    }

    // Start after the 8-byte WASM header (magic + version).
    let mut pos: usize = 8;

    while pos < input.len() {
        let section_id = input[pos];
        pos += 1;
        let section_size = read_leb128(input, &mut pos) as usize;
        let section_start = pos;

        // We're looking for section 4 (the table section).
        if section_id != 4 {
            pos = section_start + section_size;
            continue;
        }

        let _ = read_leb128(input, &mut pos); // table count
        pos += 1; // elem_type (0x70 = funcref)
        let flags = input[pos];

        // flags bit 0 indicates whether a max is present.
        if flags & 1 == 0 {
            // Already no max, nothing to patch.
            return Ok(Cow::Borrowed(input));
        }

        // Record positions of each field so we can reconstruct
        // the section without the max value.
        let flags_pos = pos;
        pos += 1; // skip flags byte
        let min_start = pos;
        let _ = read_leb128(input, &mut pos); // min
        let max_start = pos;
        let _ = read_leb128(input, &mut pos); // max
        let max_end = pos;
        let section_end = section_start + section_size;

        // Build the new section payload with the max removed:
        //   [table count + elem_type] [flags=0x00] [min] [trailing data]
        let mut payload: Vec<u8> = Vec::new();
        payload.extend_from_slice(&input[section_start..flags_pos]);
        payload.push(0x00); // flags: no max
        payload.extend_from_slice(&input[min_start..max_start]);
        payload.extend_from_slice(&input[max_end..section_end]);

        // Reassemble the full binary:
        //   [everything before this section] [section id] [new size] [new payload] [everything after]
        let before_section = &input[0..section_start - 1 - uleb128_size(section_size as u32)];
        let mut result: Vec<u8> = Vec::new();
        result.extend_from_slice(before_section);
        result.push(4); // table section id
        write_uleb128(&mut result, payload.len() as u32);
        result.extend_from_slice(&payload);
        result.extend_from_slice(&input[section_end..]);
        return Ok(Cow::Owned(result));
    }

    // No table section found; return input unchanged.
    Ok(Cow::Borrowed(input))
}

/// Decode an unsigned LEB128 value from `bytes` starting at `pos.*`,
/// advancing `pos` past the encoded bytes.
fn read_leb128(bytes: &[u8], pos: &mut usize) -> u32 {
    let mut result: u32 = 0;
    // PORT: `var shift: u5 = 0;` and `shift +%= 7;`. In Rust `wrapping_shl`
    // masks the shift amount by `& 31` (exactly the u5 domain) and
    // `wrapping_add` reproduces the `+%=` wraparound at 32.
    let mut shift: u32 = 0;
    loop {
        let byte = bytes[*pos];
        *pos += 1;
        result |= ((byte & 0x7f) as u32).wrapping_shl(shift);
        if byte & 0x80 == 0 {
            return result;
        }
        shift = shift.wrapping_add(7);
    }
}

/// Return the number of bytes needed to encode `value` as unsigned LEB128.
fn uleb128_size(value: u32) -> usize {
    let mut v = value;
    let mut size: usize = 0;
    loop {
        v >>= 7;
        size += 1;
        if v == 0 {
            return size;
        }
    }
}

// PORT: `try result.writer.writeUleb128(@as(u32, ...))` — the std Writer's
// unsigned-LEB128 emitter. Rust's `Vec<u8>` has no built-in equivalent, so this
// helper encodes `value` as unsigned LEB128 and appends it, matching the
// standard 7-bits-per-byte / high-bit-continuation form.
fn write_uleb128(out: &mut Vec<u8>, value: u32) {
    let mut v = value;
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            out.push(byte | 0x80);
        } else {
            out.push(byte);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid WASM module with a bounded table (min=1, max=1).
    /// Sections: type(1), table(4), export(7).
    #[rustfmt::skip]
    const TEST_WASM_BOUNDED_TABLE: [u8; 24] = [
        0x00, 0x61, 0x73, 0x6d, // magic
        0x01, 0x00, 0x00, 0x00, // version
        // Type section (id=1): 1 type, () -> ()
        0x01, 0x04, 0x01, 0x60,
        0x00, 0x00,
        // Table section (id=4): 1 table, funcref, flags=1, min=1, max=1
        0x04, 0x05,
        0x01, 0x70, 0x01, 0x01,
        0x01,
        // Export section (id=7): 0 exports
        0x07, 0x01, 0x00,
    ];

    /// Same module but the table already has no max (flags=0).
    #[rustfmt::skip]
    const TEST_WASM_GROWABLE_TABLE: [u8; 23] = [
        0x00, 0x61, 0x73, 0x6d, // magic
        0x01, 0x00, 0x00, 0x00, // version
        // Type section (id=1)
        0x01, 0x04, 0x01, 0x60,
        0x00, 0x00,
        // Table section (id=4): 1 table, funcref, flags=0, min=1
        0x04, 0x04,
        0x01, 0x70, 0x00, 0x01,
        // Export section (id=7): 0 exports
        0x07, 0x01, 0x00,
    ];

    /// Module with no table section at all.
    #[rustfmt::skip]
    const TEST_WASM_NO_TABLE: [u8; 17] = [
        0x00, 0x61, 0x73, 0x6d, // magic
        0x01, 0x00, 0x00, 0x00, // version
        // Type section (id=1)
        0x01, 0x04, 0x01, 0x60,
        0x00, 0x00,
        // Export section (id=7): 0 exports
        0x07, 0x01,
        0x00,
    ];

    #[test]
    fn patches_bounded_table_to_remove_max() {
        // We use a non-checking allocator because the patched result is
        // intentionally leaked (matches the real main() usage).
        let result = patch_table_growable(&TEST_WASM_BOUNDED_TABLE).unwrap();

        // Result should differ from input (max was removed).
        assert!(result.as_ref() != TEST_WASM_BOUNDED_TABLE.as_slice());

        // Find the table section in the output and verify flags=0x00.
        let result = result.as_ref();
        let mut pos: usize = 8;
        while pos < result.len() {
            let id = result[pos];
            pos += 1;
            let size = read_leb128(result, &mut pos) as usize;
            if id == 4 {
                let _ = read_leb128(result, &mut pos); // table count
                pos += 1; // elem_type
                // flags should now be 0x00 (no max).
                assert_eq!(0x00u8, result[pos]);
                return;
            }
            pos += size;
        }
        panic!("TableSectionNotFound");
    }

    #[test]
    fn already_growable_table_is_returned_unchanged() {
        let result = patch_table_growable(&TEST_WASM_GROWABLE_TABLE).unwrap();
        // PORT: `try testing.expectEqual(@as([*]const u8,
        // &test_wasm_growable_table), result.ptr);` — the Zig test asserts the
        // returned pointer is the input pointer (no copy). The `Cow::Borrowed`
        // variant IS that "same backing bytes" invariant; assert both the
        // variant and byte-identity.
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result.as_ref(), TEST_WASM_GROWABLE_TABLE.as_slice());
    }

    #[test]
    fn no_table_section_returns_input_unchanged() {
        let result = patch_table_growable(&TEST_WASM_NO_TABLE).unwrap();
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result.as_ref(), TEST_WASM_NO_TABLE.as_slice());
    }

    #[test]
    fn too_short_input_returns_invalid_wasm() {
        assert_eq!(
            Err(PatchError::InvalidWasm),
            patch_table_growable(b"short"),
        );
    }

    #[test]
    fn read_leb128_single_byte() {
        let bytes = [0x05u8];
        let mut pos: usize = 0;
        assert_eq!(5u32, read_leb128(&bytes, &mut pos));
        assert_eq!(1usize, pos);
    }

    #[test]
    fn read_leb128_multi_byte() {
        // 300 = 0b100101100 → LEB128: 0xAC 0x02
        let bytes = [0xACu8, 0x02];
        let mut pos: usize = 0;
        assert_eq!(300u32, read_leb128(&bytes, &mut pos));
        assert_eq!(2usize, pos);
    }

    #[test]
    fn uleb128_size_test() {
        assert_eq!(1usize, uleb128_size(0));
        assert_eq!(1usize, uleb128_size(0x7f));
        assert_eq!(2usize, uleb128_size(0x80));
        assert_eq!(2usize, uleb128_size(300));
        assert_eq!(5usize, uleb128_size(u32::MAX));
    }
}
