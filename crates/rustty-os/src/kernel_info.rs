//! rustty-os::kernel_info
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/kernel_info.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT: the Zig reference imports `std` and `builtin`. In Rust we use the
// standard library for the `/proc` file read; no extra system binding is needed.

#[allow(non_snake_case)]
pub fn getKernelInfo() -> Option<Vec<u8>> {
    if cfg!(not(target_os = "linux")) {
        return None;
    }
    let path = "/proc/sys/kernel/osrelease";
    let kernel_info = match std::fs::read(path) {
        Ok(v) => v,
        Err(_) => return None,
    };

    // 128 bytes should be enough to hold the kernel information
    // PORT: `file.readToEndAlloc(alloc, 128)` caps the read at 128 bytes and
    // returns an error (→ null) if the file is larger; we reproduce the cap by
    // returning null when more than 128 bytes are present.
    if kernel_info.len() > 128 {
        return None;
    }
    Some(trim_ascii_whitespace(&kernel_info).to_vec())
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
    fn read_proc_sys_kernel_osrelease() {
        if cfg!(not(target_os = "linux")) {
            return; // reference returns null (test no-op on non-Linux)
        }

        let kernel_info = getKernelInfo().unwrap();

        // Since we can't hardcode the info in tests, just check
        // if something was read from the file
        assert!(!kernel_info.is_empty());
        assert!(kernel_info != b"");
    }
}
