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
    use std::io::Read;

    // PORT NOTE: `var path_buf: [std.fs.max_path_bytes]u8 = undefined;` — we build the
    // path with `format!` into an owned `String` instead of a fixed stack buffer;
    // the resulting path bytes are identical.

    // Read our cgroup by opening /proc/<pid>/cgroup and reading the first
    // line. The first line will look something like this:
    // 0::/user.slice/user-1000.slice/session-1.scope
    // The cgroup path is the third field.
    let path = format!("/proc/{pid}/cgroup");
    let mut file = std::fs::File::open(&path).ok()?;

    // PORT NOTE: the reference reads at most `buf.len()` bytes into `buf` via a 64-byte
    // buffered reader (`readSliceShort`). `Read::read` performs a single short read
    // into `buf`; we read into `buf` directly to keep the bounded-read semantics.
    let len = file.read(buf).ok()?;
    let contents = &buf[0..len];

    // Find the last ':'
    let idx = contents.iter().rposition(|&b| b == b':')?;
    Some(trim_right(&contents[idx + 1..], b" \r\n"))
}

// PORT NOTE: `std.mem.trimRight(u8, slice, " \r\n")` — strip the given trailing bytes.
fn trim_right<'a>(slice: &'a [u8], strip: &[u8]) -> &'a [u8] {
    let mut end = slice.len();
    while end > 0 && strip.contains(&slice[end - 1]) {
        end -= 1;
    }
    &slice[0..end]
}
