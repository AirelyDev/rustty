//! rustty-vt-c row
//!
//! PORTS: ghostty src/terminal/c/row.zig
//!
//! PORT STATUS: FILLED (cluster Stage B v2). Types byte-exact; `get`/`get_multi`/
//! `getTyped` bodies + all `test {}` blocks are ported.

use crate::result::Result;
use std::ffi::{c_int, c_void};

// const page = @import("../page.zig");
// const Row = page.Row;
use rustty_vt::page::Row;

/// C: GhosttyRow -> RusttyRow (R2)
// pub const CRow = Row.C;  (Row.C = u64)
pub type CRow = u64;

/// C: GhosttyRowSemanticPrompt -> RusttyRowSemanticPrompt (R2)
// pub const SemanticPrompt = enum(c_int) { none = 0, prompt = 1, prompt_continuation = 2 };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticPrompt {
    None = 0,
    Prompt = 1,
    PromptContinuation = 2,
}

/// C: GhosttyRowData -> RusttyRowData (R2)
// pub const RowData = enum(c_int) { invalid = 0, ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowData {
    Invalid = 0,

    /// Whether this row is soft-wrapped.
    /// Output type: bool *
    Wrap = 1,

    /// Whether this row is a continuation of a soft-wrapped row.
    /// Output type: bool *
    WrapContinuation = 2,

    /// Whether any cells in this row have grapheme clusters.
    /// Output type: bool *
    Grapheme = 3,

    /// Whether any cells in this row have styling (may have false positives).
    /// Output type: bool *
    Styled = 4,

    /// Whether any cells in this row have hyperlinks (may have false positives).
    /// Output type: bool *
    Hyperlink = 5,

    /// The semantic prompt state of this row.
    /// Output type: GhosttyRowSemanticPrompt *
    SemanticPrompt = 6,

    /// Whether this row contains a Kitty virtual placeholder.
    /// Output type: bool *
    KittyVirtualPlaceholder = 7,

    /// Whether this row is dirty and requires a redraw.
    /// Output type: bool *
    Dirty = 8,
}

// D11-style ABI anchor (see `render::Data::from_c_int`): proves reading a
// `get_multi` key element out of C memory as a `c_int` leaves the ABI unchanged
// (the published header declares `const RusttyRowData *keys`).
const _: () = assert!(std::mem::size_of::<RowData>() == std::mem::size_of::<c_int>());

// PORT NOTE (W84-B01-01): range-checked `c_int -> RowData` projection — the
// reference's `std.enums.fromInt(RowData, @intFromEnum(data))` guard. `RowData`
// is a contiguous `#[repr(C)]` enum, so the range check IS `intToEnum`, after
// which a `transmute` from the validated `c_int` is the enum. This used to live
// in `lib.rs` (`row_data_from_c_int`), reachable only from the single
// `rustty_row_get` wrapper; it lives here now so `get_multi` reaches it per
// array element too, exactly as the reference's `get_multi` reaches the guard
// by calling `get`.
const ROW_DATA_COUNT: c_int = 9; // contiguous 0..=8

impl RowData {
    /// Narrow an inbound C integer to a `RowData`, mirroring
    /// `std.enums.fromInt`; `None` outside the declared range. `Invalid = 0` is
    /// IN range and keeps its `.invalid_value` disposition inside `get`.
    pub fn from_c_int(v: c_int) -> Option<RowData> {
        if (0..ROW_DATA_COUNT).contains(&v) {
            // SAFETY: `v` is a valid `RowData` discriminant (range-checked
            // above).
            Some(unsafe { std::mem::transmute::<c_int, RowData>(v) })
        } else {
            None
        }
    }
}

// Output type expected for querying the data of the given kind.
// PORT NOTE: `pub fn OutType(comptime self: RowData) type`:
//     .invalid => void,
//     .wrap, .wrap_continuation, .grapheme, .styled, .hyperlink => bool,
//     .kitty_virtual_placeholder, .dirty => bool,
//     .semantic_prompt => SemanticPrompt,
// In Rust the OutType mapping is preserved as the per-arm pointer cast in
// `get_typed`.

/// PORT NOTE: `pub fn get(row_, data, out) callconv(...) Result`.
///
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn get(row_: CRow, data: RowData, out: *mut c_void) -> Result {
    // PORT NOTE: the runtime-safety `intToEnum(RowData, @intFromEnum(data))`
    // guard on the raw `c_int` crossing the C boundary lives in `get_checked`
    // below (shared by the `extern "C"` wrapper in `lib.rs` AND by
    // `get_multi`); by the time `get` is reached the `data` enum is already
    // validated.

    // return switch (data) {
    //     .invalid => .invalid_value,
    //     inline else => |comptime_data| getTyped(row_, comptime_data, @ptrCast(@alignCast(out))),
    // };
    match data {
        RowData::Invalid => Result::InvalidValue,
        // SAFETY: `out` validity/type is the caller's contract per `data`.
        _ => unsafe { get_typed(row_, data, out) },
    }
}

// PORT NOTE (W84-B01-01): the reference carries the runtime-safety guard INSIDE
// `get`, so every caller reaches it — including `get_multi`, which calls `get`
// per array element. rustty's `get` takes the already-narrowed `RowData` (a
// `#[repr(C)]` enum cannot even hold an out-of-range tag), so the guard lives
// here, in the raw-`c_int` entry point shared by the `extern "C"` wrapper in
// `lib.rs` AND by `get_multi` — the same "every path goes through the guard"
// shape as the reference.
///
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn get_checked(row_: CRow, data: c_int, out: *mut c_void) -> Result {
    // _ = std.enums.fromInt(RowData, @intFromEnum(data)) orelse {
    //     return .invalid_value;
    // };
    let Some(d) = RowData::from_c_int(data) else {
        return Result::InvalidValue;
    };
    // SAFETY: `out` validity/type is the caller's contract.
    unsafe { get(row_, d, out) }
}

/// PORT NOTE: `pub fn get_multi(row_, count, keys, values, out_written) Result`.
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
// PORT NOTE (W84-B01-01): `keys` is C memory typed `const RusttyRowData *` in
// the published header; each element is read as a raw `c_int` (ABI-identical —
// `RowData` is `#[repr(C)]`, asserted int-sized above) and range-checked by
// `get_checked`. Reading an element straight into a `RowData` would materialize
// an out-of-range discriminant, which is UB in Rust in EVERY build mode (where
// the reference merely loses its `runtime_safety` guard).
pub unsafe fn get_multi(
    row_: CRow,
    count: usize,
    keys: *const c_int,
    values: *mut *mut c_void,
    out_written: *mut usize,
) -> Result {
    // const k = keys orelse return .invalid_value;
    // const v = values orelse return .invalid_value;
    if keys.is_null() {
        return Result::InvalidValue;
    }
    if values.is_null() {
        return Result::InvalidValue;
    }

    // for (0..count) |i| {
    //     const result = get(row_, k[i], v[i]);
    //     if (result != .success) {
    //         if (out_written) |w| w.* = i;
    //         return result;
    //     }
    // }
    for i in 0..count {
        // SAFETY: caller guarantees `keys`/`values` have `count` elements.
        let key = unsafe { *keys.add(i) };
        let value = unsafe { *values.add(i) };
        // SAFETY: `value` validity/type is the caller's contract.
        let result = unsafe { get_checked(row_, key, value) };
        if result != Result::Success {
            if !out_written.is_null() {
                // SAFETY: caller-provided out pointer, checked non-null.
                unsafe { *out_written = i };
            }
            return result;
        }
    }
    // if (out_written) |w| w.* = count;
    if !out_written.is_null() {
        // SAFETY: caller-provided out pointer, checked non-null.
        unsafe { *out_written = count };
    }
    // return .success;
    Result::Success
}

/// PORT NOTE: `fn getTyped(row_, comptime data, out: *data.OutType()) Result`.
///
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
unsafe fn get_typed(row_: CRow, data: RowData, out: *mut c_void) -> Result {
    // const row: Row = @bitCast(row_);
    let row = Row::from_bits(row_);
    match data {
        // .invalid => return .invalid_value,
        RowData::Invalid => return Result::InvalidValue,
        // .wrap => out.* = row.wrap,
        RowData::Wrap => {
            // SAFETY: `out` points to a writable `bool` per OutType.
            unsafe { *(out as *mut bool) = row.wrap() };
        }
        // .wrap_continuation => out.* = row.wrap_continuation,
        RowData::WrapContinuation => {
            // SAFETY: `out` points to a writable `bool` per OutType.
            unsafe { *(out as *mut bool) = row.wrap_continuation() };
        }
        // .grapheme => out.* = row.grapheme,
        RowData::Grapheme => {
            // SAFETY: `out` points to a writable `bool` per OutType.
            unsafe { *(out as *mut bool) = row.grapheme() };
        }
        // .styled => out.* = row.styled,
        RowData::Styled => {
            // SAFETY: `out` points to a writable `bool` per OutType.
            unsafe { *(out as *mut bool) = row.styled() };
        }
        // .hyperlink => out.* = row.hyperlink,
        RowData::Hyperlink => {
            // SAFETY: `out` points to a writable `bool` per OutType.
            unsafe { *(out as *mut bool) = row.hyperlink() };
        }
        // .semantic_prompt => out.* = @enumFromInt(@intFromEnum(row.semantic_prompt)),
        RowData::SemanticPrompt => {
            let v = match row.semantic_prompt() {
                rustty_vt::page::SemanticPrompt::None => SemanticPrompt::None,
                rustty_vt::page::SemanticPrompt::Prompt => SemanticPrompt::Prompt,
                rustty_vt::page::SemanticPrompt::PromptContinuation => {
                    SemanticPrompt::PromptContinuation
                }
            };
            // SAFETY: `out` points to a writable `SemanticPrompt` per OutType.
            unsafe { *(out as *mut SemanticPrompt) = v };
        }
        // .kitty_virtual_placeholder => out.* = row.kitty_virtual_placeholder,
        RowData::KittyVirtualPlaceholder => {
            // SAFETY: `out` points to a writable `bool` per OutType.
            unsafe { *(out as *mut bool) = row.kitty_virtual_placeholder() };
        }
        // .dirty => out.* = row.dirty,
        RowData::Dirty => {
            // SAFETY: `out` points to a writable `bool` per OutType.
            unsafe { *(out as *mut bool) = row.dirty() };
        }
    }

    // return .success;
    Result::Success
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_vt::page::SemanticPrompt as ZigSemanticPrompt;
    use std::ffi::c_void;

    // test "get wrap"
    #[test]
    fn get_wrap() {
        // var zig_row: Row = @bitCast(@as(u64, 0)); zig_row.wrap = true;
        let mut zig_row = Row::from_bits(0u64);
        zig_row.set_wrap(true);
        // const row: CRow = @bitCast(zig_row);
        let row: CRow = zig_row.to_bits();
        let mut wrap: bool = false;
        // try testing.expectEqual(Result.success, get(row, .wrap, @ptrCast(&wrap)));
        assert_eq!(Result::Success, unsafe {
            get(row, RowData::Wrap, &mut wrap as *mut bool as *mut c_void)
        });
        // try testing.expect(wrap);
        assert!(wrap);
    }

    // test "get semantic_prompt"
    #[test]
    fn get_semantic_prompt() {
        // var zig_row: Row = @bitCast(@as(u64, 0)); zig_row.semantic_prompt = .prompt;
        let mut zig_row = Row::from_bits(0u64);
        zig_row.set_semantic_prompt(ZigSemanticPrompt::Prompt);
        let row: CRow = zig_row.to_bits();
        let mut sp: SemanticPrompt = SemanticPrompt::None;
        assert_eq!(Result::Success, unsafe {
            get(
                row,
                RowData::SemanticPrompt,
                &mut sp as *mut SemanticPrompt as *mut c_void,
            )
        });
        // try testing.expectEqual(SemanticPrompt.prompt, sp);
        assert_eq!(SemanticPrompt::Prompt, sp);
    }

    // test "get dirty"
    #[test]
    fn get_dirty() {
        // var zig_row: Row = @bitCast(@as(u64, 0)); zig_row.dirty = true;
        let mut zig_row = Row::from_bits(0u64);
        zig_row.set_dirty(true);
        let row: CRow = zig_row.to_bits();
        let mut dirty: bool = false;
        assert_eq!(Result::Success, unsafe {
            get(row, RowData::Dirty, &mut dirty as *mut bool as *mut c_void)
        });
        // try testing.expect(dirty);
        assert!(dirty);
    }

    // test "get_multi success"
    #[test]
    fn get_multi_success() {
        // var zig_row: Row = @bitCast(@as(u64, 0)); zig_row.wrap = true; zig_row.dirty = true;
        let mut zig_row = Row::from_bits(0u64);
        zig_row.set_wrap(true);
        zig_row.set_dirty(true);
        // const row_val: CRow = @bitCast(zig_row);
        let row_val: CRow = zig_row.to_bits();

        let mut wrap: bool = false;
        let mut dirty: bool = false;
        let mut written: usize = 0;

        // const keys = [_]RowData{ .wrap, .dirty };
        // PORT NOTE (W84-B01-01): the key array crosses the C boundary as raw
        // ints; `get_multi` reads each element as a `c_int` and guards it.
        let keys: [c_int; 2] = [RowData::Wrap as c_int, RowData::Dirty as c_int];
        // var values = [_]?*anyopaque{ @ptrCast(&wrap), @ptrCast(&dirty) };
        let mut values = [
            &mut wrap as *mut bool as *mut c_void,
            &mut dirty as *mut bool as *mut c_void,
        ];
        // try testing.expectEqual(Result.success, get_multi(row_val, keys.len, &keys, &values, &written));
        assert_eq!(Result::Success, unsafe {
            get_multi(
                row_val,
                keys.len(),
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            )
        });
        // try testing.expectEqual(keys.len, written);
        assert_eq!(keys.len(), written);
        // try testing.expect(wrap);
        assert!(wrap);
        // try testing.expect(dirty);
        assert!(dirty);
    }

    // test "get_multi error sets out_written"
    #[test]
    fn get_multi_error_sets_out_written() {
        // var zig_row: Row = @bitCast(@as(u64, 0)); zig_row.wrap = true;
        let mut zig_row = Row::from_bits(0u64);
        zig_row.set_wrap(true);
        let row_val: CRow = zig_row.to_bits();

        let mut wrap: bool = false;
        let mut written: usize = 99;

        // const keys = [_]RowData{ .wrap, .invalid };
        let keys: [c_int; 2] = [RowData::Wrap as c_int, RowData::Invalid as c_int];
        // var values = [_]?*anyopaque{ @ptrCast(&wrap), @ptrCast(&wrap) };
        let wrap_ptr = (&raw mut wrap).cast::<c_void>();
        let mut values = [wrap_ptr, wrap_ptr];
        // try testing.expectEqual(Result.invalid_value, get_multi(row_val, keys.len, &keys, &values, &written));
        assert_eq!(Result::InvalidValue, unsafe {
            get_multi(
                row_val,
                keys.len(),
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            )
        });
        // try testing.expectEqual(1, written);
        assert_eq!(1, written);
        // try testing.expect(wrap);
        assert!(wrap);
    }

    // test "get_multi null keys returns invalid_value"
    #[test]
    fn get_multi_null_keys_returns_invalid_value() {
        // const row_val: CRow = @bitCast(@as(u64, 0));
        let row_val: CRow = 0u64;
        let mut wrap: bool = false;
        // var values = [_]?*anyopaque{@ptrCast(&wrap)};
        let mut values = [&mut wrap as *mut bool as *mut c_void];
        // try testing.expectEqual(Result.invalid_value, get_multi(row_val, 1, null, &values, null));
        assert_eq!(Result::InvalidValue, unsafe {
            get_multi(
                row_val,
                1,
                std::ptr::null(),
                values.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        });
    }

    // W84-B01-01 (Rust-only proving test; the reference's guard is a
    // `runtime_safety` block, so it has no unit test for it): a `get_multi` KEY
    // ARRAY is C memory. Before the fix the wrapper loaded each element
    // straight into a `RowData`, which materializes an out-of-range
    // discriminant — UB in Rust in EVERY build mode, and the audited C probe
    // observed `success` + `out_written=2`. The reference reaches its
    // `std.enums.fromInt` guard per element (its `get_multi` calls `get`), so
    // the parity disposition is `.invalid_value` + the failing index.
    #[test]
    fn get_multi_out_of_range_raw_key_is_rejected_with_the_failing_index() {
        let mut zig_row = Row::from_bits(0u64);
        zig_row.set_wrap(true);
        zig_row.set_dirty(true);
        let row_val: CRow = zig_row.to_bits();

        let mut wrap: bool = false;
        let mut dirty: bool = false;
        let mut written: usize = 99;

        // Element 1 is a raw C integer no `RowData` tag can hold.
        let keys: [c_int; 3] = [RowData::Wrap as c_int, 999, RowData::Dirty as c_int];
        let mut values = [
            core::ptr::addr_of_mut!(wrap) as *mut c_void,
            core::ptr::addr_of_mut!(dirty) as *mut c_void,
            core::ptr::addr_of_mut!(dirty) as *mut c_void,
        ];
        assert_eq!(Result::InvalidValue, unsafe {
            get_multi(
                row_val,
                keys.len(),
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            )
        });
        // The failing index, exactly as the reference's `get_multi` reports.
        assert_eq!(1, written);
        // Element 0 ran to completion before the guard tripped.
        assert!(wrap);
        // Element 2 was never reached.
        assert!(!dirty);

        // The same array through the published `extern "C"` entry point.
        written = 99;
        assert_eq!(Result::InvalidValue.to_c_int(), unsafe {
            crate::rustty_row_get_multi(
                row_val,
                keys.len(),
                keys.as_ptr().cast(),
                values.as_mut_ptr(),
                &mut written,
            )
        });
        assert_eq!(1, written);
    }
}
