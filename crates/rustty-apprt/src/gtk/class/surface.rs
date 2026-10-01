//! rustty-apprt::gtk::class::surface
//!
//! PORTS: ghostty `src/apprt/gtk/class/surface.zig` (`GhosttySurface`, 4127 loc) —
//! **the tier's heart** (PREPORT_GTK §1.3). A `#[glib::object_subclass]` +
//! `#[derive(CompositeTemplate)]` over `adw::Bin`, implementing `gtk::Scrollable`,
//! that wraps the terminal's `RenderSurface` (P7-W7 T2; a `GLArea` at the pin)
//! and (once initialized) the ONE OWNED libghostty-core `Surface`.
//!
//! # Scope of this task (L5.2 T13 — the subclass skeleton + `init`)
//!
//! This file ports the **structure**: the [`imp::Surface`] private per §1.3, the
//! class-init (`ensure_type` for the four template subclasses, the template from
//! `ui/1.2/surface.ui`, the 13 template children, the 39 template callbacks, the
//! 24 properties, the 9 signals — `surface.zig:3593-3706`), and `new` / `core` /
//! `rt` / `set_parent` / `redraw` / `init` / `init_action_map`
//! (`surface.zig:747-825,1784-1853`).
//!
//! Per the placeholder-leaf doctrine, the 39 template callbacks land as
//! `unimplemented!()` INHABITED placeholders (Tasks 16/17/19 fill them) — **except**
//! the three that are structurally reached during template instantiation, which
//! must return safe values instead of panicking: the two `bind …`-time closures
//! `should_border_be_shown` / `should_unfocused_split_be_shown`
//! (`surface.zig:829-860`), and `notify_config` (`propConfig`), which `init`
//! calls at `:1832` — a no-op here until T16 applies the config to the overlays.
//!
//! The OWNED private fields' **free-sites** (config `unref`, the boxed sizes, the
//! glib source ids, `core_surface` `deinit`+destroy) land in **L5.2 T14/T15** —
//! this task only declares them at their defaults. The single behavior we DO
//! reproduce now is `dispose`'s template double-free workaround (**invariant 6**,
//! `surface.zig:1905-1911`): unsetting the `adw::Bin` child before
//! `dispose_template`, because `terminal_page` is both a template child and the
//! Bin's root child — without it, dropping a `Surface` double-frees.
//!
//! # The invariants this skeleton is measured against (PREPORT_GTK §2)
//!
//! * **(3) `core_surface` realize ordering** — `core_surface` starts null and is
//!   NOT constructed here; it is lazily built on the first render-surface resize
//!   (T15).
//!   [`Surface::core`] returns `Some`/`None` from the raw pointer (null => `None`).
//! * **(9) IME buffer consistency** — `in_keyevent`/`im_composing`/`im_buf`/`im_len`
//!   are kept as ONE [`ImeState`] struct, never scattered.
//! * **(10) `action_group` self-reference** — `init_action_map` installs the group
//!   via [`crate::gtk::ext::actions::add_as_group`], whose closures hold a WEAK
//!   backref to the widget (the group's lifetime ⊆ the widget's).
//! * **(12) floating-ref sink** — `glib::Object::builder().build()` sinks the
//!   floating ref exactly as `gobject.ext.newInstance` does.

use rustty_global::EnvMapExt as _;
use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::time::Duration;

use glib::prelude::*;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::CompositeTemplate;
use libadwaita::prelude::BinExt;
use libadwaita::subclass::prelude::*;

use rustty_apprt_api::{
    ApprtError, Clipboard, ClipboardContent, ClipboardReadResult, ClipboardRequest, ContentScale,
    CursorPos, NewSurfaceContext, SurfaceSize,
};
use rustty_config::command::Command;
use rustty_config::value_types::{ClipboardAccess, ShellIntegration, WorkingDirectory};
use rustty_core::surface::{Error as CoreSurfaceError, InputEffect};
use rustty_font_shaper::face::DesiredSize;
use rustty_vt::input::key::{Action, Key, KeyEvent};
use rustty_vt::input::key_mods::Mods;
use rustty_vt::input::mouse::{Button, ButtonState, ScrollMods};
use rustty_vt::mouse::Shape as MouseShape;

use super::gtypes::{ClipboardEnum, MouseShapeEnum};

use crate::gtk::app::App;
use crate::gtk::ext;
use crate::gtk::gsettings;
use crate::gtk::i18n::tr;
use crate::gtk::key;
use crate::gtk::surface::Surface as ApprtSurface;

use super::application::Application;
use super::clipboard_confirmation_dialog::{ClipboardConfirmationDialog, ClipboardRequestBoxed};
use super::config::Config;
use super::inspector_window::InspectorWindow;
use super::key_state_overlay::KeyStateOverlay;
use super::resize_overlay::ResizeOverlay;
use super::search_overlay::SearchOverlay;
use super::split_tree::SplitTree;
// `surface.zig:59-60`:
// /// A SplitTree implementation that stores surfaces.
// pub const Tree = datastruct.SplitTree(Self);
// PORT NOTE: the alias lives in `split_tree.rs` (`SplitTree`), not here.
use super::surface_child_exited::SurfaceChildExited;
// const RenderSurface = @import("render_surface.zig").RenderSurface; (`2f0b65346`)
use super::render_surface::RenderSurface;
// const scale_util = @import("../scale.zig");
use crate::gtk::scale;

/// PORTS: `Tree.Split.Direction` (`datastruct/split_tree.zig`) as named at the
/// B6 DnD call sites (`calcDropDirection`/`moveSplit`).
use rustty_datastruct::split_tree::Direction as SplitDirection;

/// `const log = std.log.scoped(.gtk_ghostty_surface)` (`surface.zig:42`).
/// REBRANDED per the task-14d-2 precedent (`.port/tools/PARITY_NAMING.md`); the
/// GType registered NAME in this file is deliberately NOT rebranded.
const LOG_SCOPE: &str = "gtk_rustty_surface";

/// The core surface backing a GTK `Surface`. PORTS: `surface.zig:20`
/// `const CoreSurface = @import("../../../Surface.zig")`; here the apprt backend's
/// core is `rustty_core::surface::Surface<App>` (the faithful analogue of
/// `Surface.core_surface: CoreSurface`, §1.3 `core_surface`).
pub type CoreSurface = rustty_core::surface::Surface<App>;

/// The compiled surface template resource. Equals
/// `gresource::blueprint(1, 2, "surface")` (`surface.zig:3600-3604`); the
/// `#[template]` attribute needs a literal, so the equality is asserted in the
/// tests rather than const-folded from [`crate::gtk::gresource::blueprint`].
#[cfg(test)]
const TEMPLATE_RESOURCE: &str = "/io/airely/rustty/ui/1.2/surface.ui";

/// The state of the key event while we're doing IM composition.
/// See gtkKeyPressed for detailed descriptions.
///
/// PORTS: `surface.zig:657` `in_keyevent: IMKeyEvent`. Whether we are currently
/// inside a key-event dispatch (gates the IME state transitions, **invariant 9**).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImKeyEvent {
    /// Not in a key event (`IMKeyEvent.false`).
    #[default]
    False,
    /// In a key event but im_composing was either true or false
    /// prior to the calling IME processing. This is important to
    /// work around different input methods calling commit and
    /// preedit end in a different order.
    ///
    /// In a key event whose composition has not yet been handled.
    True,
    /// In a key event whose composition has been handled.
    Composing,
}

/// PORTS: the coherent IME state group `surface.zig:657-661` —
/// `in_keyevent`/`im_composing`/`im_buf:[128]u8`/`im_len:u7`. **Invariant 9**
/// keeps these as ONE struct (never scattered fields) with the load-bearing
/// bound `im_len ≤ 128`.
#[derive(Debug)]
pub struct ImeState {
    /// PORTS: `surface.zig:657` `in_keyevent`.
    pub in_keyevent: ImKeyEvent,
    /// PORTS: `surface.zig:659` `im_composing`.
    pub im_composing: bool,
    /// PORTS: `surface.zig:660` `im_buf: [128]u8`.
    pub im_buf: [u8; 128],
    /// PORTS: `surface.zig:661` `im_len: u7` — the valid prefix length of
    /// [`ImeState::im_buf`]; the `u7` upper bound (`≤ 128`) survives as this
    /// `u8` field the transitions keep `≤ 128`.
    pub im_len: u8,
}

impl Default for ImeState {
    fn default() -> Self {
        Self {
            in_keyevent: ImKeyEvent::False,
            im_composing: false,
            im_buf: [0; 128],
            im_len: 0,
        }
    }
}

/// Simple dimensions struct for the surface used by various properties.
///
/// PORTS: `surface.zig:3715-3723` `pub const Size` — the boxed dimensions type
/// (`GhosttySurfaceSize`) shared by the `default-size`/`min-size` properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, glib::Boxed)]
#[boxed_type(name = "GhosttySurfaceSize", nullable)]
pub struct Size {
    /// PORTS: `surface.zig:3716` `width: u32`.
    pub width: u32,
    /// PORTS: `surface.zig:3717` `height: u32`.
    pub height: u32,
}

/// PORTS: `surface.zig:583` `font_size_request: ?*font.face.DesiredSize`. A boxed
/// wrapper so the requested font size can ride the `font-size-request` GObject
/// property (the reference boxes `font.face.DesiredSize` directly via
/// `privateBoxedFieldAccessor`). The GType is the reference's
/// `DesiredSize.getGObjectType`, `defineBoxed(DesiredSize, .{ .name =
/// "GhosttyFontDesiredSize" })` (`src/font/face.zig:63-68`); S87-F4: it was
/// registered as `GhosttyFontSizeRequest`.
#[derive(Debug, Clone, Copy, glib::Boxed)]
#[boxed_type(name = "GhosttyFontDesiredSize", nullable)]
pub struct FontSizeRequest(pub DesiredSize);

/// PORTS: `surface.zig:747-752` the `new` overrides — the caller-supplied
/// command / working-directory / title that seed a freshly created surface.
/// (No `Debug`: `rustty_config::command::Command` is not `Debug`.)
#[derive(Default)]
pub struct SurfaceOverrides {
    /// PORTS: `surface.zig:748` `command: ?configpkg.Command`.
    pub command: Option<Command>,
    /// PORTS: `surface.zig:749` `shell_integration:
    /// ?configpkg.Config.ShellIntegration` (3650bef4e).
    pub shell_integration: Option<ShellIntegration>,
    /// PORTS: `surface.zig:750` `working_directory: ?[:0]const u8`.
    pub working_directory: Option<String>,
    /// PORTS: `surface.zig:751` `title: ?[:0]const u8` (the `title-override`
    /// construct property).
    pub title: Option<String>,
}

// The subclass, the `impl Surface` topic blocks, the template callbacks and
// the tests live in `surface/` (P7-W13 K). `imp` keeps its module name, so it
// stays reachable as `class::surface::imp`.
mod imp;

// PORTS: `gobject.SignalGroup.{new,connect,setTarget}` (`surface.zig:2452-2464`,
// `1869`). The `glib` crate's typed `SignalGroup` and its `gobject_ffi`
// bindings are all gated behind the `v2_72` glib feature this build does not
// enable (see the `vadj_signal_group` field note), so `on_notify_vadjustment`
// (build/connect) and `dispose` (detach) bind the libgobject-2.0 C symbols
// directly. They have existed since GLib 2.72; this build links 2.88.
// `GSignalGroup*` is ABI-compatible with `GObject*`.
extern "C" {
    fn g_signal_group_new(target_type: glib::ffi::GType) -> *mut glib::gobject_ffi::GObject;
    fn g_signal_group_connect_closure(
        self_: *mut glib::gobject_ffi::GObject,
        detailed_signal: *const std::os::raw::c_char,
        closure: *mut glib::gobject_ffi::GClosure,
        after: glib::ffi::gboolean,
    );
    fn g_signal_group_set_target(
        self_: *mut glib::gobject_ffi::GObject,
        target: *mut glib::gobject_ffi::GObject,
    );
    fn g_signal_group_block(self_: *mut glib::gobject_ffi::GObject);
    fn g_signal_group_unblock(self_: *mut glib::gobject_ffi::GObject);
}

/// RAII guard reproducing the reference's `group.block(); defer group.unblock();`
/// around `setScrollbar`'s `configure` (`surface.zig:1017-1018`). Holds a strong
/// ref to the untyped `GSignalGroup` for the guard's life so the unblock always
/// targets the same live group.
struct SignalGroupBlock(glib::Object);

impl SignalGroupBlock {
    fn new(group: glib::Object) -> Self {
        // SAFETY: `group` is a live `GSignalGroup` (`GSignalGroup*` is
        // ABI-compatible with `GObject*`); `g_signal_group_block` is always valid.
        unsafe { g_signal_group_block(group.as_ptr().cast()) };
        Self(group)
    }
}

impl Drop for SignalGroupBlock {
    fn drop(&mut self) {
        // SAFETY: `self.0` is the same live `GSignalGroup` we blocked in `new`;
        // block/unblock are refcounted and balanced.
        unsafe { g_signal_group_unblock(self.0.as_ptr().cast()) };
    }
}

/// Removes a glib source id in `dispose`, warning (not panicking) on failure —
/// the faithful shape of the reference's `if (glib.Source.remove(id) == 0)
/// log.warn(...)` (`surface.zig:1884-1903`, **invariant 11**). Called with the
/// already-`take()`n `Option<SourceId>` so the field is nulled by the caller.
///
/// `g_source_remove` is used directly (not [`glib::SourceId::remove`], which
/// panics on a stale id) so a source that already fired warns-and-continues,
/// exactly like the reference and like the sibling `Application::dispose`.
/// `SourceId`'s own drop does not remove the source, so the scope-end drop of
/// `id` after this is a no-op.
fn remove_source_id(id: Option<glib::SourceId>, what: &str) {
    if let Some(id) = id {
        // SAFETY: `id.as_raw()` is a source id previously returned by glib; passing
        // it to `g_source_remove` is always valid (returns FALSE if already gone).
        let removed = unsafe { glib::ffi::g_source_remove(id.as_raw()) };
        if removed == glib::ffi::GFALSE {
            log::warn!(target: LOG_SCOPE, "unable to remove {what}");
        }
    }
}

/// Removes a glib source id and DISCARDS the result — the reference's
/// `_ = glib.Source.remove(v);` (`surface.zig:3118`, `:3134`, the horizontal
/// tab-scroll reset source), which logs nothing.
///
/// PORT NOTE (task tail-s1b, N203): these two call sites went through
/// [`remove_source_id`] and so warned `unable to remove pending horizontal
/// scroll reset source` on a failure the reference ignores silently. That
/// warning is `dispose`'s (`surface.zig:1936`), not the scroll handler's.
fn discard_source_id(id: Option<glib::SourceId>) {
    if let Some(id) = id {
        // SAFETY: as in `remove_source_id` — a glib-issued id; `g_source_remove`
        // returns FALSE (and removes nothing) if the source is already gone.
        let _ = unsafe { glib::ffi::g_source_remove(id.as_raw()) };
    }
}

/// PORTS: the `defer priv.im_len = 0` at `surface.zig:1349` — resets the IME
/// buffer length to 0 on EVERY exit of [`Surface::key_event`] past the IM block.
/// A RAII guard reproduces the reference's function-scoped `defer` exactly
/// (fires on the fall-through, the error return, and the effect-arm returns),
/// **invariant 9**.
struct ImLenReset<'a>(&'a RefCell<ImeState>);

impl Drop for ImLenReset<'_> {
    fn drop(&mut self) {
        self.0.borrow_mut().im_len = 0;
    }
}

/// PORTS: `surface.zig:3740-3755` `translateMouseButton`. Maps a GTK button
/// number (1 = left, 2 = middle, 3 = right, …) to rustty's [`Button`].
fn translate_mouse_button(button: u32) -> Button {
    match button {
        1 => Button::Left,
        2 => Button::Middle,
        3 => Button::Right,
        4 => Button::Four,
        5 => Button::Five,
        6 => Button::Six,
        7 => Button::Seven,
        8 => Button::Eight,
        9 => Button::Nine,
        10 => Button::Ten,
        11 => Button::Eleven,
        _ => Button::Unknown,
    }
}

/// Compute a fraction [0.0, 1.0] from the supplied progress, which is clamped
/// to [0, 100].
///
/// PORTS: `surface.zig:4116-4120` `computeFraction`. Computes a fraction in
/// `[0.0, 1.0]` from the supplied progress, clamped to `[0, 100]`.
///
/// PORT NOTE: consumed by the progress-bar handler [`Surface::set_progress_report`].
fn compute_fraction(progress: u8) -> f64 {
    f64::from(progress.min(100)) / 100.0
}

/// PORTS: the **invariant 5** ordering of `finalize`'s core-surface teardown
/// (`surface.zig:1925-1940`): `deleteSurface` MUST run before `deinit` ("in case
/// a callback triggers searching for this surface", `:1926-1929`).
///
/// Extracted from [`imp::Surface`]'s `Drop` so the ordering is unit-testable
/// without constructing a live `CoreSurface` — that requires the lazy render-surface
/// realize path (T15) and the renderer wiring (T19), neither of which exists in
/// this tier, so `core_surface` is always `None` here and the block is dormant at
/// runtime. `present` mirrors the reference's `if (priv.core_surface) |v|` guard.
fn finalize_core_surface(
    present: bool,
    delete_surface: impl FnOnce(),
    deinit_and_destroy: impl FnOnce(),
) {
    if !present {
        return;
    }
    delete_surface();
    deinit_and_destroy();
}

/// PORTS: `surface.zig:1929` `Application.default().core().deleteSurface(self.rt())`.
///
/// The `Application` GObject stores its `core_app` type-erased as `*mut c_void`
/// (it is non-generic; see the `class::application` field notes), so — unlike the
/// reference's comptime-monomorphized `core()` — this reconstitutes the concrete
/// `rustty_core::app::App<App>` from that pointer before calling `deleteSurface`.
/// No-op when the core app is unbound (`core_app` null), e.g. in widget tests.
fn delete_surface_from_app(rt: *mut ApprtSurface) {
    let app = Application::default();
    let Some(core_app) = app.imp().core_app.get() else {
        return;
    };
    let core_app = core_app.as_ptr().cast::<rustty_core::app::App<App>>();
    // SAFETY: when a core surface exists, `bind_core_app` has installed a live
    // `App<App>` at `core_app`, and `rt` points at this widget's embedded
    // `rt_surface` (still alive during finalize). Reached only with a realized
    // surface (never in this tier).
    unsafe { (*core_app).deleteSurface(rt) };
}

/// PORTS: `surface.zig:3438-3441` `InitError = Allocator.Error ||
/// error{SurfaceError}` (`2f0b65346` dropped `GLAreaError`). The only consumer
/// ([`Surface::render_surface_resize`]) logs it, so the variant carries no
/// payload.
// task-14d-3: `Debug` is required by the restored
// `log.warn("surface failed to initialize err={}", .{err})`
// (`surface.zig:3502`) — the port had dropped the error value, so nothing
// needed to render it. Semver-additive, no layout/ABI effect, and the only new
// capability is the one the reference already exercises at that call site.
#[derive(Debug)]
enum InitError {
    /// `error.SurfaceError` — `CoreSurface.init` failed, or (rustty) the core
    /// app / rt app is not bound so no surface can be constructed.
    Surface,
}

/// The concrete core app backing [`Application::default`], reconstituted from the
/// singleton's type-erased `core_app` cell (`app.core()`, `surface.zig:3434`).
/// `None` when the core app is unbound (e.g. widget tests) — the callers then
/// bail rather than construct a surface without a live app.
fn app_core_ptr() -> Option<*mut rustty_core::app::App<App>> {
    Application::default()
        .imp()
        .core_app
        .get()
        .map(|p| p.as_ptr().cast::<rustty_core::app::App<App>>())
}

/// The concrete apprt `App` backing [`Application::default`], reconstituted from
/// the singleton's type-erased `rt_app` cell (`app.rt()`, `surface.zig:3469`).
fn app_rt_ptr() -> Option<*mut App> {
    Application::default()
        .imp()
        .rt_app
        .get()
        .map(|p| p.as_ptr().cast::<App>())
}

glib::wrapper! {
    /// PORTS: `surface.zig:42-...` `pub const Surface` (`GhosttySurface`). The GTK
    /// per-surface widget: an `adw::Bin` wrapping the terminal `RenderSurface` and the
    /// lazily-built core surface. See the module doc for the T13 scope.
    pub struct Surface(ObjectSubclass<imp::Surface>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Scrollable;
}

impl Surface {
    /// PORTS: `surface.zig:747-764` `new`. Creates a new surface, cloning the
    /// caller's overrides (the reference clones with `Application.default()
    /// .allocator()`; in rustty the owned values move in). The `title-override` is
    /// seeded as a construct property (`:755`).
    #[must_use]
    pub fn new(overrides: SurfaceOverrides) -> Self {
        // **Invariant 12**: `glib::Object::builder().build()` sinks the floating
        // ref (`gobject.ext.newInstance`, `:754`).
        let obj: Self = glib::Object::builder()
            .property("title-override", overrides.title)
            .build();

        let imp = obj.imp();
        // `priv.overrides = .{ .command = …clone, .shell_integration = …,
        // .working_directory = …dupeZ }` (`surface.zig:759-764`).
        imp.overrides_command.replace(overrides.command);
        imp.overrides_shell_integration
            .set(overrides.shell_integration);
        imp.overrides_working_directory.replace(
            overrides
                .working_directory
                .and_then(|wd| CString::new(wd).ok()),
        );

        obj
    }

    /// PORTS: `surface.zig:854-874` `bindIsSplit` (B6, 1d053bd6e; `6cd684d5d`
    /// stale-pointer fix). (Re)binds this surface's `is-split` property to the
    /// given tree's, unbinding AND releasing any stale binding first (the
    /// cross-tree-move re-bind).
    pub fn bind_is_split(&self, tree: &super::split_tree::SplitTree) {
        let imp = self.imp();
        // `if (priv.is_split_binding) |binding| { binding.unbind();
        // binding.unref(); priv.is_split_binding = null; }` — `take()` nulls the
        // field, `unbind()` releases the binding's own ref, and the scope-end
        // drop is our `unref()`.
        if let Some(binding) = imp.is_split_binding.take() {
            binding.unbind();
        }

        // `const binding = tree.bindProperty("is-split", self, "is-split",
        // .{ .sync_create = true });`
        let binding = tree
            .bind_property("is-split", self, "is-split")
            .sync_create()
            .build();
        // The ref created by bindProperty is owned by the binding itself.
        // We need another ref to prevent the binding object from being
        // freed if the source object (SplitTree) is finalized. Otherwise
        // our pointer to the binding could become stale.
        //
        // PORT NOTE (`6cd684d5d`): the reference's explicit `binding.ref()` is
        // what gtk-rs's `build()` already does — `g_object_bind_property`'s
        // (transfer none) result comes back through `from_glib_none`, i.e. a
        // strong `glib::Binding` we own on top of the binding's own ref. So the
        // pointer this field holds was never stale in the port; the END STATE's
        // `unbind()`+`unref()` pairs (here and in `dispose`) are what changed.
        imp.is_split_binding.replace(Some(binding));
    }

    /// PORTS: `surface.zig:766-769` `core` — `return priv.core_surface` (a raw
    /// `?*CoreSurface`). The core surface backing this widget, or `None` before the
    /// first render-surface resize (**invariant 3**). The returned pointer is valid only
    /// while `core_surface` stays published (non-null).
    ///
    /// SOUNDNESS: this is a cross-thread read — the render/io threads reach it via
    /// the apprt seam (`gtk::Surface::core()`). It is a single atomic pointer load
    /// (`Acquire`, pairing with `init_surface`'s `Release` publish), which is sound
    /// cross-thread and lock-free, matching the reference's raw-pointer read. The
    /// heap slot is pinned; the pointer is cleared only after the render/io threads
    /// are joined (finalize, **invariant 5**), so a non-null result is always live.
    #[must_use]
    pub fn core(&self) -> Option<*mut CoreSurface> {
        let ptr = self.imp().core_surface.load(Ordering::Acquire);
        if ptr.is_null() {
            None
        } else {
            Some(ptr)
        }
    }

    /// Read `core_surface` from a RAW `GhosttySurface` instance pointer, WITHOUT
    /// reconstructing a refcounting GObject wrapper — the faithful analogue of the
    /// reference reading `priv.core_surface` through the raw `*Surface`
    /// (`surface.zig:645-648,767-769`).
    ///
    /// This is the ONLY finalize-safe way for the apprt seam's `core()`
    /// ([`crate::gtk::surface::Surface::core`]) to read the pointer: `deleteSurface`
    /// calls it during the widget's own finalize (`surface.zig:1929`), when the
    /// instance refcount is already 0 — where the wrapper reconstitution
    /// (`from_glib_none`/`from_glib_borrow`) aborts on glib's `ref_count != 0`
    /// debug assertion. A bare instance-struct borrow + atomic load touches no
    /// refcount, so it is sound both live (render thread) and mid-finalize.
    ///
    /// # Safety
    /// `obj` must be a live `GhosttySurface` instance pointer — its GObject header
    /// and private data allocated (true even mid-finalize, before the instance slot
    /// is freed).
    #[must_use]
    pub(crate) unsafe fn core_surface_ptr(
        obj: *mut glib::gobject_ffi::GObject,
    ) -> *mut CoreSurface {
        // The `GhosttySurface` instance struct (its GObject header is the first
        // field, so the cast from `*mut GObject` is layout-valid).
        let instance = obj.cast::<<imp::Surface as ObjectSubclass>::Instance>();
        // SAFETY: `instance` is a live `GhosttySurface` instance; `imp()` borrows
        // its embedded private `imp::Surface` (a plain field of the C instance
        // struct — NO GObject refcount). The `&imp::Surface` is used only to load
        // the atomic pointer (`Acquire`, pairing with `init_surface`'s `Release`
        // publish) and is not retained past this read.
        let imp: &imp::Surface = unsafe { (*instance).imp() };
        imp.core_surface.load(Ordering::Acquire)
    }

    /// PORTS: `surface.zig:771-774` `rt`. The embedded apprt `Surface` backref.
    #[must_use]
    pub fn rt(&self) -> *const ApprtSurface {
        std::ptr::from_ref(&self.imp().rt_surface)
    }

    /// Set the parent of this surface. This will extract the information
    /// required to initialize this surface with the proper values but doesn't
    /// retain any memory.
    ///
    /// If the surface is already realized this does nothing.
    ///
    /// PORTS: `surface.zig:781-816` `setParent`. Extracts init values from a
    /// parent core surface **only before this surface is realized**
    /// (**invariant 3**): the font size is always inherited, and the working
    /// directory is inherited when the config says so.
    ///
    /// # Safety
    /// `parent` must point to a live [`CoreSurface`].
    pub unsafe fn set_parent(&self, parent: *mut CoreSurface, context: NewSurfaceContext) {
        let imp = self.imp();

        // This is a mistake! We can only set a parent before surface
        // realization. We log this because this is probably a logic error.
        // "We can only set a parent before surface realization." (`:788-793`).
        if !imp.core_surface.load(Ordering::Acquire).is_null() {
            // log.warn("setParent called after surface is already realized", .{});
            log::warn!(target: LOG_SCOPE, "setParent called after surface is already realized");
            return;
        }

        // Store the context so initSurface can use it
        // Store the context so the lazy `initSurface` (T15) can use it (`:795-796`).
        imp.context.set(context);

        // SAFETY: caller guarantees `parent` is live.
        let parent_ref: &CoreSurface = unsafe { &*parent };

        // Inherit the font size (`surface.zig:798-803`).
        imp.font_size_request
            .replace(Some(FontSizeRequest(parent_ref.font_size)));
        self.notify("font-size-request");

        // Remainder needs a config. If there is no config we just assume
        // we aren't inheriting any of these values (`:806-817`).
        if let Some(config_obj) = imp.config.borrow().as_ref() {
            // Setup our cwd if configured to inherit
            if rustty_apprt_api::should_inherit_working_directory(context, config_obj.get()) {
                // if (parent.rt_surface.surface.getPwd()) |pwd| { ... }
                //
                // CORRECTION (task 20, S54-F1): this read `parent_ref.pwd(alloc)`,
                // the CORE surface's terminal pwd (taken under the renderer mutex).
                // The reference reads the parent WIDGET's `pwd` property, which is
                // also set by the parent's own inheritance at creation — so a parent
                // that inherited a directory but whose shell has not yet sent OSC 7
                // passes it on in the reference and passed nothing here.
                // SAFETY: `parent` is live (caller contract), so its `rt_surface` is
                // the pinned apprt surface embedded in the parent widget's private.
                if let Some(pwd) = unsafe { parent_widget_pwd(parent_ref.rt_surface) } {
                    imp.pwd.replace(Some(pwd));
                    self.notify("pwd");
                }
            }
        }
    }

    /// Force the surface to redraw itself. Rustty often will only redraw
    /// the terminal in reaction to internal changes. If there are external
    /// events that invalidate the surface, such as the widget moving parents,
    /// then we should force a redraw.
    ///
    /// PORTS: `surface.zig:815-820` `redraw` (`2f0b65346`: `queueDraw` on the
    /// `RenderSurface`, whose snapshot takes the renderer's latest frame). This is
    /// the app/main-thread leg of the **T16 render contract**: the core surface's
    /// `.redraw` message (a frame was pushed) lands here through `.render`.
    pub fn redraw(&self) {
        self.imp().render_surface.queue_draw();
    }

    /// PORTS: `surface.zig:1835-1853` `initActionMap`. Installs the `surface`
    /// action group whose callbacks receive a WEAK backref to this widget
    /// (**invariant 10**: group lifetime ⊆ widget lifetime).
    fn init_action_map(&self) {
        let actions = [
            // `prompt-title` (`surface.zig:1839-1843`).
            ext::actions::Action::<Surface>::init("prompt-title", action_prompt_title, None),
            // `notify-on-next-command-finish`, stateful boolean(false)
            // (`surface.zig:1844-1849`).
            ext::actions::Action::<Surface>::init_stateful(
                "notify-on-next-command-finish",
                action_notify_on_next_command_finish,
                None,
                false.to_variant(),
            ),
        ];

        // `priv.action_group = ext.actions.addAsGroup(Self, self, "surface", &actions)`
        // (`surface.zig:1852`).
        let group = ext::actions::add_as_group(self, "surface", &actions);
        self.imp().action_group.replace(Some(group));
    }
}

mod clipboard_env;
mod core_callbacks;
mod input;

// PORT NOTE: the reference's typed `getMouseShape`/`setMouseShape`
// (`surface.zig:2176-2184`) over `terminal.MouseShape` are the generated
// `mouse_shape()`/`set_mouse_shape()` accessors, typed by the `GhosttyMouseShape`
// GObject enum (`gtypes.rs` `MouseShapeEnum`).
// CORRECTION (parity S156-F2): this note said the enum and the typed accessors
// were deferred and that the accessors were `i32`. Both landed with task 20
// (S54-F3); the note was not updated then.

/// PORTS: `surface.zig:2575-2584` `actionPromptTitle`. Perform the
/// `prompt_surface_title` binding action on the core surface; the action loops
/// back through the apprt `PromptTitle::Surface` seam to open the `TitleDialog`.
fn action_prompt_title(
    _action: &gio::SimpleAction,
    _param: Option<&glib::Variant>,
    this: &Surface,
) {
    // `const surface = self.core() orelse return` (`:2580`).
    let Some(core) = this.core() else {
        return;
    };
    // `surface.performBindingAction(.prompt_surface_title)` (`:2581`).
    if let Err(err) = unsafe {
        CoreSurface::performBindingAction(core, rustty_input::binding::Action::PromptSurfaceTitle)
    } {
        log::warn!(target: LOG_SCOPE, "unable to perform prompt title action err={err:?}");
    }
}

/// PORTS: `surface.zig:2586-2597` `actionNotifyOnNextCommandFinish`. Flip the
/// stateful boolean of the `notify-on-next-command-finish` gio action, arming (or
/// disarming) the one-shot "notify me when the next command finishes" override that
/// [`Surface::command_finished`] reads-and-resets.
fn action_notify_on_next_command_finish(
    action: &gio::SimpleAction,
    _param: Option<&glib::Variant>,
    _self: &Surface,
) {
    use gio::prelude::*;
    // `state.getBoolean()` (default false if unset / not a bool), then `setState(!value)`.
    let value = action
        .state()
        .and_then(|s| s.get::<bool>())
        .unwrap_or(false);
    action.set_state(&(!value).to_variant());
}

// The 39 template callbacks (`surface.zig:3623-3661`). Per the placeholder-leaf
// doctrine most are `unimplemented!()` (Tasks 16/17/19 fill them); the three
// reached during template instantiation return safe values (see the module doc).
#[cfg(test)]
thread_local! {
    /// The last `ClipboardConfirmationDialog` `show_clipboard_confirmation` built on
    /// this thread (TEST-ONLY; see the UD172 test).
    static LAST_CLIPBOARD_CONFIRMATION: RefCell<Option<ClipboardConfirmationDialog>> =
        const { RefCell::new(None) };
}

/// A `G_TYPE_STRING` `GValue` holding `bytes` UNCONVERTED — what the reference's
/// `[*:0]const u8` signal argument is (`surface.zig:4117-4122`, `text.ptr`). A
/// GValue string does not require UTF-8 (`g_value_set_string` copies the bytes);
/// glib-rs's `String`/`&str` values do, which is why this goes through the C call.
/// C reads up to the first NUL, as it does for the reference's pointer.
///
/// A receiver must read it with `g_value_get_string`, never
/// `Value::get::<String>()` (which panics on non-UTF-8); the one in-tree receiver
/// is `window.rs`'s `clipboard-write` handler.
pub(crate) fn raw_string_value(bytes: &[u8]) -> glib::Value {
    use glib::translate::ToGlibPtrMut;
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let c = std::ffi::CString::new(&bytes[..end]).expect("no interior NUL before `end`");
    let mut value = glib::Value::from_type(glib::Type::STRING);
    // SAFETY: `value` is an initialized G_TYPE_STRING GValue; GLib copies `c`.
    unsafe { glib::gobject_ffi::g_value_set_string(value.to_glib_none_mut().0, c.as_ptr()) };
    value
}

/// The bytes of a `G_TYPE_STRING` `GValue`, read without UTF-8 validation (the
/// reader half of [`raw_string_value`]). `None` for a NULL string.
pub(crate) fn raw_string_bytes(value: &glib::Value) -> Option<Vec<u8>> {
    use glib::translate::ToGlibPtr;
    // SAFETY: `value` is a live GValue; `g_value_get_string` returns its own
    // NUL-terminated string (or NULL), valid while `value` is.
    unsafe {
        let p = glib::gobject_ffi::g_value_get_string(value.to_glib_none().0);
        (!p.is_null()).then(|| std::ffi::CStr::from_ptr(p).to_bytes().to_vec())
    }
}

/// Map a [`MouseShape`] to its W3C/CSS cursor name for
/// `gtk.Widget.set_cursor_from_name`.
///
/// PORTS: the `switch (priv.mouse_shape)` in `surface.zig:2482-2517`
/// (`propMouseShape`), arm for arm and in the reference's arm order.
///
/// CORRECTION (task T-D, S55-F4): this took the shape as its `i32` discriminant
/// and indexed a 34-entry name table in `mouse::Shape` declaration order, with an
/// out-of-range index falling back to `"default"` — a branch the reference's
/// exhaustive switch does not have. The `mouse-shape` property has carried the
/// typed `GhosttyMouseShape` since task 20 (S54-F3), so the map is the
/// reference's switch again; an unhandled shape is now a compile error.
fn mouse_shape_cursor_name(shape: MouseShape) -> &'static str {
    match shape {
        MouseShape::Default => "default",
        MouseShape::Help => "help",
        MouseShape::Pointer => "pointer",
        MouseShape::ContextMenu => "context-menu",
        MouseShape::Progress => "progress",
        MouseShape::Wait => "wait",
        MouseShape::Cell => "cell",
        MouseShape::Crosshair => "crosshair",
        MouseShape::Text => "text",
        MouseShape::VerticalText => "vertical-text",
        MouseShape::Alias => "alias",
        MouseShape::Copy => "copy",
        MouseShape::NoDrop => "no-drop",
        MouseShape::Move => "move",
        MouseShape::NotAllowed => "not-allowed",
        MouseShape::Grab => "grab",
        MouseShape::Grabbing => "grabbing",
        MouseShape::AllScroll => "all-scroll",
        MouseShape::ColResize => "col-resize",
        MouseShape::RowResize => "row-resize",
        MouseShape::NResize => "n-resize",
        MouseShape::EResize => "e-resize",
        MouseShape::SResize => "s-resize",
        MouseShape::WResize => "w-resize",
        MouseShape::NeResize => "ne-resize",
        MouseShape::NwResize => "nw-resize",
        MouseShape::SeResize => "se-resize",
        MouseShape::SwResize => "sw-resize",
        MouseShape::EwResize => "ew-resize",
        MouseShape::NsResize => "ns-resize",
        MouseShape::NeswResize => "nesw-resize",
        MouseShape::NwseResize => "nwse-resize",
        MouseShape::ZoomIn => "zoom-in",
        MouseShape::ZoomOut => "zoom-out",
    }
}

mod template_callbacks;

impl Surface {
    /// PORTS: `surface.zig:3811-3824` `setDropOverlayDirection` — swap the
    /// `drop-{top,left,right,bottom}` CSS class on the drop overlay.
    fn set_drop_overlay_direction(&self, dir: Option<SplitDirection>) {
        let overlay = &self.imp().drop_overlay;
        for c in ["drop-top", "drop-left", "drop-right", "drop-bottom"] {
            overlay.remove_css_class(c);
        }

        if let Some(d) = dir {
            overlay.add_css_class(match d {
                SplitDirection::Up => "drop-top",
                SplitDirection::Left => "drop-left",
                SplitDirection::Right => "drop-right",
                SplitDirection::Down => "drop-bottom",
            });
        }
    }

    /// PORTS: `surface.zig:3826-3840` `calcDropDirection` — nearest edge wins.
    fn calc_drop_direction(&self, x: f64, y: f64) -> SplitDirection {
        let width = f64::from(self.width());
        let height = f64::from(self.height());

        let l_dist = x / width;
        let t_dist = y / height;
        let r_dist = 1.0 - l_dist;
        let b_dist = 1.0 - t_dist;
        let min = l_dist.min(t_dist).min(r_dist).min(b_dist);

        if min == l_dist {
            return SplitDirection::Left;
        }
        if min == r_dist {
            return SplitDirection::Right;
        }
        if min == t_dist {
            return SplitDirection::Up;
        }
        SplitDirection::Down
    }
}

/// Apply command and shell integration overrides received from the CLI.
/// Explicit commands should only receive shell integration when their
/// executable can be detected as a supported shell. An explicit shell
/// integration override is also valid without a command.
//
// PORTS: `surface.zig:4401-4417` `applyCommandOverrides` (3650bef4e — the
// `+new-window`/`+new-tab` `-e <command>` forced-shell-integration fix). The
// reference clones the command with `config.arenaAlloc()`; rustty's `Command`
// owns its bytes, so `clone()` is the whole story.
fn apply_command_overrides(
    config: &mut rustty_config::Config,
    command: Option<&Command>,
    shell_integration: Option<ShellIntegration>,
) {
    if let Some(value) = command {
        // config.command = try value.clone(config.arenaAlloc());
        config.command = Some(value.clone());

        if let Some(integration) = shell_integration {
            config.shell_integration = integration;
        } else if config.shell_integration != ShellIntegration::None {
            config.shell_integration = ShellIntegration::Detect;
        }
    } else if let Some(value) = shell_integration {
        config.shell_integration = value;
    }
}

/// Filter out environment variables that start with forbidden prefixes.
///
/// PORTS: `surface.zig:1667-1740` `filterSnapPaths`. Filter out environment
/// variables that start with forbidden prefixes.
///
/// PORT NOTE: the reference opens with `comptime assert(build_config.snap)`. Rust
/// has no comptime assertion on a runtime-read build option, so the guarantee is
/// held by the only caller ([`Surface::default_termio_env`] checks
/// `rustty_build_config::snap()` first); the function itself is plain so the unit
/// test can drive it in a non-snap build. The reference's arena is the two owned
/// `Vec`s below, dropped on return.
fn filter_snap_paths(env_map: &mut rustty_global::EnvMap) {
    const SNAP_VARS: [&[u8]; 5] = [
        b"SNAP",
        b"SNAP_USER_COMMON",
        b"SNAP_USER_DATA",
        b"SNAP_DATA",
        b"SNAP_COMMON",
    ];

    // Use an arena because everything in this function is temporary.
    // PORT NOTE: the arena is these two owned `Vec`s, dropped on return.
    let mut env_to_remove: Vec<Vec<u8>> = Vec::new();
    let mut env_to_update: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();

    for (key, value) in env_map.iter() {
        // Ignore fields we set ourself
        if key.as_slice() == b"TERMINFO" {
            continue;
        }
        // `std.mem.startsWith(u8, key, "GHOSTTY")` — R2: the product env prefix.
        if key.starts_with(rustty_identity::ENV_PREFIX.as_bytes()) {
            continue;
        }

        // Any env var starting with SNAP must be removed
        if key.starts_with(b"SNAP_") {
            env_to_remove.push(key.clone());
            continue;
        }

        let mut filtered_paths: Vec<&[u8]> = Vec::new();
        let mut modified = false;
        // `std.mem.splitAny(u8, value, ":")`.
        for path in value.split(|&b| b == b':') {
            let mut include = true;
            for k in SNAP_VARS {
                if let Some(snap_path) = env_map.getv(k) {
                    if snap_path.is_empty() {
                        continue;
                    }
                    if path.starts_with(snap_path) {
                        include = false;
                        modified = true;
                        break;
                    }
                }
            }
            if include {
                filtered_paths.push(path);
            }
        }

        if modified {
            if filtered_paths.is_empty() {
                env_to_remove.push(key.clone());
            } else {
                // `std.mem.join(alloc, ":", filtered_paths.items)`.
                let new_value = filtered_paths.join(&b':');
                env_to_update.push((key.clone(), new_value));
            }
        }
    }

    for (key, value) in env_to_update {
        env_map.put(&key, &value);
    }
    for key in env_to_remove {
        env_map.removev(&key);
    }
}

/// `parent.rt_surface.surface.getPwd()` (`surface.zig:812`): the `pwd` property of
/// the widget that owns a core surface's apprt surface. `None` when the apprt
/// surface has no widget backref or the widget has no pwd.
///
/// # Safety
/// `rt_surface` must point at a live apprt surface (the pinned one embedded in a
/// `Surface` widget's private).
unsafe fn parent_widget_pwd(rt_surface: *mut ApprtSurface) -> Option<String> {
    // SAFETY: caller contract.
    unsafe { (*rt_surface).widget() }?.pwd()
}

#[cfg(test)]
mod tests;
