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
            let r = cwd.openDirPath(Path::new(&tmp_dir));
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
        let path = self.path.join(os_str_from_bytes(sub_path));
        open_dir_abs(&path)
    }

    /// Open a subdirectory by absolute/relative `Path` (used for the root tmp
    /// dir which is an OS path string, not a basename slice).
    fn openDirPath(&self, sub_path: &Path) -> std::io::Result<Dir> {
        open_dir_abs(&self.path.join(sub_path))
    }

    /// `std.fs.Dir.close`: release the handle (a no-op for our path model).
    fn close(&mut self) {}

    /// `std.Io.Dir.realPath`: resolve this directory handle's own canonical
    /// absolute path. The reference fills a caller-provided
    /// `[max_path_bytes]u8` buffer and returns the written length; the Rust
    /// port returns an owning `PathBuf`.
    pub fn realPath(&self) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(&self.path)
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
        std::fs::canonicalize(self.path.join(sub_path))
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
    eprintln!("[err] [{LOG_SCOPE}] {msg}");
}

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE (2b32b5b75): the reference's two lifecycle tests capture the
    // raw `Dir.Handle` fds and probe them with `fcntl(F.GETFD)`, expecting
    // `EBADF` after close — proof that BOTH the dir and parent descriptors were
    // released. rustty's `Dir` models a handle as its PATH (no fd is ever
    // held; see the module PORT NOTE), so the descriptor leak the upstream fix
    // closes CANNOT occur and the fd-BADF probes are N-A. The on-disk
    // assertions (deleted after `deinit`; retained after `close(.retain)`) are
    // carried in full.

    // test {
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS makeDir/openDir/deleteTree syscalls
    fn tempdir_create_open_delete() {
        // var path_buf: [std.fs.max_path_bytes]u8 = undefined;
        // var path_len: usize = undefined;
        // var dir_handle: Dir.Handle = undefined;
        // var parent_handle: Dir.Handle = undefined;
        //
        // (the handle captures are N-A — path model, see the module-test PORT
        // NOTE above.)
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
            // dir_handle = td.dir.handle;
            // parent_handle = td.parent.handle;
            path_buf = td.dir.realPath().unwrap();

            // Should be deleted after we deinit.
            // td.deinit();
            td.deinit();
        }

        // switch (builtin.os.tag) {
        //     .freebsd, .ios, .linux, .macos => {
        //         for ([_]Dir.Handle{ dir_handle, parent_handle }) |handle| {
        //             const result = std.posix.system.fcntl(handle, std.posix.F.GETFD);
        //             try testing.expectEqual(std.posix.E.BADF, std.posix.errno(result));
        //         }
        //     },
        //
        //     else => {},
        // }
        //
        // (N-A — no fds exist in the path model, see the module-test PORT NOTE.)

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
        // var dir_handle: Dir.Handle = undefined;
        // var parent_handle: Dir.Handle = undefined;
        //
        // (the handle captures are N-A — path model, see the module-test PORT
        // NOTE above.)
        let path_buf: PathBuf;
        {
            // var td = try init();
            // errdefer td.deinit();
            let mut td = TempDir::init().unwrap();

            // path_len = try td.dir.realPath(testing.io, &path_buf);
            // dir_handle = td.dir.handle;
            // parent_handle = td.parent.handle;
            path_buf = td.dir.realPath().unwrap();

            // td.close(.retain);
            td.close(CloseMode::Retain);
        }
        // defer Dir.deleteDirAbsolute(testing.io, path_buf[0..path_len]) catch {};
        //
        // (deferred cleanup — performed at the end of this test; a panic before
        // then leaves the retained dir behind, matching the Zig defer's
        // best-effort `catch {}` semantics.)

        // switch (builtin.os.tag) {
        //     .freebsd, .ios, .linux, .macos => {
        //         for ([_]Dir.Handle{ dir_handle, parent_handle }) |handle| {
        //             const result = std.posix.system.fcntl(handle, std.posix.F.GETFD);
        //             try testing.expectEqual(std.posix.E.BADF, std.posix.errno(result));
        //         }
        //     },
        //
        //     else => {},
        // }
        //
        // (N-A — no fds exist in the path model, see the module-test PORT NOTE.)

        // var dir = try Dir.openDirAbsolute(testing.io, path_buf[0..path_len], .{});
        // dir.close(testing.io);
        let mut dir = Dir::openDirAbsolute(&path_buf).unwrap();
        dir.close();

        // (the deferred deleteDirAbsolute.)
        let _ = Dir::deleteDirAbsolute(&path_buf);
    }
}
