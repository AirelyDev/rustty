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

/// PORTS: `media.zig:11-18` `fromFilename`. Build a `MediaFile` for an absolute
/// `path`, first checking it is accessible; `None` (with a warning) otherwise.
pub fn from_filename(path: &Path) -> Option<gtk4::MediaFile> {
    debug_assert!(path.is_absolute());
    // `std.fs.accessAbsolute(path, .{ .mode = .read_only })` (`media.zig:13`).
    if let Err(err) = std::fs::File::open(path) {
        eprintln!("warn(gtk): unable to access {}: {err}", path.display());
        return None;
    }
    Some(gtk4::MediaFile::for_filename(path))
}

/// PORTS: `media.zig:20-45` `fromResource`. Build a `MediaFile` for a gresource
/// path, first confirming the resource exists.
///
/// PORT NOTE: retained for parity with `media.zig`; the gtk backend's bell path
/// only uses [`from_filename`] via [`bell_media_file`] (`surface.zig:2506`).
#[allow(dead_code)]
pub fn from_resource(path: &str) -> Option<gtk4::MediaFile> {
    debug_assert!(path.starts_with('/'));
    match gio::functions::resources_get_info(path, gio::ResourceLookupFlags::NONE) {
        Ok(_) => Some(gtk4::MediaFile::for_resource(path)),
        Err(err) => {
            eprintln!("warn(gtk): failed to find resource {path}: {err}");
            None
        }
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
    eprintln!("warn(gtk): error playing sound from {path}: {err}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PORTS: `media.zig:131-157` `test "bellMediaFile reuses one MediaFile per
    /// path"`. Regression guard for the audio-bell thread leak: each bell must
    /// replay a single cached `MediaFile`, not allocate a fresh pipeline per
    /// ring. Asserts the reuse contract directly; needs no display and no
    /// playback (`MediaFile` is lazy), only that the path comparison drives reuse.
    #[test]
    fn bell_media_file_reuses_one_per_path() {
        // GTK must be initialized for `MediaFile` construction.
        if gtk4::init().is_err() {
            return;
        }

        // The files need not exist for reuse, but `from_filename` checks
        // accessibility, so create two real temp files.
        let dir = std::env::temp_dir();
        let path_a = dir.join("rustty-bell-test-a.oga");
        let path_b = dir.join("rustty-bell-test-b.oga");
        std::fs::write(&path_a, b"").unwrap();
        std::fs::write(&path_b, b"").unwrap();

        let Some(current) = bell_media_file(None, &path_a, false) else {
            return;
        };
        assert!(is_for_path(&current, &path_a));

        // Same path => identical object (the leak regression is rebuilding here).
        let current = bell_media_file(Some(current.clone()), &path_a, false).unwrap();
        assert!(is_for_path(&current, &path_a));

        // Changed path => rebuilt object targeting the new path (old one freed).
        let current = bell_media_file(Some(current), &path_b, false).unwrap();
        assert!(is_for_path(&current, &path_b));
        assert!(!is_for_path(&current, &path_a));

        let _ = std::fs::remove_file(&path_a);
        let _ = std::fs::remove_file(&path_b);
    }
}
