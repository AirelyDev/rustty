//! A DMABUF frame produced by exporting a GPU texture.
//! This may be used on apprts like GTK that require us to manually
//! export each frame and import them as textures in their UI scene graphs.
//!
//! Note that DMABUFs are independent of the graphics API used:
//! the OpenGL renderer allocates them with GBM, and Vulkan can
//! export them with a KHR external memory implementation. Therefore
//! this struct has to be placed parallel to the renderer
//! implementations.
//!
//! PORTS: ghostty `src/renderer/Dmabuf.zig` (75 loc, new at `12752b2ac`), P7-W7 T1.
//!
//! PORT NOTE (ownership): the reference keeps each plane as a raw `fd_t` (`-1` for
//! none) and makes the owner call `deinit` to close them. Here a validated plane
//! is an [`OwnedFd`], so dropping a [`Dmabuf`] or [`Planes`] closes its fds and
//! handing one to an apprt that consumes it is `OwnedFd::into_raw_fd`. The raw,
//! driver-filled arrays exist only in [`RawPlanes`], between
//! `eglExportDMABUFImageMESA` and [`RawPlanes::validate`], which is the one
//! place a raw fd becomes owned.
//!
//! PORT NOTE (Unix-only): a DMA-BUF is a Linux fd; the module is built on every
//! Unix host (so its tests also run on macOS) and not on Windows, which builds the
//! OpenGL renderer but never exports a frame.

use std::ffi::c_int;
use std::os::fd::{FromRawFd, OwnedFd, RawFd};

/// The maximum number of planes in a DMABUF that we support.
/// This matches the maximum found in apprts such as GTK.
//
// pub const max_planes = 4;
pub const MAX_PLANES: usize = 4;

/// PORTS: `Dmabuf.zig:10-37` — `pub const Dmabuf = @This();` and its fields.
#[derive(Debug)]
pub struct Dmabuf {
    /// Width of the texture in device pixels.
    pub width: u32,

    /// Height of the texture in device pixels.
    pub height: u32,

    /// DRM fourcc of the pixel format.
    /// Rustty renders RGBA/BGRA8 premultiplied.
    pub fourcc: u32,

    /// DRM modifier of the format.
    pub modifier: u64,

    /// Whether the data is premultiplied.
    /// Rustty's GL renderers output premultiplied alpha.
    pub premultiplied: bool,

    /// The DMABUF planes for a presented frame. The DMABUF owns the fds
    /// and must either call `deinit` manually, or pass them to an apprt
    /// that consumes them.
    pub planes: Planes,
}

impl Dmabuf {
    /// PORTS: `Dmabuf.zig:73-75`.
    ///
    /// ```zig
    /// pub fn deinit(self: Dmabuf) void {
    ///     self.planes.deinit();
    /// }
    /// ```
    ///
    /// PORT NOTE: consuming `self` drops the owned fds; kept as a named method so
    /// the reference's call sites read the same.
    pub fn deinit(self) {
        self.planes.deinit();
    }
}

/// PORTS: `Dmabuf.zig:39-71` — `pub const Planes = struct { … }`, AFTER
/// validation: every plane in `0..count` has an owned fd.
#[derive(Debug)]
pub struct Planes {
    /// Number of planes. Valid planes are `planes[0..count]`.
    pub count: u8,

    /// File descriptor for each plane.
    ///
    /// PORT NOTE: `fds: [max_planes]std.posix.fd_t = @splat(-1)` → `Option`s,
    /// `Some` exactly for `0..count`.
    pub fds: [Option<OwnedFd>; MAX_PLANES],

    /// Offset into the DMABUF where each plane starts, in bytes.
    pub offsets: [c_int; MAX_PLANES],

    /// Strides of each plane, in bytes.
    pub strides: [c_int; MAX_PLANES],
}

impl Planes {
    /// Close all valid fds.
    ///
    /// PORTS: `Dmabuf.zig:52-58`.
    ///
    /// ```zig
    /// pub fn deinit(self: Planes) void {
    ///     for (self.fds[0..self.count]) |fd| {
    ///         if (fd >= 0) _ = std.posix.system.close(fd);
    ///     }
    /// }
    /// ```
    pub fn deinit(self) {
        // Dropping each `OwnedFd` closes it.
    }
}

/// The planes as `eglExportDMABUFImageMESA` fills them: raw fds (`-1` for a
/// plane the driver gave none), not yet owned.
///
/// PORT NOTE: this is the reference's `Planes` between `Target.exportDmabuf`'s
/// `var planes: Dmabuf.Planes = .{ .count = … }` and its `try planes.validate()`
/// (`Target.zig:196-203`). The Rust port splits that state out so the raw fds
/// can only become owned through [`RawPlanes::validate`].
#[derive(Debug)]
pub struct RawPlanes {
    /// Number of planes. Valid planes are `planes[0..count]`.
    pub count: u8,
    /// `fds: [max_planes]std.posix.fd_t = @splat(-1)`.
    pub fds: [c_int; MAX_PLANES],
    /// `offsets: [max_planes]c_int = @splat(0)`.
    pub offsets: [c_int; MAX_PLANES],
    /// `strides: [max_planes]c_int = @splat(0)`.
    pub strides: [c_int; MAX_PLANES],
}

/// `error{BadDmabuf}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadDmabuf;

impl RawPlanes {
    /// `var planes: Dmabuf.Planes = .{ .count = @intCast(query.num_planes) };`
    pub fn new(count: u8) -> Self {
        assert!(usize::from(count) <= MAX_PLANES);
        Self {
            count,
            fds: [-1; MAX_PLANES],
            offsets: [0; MAX_PLANES],
            strides: [0; MAX_PLANES],
        }
    }

    /// Validate the current planes. If any plane failed to export
    /// and has an invalid FD, we close all the known valid FDs
    /// and bail.
    ///
    /// PORTS: `Dmabuf.zig:59-70`.
    ///
    /// ```zig
    /// pub fn validate(self: Planes) error{BadDmabuf}!void {
    ///     var n_valid: usize = 0;
    ///     while (n_valid < self.count) : (n_valid += 1) {
    ///         if (self.fds[n_valid] < 0) {
    ///             for (self.fds[0..n_valid]) |bad| _ = std.posix.system.close(bad);
    ///             return error.BadDmabuf;
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// On success the fds become owned ([`Planes`]).
    ///
    /// R13 FIX (DECISIONS M4, `.port/parity-closure/UPSTREAM-DEFECTS.md` UD353):
    /// on the first invalid plane the reference closes `fds[0..n_valid]` — only
    /// the fds BEFORE it — and returns. A valid fd AFTER the invalid plane (a
    /// driver that exports planes 0 and 2 but not 1) is never closed and nothing
    /// else owns it: one fd leaks per such frame, and frames are exported
    /// continuously. The port closes EVERY valid fd in `fds[0..count]` before it
    /// bails, which is what the reference's own doc comment says it does ("we
    /// close all the known valid FDs").
    ///
    /// # Safety
    ///
    /// Every non-negative fd in `fds[0..count]` must be an open fd the caller owns
    /// and hands over: both outcomes close them (on `Err`) or take ownership of
    /// them (on `Ok`).
    pub unsafe fn validate(self) -> Result<Planes, BadDmabuf> {
        let count = usize::from(self.count);
        let mut n_valid: usize = 0;
        while n_valid < count {
            if self.fds[n_valid] < 0 {
                for &fd in &self.fds[..count] {
                    if fd >= 0 {
                        // SAFETY: the caller hands these fds over (see `# Safety`).
                        drop(unsafe { OwnedFd::from_raw_fd(fd as RawFd) });
                    }
                }
                return Err(BadDmabuf);
            }
            n_valid += 1;
        }

        let mut fds: [Option<OwnedFd>; MAX_PLANES] = Default::default();
        for (slot, &fd) in fds.iter_mut().zip(&self.fds[..count]) {
            // SAFETY: `fd >= 0` (checked above) and handed over by the caller.
            *slot = Some(unsafe { OwnedFd::from_raw_fd(fd as RawFd) });
        }
        Ok(Planes {
            count: self.count,
            fds,
            offsets: self.offsets,
            strides: self.strides,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::os::fd::{IntoRawFd, OwnedFd};
    use std::sync::mpsc;
    use std::time::Duration;

    use super::{BadDmabuf, RawPlanes};

    /// A pipe whose write end is handed out as a raw fd. Whether that fd was
    /// closed is then observed on the READ end: a read returns EOF once every
    /// write end is closed, and blocks while one is still open. Unlike probing the
    /// fd NUMBER, this cannot be fooled by another thread reusing the number.
    struct Probe {
        reader: std::io::PipeReader,
        raw_writer: i32,
    }

    fn probe() -> Probe {
        let (reader, writer) = std::io::pipe().expect("pipe");
        Probe {
            reader,
            raw_writer: OwnedFd::from(writer).into_raw_fd(),
        }
    }

    /// True if the probe's write end was closed (EOF within a second).
    fn closed(p: Probe) -> bool {
        let (tx, rx) = mpsc::channel();
        let mut reader = p.reader;
        std::thread::spawn(move || {
            let mut buf = [0u8; 1];
            let _ = tx.send(reader.read(&mut buf).map(|n| n == 0).unwrap_or(false));
        });
        rx.recv_timeout(Duration::from_secs(1)).unwrap_or(false)
    }

    /// M4 / UD353 fail-first test. The driver fills planes 0 and 2 and leaves
    /// plane 1 at `-1`. `validate` must reject the export AND close BOTH real fds.
    /// The reference closes only the fds BEFORE the first invalid one (`fds[0..
    /// n_valid]`), so plane 2's fd leaks there.
    #[test]
    fn dmabuf_validate_closes_every_fd_after_an_invalid_plane() {
        let before = probe();
        let after = probe();

        let mut raw = RawPlanes::new(3);
        raw.fds = [before.raw_writer, -1, after.raw_writer, -1];

        // SAFETY: both fds are fresh pipe ends this test owns and hands over.
        let res = unsafe { raw.validate() };
        assert_eq!(res.map(|_| ()), Err(BadDmabuf));

        assert!(closed(before), "the fd before the invalid plane leaked");
        assert!(closed(after), "the fd after the invalid plane leaked");
    }

    /// The success path: every plane valid, the fds become owned, and dropping
    /// the `Planes` (`deinit`) closes them all.
    #[test]
    fn dmabuf_validate_takes_ownership_of_valid_planes() {
        let a = probe();
        let b = probe();

        let mut raw = RawPlanes::new(2);
        raw.fds = [a.raw_writer, b.raw_writer, -1, -1];
        raw.strides = [256, 128, 0, 0];
        raw.offsets = [0, 4096, 0, 0];

        // SAFETY: both fds are fresh pipe ends this test owns and hands over.
        let planes = unsafe { raw.validate() }.expect("valid planes");
        assert_eq!(planes.count, 2);
        assert!(planes.fds[0].is_some() && planes.fds[1].is_some());
        assert!(planes.fds[2].is_none() && planes.fds[3].is_none());
        assert_eq!(planes.strides, [256, 128, 0, 0]);
        assert_eq!(planes.offsets, [0, 4096, 0, 0]);

        planes.deinit();
        assert!(closed(a));
        assert!(closed(b));
    }

    /// A first plane that failed rejects the export without closing anything
    /// (there is nothing valid before it), and a zero-plane set is vacuously valid,
    /// as in the reference (`Target.exportDmabuf` rejects a count below 1 before
    /// it ever gets here).
    #[test]
    fn dmabuf_validate_edge_counts() {
        let only = probe();
        let mut raw = RawPlanes::new(2);
        raw.fds = [-1, only.raw_writer, -1, -1];
        // SAFETY: the fd is a fresh pipe end this test owns and hands over.
        assert_eq!(unsafe { raw.validate() }.map(|_| ()), Err(BadDmabuf));
        assert!(closed(only), "the fd after an invalid FIRST plane leaked");

        // SAFETY: no fds.
        let empty = unsafe { RawPlanes::new(0).validate() }.expect("zero planes");
        assert_eq!(empty.count, 0);
    }
}
