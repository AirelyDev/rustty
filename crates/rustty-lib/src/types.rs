//! PORT NOTE: Ports ghostty `src/lib/types.zig`. Zig `extern struct`s become
//! `#[repr(C)]` structs preserving the exact field order and layout.

/// PORT NOTE: Zig's `String.init` accepts `anytype` and switches on `[]u8`
/// or `[]const u8`. In Rust we model both as a byte slice; the borrow is
/// captured via a raw pointer + length, matching the extern layout.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct String {
    pub ptr: *const u8,
    pub len: usize,
}

impl String {
    pub fn init(zig: &[u8]) -> String {
        String {
            ptr: zig.as_ptr(),
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
