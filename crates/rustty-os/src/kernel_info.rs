//! rustty-os::kernel_info
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/kernel_info.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std` and `builtin`. In Rust we use the
// standard library for the `/proc` file read; no extra system binding is needed.

#[allow(non_snake_case)]
pub fn getKernelInfo() -> Option<Vec<u8>> {
    if cfg!(not(target_os = "linux")) {
        return None;
    }
    let path = "/proc/sys/kernel/osrelease";
    read_kernel_info(std::path::Path::new(path))
}

/// The body of `getKernelInfo` after the OS check, on a given path.
fn read_kernel_info(path: &std::path::Path) -> Option<Vec<u8>> {
    use std::io::Read as _;
    // var file = std.Io.Dir.openFileAbsolute(global.io(), path, .{}) catch return null;
    let mut file = std::fs::File::open(path).ok()?;

    // 128 bytes should be enough to hold the kernel information
    // var kernel_info_buf: [128]u8 = undefined;
    // const kernel_info = kernel_info_buf[0 .. file.readPositionalAll(
    //     global.io(),
    //     &kernel_info_buf,
    //     0,
    // ) catch return null];
    // PORT NOTE (parity S111-F7): `readPositionalAll` fills the buffer until it
    // is full or the file ends, so a longer file is TRUNCATED to 128 bytes, not
    // rejected. CORRECTION: this used to read the whole file and return null
    // above 128 bytes, after the older `readToEndAlloc(alloc, 128)` form.
    let mut kernel_info_buf = [0u8; 128];
    let mut len = 0;
    while len < kernel_info_buf.len() {
        match file.read(&mut kernel_info_buf[len..]) {
            Ok(0) => break,
            Ok(n) => len += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        }
    }
    let kernel_info = &kernel_info_buf[0..len];
    // return alloc.dupe(u8, std.mem.trim(u8, kernel_info, &std.ascii.whitespace)) catch return null;
    Some(trim_ascii_whitespace(kernel_info).to_vec())
}

// `std.mem.trim(u8, kernel_info, &std.ascii.whitespace)`: trims ASCII
// whitespace (space, tab, LF, VT, FF, CR) from both ends.
fn trim_ascii_whitespace(input: &[u8]) -> &[u8] {
    let is_ws = |c: u8| matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r');
    let mut start = 0;
    let mut end = input.len();
    while start < end && is_ws(input[start]) {
        start += 1;
    }
    while end > start && is_ws(input[end - 1]) {
        end -= 1;
    }
    &input[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore)] // reads /proc/sys/kernel/osrelease (real OS file)
    #[cfg_attr(not(target_os = "linux"), ignore = "reads /proc: Linux only (R14)")]
    fn read_proc_sys_kernel_osrelease() {
        let kernel_info = getKernelInfo().unwrap();

        // Since we can't hardcode the info in tests, just check
        // if something was read from the file
        assert!(!kernel_info.is_empty());
        assert!(kernel_info != b"");
    }

    /// Parity S111-F7: a file longer than the 128-byte buffer is truncated to
    /// its first 128 bytes (then trimmed), as `readPositionalAll` does.
    #[test]
    #[cfg_attr(miri, ignore)] // real file I/O
    fn kernel_info_longer_than_the_buffer_is_truncated_to_128_bytes() {
        let dir =
            std::env::temp_dir().join(format!("rustty-t-h-kernel-info-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("osrelease");
        let mut contents = b"  6.1.0-".to_vec();
        contents.extend(std::iter::repeat_n(b'a', 200));
        contents.push(b'\n');
        std::fs::write(&path, &contents).unwrap();
        let got = read_kernel_info(&path);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(got.as_deref(), Some(&contents[2..128]));
    }
}
