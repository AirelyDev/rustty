//! rustty-apprt::gtk::media
//!
//! PORTS: ghostty `src/apprt/gtk/media.zig` — the audio-bell `gtk.MediaFile`
//! helpers.
//!
//! A get-or-create cache for the per-surface bell `MediaFile` plus the play +
//! error-logging plumbing. Reusing one `MediaFile` per surface (rebuilt only
//! when the configured path changes) is what prevents the GStreamer pipeline +
//! `gstglcontext`/`gldisplay-event` thread leak the reference documents
//! (`media.zig:47-90`): `MediaFile::for_filename` spins up a full pipeline that
//! is never torn down on the happy path, so allocating one per bell leaked a
//! pipeline per ring.

use std::path::Path;

use gtk4::prelude::*;

// const log = std.log.scoped(.gtk_media);
const LOG_SCOPE: &str = "gtk_media";

/// PORTS: `media.zig:11-18` `fromFilename`. Build a `MediaFile` for an absolute
/// `path`, first checking it is accessible; `None` (with a warning) otherwise.
pub fn from_filename(path: &Path) -> Option<gtk4::MediaFile> {
    debug_assert!(path.is_absolute());
    // `std.fs.accessAbsolute(path, .{ .mode = .read_only })` (`media.zig:13`).
    if let Err(err) = std::fs::File::open(path) {
        // log.warn("unable to access {s}: {t}", .{ path, err });
        log::warn!(target: LOG_SCOPE, "unable to access {}: {err}", path.display());
        return None;
    }
    Some(gtk4::MediaFile::for_filename(path))
}

/// PORTS: `media.zig:20-45` `fromResource`. Build a `MediaFile` for a gresource
/// path, first confirming the resource exists.
///
/// PORT NOTE: retained for parity with `media.zig`; the gtk backend's bell path
/// only uses [`from_filename`] via [`bell_media_file`] (`surface.zig:2506`).
///
/// CORRECTION (task T-D, S48-F5): this went through gio-rs's
/// `resources_get_info`, which folds the reference's two failure arms into one
/// `Err`, and logged only the GError's message. The reference calls
/// `g_resources_get_info` itself and logs the quark domain, the code and the
/// message when a GError is set, and a separate message-less warning when the
/// call returns FALSE without one; both arms are restored through the C call.
#[allow(dead_code)]
pub fn from_resource(path: &str) -> Option<gtk4::MediaFile> {
    debug_assert!(path.starts_with('/'));
    let c_path = std::ffi::CString::new(path).ok()?;
    // var gerr: ?*glib.Error = null;
    // const found = gio.resourcesGetInfo(path, .{}, null, null, &gerr);
    let mut gerr: *mut glib::ffi::GError = std::ptr::null_mut();
    // SAFETY: `c_path` is NUL-terminated and outlives the call; the size and flags
    // out-pointers may be null (`gio/gresource.c`), and `gerr` is a valid
    // out-pointer to a null `GError*`.
    let found = unsafe {
        gio::ffi::g_resources_get_info(
            c_path.as_ptr(),
            gio::ffi::G_RESOURCE_LOOKUP_FLAGS_NONE,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut gerr,
        )
    };
    // if (gerr) |err| { defer err.free(); log.warn(...); return null; }
    if !gerr.is_null() {
        // SAFETY: GLib set `gerr` to a GError we now own; `from_glib_full` frees it.
        let err: glib::Error = unsafe { glib::translate::from_glib_full(gerr) };
        let (domain, code, message) = gerror_parts(&err, "(no message)");
        // log.warn("failed to find resource {s}: {s} {d} {s}", .{ path,
        //     glib.quarkToString(err.f_domain), err.f_code,
        //     err.f_message orelse "(no message)" });
        log::warn!(target: LOG_SCOPE, "failed to find resource {path}: {domain} {code} {message}");
        return None;
    }
    // if (found == 0) { log.warn("failed to find resource {s}", .{path}); return null; }
    if found == glib::ffi::GFALSE {
        log::warn!(target: LOG_SCOPE, "failed to find resource {path}");
        return None;
    }
    Some(gtk4::MediaFile::for_resource(path))
}

/// `glib.quarkToString(err.f_domain)`, `err.f_code` and `err.f_message orelse
/// <fallback>` — the three GError fields the reference's warnings print, read from
/// the `GError` itself (glib-rs's `Display` prints only the message).
fn gerror_parts(err: &glib::Error, no_message: &str) -> (String, i32, String) {
    use glib::translate::ToGlibPtr;
    let raw: *const glib::ffi::GError = err.to_glib_none().0;
    // SAFETY: `raw` points at the live GError `err` owns for this call.
    unsafe {
        let domain = glib::ffi::g_quark_to_string((*raw).domain);
        let domain = if domain.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(domain)
                .to_string_lossy()
                .into_owned()
        };
        let message = if (*raw).message.is_null() {
            no_message.to_owned()
        } else {
            std::ffi::CStr::from_ptr((*raw).message)
                .to_string_lossy()
                .into_owned()
        };
        (domain, (*raw).code, message)
    }
}

/// PORTS: `media.zig:59-90` `bellMediaFile`. Get-or-create the reusable bell
/// `MediaFile` targeting `path`.
///
/// `current` is the surface's currently-cached `MediaFile` (or `None`). If it
/// already targets `path` it is returned unchanged; otherwise it is dropped
/// (unref'd) and a fresh one is built for `path`. Returns `None` (after freeing
/// `current`) if `path` is inaccessible, leaving the caller's slot empty.
pub fn bell_media_file(
    current: Option<gtk4::MediaFile>,
    path: &Path,
    required: bool,
) -> Option<gtk4::MediaFile> {
    if let Some(media_file) = current {
        if is_for_path(&media_file, path) {
            return Some(media_file);
        }
        // `media_file.unref()` (`media.zig:66`) — the owned handle drops here.
        drop(media_file);
    }

    let media_file = from_filename(path)?;

    // If the audio file is marked as required, emit an error if playback fails;
    // otherwise there is silence. Connected once, here, because the `MediaFile`
    // is reused across bells (`media.zig:79-87`).
    //
    // NOTE: we intentionally do NOT connect notify::ended to unref. The
    // `MediaFile` is owned by the surface and replayed via `seek(0)` for every
    // bell; unref'ing on `ended` is precisely what previously leaked a pipeline
    // per ring.
    if required {
        media_file.connect_error_notify(media_file_error);
    }

    Some(media_file)
}

/// PORTS: `media.zig:92-100` `playBell`. (Re)play `media_file` at `volume`.
/// `seek(0)` rewinds first so a previously-ended stream plays again (#8957);
/// safe on a freshly-created stream too.
pub fn play_bell(media_file: &gtk4::MediaFile, volume: f64) {
    media_file.set_volume(volume);
    media_file.seek(0);
    media_file.play();
}

/// PORTS: `media.zig:102-108` `isForPath`. Whether `media_file` was created for
/// `path`.
fn is_for_path(media_file: &gtk4::MediaFile, path: &Path) -> bool {
    let Some(file) = media_file.file() else {
        return false;
    };
    let Some(cur) = file.path() else {
        return false;
    };
    cur == path
}

/// PORTS: `media.zig:110-129` `mediaFileError`. The notify::error handler for a
/// `required` bell sound: log the playback error (path + GError detail).
fn media_file_error(media_file: &gtk4::MediaFile) {
    let path = media_file.file().and_then(|f| f.path());
    let Some(err) = media_file.error() else {
        return;
    };
    let path = path
        .as_deref()
        .map_or_else(|| "<<unknown>>".to_string(), |p| p.display().to_string());
    // CORRECTION (task T-D, S48-F5): this printed `{err}` (the message only);
    // the reference prints the quark domain and code too.
    let (domain, code, message) = gerror_parts(&err, "");
    // log.warn("error playing sound from {s}: {s} {d} {s}", .{
    //     path orelse "<<unknown>>", glib.quarkToString(err.f_domain), err.f_code,
    //     err.f_message orelse "" });
    log::warn!(target: LOG_SCOPE, "error playing sound from {path}: {domain} {code} {message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// S48-F5: `fromResource`'s GError arm prints the quark domain, the code and
    /// the message under the file's scope (`media.zig:29-41`). Display-free: a
    /// missing resource fails before any `MediaFile` exists. The domain/code are
    /// GLib's own `G_RESOURCE_ERROR_NOT_FOUND` (code 0 of
    /// `g-resource-error-quark`), read back from GLib rather than assumed.
    ///
    /// The reference's second arm (`found == 0` with no GError, `:43-46`) is
    /// ported but not driven here: `g_resources_get_info` sets a GError on every
    /// FALSE return in GLib 2.88, so no input reaches it.
    #[test]
    fn from_resource_missing_logs_domain_code_and_message() {
        let cap = crate::log_capture::start();
        let got = from_resource("/io/airely/rustty/no/such/resource.oga");
        let messages = cap.messages();
        drop(cap);
        assert!(got.is_none(), "a missing resource yields no MediaFile");

        use glib::error::ErrorDomain;
        let domain = glib::Quark::from_str("g-resource-error-quark");
        assert_eq!(gio::ResourceError::domain(), domain);
        let not_found = gio::ResourceError::NotFound.code();
        assert_eq!(messages.len(), 1, "exactly one warning; got {messages:?}");
        let (level, target, msg) = &messages[0];
        assert_eq!(*level, log::Level::Warn);
        assert_eq!(target, "gtk_media");
        let prefix = format!(
            "failed to find resource /io/airely/rustty/no/such/resource.oga: \
             g-resource-error-quark {not_found} "
        );
        assert!(
            msg.starts_with(&prefix) && msg.len() > prefix.len(),
            "domain, code and a non-empty message; got {msg:?}"
        );
    }

    /// S48-F5: `mediaFileError` prints the domain and the code too
    /// (`media.zig:110-133`). Sets a GError on a `required` bell's stream, which
    /// fires the `notify::error` handler `bell_media_file` connected.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib media_file_error -- --ignored"]
    fn media_file_error_logs_domain_code_and_message() {
        crate::gtk::init_for_display_test("media_file_error_logs_domain_code_and_message");
        let path = std::env::temp_dir().join("rustty-bell-test-error.oga");
        std::fs::write(&path, b"").unwrap();
        let Some(media) = bell_media_file(None, &path, true) else {
            panic!("bell_media_file returned None for an accessible file");
        };

        let cap = crate::log_capture::start();
        media.set_error(glib::Error::new(glib::FileError::Noent, "boom"));
        let messages = cap.messages();
        drop(cap);

        use glib::error::ErrorDomain;
        let code = glib::FileError::Noent.code();
        assert_eq!(
            messages,
            [(
                log::Level::Warn,
                "gtk_media".to_owned(),
                format!(
                    "error playing sound from {}: g-file-error-quark {code} boom",
                    path.display()
                ),
            )]
        );
        // Leaked for the GstPlay teardown race documented in
        // `bell_media_file_reuses_one_per_path`.
        std::mem::forget(media);
        let _ = std::fs::remove_file(&path);
    }

    /// PORTS: `media.zig:131-157` `test "bellMediaFile reuses one MediaFile per
    /// path"`. Regression guard for the audio-bell thread leak: each bell must
    /// replay a single cached `MediaFile`, not allocate a fresh pipeline per
    /// ring. Asserts the reuse contract directly; needs no playback (`MediaFile`
    /// is lazy), only that the path comparison drives reuse. It DOES need a
    /// display, and is `#[ignore]`d for it (see the FIX below).
    ///
    /// CORRECTION (task 20): this doc said the test "needs no display and no
    /// playback", which task 19's fix round made false.
    #[test]
    #[ignore = "requires a display; run via GDK_BACKEND=broadway (with broadwayd) or xvfb-run -a cargo test -p rustty-apprt --features gtk,x11,wayland bell_media_file -- --ignored"]
    fn bell_media_file_reuses_one_per_path() {
        // GTK must be initialized for `MediaFile` construction.
        //
        // FIX (task 19 fix round 1): this was `if gtk4::init().is_err() {
        // return; }`, the quiet guard task-14d-3 moved nine tests off. On the
        // headless Linux lane `gtk4::init()` fails, so the test asserted
        // NOTHING and reported `1 passed` — including with the same-path reuse
        // branch disabled (reviewer-measured). The S48-F1 identity assertion
        // below was only ever proven under `GDK_BACKEND=broadway`. A missing
        // display is now a loud FAILURE of an explicitly requested test.
        //
        // PORT NOTE: the reference says this "needs no display"; that is true of
        // Zig's GObject bindings, not of gtk4-rs, whose `MediaFile`
        // constructors assert `gtk4::init()` has succeeded.
        crate::gtk::init_for_display_test("bell_media_file_reuses_one_per_path");

        // The files need not exist for reuse, but `from_filename` checks
        // accessibility, so create two real temp files.
        let dir = std::env::temp_dir();
        let path_a = dir.join("rustty-bell-test-a.oga");
        let path_b = dir.join("rustty-bell-test-b.oga");
        std::fs::write(&path_a, b"").unwrap();
        std::fs::write(&path_b, b"").unwrap();

        // var current = bellMediaFile(null, path_a, false) orelse return error.SkipZigTest;
        //
        // PORT NOTE: Rust has no SkipZigTest, and a bare `return` would report
        // `1 passed` for a run that asserted nothing — the same vacuity as the
        // guard above. With a display bound and both files created above, a
        // `None` here is a broken environment for an explicitly requested test.
        let Some(current) = bell_media_file(None, &path_a, false) else {
            panic!("bell_media_file(None, path_a) returned None: MediaFile construction failed, so the reuse contract was not tested");
        };
        // const first = current;
        let first = current.clone();
        assert!(is_for_path(&current, &path_a));

        // Same path => identical object (the leak regression is rebuilding here).
        let current = bell_media_file(Some(current), &path_a, false).unwrap();
        // try testing.expectEqual(first, current);
        //
        // FIX (S48-F1): this asserted `is_for_path(&current, &path_a)` — path
        // EQUALITY, which a freshly rebuilt `MediaFile` for the same path also
        // satisfies, so the test could not catch the regression it documents.
        // The reference asserts object IDENTITY against the handle captured
        // before the call. glib wrapper types compare by underlying pointer, so
        // `assert_eq!` is that same claim.
        assert_eq!(first, current);

        // Changed path => rebuilt object targeting the new path (old one freed).
        let current = bell_media_file(Some(current), &path_b, false).unwrap();
        assert!(is_for_path(&current, &path_b));
        assert!(!is_for_path(&current, &path_a));

        // Deliberately leak the last handle instead of unref'ing it here (the
        // oracle's trailing `current.unref()`, `media.zig:157`).
        //
        // WHY: `GtkMediaFile` is backed by `GstPlay`, which runs its own
        // `gst_play_main` thread. Dropping the LAST reference within a few
        // microseconds of construction races that thread's startup: the play
        // thread is still inside `gst_play_set_uri_internal` /
        // `on_error` -> `api_bus_post_message`, and the bus message it posts
        // ends up holding the final `GstPlay` reference, so `gst_play_finalize`
        // runs ON the play thread and calls `g_mutex_clear()` on a mutex that
        // thread still holds. Measured on Fedora 44 / GStreamer 1.28.1: the
        // unref'ing version of this test aborted (`g_mutex_clear() called on
        // uninitialised or locked mutex`) or segfaulted
        // (`gst_object_ref` on a freed object) in ~30% of runs, with and
        // without a display, which made `cargo test --workspace` red at random.
        // Calling `clear()` first only narrows the window (~3%); nothing in the
        // GTK API closes it. This is an upstream `GstPlay` teardown bug, not a
        // property of `bell_media_file`.
        //
        // Leaking is sound HERE and nowhere else: it is one `GstPlay` for the
        // remaining lifetime of a test process that is about to exit, and it
        // costs the test no coverage — every assertion above is about the reuse
        // contract (`is_for_path` driving rebuild), never about the final
        // teardown. `bell_media_file`'s own `drop` of a replaced handle is still
        // exercised, on the line above.
        std::mem::forget(current);
        // `first` aliases the SAME object, and `bell_media_file` already
        // dropped its own handle to it when the path changed — so dropping
        // `first` here would be the last unref, i.e. exactly the `GstPlay`
        // teardown race described above. Leak it for the same reason.
        std::mem::forget(first);

        let _ = std::fs::remove_file(&path_a);
        let _ = std::fs::remove_file(&path_b);
    }
}
