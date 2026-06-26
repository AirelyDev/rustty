//! rustty-cfg-fmt::writer
//!
//! PORT STATUS: ported (glue)
//
// PORT NOTE: the reference's `formatEntry`/`EntryFormatter` write to a
// `*std.Io.Writer` via `writer.print(...)`. Following the established precedent
// (`rustty-os::string_encoding`), we model the writer as the `Writer` trait below
// — the `std.Io.Writer` interface's `writeAll` surface used by this module — and
// model the test-side `std.Io.Writer.Allocating` (whose `written()` returns the
// accumulated buffer) as `AllocatingWriter`.

use std::convert::Infallible;

/// PORT: `std.Io.Writer` — the subset (`writeAll`, used to back `print`) needed
/// by `formatEntry`, with the associated `Writer.Error`.
//
// PORT NOTE: the reference calls `writer.print("{s} = {}\n", ...)`. `print` is
// just formatted `writeAll`s; each format arm of `formatEntry` is reproduced by
// a sequence of `write_all` calls in `formatter.rs`, so this trait only needs
// `write_all`.
pub trait Writer {
    type Error;
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
}

/// PORT: `std.Io.Writer.Allocating` — an allocating writer accumulating into an
/// owned buffer; `written()` returns the accumulated bytes.
#[derive(Default)]
pub struct AllocatingWriter {
    buf: Vec<u8>,
}

impl AllocatingWriter {
    // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
    pub fn init() -> AllocatingWriter {
        AllocatingWriter { buf: Vec::new() }
    }

    // buf.written()
    pub fn written(&self) -> &[u8] {
        &self.buf
    }
}

impl Writer for AllocatingWriter {
    type Error = Infallible;
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Infallible> {
        self.buf.extend_from_slice(bytes);
        Ok(())
    }
}
