//! PORT NOTE: Ports ghostty `src/lib/types.zig`. Zig `extern struct`s become
//! `#[repr(C)]` structs preserving the exact field order and layout.

/// PORT NOTE: Zig's `String.init` accepts `anytype` and switches on `[]u8`
/// or `[]const u8`. In Rust we model both as a byte slice; the borrow is
/// captured via a raw pointer + length, matching the extern layout.
/// (e03475c0c added an `else => @compileError("invalid type")` arm to that
/// switch; Rust's static `&[u8]` parameter type rejects every other type at
/// compile time already, so the arm is satisfied by the type system.)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct String {
    pub ptr: *const u8,
    pub len: usize,
}

impl String {
    pub fn init(zig: &[u8]) -> String {
        String {
            // Borrowed strings keep a non-null pointer, but it must point
            // to real storage: empty Zig slices can use invalid sentinels
            // that foreign runtimes reject even without dereferencing.
            //
            // PORT NOTE (DECISIONS B1): Zig's `""` is `crate::EMPTY` (see its
            // doc). An empty Rust slice's `as_ptr()` is the dangling address
            // just as an empty Zig slice's is a sentinel.
            ptr: if zig.is_empty() {
                crate::EMPTY.as_ptr()
            } else {
                zig.as_ptr()
            },
            len: zig.len(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Buffer {
    pub ptr: *mut u8,
    pub cap: usize,
    pub len: usize,
}

impl Default for Buffer {
    fn default() -> Buffer {
        Buffer {
            ptr: std::ptr::null_mut(),
            cap: 0,
            len: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allocator::{GlobalZigAllocator, ZigAllocator, ZigSlice};

    /// `defer std.testing.allocator.free(empty);`
    struct FreeOnDrop(*mut u8);
    impl Drop for FreeOnDrop {
        fn drop(&mut self) {
            GlobalZigAllocator.free_bytes(ZigSlice::new(self.0, 0));
        }
    }

    // test "String.init empty output"
    #[test]
    fn string_init_empty_output() {
        // const empty = try std.testing.allocator.alloc(u8, 0);
        // defer std.testing.allocator.free(empty);
        // PORT NOTE: the typed zero-length `alloc` (`alloc_bytes`) returns
        // the allocator's zero-length sentinel without allocating, exactly as
        // Zig's `alloc(u8, 0)` does, and the typed `free` of it is a no-op.
        let empty_ptr = GlobalZigAllocator.alloc_bytes(0);
        let _free = FreeOnDrop(empty_ptr);
        // SAFETY: a zero-length slice over a non-null, byte-aligned pointer.
        let empty: &[u8] = unsafe { core::slice::from_raw_parts(empty_ptr, 0) };
        // const str = String.init(empty);
        let str = String::init(empty);
        // try std.testing.expectEqual(@as(usize, 0), str.len);
        assert_eq!(0usize, str.len);
        // Empty borrowed strings use the same real storage as an empty literal,
        // never Zig's zero-length allocation sentinel.
        // try std.testing.expectEqual(@as([*]const u8, ""), str.ptr);
        assert_eq!(crate::EMPTY.as_ptr(), str.ptr);
    }
}
