//! Creates a temporary directory at runtime that can be safely used to
//! store temporary data and is destroyed on deinit.
//!
//! rustty-os::tempdir
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/TempDir.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `Dir = std.fs.Dir`, and
// `file = @import("file.zig")`. In Rust the directory "handles" are modeled as
// owned `PathBuf`s rooted at the absolute directory path (Rust's `std::fs` has
// no openable `Dir` handle type); the filesystem operations are reproduced over
// `std::fs` so behavior matches the reference exactly.

use crate::file;
use std::path::{Path, PathBuf};

const LOG_SCOPE: &str = "tempdir";

/// A temporary directory. See module docs.
pub struct TempDir {
    /// Dir is the directory handle
    dir: Dir,

    /// Parent directory
    parent: Dir,

    /// Name buffer that name points into. Generally do not use. To get the
    /// name call the name() function.
    name_buf: [u8; file::random_basename_len + 1],
}

impl TempDir {
    /// Create the temporary directory.
    pub fn init() -> std::io::Result<TempDir> {
        // Note: the tmp_path_buf sentinel is important because it ensures
        // we actually always have random_basename_len+1 bytes of available
        // space. We need that so we can set the sentinel in the case we use
        // all the possible length.
        let mut tmp_path_buf = [0u8; file::random_basename_len + 1];

        let dir = {
            let cwd = std_fs_cwd();
            // PORT NOTE (P3-W7 T3 / area-E `E15`):
            // `file.allocTmpDir(std.heap.page_allocator, global.environ())`.
            let tmp_dir = file::allocTmpDir(&rustty_global::environ());
            let r = cwd.openDirPath(Path::new(os_str_from_bytes(&tmp_dir)));
            file::freeTmpDir(&tmp_dir);
            r?
        };

        // We now loop forever until we can find a directory that we can create.
        loop {
            let tmp_path_len = {
                let tmp_path = file::randomBasename(&mut tmp_path_buf)
                    .map_err(|_| std::io::Error::from(std::io::ErrorKind::Other))?;
                tmp_path.len()
            };
            tmp_path_buf[tmp_path_len] = 0;
            let tmp_path = &tmp_path_buf[0..tmp_path_len];

            match dir.makeDir(tmp_path) {
                Ok(()) => {}
                Err(err) => match classify(&err) {
                    DirError::PathAlreadyExists => continue,
                    DirError::Other => return Err(err),
                },
            }

            return Ok(TempDir {
                dir: dir.openDir(tmp_path)?,
                parent: dir,
                name_buf: tmp_path_buf,
            });
        }
    }

    /// Name returns the name of the directory. This is just the basename
    /// and is not the full absolute path.
    pub fn name(&self) -> &[u8] {
        slice_to(&self.name_buf, 0)
    }

    /// Access the directory handle. In the reference `TempDir.dir` is a public
    /// `std.fs.Dir` field; callers (e.g. Kitty graphics image tests) use it to
    /// write/realpath/access files inside the temp directory.
    pub fn dir(&self) -> &Dir {
        &self.dir
    }

    /// Finish with the temporary directory. This deletes all contents in the
    /// directory.
    pub fn deinit(&mut self) {
        self.close(CloseMode::Delete);
    }

    /// Close the directory handles, optionally retaining the temporary directory
    /// and its contents on disk.
    //
    // pub fn close(self: *TempDir, mode: CloseMode) void {
    //     self.dir.close(global.io());
    //     switch (mode) {
    //         .delete => self.parent.deleteTree(global.io(), self.name()) catch |err|
    //             log.err("error deleting temp dir err={}", .{err}),
    //         .retain => {},
    //     }
    //     self.parent.close(global.io());
    // }
    pub fn close(&mut self, mode: CloseMode) {
        self.dir.close();
        match mode {
            CloseMode::Delete => {
                if let Err(err) = self.parent.deleteTree(self.name()) {
                    log_err(&format!("error deleting temp dir err={err}"));
                }
            }
            CloseMode::Retain => {}
        }
        self.parent.close();
    }
}

// pub const CloseMode = enum { delete, retain };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseMode {
    Delete,
    Retain,
}

// ---------------------------------------------------------------------------
// PORT NOTE: support shims for the `std.fs.Dir` operations used above.
// ---------------------------------------------------------------------------

/// `std.fs.Dir`: a directory handle, modeled here as its absolute path.
#[derive(Debug)]
pub struct Dir {
    path: PathBuf,
}

#[allow(non_snake_case)]
impl Dir {
    /// `std.fs.Dir.makeDir`: create a subdirectory by relative `sub_path`.
    fn makeDir(&self, sub_path: &[u8]) -> std::io::Result<()> {
        std::fs::create_dir(self.path.join(os_str_from_bytes(sub_path)))
    }

    /// `std.fs.Dir.openDir`: open a subdirectory by relative `sub_path`.
    fn openDir(&self, sub_path: &[u8]) -> std::io::Result<Dir> {
        reject_empty_sub_path(sub_path.is_empty())?;
        let path = self.path.join(os_str_from_bytes(sub_path));
        open_dir_abs(&path)
    }

    /// Open a subdirectory by absolute/relative `Path` (used for the root tmp
    /// dir which is an OS path string, not a basename slice).
    fn openDirPath(&self, sub_path: &Path) -> std::io::Result<Dir> {
        reject_empty_sub_path(sub_path.as_os_str().is_empty())?;
        open_dir_abs(&self.path.join(sub_path))
    }

    /// `std.fs.Dir.close`: release the handle (a no-op for our path model).
    fn close(&mut self) {}

    /// `std.Io.Dir.realPath`: resolve this directory handle's own canonical
    /// absolute path. The reference fills a caller-provided
    /// `[max_path_bytes]u8` buffer and returns the written length; the Rust
    /// port returns an owning `PathBuf`.
    //
    // (Windows: the DOS form, as `realpath` below; task W-2 routes both through
    // `crate::path::realpath`.)
    pub fn realPath(&self) -> std::io::Result<PathBuf> {
        crate::path::realpath(&self.path)
    }

    /// `std.Io.Dir.openDirAbsolute`: open a directory by absolute path.
    pub fn openDirAbsolute(path: &Path) -> std::io::Result<Dir> {
        open_dir_abs(path)
    }

    /// `std.Io.Dir.deleteDirAbsolute`: delete the (empty) directory at the
    /// absolute path.
    pub fn deleteDirAbsolute(path: &Path) -> std::io::Result<()> {
        std::fs::remove_dir(path)
    }

    /// `std.fs.Dir.deleteTree`: recursively delete `sub_path` and its contents.
    fn deleteTree(&self, sub_path: &[u8]) -> std::io::Result<()> {
        std::fs::remove_dir_all(self.path.join(os_str_from_bytes(sub_path)))
    }

    /// `std.fs.Dir.writeFile`: create (truncating) the file at `sub_path` within
    /// this directory and write `data` to it.
    pub fn writeFile(&self, sub_path: &str, data: &[u8]) -> std::io::Result<()> {
        std::fs::write(self.path.join(sub_path), data)
    }

    /// `std.fs.Dir.createFile`: create (or truncate) the file at `sub_path`
    /// within this directory, returning the open handle. Zig's `CreateFlags`
    /// default to `truncate = true`; `read` mirrors `.{ .read = true }` (the
    /// handle is opened read+write). Used by consumers that spawn processes
    /// with redirected stdio (e.g. the relocated Command.zig tests in
    /// rustty-termio).
    pub fn createFile(&self, sub_path: &str, read: bool) -> std::io::Result<std::fs::File> {
        std::fs::OpenOptions::new()
            .read(read)
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.path.join(sub_path))
    }

    /// `std.fs.Dir.realpath`: resolve `sub_path` (relative to this directory)
    /// to a canonical absolute path, returning the path bytes. The reference
    /// fills a caller-provided `[max_path_bytes]u8` buffer; the Rust port
    /// returns an owning `PathBuf` and the caller obtains the bytes from it.
    pub fn realpath(&self, sub_path: &str) -> std::io::Result<PathBuf> {
        // (task W-2: the canonicalize + DOS form below is now the shared
        // `crate::path::realpath`, which the other Windows realpath sites use.)
        // Windows: `canonicalize` returns a verbatim `\\?\C:\...` path, which
        // turns off separator normalization, so a `/` joined onto it later no
        // longer resolves. Zig's `realpath` returns the DOS form
        // (`GetFinalPathNameByHandle` with `.volume_name = .Dos`, which drops
        // the prefix): `C:\...`, and `\\server\share\...` for `\\?\UNC\...`.
        // Measured in task W-1: with the verbatim form every shell-integration
        // test that joins `/`-separated paths onto the resources dir failed.
        crate::path::realpath(&self.path.join(sub_path))
    }

    /// `std.fs.Dir.access`: check that `path` is accessible. `path` may be
    /// absolute (the reference passes the realpath result) or relative to this
    /// directory. Returns `FileNotFound` (`NotFound`) when it does not exist.
    pub fn access(&self, path: &Path) -> std::io::Result<()> {
        let full = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.path.join(path)
        };
        std::fs::metadata(full).map(|_| ())
    }
}

/// `openDir("")` fails: the reference opens `sub_path` with `openat(dirfd, "")`,
/// which the kernel answers with `ENOENT`, surfaced as `error.FileNotFound`.
/// MEASURED (zig 0.16.0, macOS, `std.Io.Dir.cwd().openDir(io, "", .{})`):
/// `error.FileNotFound`, while `"."` and `/tmp` open.
///
/// PORT NOTE (parity N172): the port models a `Dir` as a path, and
/// `PathBuf::join("")` is the directory itself, so without this check an empty
/// `sub_path` opened the parent: `TempDir.init` with `TMPDIR=` (present, empty)
/// created its directory under the process cwd where the reference fails
/// (reachable through `Surface.writeScreenFile`, `Surface.zig:5699`).
fn reject_empty_sub_path(empty: bool) -> std::io::Result<()> {
    if empty {
        return Err(std::io::Error::from(std::io::ErrorKind::NotFound));
    }
    Ok(())
}

fn open_dir_abs(path: &Path) -> std::io::Result<Dir> {
    // Verify the directory exists (mirrors `openDir` failing with FileNotFound
    // when the path does not exist).
    let md = std::fs::metadata(path)?;
    if !md.is_dir() {
        return Err(std::io::Error::from(std::io::ErrorKind::NotADirectory));
    }
    Ok(Dir {
        path: path.to_path_buf(),
    })
}

enum DirError {
    PathAlreadyExists,
    Other,
}

fn classify(err: &std::io::Error) -> DirError {
    if err.kind() == std::io::ErrorKind::AlreadyExists {
        DirError::PathAlreadyExists
    } else {
        DirError::Other
    }
}

// `std.fs.cwd()`: a `Dir` handle for the current working directory.
fn std_fs_cwd() -> Dir {
    Dir {
        path: PathBuf::from("."),
    }
}

// `std.mem.sliceTo(&self.name_buf, 0)`: slice up to (but not including) the
// first sentinel byte.
fn slice_to(buf: &[u8], sentinel: u8) -> &[u8] {
    let end = buf.iter().position(|&b| b == sentinel).unwrap_or(buf.len());
    &buf[0..end]
}

#[cfg(not(windows))]
fn os_str_from_bytes(b: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(b)
}

#[cfg(windows)]
fn os_str_from_bytes(b: &[u8]) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(std::str::from_utf8(b).unwrap_or(""))
}

fn log_err(msg: &str) {
    log::error!(target: LOG_SCOPE, "{msg}");
}

/// The DOS form of a verbatim path, as Zig's `realpath` returns it
/// (`GetFinalPathNameByHandle` with `.volume_name = .Dos`): `\\?\C:\..` becomes
/// `C:\..` and `\\?\UNC\server\..` becomes `\\server\..`. Any other verbatim
/// path (for example `\\?\Volume{GUID}\..`, a volume with no drive letter) has
/// no DOS form and is returned unchanged.
///
/// CORRECTION (task W-1b, review W-1 F5): W-1 stripped `\\?\` from every
/// verbatim path, which turned a `\\?\Volume{GUID}\..` result into a non-path.
#[cfg(windows)]
pub(crate) fn dos_form(canonical: PathBuf) -> PathBuf {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let wide: Vec<u16> = canonical.as_os_str().encode_wide().collect();
    let verbatim: Vec<u16> = r"\\?\".encode_utf16().collect();
    let unc: Vec<u16> = r"\\?\UNC\".encode_utf16().collect();
    if wide.starts_with(&unc) {
        let mut dos: Vec<u16> = r"\\".encode_utf16().collect();
        dos.extend_from_slice(&wide[unc.len()..]);
        return PathBuf::from(std::ffi::OsString::from_wide(&dos));
    }
    let rest = wide.get(verbatim.len()..).unwrap_or(&[]);
    let drive_follows = rest.len() >= 3
        && u8::try_from(rest[0]).is_ok_and(|c| c.is_ascii_alphabetic())
        && rest[1] == u16::from(b':')
        && rest[2] == u16::from(b'\\');
    if wide.starts_with(&verbatim) && drive_follows {
        PathBuf::from(std::ffi::OsString::from_wide(rest))
    } else {
        canonical
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// W-1b (review W-1 F5): only a drive-letter or UNC verbatim path has a DOS
    /// form; a volume GUID path is left as it is.
    #[cfg(windows)]
    #[test]
    fn dos_form_strips_only_drive_and_unc_prefixes() {
        let f = |p: &str| dos_form(PathBuf::from(p));
        assert_eq!(f(r"\\?\C:\Users\x"), PathBuf::from(r"C:\Users\x"));
        assert_eq!(f(r"\\?\UNC\srv\share\x"), PathBuf::from(r"\\srv\share\x"));
        let guid = r"\\?\Volume{0b5b1a3e-0000-0000-0000-100000000000}\x";
        assert_eq!(f(guid), PathBuf::from(guid));
        assert_eq!(f(r"C:\already\dos"), PathBuf::from(r"C:\already\dos"));
    }

    // PORT NOTE (1ffdd7415): upstream DELETED its flaky fd-BADF probes from
    // both lifecycle tests ("Don't check the raw handles after close:
    // descriptor numbers can be reused immediately, so a valid number does not
    // imply that we leaked it.") — rustty never ran them (its `Dir` models a
    // handle as its PATH; no fd is ever held), so the tests were already at
    // this end state. The on-disk assertions (deleted after `deinit`; retained
    // after `close(.retain)`) are carried in full; the BADF expects must NOT
    // be resurrected.

    // test {
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS makeDir/openDir/deleteTree syscalls
    fn tempdir_create_open_delete() {
        // var path_buf: [std.fs.max_path_bytes]u8 = undefined;
        // var path_len: usize = undefined;
        let path_buf: PathBuf;
        {
            // var td = try init();
            // errdefer td.deinit();
            //
            // (errdefer — handled by the explicit deinit below in the success
            // path; a panic before then leaves the dir, matching errdefer's
            // best-effort semantics on the abnormal path.)
            let mut td = TempDir::init().unwrap();

            // const nameval = td.name();
            // try testing.expect(nameval.len > 0);
            let nameval = td.name().to_vec();
            assert!(!nameval.is_empty());

            // Can open a new handle to it proves it exists.
            // var dir = try td.parent.openDir(testing.io, nameval, .{});
            // dir.close(testing.io);
            let mut dir = td.parent.openDir(&nameval).unwrap();
            dir.close();

            // path_len = try td.dir.realPath(testing.io, &path_buf);
            path_buf = td.dir.realPath().unwrap();

            // Don't check the raw handles after close: descriptor numbers can be
            // reused immediately, so a valid number does not imply that we leaked it.
            // td.deinit();
            td.deinit();
        }

        // try testing.expectError(
        //     error.FileNotFound,
        //     Dir.openDirAbsolute(testing.io, path_buf[0..path_len], .{}),
        // );
        assert_eq!(
            Dir::openDirAbsolute(&path_buf).unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
    }

    // test "close retains temporary directory" {
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS makeDir/openDir/deleteTree syscalls
    fn close_retains_temporary_directory() {
        // var path_buf: [std.fs.max_path_bytes]u8 = undefined;
        // var path_len: usize = undefined;
        let path_buf: PathBuf;
        {
            // var td = try init();
            // errdefer td.deinit();
            let mut td = TempDir::init().unwrap();

            // path_len = try td.dir.realPath(testing.io, &path_buf);
            path_buf = td.dir.realPath().unwrap();

            // Don't check the raw handles after close: descriptor numbers can be
            // reused immediately, so a valid number does not imply that we leaked it.
            // td.close(.retain);
            td.close(CloseMode::Retain);
        }
        // defer Dir.deleteDirAbsolute(testing.io, path_buf[0..path_len]) catch {};
        //
        // (deferred cleanup — performed at the end of this test; a panic before
        // then leaves the retained dir behind, matching the Zig defer's
        // best-effort `catch {}` semantics.)

        // var dir = try Dir.openDirAbsolute(testing.io, path_buf[0..path_len], .{});
        // dir.close(testing.io);
        let mut dir = Dir::openDirAbsolute(&path_buf).unwrap();
        dir.close();

        // (the deferred deleteDirAbsolute.)
        let _ = Dir::deleteDirAbsolute(&path_buf);
    }

    /// Parity N172: `openDir("")` fails with `FileNotFound` in the reference
    /// (`openat(dirfd, "")` is `ENOENT`), where `PathBuf::join("")` would name
    /// the directory itself. `"."` still opens, so the failure is the
    /// empty-path rule, not the directory.
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS stat
    fn open_dir_fails_with_not_found_on_an_empty_sub_path() {
        let cwd = std_fs_cwd();
        assert_eq!(
            cwd.openDir(b"").unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
        assert_eq!(
            cwd.openDirPath(Path::new("")).unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
        let tmp = Dir::openDirAbsolute(&std::env::temp_dir()).unwrap();
        assert_eq!(
            tmp.openDir(b"").unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
        cwd.openDir(b".").unwrap();
        cwd.openDirPath(Path::new(".")).unwrap();
    }

    /// Parity N172: with `TMPDIR` present but empty, the reference's
    /// `TempDir.init` fails with `FileNotFound` (`allocTmpDir` returns `""`,
    /// then `cwd.openDir("")`); the port used to create its directory under
    /// the process cwd. The parent re-runs this test in a child whose
    /// environment has `TMPDIR=` and whose cwd is a fresh empty directory, and
    /// asserts that the child ran the test, passed, and left the cwd empty.
    //
    // (Not Windows: there `allocTmpDir` is `GetTempPathW`, which does not read
    // `TMPDIR`; measured in task W-1, the child gets the user's temp dir.)
    #[cfg(not(windows))]
    #[test]
    #[cfg_attr(miri, ignore)] // spawns a child process
    fn init_fails_with_not_found_when_tmpdir_is_empty() {
        const CHILD: &str = "RUSTTY_N172_TEMPDIR_CHILD";
        const NAME: &str = "tempdir::tests::init_fails_with_not_found_when_tmpdir_is_empty";
        if std::env::var_os(CHILD).is_some() {
            // Child arm: the environment is captured from this process.
            assert_eq!(
                std::env::var_os("TMPDIR").as_deref(),
                Some(std::ffi::OsStr::new(""))
            );
            assert_eq!(file::allocTmpDir(&rustty_global::environ()), b"");
            match TempDir::init() {
                Ok(mut td) => {
                    let name = td.name().to_vec();
                    td.deinit();
                    panic!(
                        "TempDir::init succeeded with an empty TMPDIR (created {:?} under the cwd)",
                        String::from_utf8_lossy(&name)
                    );
                }
                Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::NotFound),
            }
            return;
        }
        let mut name_buf = [0u8; file::random_basename_len + 1];
        let Ok(base) = file::randomBasename(&mut name_buf).map(<[u8]>::to_vec) else {
            panic!("randomBasename failed");
        };
        let cwd =
            std::env::temp_dir().join(format!("rustty-n172-{}", String::from_utf8_lossy(&base)));
        std::fs::create_dir(&cwd).unwrap();
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .env("TMPDIR", "")
            .current_dir(&cwd)
            .output()
            .expect("spawn the child test");
        let left: Vec<_> = std::fs::read_dir(&cwd).unwrap().collect();
        let _ = std::fs::remove_dir_all(&cwd);
        assert!(out.status.success(), "child failed: {out:?}");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("test result: ok. 1 passed"),
            "the child did not run exactly one test: {stdout}"
        );
        assert!(left.is_empty(), "the child created {left:?} under its cwd");
    }
}
