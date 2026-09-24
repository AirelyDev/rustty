//! rustty-os::cgroup
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/cgroup.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std` and uses `std.log.scoped(.@"linux-cgroup")`,
// `std.fs`, `std.fmt.bufPrint`, and `std.mem`. In Rust we use the standard library
// directly; this is a Linux-only helper (it reads `/proc/<pid>/cgroup`).

#[allow(dead_code)]
const LOG_SCOPE: &str = "linux-cgroup";

/// Returns the path to the cgroup for the given pid.
pub fn current(buf: &mut [u8], pid: u32) -> Option<&[u8]> {
    // PORT NOTE: `var path_buf: [std.fs.max_path_bytes]u8 = undefined;` — we build the
    // path with `format!` into an owned `String` instead of a fixed stack buffer;
    // the resulting path bytes are identical.

    // Read our cgroup by opening /proc/<pid>/cgroup and reading the first
    // line. The first line will look something like this:
    // 0::/user.slice/user-1000.slice/session-1.scope
    // The cgroup path is the third field.
    let path = format!("/proc/{pid}/cgroup");
    let mut file = std::fs::File::open(&path).ok()?;

    // var read_buf: [64]u8 = undefined;
    // var file_reader = file.reader(global.io(), &read_buf);
    // const reader = &file_reader.interface;
    // const len = reader.readSliceShort(buf) catch return null;
    let len = read_slice_short(&mut file, buf).ok()?;
    let contents = &buf[0..len];

    // Find the last ':'
    let idx = contents.iter().rposition(|&b| b == b':')?;
    Some(trim_right(&contents[idx + 1..], b" \r\n"))
}

/// `Io.Reader.readSliceShort`: fill `buf` until it is full or the stream
/// ends, returning the byte count; fewer than `buf.len()` bytes means end of
/// stream.
///
/// PORT NOTE (parity S111-F8): the 64-byte `read_buf` only buffers the
/// reader; the result is the same bytes. CORRECTION: this used to be a single
/// `Read::read`, and the note claimed the reference did "a single short read",
/// which `readSliceShort` does not: it returns short only at end of stream.
fn read_slice_short(reader: &mut impl std::io::Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut len = 0;
    while len < buf.len() {
        match reader.read(&mut buf[len..]) {
            Ok(0) => break,
            Ok(n) => len += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(len)
}

// PORT NOTE: `std.mem.trimRight(u8, slice, " \r\n")` — strip the given trailing bytes.
fn trim_right<'a>(slice: &'a [u8], strip: &[u8]) -> &'a [u8] {
    let mut end = slice.len();
    while end > 0 && strip.contains(&slice[end - 1]) {
        end -= 1;
    }
    &slice[0..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reader that hands out at most three bytes per `read`, as a pipe or a
    /// procfs file may.
    struct Trickle<'a>(&'a [u8]);

    impl std::io::Read for Trickle<'_> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let n = self.0.len().min(out.len()).min(3);
            out[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }

    /// Parity S111-F8: short reads before end of stream do not end the read;
    /// `readSliceShort` keeps filling the buffer, and stops short only at EOF.
    #[test]
    fn read_slice_short_fills_across_short_reads_until_eof() {
        let data = b"0::/user.slice/user-1000.slice/session-1.scope\n";
        let mut buf = [0u8; 128];
        let n = read_slice_short(&mut Trickle(data), &mut buf).unwrap();
        assert_eq!(&buf[..n], &data[..]);

        let mut small = [0u8; 10];
        let n = read_slice_short(&mut Trickle(data), &mut small).unwrap();
        assert_eq!(&small[..n], &data[..10]);
    }
}
