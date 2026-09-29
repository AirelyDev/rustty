//! rustty-apprt::gtk::scale
//!
//! PORTS: ghostty `src/apprt/gtk/scale.zig` (186 loc, 7 `test{}`; new at
//! `12752b2ac`, the end state of the fractional-scaling series `45013faf8`,
//! `1964ed123`, `df2186dce`, `08931f405`, `9dc7408be`, `0070f9d0a`, `70a75ceb5`,
//! `d1da18350`, `e1916c1ab`). The device-pixel geometry of a widget at a
//! (possibly fractional) GdkSurface scale: the scale itself, the widget's origin
//! on the GdkSurface, and the device size that both edges snap to.
//!
//! The pure functions ([`device_size`], [`snapped_device_size`],
//! [`snap_offset`]) and their seven tests build on every host; the widget
//! helpers need GTK.

/// `pub const DeviceSize = struct { width: u32, height: u32 };`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceSize {
    pub width: u32,
    pub height: u32,
}

/// `pub const CssPoint = struct { x: f64, y: f64 };`
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CssPoint {
    pub x: f64,
    pub y: f64,
}

/// Scale, GdkSurface-space origin, and snapped device size for one widget.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub scale: f64,
    pub origin: CssPoint,
    pub size: DeviceSize,
}

impl Layout {
    /// CSS offset that moves widget (0,0) onto the nearest device pixel.
    pub fn snap_origin(self) -> CssPoint {
        CssPoint {
            x: snap_offset(self.origin.x, self.scale),
            y: snap_offset(self.origin.y, self.scale),
        }
    }
}

/// PORTS: `scale.zig:32-43` `widgetSurfaceScale`.
///
/// ```zig
/// pub fn widgetSurfaceScale(widget: *gtk.Widget) f64 {
///     if (widget.getNative()) |native| {
///         if (native.getSurface()) |surface| {
///             const scale = surface.getScale();
///             if (scale > 0) return scale;
///         }
///     }
///
///     const scale = widget.getScaleFactor();
///     if (scale <= 0) return 1.0;
///     return @floatFromInt(scale);
/// }
/// ```
#[cfg(all(feature = "gtk", any(target_os = "linux", target_os = "freebsd")))]
pub fn widget_surface_scale(widget: &gtk4::Widget) -> f64 {
    use gtk4::prelude::*;

    if let Some(native) = widget.native() {
        if let Some(surface) = native.surface() {
            let scale = surface.scale();
            if scale > 0.0 {
                return scale;
            }
        }
    }

    let scale = widget.scale_factor();
    if scale <= 0 {
        return 1.0;
    }
    f64::from(scale)
}

/// Widget (0,0) in GdkSurface CSS coordinates.
///
/// `computePoint` to the native widget is CSS inside the window. CSD shadows
/// live outside that, so add `gtk_native_get_surface_transform` to get the
/// origin on the GdkSurface.
//
// PORTS: `scale.zig:45-71` `widgetSurfaceOrigin`.
#[cfg(all(feature = "gtk", any(target_os = "linux", target_os = "freebsd")))]
pub fn widget_surface_origin(widget: &gtk4::Widget) -> CssPoint {
    use gtk4::prelude::*;

    // const native = widget.getNative() orelse return .{ .x = 0, .y = 0 };
    let Some(native) = widget.native() else {
        return CssPoint { x: 0.0, y: 0.0 };
    };
    // const native_widget = gobject.ext.cast(gtk.Widget, native) orelse return ...;
    let native_widget: &gtk4::Widget = native.upcast_ref();

    // var native_origin: graphene.Point = undefined;
    // if (widget.computePoint(native_widget, &.{ .f_x = 0, .f_y = 0 }, &native_origin) == 0)
    //     return .{ .x = 0, .y = 0 };
    let Some(native_origin) =
        widget.compute_point(native_widget, &gtk4::graphene::Point::new(0.0, 0.0))
    else {
        return CssPoint { x: 0.0, y: 0.0 };
    };

    // var tx: f64 = 0; var ty: f64 = 0;
    // native.getSurfaceTransform(&tx, &ty);
    let (tx, ty) = native.surface_transform();

    CssPoint {
        x: f64::from(native_origin.x()) + tx,
        y: f64::from(native_origin.y()) + ty,
    }
}

/// PORTS: `scale.zig:73-75` `widgetLayout`.
#[cfg(all(feature = "gtk", any(target_os = "linux", target_os = "freebsd")))]
pub fn widget_layout(widget: &gtk4::Widget) -> Layout {
    use gtk4::prelude::*;
    widget_layout_for_size(widget, widget.width(), widget.height())
}

/// PORTS: `scale.zig:77-85` `widgetLayoutForSize`.
#[cfg(all(feature = "gtk", any(target_os = "linux", target_os = "freebsd")))]
pub fn widget_layout_for_size(widget: &gtk4::Widget, css_width: i32, css_height: i32) -> Layout {
    let scale = widget_surface_scale(widget);
    let origin = widget_surface_origin(widget);
    Layout {
        scale,
        origin,
        size: snapped_device_size(origin.x, origin.y, css_width, css_height, scale),
    }
}

/// PORTS: `scale.zig:87-89` `deviceSize`.
pub fn device_size(css_width: i32, css_height: i32, scale: f64) -> DeviceSize {
    snapped_device_size(0.0, 0.0, css_width, css_height, scale)
}

/// PORTS: `scale.zig:91-102` `snappedDeviceSize`.
pub fn snapped_device_size(
    origin_x: f64,
    origin_y: f64,
    css_width: i32,
    css_height: i32,
    scale: f64,
) -> DeviceSize {
    DeviceSize {
        width: snapped_axis(origin_x, css_width, scale),
        height: snapped_axis(origin_y, css_height, scale),
    }
}

/// CSS delta that lands a surface-relative origin on the nearest device pixel.
///
/// Tab chrome can put widget (0,0) on a fractional device coordinate, so GTK
/// linearly samples every texel. Shift by less than 0.5 device px without
/// changing content size.
//
// PORTS: `scale.zig:104-114` `snapOffset`.
//
// PORT NOTE: Zig `@round` rounds half away from zero, as `f64::round` does.
pub fn snap_offset(css_origin: f64, scale: f64) -> f64 {
    #[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(scale > 0)` also rejects NaN, as Zig's does
    if !(scale > 0.0) {
        return 0.0;
    }
    let device_origin = css_origin * scale;
    (device_origin.round() - device_origin) / scale
}

/// Device coverage of one axis after snapping both edges.
///
/// `round(end) - round(start)`, not `ceil(css × scale)`, so the buffer spans
/// the same snapped pixels the texture is drawn into.
//
// PORTS: `scale.zig:116-128` `snappedAxis`.
//
// PORT NOTE (R7): `@intFromFloat(end - start)` traps when the span leaves
// `u32`; the port asserts the same.
fn snapped_axis(css_origin: f64, css: i32, scale: f64) -> u32 {
    #[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(scale > 0)` also rejects NaN
    if css <= 0 || !(scale > 0.0) {
        return 0;
    }
    let css_size = f64::from(css);
    let start = (css_origin * scale).round();
    let end = ((css_origin + css_size) * scale).round();
    if end <= start {
        return 0;
    }
    let span = end - start;
    assert!(
        span <= f64::from(u32::MAX),
        "snapped span {span} exceeds u32"
    );
    span as u32
}

#[cfg(test)]
mod tests {
    use super::{device_size, snap_offset, snapped_device_size, DeviceSize};

    fn assert_approx(expected: f64, actual: f64, tolerance: f64) {
        assert!(
            (expected - actual).abs() <= tolerance,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn device_size_integer_scale() {
        assert_eq!(
            DeviceSize {
                width: 800,
                height: 600
            },
            device_size(800, 600, 1.0)
        );
        assert_eq!(
            DeviceSize {
                width: 1600,
                height: 1200
            },
            device_size(800, 600, 2.0)
        );
    }

    #[test]
    fn device_size_fractional_scale() {
        assert_eq!(
            DeviceSize {
                width: 1000,
                height: 750
            },
            device_size(800, 600, 1.25)
        );
        assert_eq!(
            DeviceSize {
                width: 1200,
                height: 900
            },
            device_size(800, 600, 1.5)
        );
        assert_eq!(
            DeviceSize {
                width: 126,
                height: 126
            },
            device_size(101, 101, 1.25)
        );
    }

    #[test]
    fn device_size_rejects_non_positive_inputs() {
        assert_eq!(
            DeviceSize {
                width: 0,
                height: 15
            },
            device_size(0, 10, 1.5)
        );
        assert_eq!(
            DeviceSize {
                width: 0,
                height: 0
            },
            device_size(10, 10, 0.0)
        );
        assert_eq!(
            DeviceSize {
                width: 0,
                height: 15
            },
            device_size(-1, 10, 1.5)
        );
    }

    #[test]
    fn snap_offset_aligns_fractional_device_origins() {
        assert_approx(0.0, snap_offset(0.0, 1.75), 0.000001);
        assert_approx(-1.0 / 7.0, snap_offset(47.0, 1.75), 0.000001);
        assert_approx(0.2, snap_offset(47.0, 1.25), 0.000001);
        assert_approx(0.25, snap_offset(47.0, 4.0 / 3.0), 0.000001);
    }

    #[test]
    fn snap_offset_rejects_non_positive_scale() {
        assert_eq!(0.0, snap_offset(47.0, 0.0));
        assert_eq!(0.0, snap_offset(47.0, -1.25));
    }

    #[test]
    fn snapped_device_size_uses_origin_aware_device_spans() {
        let scale_4_3 = 4.0 / 3.0;
        assert_eq!(
            DeviceSize {
                width: 1066,
                height: 1066
            },
            snapped_device_size(47.0, 47.0, 800, 800, scale_4_3)
        );
        assert_eq!(
            DeviceSize {
                width: 1097,
                height: 628
            },
            snapped_device_size(0.0, 0.0, 823, 471, scale_4_3)
        );
        assert_eq!(
            DeviceSize {
                width: 126,
                height: 126
            },
            snapped_device_size(0.0, 0.0, 101, 101, 1.25)
        );
    }

    #[test]
    fn snapped_device_size_integer_scale_with_origin() {
        assert_eq!(
            DeviceSize {
                width: 800,
                height: 600
            },
            snapped_device_size(47.0, 11.0, 800, 600, 1.0)
        );
        assert_eq!(
            DeviceSize {
                width: 1600,
                height: 1200
            },
            snapped_device_size(0.25, 0.75, 800, 600, 2.0)
        );
    }
}
