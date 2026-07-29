//! Compression primitives used by the terminal.
//!
//! This namespace contains only the representation and codecs. Policy about
//! which pages to compress, when to decommit their resident memory, and when
//! to restore them belongs to `PageList`.
//!
//! PORTS: ghostty src/terminal/compress.zig (oracle origin/main)

/// The raw LZ4 block codec used for terminal page memory.
pub mod lz4;

// test { _ = @import("lz4_differential.zig"); } (lz4.zig's trailing test block)
#[cfg(test)]
mod lz4_differential;
