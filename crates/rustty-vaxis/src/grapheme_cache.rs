//! Port of `libvaxis` `GraphemeCache.zig` (20 LOC) — an 8 KiB ring buffer backing the
//! transient grapheme/`Key.text` byte-slices.
//!
//! PORT-NOTE: Zig `buf: [1024*8]u8 = undefined` → Rust `[0u8; 1024 * 8]`. Zig's
//! uninitialized read is only ever of bytes it just wrote, so zero-initialization is
//! behaviorally identical and Miri-safe (a benign divergence).

/// GraphemeCache — the ring buffer.
#[derive(Clone, Copy, Debug)]
pub struct GraphemeCache {
    /// the underlying storage for graphemes. Right now 8kb
    pub buf: [u8; 1024 * 8],

    // the start index of the next grapheme
    pub idx: usize,
}

impl Default for GraphemeCache {
    fn default() -> Self {
        GraphemeCache {
            buf: [0u8; 1024 * 8],
            idx: 0,
        }
    }
}

impl GraphemeCache {
    /// put a slice of bytes in the cache as a grapheme
    pub fn put(&mut self, bytes: &[u8]) -> &mut [u8] {
        // reset the idx to 0 if we would overflow
        if self.idx + bytes.len() > self.buf.len() {
            self.idx = 0;
        }
        // (Zig `defer self.idx += bytes.len` — we compute the slice range, copy, then
        // bump `idx` at the end.)
        let start = self.idx;
        let end = self.idx + bytes.len();
        // copy the grapheme to our storage
        self.buf[start..end].copy_from_slice(bytes);
        self.idx += bytes.len();
        // return the slice
        &mut self.buf[start..end]
    }
}
