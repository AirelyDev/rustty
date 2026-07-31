#!/usr/bin/env python3
"""P3-W7 T3 (area-E `E15`) — thread the explicit environment map through
`rustty-os`'s homedir/path/file/xdg signatures, mirroring oracle `e8525c0fd` +
`f8e13f31e`. Recorded here rather than run from a scratch dir so the diff is
auditable."""
import sys

EDITS = {
    "crates/rustty-os/src/homedir.rs": [
        (
            """#[inline]
pub fn home(buf: &mut [u8]) -> Result<Option<&[u8]>, Error> {
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
    {
        homeUnix(buf)
    }

    #[cfg(windows)]
    {
        homeWindows(buf)
    }""",
            """//
// PORT NOTE (P3-W7 T3 / area-E `E15`): upstream gained an explicit
// `environ_map: *const std.process.Environ.Map` first parameter so `HOME` (and
// `HOMEDRIVE`/`HOMEPATH` on Windows) is read from a caller-supplied map rather
// than the process environment. See `desktop.rs` for why that matters more in
// Rust than in Zig.
#[inline]
pub fn home<'a>(
    environ_map: &rustty_global::EnvMap,
    buf: &'a mut [u8],
) -> Result<Option<&'a [u8]>, Error> {
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
    {
        homeUnix(environ_map, buf)
    }

    #[cfg(windows)]
    {
        homeWindows(environ_map, buf)
    }""",
        ),
        (
            """    #[cfg(target_os = "ios")]
    {
        let _ = buf;
        Ok(None)
    }""",
            """    #[cfg(target_os = "ios")]
    {
        let _ = (environ_map, buf);
        Ok(None)
    }""",
        ),
        (
            """fn homeUnix(buf: &mut [u8]) -> Result<Option<&[u8]>, Error> {
    // First: if we have a HOME env var, then we use that.
    if let Some(result) = posix_getenv("HOME") {
        if buf.len() < result.len() {
            return Err(Error::BufferTooSmall);
        }
        buf[0..result.len()].copy_from_slice(&result);""",
            """fn homeUnix<'a>(
    environ_map: &rustty_global::EnvMap,
    buf: &'a mut [u8],
) -> Result<Option<&'a [u8]>, Error> {
    // First: if we have a HOME env var, then we use that.
    if let Some(result) = environ_map.get(&b"HOME"[..]) {
        if buf.len() < result.len() {
            return Err(Error::BufferTooSmall);
        }
        buf[0..result.len()].copy_from_slice(result);""",
        ),
        (
            """fn homeWindows(buf: &mut [u8]) -> Result<Option<&[u8]>, Error> {""",
            """fn homeWindows<'a>(
    environ_map: &rustty_global::EnvMap,
    buf: &'a mut [u8],
) -> Result<Option<&'a [u8]>, Error> {""",
        ),
        (
            """        let drive = match fixed_buffer_env(buf, "HOMEDRIVE") {""",
            """        let drive = match fixed_buffer_env(environ_map, buf, b"HOMEDRIVE") {""",
        ),
        (
            """        let homepath = match fixed_buffer_env(&mut buf[drive_len..], "HOMEPATH") {""",
            """        let homepath = match fixed_buffer_env(environ_map, &mut buf[drive_len..], b"HOMEPATH") {""",
        ),
        (
            """fn fixed_buffer_env<'a>(dst: &'a mut [u8], key: &str) -> Result<Option<&'a [u8]>, ()> {
    let value = match std::env::var(key) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let bytes = value.as_bytes();""",
            """fn fixed_buffer_env<'a>(
    environ_map: &rustty_global::EnvMap,
    dst: &'a mut [u8],
    key: &[u8],
) -> Result<Option<&'a [u8]>, ()> {
    let bytes: &[u8] = match environ_map.get(key) {
        Some(v) => v,
        None => return Ok(None),
    };""",
        ),
        (
            """pub fn expandHome<'a>(path: &'a [u8], buf: &'a mut [u8]) -> Result<&'a [u8], ExpandError> {
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
    {
        expandHomeUnix(path, buf)
    }""",
            """pub fn expandHome<'a>(
    environ_map: &rustty_global::EnvMap,
    path: &'a [u8],
    buf: &'a mut [u8],
) -> Result<&'a [u8], ExpandError> {
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
    {
        expandHomeUnix(environ_map, path, buf)
    }""",
        ),
        (
            """    // `~/` is not an idiom generally used on Windows
    #[cfg(windows)]
    {
        let _ = buf;
        Ok(path)
    }

    // iOS doesn't have a user-writable home directory
    #[cfg(target_os = "ios")]
    {
        let _ = buf;
        Ok(path)
    }""",
            """    // `~/` is not an idiom generally used on Windows
    #[cfg(windows)]
    {
        let _ = (environ_map, buf);
        Ok(path)
    }

    // iOS doesn't have a user-writable home directory
    #[cfg(target_os = "ios")]
    {
        let _ = (environ_map, buf);
        Ok(path)
    }""",
        ),
        (
            """fn expandHomeUnix<'a>(path: &'a [u8], buf: &'a mut [u8]) -> Result<&'a [u8], ExpandError> {""",
            """fn expandHomeUnix<'a>(
    environ_map: &rustty_global::EnvMap,
    path: &'a [u8],
    buf: &'a mut [u8],
) -> Result<&'a [u8], ExpandError> {""",
        ),
        (
            """    let home_dir_len = match home(buf) {""",
            """    let home_dir_len = match home(environ_map, buf) {""",
        ),
        (
            """// `posix.getenv`: returns the value bytes (no allocation) or `None`.
#[cfg(not(windows))]
fn posix_getenv(key: &str) -> Option<Vec<u8>> {
    std::env::var_os(key).map(|v| {
        use std::os::unix::ffi::OsStrExt;
        v.as_os_str().as_bytes().to_vec()
    })
}
""",
            """// PORT NOTE (P3-W7 T3): the `posix_getenv` shim is GONE — `HOME` is read from
// the caller-supplied `EnvMap` now, matching upstream after `e8525c0fd`.
""",
        ),
        # tests
        (
            """        let _env = crate::env_test_lock();
        let mut buf = [0u8; libc::PATH_MAX as usize];
        let home_dir = expandHomeUnix(b"~/", &mut buf).unwrap().to_vec();""",
            """        // PORT NOTE (P3-W7 T3): upstream's test now builds the map from
        // `testing.environ`; rustty's equivalent is the captured process
        // environment, and the `crate::env_test_lock()` serialization is gone
        // because no test mutates the process environment any more.
        let environ_map = rustty_global::environ_map();
        let mut buf = [0u8; libc::PATH_MAX as usize];
        let home_dir = expandHomeUnix(&environ_map, b"~/", &mut buf)
            .unwrap()
            .to_vec();""",
        ),
        (
            """        let downloads = expandHomeUnix(b"~/Downloads/shader.glsl", &mut buf)
            .unwrap()
            .to_vec();""",
            """        let downloads = expandHomeUnix(&environ_map, b"~/Downloads/shader.glsl", &mut buf)
            .unwrap()
            .to_vec();""",
        ),
        (
            """        assert_eq!(b"~".as_slice(), expandHomeUnix(b"~", &mut buf).unwrap());
        assert_eq!(
            b"~abc/".as_slice(),
            expandHomeUnix(b"~abc/", &mut buf).unwrap()
        );
        assert_eq!(
            b"/home/user".as_slice(),
            expandHomeUnix(b"/home/user", &mut buf).unwrap()
        );
        assert_eq!(b"".as_slice(), expandHomeUnix(b"", &mut buf).unwrap());""",
            """        assert_eq!(
            b"~".as_slice(),
            expandHomeUnix(&environ_map, b"~", &mut buf).unwrap()
        );
        assert_eq!(
            b"~abc/".as_slice(),
            expandHomeUnix(&environ_map, b"~abc/", &mut buf).unwrap()
        );
        assert_eq!(
            b"/home/user".as_slice(),
            expandHomeUnix(&environ_map, b"/home/user", &mut buf).unwrap()
        );
        assert_eq!(
            b"".as_slice(),
            expandHomeUnix(&environ_map, b"", &mut buf).unwrap()
        );""",
        ),
        (
            """            expandHomeUnix(b"~/Downloads", &mut small_buf[..])""",
            """            expandHomeUnix(&environ_map, b"~/Downloads", &mut small_buf[..])""",
        ),
        (
            """        let _env = crate::env_test_lock();
        let mut buf = [0u8; 1024];
        let result = home(&mut buf).unwrap();""",
            """        let environ_map = rustty_global::environ_map();
        let mut buf = [0u8; 1024];
        let result = home(&environ_map, &mut buf).unwrap();""",
        ),
    ],
}


def main() -> int:
    for path, edits in EDITS.items():
        src = open(path, encoding="utf-8").read()
        for old, new in edits:
            if old not in src:
                print(f"MISS in {path}: {old.splitlines()[0][:70]}", file=sys.stderr)
                return 1
            src = src.replace(old, new, 1)
        open(path, "w", encoding="utf-8").write(src)
        print("patched", path)
    return 0


sys.exit(main())
