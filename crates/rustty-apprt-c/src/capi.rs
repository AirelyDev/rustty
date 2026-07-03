//! capi — the libghostty C-ABI extern structs (`apprt/embedded.zig` `CAPI` block).
//!
//! PORTS: the extern-struct declarations of `pub const CAPI = struct { ... }` in
//! `src/apprt/embedded.zig` — the E5 subset (embedded.zig:1244-1397): the raw
//! C-API [`KeyEvent`], [`SurfaceSize`], [`Text`], [`Point`] (+ its `Tag`/
//! `CoordTag`), and [`Selection`], together with the D2 const-size/align-asserts
//! that pin each layout to the reference ABI (vt-c `point.rs`/`sgr.rs` pattern).
//! `ClipboardContent` (`ghostty_clipboard_content_s`) is a BACKEND field type (it
//! is referenced by [`rustty_apprt::embedded::Options::write_clipboard`]) so it
//! lives in the `rustty-apprt` backend; here we only re-assert its pinned ABI
//! layout (E5) — apprt-c hand-pins it in `cbindgen.toml` `after_includes` when
//! header generation is wired (E11).
//!
//! The `ghostty_*` (`rustty_*`) CAPI exports that CONSUME these structs land in
//! chunks E6..E10; the const-asserts below are the build-failure gate that keeps
//! the layouts byte-faithful in the meantime (R1/R3, EMBEDDED_BLUEPRINT §3/§6).
//!
//! PORT NOTE — the CAPI `global` alias. The reference opens the `CAPI` block with
//! `const global = &@import("../global.zig").state;` (embedded.zig:1245), the
//! process-global state that owns the allocator the C-string frees use. In rustty
//! the global-alloc seam is `()` — Rust's global allocator IS the allocator
//! (`rustty_apprt::init` / EMBEDDED_BLUEPRINT §3 "Global-state precursor") — so the
//! only place it appears here, [`Text::deinit`], frees through Rust's global
//! allocator directly (reconstructing the leaked box). No `global` binding is
//! needed.

// The reference method identifier `keyEvent` is camelCase (preserved 1:1, as the
// `rustty-apprt` backend does for the reference's camelCase API — R3); silence the
// snake-case lint at module scope.
#![allow(non_snake_case)]

use core::ffi::{c_char, c_int};

use rustty_apprt::embedded;

use rustty_vt::input::key;
use rustty_vt::input::key_mods::{Mods, ModsBacking};
use rustty_vt::pagelist::Pin;
use rustty_vt::point;
use rustty_vt::screen::Screen;
use rustty_vt::selection;
use rustty_vt::size;

/// This is the same as Surface.KeyEvent but this is the raw C API version.
//
// PORT NOTE: reference `const KeyEvent = extern struct { ... }` (embedded.zig:1248).
// The native `App.KeyEvent` is `rustty_apprt::embedded::KeyEvent<'a>`; this is the
// wire-format struct the `ghostty_surface_key`/`ghostty_app_key` exports (E8)
// receive, converted to the native form by [`KeyEvent::keyEvent`].
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KeyEvent {
    // action: input.Action,
    pub action: key::Action,
    // mods: c_int,
    pub mods: c_int,
    // consumed_mods: c_int,
    pub consumed_mods: c_int,
    // keycode: u32,
    pub keycode: u32,
    // text: ?[*:0]const u8,
    //
    // PORT NOTE: `?[*:0]const u8` -> a nullable pointer to a NUL-terminated C
    // string (null == none), modeled as the raw `*const c_char` the rustty CAPI
    // convention uses for `[*:0]const u8` fields (cf. `rustty-config`'s C `String`
    // and the backend `ClipboardContent`). 8 bytes, null-representable.
    pub text: *const c_char,
    // unshifted_codepoint: u32,
    pub unshifted_codepoint: u32,
    // composing: bool,
    pub composing: bool,
}

// BUILD-failure gate (D2): pin the reference `extern struct` ABI. Field layout:
// action@0(4) mods@4(4) consumed_mods@8(4) keycode@12(4) text@16(8) unshifted@24(4)
// composing@28(1) -> size 32 (padded to the pointer alignment), align 8. Drift in
// any field add/remove changes the size and fails the build. [ADV-FIX 7] this
// catches field add/remove, NOT callback/signature drift (there are no fn-ptrs
// here); the `action` c_int-width assert pins the enum tag width.
const _: () = assert!(size_of::<KeyEvent>() == 32);
const _: () = assert!(align_of::<KeyEvent>() == 8);
const _: () = assert!(size_of::<key::Action>() == size_of::<c_int>());

impl KeyEvent {
    /// Convert to Zig key event.
    ///
    /// # Safety
    /// `self.text`, if non-null, must point to a valid NUL-terminated C string that
    /// stays alive for the chosen lifetime `'a` (the reference relies on the same
    /// invariant for its returned slice).
    //
    // PORT NOTE: reference `fn keyEvent(self: KeyEvent) App.KeyEvent`. The returned
    // `rustty_apprt::embedded::KeyEvent<'a>` borrows its `text` from the C string
    // `self.text` points to (the reference `std.mem.sliceTo(ptr, 0)`), which is
    // owned by the caller and outlives the call — hence the caller-chosen `'a`.
    pub unsafe fn keyEvent<'a>(self) -> embedded::KeyEvent<'a> {
        embedded::KeyEvent {
            // .action = self.action,
            action: self.action,
            // .mods = @bitCast(@as(input.Mods.Backing, @truncate(@as(c_uint, @bitCast(self.mods))))),
            //
            // PORT NOTE: reinterpret the signed `c_int` as unsigned (`@bitCast` ->
            // `as c_uint`), truncate to the `u16` `Mods.Backing` (`@truncate` ->
            // `as ModsBacking`), then reinterpret the backing bits as `Mods`
            // (`@bitCast` -> `Mods::from_int`).
            mods: Mods::from_int(self.mods as core::ffi::c_uint as ModsBacking),
            // .consumed_mods = @bitCast(@as(input.Mods.Backing, @truncate(@as(c_uint, @bitCast(self.consumed_mods))))),
            consumed_mods: Mods::from_int(self.consumed_mods as core::ffi::c_uint as ModsBacking),
            // .keycode = self.keycode,
            keycode: self.keycode,
            // .text = if (self.text) |ptr| std.mem.sliceTo(ptr, 0) else null,
            text: if self.text.is_null() {
                None
            } else {
                // SAFETY: guaranteed by this fn's contract — `self.text` is a valid
                // NUL-terminated C string living for `'a`.
                Some(unsafe { core::ffi::CStr::from_ptr(self.text) }.to_bytes())
            },
            // .unshifted_codepoint = self.unshifted_codepoint,
            unshifted_codepoint: self.unshifted_codepoint,
            // .composing = self.composing,
            composing: self.composing,
        }
    }
}

// PORT NOTE: reference `const SurfaceSize = extern struct { ... }`
// (embedded.zig:1277). Returned by `ghostty_surface_size` (E7); distinct from the
// native `apprt.SurfaceSize` ({width,height}) — this is the grid+pixel wire form.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SurfaceSize {
    // columns: u16,
    pub columns: u16,
    // rows: u16,
    pub rows: u16,
    // width_px: u32,
    pub width_px: u32,
    // height_px: u32,
    pub height_px: u32,
    // cell_width_px: u32,
    pub cell_width_px: u32,
    // cell_height_px: u32,
    pub cell_height_px: u32,
}

// BUILD-failure gate (D2): {u16,u16,u32,u32,u32,u32} -> size 20, align 4.
const _: () = assert!(size_of::<SurfaceSize>() == 20);
const _: () = assert!(align_of::<SurfaceSize>() == 4);

// ghostty_clipboard_content_s
//
// PORT NOTE: `const ClipboardContent = extern struct { mime, data }`
// (embedded.zig:1287) is a BACKEND field type (`Options.write_clipboard` takes a
// `[*]const CAPI.ClipboardContent`), so it is DEFINED in `rustty-apprt`
// (`rustty_apprt::embedded::ClipboardContent`, `{ *const c_char, *const c_char }`);
// apprt-c only re-asserts its pinned ABI here and hand-pins it in cbindgen
// `after_includes` at header-gen time (E11).
//
// BUILD-failure gate (D2): {*const c_char, *const c_char} -> size 16, align 8.
const _: () = assert!(size_of::<embedded::ClipboardContent>() == 16);
const _: () = assert!(align_of::<embedded::ClipboardContent>() == 8);

// ghostty_text_s
//
// PORT NOTE: reference `const Text = extern struct { ... }` (embedded.zig:1293).
// The read-text/selection exports (E7) return this by value; C reclaims the owned
// `text` C-string via `ghostty_surface_free_text` -> [`Text::deinit`]. NOT `Copy`:
// it owns a heap allocation, so a bit-copy would risk a double free (the reference
// extern struct is trivially copyable, but ownership is by C-side convention).
#[repr(C)]
pub struct Text {
    // tl_px_x: f64,
    pub tl_px_x: f64,
    // tl_px_y: f64,
    pub tl_px_y: f64,
    // offset_start: u32,
    pub offset_start: u32,
    // offset_len: u32,
    pub offset_len: u32,
    // text: ?[*:0]const u8,
    pub text: *const c_char,
    // text_len: usize,
    pub text_len: usize,
}

// BUILD-failure gate (D2): tl_px_x@0(8) tl_px_y@8(8) offset_start@16(4)
// offset_len@20(4) text@24(8) text_len@32(8) -> size 40, align 8.
const _: () = assert!(size_of::<Text>() == 40);
const _: () = assert!(align_of::<Text>() == 8);

impl Text {
    // pub fn deinit(self: *Text) void {
    //     if (self.text) |ptr| {
    //         global.alloc.free(ptr[0..self.text_len :0]);
    //     }
    // }
    //
    // PORT NOTE: the rustty global-alloc seam is Rust's global allocator (see the
    // module `global` note), so `global.alloc.free(ptr[0..text_len :0])` frees the
    // producer's leaked boxed slice. The reference frees a NUL-SENTINEL slice
    // (`[0..text_len :0]`), so the backing allocation is `text_len + 1` bytes (the
    // data plus the sentinel); the producing export (E7) MUST leak a
    // `Box<[u8]>` of exactly that length so this reconstruction frees the same
    // memory.
    pub fn deinit(&mut self) {
        // if (self.text) |ptr| { ... }
        if !self.text.is_null() {
            // SAFETY: `self.text` is non-null and (per the producer contract) points
            // to the start of a `Box<[u8]>` of length `text_len + 1` that has not yet
            // been freed; reconstruct and drop it to free via the global allocator.
            unsafe {
                let len = self.text_len + 1;
                drop(Box::from_raw(core::ptr::slice_from_raw_parts_mut(
                    self.text as *mut u8,
                    len,
                )));
            }
        }
    }
}

// ghostty_point_s
//
// PORT NOTE: reference `const Point = extern struct { tag, coord_tag, x, y }`
// (embedded.zig:1309), with the nested `Tag`/`CoordTag` `enum(c_int)`s. Rust has no
// struct-nested enums, so they are the module-level [`PointTag`]/[`PointCoordTag`].
// This is the C wire `Point` (distinct from `rustty_vt::point::Point`); [`Point::pin`]
// resolves it against a screen to a core `Pin`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Point {
    // tag: Tag,
    pub tag: PointTag,
    // coord_tag: CoordTag,
    pub coord_tag: PointCoordTag,
    // x: u32,
    pub x: u32,
    // y: u32,
    pub y: u32,
}

/// The reference `Point.Tag` — `enum(c_int) { active, viewport, screen, history }`.
#[repr(C)]
#[derive(Clone, Copy)]
pub enum PointTag {
    // active = 0,
    Active = 0,
    // viewport = 1,
    Viewport = 1,
    // screen = 2,
    Screen = 2,
    // history = 3,
    History = 3,
}

/// The reference `Point.CoordTag` — `enum(c_int) { exact, top_left, bottom_right }`.
#[repr(C)]
#[derive(Clone, Copy)]
pub enum PointCoordTag {
    // exact = 0,
    Exact = 0,
    // top_left = 1,
    TopLeft = 1,
    // bottom_right = 2,
    BottomRight = 2,
}

// BUILD-failure gate (D2): {c_int, c_int, u32, u32} -> size 16, align 4; the two
// tag enums are `enum(c_int)` -> c_int-width.
const _: () = assert!(size_of::<Point>() == 16);
const _: () = assert!(align_of::<Point>() == 4);
const _: () = assert!(size_of::<PointTag>() == size_of::<c_int>());
const _: () = assert!(size_of::<PointCoordTag>() == size_of::<c_int>());

impl Point {
    // fn pin(self: Point, screen: *const terminal.Screen) ?terminal.Pin
    //
    // PORT NOTE: `*const terminal.Screen` -> `&Screen` (read-only borrow).
    pub fn pin(self, screen: &Screen) -> Option<Pin> {
        // The core point tag.
        // const tag: terminal.point.Tag = switch (self.tag) {
        //     inline else => |tag| @field(terminal.point.Tag, @tagName(tag)),
        // };
        //
        // PORT NOTE: the reference's `@field(..., @tagName(tag))` reflection maps the
        // C `Tag` to the same-named core `point.Tag`; ported as an explicit match.
        let tag: point::Tag = match self.tag {
            PointTag::Active => point::Tag::Active,
            PointTag::Viewport => point::Tag::Viewport,
            PointTag::Screen => point::Tag::Screen,
            PointTag::History => point::Tag::History,
        };

        // Clamp our point to the screen bounds.
        // const clamped_x = @min(self.x, screen.pages.cols -| 1);
        let clamped_x: u32 = self.x.min(screen.pages.cols.saturating_sub(1) as u32);
        // const clamped_y = @min(self.y, screen.pages.rows -| 1);
        let clamped_y: u32 = self.y.min(screen.pages.rows.saturating_sub(1) as u32);

        match self.coord_tag {
            // Exact coordinates require a specific pin.
            // .exact => exact: {
            PointCoordTag::Exact => {
                // const pt_x = std.math.cast(terminal.size.CellCountInt, clamped_x)
                //     orelse std.math.maxInt(terminal.size.CellCountInt);
                //
                // PORT NOTE: `clamped_x <= cols - 1 <= CellCountInt::MAX`, so the cast
                // always succeeds and the `orelse maxInt` fallback is unreachable;
                // preserved via `try_from().unwrap_or(MAX)`.
                let pt_x: size::CellCountInt =
                    size::CellCountInt::try_from(clamped_x).unwrap_or(size::CellCountInt::MAX);

                // const pt: terminal.Point = switch (tag) {
                //     inline else => |v| @unionInit(
                //         terminal.Point, @tagName(v), .{ .x = pt_x, .y = clamped_y },
                //     ),
                // };
                let pt: point::Point = point::Point::from_c(
                    tag,
                    point::Coordinate {
                        x: pt_x,
                        y: clamped_y,
                    },
                );

                // break :exact screen.pages.pin(pt) orelse null;
                screen.pages.pin(pt)
            }

            // .top_left => screen.pages.getTopLeft(tag),
            PointCoordTag::TopLeft => Some(screen.pages.get_top_left(tag)),

            // .bottom_right => screen.pages.getBottomRight(tag),
            PointCoordTag::BottomRight => screen.pages.get_bottom_right(tag),
        }
    }
}

// ghostty_selection_s
//
// PORT NOTE: reference `const Selection = extern struct { tl, br, rectangle }`
// (embedded.zig:1371). [`Selection::core`] resolves both endpoints against a screen
// to a core `terminal.Selection`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Selection {
    // tl: Point,
    pub tl: Point,
    // br: Point,
    pub br: Point,
    // rectangle: bool,
    pub rectangle: bool,
}

// BUILD-failure gate (D2): tl@0(16) br@16(16) rectangle@32(1) -> size 36, align 4.
const _: () = assert!(size_of::<Selection>() == 36);
const _: () = assert!(align_of::<Selection>() == 4);

impl Selection {
    // fn core(self: Selection, screen: *const terminal.Screen) ?terminal.Selection {
    //     return .{
    //         .bounds = .{ .untracked = .{
    //             .start = self.tl.pin(screen) orelse return null,
    //             .end = self.br.pin(screen) orelse return null,
    //         } },
    //         .rectangle = self.rectangle,
    //     };
    // }
    pub fn core(self, screen: &Screen) -> Option<selection::Selection> {
        Some(selection::Selection {
            bounds: selection::Bounds::Untracked {
                start: self.tl.pin(screen)?,
                end: self.br.pin(screen)?,
            },
            rectangle: self.rectangle,
        })
    }
}

// Reference the conditional exports based on target platform
// so they're included in the C API.
// comptime {
//     if (builtin.target.os.tag.isDarwin()) {
//         _ = Darwin;
//     }
// }
//
// PORT NOTE: the reference `comptime` block force-references the `Darwin` decl so
// its `export fn`s are pulled into the C API on Darwin targets. In rustty the
// Darwin CAPI exports (E10 — `ghostty_surface_set_display_id`/`_quicklook_*`/
// `ghostty_inspector_metal_*`) are `#[cfg(target_os = "macos")]`-gated free
// functions that the compiler includes automatically when targeting Darwin, so no
// force-reference is needed; this block has no runtime analogue.
