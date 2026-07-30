//! rustty-apprt::gtk::winproto::blur_region
//!
//! PORTS: ghostty `src/apprt/gtk/winproto/BlurRegion.zig` (188 loc) — the helper
//! that computes the rectangular slices of a window that should receive the
//! compositor's background blur. Because rustty renders rounded corners by
//! default, the corner pixels are carved out to avoid the "korners bug"
//! (`BlurRegion.zig:36-40`).
//!
//! Consumed by the Wayland winproto arm's `Window::sync_blur`
//! ([`super::wayland::Window`]); gated `#[cfg(feature = "wayland")]` alongside it.
//!
//! # Divergence from the reference
//!
//! * The reference's `Slice`/`Pos` are `extern`/`c_long` so the SAME struct can be
//!   handed to XLib directly (`BlurRegion.zig:15-25`). rustty's X11 arm is not yet
//!   ported (L6), so [`Pos`] is a plain `i64` (the `c_long` width on the 64-bit
//!   Linux targets rustty builds for) and [`Slice`] is an ordinary `#[repr(C)]`
//!   struct — ready for the X11 arm to reuse verbatim when it lands.
//! * The reference threads an `Allocator` through `calcForWindow`/`approxRoundedRect`
//!   and the `slices` `ArrayList`; rustty uses the global allocator (a `Vec`), per
//!   the tier-wide allocator-seam invariant (PREPORT_GTK §2, invariant 2), so the
//!   `alloc` parameter is dropped.

use gdk4::prelude::*;
use gtk4::prelude::*;

/// PORTS: `BlurRegion.zig:25` `const Pos = c_long`. XLib compatibility width; on
/// the 64-bit Linux targets rustty builds for, `c_long` is `i64`. Wayland itself
/// only needs `i32` (the slices are cast down at the `wl_region.add` call site).
pub type Pos = i64;

/// PORTS: `BlurRegion.zig:16-21` `Slice` — one rectangular slice of the blur
/// region. `#[repr(C)]` mirrors the reference's `extern struct` so the X11 arm
/// (L6) can reuse it across the XLib FFI boundary unchanged.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slice {
    pub x: Pos,
    pub y: Pos,
    pub width: Pos,
    pub height: Pos,
}

/// PORTS: `BlurRegion.zig` (the `@This()` struct) — the set of blur slices for a
/// window.
#[derive(Debug, Default)]
pub struct BlurRegion {
    slices: Vec<Slice>,
}

impl BlurRegion {
    /// PORTS: `BlurRegion.zig:27-29` `empty`.
    #[must_use]
    pub fn empty() -> Self {
        Self { slices: Vec::new() }
    }

    /// PORTS: `BlurRegion.zig:31-34` `deinit`. rustty has no allocator to thread,
    /// so this just clears the `Vec`.
    pub fn clear(&mut self) {
        self.slices.clear();
    }

    /// The computed slices (surface-local, or device coordinates when
    /// `calc_for_window` was called with `to_device_coordinates = true`).
    #[must_use]
    pub fn slices(&self) -> &[Slice] {
        &self.slices
    }

    /// PORTS: `BlurRegion.zig:124-130` `eql`. Whether two sets of blur regions are
    /// equal.
    #[must_use]
    pub fn eql(&self, other: &BlurRegion) -> bool {
        self.slices == other.slices
    }

    /// PORTS: `BlurRegion.zig:41-121` `calcForWindow`. Compute the blur region for
    /// a window, carving out the rounded corners so blur does not bleed past them.
    ///
    /// `native` is the window as a [`gtk4::Native`] (the reference's
    /// `window.as(gtk.Native)`); `csd` is whether client-side decorations are
    /// enabled (used to inset the corners); `to_device_coordinates` scales the
    /// slices by the surface scale factor (the X11 path; the Wayland path passes
    /// `false`).
    #[must_use]
    pub fn calc_for_window(
        native: &gtk4::Native,
        csd: bool,
        to_device_coordinates: bool,
    ) -> BlurRegion {
        // `native.getSurface() orelse return .empty` (`BlurRegion.zig:48-49`).
        let Some(surface) = native.surface() else {
            return BlurRegion::empty();
        };

        // NOTE(pluiedev, ported verbatim): CSDs make GNOME report a window ~30%
        // bigger than it really is (shadows + rounded corners). We trim those
        // offsets. (`BlurRegion.zig:58-72`)
        let (x, y): (Pos, Pos) = {
            // `native.getSurfaceTransform(&x, &y)` (`BlurRegion.zig:65`).
            let (mut xf, mut yf) = native.surface_transform();
            // Slightly inset the corners if we're using CSDs (`:66-70`).
            if csd {
                xf += 1.0;
                yf += 1.0;
            }
            (xf as Pos, yf as Pos)
        };

        // `BlurRegion.zig:74-80` — trim the CSD offsets off both ends (folded into
        // the initializer; be careful not to go negative).
        let width = Pos::from(surface.width()) - x * 2;
        let height = Pos::from(surface.height()) - y * 2;
        if width <= 0 || height <= 0 {
            return BlurRegion::empty();
        }

        // Empirically determined (`BlurRegion.zig:82-97`).
        let are_corners_rounded = {
            // This cast should always succeed as all of our windows should be
            // toplevel (`:85-90`).
            if let Some(toplevel) = surface.downcast_ref::<gdk4::Toplevel>() {
                let state = toplevel.state();
                if state.contains(gdk4::ToplevelState::FULLSCREEN)
                    || state.contains(gdk4::ToplevelState::MAXIMIZED)
                    || state.contains(gdk4::ToplevelState::TILED)
                {
                    false
                } else {
                    csd
                }
            } else {
                false
            }
        };

        // See https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/css-variables.html#window-radius
        // (`BlurRegion.zig:99-107`).
        let mut slices = approx_rounded_rect(
            x,
            y,
            width,
            height,
            if are_corners_rounded { 15 } else { 0 },
        );

        if to_device_coordinates {
            // Transform surface coordinates to device coordinates
            // (`BlurRegion.zig:109-118`).
            let sf = Pos::from(surface.scale_factor());
            for s in &mut slices {
                s.x *= sf;
                s.y *= sf;
                s.width *= sf;
                s.height *= sf;
            }
        }

        BlurRegion { slices }
    }
}

/// PORTS: `BlurRegion.zig:132-187` `approxRoundedRect`. Approximate a rounded
/// rectangle with many smaller rectangles. Honestly quite cursed (the reference
/// says so at `:154`), but ported faithfully.
fn approx_rounded_rect(x: Pos, y: Pos, width: Pos, height: Pos, radius: Pos) -> Vec<Slice> {
    let r_f = radius as f32;

    let mut slices: Vec<Slice> = Vec::new();

    // Add the central rectangle (`BlurRegion.zig:146-152`).
    slices.push(Slice {
        x,
        y: y + radius,
        width,
        height: height - 2 * radius,
    });

    // Add the corner rows (`BlurRegion.zig:154-184`).
    let mut row: Pos = 0;
    while row < radius {
        // y distance from this row to the center corner circle (`:157-158`).
        let dy = (radius - row) as f32 - 0.5;

        // x distance - as given by the definition of a circle (`:160-161`).
        let dx = (r_f * r_f - dy * dy).sqrt();

        // How much each row should be offset, rounded to an integer (`:163-164`).
        let row_x: Pos = (r_f - (dx + 0.5).round()) as Pos;

        // Remove the offset from both ends (`:166-167`).
        let row_w = width - 2 * row_x;

        // Top slice (`:169-175`).
        slices.push(Slice {
            x: x + row_x,
            y: y + row,
            width: row_w,
            height: 1,
        });

        // Bottom slice (`:177-183`).
        slices.push(Slice {
            x: x + row_x,
            y: y + height - 1 - row,
            width: row_w,
            height: 1,
        });

        row += 1;
    }

    slices
}
