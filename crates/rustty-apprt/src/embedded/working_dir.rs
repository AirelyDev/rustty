//! PORTS: ghostty src/apprt/embedded.zig — the working-directory helpers of
//! `Surface.init` (embedded.zig 529-553) and `newSurfaceOptions`
//! (embedded.zig 1167-1172), with their tests.

use super::*;

/// PORTS: `embedded.zig:529-553` — the requested-working-directory checks of
/// `Surface.init`: open the directory, stat the OPEN directory, and require a
/// directory. Returns `false` (the reference's `break :wd`) after logging the
/// failing step, `true` when `wd` may become the surface's working directory.
///
/// PORT NOTE (task tail-s1b): this used to be ONE `std::fs::metadata(wd)`, so
/// the reference's second diagnostic (`failed to stat requested working
/// directory`, `:540`) had no arm, and a path naming a regular FILE took the
/// `is not a directory` branch — where the reference's `openDirAbsolute` fails
/// at the open with `error.NotDir` and logs `error opening requested working
/// directory`. The open now carries `openDirAbsolute`'s flags (Zig 0.16
/// `dirOpenDirPosix`: `O_RDONLY | O_DIRECTORY | O_CLOEXEC`, plus `O_PATH`
/// where the platform has it, as `.iterate = false` requests), and the stat is
/// an `fstat` of that descriptor, so each step fails where the reference's
/// does. The `is not a directory` check is kept as the reference keeps it,
/// although after an `O_DIRECTORY` open only a race can reach it.
///
/// PORT NOTE (S46-F5): `{s}` on the reference's `wd` byte slice prints the
/// bytes verbatim; `wd` is a C string from the host and is not guaranteed
/// UTF-8, so it is rendered lossily here — the same choice `rustty-os`'s
/// byte-path diagnostics make.
pub(super) fn requested_working_directory_is_usable(wd: &[u8]) -> bool {
    let wd_display = String::from_utf8_lossy(wd);

    // var dir = std.Io.Dir.openDirAbsolute(global.io(), wd, .{}) catch |err| {
    let dir = match open_dir_absolute(wd) {
        Ok(dir) => dir,
        Err(err) => {
            // log.warn("error opening requested working directory dir={s} err={}", .{ wd, err });
            log::warn!(
                target: LOG_SCOPE,
                "error opening requested working directory dir={wd_display} err={err:?}",
            );
            // break :wd;
            return false;
        }
    };
    // defer dir.close(global.io());  (the `File` closes on drop)

    // const stat = dir.stat(global.io()) catch |err| {
    let stat = match dir.metadata() {
        Ok(md) => md,
        Err(err) => {
            // log.warn("failed to stat requested working directory dir={s} err={}", .{ wd, err });
            log::warn!(
                target: LOG_SCOPE,
                "failed to stat requested working directory dir={wd_display} err={err:?}",
            );
            // break :wd;
            return false;
        }
    };

    // if (stat.kind != .directory) {
    //     log.warn("requested working directory is not a directory dir={s}", .{wd});
    //     break :wd;
    // }
    if !stat.is_dir() {
        log::warn!(
            target: LOG_SCOPE,
            "requested working directory is not a directory dir={wd_display}",
        );
        return false;
    }
    true
}

/// `std.Io.Dir.openDirAbsolute(io, wd, .{})` for
/// [`requested_working_directory_is_usable`]: the POSIX arm (see its PORT NOTE).
#[cfg(unix)]
fn open_dir_absolute(wd: &[u8]) -> std::io::Result<std::fs::File> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;

    #[cfg(any(target_os = "linux", target_os = "android"))]
    let flags = libc::O_DIRECTORY | libc::O_PATH;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let flags = libc::O_DIRECTORY;

    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(flags)
        .open(std::ffi::OsStr::from_bytes(wd))
}

/// `std.Io.Dir.openDirAbsolute(io, wd, .{})`, the Windows arm (task W-2, N342).
/// Zig 0.16's `dirOpenDirWindows` (`std/Io/Threaded.zig:5356-5416`) converts the
/// WTF-8 path (`error.InvalidWtf8` when it is not) and opens it with one
/// `NtCreateFile(.., .{ .DIRECTORY_FILE = true, .OPEN_FOR_BACKUP_INTENT = true })`,
/// which fails with `NOT_A_DIRECTORY` (`error.NotDir`) on a regular file. Rust's
/// `CreateFileW` has no directory-only flag: the open takes
/// `FILE_FLAG_BACKUP_SEMANTICS` (`OPEN_FOR_BACKUP_INTENT`, needed to open a
/// directory at all), and a handle that is not a directory is closed and
/// reported as the open's `NotADirectory`, so each step still fails where the
/// reference's does.
#[cfg(windows)]
fn open_dir_absolute(wd: &[u8]) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    // winbase.h `FILE_FLAG_BACKUP_SEMANTICS`.
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    let path = rustty_os::path::os_string_from_bytes(wd)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "InvalidWtf8"))?;
    let dir = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    if !dir.metadata()?.is_dir() {
        return Err(std::io::ErrorKind::NotADirectory.into());
    }
    Ok(dir)
}

#[cfg(test)]
mod requested_working_directory_tests {
    use super::{requested_working_directory_is_usable, LOG_SCOPE};
    use crate::log_capture;

    /// A unique scratch directory, removed on drop.
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir()
                .join(format!("rustty-embedded-wd-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Scratch(p)
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn warns(cap: &log_capture::Capture) -> Vec<String> {
        cap.messages()
            .into_iter()
            .filter(|(level, target, _)| *level == log::Level::Warn && target == LOG_SCOPE)
            .map(|(_, _, msg)| msg)
            .collect()
    }

    /// A directory is usable and logs nothing.
    #[test]
    fn a_directory_is_usable_without_a_diagnostic() {
        let dir = Scratch::new("dir");
        let cap = log_capture::start();
        assert!(requested_working_directory_is_usable(
            dir.path().as_os_str().as_encoded_bytes()
        ));
        assert_eq!(warns(&cap), Vec::<String>::new());
    }

    /// A missing path fails at the OPEN (`embedded.zig:531`).
    #[test]
    fn a_missing_path_fails_at_the_open() {
        let dir = Scratch::new("missing");
        let missing = dir.path().join("nope");
        let cap = log_capture::start();
        assert!(!requested_working_directory_is_usable(
            missing.as_os_str().as_encoded_bytes()
        ));
        let got = warns(&cap);
        assert_eq!(got.len(), 1, "{got:?}");
        assert!(
            got[0].starts_with(&format!(
                "error opening requested working directory dir={} err=",
                missing.display()
            )),
            "{got:?}"
        );
    }

    /// A regular FILE fails at the open with ENOTDIR, as `openDirAbsolute`
    /// does — NOT at the `is not a directory` check, which is what the old
    /// single `std::fs::metadata` produced.
    #[test]
    fn a_regular_file_fails_at_the_open_not_the_kind_check() {
        let dir = Scratch::new("file");
        let file = dir.path().join("f");
        std::fs::write(&file, b"x").unwrap();
        let cap = log_capture::start();
        assert!(!requested_working_directory_is_usable(
            file.as_os_str().as_encoded_bytes()
        ));
        let got = warns(&cap);
        assert_eq!(got.len(), 1, "{got:?}");
        assert!(
            got[0].starts_with("error opening requested working directory dir=")
                && got[0].contains("NotADirectory"),
            "{got:?}"
        );
    }
}

/// PORT NOTE: no reference analogue — the reference's laziness is visible in
/// `App.keymap`'s type (`?input.Keymap`) and `keyboardLayout`'s `if (self.keymap
/// == null)` block, which Zig's own tests never exercise. These pin the
/// behaviour the port lost by initializing the keymap EAGERLY in `App::init`
/// (S46-F1).
/// `break :wd self.app.core_app.alloc.dupeZ(u8, cwd) catch null`
/// (`embedded.zig:1171`), coerced into the `?[*:0]const u8` the host reads.
///
/// PORT NOTE (task R13-P, N50): the pwd can hold an interior NUL — a program
/// that prints `OSC 7` with `%00` in the path plants one, and it reaches here
/// through `Surface.pwd`. The reference hands the host a pointer into the
/// `dupeZ` copy, so the host's C string is the bytes BEFORE that NUL (the macOS
/// app's `String(cString:)`, `SurfaceView.swift:606`); it then passes those
/// bytes back as `working_directory`, and the new surface starts in that
/// directory if it exists. This returned `null` instead, which dropped the
/// override entirely. Cut at the first NUL, so the host sees what it sees in
/// the reference.
pub(super) fn inherited_working_directory_c(cwd: Vec<u8>) -> Option<CString> {
    let end = cwd.iter().position(|&b| b == 0).unwrap_or(cwd.len());
    // R7: the bytes before the first NUL hold no NUL, so this is never `Err`.
    CString::new(&cwd[..end]).ok()
}

/// The inherited working directory [`Surface::newSurfaceOptions`] last handed
/// to the host, owned by the surface.
///
/// PORT NOTE: reference defect (R13), UD68. The reference returns
/// `alloc.dupeZ(u8, cwd)` (`embedded.zig:1167-1172`) through the C ABI's
/// `const char* working_directory` and never frees it; the host has no free
/// function to call, so the string leaks on every inherited tab, split and
/// window. Changing the C ABI (a free function, or an owned return) was ruled
/// out, so ownership stays here: each [`hand_out`](Self::hand_out) frees the
/// previous string, and [`clear`](Self::clear) (from `Surface::deinit`) frees
/// the last. The pointer stays valid until the next `newSurfaceOptions` on the
/// same surface or the surface's destruction — the host copies it at once
/// (`SurfaceView.swift:606`), or passes it straight to `rustty_surface_new`,
/// which reads it synchronously.
#[derive(Default)]
pub(super) struct InheritedWorkingDirectory(core::cell::Cell<Option<CString>>);

impl InheritedWorkingDirectory {
    /// Keep `cwd`, free the string handed out before it, and return the
    /// pointer the host reads (`null` for `None`).
    pub(super) fn hand_out(&self, cwd: Option<CString>) -> *const c_char {
        let ptr = cwd.as_ref().map_or(core::ptr::null(), |c| c.as_ptr());
        drop(self.0.replace(cwd));
        ptr
    }

    /// Free the last string handed out.
    pub(super) fn clear(&self) {
        drop(self.0.take());
    }
}

#[cfg(test)]
mod inherited_working_directory_tests {
    use super::*;

    fn take(c: Option<CString>) -> Option<Vec<u8>> {
        c.map(CString::into_bytes)
    }

    /// The OSC 7 `%00` pwd: the host gets the same C string the reference's
    /// `dupeZ` pointer gives it, not a dropped override.
    #[test]
    fn an_inherited_pwd_with_a_nul_reaches_the_host_cut_at_the_nul() {
        let got = take(inherited_working_directory_c(b"/tmp/a\x00b".to_vec()));
        assert_eq!(Some(b"/tmp/a".to_vec()), got);
    }

    #[test]
    fn an_ordinary_pwd_is_unchanged() {
        let got = take(inherited_working_directory_c(b"/tmp/a".to_vec()));
        assert_eq!(Some(b"/tmp/a".to_vec()), got);
    }

    /// A pwd that begins with the NUL gives the host an empty string, which its
    /// `wd.len > 0` check (`embedded.zig:527`) then skips.
    #[test]
    fn a_pwd_that_starts_with_a_nul_is_empty() {
        let got = take(inherited_working_directory_c(b"\x00b".to_vec()));
        assert_eq!(Some(Vec::new()), got);
    }

    // UD68 probe. A `#[global_allocator]` that forwards every call to `System`
    // and, on `dealloc`, notes whether the freed pointer is one this THREAD
    // asked it to watch. Thread-local, so frees on parallel test threads cannot
    // set it; and a watched string is live until freed, so its address cannot
    // be reused by a later allocation to fake the free.
    struct DeallocWatch;

    thread_local! {
        static WATCHED: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        static WATCHED_FREED: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
    }

    unsafe impl std::alloc::GlobalAlloc for DeallocWatch {
        unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
            unsafe { std::alloc::System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
            // `try_with`: never touch a thread-local that is being torn down.
            let _ = WATCHED.try_with(|w| {
                if w.get() != 0 && w.get() == ptr as usize {
                    let _ = WATCHED_FREED.try_with(|f| f.set(true));
                }
            });
            unsafe { std::alloc::System.dealloc(ptr, layout) }
        }
        unsafe fn realloc(
            &self,
            ptr: *mut u8,
            layout: std::alloc::Layout,
            new_size: usize,
        ) -> *mut u8 {
            unsafe { std::alloc::System.realloc(ptr, layout, new_size) }
        }
        unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
            unsafe { std::alloc::System.alloc_zeroed(layout) }
        }
    }

    // D6-ALLOW(watching shim, not an allocator substitution): this lives inside
    // `#[cfg(test)] mod inherited_working_directory_tests`, so it exists only in
    // this crate's test binary and can never enter a shipped build. Every arm
    // forwards to `std::alloc::System`, i.e. plain `malloc`/`free`, which the
    // ASan/LSan interceptors still see; `dealloc` only compares a pointer
    // against a thread-local first. Same shape as `rustty-font-shaper`'s
    // `face/freetype.rs` probe.
    #[global_allocator]
    static DEALLOC_WATCH: DeallocWatch = DeallocWatch;

    fn watch(p: *const c_char) {
        WATCHED_FREED.with(|f| f.set(false));
        WATCHED.with(|w| w.set(p as usize));
    }

    fn freed() -> bool {
        let f = WATCHED_FREED.with(|f| f.get());
        WATCHED.with(|w| w.set(0));
        f
    }

    /// UD68 (R13): the string handed to the host is freed by the next
    /// `newSurfaceOptions` on the surface (here the next `hand_out`) and by the
    /// surface's `deinit` (here `clear`), where the reference leaks every one.
    /// It is readable, unchanged, until then.
    #[test]
    fn the_inherited_working_directory_is_freed_by_the_next_call_and_by_deinit() {
        // The probe observes a free of the watched pointer, and only then.
        let sentinel = CString::new("sentinel").unwrap();
        watch(sentinel.as_ptr());
        assert!(!WATCHED_FREED.with(|f| f.get()));
        drop(sentinel);
        assert!(freed(), "the probe observes the free");

        let slot = InheritedWorkingDirectory::default();

        let first = slot.hand_out(inherited_working_directory_c(b"/tmp/first".to_vec()));
        assert!(!first.is_null());
        watch(first);
        // SAFETY: `first` is owned by `slot` until the next `hand_out`/`clear`.
        assert_eq!(b"/tmp/first", unsafe { CStr::from_ptr(first) }.to_bytes());
        assert!(
            !WATCHED_FREED.with(|f| f.get()),
            "the host's string is live until the next call"
        );

        let second = slot.hand_out(inherited_working_directory_c(b"/tmp/second".to_vec()));
        assert!(freed(), "the next call frees the previous string");
        // SAFETY: as above, for `second`.
        assert_eq!(b"/tmp/second", unsafe { CStr::from_ptr(second) }.to_bytes());

        // A call that inherits nothing still frees what was handed out before.
        watch(second);
        assert!(slot.hand_out(None).is_null());
        assert!(freed(), "a null hand-out frees the previous string");

        let third = slot.hand_out(inherited_working_directory_c(b"/tmp/third".to_vec()));
        watch(third);
        slot.clear();
        assert!(freed(), "deinit frees the last string");
    }
}
