//! rustty-vaxis::vxfw::center — the `Center` widget.
//!
//! PORTS: `vaxis/src/vxfw/Center.zig` — centers a single child within the max
//! constraints (bias toward the top-left corner). `cli +boo` uses this for the
//! "Screen must be at least 100w x 41h" warning.

use std::os::raw::c_void;

use super::{DrawContext, Error, RelativePoint, Size, SubSurface, Surface, Widget};

/// `child: vxfw.Widget,` — the `Center` struct is a single field.
#[derive(Clone, Copy)]
pub struct Center<'a> {
    pub child: Widget<'a>,
}

impl<'a> Center<'a> {
    /// `pub fn widget(self: *const Center) vxfw.Widget`
    ///
    /// PORT-NOTE: Zig takes `*const Center` and `@constCast`s it into `userdata`.
    /// We take `*const Center` and store it as the type-erased `*mut c_void`; the
    /// draw fn casts it back to `*const Center` (never mutated). No `eventHandler`
    /// (Center is a pure layout widget).
    pub fn widget(&self) -> Widget<'a> {
        Widget {
            userdata: self as *const Center<'a> as *mut c_void,
            capture_handler: None,
            event_handler: None,
            draw_fn: Center::type_erased_draw_fn,
        }
    }

    fn type_erased_draw_fn(ptr: *mut c_void, ctx: DrawContext) -> Result<Surface<'a>, Error> {
        // SAFETY: `ptr` is the `*const Center` stored by `widget()`; read-only.
        let self_ref: &Center<'a> = unsafe { &*(ptr as *const Center<'a>) };
        self_ref.draw(ctx)
    }

    /// Cannot have unbounded constraints
    pub fn draw(&self, ctx: DrawContext) -> Result<Surface<'a>, Error> {
        let child_ctx = ctx.with_constraints(
            Size {
                width: 0,
                height: 0,
            },
            ctx.max,
        );
        let max_size = ctx.max.size();
        let child = self.child.draw(child_ctx)?;

        let x = (max_size.width - child.size.width) / 2;
        let y = (max_size.height - child.size.height) / 2;

        // const children = try ctx.arena.alloc(vxfw.SubSurface, 1);
        // SAFETY: `ctx.arena` is a live arena; we init `children[0]` before return.
        let children: *mut [SubSurface<'a>] =
            unsafe { (*ctx.arena).alloc_slice::<SubSurface<'a>>(1) };
        unsafe {
            children.cast::<SubSurface<'a>>().write(SubSurface {
                origin: RelativePoint {
                    col: x as i32,
                    row: y as i32,
                },
                z_index: 0,
                surface: child,
            });
        }

        Ok(Surface {
            size: max_size,
            widget: self.widget(),
            cursor: None,
            buffer: super::empty_cells(),
            children,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vxfw::text::Text;
    use crate::vxfw::{MaxSize, VxfwArena};

    // test Center
    #[test]
    fn center() {
        // Will be height=1, width=3
        let text = Text {
            text: b"abc",
            ..Text::default()
        };
        let center = Center {
            child: text.widget(),
        };

        let arena = VxfwArena::new();
        DrawContext::init(crate::gwidth::Method::Unicode);

        {
            // Center expands to the max size. It must therefore have non-null max width and max height.
            let ctx = DrawContext {
                arena: &arena,
                min: Size::default(),
                max: MaxSize {
                    width: Some(10),
                    height: Some(10),
                },
                cell_size: Size {
                    width: 10,
                    height: 20,
                },
            };

            let surface = center.draw(ctx).unwrap();
            // Center does not produce any drawable cells
            assert_eq!(surface.buffer.len(), 0);
            // Center has 1 child
            assert_eq!(surface.children.len(), 1);
            // Center is the max size
            assert_eq!(surface.size, ctx.max.size());
            // SAFETY: exactly one child was allocated + initialized.
            let child = unsafe { &*surface.children.cast::<SubSurface>() };
            // The child is 1x3
            assert_eq!(child.surface.size.width, 3);
            assert_eq!(child.surface.size.height, 1);
            // A centered 1x3 in 10x10 should be at origin 3, 4. The bias is toward the top left corner
            assert_eq!(child.origin.row, 4);
            assert_eq!(child.origin.col, 3);
        }
        {
            let ctx = DrawContext {
                arena: &arena,
                min: Size::default(),
                max: MaxSize {
                    width: Some(5),
                    height: Some(3),
                },
                cell_size: Size {
                    width: 10,
                    height: 20,
                },
            };

            let surface = center.draw(ctx).unwrap();
            assert_eq!(surface.buffer.len(), 0);
            assert_eq!(surface.children.len(), 1);
            assert_eq!(surface.size, ctx.max.size());
            // SAFETY: exactly one child was allocated + initialized.
            let child = unsafe { &*surface.children.cast::<SubSurface>() };
            assert_eq!(child.surface.size.width, 3);
            assert_eq!(child.surface.size.height, 1);
            // A centered 1x3 in 3x5 should be at origin 1, 1. This is a perfectly centered child
            assert_eq!(child.origin.row, 1);
            assert_eq!(child.origin.col, 1);
        }
    }
}
