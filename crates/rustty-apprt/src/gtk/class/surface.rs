//! rustty-apprt::gtk::class::surface
//!
//! PORTS: ghostty `src/apprt/gtk/class/surface.zig` (`GhosttySurface`, 4127 loc) —
//! **the tier's heart** (PREPORT_GTK §1.3). A `#[glib::object_subclass]` +
//! `#[derive(CompositeTemplate)]` over `adw::Bin`, implementing `gtk::Scrollable`,
//! that wraps the terminal's `GLArea` and (once realized) the ONE OWNED
//! libghostty-core `Surface`.
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
//!   NOT constructed here; it is lazily built on the first GLArea resize (T15).
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

mod imp {
    use super::*;

    /// PORTS: `surface.zig:570-745` `Surface.Private` — the tier's heart
    /// (PREPORT_GTK §1.3). Every pointer-typed field is present with its §1.3
    /// class: `TemplateChild<T>` for BORROW_FIELD rows (released en masse by
    /// `dispose_template`, never unref'd by field), the OWNED rows at their
    /// defaults (free-sites in T14/T15), and `core_surface` — the ONE OWNED
    /// libghostty-core object — as `RefCell<Option<Box<CoreSurface>>>`, `None`
    /// until the first GLArea resize (**invariant 3**).
    #[derive(CompositeTemplate, glib::Properties)]
    #[template(resource = "/io/airely/rustty/ui/1.2/surface.ui")]
    #[properties(wrapper_type = super::Surface)]
    pub struct Surface {
        // ---- Properties (surface.zig:3664-3691; 24 registered) --------------
        /// This property is set to true when the bell is ringing. Note that
        /// this property will only emit a changed signal when there is a
        /// full state change. If a bell is ringing and another bell event
        /// comes through, the change notification will NOT be emitted.
        ///
        /// If you need to know every scenario the bell is triggered,
        /// listen to the `bell` signal instead.
        ///
        /// Private field (`surface.zig:674-676`):
        // True while the bell is ringing. This will be set to false (after
        // true) under various scenarios, but can also manually be set to
        // false by a parent widget.
        /// PORTS: `surface.zig:69-79,675` `bell-ringing` (privateShallow).
        #[property(get, set)]
        pub bell_ringing: Cell<bool>,

        /// PORTS: `surface.zig:82-91,572` `config` (`?*Config`, SHARED; obj
        /// accessor: get `.none` refs, set `.full` unrefs prior).
        #[property(get, set, nullable)]
        pub config: RefCell<Option<Config>>,

        /// True when the child has exited.
        /// PORTS: `surface.zig:94-109,667` `child-exited`.
        #[property(get, set)]
        pub child_exited: Cell<bool>,

        /// The default size for a window that embeds this surface.
        /// PORTS: `surface.zig:112-121,575` `default-size` (`?*Size`, OWNED boxed).
        #[property(get, set, nullable)]
        pub default_size: RefCell<Option<Size>>,

        /// True if this surface is in an error state. This is currently
        /// a simple boolean with no additional information on WHAT the
        /// error state is, because we don't yet need it or use it. For now,
        /// if this is true, then it means the terminal is non-functional.
        /// PORTS: `surface.zig:124-139,687` `error`.
        #[property(name = "error", get, set)]
        pub error: Cell<bool>,

        /// The requested font size. This only applies to initialization
        /// and has no effect later.
        /// PORTS: `surface.zig:142-152,583` `font-size-request` (OWNED boxed).
        #[property(get, set, nullable)]
        pub font_size_request: RefCell<Option<FontSizeRequest>>,

        /// The current focus state of the terminal based on the
        /// focus events.
        /// PORTS: `surface.zig:154-170,611` `focused`. The pspec default is `false`
        /// (`:161`) while the private field starts `true` (`:611`, see [`Default`]).
        ///
        /// CORRECTION (task 20, S54-F3): the attribute was `default = true`, which
        /// set the PSPEC default to the field's initial value; the reference's
        /// `g_param_spec_get_default_value` reads `false`.
        #[property(get, set)]
        pub focused: Cell<bool>,

        /// Whether the GLArea widget is mapped. Some operations like grabbing
        /// focus only work if a widget is mapped.
        /// PORTS: `surface.zig:172-188,615` `mapped`.
        #[property(get, set)]
        pub mapped: Cell<bool>,

        /// PORTS: `surface.zig:386-403,709` `key-sequence` (computed, getter
        /// transfer `.full`). Backed by [`Surface::key_sequence`].
        #[property(name = "key-sequence", get = Self::key_sequence_prop, nullable)]
        pub key_sequence_prop: std::marker::PhantomData<Option<ext::StringList>>,

        /// PORTS: `surface.zig:405-422,710` `key-table` (computed, getter
        /// transfer `.full`). Backed by [`Surface::key_tables`].
        #[property(name = "key-table", get = Self::key_table_prop, nullable)]
        pub key_table_prop: std::marker::PhantomData<Option<ext::StringList>>,

        /// The minimum size for this surface. Embedders enforce this,
        /// not the surface itself.
        /// PORTS: `surface.zig:190-200,579` `min-size` (`?*Size`, OWNED boxed).
        #[property(get, set, nullable)]
        pub min_size: RefCell<Option<Size>>,

        /// The mouse shape to show for the surface.
        /// PORTS: `surface.zig:222-240,586` `mouse-shape`, typed
        /// `terminal.MouseShape` with pspec default `.text`.
        ///
        /// CORRECTION (task 20, S54-F3): this was a `Cell<i32>` (pspec default 0)
        /// under a PORT NOTE saying the enum GType would come "until the mouse-input
        /// task (T16/T17) needs" it. The GType is observable now — introspection,
        /// `g_param_spec_get_default_value`, GtkBuilder — so the property carries the
        /// `GhosttyMouseShape` registration ([`MouseShapeEnum`]).
        #[property(name = "mouse-shape", get, set, builder(MouseShapeEnum::Text))]
        pub mouse_shape: Cell<MouseShapeEnum>,

        /// Whether the mouse should be hidden or not as requested externally.
        /// PORTS: `surface.zig:202-220,589` `mouse-hidden`.
        #[property(get, set)]
        pub mouse_hidden: Cell<bool>,

        /// The URL that the mouse is currently hovering over.
        /// PORTS: `surface.zig:242-253,592` `mouse-hover-url` (string; getter
        /// `.none`, setter `.full`).
        #[property(get, set, nullable)]
        pub mouse_hover_url: RefCell<Option<String>>,

        /// The current working directory. This has to be reported externally,
        /// usually by shell integration which then talks to librustty
        /// which triggers this property.
        ///
        /// If this is set prior to initialization then the surface will
        /// start in this pwd. If it is set after, it has no impact on the
        /// core surface.
        /// PORTS: `surface.zig:255-266,601` `pwd` (string).
        #[property(get, set, nullable)]
        pub pwd: RefCell<Option<String>>,

        /// The title of this surface, if any has been set.
        /// PORTS: `surface.zig:268-279,604` `title` (string).
        #[property(get, set, nullable)]
        pub title: RefCell<Option<String>>,

        /// The manually overridden title of this surface from `promptTitle`.
        /// PORTS: `surface.zig:281-292,607` `title-override` (string).
        #[property(name = "title-override", get, set, nullable)]
        pub title_override: RefCell<Option<String>>,

        /// Whether this surface is "zoomed" or not. A zoomed surface
        /// shows up taking the full bounds of a split view.
        /// PORTS: `surface.zig:294-310,619` `zoom`.
        #[property(get, set)]
        pub zoom: Cell<bool>,

        // True if the current surface is a split, this is used to apply
        // unfocused-split-* options
        /// PORTS: `surface.zig:312-328,697` `is-split`.
        #[property(name = "is-split", get, set)]
        pub is_split: Cell<bool>,

        /// PORTS: `surface.zig:424-441` `readonly` (computed, read-only). Reads the
        /// core surface's `readonly`; `false` before realize (**invariant 3**).
        #[property(name = "readonly", get = Self::readonly_prop)]
        pub readonly_prop: std::marker::PhantomData<bool>,

        // ---- Gtk.Scrollable interface (surface.zig:330-384,702-706) ---------
        /// PORTS: `surface.zig:330-343,702` `hadjustment` + the OWNED `hadj` field.
        #[property(get, set, override_interface = gtk4::Scrollable, nullable)]
        pub hadjustment: RefCell<Option<gtk4::Adjustment>>,

        /// PORTS: `surface.zig:345-358,703` `vadjustment` + the OWNED `vadj` field.
        #[property(get, set, override_interface = gtk4::Scrollable, nullable)]
        pub vadjustment: RefCell<Option<gtk4::Adjustment>>,

        /// PORTS: `surface.zig:360-371,704` `hscroll-policy` (default `.natural`).
        #[property(name = "hscroll-policy", get, set, override_interface = gtk4::Scrollable, builder(gtk4::ScrollablePolicy::Natural))]
        pub hscroll_policy: Cell<gtk4::ScrollablePolicy>,

        /// PORTS: `surface.zig:373-384,705` `vscroll-policy` (default `.natural`).
        #[property(name = "vscroll-policy", get, set, override_interface = gtk4::Scrollable, builder(gtk4::ScrollablePolicy::Natural))]
        pub vscroll_policy: Cell<gtk4::ScrollablePolicy>,

        // ---- Template children (13; BORROW_FIELD, surface.zig:3608-3620) ----
        // Never unref'd by field: released en masse by `dispose_template`.
        /// The GLArea that renders the actual surface. This is a binding
        /// to the template so it doesn't have to be unrefed manually.
        /// PORTS: `surface.zig:623` `gl_area` — the GL context source.
        #[template_child]
        pub gl_area: TemplateChild<gtk4::GLArea>,
        /// The labels for the left/right sides of the URL hover tooltip.
        /// PORTS: `surface.zig:626` `url_left`.
        #[template_child]
        pub url_left: TemplateChild<gtk4::Label>,
        /// PORTS: `surface.zig:627` `url_right`.
        #[template_child]
        pub url_right: TemplateChild<gtk4::Label>,
        /// PORTS: `surface.zig:713` `child_exited_overlay`.
        #[template_child]
        pub child_exited_overlay: TemplateChild<SurfaceChildExited>,
        /// PORTS: `surface.zig:714` `context_menu`.
        #[template_child]
        pub context_menu: TemplateChild<gtk4::PopoverMenu>,
        /// PORTS: `surface.zig:717` `error_page`.
        #[template_child]
        pub error_page: TemplateChild<libadwaita::StatusPage>,
        /// PORTS: `surface.zig:716` `progress_bar_overlay`.
        #[template_child]
        pub progress_bar_overlay: TemplateChild<gtk4::ProgressBar>,
        /// PORTS: `surface.zig:630` `resize_overlay`.
        #[template_child]
        pub resize_overlay: TemplateChild<ResizeOverlay>,
        /// PORTS: `surface.zig:633` `search_overlay`.
        #[template_child]
        pub search_overlay: TemplateChild<SearchOverlay>,
        /// PORTS: `surface.zig:636` `key_state_overlay`.
        #[template_child]
        pub key_state_overlay: TemplateChild<KeyStateOverlay>,
        /// PORTS: `surface.zig:718` `terminal_page`.
        #[template_child]
        pub terminal_page: TemplateChild<gtk4::Overlay>,
        /// PORTS: `surface.zig:715` `drop_target`.
        #[template_child]
        pub drop_target: TemplateChild<gtk4::DropTarget>,
        /// PORTS: `surface.zig:713` `surface_drop_target` (B6 split DnD,
        /// 1d053bd6e): the uint64 surface-id drop target.
        #[template_child]
        pub surface_drop_target: TemplateChild<gtk4::DropTarget>,
        /// PORTS: `surface.zig:714` `drag_handle` (B6).
        #[template_child]
        pub drag_handle: TemplateChild<gtk4::Widget>,
        /// PORTS: `surface.zig:715` `drop_overlay` (B6).
        #[template_child]
        pub drop_overlay: TemplateChild<gtk4::Widget>,
        /// PORTS: `surface.zig:658` `im_context`.
        #[template_child]
        pub im_context: TemplateChild<gtk4::IMMulticontext>,

        // ---- Non-property owned / backref state (surface.zig:639-742) -------
        /// PORTS: `surface.zig:639` `rt_surface: ApprtSurface` — the embedded apprt
        /// `Surface` backref (`Surface.zig:12`). The GObject private is heap-pinned,
        /// so this is the correct, never-moved home for the self-referential apprt
        /// surface (memory `mirihar-surface-pin`). Its `*Surface` widget backref is
        /// wired when the apprt `Surface` grows its handle (L5.1 T18).
        pub rt_surface: ApprtSurface,

        /// The core surface backing this GTK surface. This starts out
        /// null because it can't be initialized until there is an available
        /// GLArea that is realized.
        //
        // NOTE(mitchellh): This is a limitation we should definitely remove
        // at some point by modifying our OpenGL renderer for GTK to
        // start in an unrealized state. There are other benefits to being
        // able to initialize the surface early so we should aim for that,
        // eventually.
        /// PORTS: `surface.zig:650` `core_surface: ?*CoreSurface` — the ONE OWNED
        /// libghostty-core object. **Invariant 3**: null until the first GLArea
        /// resize (T15); free-site (`deinit` + destroy) in the finalize `Drop`.
        ///
        /// # Storage & thread-affinity (soundness)
        ///
        /// Stored as an [`AtomicPtr<CoreSurface>`] — the faithful Rust analogue of
        /// the reference's RAW optional pointer `?*CoreSurface` (`surface.zig:650`),
        /// NOT a `RefCell`. This is load-bearing for SOUNDNESS: the RENDER/IO
        /// threads read this field cross-thread (apprt `gtk::Surface::core()` →
        /// widget [`super::Surface::core`] → this field), while the MAIN/finalize
        /// thread also reads and writes it. A `RefCell` is single-threaded — a
        /// concurrent borrow from the render thread is unsound. An atomic pointer
        /// read/write is sound cross-thread with no locking (matching the
        /// reference's lock-free raw-pointer read; a `Mutex` here would change the
        /// locking model and could deadlock the render thread).
        ///
        /// OWNERSHIP: the heap allocation is a stable `Box` (the self-referential
        /// pin — `CoreSurface` is heap-boxed and never moves after `init`, so the
        /// pointers the renderer/io threads hold stay valid). At [`super::Surface::init_surface`]
        /// the box is published via `Box::into_raw` + a `Release` store; at finalize
        /// it is reclaimed via `Box::from_raw` + `deinit` (which JOINS the threads)
        /// and only THEN cleared to null (**invariant 5** ordering) — so no thread
        /// ever reads a cleared/freed pointer (the join-before-clear invariant).
        pub core_surface: AtomicPtr<CoreSurface>,

        /// Cached metrics for librustty callbacks
        /// PORTS: `surface.zig:653` `size: apprt.SurfaceSize` — cached metrics.
        pub size: Cell<SurfaceSize>,
        /// PORTS: `surface.zig:654` `cursor_pos: apprt.CursorPos`.
        pub cursor_pos: Cell<CursorPos>,

        /// Various input method state. All related to key input.
        /// PORTS: `surface.zig:657-661` the IME state group (**invariant 9**).
        pub ime: RefCell<ImeState>,

        /// True when we have a precision scroll in progress
        /// PORTS: `surface.zig:664` `precision_scroll`.
        pub precision_scroll: Cell<bool>,

        // Progress bar
        /// PORTS: `surface.zig:670` `progress_bar_timer: ?c_uint` — glib source id
        /// (OWNED handle; `Source.remove` in dispose, **invariant 11**, T14).
        pub progress_bar_timer: RefCell<Option<glib::SourceId>>,

        /// PORTS: `surface.zig:694` `is_split_binding: ?*gobject.Binding` (B6,
        /// 1d053bd6e) — the live `is-split` property binding installed by
        /// [`super::Surface::bind_is_split`]; unbound before re-binding on a
        /// cross-tree move.
        //
        // PORT NOTE (B7, f0f3f4d8d): the reference DELETED the surface-owned
        // `bell_media` field here — the audio bell moved to the application
        // (`Application.Private.bell_media` + the `app.ring-bell` action).
        pub is_split_binding: RefCell<Option<glib::Binding>>,

        /// The source that handles setting our child property.
        /// PORTS: `surface.zig:690` `idle_rechild: ?c_uint` (OWNED source; T14).
        pub idle_rechild: RefCell<Option<glib::SourceId>>,

        /// A weak reference to an inspector window.
        /// PORTS: `surface.zig:693` `inspector: ?*InspectorWindow` — a BACKREF
        /// (weak; nulled by weak-notify, `:2624`). The `InspectorWindow` GType lands
        /// later; stored as an untyped weak backref placeholder for now.
        pub inspector: RefCell<Option<glib::WeakRef<glib::Object>>>,

        /// PORTS: `surface.zig:699` `action_group: ?*gio.SimpleActionGroup`
        /// (SHARED; widget owns it — **invariant 10**).
        pub action_group: RefCell<Option<gio::SimpleActionGroup>>,

        /// PORTS: `surface.zig:706` `vadj_signal_group: ?*gobject.SignalGroup`
        /// (OWNED; `setTarget(null)`+unref in dispose, T14). Held as an untyped
        /// `glib::Object` (a `GSignalGroup`): the typed `glib::SignalGroup` binding
        /// is behind the `glib` `v2_74` feature this build does not enable; T15
        /// (the scroll wiring) constructs it via FFI when it lands.
        pub vadj_signal_group: RefCell<Option<glib::Object>>,

        // Key state tracking for key sequences and tables
        /// PORTS: `surface.zig:709` `key_sequence: ArrayListUnmanaged([:0]const u8)`
        /// (OWNED; free each + deinit, T15). Backs the `key-sequence` property.
        pub key_sequence: RefCell<Vec<CString>>,
        /// PORTS: `surface.zig:710` `key_tables` (OWNED; T15). Backs `key-table`.
        pub key_tables: RefCell<Vec<CString>>,

        /// The context for this surface (window, tab, or split)
        /// PORTS: `surface.zig:721` `context: apprt.surface.NewSurfaceContext`.
        pub context: Cell<NewSurfaceContext>,

        /// Whether primary paste (middle-click paste) is enabled.
        /// PORTS: `surface.zig:724` `gtk_enable_primary_paste` (struct default true).
        pub gtk_enable_primary_paste: Cell<bool>,

        /// True when a left mouse down was consumed purely for a focus change,
        /// and the matching left mouse release should also be suppressed.
        /// PORTS: `surface.zig:728` `suppress_left_mouse_release`.
        pub suppress_left_mouse_release: Cell<bool>,

        /// How much pending horizontal scroll do we have?
        /// PORTS: `surface.zig:731` `pending_horizontal_scroll`.
        pub pending_horizontal_scroll: Cell<f64>,

        /// Timer to reset the amount of horizontal scroll if the user
        /// stops scrolling.
        /// PORTS: `surface.zig:735` `pending_horizontal_scroll_reset: ?c_uint`
        /// (OWNED source; T14).
        pub pending_horizontal_scroll_reset: RefCell<Option<glib::SourceId>>,

        /// PORTS: `surface.zig:738` `overrides.command: ?configpkg.Command`
        /// (OWNED; `deinit` T15).
        pub overrides_command: RefCell<Option<Command>>,
        /// PORTS: `surface.zig:739` `overrides.shell_integration:
        /// ?configpkg.Config.ShellIntegration` (3650bef4e; Copy — no free site).
        pub overrides_shell_integration: Cell<Option<ShellIntegration>>,
        /// PORTS: `surface.zig:740` `overrides.working_directory: ?[:0]const u8`
        /// (OWNED; free T15).
        pub overrides_working_directory: RefCell<Option<CString>>,
    }

    impl Default for Surface {
        /// Mirrors the Zig `Private` struct defaults (`surface.zig:570-742`) that
        /// are non-zero — these are visible to the template's `bind …` closures at
        /// `init_template` time, BEFORE `constructed` runs (matching the reference,
        /// where `initTemplate` at `:1785` sees the struct defaults).
        fn default() -> Self {
            Self {
                bell_ringing: Cell::new(false),
                config: RefCell::new(None),
                child_exited: Cell::new(false),
                default_size: RefCell::new(None),
                error: Cell::new(false),
                font_size_request: RefCell::new(None),
                // `focused: bool = true` (`surface.zig:611`).
                focused: Cell::new(true),
                mapped: Cell::new(false),
                key_sequence_prop: std::marker::PhantomData,
                key_table_prop: std::marker::PhantomData,
                min_size: RefCell::new(None),
                // `mouse_shape: .default` (`surface.zig:586`); `init` sets `.text`.
                mouse_shape: Cell::new(MouseShapeEnum::from(MouseShape::Default)),
                mouse_hidden: Cell::new(false),
                mouse_hover_url: RefCell::new(None),
                pwd: RefCell::new(None),
                title: RefCell::new(None),
                title_override: RefCell::new(None),
                zoom: Cell::new(false),
                is_split: Cell::new(false),
                readonly_prop: std::marker::PhantomData,
                hadjustment: RefCell::new(None),
                vadjustment: RefCell::new(None),
                // `.natural` (`surface.zig:704-705`).
                hscroll_policy: Cell::new(gtk4::ScrollablePolicy::Natural),
                vscroll_policy: Cell::new(gtk4::ScrollablePolicy::Natural),
                gl_area: TemplateChild::default(),
                url_left: TemplateChild::default(),
                url_right: TemplateChild::default(),
                child_exited_overlay: TemplateChild::default(),
                context_menu: TemplateChild::default(),
                error_page: TemplateChild::default(),
                progress_bar_overlay: TemplateChild::default(),
                resize_overlay: TemplateChild::default(),
                search_overlay: TemplateChild::default(),
                key_state_overlay: TemplateChild::default(),
                terminal_page: TemplateChild::default(),
                drop_target: TemplateChild::default(),
                surface_drop_target: TemplateChild::default(),
                drag_handle: TemplateChild::default(),
                drop_overlay: TemplateChild::default(),
                im_context: TemplateChild::default(),
                rt_surface: ApprtSurface::default(),
                core_surface: AtomicPtr::new(std::ptr::null_mut()),
                size: Cell::new(SurfaceSize {
                    width: 0,
                    height: 0,
                }),
                cursor_pos: Cell::new(CursorPos { x: 0.0, y: 0.0 }),
                ime: RefCell::new(ImeState::default()),
                precision_scroll: Cell::new(false),
                progress_bar_timer: RefCell::new(None),
                is_split_binding: RefCell::new(None),
                idle_rechild: RefCell::new(None),
                inspector: RefCell::new(None),
                action_group: RefCell::new(None),
                vadj_signal_group: RefCell::new(None),
                key_sequence: RefCell::new(Vec::new()),
                key_tables: RefCell::new(Vec::new()),
                // `.window` (`surface.zig:721`).
                context: Cell::new(NewSurfaceContext::Window),
                // `gtk_enable_primary_paste: bool = true` (`surface.zig:724`).
                gtk_enable_primary_paste: Cell::new(true),
                suppress_left_mouse_release: Cell::new(false),
                pending_horizontal_scroll: Cell::new(0.0),
                pending_horizontal_scroll_reset: RefCell::new(None),
                overrides_command: RefCell::new(None),
                overrides_shell_integration: Cell::new(None),
                overrides_working_directory: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Surface {
        // PORTS: `surface.zig:48` `.name = "GhosttySurface"`. Preserved 1:1.
        const NAME: &'static str = "GhosttySurface";
        type Type = super::Surface;
        // PORTS: `surface.zig:45` `pub const Parent = adw.Bin`.
        type ParentType = libadwaita::Bin;
        // PORTS: `surface.zig:46` `pub const Implements = [_]type{gtk.Scrollable}`.
        type Interfaces = (gtk4::Scrollable,);

        /// PORTS: `surface.zig:3593-3706` `Class.init`. `ensure_type`s the four
        /// template subclasses so `ui/1.2/surface.ui` can instantiate them
        /// (`:3594-3597`), then binds the template + the instance callbacks.
        fn class_init(klass: &mut Self::Class) {
            ResizeOverlay::ensure_type();
            SearchOverlay::ensure_type();
            KeyStateOverlay::ensure_type();
            SurfaceChildExited::ensure_type();

            // `setTemplateFromResource(gresource.blueprint(1,2,"surface"))`
            // (`:3598-3605`) + the 13 `bindTemplateChildPrivate` (`:3608-3620`).
            klass.bind_template();
            // The 39 `bindTemplateCallback`s (`:3623-3661`).
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            // PORTS: `surface.zig:1785` `gtk.Widget.initTemplate`.
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Surface {
        /// PORTS: `surface.zig:3694-3702` — the 9 registered signals.
        fn signals() -> &'static [glib::subclass::Signal] {
            use std::sync::OnceLock;
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // Emitted whenever the bell event is received. Unlike the
                    // `bell-ringing` property, this is emitted every time the event
                    // is received and not just on state changes.
                    // `bell` (`:448-457`).
                    glib::subclass::Signal::builder("bell").build(),
                    // Emitted whenever the surface would like to be closed for any
                    // reason.
                    //
                    // The surface view does NOT handle its own close confirmation.
                    // If there is a process alive then the boolean parameter will
                    // specify it and the parent widget should handle this request.
                    //
                    // This signal lets the containing widget decide how closure works.
                    // This lets this Surface widget be used as a split, tab, etc.
                    // without it having to be aware of its own semantics.
                    // `close-request` (`:468-477`).
                    glib::subclass::Signal::builder("close-request").build(),
                    // Emitted whenever the surface reads the clipboard.
                    // `clipboard-read` (`:495-504`).
                    glib::subclass::Signal::builder("clipboard-read").build(),
                    // Emitted whenever the clipboard has been written.
                    // `clipboard-write` (`:480-492`), params `(apprt.Clipboard,
                    // [*:0]const u8)`. Emitted by `Clipboard::set` after a
                    // non-confirm write (surface.rs `set_clipboard`).
                    //
                    // CORRECTION (task 20, S54-F3): the first param was
                    // `i32::static_type()` under a note saying "`apprt.Clipboard` has
                    // no GObject type yet". It has one: `GhosttyApprtClipboard`
                    // ([`ClipboardEnum`], `gtypes.rs`).
                    glib::subclass::Signal::builder("clipboard-write")
                        .param_types([ClipboardEnum::static_type(), String::static_type()])
                        .build(),
                    // Emitted after the surface is initialized.
                    // `init` (`:507-516`).
                    glib::subclass::Signal::builder("init").build(),
                    // Emitted just prior to the context menu appearing.
                    // `menu` (`:519-528`).
                    glib::subclass::Signal::builder("menu").build(),
                    // Emitted when the focus wants to be brought to the top and
                    // focused.
                    // `present-request` (`:532-541`).
                    glib::subclass::Signal::builder("present-request").build(),
                    // Emitted when this surface requests its container to toggle its
                    // fullscreen state.
                    // `toggle-fullscreen` (`:545-554`).
                    glib::subclass::Signal::builder("toggle-fullscreen").build(),
                    // Emitted when this surface requests its container to toggle its
                    // maximized state.
                    // `toggle-maximize` (`:558-567`).
                    glib::subclass::Signal::builder("toggle-maximize").build(),
                ]
            })
        }

        /// PORTS: `surface.zig:1784-1833` `init` (instance-init tail). `init_template`
        /// already ran in [`ObjectSubclass::instance_init`]; the remainder — the
        /// action map, the private-field defaults, the config fallback, the IME
        /// reset, the gsettings primary-paste read, the drop-target gtype ordering,
        /// the text cursor, and the initial `propConfig` — runs here.
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // `priv.rt_surface = .{ .surface = self }` (`surface.zig:1793`). Wire
            // the embedded apprt `Surface`'s unowned backref to this widget. The
            // GObject private is heap-pinned, so `&priv.rt_surface` (handed to
            // core in `initSurface`) and this backref both stay stable for life
            // (memory `mirihar-surface-pin`). Set exactly once, never rewritten.
            self.rt_surface.set_widget(&obj);

            // `self.initActionMap()` (`surface.zig:1788`). **Invariant 10**.
            obj.init_action_map();

            // Initialize some private fields so they aren't undefined
            // Private-field defaults (`surface.zig:1793-1801`). Most are already the
            // struct defaults ([`Default`]); we re-assert the ones `init` sets.
            self.ime.replace(ImeState::default());
            self.cursor_pos.set(CursorPos { x: 0.0, y: 0.0 });
            // `priv.mouse_shape = .text` (`surface.zig:1796`).
            self.mouse_shape.set(MouseShapeEnum::from(MouseShape::Text));
            self.mouse_hidden.set(false);
            self.focused.set(true);
            self.mapped.set(false);
            self.size.set(SurfaceSize {
                width: 0,
                height: 0,
            });
            self.vadj_signal_group.replace(None);

            // Config fallback from the app (`surface.zig:1805-1808`). Set the imp
            // field directly (NOT via the property) so no `notify::config` fires —
            // matching the reference's direct `priv.config = app.getConfig()`.
            if self.config.borrow().is_none() {
                let config = Application::default().get_config();
                self.config.replace(config);
            }

            // Setup our input method state
            // IME state (`surface.zig:1811-1813`) — already reset above.

            // Read GTK primary paste setting
            // Read the GTK primary-paste setting (`surface.zig:1816`).
            self.gtk_enable_primary_paste
                .set(gsettings::get_bool(gsettings::Key::GtkEnablePrimaryPaste).unwrap_or(true));

            // Set up to handle items being dropped on our surface. Files can be dropped
            // from Nautilus and strings can be dropped from many programs. The order
            // of these types matter.
            // Drop-target gtypes IN ORDER (`surface.zig:1818-1826`) — the order is
            // load-bearing: FileList, then File, then string.
            self.drop_target.set_types(&[
                gdk4::FileList::static_type(),
                gio::File::static_type(),
                String::static_type(),
            ]);

            // Also have to set up the surface drop target to accept other surfaces
            // (in particular, their surface IDs) (`surface.zig:1846-1854`, B6).
            self.surface_drop_target.set_types(&[u64::static_type()]);

            // Setup properties we can't set from our Blueprint file.
            // `setCursorFromName("text")` (`surface.zig:1858`).
            obj.set_cursor_from_name(Some("text"));
            // `priv.drag_handle.setCursorFromName("grab")` (`surface.zig:1859`, B6).
            self.drag_handle.set_cursor_from_name(Some("grab"));

            // Initial `propConfig` (`surface.zig:1832`) — call the handler directly
            // (no spurious `notify::config` emission), matching the reference.
            obj.on_notify_config();
        }

        /// PORTS: `surface.zig:1855-1920` `dispose`. Frees the OWNED §1.3 rows in
        /// the reference's exact field order (`:1858-1903`), then the template
        /// double-free workaround (**invariant 6**, `:1905-1911`), then chains the
        /// parent dispose (gtk4-rs does this for us after we return).
        ///
        /// Every field is `take()`n (null-guarded) so `dispose` is idempotent — it
        /// can run more than once (PREPORT_GTK §5.2). For GObject-typed rows the
        /// `take()`'d value's scope-end drop is the `unref`; for the glib source ids
        /// (**invariant 11**) the `SourceId` is removed via `g_source_remove` with
        /// the reference's warn-on-failure (`SourceId::drop` does NOT remove).
        fn dispose(&self) {
            // `config` unref+null (`surface.zig:1858-1861`). Scope-end drop unrefs.
            drop(self.config.take());

            // (B7, f0f3f4d8d: the reference deleted the `bell_media` unref here —
            // the audio bell is application-owned now.)

            // `is_split_binding`: `unbind()` + `unref()` + null (`6cd684d5d`,
            // `surface.zig:1898-1902`) — the binding's own ref goes with
            // `unbind()`, the extra ref `bind_is_split` holds goes with the
            // scope-end drop.
            if let Some(binding) = self.is_split_binding.take() {
                binding.unbind();
            }

            // `vadj_signal_group`: `setTarget(null)` THEN unref (`:1868-1872` — the
            // order matters: detach the target before releasing our ref). The typed
            // `SignalGroup` binding is behind a glib feature this build does not
            // enable, so the untyped `GSignalGroup` is detached via FFI; the
            // scope-end drop then unrefs (`:1870`).
            if let Some(group) = self.vadj_signal_group.take() {
                // SAFETY: `group` is a live `GSignalGroup` (the only thing T15 ever
                // stores here); `g_signal_group_set_target(_, null)` is always valid.
                // The `glib::gobject_ffi` binding is behind the `v2_72` feature this
                // build does not enable, so we bind the libgobject-2.0 symbol
                // directly (it exists since 2.72; this build links 2.88).
                unsafe {
                    g_signal_group_set_target(group.as_ptr().cast(), std::ptr::null_mut());
                }
            }

            // `hadj` unref+null (`surface.zig:1874-1877`).
            drop(self.hadjustment.take());

            // `vadj` unref+null (`surface.zig:1879-1882`).
            drop(self.vadjustment.take());

            // The three glib source ids — `Source.remove` + null, warn-on-failure
            // (**invariant 11**, `surface.zig:1884-1903`).
            remove_source_id(self.progress_bar_timer.take(), "progress bar timer");
            remove_source_id(self.idle_rechild.take(), "idle source");
            remove_source_id(
                self.pending_horizontal_scroll_reset.take(),
                "pending horizontal scroll reset source",
            );

            // **Invariant 6** — the GTK double-free workaround. Unset the Bin child
            // before disposing the template: `terminal_page` is both a template
            // child and the Bin's root child (`surface.blp:248` `child:
            // terminal_page`); without this, GTK double-frees it. Port the comment
            // verbatim (`surface.zig:1905-1909`):
            //
            //   This works around a GTK double-free bug where if you bind
            //   to a top-level template child, it frees twice if the widget is
            //   also the root child of the template. By unsetting the child here,
            //   we avoid the double-free.
            self.obj().set_child(gtk4::Widget::NONE);

            // `gtk.Widget.disposeTemplate(self, getGObjectType())`
            // (`surface.zig:1911-1914`).
            self.dispose_template();
        }
    }

    impl Drop for Surface {
        /// PORTS: `surface.zig:1922-1988` `finalize`. In gtk4-rs the `imp` struct's
        /// `Drop` IS the finalize hook (the framework runs it, then chains the
        /// parent finalize) — the same idiom [`super::super::application`] uses.
        ///
        /// The load-bearing part is the core-surface teardown (**invariant 5**):
        /// `deleteSurface` MUST run before `deinit` (`:1926-1936`), extracted into
        /// [`super::finalize_core_surface`] so the ordering is unit-testable without
        /// a live `CoreSurface` (which is built lazily in T15/T19). The remaining
        /// OWNED rows (`:1941-1982`) are freed by Rust's field-drop; the explicit
        /// `take()`s here reproduce the reference's field-order free+null one for
        /// one (idempotent, and the checklist against PREPORT_GTK §1.3).
        fn drop(&mut self) {
            // **Invariant 5** — core-surface teardown (`surface.zig:1922-1940`).
            //
            // NOTE: We must deinit the surface in the finalize (this `Drop`) and
            // NOT in `dispose`, because the inspector widget relies on this behavior
            // with a weakRef to properly deactivate (`surface.zig:1931-1933`).
            // NOTE: We must deinit the surface in the finalize call and NOT
            // the dispose call because the inspector widget relies on this
            // behavior with a weakRef to properly deactivate.
            //
            // Load the raw pointer WITHOUT clearing it (`Acquire` pairs with the
            // `Release` publish in `init_surface`). **Invariant 5** — the pointer
            // MUST stay published until the render/io threads are joined (below),
            // so a still-running render thread's `core()` read never sees a cleared
            // pointer (the panic this hardens against).
            let ptr = self.core_surface.load(Ordering::Acquire);
            super::finalize_core_surface(
                !ptr.is_null(),
                // Remove ourselves from the list of known surfaces in the app. We
                // do this before deinit in case a callback triggers searching for
                // this surface (`surface.zig:1926-1929`).
                || super::delete_surface_from_app(std::ptr::from_ref(&self.rt_surface).cast_mut()),
                || {
                    // `v.deinit(); alloc.destroy(v); priv.core_surface = null`
                    // (**invariants 2 & 5**, `surface.zig:1936-1939`) — IN THAT
                    // ORDER. `deinit` stops+JOINS the renderer/io threads FIRST;
                    // only after the join do we free the heap slot and clear the
                    // pointer, so no thread can read a cleared/dangling pointer.
                    //
                    // SAFETY: `!ptr.is_null()` implies a fully-initialized surface
                    // (a successful `Surface::init` published this box via
                    // `Box::into_raw`); we reconstitute the SAME box exactly once.
                    //
                    // [DOUBLE-FREE FIX] `CoreSurface::deinit`'s contract
                    // (Surface.zig:1922-1988 finalize; rustty-core
                    // `Surface::deinit` doc) is the reference `alloc.destroy`:
                    // after deinit the pointee "must NOT be used OR DROPPED; the
                    // owner must free the allocation WITHOUT dropping." deinit
                    // frees the owned rows via CONSUMING (`ptr::read`) receivers
                    // — including the termio `Subprocess.args` `Vec<CString>`,
                    // freed via the read-out Termio copy's teardown. The previous
                    // `drop(boxed)` VIOLATED the contract: it ran the surface's
                    // field-Drop glue, which re-freed those SAME `ptr::read`'d
                    // rows (the argv `Vec<CString>`, the mailbox queue, the
                    // terminal), aborting with glibc `free(): invalid size`. Free
                    // the heap SLOT without running Drop — exactly what the
                    // init-error path (`Surface::initErrdefers` + caller
                    // free-without-drop) already does.
                    let mut boxed = unsafe { Box::from_raw(ptr) };
                    unsafe { boxed.deinit() };
                    // Free the box's backing allocation WITHOUT dropping its
                    // contents (deinit already tore every field down).
                    let raw: *mut CoreSurface = Box::into_raw(boxed);
                    unsafe {
                        std::alloc::dealloc(
                            raw.cast::<u8>(),
                            std::alloc::Layout::new::<CoreSurface>(),
                        );
                    }
                    // Clear the pointer LAST (`Release`), after the join+free — the
                    // reference's `priv.core_surface = null` (`surface.zig:1939`).
                    self.core_surface
                        .store(std::ptr::null_mut(), Ordering::Release);
                },
            );

            // The remaining OWNED rows, in the reference's field order
            // (`surface.zig:1941-1982`). Each `take()` drops (frees) + nulls.
            // `default_size`/`font_size_request`/`min_size` are `Copy` POD boxes,
            // so `take()` alone nulls the field (the `boxedFree`/`destroy`
            // free-site) and the returned value is trivially discarded.
            drop(self.mouse_hover_url.take()); // `:1941` `glib.free`
            self.default_size.take(); // `:1945` `ext.boxedFree`
            self.font_size_request.take(); // `:1949` `glib.ext.destroy`
            self.min_size.take(); // `:1953` `ext.boxedFree`
            drop(self.pwd.take()); // `:1957` `glib.free`
            drop(self.title.take()); // `:1961` `glib.free`
            drop(self.title_override.take()); // `:1965` `glib.free`
            drop(self.overrides_command.take()); // `:1969-1972` `command.deinit`
            drop(self.overrides_working_directory.take()); // `:1973-1976` `alloc.free`

            // Clean up key sequence and key table state (`:1979-1982`): free each
            // element + deinit the container. `Vec::clear` drops every `CString`
            // then leaves the container empty (the `deinit`).
            self.key_sequence.borrow_mut().clear();
            self.key_tables.borrow_mut().clear();
        }
    }

    impl WidgetImpl for Surface {
        /// PORTS: `surface.zig:2014-2065` `snapshot` (B8, 3263fc6c4): the widget
        /// `snapshot` virtual override that renders the native GTK blur behind
        /// the terminal on GTK >= 4.23.3 (paired with the wayland winproto
        /// skipping its own `ext_background_effect` there, 04f1bc096 — the
        /// atomic B8 pair: porting only one half double-blurs or de-blurs).
        fn snapshot(&self, snap: &gtk4::Snapshot) {
            let blur: Option<f32> = 'blur: {
                // Native GTK blur is only available since GTK 4.23.3
                if crate::gtk::gtk_version::runtime_until(4, 23, 3) {
                    break 'blur None;
                }
                let cfg = self.config.borrow();
                let Some(config) = cfg.as_ref() else {
                    break 'blur None;
                };

                match config.get().background_blur {
                    rustty_config::value_types::BackgroundBlur::Radius(v) => Some(f32::from(v)),
                    rustty_config::value_types::BackgroundBlur::True
                    | rustty_config::value_types::BackgroundBlur::MacosGlassRegular
                    | rustty_config::value_types::BackgroundBlur::MacosGlassClear => Some(20.0),
                    rustty_config::value_types::BackgroundBlur::False => None,
                }
            };

            if let Some(b) = blur {
                // pushCopy and appendPaste are only supported since 4.22.
                // These two functions are crucial for the blur function
                // and it cannot be implemented otherwise, so if you compile
                // Rustty on an older GTK version, you won't get blur.
                // Sorry.
                //
                // PORT NOTE: the reference's `comptime !gtk_version.atLeast(4,
                // 22, 0)` compile gate maps to the opt-in `gtk-v4_22` cargo
                // feature (the `gtk-v4_16`/`gtk-v4_20` convention: the feature
                // raises the gtk4-rs API ceiling so `push_copy`/`push_blur`/
                // `append_paste` exist; the `runtime_until(4, 23, 3)` guard
                // above stays live either way).
                #[cfg(feature = "gtk-v4_22")]
                {
                    let widget = self.obj();
                    let width = widget.width();
                    let height = widget.height();

                    // Push the current render state (background)
                    // to be appended below
                    snap.push_copy();

                    // Apply blur to the copied background
                    snap.push_blur(f64::from(b));

                    snap.append_paste(
                        &gtk4::graphene::Rect::new(0.0, 0.0, width as f32, height as f32),
                        0,
                    );

                    // The reference's `defer snap.pop()` pair — blur first,
                    // then the copy — runs here in reverse push order.
                    snap.pop();
                    snap.pop();
                }
                #[cfg(not(feature = "gtk-v4_22"))]
                let _ = b;
            }

            // Draw the children normally
            self.parent_snapshot(snap);
        }
    }
    impl BinImpl for Surface {}
    impl ScrollableImpl for Surface {}

    impl Surface {
        /// Get the key sequence list. Full transfer.
        ///
        /// PORTS: `surface.zig:2141-2145` `getKeySequence` (transfer `.full`).
        /// Builds a fresh [`ext::StringList`] from the owned key-sequence.
        fn key_sequence_prop(&self) -> Option<ext::StringList> {
            Some(string_list_from(&self.key_sequence.borrow()))
        }

        /// Get the key table list. Full transfer.
        ///
        /// PORTS: `surface.zig:2148-2152` `getKeyTable` (transfer `.full`).
        fn key_table_prop(&self) -> Option<ext::StringList> {
            Some(string_list_from(&self.key_tables.borrow()))
        }

        /// Get the readonly state from the core surface.
        ///
        /// PORTS: `surface.zig:1197-1201` `getReadonly`. `false` before realize
        /// (`core_surface == null`, **invariant 3**); otherwise the core's flag.
        fn readonly_prop(&self) -> bool {
            let ptr = self.core_surface.load(Ordering::Acquire);
            if ptr.is_null() {
                return false;
            }
            // SAFETY: a non-null pointer is a live, heap-stable `CoreSurface` (the
            // pin; published by `init_surface`, cleared only after the render/io
            // threads are joined in finalize). This getter runs on the main thread;
            // the `readonly` field is a plain bool read (the reference reads
            // `priv.core_surface.?` fields the same way, unsynchronized).
            unsafe { (*ptr).readonly }
        }
    }

    /// Builds a [`ext::StringList`] from owned NUL-terminated key strings — the
    /// rustty analogue of `ext.StringList.create(alloc, items)`
    /// (`surface.zig:2144`). Non-UTF-8 bytes decode lossily (the storage is
    /// `String`; see `ext::slice`).
    fn string_list_from(items: &[CString]) -> ext::StringList {
        let owned: Vec<String> = items
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
        ext::StringList::new(&refs)
    }
}

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
/// without constructing a live `CoreSurface` — that requires the lazy GLArea
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

/// PORTS: `surface.zig:3406-3409` `InitError = Allocator.Error ||
/// error{GLAreaError, SurfaceError}`. The only consumer ([`Surface::gl_resize`])
/// logs and discards it (`:3401-3403`), so the variants carry no payload.
// task-14d-3: `Debug` is required by the restored
// `log.warn("surface failed to initialize err={}", .{err})`
// (`surface.zig:3502`) — the port had dropped the error value, so nothing
// needed to render it. Semver-additive, no layout/ABI effect, and the only new
// capability is the one the reference already exercises at that call site.
#[derive(Debug)]
enum InitError {
    /// `error.GLAreaError` — `makeCurrent` reported a GL error (`:3423`).
    GlArea,
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
    /// per-surface widget: an `adw::Bin` wrapping the terminal `GLArea` and the
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
    /// first GLArea resize (**invariant 3**). The returned pointer is valid only
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
    /// PORTS: `surface.zig:822-825` `redraw`. Forces the GLArea to re-render — used
    /// when external events (e.g. a reparent) invalidate the surface. This is the
    /// app/main-thread leg of the **T16 render contract** (`must_draw_from_app_thread`).
    pub fn redraw(&self) {
        self.imp().gl_area.queue_render();
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

// ===========================================================================
// Libghostty callbacks + GLArea lifecycle (L5.2 T15/T17)
//
// The apprt seam (`gtk::surface::Surface`) forwards to these; the GLArea
// signal handlers (the `gl_*` template callbacks) drive them. Ported from
// `surface.zig` — the "Libghostty Callbacks" region (`:1497-...`) and the
// GLArea lifecycle (`:3228-3489`).
// ===========================================================================

impl Surface {
    // PORTS: `surface.zig:2225-2229` `setError`. The `glib::Properties`-generated
    // `set_error(bool)` setter for the `error` property already does the reference's
    // `priv.@"error" = v; notifyByPspec(...)` (set the cell + notify on change), so
    // callers use it directly (e.g. `Surface::gl_realize`'s GL-error branch).

    /// PORTS: `surface.zig:2378-2429` `propMouseShape`. Set the widget cursor from the
    /// current `mouse-shape` property. Shared by `on_notify_mouse_shape` and
    /// `on_notify_mouse_hidden` (which re-runs the shape logic on un-hide).
    fn apply_mouse_shape_cursor(&self) {
        // If our mouse should be hidden currently then we don't do anything
        // (`:2385-2387`).
        if self.imp().mouse_hidden.get() {
            return;
        }
        // Map the typed shape to a CSS cursor name and set it (`:2482-2520`).
        let name = mouse_shape_cursor_name(MouseShape::from(self.imp().mouse_shape.get()));
        self.upcast_ref::<gtk4::Widget>()
            .set_cursor_from_name(Some(name));
    }

    /// PORTS: `surface.zig:1500-1507` `close`. Emits `close-request`; the window
    /// tier decides what to do. The apprt seam's `close(process_alive)` discards
    /// `process_alive` and calls this (`Surface.zig:35-38`).
    pub fn close(&self) {
        self.emit_by_name::<()>("close-request", &[]);
    }

    /// Focus this surface. This properly focuses the input part of
    /// our surface.
    ///
    /// PORTS: `surface.zig:1742-1745` `grabFocus`. Focus the input part of the
    /// surface — the inner `GLArea`, NOT the outer `adw::Bin`. The owner tiers
    /// (`window.rs`, `split_tree.rs`, `application.rs`) call `surface.grab_focus()`;
    /// this inherent method shadows `WidgetExt::grab_focus` so those calls target the
    /// GLArea explicitly, exactly as the reference does (rather than relying on
    /// `adw::Bin`'s implicit focus-descent).
    pub fn grab_focus(&self) {
        let _ = self.imp().gl_area.upcast_ref::<gtk4::Widget>().grab_focus();
    }

    /// PORTS: `surface.zig:2231-2248` `setSearchActive`. Toggle the
    /// [`SearchOverlay`]'s `active` property (which drives its `visible` binding);
    /// on activation, seed a non-empty `needle` into the entry and grab focus. The
    /// `active` property setter re-emits `search-changed`, and
    /// [`SearchOverlay::set_search_contents`] re-emits it with `needle`, so the
    /// core search is driven through the surface's `search_changed` signal handler.
    pub fn set_search_active(&self, active: bool, needle: &[u8]) {
        let overlay = &*self.imp().search_overlay;
        // `gobject.Object.setProperty(search_overlay, "active", active)` (`:2233-2238`).
        overlay.set_active(active);

        // `if (!std.mem.eql(u8, needle, "")) search_overlay.setSearchContents(needle)`
        // (`:2240-2242`).
        if !needle.is_empty() {
            overlay.set_search_contents(&String::from_utf8_lossy(needle));
        }

        // `if (active) search_overlay.grabFocus()` (`:2244-2246`).
        if active {
            overlay.grab_focus();
        }
    }

    /// PORTS: `surface.zig:2250-2252` `setSearchTotal`. Forward the total match
    /// count to the search overlay's counter.
    pub fn set_search_total(&self, total: Option<usize>) {
        self.imp().search_overlay.set_search_total(total);
    }

    /// PORTS: `surface.zig:2254-2256` `setSearchSelected`. Forward the selected
    /// match index to the search overlay's counter.
    pub fn set_search_selected(&self, selected: Option<usize>) {
        self.imp().search_overlay.set_search_selected(selected);
    }

    /// Handle a key sequence action from the apprt.
    ///
    /// PORTS: `surface.zig:916-950` `keySequenceAction`. Handle a key-sequence
    /// action from the apprt, updating the owned `key_sequence` (which backs the
    /// `key-sequence` property the key-state overlay binds, revealing pending
    /// leader-key feedback).
    pub fn key_sequence_action(&self, value: rustty_apprt_api::action::KeySequence) {
        use rustty_apprt_api::action::KeySequence;
        let imp = self.imp();

        // `freezeNotify` / `defer thawNotify` (`:923-924`): the guard thaws on
        // drop, coalescing the notify below.
        let _guard = self.freeze_notify();
        // Notify unconditionally (`:925`) — queued before the (possibly
        // early-returning) switch, exactly like the reference.
        self.notify("key-sequence");

        match value {
            KeySequence::Trigger(trigger) => {
                // Convert the trigger to a human-readable label (`:930-934`).
                let mut label = String::new();
                if !key::label_from_trigger(&mut label, trigger) {
                    // `if (!success) return` (`:933`).
                    return;
                }
                // Copy and append (`:937-942`). A label can't contain a NUL, so
                // `CString::new` only fails on a malformed label; drop it if so.
                if let Ok(duped) = CString::new(label) {
                    imp.key_sequence.borrow_mut().push(duped);
                }
            }
            KeySequence::End => {
                // Free all the stored strings and clear
                // Free all stored strings and clear (`:944-948`).
                imp.key_sequence.borrow_mut().clear();
            }
        }
    }

    /// Handle a key table action from the apprt.
    ///
    /// PORTS: `surface.zig:953-981` `keyTableAction`. Handle a key-table action,
    /// maintaining the owned `key_tables` stack (which backs the `key-table`
    /// property the key-state overlay binds).
    pub fn key_table_action(&self, value: rustty_apprt_api::action::KeyTable<'_>) {
        use rustty_apprt_api::action::KeyTable;
        let imp = self.imp();

        let _guard = self.freeze_notify();
        self.notify("key-table");

        match value {
            KeyTable::Activate(name) => {
                // Duplicate the name string and push onto stack
                // Duplicate the name and push onto the stack (`:965-969`).
                if let Ok(duped) = CString::new(name) {
                    imp.key_tables.borrow_mut().push(duped);
                }
            }
            KeyTable::Deactivate => {
                // Pop and free the top table (`:971-973`).
                imp.key_tables.borrow_mut().pop();
            }
            KeyTable::DeactivateAll => {
                // Free all tables and clear (`:975-978`).
                imp.key_tables.borrow_mut().clear();
            }
        }
    }

    /// Set the current progress report state.
    ///
    /// PORTS: `surface.zig:1030-1113` `setProgressReport`. Drive the
    /// `progress_bar_overlay` (a `gtk.ProgressBar` in `surface.blp`) visible +
    /// fraction/pulse from an OSC 9;4 progress report, with a 15s watchdog timer
    /// that removes bars stalled by bad-actor programs.
    pub fn set_progress_report(&self, value: rustty_vt::osc::ProgressReport) {
        use rustty_vt::osc::ProgressReportState;
        let imp = self.imp();

        // No matter what, we stop the timer because if we're removing
        // then we're done and otherwise we restart it.
        // Stop the timer no matter what — if removing we're done, otherwise we
        // restart it below (`:1036-1043`).
        remove_source_id(imp.progress_bar_timer.take(), "progress bar timer");

        // Config gate: `progress-style` off => hide + return (`:1045-1051`).
        if let Some(config) = imp.config.borrow().as_ref() {
            if !config.get().progress_style {
                // log.debug("progress_report action blocked by config", .{});
                //
                // S54-F4: this was dropped entirely. It is the only signal a
                // user gets that a `progress_report` OSC was ignored on purpose.
                log::debug!(target: LOG_SCOPE, "progress_report action blocked by config");
                imp.progress_bar_overlay.set_visible(false);
                return;
            }
        }

        let progress_bar = &*imp.progress_bar_overlay;
        match value.state {
            // Remove the progress bar (`:1055-1059`).
            ProgressReportState::Remove => {
                progress_bar.set_visible(false);
                return;
            }
            // Set the progress bar to a fixed value if one was provided, otherwise pulse.
            // Remove the `error` CSS class so that the progress bar shows as normal.
            // Fixed value if provided, otherwise pulse; clear the `error` class
            // (`:1063-1070`).
            ProgressReportState::Set => {
                progress_bar.remove_css_class("error");
                match value.progress {
                    Some(progress) => progress_bar.set_fraction(compute_fraction(progress)),
                    None => progress_bar.pulse(),
                }
            }
            // Set the progress bar to a fixed value if one was provided, otherwise pulse.
            // Set the `error` CSS class so that the progress bar shows as an error color.
            // As `.set`, but with the `error` class (`:1074-1081`).
            ProgressReportState::Error => {
                progress_bar.add_css_class("error");
                match value.progress {
                    Some(progress) => progress_bar.set_fraction(compute_fraction(progress)),
                    None => progress_bar.pulse(),
                }
            }
            // The state of progress is unknown, so pulse the progress bar to
            // indicate that things are still happening.
            // Unknown progress — pulse (`:1085-1087`).
            ProgressReportState::Indeterminate => progress_bar.pulse(),
            // If a progress value was provided, set the progress bar to that value.
            // Don't pulse the progress bar as that would indicate that things were
            // happening. Otherwise this is mainly used to keep the progress bar on
            // screen instead of timing out.
            // Keep the bar on screen; set a fixed value if provided, but don't
            // pulse (`:1093-1097`).
            ProgressReportState::Pause => {
                if let Some(progress) = value.progress {
                    progress_bar.set_fraction(compute_fraction(progress));
                }
            }
        }

        // Assume all states lead to visibility
        // All non-remove states lead to visibility (`:1100-1102`).
        progress_bar.set_visible(true);

        // Start our timer to remove bad actor programs that stall
        // the progress bar.
        // Start the watchdog to remove bad-actor programs that stall the bar
        // (`:1104-1112`). 15 seconds, mirroring `progress_bar_timeout_seconds`.
        let id = glib::timeout_add_local(
            Duration::from_secs(15),
            glib::clone!(
                #[weak(rename_to = this)]
                self,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || this.progress_bar_timer()
            ),
        );
        imp.progress_bar_timer.replace(Some(id));
    }

    /// The progress bar hasn't been updated by the TUI recently, remove it.
    ///
    /// PORTS: `surface.zig:1116-1122` `progressBarTimer`. The bar hasn't been
    /// updated by the TUI recently; remove it. Nulls the field first (before the
    /// re-entrant `setProgressReport(.remove)`) so its timer-removal branch is a
    /// no-op on the source currently firing (`:1119`).
    fn progress_bar_timer(&self) -> glib::ControlFlow {
        self.imp().progress_bar_timer.replace(None);
        self.set_progress_report(rustty_vt::osc::ProgressReport {
            state: rustty_vt::osc::ProgressReportState::Remove,
            progress: None,
        });
        glib::ControlFlow::Break
    }

    /// PORTS: `surface.zig:1509-1530` `childExited`. Set the `child-exited`
    /// property (which reveals `child_exited_overlay` via the `visible: bind
    /// template.child-exited` in `surface.blp`) and populate the banner from the
    /// exit payload. Always returns `true`: the `SurfaceChildExitedNoop` path that
    /// returns `false` (forcing the text-based message) does not exist in this
    /// Adwaita-1.3+ port (`:1521-1526`).
    pub fn child_exited_show(&self, value: rustty_apprt_api::surface::ChildExited) -> bool {
        // Even if we don't support the overlay, we still keep our property
        // up to date for anyone listening.
        // Keep the property up to date for anyone listening (`:1513-1519`).
        self.set_child_exited(true);

        // If we have the noop child exited overlay then we don't do anything
        // for child exited. The false return will force librustty to show
        // the normal text-based message.
        // PORT NOTE: this port has no noop child-exited overlay (`surface.zig:1541-1543`
        // `@hasDecl(ChildExited, "noop")`), so that early `return false` has no arm here.
        // Populate the banner (`:1528`).
        self.imp()
            .child_exited_overlay
            .set_child_exited_data(Some(value));
        true
    }

    /// PORTS: `surface.zig:2296-2316` `setBellRinging`. Named `_inner` because the
    /// `bell-ringing` property's generated setter owns `set_bell_ringing`; that
    /// generated setter is the reference's `privateShallowFieldAccessor` (a plain
    /// write), and THIS is the reference method every bell path calls.
    ///
    /// CORRECTION (task 20, S55-F2 + S54-F2): there was no such method. Callers
    /// used the generated setter, which writes and notifies unconditionally, and
    /// `ring_bell` carried the `setBellRinging(true)` half without the freeze. So
    /// `notify::bell-ringing` fired on every focus-in, mouse press and key press
    /// while the value did not change, and the notifications raised by `bell`
    /// handlers and the ring-bell actions were not coalesced around the write. The
    /// property's own contract (`surface.zig:62-68`) promises a notify only on a
    /// full state change.
    pub fn set_bell_ringing_inner(&self, ringing: bool) {
        // Prevent duplicate change notifications if the signals we emit
        // in this function cause this state to change again.
        let _thaw = self.freeze_notify();

        // Logic around bell reaction happens on every event even if we're
        // already in the ringing state.
        if ringing {
            self.ring_bell();
            // focus clears ringing state, so we should not change state to
            // ringing if we're already focused
            if self.focused() {
                return;
            }
        }

        // Property change only happens on actual state change
        let imp = self.imp();
        if imp.bell_ringing.get() == ringing {
            return;
        }
        imp.bell_ringing.set(ringing);
        self.notify_bell_ringing();
    }

    /// Handle bell features that need to happen every time a BEL is received
    /// Currently this is audio and system but this could change in the future.
    ///
    /// PORTS: `surface.zig:2561-2574` `ringBell` (B7 end state, f0f3f4d8d): emit
    /// the `bell` signal, then fan out to the `tab.`/`win.`/`app.ring-bell`
    /// widget actions (system beep, title bell, urgency — `window.rs`/`tab.rs` —
    /// and the application-owned audio bell, `application.rs` `action_ring_bell`).
    ///
    /// CORRECTION (task 20): this used to also host the focused guard and the
    /// `bell-ringing` write, with a PORT NOTE saying "rustty keeps
    /// `set_bell_ringing` as the plain property write and hosts the full bell
    /// here". Those belong to `setBellRinging` ([`Self::set_bell_ringing_inner`]),
    /// which calls this; `ringBell` is private in the reference.
    fn ring_bell(&self) {
        // Emit the signal
        // `signals.bell.impl.emit(...)` — fire any bell subscriber BEFORE the
        // action fan-out.
        self.emit_by_name::<()>("bell", &[]);

        // Activate actions if they exist
        // `tab.ring-bell` / `win.ring-bell` do the system beep, title bell, and
        // attention/urgency (`window.rs`, `tab.rs`); `app.ring-bell` plays the
        // application-owned audio bell (B7, `application.rs`). They bubble up from
        // the surface widget.
        let _ = self.activate_action("tab.ring-bell", None);
        let _ = self.activate_action("win.ring-bell", None);
        let _ = self.activate_action("app.ring-bell", None);
    }

    /// Request that this terminal come to the front and become focused.
    /// It is up to the embedding widget to react to this.
    ///
    /// PORTS: `surface.zig:1126-1133` `present`. Request that this terminal come to
    /// the front and become focused. It is up to the embedding widget (the owning
    /// [`Window`](super::Window)) to react to this via the `present-request` signal
    /// (`window.rs` `surface_present_request`).
    pub fn present(&self) {
        self.emit_by_name::<()>("present-request", &[]);
    }

    /// PORTS: `surface.zig:862-869` `toggleFullscreen`. Emits `toggle-fullscreen`;
    /// the owning window reacts (`window.rs` `surface_toggle_fullscreen`).
    pub fn toggle_fullscreen(&self) {
        self.emit_by_name::<()>("toggle-fullscreen", &[]);
    }

    /// PORTS: `surface.zig:871-878` `toggleMaximize`. Emits `toggle-maximize`;
    /// the owning window reacts (`window.rs` `surface_toggle_maximize`).
    pub fn toggle_maximize(&self) {
        self.emit_by_name::<()>("toggle-maximize", &[]);
    }

    /// Estimate and set the initial window size from config and font metrics.
    /// This can be called before the core surface exists to set up the window
    /// size before presenting. This is an estimate because it does not take
    /// into account any padding that may need to be added to the window.
    ///
    /// PORTS: `surface.zig:2095-2138` `estimateInitialSize`. Estimate the window's
    /// initial pixel size from `window-width`/`window-height` × the font-grid cell
    /// size, so the window manager can position it before the surface initializes;
    /// stores it in the `default-size` property (read back by
    /// `Application::init_and_show_window`).
    pub fn estimate_initial_size(&self) {
        // `const config = priv.config orelse return;` (`:2097-2099`).
        let config_ref = self.imp().config.borrow();
        let Some(config_obj) = config_ref.as_ref() else {
            return;
        };
        let config = config_obj.get();

        // Both dimensions must be configured (`:2101-2102`).
        if config.window_height == 0 || config.window_width == 0 {
            return;
        }

        // Get content scale and compute DPI
        // Content scale -> DPI (`:2104-2113`). `default_dpi` = 72 on macOS else 96
        // (`rustty-font-shaper/face/mod.rs` `default_dpi`).
        let content_scale = self.get_content_scale();
        let default_dpi: f32 = if cfg!(target_os = "macos") {
            72.0
        } else {
            96.0
        };
        let font_size = DesiredSize {
            points: config.font_size,
            xdpi: (content_scale.x * default_dpi) as u16,
            ydpi: (content_scale.y * default_dpi) as u16,
        };

        // Get font grid for cell metrics
        // Get the font grid for cell metrics (`:2115-2126`). Reach the core app's
        // `font_grid_set` the way `delete_surface` reaches the core app (above).
        let app = Application::default();
        let Some(core_app) = app.imp().core_app.get() else {
            return;
        };
        let core_app = core_app.as_ptr().cast::<rustty_core::app::App<App>>();
        let derived = rustty_core::surface::font_shared_grid_set_derived_config(config);
        // SAFETY: a bound core app owns a live `App<App>`; `font_grid_set` is
        // main-thread-only and this runs on the main thread. `ref` is balanced by
        // the `deref` below (the reference `defer font_grid_set.deref(key)`).
        let (font_grid_key, font_grid) =
            match unsafe { (*core_app).font_grid_set.r#ref(&derived, font_size) } {
                Ok(pair) => pair,
                Err(_) => return,
            };
        let cell = unsafe { (*font_grid).cell_size() };
        unsafe { (*core_app).font_grid_set.deref(font_grid_key) };

        // Cells -> pixels -> logical size (`:2128-2137`). Min 10 cols × 4 rows
        // (`CoreSurface.min_window_{width,height}_cells`).
        let width = config.window_width.max(10) * cell.width;
        let height = config.window_height.max(4) * cell.height;
        let final_width = (width as f32 / content_scale.x).ceil() as u32;
        let final_height = (height as f32 / content_scale.y).ceil() as u32;

        // `self.setDefaultSize(...)` (`:2137`).
        self.set_default_size(Some(Size {
            width: final_width,
            height: final_height,
        }));
    }

    /// PORTS: `surface.zig:1532-1576` `getContentScale`. The GTK widget scale
    /// factor multiplied by the `gtk-xft-dpi` font-DPI scale.
    #[must_use]
    pub fn get_content_scale(&self) -> ContentScale {
        let imp = self.imp();

        // Future: detect GTK version 4.12+ and use gdk_surface_get_scale so we
        // can support fractional scaling.
        // The GTK widget scale factor (`:1536-1546`).
        let scale = imp.gl_area.scale_factor();
        let gtk_scale: f32 = if scale <= 0 {
            log::warn!(target: LOG_SCOPE, "gtk_widget_get_scale_factor returned a non-positive number: {scale}");
            1.0
        } else {
            scale as f32
        };

        // Also scale using font-specific DPI, which is often exposed to the user
        // via DE accessibility settings (see https://docs.gtk.org/gtk4/class.Settings.html).
        //
        // gtk-xft-dpi is font DPI multiplied by 1024. See
        // https://docs.gtk.org/gtk4/property.Settings.gtk-xft-dpi.html
        //
        // The gtk-xft-dpi font-DPI scale (`:1548-1572`). gtk-xft-dpi is the font
        // DPI × 1024; divide by 1024 then by the 96 default to derive a scale.
        let xft_dpi_scale: f32 = match gsettings::get_c_int(gsettings::Key::GtkXftDpi) {
            None => {
                log::warn!(target: LOG_SCOPE, "gtk-xft-dpi was not set, using default value");
                1.0
            }
            // Use a value of 1.0 for the XFT DPI scale if the setting is <= 0
            // See:
            // https://gitlab.gnome.org/GNOME/libadwaita/-/commit/a7738a4d269bfdf4d8d5429ca73ccdd9b2450421
            // https://gitlab.gnome.org/GNOME/libadwaita/-/commit/9759d3fd81129608dd78116001928f2aed974ead
            Some(dpi) if dpi <= 0 => {
                // -1 is a valid value which specifies default scale.
                // https://docs.gtk.org/gtk4/property.Settings.gtk-xft-dpi.html
                if dpi != -1 {
                    log::warn!(target: LOG_SCOPE, "gtk-xft-dpi has invalid value ({dpi}), using default");
                }
                1.0
            }
            // As noted above gtk-xft-dpi is multiplied by 1024, so we divide by
            // 1024, then divide by the default value (96) to derive a scale. Note
            // gtk-xft-dpi can be fractional, so we use floating point math here.
            Some(dpi) => (dpi as f32 / 1024.0) / 96.0,
        };

        let scale = gtk_scale * xft_dpi_scale;
        ContentScale { x: scale, y: scale }
    }

    /// PORTS: `surface.zig:1578-1586` `getSize`. Returns the CACHED size, not a
    /// live `gtk_widget_get_width` — this is VERY important because it avoids the
    /// pty having an incorrect initial size. We cache it in [`Surface::gl_resize`]
    /// and only build the core surface once we are in a widget tree with proper
    /// dimensions (**invariant 3**).
    #[must_use]
    pub fn get_size(&self) -> SurfaceSize {
        // By the time this is called, we should be in a widget tree.
        // This should not be called before that. We ensure this by initializing
        // the surface in `glareaResize`. This is VERY important because it
        // avoids the pty having an incorrect initial size.
        self.imp().size.get()
    }

    /// PORTS: `surface.zig:1588-1590` `getCursorPos`.
    #[must_use]
    pub fn get_cursor_pos(&self) -> CursorPos {
        self.imp().cursor_pos.get()
    }

    // `surface.zig:2104-2152`, the remaining property accessors. PORT NOTE: each is
    // the `glib::Properties`-generated getter/setter on the field of the same name
    // (`imp::Surface`), which sets the field and notifies as the reference does:
    //
    // /// Set the title for this surface, copies the value. This should always
    // /// be the title as set by the terminal program, not any manually set
    // /// title. For manually set titles see `setTitleOverride`.
    // pub fn setTitle(self: *Self, title: ?[:0]const u8) void {
    //
    // /// Overridden title. This will be generally be shown over the title
    // /// unless this is unset (null).
    // pub fn setTitleOverride(self: *Self, title: ?[:0]const u8) void {
    //
    // /// Returns the pwd property without a copy.
    // pub fn getPwd(self: *Self) ?[:0]const u8 {
    //
    // /// Set the pwd for this surface, copies the value.
    // pub fn setPwd(self: *Self, pwd: ?[:0]const u8) void {
    //
    // /// Returns the focus state of this surface.
    // pub fn getFocused(self: *Self) bool {
    //
    // /// Returns true if the GLArea of this surface is mapped.
    // pub fn getMapped(self: *Self) bool {
    //
    // /// Change the configuration for this surface.
    // pub fn setConfig(self: *Self, config: *Config) void {
    //
    // /// Return the default size, if set.
    // pub fn getDefaultSize(self: *Self) ?*Size {
    //
    // /// Set the default size for a window that contains this surface.
    // /// This is up to the embedding widget to respect this. Generally, only
    // /// the first surface in a window respects this.
    // pub fn setDefaultSize(self: *Self, size: Size) void {
    //
    // /// Return the min size, if set.
    // pub fn getMinSize(self: *Self) ?*Size {
    //
    // /// Set the min size for a window that contains this surface.
    // /// This is up to the embedding widget to respect this. Generally, only
    // /// the first surface in a window respects this.
    // pub fn setMinSize(self: *Self, size: Size) void {
    /// Returns the title property without a copy.
    ///
    /// PORTS: `surface.zig:1994-1996` `getTitle`. The stored surface title (the
    /// terminal-set title, before the user override).
    #[must_use]
    pub fn get_title(&self) -> Option<String> {
        self.imp().title.borrow().clone()
    }

    /// Returns the effective title: the user-overridden title if set,
    /// otherwise the terminal-set title.
    ///
    /// PORTS: `surface.zig:1998-2004` `getEffectiveTitle`. The user override wins
    /// over the terminal-set title.
    #[must_use]
    pub fn get_effective_title(&self) -> Option<String> {
        let imp = self.imp();
        // `return priv.title_override orelse priv.title` (`:2003`).
        imp.title_override
            .borrow()
            .clone()
            .or_else(|| imp.title.borrow().clone())
    }

    /// Copies the effective title to the clipboard.
    ///
    /// PORTS: `surface.zig:2006-2019` `copyTitleToClipboard`. Copies the effective
    /// title to the standard clipboard.
    pub fn copy_title_to_clipboard(&self) -> bool {
        // `const title = self.getEffectiveTitle() orelse return false` (`:2007`).
        let Some(title) = self.get_effective_title() else {
            return false;
        };
        // `if (title.len == 0) return false` (`:2008`).
        if title.is_empty() {
            return false;
        }
        // `self.setClipboard(.standard, &.{.{ .mime = "text/plain", .data = title }},
        // false)` (`:2009-2013`). Reuses the Phase-A clipboard WRITE path.
        self.set_clipboard(
            Clipboard::Standard,
            &[ClipboardContent {
                mime: b"text/plain",
                data: title.as_bytes(),
            }],
            false,
        );
        true
    }

    /// Prompt for a manual title change for the surface.
    ///
    /// PORTS: `surface.zig:1466-1478` `promptTitle` + `titleDialogSet`
    /// (`:3543-3551`). Mirrors [`super::Tab::prompt_tab_title`], but targets the
    /// surface: the initial value is `title_override orelse title` (`:1468`), and
    /// on the dialog's `set` signal we apply the empty-string → `None`
    /// normalization via `setTitleOverride` (`:3550`).
    pub fn prompt_title(&self) {
        use super::title_dialog::{Target, TitleDialog};

        // PORTS: `surface.zig:1468` `priv.title_override orelse priv.title`.
        let initial = self.title_override().or_else(|| self.get_title());
        let dialog = TitleDialog::new(Target::Surface, initial.as_deref());

        // PORTS: `surface.zig:1469-1476` — connect `titleDialogSet` to `set`.
        dialog.connect_local(
            "set",
            false,
            glib::clone!(
                #[weak(rename_to = this)]
                self,
                #[upgrade_or]
                None,
                move |args| {
                    // arg 0 is the emitter; arg 1 is the string param.
                    let title: String = args[1].get().expect("set arg is a string");
                    // PORTS: `surface.zig:3549-3550` `titleDialogSet`.
                    this.set_title_override(if title.is_empty() { None } else { Some(title) });
                    None
                }
            ),
        );

        // PORTS: `surface.zig:1478`.
        dialog.present(self.upcast_ref::<gtk4::Widget>());
    }

    /// PORTS: `surface.zig:1747-1778` `sendDesktopNotification`. Build a
    /// `gio.Notification` and send it via the application. The `present-surface`
    /// default action targets this surface's core id, so activating the
    /// notification focuses this surface.
    pub fn send_desktop_notification(&self, title: &[u8], body: &[u8]) {
        use gio::prelude::ApplicationExt as _;

        let app = Application::default();
        let imp = self.imp();

        // `const core_surface = priv.core_surface orelse { warn; return; }`
        // (`:1751-1754`).
        let core = imp.core_surface.load(Ordering::Acquire);
        if core.is_null() {
            log::warn!(target: LOG_SCOPE, "can't send notification because there is no core surface");
            return;
        }
        // SAFETY: a non-null pointer is a live, heap-stable `CoreSurface` (the pin;
        // published by `init_surface`, cleared only after the render/io threads are
        // joined in finalize). This runs on the main thread; `id` is a plain u64.
        let core_id = unsafe { (*core).id };

        // Set a default title if we don't already have one (`:1756-1759`). PORT
        // NOTE (R2 identity): the reference's literal `"Ghostty"` becomes the
        // product display name [`rustty_identity::APP_NAME_TITLECASE`].
        let title_owned;
        let title_str: &str = if title.is_empty() {
            rustty_identity::APP_NAME_TITLECASE
        } else {
            title_owned = String::from_utf8_lossy(title);
            &title_owned
        };
        let body_str = String::from_utf8_lossy(body);

        // Build the notification (`:1761-1774`).
        let notification = gio::Notification::new(title_str);
        notification.set_body(Some(&body_str));

        // `gio.ThemedIcon.new("com.mitchellh.ghostty")` (`:1765`). PORT NOTE (R2):
        // the themed-icon name IS the application id ([`build_info::BASE_APPLICATION_ID`],
        // the single source of truth).
        let icon = gio::ThemedIcon::new(crate::gtk::build_info::BASE_APPLICATION_ID);
        notification.set_icon(&icon);

        // `notification.setDefaultActionAndTargetValue("app.present-surface",
        // glib.Variant.newUint64(core_surface.id))` (`:1768-1772`).
        notification.set_default_action_and_target_value(
            "app.present-surface",
            Some(&core_id.to_variant()),
        );

        // We set the notification ID to the body content. If the content is the
        // same, this notification may replace a previous notification
        // We set the notification ID to the body content so an identical follow-up
        // replaces the prior notification (`:1774-1777`).
        app.upcast_ref::<gio::Application>()
            .send_notification(Some(&body_str), &notification);
    }

    /// Set the scrollbar state for this surface. This will setup the
    /// properties for our Gtk.Scrollable interface properly.
    ///
    /// PORTS: `surface.zig:988-1031` `setScrollbar`. Update the `Gtk.Scrollable`
    /// vadjustment in place from the core's scroll state.
    pub fn set_scrollbar(&self, scrollbar: rustty_vt::pagelist::grow::Scrollbar) {
        let imp = self.imp();

        // Update existing adjustment in-place. If we don't have an
        // adjustment then we do nothing because we're not part of a
        // scrolled window.
        // `const vadj = self.getVAdjustment() orelse return` (`:993`) — reads
        // `priv.vadj`. Clone the GObject so we don't hold the `RefCell` borrow
        // across `configure` (which may synchronously emit `value-changed`). No
        // adjustment => we're not part of a scrolled window, so nothing to do.
        let vadj = {
            let borrow = imp.vadjustment.borrow();
            match borrow.as_ref() {
                Some(adj) => adj.clone(),
                None => return,
            }
        };

        // Check if values match existing adjustment and skip update if so
        // Target values (`:996-999`).
        let value = scrollbar.offset as f64;
        let upper = scrollbar.total as f64;
        let page_size = scrollbar.len as f64;

        // Skip the update if the adjustment already matches (`:1001-1007`,
        // `approxEqAbs` with a 0.001 tolerance).
        const EPS: f64 = 0.001;
        if (vadj.value() - value).abs() < EPS
            && (vadj.upper() - upper).abs() < EPS
            && (vadj.page_size() - page_size).abs() < EPS
        {
            return;
        }

        // If we have a vadjustment we MUST have the signal group since
        // it is setup in the prop handler.
        //
        // During manual scrollbar changes from Rustty core we don't
        // want to emit value-changed signals so we block them. This would
        // cause a waste of resources at best and infinite loops at worst.
        //
        // If we have a vadjustment we MUST have the signal group since it is
        // setup in the prop handler (`:1009-1011`, `const group =
        // priv.vadj_signal_group.?`). rustty's `on_notify_vadjustment` builds the
        // group whenever a vadjustment is set, so it is present here; we tolerate
        // `None` (configure directly) rather than assert, since there is then no
        // connected handler to loop.
        //
        // During manual scrollbar changes from Ghostty core we don't want to emit
        // value-changed signals so we block them. This would cause a waste of
        // resources at best and infinite loops at worst (`:1013-1019`). The
        // scope-end `SignalGroupBlock` guard is the reference's `defer
        // group.unblock()`. The typed binding is behind a glib feature this build
        // does not enable, so block/unblock go through the libgobject-2.0 symbols.
        let _block_guard = imp
            .vadj_signal_group
            .borrow()
            .clone()
            .map(SignalGroupBlock::new);

        // vadj.configure(
        //     value, // value: current scroll position
        //     0, // lower: minimum value
        //     upper, // upper: maximum value (total scrollable area)
        //     1, // step_increment: amount to scroll on arrow click
        //     page_size, // page_increment: amount to scroll on page up/down
        //     page_size, // page_size: size of visible area
        // );
        // `vadj.configure(value, 0, upper, 1, page_size, page_size)` (`:1024-1031`).
        vadj.configure(value, 0.0, upper, 1.0, page_size, page_size);
    }

    /// PORTS: `surface.zig:1135-1202` `commandFinished`. Decide whether a
    /// command-finished notification is warranted (config + the one-shot
    /// `notify-on-next-command-finish` override + duration gate) and, if so, ring
    /// the bell and/or send a desktop notification.
    pub fn command_finished(&self, value: rustty_apprt_api::action::CommandFinished) -> bool {
        use gio::prelude::ActionGroupExt as _;
        use rustty_config::value_types::NotifyOnCommandFinish;

        let imp = self.imp();

        // Read + reset the one-shot `notify-on-next-command-finish` action state
        // (`:1139-1150`). Absent group / absent-or-non-boolean state => false.
        let notify_next_command_finish = {
            let group_ref = imp.action_group.borrow();
            match group_ref.as_ref() {
                None => false,
                Some(group) => {
                    let ag = group.upcast_ref::<gio::ActionGroup>();
                    match ag.action_state("notify-on-next-command-finish") {
                        // `state.isOfType(bool)` + `state.getBoolean()` (`:1145-1146`);
                        // `get::<bool>()` yields `None` for a non-boolean variant.
                        Some(state) => match state.get::<bool>() {
                            Some(notify) => {
                                // Reset to false (`:1147`).
                                ag.change_action_state(
                                    "notify-on-next-command-finish",
                                    &false.to_variant(),
                                );
                                notify
                            }
                            None => false,
                        },
                        None => false,
                    }
                }
            }
        };

        // `const config = priv.config orelse return false` (`:1152`).
        let config_ref = imp.config.borrow();
        let Some(config) = config_ref.as_ref() else {
            return false;
        };
        let cfg = config.get();

        // Config gates (`:1156-1159`), skipped when the one-shot override is set.
        if !notify_next_command_finish {
            if cfg.notify_on_command_finish == NotifyOnCommandFinish::Never {
                return true;
            }
            if cfg.notify_on_command_finish == NotifyOnCommandFinish::Unfocused && self.focused() {
                return true;
            }
        }

        // Duration gate (`:1161`).
        if value.duration.lte(cfg.notify_on_command_finish_after) {
            return true;
        }

        let action = cfg.notify_on_command_finish_action;

        // `if (action.bell) self.setBellRinging(true)` (`:1190`).
        //
        // CORRECTION (task 20, S54-F2): this called `self.ring_bell()`, whose note
        // said "In rustty the full bell lives in [`Surface::ring_bell`]"; the write
        // there was unconditional and unfrozen. See
        // [`Surface::set_bell_ringing_inner`].
        if action.bell {
            self.set_bell_ringing_inner(true);
        }

        // `if (action.notify) { ... sendDesktopNotification(title, body) }`
        // (`:1167-1200`).
        if action.notify {
            // Title (`:1170-1178`), PORTING the reference's `i18n._(...)` wrapping
            // of all three msgids via [`crate::gtk::i18n::tr`].
            let title = match value.exit_code {
                None => tr(c"Command Finished"),
                Some(0) => tr(c"Command Succeeded"),
                Some(_) => tr(c"Command Failed"),
            };

            // Body (`:1180-1196`): `"Command took {f}."` / `"Command took {f} and
            // exited with code {d}."`, the duration rounded to the millisecond and
            // rendered via `Duration`'s human formatter (the `{f}` format).
            // `std.time.ns_per_ms` == 1_000_000.
            let rounded = value.duration.round(1_000_000);
            let mut dur_buf: Vec<u8> = Vec::new();
            rounded.format(&mut dur_buf);
            let dur = String::from_utf8_lossy(&dur_buf);
            let body = match value.exit_code {
                None => format!("Command took {dur}."),
                Some(code) => format!("Command took {dur} and exited with code {code}."),
            };

            self.send_desktop_notification(title.as_bytes(), body.as_bytes());
        }

        true
    }

    /// Notify anyone interested that the readonly status has changed.
    ///
    /// PORTS: `surface.zig:1204-1208` `setReadonly`. The `readonly` property is a
    /// computed read of the core surface's flag (which core has already toggled by
    /// the time this fires); notifying re-evaluates the `reveal-child: bind
    /// template.readonly` binding in `surface.blp`, showing/hiding the read-only
    /// overlay. The `value` is ignored exactly as in the reference (`:1204`).
    pub fn set_readonly(&self, _value: rustty_apprt_api::action::Readonly) -> bool {
        // `self.as(gobject.Object).notifyByPspec(properties.readonly...)` (`:1206`).
        self.notify("readonly");
        true
    }

    // `surface.zig:934-938`:
    // /// Redraw our inspector, if there is one associated with this surface.
    // pub fn redrawInspector(self: *Self) void {
    // PORT NOTE: ported as `crate::gtk::surface::Surface::redraw_inspector`.
    /// The [`InspectorWindow`] currently attached to this surface, if any,
    /// upgraded from the weak backref (`surface.zig:891` `priv.inspector`). The
    /// backing store is a weak ref (`gtk/class/surface.rs:396`); the weak-notify
    /// (`:899`, `inspectorWeakNotify`) nulls it once the window is finalized.
    pub(crate) fn inspector(&self) -> Option<InspectorWindow> {
        self.imp()
            .inspector
            .borrow()
            .as_ref()
            .and_then(glib::object::WeakRef::upgrade)
            .and_then(|obj| obj.downcast::<InspectorWindow>().ok())
    }

    /// PORTS: `surface.zig:885-907` `controlInspector`. Show, hide, or toggle the
    /// terminal inspector window attached to this surface.
    pub fn control_inspector(&self, value: rustty_apprt_api::action::Inspector) -> bool {
        use rustty_apprt_api::action::Inspector;

        // Let's see if we have an inspector already (`:889-891`).
        if let Some(inspector) = self.inspector() {
            match value {
                Inspector::Show => {}
                // Our weak ref will set our private value to null (`:893-894`).
                Inspector::Toggle | Inspector::Hide => {
                    inspector.upcast::<gtk4::Window>().destroy();
                }
            }
        } else {
            match value {
                // `:896-901`.
                Inspector::Toggle | Inspector::Show => {
                    let inspector = InspectorWindow::new(self);
                    inspector.present();

                    // `inspector.as(gobject.Object).weakRef(inspectorWeakNotify,
                    // self)` (`:899`; the notify at `:2618-2625` nulls the
                    // backref). Discarding the returned handle keeps the notify
                    // registered (fire-once on finalize) rather than
                    // disconnecting it.
                    let self_weak = self.downgrade();
                    inspector.add_weak_ref_notify_local(move || {
                        if let Some(this) = self_weak.upgrade() {
                            *this.imp().inspector.borrow_mut() = None;
                        }
                    });

                    // `priv.inspector = inspector` (`:900`), held as a weak
                    // backref (`gtk/class/surface.rs:396`).
                    *self.imp().inspector.borrow_mut() =
                        Some(inspector.upcast_ref::<glib::Object>().downgrade());
                }

                Inspector::Hide => {}
            }
        }

        true
    }

    /// PORTS: `Surface.zig:40-42` `cgroup` → `self.surface.cgroupPath()`.
    ///
    /// **Placeholder-leaf (L6, invariant 8):** the systemd cgroup single-instance
    /// work owns this; kept inhabited returning `None`.
    #[must_use]
    pub fn cgroup_path(&self) -> Option<String> {
        None
    }

    /// Request data from the clipboard (read the clipboard). A started
    /// request completes asynchronously and will call the
    /// `completeClipboardRequest` core surface API when done.
    ///
    /// PORTS: `surface.zig` `clipboardRequest` → delegates to
    /// `Clipboard.request`.
    pub fn clipboard_request(
        &self,
        clipboard_type: Clipboard,
        state: ClipboardRequest,
    ) -> Result<ClipboardReadResult, ApprtError> {
        // The GTK apprt doesn't support the Kitty clipboard protocol
        // yet.
        // if (state == .kitty_read or state == .kitty_write or state == .list)
        //     return .unsupported;
        if matches!(
            state,
            ClipboardRequest::KittyRead(_)
                | ClipboardRequest::KittyWrite(_)
                | ClipboardRequest::List(_)
        ) {
            return Ok(ClipboardReadResult::Unsupported);
        }

        // Get our requested clipboard (`surface.zig:3861-3864`).
        let clipboard = self.clipboard_for(clipboard_type);

        // For paste requests, check if the clipboard has a text format available.
        // This is a synchronous check that allows performable keybinds to pass
        // through when the clipboard contains non-text content, e.g. images
        // (`surface.zig:3869-3875`).
        if matches!(state, ClipboardRequest::Paste(_))
            && !clipboard.formats().contains_type(String::static_type())
        {
            log::debug!(target: LOG_SCOPE, "clipboard has no text format, not starting paste request");
            // return .unavailable;
            return Ok(ClipboardReadResult::Unavailable);
        }

        // Important: we ref self here so that we can't free memory
        // while we have an outstanding clipboard read.
        //
        // PORT NOTE: `self.ref()` into the `Request` userdata, whose doc reads
        // (`surface.zig:4392-4396`):
        //   /// The request we send as userdata to the clipboard read.
        //   const Request = struct {
        //       /// "Self" is reffed so we can't dispose it until the clipboard
        //       /// read is complete. Callers must unref when done.
        // Here: we ref `self` (clone the GObject — a cheap refcount bump) into
        // the async closure so the surface can't be freed while a clipboard read is
        // outstanding. The reference `self.ref()`s into the `Request` userdata
        // (`surface.zig:3882-3886`); the gtk4-rs closure captures the clone instead.
        let this = self.clone();

        // Read (`surface.zig:3890-3894`). The completion mirrors `clipboardReadText`
        // (`surface.zig:4042-4105`).
        clipboard.read_text_async(gio::Cancellable::NONE, move |result| {
            let text = match result {
                Ok(Some(text)) => text,
                // No text on the clipboard — nothing to deliver.
                Ok(None) => return,
                Err(err) => {
                    log::warn!(target: LOG_SCOPE, "failed to read clipboard err={err}");
                    return;
                }
            };

            // Reach the core the same way the other handlers do (a single
            // atomic load returning `*mut CoreSurface`).
            let Some(core) = this.core() else {
                return;
            };

            // `data: [:0]const u8` -> raw bytes (`&[u8]`, never a lossy String
            // — R3). `completeClipboardRequest` takes `&mut self`; mirror the
            // `&mut *core` deref used by the sizeCallback/occlusion handlers.
            // (`0ce9054bf` end state: the read result travels as a single
            // text/plain representation in the CompleteClipboard payload.)
            match unsafe { &mut *core }.completeClipboardRequest(
                state,
                rustty_core::surface::CompleteClipboard {
                    contents: &[rustty_vt::clipboard::Content {
                        mime: b"text/plain",
                        data: text.as_bytes(),
                    }],
                    ..rustty_core::surface::CompleteClipboard::default()
                },
            ) {
                Ok(()) => {
                    // `Surface.signals.@"clipboard-read".impl.emit(...)`
                    // (`surface.zig:4099-4104`).
                    this.emit_by_name::<()>("clipboard-read", &[]);
                }

                // error.UnsafePaste, error.UnauthorizedPaste => confirmation
                // (`surface.zig:4079-4088`). DEFERRED to Phase B.
                Err(CoreSurfaceError::UnsafePaste | CoreSurfaceError::UnauthorizedPaste) => {
                    this.show_clipboard_confirmation(state, &text);
                }

                Err(err) => {
                    log::warn!(target: LOG_SCOPE, "failed to complete clipboard request err={err}");
                }
            }
        });

        // return .started; (`surface.zig:3896`)
        Ok(ClipboardReadResult::Started)
    }

    /// `surface.zig:4042`: A namespace for our clipboard-related functions so Surface isn't SO large.
    /// PORT NOTE: the `Clipboard` namespace's functions are methods on [`Surface`]
    /// here (`set_clipboard`, `clipboard_request`, `paste`, `clipboard_for`, ...).
    ///
    /// PORTS: `surface.zig:1726-1738` `setClipboard` → delegates to `Clipboard.set`
    /// (`surface.zig:3760-3847`). Set the clipboard contents (a WRITE).
    pub fn set_clipboard(
        &self,
        clipboard_type: Clipboard,
        contents: &[ClipboardContent<'_>],
        confirm: bool,
    ) {
        // Grab our plaintext content for use in confirmation dialogs
        // and signals. We always expect one to exist. (`surface.zig:3768-3774`)
        let Some(text_content) = contents.iter().find(|c| c.mime == b"text/plain") else {
            return;
        };
        // `const text: [:0]const u8 = ... break content.data;` — the payload's
        // BYTES. CORRECTION (task T-D, S56-F3): this was
        // `String::from_utf8_lossy(text_content.data)`, so a non-UTF-8 OSC 52
        // payload reached the `clipboard-write` signal with U+FFFD in place of its
        // bytes. The signal now carries the bytes ([`raw_string_value`]); the
        // confirmation dialog's copy is the one R13 exception (below).
        let text: &[u8] = text_content.data;

        // If no confirmation is necessary, set the clipboard (`surface.zig:3777`).
        if !confirm {
            let clipboard = self.clipboard_for(clipboard_type);

            // Build one `ContentProvider` per content, then union them all
            // (`surface.zig:3784-3826`).
            // Note: we don't need to unref the individual providers
            // because new_union takes ownership of them.
            //
            // The reference falls back to `clipboard.setText(text)` only on an
            // allocation failure:
            // If we fail to alloc, we can at least set the text content.
            // Rust's `Vec` aborts on OOM, so the fallback path is unreachable here.
            let mut providers: Vec<gdk4::ContentProvider> = Vec::with_capacity(contents.len());
            for content in contents {
                let bytes = glib::Bytes::from(content.data);
                if content.mime == b"text/plain" {
                    // Add an explicit UTF-8 encoding parameter to the
                    // text/plain type. The default charset when there is
                    // none is ASCII, and lots of things look for UTF-8
                    // specifically.
                    // The specs are not clear about the order here, but
                    // some clients apparently pick the first match in the
                    // order we set here then garble up bare 'text/plain'
                    // with non-ASCII UTF-8 content, so offer UTF-8 first.
                    //
                    // Note that under X11, GTK automatically adds the
                    // UTF8_STRING atom when this is present.
                    // (`surface.zig:3792-3817`)
                    let text_providers = [
                        gdk4::ContentProvider::for_bytes("text/plain;charset=utf-8", &bytes),
                        gdk4::ContentProvider::for_bytes("text/plain", &bytes),
                    ];
                    providers.push(gdk4::ContentProvider::new_union(&text_providers));
                } else {
                    let mime = String::from_utf8_lossy(content.mime);
                    providers.push(gdk4::ContentProvider::for_bytes(&mime, &bytes));
                }
            }

            let all = gdk4::ContentProvider::new_union(&providers);
            // _ = clipboard.setContent(all);
            //
            // PORT NOTE (task tail-s1b, N203): the reference discards the result
            // and logs nothing; this used to warn `failed to set clipboard
            // content` on gdk4-rs's `Err` arm, a diagnostic the reference does not
            // emit.
            let _ = clipboard.set_content(Some(&all));

            // Surface.signals.@"clipboard-write".impl.emit(
            //     self,
            //     null,
            //     .{ clipboard_type, text.ptr },
            //     null,
            // );
            // (`surface.zig:4117-4122`): the first param as the
            // `GhosttyApprtClipboard` enum the signal declares, the second as the
            // payload's C string, raw.
            self.emit_by_name_with_values(
                "clipboard-write",
                &[
                    ClipboardEnum::from(clipboard_type).to_value(),
                    raw_string_value(text),
                ],
            );

            return;
        }

        // showClipboardConfirmation(self, .{ .osc_52_write = clipboard_type }, text);
        // (`surface.zig:4127-4131`).
        //
        // PORT NOTE: reference defect (R13), `UPSTREAM-DEFECTS.md` UD172. The
        // reference inserts the payload's raw bytes into the dialog's
        // `GtkTextBuffer` (`showClipboardConfirmation`, `insertAtCursor(str,
        // len)`), whose precondition is valid UTF-8. For a non-UTF-8 OSC 52 payload
        // (any program can send one) GTK rejects the insert —
        // `Gtk-CRITICAL: gtk_text_buffer_emit_insert: assertion 'g_utf8_validate
        // (text, len, NULL)' failed`, MEASURED on GTK 4.22.4 — the preview is
        // EMPTY, and confirming writes that empty buffer text to the clipboard
        // (`clipboardConfirmationConfirm` reads the buffer's `text`); in a GTK
        // built with `G_DISABLE_CHECKS` the invalid bytes reach the text btree,
        // which assumes UTF-8 (UNVERIFIED). The port keeps the conservative
        // handling it already had for this one copy: the preview shows the payload
        // with each invalid sequence replaced by U+FFFD, and confirming writes that
        // text, so the user confirms exactly what is written.
        self.show_clipboard_confirmation(
            ClipboardRequest::Osc52Write(clipboard_type),
            &String::from_utf8_lossy(text),
        );
    }

    /// Get the specific type of clipboard for a widget.
    ///
    /// PORTS: `surface.zig:3934-3943` `Clipboard.get`. Get the specific GDK
    /// clipboard for a widget: `.standard` → the widget's default clipboard,
    /// `.selection`/`.primary` → the primary (selection) clipboard.
    ///
    /// The reference returns `?*gdk.Clipboard` (nullable), but gtk4-rs's
    /// `WidgetExt::{clipboard,primary_clipboard}` always yield a `gdk::Clipboard`
    /// (the display always has one), so the `orelse return` guards in the
    /// reference callers are statically unreachable here.
    fn clipboard_for(&self, clipboard: Clipboard) -> gdk4::Clipboard {
        let widget = self.imp().gl_area.upcast_ref::<gtk4::Widget>();
        match clipboard {
            Clipboard::Standard => widget.clipboard(),
            Clipboard::Selection | Clipboard::Primary => widget.primary_clipboard(),
        }
    }

    /// PORTS: `surface.zig:3945-3984` `Clipboard.showClipboardConfirmation`.
    /// Build a `TextBuffer` for the contents, construct the
    /// [`ClipboardConfirmationDialog`], wire `confirm`/`deny`, and present it.
    fn show_clipboard_confirmation(&self, req: ClipboardRequest, text: &str) {
        // Build a text buffer for our contents (`surface.zig:3950-3953`). The
        // dialog takes its own ref via the `clipboard-contents` property; the
        // reference `defer`-unrefs its local afterwards, which the gtk4-rs
        // refcount handles automatically when this local drops.
        let contents_buf = gtk4::TextBuffer::new(None);
        contents_buf.insert_at_cursor(text);

        // `gobject.ext.newInstance(ClipboardConfirmationDialog, ...)`
        // (`surface.zig:3956-3966`) — construct, then set the three properties.
        // `can-remember` is true for OSC 52 read/write, false for paste.
        let dialog = ClipboardConfirmationDialog::new();
        let can_remember = matches!(
            req,
            ClipboardRequest::Osc52Read(_) | ClipboardRequest::Osc52Write(_)
        );
        dialog.set_clipboard_contents(Some(contents_buf));
        dialog.set_can_remember(can_remember);
        // Set `request` LAST: it fires `notify::request`, which sets the dialog's
        // heading/body from the variant.
        dialog.set_request(Some(ClipboardRequestBoxed(req)));
        // TEST-ONLY observation point (compiled out of non-test builds): the UD172
        // test reads back the preview buffer of the dialog this call built.
        #[cfg(test)]
        LAST_CLIPBOARD_CONFIRMATION.with(|last| last.replace(Some(dialog.clone())));

        // Wire confirm/deny (`surface.zig:3968-3981`). The signals carry the
        // "remember" flag as their single bool param (`args[1]`). We ref `self`
        // (a cheap GObject refcount bump) into each closure so the surface can't be
        // freed while the dialog is open — mirroring the reference's `self` capture.
        dialog.connect_local(
            "confirm",
            false,
            glib::clone!(
                #[weak(rename_to = this)]
                self,
                #[weak]
                dialog,
                #[upgrade_or]
                None,
                move |args| {
                    let remember = args[1].get::<bool>().unwrap_or(false);
                    this.clipboard_confirmation_confirm(&dialog, remember);
                    None
                }
            ),
        );
        dialog.connect_local(
            "deny",
            false,
            glib::clone!(
                #[weak(rename_to = this)]
                self,
                #[weak]
                dialog,
                #[upgrade_or]
                None,
                move |args| {
                    let remember = args[1].get::<bool>().unwrap_or(false);
                    this.clipboard_confirmation_deny(&dialog, remember);
                    None
                }
            ),
        );

        // `dialog.present(self.as(gtk.Widget))` (`surface.zig:3983`).
        dialog.present(Some(self.upcast_ref::<gtk4::Widget>()));
    }

    /// PORTS: `surface.zig:3986-4022` `clipboardConfirmationConfirm`. On confirm,
    /// optionally persist the "remember" choice to the core surface config, then
    /// complete the clipboard request with `confirmed = true`.
    fn clipboard_confirmation_confirm(&self, dialog: &ClipboardConfirmationDialog, remember: bool) {
        // `priv.core_surface orelse return` (`:3991-3992`).
        let Some(core) = self.core() else {
            return;
        };
        // `dialog.getRequest() orelse return` (`:3993`).
        let Some(req) = dialog.get_request() else {
            return;
        };

        // Handle remember (`:3995-4000`). The core config's `clipboard_read`/
        // `clipboard_write` are directly reachable and mutable here (`&mut *core`),
        // so we port the persistence faithfully (no defer).
        if remember {
            match req.0 {
                ClipboardRequest::Osc52Read(_) => {
                    unsafe { &mut *core }.config.clipboard_read = ClipboardAccess::Allow;
                }
                ClipboardRequest::Osc52Write(_) => {
                    unsafe { &mut *core }.config.clipboard_write = ClipboardAccess::Allow;
                }
                // .paste, .list, .kitty_read, .kitty_write => {},
                ClipboardRequest::Paste(_)
                | ClipboardRequest::List(_)
                | ClipboardRequest::KittyRead(_)
                | ClipboardRequest::KittyWrite(_) => {}
            }
        }

        // Get our text from the dialog's clipboard-contents buffer (`:4002-4014`).
        let Some(buffer) = dialog.get_clipboard_contents() else {
            return;
        };
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);

        // surface.completeClipboardRequest(req.*, .{
        //     .contents = &.{.{ .mime = "text/plain", .data = text }},
        //     .confirmed = true,
        // }) catch |err| { ... };
        // Raw bytes, never a lossy String (R3).
        if let Err(err) = unsafe { &mut *core }.completeClipboardRequest(
            req.0,
            rustty_core::surface::CompleteClipboard {
                contents: &[rustty_vt::clipboard::Content {
                    mime: b"text/plain",
                    data: text.as_bytes(),
                }],
                confirmed: true,
                ..rustty_core::surface::CompleteClipboard::default()
            },
        ) {
            log::warn!(target: LOG_SCOPE, "failed to complete clipboard request err={err}");
        }
    }

    /// PORTS: `surface.zig:4025-4040` `clipboardConfirmationDeny`. On deny, only
    /// the "remember" persistence happens (the request is simply left uncompleted).
    fn clipboard_confirmation_deny(&self, dialog: &ClipboardConfirmationDialog, remember: bool) {
        // `priv.core_surface orelse return` (`:4030-4031`).
        let Some(core) = self.core() else {
            return;
        };
        // `dialog.getRequest() orelse return` (`:4032`).
        let Some(req) = dialog.get_request() else {
            return;
        };

        // Handle remember (`:4034-4039`).
        if remember {
            match req.0 {
                ClipboardRequest::Osc52Read(_) => {
                    unsafe { &mut *core }.config.clipboard_read = ClipboardAccess::Deny;
                }
                ClipboardRequest::Osc52Write(_) => {
                    unsafe { &mut *core }.config.clipboard_write = ClipboardAccess::Deny;
                }
                // PORT NOTE: the reference `@panic("request should not be able
                // to be remembered")`; these requests can never set the
                // remember switch to reach here. In Rust we log instead of
                // aborting.
                ClipboardRequest::Paste(_)
                | ClipboardRequest::List(_)
                | ClipboardRequest::KittyRead(_)
                | ClipboardRequest::KittyWrite(_) => {
                    log::warn!(target: LOG_SCOPE, "request should not be able to be remembered");
                }
            }
        }
    }

    /// The baseline environment `defaultTermioEnv` starts from
    /// (`surface.zig:1596-1599`, oracle `b988efcfe`): under Flatpak the child env
    /// starts EMPTY (`std.process.Environ.Map.init(alloc)`) so the sandbox's own
    /// environment is not leaked to the host command; otherwise it is the
    /// inherited process env (`global.environMap()`).
    ///
    /// Extracted with the predicate as an argument so it is testable without a
    /// live Flatpak sandbox.
    #[must_use]
    fn termio_env_baseline(is_flatpak: bool) -> rustty_global::EnvMap {
        if is_flatpak {
            rustty_global::EnvMap::default()
        } else {
            // PORT NOTE (P3-W7 T4): `internal_os.getEnvMap(alloc)` ->
            // `global.environMap()` (`src/os/env.zig` was DELETED by `e8525c0fd`).
            rustty_global::environ_map()
        }
    }

    /// PORTS: `surface.zig:1593-1645` `defaultTermioEnv`. The default environment
    /// for the surface's termio subprocess: the Flatpak-aware baseline env
    /// (`:1596-1599`) with the language override applied and the GTK /
    /// desktop-activation vars stripped so they don't leak to the child.
    ///
    /// CORRECTION (task 20, S55-F1): this doc used to carry "PORT NOTE (deferred):
    /// the snap-path filtering (`:1625-1630`, comptime `build_config.snap` — false
    /// in this build) lands with its own tier". `snap` is a real build option
    /// (`rustty_build_config::snap()`, set by `snap/snapcraft.yaml`'s build), so a
    /// shipped snap leaked `SNAP_*` and snap-prefixed paths to child shells.
    /// [`filter_snap_paths`] is the port, called below.
    #[must_use]
    pub fn default_termio_env(&self) -> rustty_global::EnvMap {
        let app = Application::default();
        // `var env = if (internal_os.isFlatpak()) std.process.Environ.Map.init(alloc)
        // else try global.environMap();` (`:1596-1599`).
        let mut env = Self::termio_env_baseline(rustty_os::flatpak::isFlatpak());

        // Language override (`:1602-1606`).
        match app.saved_language() {
            Some(language) => {
                env.put(b"LANG", language.as_bytes());
            }
            None => {
                env.removev(b"LANG");
            }
        }

        // (`:1608-1621`)
        for key in [
            // Don't leak these GTK environment variables to child processes.
            b"GDK_DEBUG".as_slice(),
            b"GDK_DISABLE",
            b"GSK_RENDERER",
            // Remove some environment variables that are set when Rustty is launched
            // from a `.desktop` file, by D-Bus activation, or systemd.
            b"GIO_LAUNCHED_DESKTOP_FILE",
            b"GIO_LAUNCHED_DESKTOP_FILE_PID",
            b"DBUS_STARTER_ADDRESS",
            b"DBUS_STARTER_BUS_TYPE",
            b"INVOCATION_ID",
            b"JOURNAL_STREAM",
            b"NOTIFY_SOCKET",
        ] {
            env.removev(key);
        }

        // Unset environment varies set by snaps if we're running in a snap.
        // This allows Ghostty to further launch additional snaps.
        // if (comptime build_config.snap) { if (env.get("SNAP") != null) try filterSnapPaths(alloc, &env); }
        if rustty_build_config::snap() && env.hasv(b"SNAP") {
            filter_snap_paths(&mut env);
        }

        // `if (ext.getAncestor(Window, self.as(gtk.Widget))) |window| { try
        // window.winproto().addSubprocessEnv(&env); if (window.isQuickTerminal())
        // try env.put("GHOSTTY_QUICK_TERMINAL", "1"); }` (`surface.zig:1636-1642`).
        //
        // This is a hack because it ties ourselves (optionally) to the
        // Window class. The right solution we should do is emit a signal
        // here where the handler can modify our EnvMap, but boxing the
        // EnvMap is a bit annoying so I'm punting it.
        if let Some(window) = crate::gtk::ext::ancestor::<crate::gtk::class::window::Window>(self) {
            if let Err(err) = window.winproto_mut().add_subprocess_env(&mut env) {
                // log.warn("winproto failed to add subprocess env err={}", .{err});
                log::warn!(
                    target: LOG_SCOPE,
                    "winproto failed to add subprocess env err={err:?}"
                );
            }

            if window.is_quick_terminal() {
                // R2: product-namespaced, unlike `WINDOWID` above.
                env.put(
                    format!("{}_QUICK_TERMINAL", rustty_identity::ENV_PREFIX).as_bytes(),
                    b"1",
                );
            }
        }

        env
    }

    /// PORTS: `surface.zig:983-986` `showOnScreenKeyboard`. Activates the
    /// on-screen keyboard through the IME context for the given event.
    #[must_use]
    pub fn show_on_screen_keyboard(&self, event: Option<&gdk4::Event>) -> bool {
        self.imp().im_context.activate_osk(event)
    }

    /// Scale x/y by the GDK device scale.
    ///
    /// PORTS: `surface.zig:1481-1495` `scaledCoordinates`. Scales `x`/`y` by the
    /// GDK device (widget) scale factor.
    fn scaled_coordinates(&self, x: f64, y: f64) -> (f64, f64) {
        let scale_factor = f64::from(
            self.imp()
                .gl_area
                .upcast_ref::<gtk4::Widget>()
                .scale_factor(),
        );
        (x * scale_factor, y * scale_factor)
    }

    /// Key press event (press or release).
    ///
    /// At a high level, we want to construct an `input.KeyEvent` and
    /// pass that to `keyCallback`. At a low level, this is more complicated
    /// than it appears because we need to construct all of this information
    /// and its not given to us.
    ///
    /// For all events, we run the GdkEvent through the input method context.
    /// This allows the input method to capture the event and trigger
    /// callbacks such as preedit, commit, etc.
    ///
    /// There are a couple important aspects to the prior paragraph: we must
    /// send ALL events through the input method context. This is because
    /// input methods use both key press and key release events to determine
    /// the state of the input method. For example, fcitx uses key release
    /// events on modifiers (i.e. ctrl+shift) to switch the input method.
    ///
    /// We set some state to note we're in a key event (self.in_keyevent)
    /// because some of the input method callbacks change behavior based on
    /// this state. For example, we don't want to send character events
    /// like "a" via the input "commit" event if we're actively processing
    /// a keypress because we'd lose access to the keycode information.
    /// However, a "commit" event may still happen outside of a keypress
    /// event from e.g. a tablet or on-screen keyboard.
    ///
    /// Finally, we take all of the information in order to determine if we have
    /// a unicode character or if we have to map the keyval to a code to
    /// get the underlying logical key, etc.
    ///
    /// Then we can emit the keyCallback.
    ///
    /// PORTS: `surface.zig:1240-1463` `keyEvent` — the tier's subtlest function.
    /// A GTK key event (press/repeat/release) is passed FIRST through the input
    /// method (`filterKeypress`), then — depending on the composition state —
    /// either consumed by the IME or encoded into a core `keyCallback`.
    ///
    /// **Invariant 9**: the whole IME state group ([`ImeState`]) is read/written
    /// coherently here. Port the whole IM block including its comments verbatim.
    ///
    /// PORT NOTE ([`ImKeyEvent`] naming): the reference's `IMKeyEvent` enum is
    /// `{ .false, .composing, .not_composing }`. rustty's [`ImKeyEvent`] (defined
    /// in T13) spells `not_composing` as [`ImKeyEvent::True`]; the mapping used
    /// throughout is `False ⇔ .false`, `Composing ⇔ .composing`, `True ⇔
    /// .not_composing`.
    fn key_event(
        &self,
        action: Action,
        ec_key: &gtk4::EventControllerKey,
        keyval: u32,
        keycode: u32,
        gtk_mods: gdk4::ModifierType,
    ) -> bool {
        let imp = self.imp();

        // `getCurrentEvent()` + cast to a `gdk.KeyEvent` (`:1249-1250`).
        let Some(event) = ec_key.current_event() else {
            return false;
        };
        let Some(key_event) = event.downcast_ref::<gdk4::KeyEvent>() else {
            return false;
        };

        // The block below is all related to input method handling. See the function
        // comment for some high level details and then the comments within
        // the block for more specifics. (`:1253-1343`)
        {
            // This can trigger an input method so we need to notify the im
            // context where the cursor is so it can render the dropdowns in the
            // correct place (`:1257-1268`).
            if let Some(core) = self.core() {
                // SAFETY: `core` is the widget's boxed, initialized core surface.
                let ime_point = unsafe { (*core).imePoint() };
                imp.im_context.set_cursor_location(&gdk4::Rectangle::new(
                    ime_point.x as i32,
                    ime_point.y as i32,
                    1,
                    1,
                ));
            }

            // We note that we're in a keypress because we want some logic to
            // depend on this. For example, we don't want to send character
            // events like "a" via the input "commit" event if we're actively
            // processing a keypress because we'd lose access to the keycode
            // information.
            //
            // We have to maintain some additional state here of whether we were
            // composing because different input methods call the callbacks in
            // different orders. For example, ibus calls commit THEN preedit end
            // but simple calls preedit end THEN commit.
            //
            // `priv.in_keyevent = if (priv.im_composing) .composing else
            // .not_composing` (`:1279`); the `defer priv.in_keyevent = .false`
            // (`:1280`) is reproduced by the explicit reset after the IM block.
            {
                let mut ime = imp.ime.borrow_mut();
                ime.in_keyevent = if ime.im_composing {
                    ImKeyEvent::Composing
                } else {
                    ImKeyEvent::True
                };
            }

            // Pass the event through the input method which returns true if handled.
            // Confusingly, not all events handled by the input method result
            // in this returning true so we have to maintain some additional
            // state about whether we were composing or not to determine if
            // we should proceed with key encoding.
            //
            // Cases where the input method does not mark the event as handled:
            //
            // - If we change the input method via keypress while we have preedit
            //   text, the input method will commit the pending text but will not
            //   mark it as handled. We use the `.composing` state to detect
            //   this case.
            //
            // - If we switch input methods (i.e. via ctrl+shift with fcitx),
            //   the input method will handle the key release event but will not
            //   mark it as handled. I don't know any way to detect this case so
            //   it will result in a key event being sent to the key callback.
            //   For Kitty text encoding, this will result in modifiers being
            //   triggered despite being technically consumed. At the time of
            //   writing, both Kitty and Alacritty have the same behavior. I
            //   know of no way to fix this. (`:1282-1302`)
            let im_handled = imp.im_context.filter_keypress(&event);
            // log.warn("GTKIM: im_handled={} im_len={} im_composing={}", .{
            //     im_handled,
            //     self.im_len,
            //     self.im_composing,
            // });

            // Read the (possibly IME-mutated) state, then reset `in_keyevent` to
            // `.false` — the block-scoped `defer` at `:1280`, which fires AFTER
            // the early-return arms below read `in_keyevent`.
            let (im_composing, in_keyevent, im_len) = {
                let ime = imp.ime.borrow();
                (ime.im_composing, ime.in_keyevent, ime.im_len)
            };
            imp.ime.borrow_mut().in_keyevent = ImKeyEvent::False;

            // If the input method handled the event, you would think we would
            // never proceed with key encoding for Rustty but that is not the
            // case. Input methods will handle basic character encoding like
            // typing "a" and we want to associate that with the key event.
            // So we have to check additional state to determine if we exit.
            // (`:1310-1336`)
            if im_handled {
                // If we are composing then we're in a preedit state and do
                // not want to encode any keys. For example: type a deadkey
                // such as single quote on a US international keyboard layout.
                // (`:1316-1319`)
                if im_composing {
                    return true;
                }

                // If we were composing and now we're not, it means that we committed
                // the text. We also don't want to encode a key event for this.
                // Example: enable Japanese input method, press "konn" and then
                // press enter. The final enter should not be encoded and "konn"
                // (in hiragana) should be written as "こん". (`:1321-1326`)
                if in_keyevent == ImKeyEvent::Composing {
                    return true;
                }

                // Not composing and our input method buffer is empty. This could
                // mean that the input method reacted to this event by activating
                // an onscreen keyboard or something equivalent. We don't know.
                // But the input method handled it and didn't give us text so
                // we will just assume we should not encode this. This handles a
                // real scenario when ibus starts the emoji input method
                // (super+.). (`:1328-1335`)
                if im_len == 0 {
                    return true;
                }
            }

            // At this point, for the sake of explanation of internal state:
            // it is possible that im_len > 0 and im_composing == false. This
            // means that we received a commit event from the input method that
            // we want associated with the key event. This is common: its how
            // basic character translation for simple inputs like "a" work.
            // (`:1338-1342`)
        }

        // We always reset the length of the im buffer. There's only one scenario
        // we reach this point with im_len > 0 and that's if we received a commit
        // event from the input method. We don't want to keep that state around
        // since we've handled it here.
        // `defer priv.im_len = 0` (`:1349`) — the guard resets on every return
        // past this point.
        let _im_len_reset = ImLenReset(&imp.ime);

        // Get the keyvals for this event (`:1352-1357`).
        let keyval_unicode = key::keyval_to_unicode(keyval);
        let keyval_unicode_unshifted = key::keyval_unicode_unshifted(
            imp.gl_area.upcast_ref::<gtk4::Widget>(),
            key_event,
            keycode,
        );

        // We want to get the physical unmapped key to process physical keybinds.
        // (These are keybinds explicitly marked as requesting physical mapping).
        // (`:1359-1379`; `ef01d5fda`: the inline remap block is now
        // `key::remap_key`, folded into `physical_key` with the GDK modifier
        // flag).
        // break :keycode gtk_key.remapKey(w3c_key, keyval, key_event.isModifier() != 0);
        let physical_key = key::physical_key(keycode, keyval, key_event.is_modifier());

        // Get our modifier for the event (`:1381-1388`).
        let mods = {
            let app = Application::default();
            let wp = app.imp().winproto.borrow();
            key::event_mods(&event, physical_key, gtk_mods, action, &wp)
        };

        // Get our consumed modifiers (`:1390-1398`).
        let consumed_mods = {
            let masked =
                key_event.consumed_modifiers().bits() & (gdk4::ffi::GDK_MODIFIER_MASK as u32);
            key::translate_mods(gdk4::ModifierType::from_bits_truncate(masked))
        };

        // log.debug("key pressed key={} keyval={x} physical_key={} composing={} text_len={} mods={}", .{
        //     key,
        //     keyval,
        //     physical_key,
        //     priv.im_composing,
        //     priv.im_len,
        //     mods,
        // });

        // If we have no UTF-8 text, we try to convert our keyval to
        // a text value. We have to do this because GTK will not process
        // "Ctrl+Shift+1" (on US keyboards) as "Ctrl+!" but instead as "".
        // But the keyval is set correctly so we can at least extract that.
        // (`:1409-1424`)
        {
            let mut ime = imp.ime.borrow_mut();
            if ime.im_len == 0 && keyval_unicode > 0 {
                if let Some(cp) = char::from_u32(keyval_unicode) {
                    // We don't want to send control characters as IM text.
                    // Control characters are handled already by the encoder
                    // directly (`:1415-1418`).
                    if (cp as u32) >= 0x20 {
                        let mut buf = [0u8; 4];
                        let encoded = cp.encode_utf8(&mut buf);
                        let bytes = encoded.as_bytes();
                        ime.im_buf[..bytes.len()].copy_from_slice(bytes);
                        ime.im_len = bytes.len() as u8;
                    }
                }
            }
        }

        // Invoke the core Ghostty logic to handle this input (`:1426-1439`).
        let Some(core) = self.core() else {
            return false;
        };
        let (utf8, composing) = {
            let ime = imp.ime.borrow();
            (ime.im_buf[..ime.im_len as usize].to_vec(), ime.im_composing)
        };
        let core_event = KeyEvent {
            action,
            key: physical_key,
            mods,
            consumed_mods,
            composing,
            utf8,
            unshifted_codepoint: keyval_unicode_unshifted,
        };
        // SAFETY: `core` is the widget's boxed, initialized core surface; called
        // on the app/main thread from the key controller.
        let effect = match unsafe { (*core).keyCallback(core_event) } {
            Ok(effect) => effect,
            Err(err) => {
                // log.err("error in key callback err={}", .{err});
                //
                // S54-F6: this was `log.warn` with NO error value. The
                // reference uses `log.err` HERE specifically
                // (`surface.zig:1454`) while its three sibling
                // `error in key callback` sites (`:2898 :2959 :3313`) are
                // `log.warn` — the level is per-site, not per-message.
                log::error!(target: LOG_SCOPE, "error in key callback err={err:?}");
                return false;
            }
        };

        match effect {
            InputEffect::Closed => true,
            InputEffect::Ignored => false,
            InputEffect::Consumed => {
                if action == Action::Press || action == Action::Repeat {
                    // If we were in the composing state then we reset our context.
                    // We do NOT want to reset if we're not in the composing state
                    // because there is other IME state that we want to preserve,
                    // such as quotation mark ordering for Chinese input
                    // (`:1444-1452`).
                    if imp.ime.borrow().im_composing {
                        imp.im_context.reset();
                        // SAFETY: `core` is the initialized core surface.
                        let _ = unsafe { (*core).preeditCallback(None) };
                    }

                    // Bell stops ringing when any key is pressed that is used by
                    // the core in any way. (`:1454-1456`)
                    self.set_bell_ringing_inner(false);

                    return true;
                }
                false
            }
        }
    }

    /// PORTS: `surface.zig:2754-2766` `updateFocus`. Records focus, drives the IME
    /// focus, schedules the (idle) core focus callback, notifies, and stops the
    /// bell on focus gain.
    fn update_focus(&self, focused: bool) {
        let imp = self.imp();
        imp.focused.set(focused);

        // `if (focused) ctx.focusIn() else ctx.focusOut()` (`:2758-2759`).
        if focused {
            imp.im_context.focus_in();
        } else {
            imp.im_context.focus_out();
        }

        // `idleFocus` (`:2761,2768-2783`):
        // The focus callback must be triggered on an idle loop source because
        // there are actions within librustty callbacks (such as showing close
        // confirmation dialogs) that can trigger focus loss and cause a deadlock
        // because the lock may be held during the callback.
        //
        // Userdata should be a `*Surface`. This will unref once.
        //
        // The reference `ref`s the widget and
        // `unref`s in the callback; capturing an owned strong ref here is the same
        // (dropped at the closure's end).
        let obj = self.clone();
        glib::idle_add_local_once(move || {
            let Some(core) = obj.core() else {
                return;
            };
            // SAFETY: `core` points at this widget's boxed, initialized core
            // surface (pinned in the imp); the idle source runs on the app thread.
            if let Err(err) = unsafe { &mut *core }.focusCallback(obj.imp().focused.get()) {
                // log.warn("error in focus callback err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in focus callback err={err:?}");
            }
        });

        self.notify("focused");

        // Bell stops ringing as soon as we gain focus (`:2830`).
        //
        // CORRECTION (task 20, S55-F2): this used the generated property setter,
        // under a note calling it "set + notify-on-change" and saying the
        // freeze-notify semantics "only matter for `setBellRinging(true)`". The
        // generated setter notifies on EVERY write, so every focus-in fired
        // `notify::bell-ringing`; the change gate matters for `false` too.
        if focused {
            self.set_bell_ringing_inner(false);
        }
    }

    /// PORTS: `surface.zig:3315-3319` `updateMapped`.
    fn update_mapped(&self, mapped: bool) {
        self.imp().mapped.set(mapped);
        self.notify("mapped");
    }

    /// Update the core surface visibility based on both GTK widget and
    /// toplevel state. This is public so the window can call it when its
    /// suspended state changes.
    ///
    /// PORTS: `surface.zig:3344-3358` `updateOcclusion` (oracle `2f3814ca5`,
    /// P3-W8 T5). Before that commit this took the visibility as an argument
    /// and was private; it now derives visibility from `mapped` AND the
    /// Wayland `xdg_toplevel` suspended state, so a mapped surface on an
    /// inactive workspace (or fully covered) stops producing frames.
    pub fn update_occlusion(&self) {
        let Some(core) = self.core() else {
            return;
        };
        let visible = self.imp().mapped.get() && !self.window_suspended();
        // SAFETY: `core` is the widget's boxed, initialized core surface; called
        // on the app thread from map/unmap or from the window's
        // `notify::suspended` handler.
        if let Err(err) = unsafe { &mut *core }.occlusionCallback(visible) {
            // log.warn("error in occlusion callback err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error in occlusion callback err={err:?}");
        }
    }

    /// PORTS: `surface.zig:3360-3364` `windowSuspended` (oracle `2f3814ca5`).
    fn window_suspended(&self) -> bool {
        // const native = self.as(gtk.Widget).getNative() orelse return false;
        // const window = gobject.ext.cast(gtk.Window, native) orelse return false;
        // return window.isSuspended() != 0;
        let Some(native) = self.native() else {
            return false;
        };
        let Ok(window) = native.downcast::<gtk4::Window>() else {
            return false;
        };
        window.is_suspended()
    }

    /// PORTS: `surface.zig:3491-3520` `resizeOverlaySchedule`. Sets the overlay
    /// label to the current `cols x rows` and schedules it to show/hide.
    fn resize_overlay_schedule(&self) {
        let imp = self.imp();

        // `const surface = priv.core_surface orelse return` (`:3493`).
        let Some(core) = self.core() else {
            return;
        };

        // Only show the resize overlay if its enabled (`:3496-3501`).
        let cfg_ref = imp.config.borrow();
        let Some(config) = cfg_ref.as_ref() else {
            return;
        };
        match config.get().resize_overlay {
            rustty_config::value_types::ResizeOverlay::Always
            | rustty_config::value_types::ResizeOverlay::AfterFirst => {}
            rustty_config::value_types::ResizeOverlay::Never => return,
        }

        // If we have resize overlays enabled, setup an idler
        // to show that. We do this in an idle tick because doing it
        // during the resize results in flickering. (`:3503-3519`)
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        let grid = unsafe { &*core }.size.grid();
        let text = format!("{} x {}", grid.columns, grid.rows);
        imp.resize_overlay.set_label(Some(text.as_str()));
        imp.resize_overlay.schedule();
    }

    /// PORTS: `surface.zig:3411-3489` `initSurface`. The lazy core-surface
    /// constructor — built on the FIRST GLArea resize (**invariant 3**), NOT on
    /// realize, so we have proper initial dimensions before constructing the pty.
    ///
    /// NOTE(mitchellh) (`surface.zig:642-649`): initializing lazily on first
    /// resize is a known limitation to remove later; ported as-is — do NOT "fix".
    fn init_surface(&self) -> Result<(), InitError> {
        let imp = self.imp();
        // `assert(priv.core_surface == null)` (`:3413`).
        assert!(
            imp.core_surface.load(Ordering::Acquire).is_null(),
            "initSurface called with a core surface already present"
        );

        // We need to make the context current so we can call GL functions.
        // This is required for all surface operations. (`:3416-3424`)
        imp.gl_area.make_current();
        if let Some(err) = imp.gl_area.error() {
            log::warn!(target: LOG_SCOPE, "failed to make GL context current: {err}");
            log::warn!(target: LOG_SCOPE, "this error is usually due to a driver or gtk bug");
            // log.warn("this is a common cause of this issue: https://gitlab.gnome.org/GNOME/gtk/-/issues/4950", .{});
            log::warn!(
                target: LOG_SCOPE,
                "this is a common cause of this issue: https://gitlab.gnome.org/GNOME/gtk/-/issues/4950"
            );
            return Err(InitError::GlArea);
        }

        let app = Application::default();
        let alloc = app.allocator();

        // The core app / rt app backrefs (`app.core()` / `app.rt()`). In rustty
        // these are reconstituted from the singleton's type-erased cells; when
        // unbound (widget tests) we cannot construct a surface — bail (the lazy
        // init simply doesn't complete, leaving `core_surface` None).
        let Some(core_app) = app_core_ptr() else {
            log::warn!(target: LOG_SCOPE, "core app not bound; cannot initialize surface");
            return Err(InitError::Surface);
        };
        let Some(rt_app) = app_rt_ptr() else {
            log::warn!(target: LOG_SCOPE, "rt app not bound; cannot initialize surface");
            return Err(InitError::Surface);
        };

        // Make our pointer to store our surface
        // Our stable heap slot for the core surface (`alloc.create(CoreSurface)`,
        // **invariant 2**, `:3430`). `Box`'s allocation is the pin the renderer/io
        // threads' pointers rely on; on the error path the `MaybeUninit` box is
        // dropped WITHOUT dropping a `CoreSurface` (the faithful `alloc.destroy`).
        let mut slot = Box::<std::mem::MaybeUninit<CoreSurface>>::new_uninit();
        // `MaybeUninit::as_mut_ptr` (via the cast) — `Box::as_mut_ptr` would yield
        // `*mut MaybeUninit<_>`; the slot's payload address is layout-identical.
        let surface_ptr: *mut CoreSurface = slot.as_mut_ptr().cast::<CoreSurface>();

        // The `rt_surface` self-ref pointer handed to core (`&priv.rt_surface`,
        // `:3470`). Stable because the GObject private is pinned.
        let rt_surface: *mut ApprtSurface = self.rt().cast_mut();

        // Add ourselves to the app's surface list (`:3434`); on any later failure
        // we remove ourselves again (the `errdefer deleteSurface`, `:3435`).
        // SAFETY: `core_app` is the live `App<App>` bound by `bind_core_app`.
        unsafe { &mut *core_app }.addSurface(rt_surface);

        // Initialize our surface configuration. (`:3438-3442`), applying the
        // caller overrides and font-size / pwd (`:3445-3462`).
        let mut config = {
            let cfg_ref = imp.config.borrow();
            let core_cfg = cfg_ref
                .as_ref()
                .expect("surface config is set by init's app fallback")
                .get();
            // SAFETY: `core_app` is live; `new_config` reads the focused surface's
            // pwd through the app's validated surface list.
            rustty_core::app::new_config(unsafe { &*core_app }, core_cfg, imp.context.get())
        };

        // `try applyCommandOverrides(&config, priv.overrides.command,
        // priv.overrides.shell_integration)` (`:3533-3537`, 3650bef4e).
        apply_command_overrides(
            &mut config,
            imp.overrides_command.borrow().as_ref(),
            imp.overrides_shell_integration.get(),
        );
        // `if (priv.overrides.working_directory) |wd| ...` (`:3448-3453`).
        if let Some(wd) = imp.overrides_working_directory.borrow().as_ref() {
            let mut wd_val = WorkingDirectory::Path(wd.to_bytes().to_vec());
            wd_val.finalize();
            config.working_directory = Some(wd_val);
        }
        // Properties that can impact surface init
        // `if (priv.font_size_request) |size| config.@"font-size" = size.points` (`:3456`).
        if let Some(size) = imp.font_size_request.borrow().as_ref() {
            config.font_size = size.0.points;
        }
        // `if (priv.pwd) |pwd| config.@"working-directory" = ...` (`:3457-3462`).
        if let Some(pwd) = imp.pwd.borrow().as_ref() {
            let mut wd_val = WorkingDirectory::Path(pwd.clone().into_bytes());
            wd_val.finalize();
            config.working_directory = Some(wd_val);
        }

        // PRE-PUBLISH the stable slot pointer BEFORE `CoreSurface::init` spawns the
        // renderer/io threads (**startup soundness**). This is a rustty-specific
        // necessity, NOT in the reference: the reference's renderer thread reads its
        // surface from `renderer.surface_mailbox.surface` (set at renderer
        // construction), but rustty dropped `surface_mailbox`, so the renderer
        // thread's FIRST action (`renderer_seam.rs` `threadMain_`, crash-sentry
        // setup) calls `apprt::Surface::core()` — which reads THIS field. Publishing
        // after `init` (the reference's `:3477`) would race: the just-spawned render
        // thread could read a null `core_surface` and hit the non-null assert. We
        // publish the (stable, about-to-be-initialized) slot address now; the render
        // thread's startup read only STORES the pointer as crash metadata and never
        // dereferences it before `init` completes and sends the first wakeup, so the
        // brief "published but not yet constructed" window is sound. `Release` pairs
        // with the `Acquire` loads in `core()`.
        imp.core_surface.store(surface_ptr, Ordering::Release);

        // Initialize the surface (`:3465-3474`). `config`'s Drop is the reference's
        // `defer config.deinit()`.
        // SAFETY: `surface_ptr` is a stable, uninitialized `CoreSurface` slot;
        // `core_app`/`rt_app`/`rt_surface` are live for the call; on `Err` the slot
        // is (still) uninitialized and freed below without being dropped.
        let init_res =
            unsafe { CoreSurface::init(surface_ptr, alloc, &config, core_app, rt_app, rt_surface) };
        drop(config);

        if let Err(ref err) = init_res {
            // log.warn("failed to initialize surface err={}", .{err});
            log::warn!(target: LOG_SCOPE, "failed to initialize surface err={err:?}");
            // Un-publish the slot before freeing it — `init` failed, so no live
            // `CoreSurface` backs the pointer (and any late startup read must see
            // null again, restoring **invariant 3**).
            imp.core_surface
                .store(std::ptr::null_mut(), Ordering::Release);
            // `errdefer app.core().deleteSurface(self.rt())` (`:3435`).
            // SAFETY: `core_app` is live; `rt_surface` still points at our pinned
            // embedded apprt surface.
            unsafe { &mut *core_app }.deleteSurface(rt_surface);
            // `slot` (still `MaybeUninit`) drops here, freeing the heap slot
            // without running `CoreSurface`'s teardown (`alloc.destroy`).
            return Err(InitError::Surface);
        }

        // Store it! (`:3477-3478`). Rebuild the `Box` over the SAME heap allocation
        // (now initialized) and LEAK it back into the atomic field, transferring
        // ownership of the stable heap slot to `core_surface` (reclaimed by
        // `Box::from_raw` in finalize). The published pointer is unchanged (the box
        // is the same `surface_ptr` address, already visible to the threads); this
        // step exists to move OWNERSHIP out of the local `slot`/`boxed` so it is not
        // freed at scope end.
        // SAFETY: `init_res` is `Ok`, so the slot's payload is fully initialized;
        // the raw pointer came from `Box::into_raw` and is reconstituted exactly
        // once as `Box<CoreSurface>` (layout-identical to `Box<MaybeUninit<_>>`).
        let boxed: Box<CoreSurface> =
            unsafe { Box::from_raw(Box::into_raw(slot).cast::<CoreSurface>()) };
        let boxed_ptr = Box::into_raw(boxed);
        debug_assert_eq!(
            boxed_ptr, surface_ptr,
            "the initialized box is the same pinned slot we pre-published"
        );
        imp.core_surface.store(boxed_ptr, Ordering::Release);

        // Emit the signal that we initialized the surface (`:3481-3486`).
        self.emit_by_name::<()>("init", &[]);

        // `self.updateFocus(priv.focused)` (`:3488`).
        self.update_focus(imp.focused.get());

        Ok(())
    }
}

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
    if let Err(err) = unsafe { &mut *core }
        .performBindingAction(rustty_input::binding::Action::PromptSurfaceTitle)
    {
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

#[gtk4::template_callbacks]
impl Surface {
    // -- EventControllerFocus (surface.blp:47-50) --------------------------
    // PORT NOTE (arg order): these controllers are connected NON-swapped in the
    // blueprint (unlike the `swapped` `gl_*` GLArea callbacks), so gtk4-rs
    // delivers the vector `[emitter, ...signal_args, this]` — the EMITTING
    // controller first and the template instance (`this: &Surface`) LAST. This
    // matches the reference's Zig callback signatures exactly (e.g.
    // `ecKeyPressed(ec_key, keyval, keycode, gtk_mods, self)`).
    /// PORTS: `surface.zig:2746-2748` `ecFocusEnter`.
    #[template_callback]
    fn focus_enter(_ec: &gtk4::EventControllerFocus, this: &Surface) {
        this.update_focus(true);
    }
    /// PORTS: `surface.zig:2750-2752` `ecFocusLeave`.
    #[template_callback]
    fn focus_leave(_ec: &gtk4::EventControllerFocus, this: &Surface) {
        this.update_focus(false);
    }

    // -- EventControllerKey (surface.blp:52-55) ----------------------------
    /// PORTS: `surface.zig:2714-2728` `ecKeyPressed`.
    #[template_callback]
    fn key_pressed(
        ec_key: &gtk4::EventControllerKey,
        keyval: u32,
        keycode: u32,
        gtk_mods: gdk4::ModifierType,
        this: &Surface,
    ) -> bool {
        this.key_event(Action::Press, ec_key, keyval, keycode, gtk_mods)
    }
    /// PORTS: `surface.zig:2730-2744` `ecKeyReleased`.
    #[template_callback]
    fn key_released(
        ec_key: &gtk4::EventControllerKey,
        keyval: u32,
        keycode: u32,
        state: gdk4::ModifierType,
        this: &Surface,
    ) {
        let _ = this.key_event(Action::Release, ec_key, keyval, keycode, state);
    }

    // -- GestureClick (surface.blp:74-78) ----------------------------------
    /// PORTS: `surface.zig:2785-2857` `gcMouseDown`.
    #[template_callback]
    fn mouse_down(gesture: &gtk4::GestureClick, _n_press: i32, x: f64, y: f64, this: &Surface) {
        let imp = this.imp();
        let Some(event) = gesture.current_event() else {
            return;
        };

        // Bell stops ringing if any mouse button is pressed (`:2794-2795`).
        this.set_bell_ringing_inner(false);

        // Get our surface. If we don't have one, ignore this (`:2797-2799`).
        let Some(core) = this.core() else {
            return;
        };

        // If we don't have focus, grab it (`:2801-2806`).
        let gl_area_widget = imp.gl_area.upcast_ref::<gtk4::Widget>();
        let had_focus = gl_area_widget.has_focus();
        if !had_focus {
            gl_area_widget.grab_focus();
        }

        // Report the event (`:2808-2809`).
        let button = translate_mouse_button(gesture.current_button());

        // If this click is only transitioning split focus, suppress it so it
        // doesn't get forwarded to the terminal as a mouse event (`:2811-2816`).
        if !had_focus && button == Button::Left {
            imp.suppress_left_mouse_release.set(true);
            return;
        }

        // (488b7109b) `button == .middle and !priv.gtk_enable_primary_paste and
        // !core_surface.mouseReportingActive()`: middle clicks are forwarded to
        // the TUI when it is actively reporting mouse events, even with primary
        // paste disabled.
        //
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        if button == Button::Middle
            && !imp.gtk_enable_primary_paste.get()
            && !unsafe { (*core).mouseReportingActive() }
        {
            return;
        }

        let mods = key::translate_mods(event.modifier_state());
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        let consumed =
            match unsafe { (*core).mouseButtonCallback(ButtonState::Press, button, mods) } {
                Ok(consumed) => consumed,
                Err(err) => {
                    // log.warn("error in key callback err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "error in key callback err={err:?}");
                    false
                }
            };

        // If a right click isn't consumed, mouseButtonCallback selects the
        // hovered word and returns false. We can use this to handle the context
        // menu opening under normal scenarios (`:2835-2856`).
        if !consumed && button == Button::Right {
            this.emit_by_name::<()>("menu", &[]);

            let rect = gdk4::Rectangle::new(x as i32, y as i32, 1, 1);
            imp.context_menu.set_pointing_to(Some(&rect));
            imp.context_menu.popup();
        }
    }
    /// PORTS: `surface.zig:2859-2902` `gcMouseUp`.
    #[template_callback]
    fn mouse_up(gesture: &gtk4::GestureClick, _n_press: i32, _x: f64, _y: f64, this: &Surface) {
        let imp = this.imp();
        let Some(event) = gesture.current_event() else {
            return;
        };
        let Some(core) = this.core() else {
            return;
        };
        let button = translate_mouse_button(gesture.current_button());

        if button == Button::Left && imp.suppress_left_mouse_release.get() {
            imp.suppress_left_mouse_release.set(false);
            return;
        }

        // (488b7109b) see the matching `mouse_down` guard: forward middle-click
        // releases to a mouse-reporting TUI.
        //
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        if button == Button::Middle
            && !imp.gtk_enable_primary_paste.get()
            && !unsafe { (*core).mouseReportingActive() }
        {
            return;
        }

        let mods = key::translate_mods(event.modifier_state());
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        let consumed =
            match unsafe { (*core).mouseButtonCallback(ButtonState::Release, button, mods) } {
                Ok(consumed) => consumed,
                Err(err) => {
                    // log.warn("error in key callback err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "error in key callback err={err:?}");
                    return;
                }
            };

        // Trigger the on-screen keyboard if we have no selection,
        // and that the mouse event hasn't been intercepted by the callback.
        //
        // It's better to do this here rather than within the core callback
        // since we have direct access to the underlying gdk.Event here.
        // (`:2892-2901`)
        // SAFETY: `core` is the initialized core surface.
        if !consumed
            && button == Button::Left
            && !unsafe { (*core).hasSelection() }
            && !this.show_on_screen_keyboard(Some(&event))
        {
            log::warn!(target: LOG_SCOPE, "failed to activate the on-screen keyboard");
        }
    }

    // -- EventControllerMotion (surface.blp:69-72) -------------------------
    /// PORTS: `surface.zig:2904-2953` `ecMouseMotion`.
    #[template_callback]
    fn mouse_motion(ec: &gtk4::EventControllerMotion, x: f64, y: f64, this: &Surface) {
        let imp = this.imp();
        let Some(event) = ec.current_event() else {
            return;
        };

        let (sx, sy) = this.scaled_coordinates(x, y);
        let pos = CursorPos {
            x: sx as f32,
            y: sy as f32,
        };

        // There seem to be at least two cases where GTK issues a mouse motion
        // event without the cursor actually moving:
        // 1. GLArea is resized under the mouse. This has the unfortunate
        //    side effect of causing focus to potentially change when
        //    `focus-follows-mouse` is enabled.
        // 2. The window title is updated. This can cause the mouse to unhide
        //    incorrectly when hide-mouse-when-typing is enabled.
        // To prevent incorrect behavior, we'll only grab focus and
        // continue with callback logic if the cursor has actually moved.
        // (`:2919-2930`)
        let cur = imp.cursor_pos.get();
        let is_cursor_still = (cur.x - pos.x).abs() < 1.0 && (cur.y - pos.y).abs() < 1.0;
        if is_cursor_still {
            return;
        }

        // If we don't have focus, and we want it, grab it (`:2932-2940`).
        {
            let cfg = imp.config.borrow();
            if let Some(config) = cfg.as_ref() {
                let gl_area_widget = imp.gl_area.upcast_ref::<gtk4::Widget>();
                if !gl_area_widget.has_focus() && config.get().focus_follows_mouse {
                    gl_area_widget.grab_focus();
                }
            }
        }

        // Our pos changed, update (`:2942-2943`).
        imp.cursor_pos.set(pos);

        // Notify the callback (`:2945-2952`).
        if let Some(core) = this.core() {
            let mods = key::translate_mods(event.modifier_state());
            // SAFETY: `core` is the widget's boxed, initialized core surface.
            if let Err(err) = unsafe { (*core).cursorPosCallback(pos, Some(mods)) } {
                // log.warn("error in cursor pos callback err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in cursor pos callback err={err:?}");
            }
        }
    }
    /// PORTS: `surface.zig:2955-2976` `ecMouseLeave`.
    #[template_callback]
    fn mouse_leave(ec_motion: &gtk4::EventControllerMotion, this: &Surface) {
        let Some(event) = ec_motion.current_event() else {
            return;
        };

        // If we have a core surface then we can send the cursor pos
        // callback with an invalid position to indicate the mouse left.
        // (`:2962-2975`)
        if let Some(core) = this.core() {
            let mods = key::translate_mods(event.modifier_state());
            let pos = CursorPos { x: -1.0, y: -1.0 };
            // SAFETY: `core` is the widget's boxed, initialized core surface.
            if let Err(err) = unsafe { (*core).cursorPosCallback(pos, Some(mods)) } {
                // log.warn("error in cursor pos callback err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in cursor pos callback err={err:?}");
            }
        }
    }

    // -- EventControllerScroll (surface.blp:57-67) -------------------------
    /// PORTS: `surface.zig:2992-3023` `ecMouseScrollVertical`.
    #[template_callback]
    fn scroll_vertical(_ec: &gtk4::EventControllerScroll, x: f64, y: f64, this: &Surface) -> bool {
        let imp = this.imp();
        let Some(core) = this.core() else {
            return false;
        };

        // Multiply precision scrolls by 10 to get a better response from touchpad
        // scrolling (`:3001-3006`).
        let precision = imp.precision_scroll.get();
        let multiplier: f64 = if precision { 10.0 } else { 1.0 };
        let scroll_mods = ScrollMods {
            precision,
            ..ScrollMods::default()
        };

        let (sx, sy) = this.scaled_coordinates(x, y);
        // We invert because we apply natural scrolling to the values.
        // This behavior has existed for years without Linux users complaining
        // but I suspect we'll have to make this configurable in the future
        // or read a system setting.
        // (`:3009-3016`); written `-x * m` to satisfy clippy::neg_multiply.
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        if let Err(err) =
            unsafe { (*core).scrollCallback(-sx * multiplier, -sy * multiplier, scroll_mods) }
        {
            // log.warn("error in scroll callback err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error in scroll callback err={err:?}");
            return false;
        }

        true
    }
    /// PORTS: `surface.zig:2978-2983` `ecMouseScrollVerticalPrecisionBegin`.
    #[template_callback]
    fn scroll_vertical_begin(_ec: &gtk4::EventControllerScroll, this: &Surface) {
        this.imp().precision_scroll.set(true);
    }
    /// PORTS: `surface.zig:2985-2990` `ecMouseScrollVerticalPrecisionEnd`.
    #[template_callback]
    fn scroll_vertical_end(_ec: &gtk4::EventControllerScroll, this: &Surface) {
        this.imp().precision_scroll.set(false);
    }
    /// PORTS: `surface.zig:3025-3080` `ecMouseScrollHorizontal` (oracle
    /// `5ea45919d`).
    ///
    /// PORT NOTE (`P3-W11 T5` = `P3-W8 T5` Step 3): the tab-switch accumulator
    /// used to run unconditionally for `unit == .surface`, and any other unit
    /// was DROPPED. It now runs only when `gtk-horizontal-tab-scroll` is on AND
    /// the unit is `.surface` (i.e. a touchpad); otherwise the horizontal
    /// scroll is FORWARDED to the terminal, so applications like neovim receive
    /// it instead of it being swallowed. The predicate is extracted into
    /// [`Self::horizontal_tab_scroll_applies`] so it is testable without a
    /// widget — a test of only the accumulator arm passes without the new
    /// behaviour.
    #[template_callback]
    fn scroll_horizontal(
        ec: &gtk4::EventControllerScroll,
        x: f64,
        _y: f64,
        this: &Surface,
    ) -> bool {
        let imp = this.imp();

        // Check if horizontal tab scrolling is enabled and this is a
        // touchpad surface scroll. If not, forward to the terminal.
        //
        // `const tab_scroll_enabled = if (priv.config) |config|
        //      config.get().@"gtk-horizontal-tab-scroll" else true;`
        let tab_scroll_enabled = {
            let config_ref = imp.config.borrow();
            match config_ref.as_ref() {
                Some(config) => config.get().gtk_horizontal_tab_scroll,
                None => true,
            }
        };

        if !Self::horizontal_tab_scroll_applies(tab_scroll_enabled, ec.unit()) {
            // Forward horizontal scroll to the terminal (e.g. for neovim).
            // `const surface = priv.core_surface orelse return @intFromBool(false);`
            let Some(core) = this.core() else {
                return false;
            };
            // `const scaled = self.scaledCoordinates(x, 0);`
            let (sx, _) = this.scaled_coordinates(x, 0.0);
            // `surface.scrollCallback(scaled.x * -1, 0, .{})` — written `-sx`
            // to satisfy clippy::neg_multiply, as in `scroll_vertical`.
            //
            // SAFETY: `core` is the widget's boxed, initialized core surface.
            if let Err(err) = unsafe { (*core).scrollCallback(-sx, 0.0, ScrollMods::default()) } {
                // log.warn("error in scroll callback err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in scroll callback err={err:?}");
                return false;
            }
            return true;
        }

        let pending = imp.pending_horizontal_scroll.get() + x;
        imp.pending_horizontal_scroll.set(pending);

        if pending.abs() < 120.0 {
            // if (priv.pending_horizontal_scroll_reset) |v| {
            //     _ = glib.Source.remove(v);
            //     priv.pending_horizontal_scroll_reset = null;
            // }
            discard_source_id(imp.pending_horizontal_scroll_reset.take());

            // The reset source clears the accumulator after 500ms of inactivity
            // (`:3046`). A weak backref keeps the reference's non-owning semantics
            // (the source is removed in `dispose`, **invariant 11**).
            let weak = this.downgrade();
            let id = glib::timeout_add_local(Duration::from_millis(500), move || {
                if let Some(this) = weak.upgrade() {
                    let imp = this.imp();
                    imp.pending_horizontal_scroll.set(0.0);
                    imp.pending_horizontal_scroll_reset.replace(None);
                }
                glib::ControlFlow::Break
            });
            imp.pending_horizontal_scroll_reset.replace(Some(id));
            return true;
        }

        let action_name = if pending < 0.0 {
            "tab.next-page"
        } else {
            "tab.previous-page"
        };
        let _ = this
            .upcast_ref::<gtk4::Widget>()
            .activate_action(action_name, None);

        // if (priv.pending_horizontal_scroll_reset) |v| {
        //     _ = glib.Source.remove(v);
        //     priv.pending_horizontal_scroll_reset = null;
        // }
        discard_source_id(imp.pending_horizontal_scroll_reset.take());
        imp.pending_horizontal_scroll.set(0.0);

        true
    }

    /// The `5ea45919d` predicate, extracted so it is testable without a widget:
    /// the tab-switch accumulator runs only when the `gtk-horizontal-tab-scroll`
    /// option is on AND the scroll unit is `.surface` (a touchpad two-finger
    /// scroll). Every other combination forwards the event to the terminal.
    fn horizontal_tab_scroll_applies(tab_scroll_enabled: bool, unit: gdk4::ScrollUnit) -> bool {
        tab_scroll_enabled && matches!(unit, gdk4::ScrollUnit::Surface)
    }

    // -- DropTarget (surface.blp:224-227) ----------------------------------
    /// PORTS: `surface.zig:2627-2718` `dtDrop`.
    ///
    /// NON-SWAPPED template callback (`surface.blp:225` `drop => $drop()`): the
    /// `DropTarget` emitter is arg0 and the `Surface` template instance is the
    /// trailing user-data arg (mirrors `focus_enter`/`mouse_down`; the
    /// reference's `callconv(.c) fn(_: *gtk.DropTarget, value, x, y, self)`).
    ///
    /// Files can be dropped from Nautilus and strings from many programs; the
    /// drop-target gtypes are set IN ORDER FileList, File, string
    /// (`constructed`, `:1818-1826`), and we branch on the value's runtime type
    /// with [`ext::g_value_holds`] in the same order (`:2636`, `:2678`, `:2703`).
    #[template_callback]
    fn drop(
        _target: &gtk4::DropTarget,
        value: &glib::Value,
        _x: f64,
        _y: f64,
        this: &Surface,
    ) -> bool {
        use rustty_os::string_encoding::{AllocatingWriter, Writer};
        use rustty_os::ShellEscapeWriter;
        use std::os::unix::ffi::OsStrExt;

        // gdk.FileList: shell-escape each file path and join with newlines, then
        // paste the whole blob (`:2636-2676`).
        if ext::g_value_holds(value, gdk4::FileList::static_type()) {
            let Ok(list) = value.get::<gdk4::FileList>() else {
                return false;
            };

            // `var stream: Allocating` + `ShellEscapeWriter.init(&stream.writer)`
            // (`:2637-2641`). The escape writer excludes linefeeds so newlines
            // delimit the list of paths.
            let mut stream = AllocatingWriter::init();
            {
                let mut shell_escape_writer = ShellEscapeWriter::init(&mut stream);

                // `while (current) |item| : file.getPath()` (`:2650-2668`).
                for file in list.files() {
                    let Some(path) = file.path() else {
                        continue;
                    };
                    if let Err(err) = shell_escape_writer.writeAll(path.as_os_str().as_bytes()) {
                        // log.err("unable to write path to buffer: {}", .{err});
                        log::error!(target: LOG_SCOPE, "unable to write path to buffer: {err:?}");
                        continue;
                    }
                    if let Err(err) = shell_escape_writer.writeAll(b"\n") {
                        // log.err("unable to write to buffer: {}", .{err});
                        log::error!(target: LOG_SCOPE, "unable to write to buffer: {err:?}");
                        continue;
                    }
                }
            }

            // `Clipboard.paste(self, string)` (`:2674`).
            this.paste(stream.written());
            return true;
        }

        // gio.File: shell-escape the single path (`:2678-2701`). NOTE: the
        // reference builds the escaped string but does NOT paste it (`:2694-2700`
        // ends `return 1` without a `Clipboard.paste` call), so we mirror that —
        // the byte-faithful behaviour is to consume the drop without inserting.
        if ext::g_value_holds(value, gio::File::static_type()) {
            let Ok(file) = value.get::<gio::File>() else {
                return false;
            };
            let Some(path) = file.path() else {
                return false;
            };
            let mut stream = AllocatingWriter::init();
            {
                let mut shell_escape_writer = ShellEscapeWriter::init(&mut stream);
                if let Err(err) = shell_escape_writer.writeAll(path.as_os_str().as_bytes()) {
                    // log.err("unable to write path to buffer: {}", .{err});
                    log::error!(target: LOG_SCOPE, "unable to write path to buffer: {err:?}");
                    return false;
                }
                if let Err(err) = shell_escape_writer.writeAll(b"\n") {
                    // log.err("unable to write to buffer: {}", .{err});
                    log::error!(target: LOG_SCOPE, "unable to write to buffer: {err:?}");
                    return false;
                }
            }
            let _ = stream.written();
            return true;
        }

        // string: paste it verbatim (`:2703-2708`).
        if ext::g_value_holds(value, String::static_type()) {
            if let Ok(string) = value.get::<String>() {
                this.paste(string.as_bytes());
            }
            return true;
        }

        true
    }

    /// Paste explicit text directly into the surface, regardless of the
    /// actual clipboard contents.
    ///
    /// PORTS: `surface.zig:3898-3931` `Clipboard.paste`.
    /// Mirrors the Phase-A `clipboard_request` completion path (the core mutable
    /// deref + the `UnsafePaste`/`UnauthorizedPaste` → confirmation fallback).
    fn paste(&self, text: &[u8]) {
        // `if (text.len == 0) return;` (`:3903`).
        if text.is_empty() {
            return;
        }

        // `const surface = self.private().core_surface orelse return;` (`:3905`).
        let Some(core) = self.core() else {
            return;
        };

        // surface.completeClipboardRequest(
        //     .{ .paste = .standard },
        //     .{ .contents = &.{.{ .mime = "text/plain", .data = text }} },
        // ) catch |err| switch (err) { ... };
        // SAFETY: `core` is the widget's boxed, initialized core surface;
        // `completeClipboardRequest` takes `&mut self` (mirror the deref used by
        // the async `clipboard_request` completion above).
        match unsafe { &mut *core }.completeClipboardRequest(
            ClipboardRequest::Paste(Clipboard::Standard),
            rustty_core::surface::CompleteClipboard {
                contents: &[rustty_vt::clipboard::Content {
                    mime: b"text/plain",
                    data: text,
                }],
                ..rustty_core::surface::CompleteClipboard::default()
            },
        ) {
            Ok(()) => {}

            // error.UnsafePaste, error.UnauthorizedPaste => confirmation
            // (`:3911-3921`).
            Err(CoreSurfaceError::UnsafePaste | CoreSurfaceError::UnauthorizedPaste) => {
                self.show_clipboard_confirmation(
                    ClipboardRequest::Paste(Clipboard::Standard),
                    &String::from_utf8_lossy(text),
                );
            }

            Err(err) => {
                log::warn!(target: LOG_SCOPE, "failed to complete clipboard request err={err}");
            }
        }
    }

    // -- GLArea lifecycle (surface.blp:24-29) ------------------------------
    // These are the GLArea CHILD's signals, so `&self` is the Surface (the
    // template instance, sourced from the instance-callback user data) and the
    // emitting `gtk::GLArea` is the first explicit parameter — the gtk4-rs
    // composite-template convention (unlike the self-emitted `notify::*`
    // callbacks, where `&self` is the emitter). Ported from `surface.zig:3228-3404`.

    /// PORTS: `surface.zig:3228-3263` `glareaRealize`.
    #[template_callback]
    fn gl_realize(&self, _area: &gtk4::GLArea) {
        log::debug!(target: LOG_SCOPE, "realize");
        let imp = self.imp();

        // Make the GL area current so we can detect any OpenGL errors. If
        // we have errors here we can't render and we switch to the error
        // state. (`:3234-3245`)
        imp.gl_area.make_current();
        if let Some(err) = imp.gl_area.error() {
            log::warn!(target: LOG_SCOPE, "failed to make GL context current: {err}");
            log::warn!(target: LOG_SCOPE, "this error is almost always due to a library, driver, or GTK issue");
            // log.warn("this is a common cause of this issue: https://ghostty.org/docs/help/gtk-opengl-context", .{});
            //
            // PORT NOTE: the URL is the REFERENCE PROJECT's own documentation
            // page, not a rustty runtime identity string, so it is NOT rebranded
            // (PARITY_NAMING: only identity strings become rustty; a link to
            // upstream docs still has to resolve).
            log::warn!(
                target: LOG_SCOPE,
                "this is a common cause of this issue: https://ghostty.org/docs/help/gtk-opengl-context"
            );
            self.set_error(true);
            return;
        }

        // If we already have an initialized surface then we notify it.
        // If we don't, we'll initialize it on the first resize so we have
        // our proper initial dimensions. (`:3247-3257`)
        if let Some(core) = self.core() {
            // SAFETY: `core` is the widget's boxed, initialized core surface; the
            // renderer is aliased raw by the render thread (W5), so use raw-self.
            match unsafe {
                rustty_core::renderer_seam::Renderer::displayRealized(&raw mut (*core).renderer)
            } {
                Ok(()) => self.redraw(),
                Err(err) => {
                    // log.warn("core displayRealized failed err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "core displayRealized failed err={err:?}");
                }
            }
        }

        // Setup our input method. We do this here because this will
        // create a strong reference back to ourself and we want to be
        // able to release that in unrealize. (`:3259-3262`)
        imp.im_context.set_client_widget(Some(self));
    }

    /// PORTS: `surface.zig:3265-3297` `glareaUnrealize`.
    #[template_callback]
    fn gl_unrealize(&self, _area: &gtk4::GLArea) {
        log::debug!(target: LOG_SCOPE, "unrealize");
        let imp = self.imp();

        // Notify our core surface
        if let Some(core) = self.core() {
            // There is no guarantee that our GLArea context is current
            // when unrealize is emitted, so we need to make it current.
            // (`:3274-3276`)
            imp.gl_area.make_current();
            if let Some(err) = imp.gl_area.error() {
                // I don't know a scenario this can happen, but it means
                // we probably leaked memory because displayUnrealized
                // below frees resources that aren't specifically OpenGL
                // related. I didn't make the OpenGL renderer handle this
                // scenario because I don't know if its even possible
                // under valid circumstances, so let's log.
                log::warn!(target: LOG_SCOPE, "gl_area_make_current failed in unrealize msg={err}");
                log::warn!(target: LOG_SCOPE, "OpenGL resources and memory likely leaked");
                return;
            }
            // SAFETY: `core` is the widget's boxed, initialized core surface;
            // raw-self per the render-thread aliasing contract (W5).
            unsafe {
                rustty_core::renderer_seam::Renderer::displayUnrealized(&raw mut (*core).renderer);
            }
        }

        // Unset our input method
        // (`:3295-3296`)
        imp.im_context.set_client_widget(gtk4::Widget::NONE);
    }

    /// PORTS: `surface.zig:3299-3305` `glareaMap`.
    #[template_callback]
    fn gl_map(&self, _area: &gtk4::GLArea) {
        self.update_mapped(true);
        self.update_occlusion();
    }

    /// PORTS: `surface.zig:3307-3313` `glareaUnmap`.
    #[template_callback]
    fn gl_unmap(&self, _area: &gtk4::GLArea) {
        self.update_mapped(false);
        self.update_occlusion();
    }

    /// PORTS: `surface.zig:3328-3344` `glareaRender`. The app/main-thread draw
    /// leg of the **T16 render contract** — the ONLY place a frame is drawn.
    ///
    /// The GTK `render` signal is `swapped`-connected (surface.blp), so gtk4-rs
    /// delivers the argument vector `[self, GdkGLContext, GtkGLArea]` — `self`
    /// first, the emitting widget LAST (builder_rust_scope pushes `self` then
    /// swaps index 0 with the tail). The `render` signal carries a
    /// `GdkGLContext`, so a `_area: &GLArea` second param would read the
    /// *context* value and abort the (nounwind) FFI trampoline. We take only
    /// `&self`; the trailing context / area values are ignored positionally.
    #[template_callback]
    fn gl_render(&self) -> bool {
        // If we don't have a surface then we failed to initialize for
        // some reason and there's nothing to draw to the GLArea.
        // (`:3333-3336`, returns 1 / `true`: handled).
        let Some(core) = self.core() else {
            return true;
        };
        // SAFETY: `core` is the widget's boxed, initialized core surface; the
        // render signal fires on the app/main thread. Raw-self per W5 (the render
        // thread aliases the renderer raw; no protected `&mut` may span the call).
        match unsafe {
            rustty_core::renderer_seam::Renderer::drawFrame(&raw mut (*core).renderer, true)
        } {
            Ok(()) => true,
            Err(err) => {
                // log.warn("failed to draw frame err={}", .{err});
                log::warn!(target: LOG_SCOPE, "failed to draw frame err={err:?}");
                false
            }
        }
    }

    /// PORTS: `surface.zig:3346-3404` `glareaResize`. Caches the size, drives the
    /// content-scale / size callbacks when the surface exists, and otherwise
    /// LAZILY builds the core surface (**invariant 3** — on first resize, not
    /// realize, so we have proper initial dimensions).
    ///
    /// The GTK `resize` signal is `swapped`-connected (surface.blp), so gtk4-rs
    /// delivers the argument vector `[self, width, height, GtkGLArea]` — `self`
    /// first, the emitting widget LAST (builder_rust_scope pushes `self` then
    /// swaps index 0 with the tail). A leading `_area: &GLArea` param would read
    /// values[1] (an `i32` width) and abort the (nounwind) FFI trampoline with a
    /// type mismatch. We take `(&self, width, height)`; the trailing area value
    /// is ignored positionally.
    /// `glareaResize`'s debug block (`surface.zig:3452-3470`): the GL area's scale
    /// factor and the scale of the window's `GdkSurface`, 0 when the widget has no
    /// root or the root no surface yet.
    ///
    /// CORRECTION (task T-D, S56-F4): the port logged only `width`/`height`/`scale`
    /// under a comment saying "The window (gdk-surface) scale is elided from the
    /// log here". The reference's `window_scale={}` is restored.
    fn gl_resize_debug_log(&self, width: i32, height: i32) {
        // const widget = gl_area.as(gtk.Widget);
        let widget = self.imp().gl_area.upcast_ref::<gtk4::Widget>();
        // const scale_factor = widget.getScaleFactor();
        let scale_factor = widget.scale_factor();
        // const window_scale_factor = scale: {
        //     const root = widget.getRoot() orelse break :scale 0;
        //     const gtk_native = root.as(gtk.Native);
        //     const gdk_surface = gtk_native.getSurface() orelse break :scale 0;
        //     break :scale gdk_surface.getScaleFactor();
        // };
        let window_scale_factor = widget
            .root()
            .and_then(|root| root.upcast_ref::<gtk4::Native>().surface())
            .map_or(0, |surface| surface.scale_factor());

        // log.debug("gl resize width={} height={} scale={} window_scale={}", .{
        log::debug!(target: LOG_SCOPE, "gl resize width={width} height={height} scale={scale_factor} window_scale={window_scale_factor}");
    }

    #[template_callback]
    fn gl_resize(&self, width: i32, height: i32) {
        let imp = self.imp();

        // Some debug output to help understand what GTK is telling us.
        // (`:3452-3470`)
        self.gl_resize_debug_log(width, height);

        // Store our cached size
        // (`:3371-3379`)
        let new_size = SurfaceSize {
            width: width as u32,
            height: height as u32,
        };
        let changed = !imp.size.get().eql(&new_size);
        imp.size.set(new_size);

        // If our surface is realize, we send callbacks. (`:3381-3398`)
        if let Some(core) = self.core() {
            // We also update the content scale because there is no signal for
            // content scale change and it seems to trigger a resize event.
            // (`:3383-3387`) — this note is load-bearing.
            // SAFETY: `core` is the widget's boxed, initialized core surface.
            if let Err(err) = unsafe { &mut *core }.contentScaleCallback(self.get_content_scale()) {
                // log.warn("error in content scale callback err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in content scale callback err={err:?}");
            }

            if changed {
                // SAFETY: as above.
                if let Err(err) = unsafe { &mut *core }.sizeCallback(new_size) {
                    // log.warn("error in size callback err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "error in size callback err={err:?}");
                }
                // Setup our resize overlay if configured
                self.resize_overlay_schedule();
            }

            return;
        }

        // If we don't have a surface, then we initialize it.
        // (`:3400-3403`)
        if let Err(err) = self.init_surface() {
            // log.warn("surface failed to initialize err={}", .{err});
            log::warn!(target: LOG_SCOPE, "surface failed to initialize err={err:?}");
        }
    }

    // -- IMMulticontext (surface.blp:253-256) ------------------------------
    /// PORTS: `surface.zig:3076-3087` `imPreeditStart`.
    #[template_callback]
    fn im_preedit_start(_ctx: &gtk4::IMMulticontext, this: &Surface) {
        // log.warn("GTKIM: preedit start", .{});

        // Start our composing state for the input method and reset our
        // input buffer to empty. (`:3082-3086`)
        let mut ime = this.imp().ime.borrow_mut();
        ime.im_composing = true;
        ime.im_len = 0;
    }
    /// PORTS: `surface.zig:3089-3126` `imPreeditChanged`.
    #[template_callback]
    fn im_preedit_changed(ctx: &gtk4::IMMulticontext, this: &Surface) {
        // Any preedit change should mark that we're composing. Its possible this
        // is false using fcitx5-hangul and typing "dkssud<space>" ("안녕"). The
        // second "s" results in a "commit" for "안" which sets composing to false,
        // but then immediately sends a preedit change for the next symbol. With
        // composing set to false we won't commit this text. Therefore, we must
        // ensure it is set here. (`:3095-3101`)
        this.imp().ime.borrow_mut().im_composing = true;

        // We can't set our preedit on our surface unless we're realized.
        // We do this now because we want to still keep our input method
        // state coherent. (`:3103-3106`)
        let Some(core) = this.core() else {
            return;
        };

        // Get our pre-edit string that we'll use to show the user (`:3108-3116`).
        let (preedit, _attrs, _pos) = ctx.preedit_string();

        // Update our preedit state in Rustty core
        // log.warn("GTKIM: preedit change str={s}", .{str});
        // (`:3118-3125`)
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        if let Err(err) = unsafe { (*core).preeditCallback(Some(preedit.as_bytes())) } {
            // log.warn("error in preedit callback err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error in preedit callback err={err:?}");
        }
    }
    /// PORTS: `surface.zig:3128-3143` `imPreeditEnd`.
    #[template_callback]
    fn im_preedit_end(_ctx: &gtk4::IMMulticontext, this: &Surface) {
        // log.warn("GTKIM: preedit end", .{});

        // End our composing state for GTK, allowing us to commit the text.
        // (`:3134-3136`)
        this.imp().ime.borrow_mut().im_composing = false;

        // End our preedit state in Ghostty core (`:3138-3142`).
        let Some(core) = this.core() else {
            return;
        };
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        if let Err(err) = unsafe { (*core).preeditCallback(None) } {
            // log.warn("error in preedit callback err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error in preedit callback err={err:?}");
        }
    }
    /// PORTS: `surface.zig:3145-3226` `imCommit`. **Invariant 9**: how a commit
    /// event is handled depends on the current `in_keyevent` state — during a
    /// keypress a simple commit (e.g. "a") is BUFFERED for the key encoder rather
    /// than committed as IME text.
    #[template_callback]
    fn im_commit(_ctx: &gtk4::IMMulticontext, bytes: &str, this: &Surface) {
        let imp = this.imp();
        let str_bytes = bytes.as_bytes();

        // log.debug("GTKIM: input commit composing={} keyevent={} str={s}", .{
        //     self.im_composing,
        //     self.in_keyevent,
        //     str,
        // });

        // We need to handle commit specially if we're in a key event.
        // Specifically, GTK will send us a commit event for basic key
        // encodings like "a" (on a US layout keyboard). We don't want
        // to treat this as IME committed text because we want to associate
        // it with a key event (i.e. "a" key press). (`:3159-3192`)
        let in_keyevent = imp.ime.borrow().in_keyevent;
        match in_keyevent {
            // If we're not in a key event then this commit is from
            // some other source (i.e. on-screen keyboard, tablet, etc.)
            // and we want to commit the text to the core surface. (`:3165-3168`)
            ImKeyEvent::False => {}

            // If we're in a composing state and in a key event then this
            // key event is resulting in a commit of multiple keypresses
            // and we don't want to encode it alongside the keypress. (`:3170-3173`)
            ImKeyEvent::Composing => {}

            // If we're not composing then this commit is just a normal
            // key encoding and we want our key event to handle it so
            // that Rustty can be aware of the key event alongside
            // the text. (`:3175-3191`,
            // `.not_composing`). See the [`ImKeyEvent`] naming note in `key_event`.
            ImKeyEvent::True => {
                let mut ime = imp.ime.borrow_mut();
                if str_bytes.len() > ime.im_buf.len() {
                    log::warn!(target: LOG_SCOPE, "not enough buffer space for input method commit");
                    return;
                }

                // Copy our committed text to the buffer (`:3185-3187`).
                ime.im_buf[..str_bytes.len()].copy_from_slice(str_bytes);
                // **Invariant 9**: `im_len ≤ 128`, the buffer's length (guarded
                // just above).
                //
                // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD90.
                // The reference's `im_len` is a `u7` (0..=127) but the guard
                // admits a commit of exactly `im_buf.len()` = 128 bytes, so
                // `@intCast(str.len)` traps in ReleaseSafe on a 128-byte commit
                // and truncates it to 0 in ReleaseFast (measured, Zig 0.16.0),
                // silently dropping the text. The input method (IBus, fcitx,
                // XCompose) decides the commit. The port's `u8` holds 128, so the
                // commit is buffered whole, which is what the guard intends.
                debug_assert!(str_bytes.len() <= ime.im_buf.len());
                ime.im_len = str_bytes.len() as u8;

                // log.debug("input commit len={}", .{priv.im_len});
                return;
            }
        }

        // If we reach this point from above it means we're composing OR
        // not in a keypress. In either case, we want to commit the text
        // given to us because that's what GTK is asking us to do. If we're
        // not in a keypress it means that this commit came via a non-keyboard
        // event (i.e. on-screen keyboard, tablet of some kind, etc.).
        // (`:3194-3198`)

        // Committing ends composing state (`:3200-3201`).
        imp.ime.borrow_mut().im_composing = false;

        // We can't set our preedit on our surface unless we're realized.
        // We do this now because we want to still keep our input method
        // state coherent. (`:3203-3225`)
        if let Some(core) = this.core() {
            // End our preedit state. Well-behaved input methods do this for us by
            // triggering a preedit-end event but some do not (ibus 1.5.29)
            // (`:3207-3211`).
            // SAFETY: `core` is the widget's boxed, initialized core surface.
            if let Err(err) = unsafe { (*core).preeditCallback(None) } {
                // log.warn("error in preedit callback err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in preedit callback err={err:?}");
            }

            // Send the text to the core surface, associated with no key (an
            // invalid key, which should produce no PTY encoding) (`:3213-3224`).
            let event = KeyEvent {
                action: Action::Press,
                key: Key::Unidentified,
                mods: Mods::default(),
                consumed_mods: Mods::default(),
                composing: false,
                utf8: str_bytes.to_vec(),
                unshifted_codepoint: 0,
            };
            // SAFETY: `core` is the widget's boxed, initialized core surface.
            if let Err(err) = unsafe { (*core).keyCallback(event) } {
                // log.warn("error in key callback err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in key callback err={err:?}");
            }
        }
    }

    // -- URL hover (surface.blp:171-174) -----------------------------------
    // PORT NOTE (arg order): the `url_ec_motion` EventControllerMotion is connected
    // NON-swapped in the blueprint (`surface.blp:172-173`), so gtk4-rs delivers
    // `[emitter, ...args, this]` — the emitting controller first, `this: &Surface`
    // LAST — matching the reference `callconv(.c)` signatures (`ecUrlMouseEnter`
    // takes `(_: *EventControllerMotion, _: f64, _: f64, self)`).
    /// PORTS: `surface.zig:3523-3532` `ecUrlMouseEnter`. When the pointer enters the
    /// `url_left` overlay label (bottom-left), reveal `url_right` (bottom-right) so
    /// the URL text moves to the opposite corner, out from under the cursor.
    #[template_callback]
    fn url_mouse_enter(_ec: &gtk4::EventControllerMotion, _x: f64, _y: f64, this: &Surface) {
        this.imp().url_right.set_visible(true);
    }
    /// PORTS: `surface.zig:3534-3537` `ecUrlMouseLeave`. Hide `url_right` again when
    /// the pointer leaves `url_left`.
    #[template_callback]
    fn url_mouse_leave(_ec: &gtk4::EventControllerMotion, this: &Surface) {
        this.imp().url_right.set_visible(false);
    }

    // -- child-exited / context menu ---------------------------------------
    #[template_callback]
    fn child_exited_close(_child_exited: &SurfaceChildExited, this: &Surface) {
        // PORTS: `surface.zig:2600-2603` `childExitedClose(_: *ChildExited, self)`.
        // This closes the surface with no confirmation.
        // NON-swapped signal: the emitting
        // ChildExited overlay is arg 0, `this: &Surface` is last (PORT NOTE above).
        this.close();
    }
    #[template_callback]
    fn context_menu_closed(_popover: &gtk4::PopoverMenu, this: &Surface) {
        // PORTS: `surface.zig:2608-2618` `contextMenuClosed(_: *gtk.PopoverMenu, self)`.
        // When the context menu closes, it moves focus back to the tab
        // bar if there are tabs. That's not correct. We need to grab it
        // on the surface.
        // NON-swapped signal: the emitting PopoverMenu is arg 0, `this: &Surface` last.
        this.grab_focus();
    }

    // -- notify:: property handlers (surface.blp:235-240) ------------------
    /// PORTS: `surface.zig:2258-2308` `propConfig`. **Reached during `init`**
    /// (`:1832`), so this must not panic. Pushes `resize-overlay-duration` and
    /// `resize-overlay-position` from the config onto the `resize_overlay`.
    // Renamed `on_notify_*` (blueprint name kept via `name = …`) so they don't
    // clash with the `glib::Properties`-generated `notify_<prop>()` emitters.
    #[template_callback(name = "notify_config")]
    fn on_notify_config(&self) {
        use rustty_config::value_types::ResizeOverlayPosition;

        let imp = self.imp();
        // `const config = if (priv.config) |c| c.get() else return` (`:2263-2264`).
        let cfg_ref = imp.config.borrow();
        let Some(config) = cfg_ref.as_ref() else {
            return;
        };
        let config = config.get();

        // resize-overlay-duration (`:2266-2276`).
        imp.resize_overlay
            .set_duration(config.resize_overlay_duration.as_milliseconds());

        // resize-overlay-position (`:2278-2307`).
        let (halign, valign): (gtk4::Align, gtk4::Align) = match config.resize_overlay_position {
            ResizeOverlayPosition::Center => (gtk4::Align::Center, gtk4::Align::Center),
            ResizeOverlayPosition::TopLeft => (gtk4::Align::Start, gtk4::Align::Start),
            ResizeOverlayPosition::TopRight => (gtk4::Align::End, gtk4::Align::Start),
            ResizeOverlayPosition::TopCenter => (gtk4::Align::Center, gtk4::Align::Start),
            ResizeOverlayPosition::BottomLeft => (gtk4::Align::Start, gtk4::Align::End),
            ResizeOverlayPosition::BottomRight => (gtk4::Align::End, gtk4::Align::End),
            ResizeOverlayPosition::BottomCenter => (gtk4::Align::Center, gtk4::Align::End),
        };
        imp.resize_overlay.set_overlay_halign(halign);
        imp.resize_overlay.set_overlay_valign(valign);
    }
    /// PORTS: `surface.zig:2310-2348` `propError` + `onIdleRechild`. **Reached
    /// from [`Surface::set_error`]** (the GL-error path), so this must not panic.
    /// Toggles the opaque-background CSS (`:2316-2326`) and schedules an idle-tick
    /// child swap between the `terminal_page` and the `error_page` (`:2328-2348`).
    #[template_callback(name = "notify_error")]
    fn on_notify_error(&self) {
        let widget = self.upcast_ref::<gtk4::Widget>();
        if self.imp().error.get() {
            // Ensure we have an opaque background. The window will NOT set
            // this if we have transparency set and we need an opaque
            // background for the error message to be readable. (`:2316-2322`)
            widget.add_css_class("background");
        } else {
            // Regardless of transparency setting, we remove the background
            // CSS class from this widget. Parent widgets will set it
            // appropriately (see window.zig for example). (`:2323-2326`)
            widget.remove_css_class("background");
        }

        // We need to set our child property on an idle tick, because the
        // error property can be triggered by signals that are in the middle
        // of widget mapping and changing our child during that time
        // results in a hard gtk crash. (`:2328-2335`)
        // A weak backref keeps the source non-owning (removed in `dispose`,
        // **invariant 11**).
        let imp = self.imp();
        if imp.idle_rechild.borrow().is_none() {
            let weak = self.downgrade();
            let id = glib::idle_add_local(move || {
                // PORTS: `onIdleRechild` (`:2338-2348`). Re-reads `error` at idle
                // time (not capture time), matching the reference.
                if let Some(this) = weak.upgrade() {
                    let imp = this.imp();
                    imp.idle_rechild.replace(None);
                    if imp.error.get() {
                        this.set_child(Some(&*imp.error_page));
                    } else {
                        this.set_child(Some(&*imp.terminal_page));
                    }
                }
                glib::ControlFlow::Break
            });
            imp.idle_rechild.replace(Some(id));
        }
    }
    /// PORTS: `surface.zig:2350-2358` `propMouseHoverUrl`. Reveal `url_left` (the
    /// bottom-left URL overlay label, whose text is bound to `mouse-hover-url` in the
    /// blueprint) when the hovered URL is non-empty, hide it otherwise.
    #[template_callback(name = "notify_mouse_hover_url")]
    fn on_notify_mouse_hover_url(&self) {
        let visible = self
            .imp()
            .mouse_hover_url
            .borrow()
            .as_deref()
            .is_some_and(|v| !v.is_empty());
        self.imp().url_left.set_visible(visible);
    }
    /// PORTS: `surface.zig:2360-2376` `propMouseHidden`. When the mouse is hidden set
    /// the widget cursor to `"none"`; otherwise re-run the mouse-shape logic to
    /// restore the proper cursor.
    #[template_callback(name = "notify_mouse_hidden")]
    fn on_notify_mouse_hidden(&self) {
        // If we're hidden we set it to "none" (`:2367-2371`).
        if self.imp().mouse_hidden.get() {
            self.upcast_ref::<gtk4::Widget>()
                .set_cursor_from_name(Some("none"));
            return;
        }
        // If we're not hidden we just trigger the mouse shape
        // prop notification to handle setting the proper mouse shape. (`:2373-2375`)
        self.apply_mouse_shape_cursor();
    }
    /// PORTS: `surface.zig:2378-2429` `propMouseShape`. Set the GLArea/widget cursor
    /// from the `mouse-shape` property (a no-op while the mouse is hidden).
    #[template_callback(name = "notify_mouse_shape")]
    fn on_notify_mouse_shape(&self) {
        self.apply_mouse_shape_cursor();
    }
    /// PORTS: `surface.zig:2441-2465` `propVAdjustment`. **Reached during
    /// construction** — GtkScrollable's `vadjustment` is a construct property GTK
    /// sets to a default adjustment, firing `notify::vadjustment` — so this must
    /// not panic. Lazily builds the vadj `SignalGroup`, connects `value-changed`
    /// to [`vadjValueChanged`](Self::on_notify_vadjustment) (dragging the
    /// scrollbar performs `.scroll_to_row`), and points the group at the current
    /// adjustment. Idempotent: the group is built once, then re-targeted.
    #[template_callback(name = "notify_vadjustment")]
    fn on_notify_vadjustment(&self) {
        let imp = self.imp();

        // When vadjustment is first set, we setup the signal group lazily.
        // This makes it so that if we don't use scrollbars, we never
        // pay the memory cost of this. (`:2448-2461`) The typed `gobject.SignalGroup` binding is
        // behind a glib feature this build does not enable, so the group is built
        // and connected via the libgobject-2.0 C symbols directly (see the FFI
        // block by `g_signal_group_set_target`).
        // Bind the current group out of the RefCell in its OWN statement first:
        // a `borrow()` temporary in an `if let` scrutinee lives for the whole
        // `if/else` (temporary lifetime extension), so borrowing inline would
        // still be alive when the `else` branch calls `replace()` below — a
        // guaranteed "RefCell already borrowed" abort. The `let` drops the `Ref`
        // at the semicolon.
        let existing = imp.vadj_signal_group.borrow().clone();
        let group = if let Some(group) = existing {
            group
        } else {
            // `gobject.SignalGroup.new(gtk.Adjustment.getGObjectType())` (`:2452`).
            // `g_signal_group_new` transfers full ownership of the new group.
            let group: glib::Object = unsafe {
                glib::translate::from_glib_full(g_signal_group_new(
                    glib::translate::IntoGlib::into_glib(gtk4::Adjustment::static_type()),
                ))
            };

            // `group.connect("value-changed", &vadjValueChanged, self)`
            // (`:2453-2457`). The reference passes the raw `self` pointer as the
            // untyped user-data (no ref, sidestepping a cycle); rustty captures a
            // WEAK backref to the widget in the closure for the same reason — the
            // group is owned by the widget (`vadj_signal_group`), so a strong
            // capture would leak.
            let weak = self.downgrade();
            let closure = glib::Closure::new_local(move |values| {
                // PORTS: `vadjValueChanged` (`:2430-2439`).
                // This will trigger for every single pixel change in the adjustment,
                // but our core surface handles the noise from this so that identical
                // rows are cheap.
                // `values[0]` is the emitting `gtk.Adjustment`.
                let this: Surface = weak.upgrade()?;
                // `const core_surface = self.core() orelse return` (`:2434`).
                let core = this.core()?;
                let adj: gtk4::Adjustment = values.first()?.get().ok()?;
                // `const row: usize = @intFromFloat(@round(adj.getValue()))`
                // (`:2435`).
                let row = adj.value().round() as usize;
                // `surface.performBindingAction(.{ .scroll_to_row = row })`
                // (`:2436-2438`).
                if let Err(err) = unsafe { &mut *core }
                    .performBindingAction(rustty_input::binding::Action::ScrollToRow(row))
                {
                    log::warn!(target: LOG_SCOPE, "error performing scroll_to_row action err={err:?}");
                }
                None
            });
            // SAFETY: `group` is a live `GSignalGroup`; `closure` is a live
            // `GClosure`. `g_signal_group_connect_closure` takes its own ref on
            // the closure, so the local `closure`'s scope-end unref is balanced.
            unsafe {
                g_signal_group_connect_closure(
                    group.as_ptr().cast(),
                    c"value-changed".as_ptr(),
                    closure.as_ptr(),
                    glib::ffi::GFALSE,
                );
            }

            imp.vadj_signal_group.replace(Some(group.clone()));
            group
        };

        // Setup our signal group target (`:2463-2464`).
        let target = imp.vadjustment.borrow().clone();
        // SAFETY: `group` is a live `GSignalGroup`; the target is a live
        // `gtk.Adjustment` (or null). `GSignalGroup*`/`GtkAdjustment*` are
        // ABI-compatible with `GObject*`; `set_target` is always valid.
        unsafe {
            g_signal_group_set_target(
                group.as_ptr().cast(),
                target.map_or(std::ptr::null_mut(), |t| t.as_ptr().cast()),
            );
        }
    }

    // -- bind-time closures — MUST return safe values (evaluated at init) ---
    /// Callback used to determine whether border should be shown around the
    /// surface.
    ///
    /// PORTS: `surface.zig:829-848` `closureShouldBorderBeShown`. `false` unless
    /// the bell is ringing AND the config's `bell-features.border` is set.
    #[template_callback]
    fn should_border_be_shown(&self, config: Option<Config>, bell_ringing: bool) -> bool {
        // If the bell isn't ringing exit early because when the surface is
        // first created there's a race between this code being run and the
        // config being set on the surface. That way we don't overwhelm people
        // with the warning that we issue if the config isn't set and overwhelm
        // ourselves with large numbers of bug reports. (`surface.zig:836-841`)
        if !bell_ringing {
            return false;
        }
        let Some(config) = config else {
            // log.warn("config unavailable for computing whether border should be shown, likely bug", .{});
            log::warn!(
                target: LOG_SCOPE,
                "config unavailable for computing whether border should be shown, likely bug"
            );
            return false;
        };
        // `config.@"bell-features".border` (`surface.zig:848`).
        config.get().bell_features.border
    }

    /// Callback used to determine whether unfocused-split-fill / unfocused-split-opacity
    /// should be applied to the surface
    ///
    /// PORTS: `surface.zig:853-860` `closureShouldUnfocusedSplitBeShown`.
    #[template_callback]
    fn should_unfocused_split_be_shown(
        &self,
        search_active: bool,
        focused: bool,
        is_split: bool,
    ) -> bool {
        !search_active && !focused && is_split
    }

    // -- SearchOverlay (surface.blp:192-195) -------------------------------
    // PORT NOTE (arg order): these four signals are connected NON-swapped in the
    // blueprint (unlike the `swapped` `gl_*` GLArea callbacks), so gtk4-rs
    // delivers `[emitter, ...signal_args, this]` — the emitting `SearchOverlay`
    // FIRST and the template instance (`this: &Surface`) LAST — matching the
    // reference callconv(.c) signatures (`surface.zig:3552-3577`,
    // `fn searchStop(_: *SearchOverlay, self: *Self)` etc.). A wrong (`&self`)
    // signature COMPILES but aborts at runtime (WrongValueType) when the signal
    // fires, so the shape is copied verbatim from `focus_enter`/`mouse_down`.

    /// PORTS: `surface.zig:3552-3558` `searchStop`. Perform the `end_search`
    /// binding action on the core surface, then return focus to the GLArea.
    #[template_callback]
    fn search_stop(_o: &SearchOverlay, this: &Surface) {
        let Some(core) = this.core() else {
            return;
        };
        // `surface.performBindingAction(.end_search)` (`:3554`).
        if let Err(err) =
            unsafe { &mut *core }.performBindingAction(rustty_input::binding::Action::EndSearch)
        {
            log::warn!(target: LOG_SCOPE, "unable to perform end_search action err={err:?}");
        }
        // `self.private().gl_area.as(gtk.Widget).grabFocus()` (`:3557`).
        this.grab_focus();
    }

    /// PORTS: `surface.zig:3560-3565` `searchChanged`. Drive the `search` binding
    /// action with the (nullable) needle. `needle` is the debounced search-entry
    /// text; a null needle is treated as the empty string (`std.mem.sliceTo(needle
    /// orelse "", 0)`), which the core search arm interprets as "stop search".
    #[template_callback]
    fn search_changed(_o: &SearchOverlay, needle: Option<&str>, this: &Surface) {
        let Some(core) = this.core() else {
            return;
        };
        // `.{ .search = std.mem.sliceTo(needle orelse "", 0) }` (`:3562`).
        let needle_bytes = needle.unwrap_or("").as_bytes().to_vec();
        if let Err(err) = unsafe { &mut *core }
            .performBindingAction(rustty_input::binding::Action::Search(needle_bytes))
        {
            log::warn!(target: LOG_SCOPE, "unable to perform search action err={err:?}");
        }
    }

    /// PORTS: `surface.zig:3567-3572` `searchNextMatch`. Drive
    /// `navigate_search = .next` on the core surface.
    #[template_callback]
    fn search_next_match(_o: &SearchOverlay, this: &Surface) {
        let Some(core) = this.core() else {
            return;
        };
        // `.{ .navigate_search = .next }` (`:3569`).
        if let Err(err) = unsafe { &mut *core }.performBindingAction(
            rustty_input::binding::Action::NavigateSearch(
                rustty_input::binding::NavigateSearch::Next,
            ),
        ) {
            log::warn!(target: LOG_SCOPE, "unable to perform navigate_search action err={err:?}");
        }
    }

    /// PORTS: `surface.zig:3574-3579` `searchPreviousMatch`. Drive
    /// `navigate_search = .previous` on the core surface.
    #[template_callback]
    fn search_previous_match(_o: &SearchOverlay, this: &Surface) {
        let Some(core) = this.core() else {
            return;
        };
        // `.{ .navigate_search = .previous }` (`:3576`).
        if let Err(err) = unsafe { &mut *core }.performBindingAction(
            rustty_input::binding::Action::NavigateSearch(
                rustty_input::binding::NavigateSearch::Previous,
            ),
        ) {
            log::warn!(target: LOG_SCOPE, "unable to perform navigate_search action err={err:?}");
        }
    }

    // -- Split drag-and-drop (B6, 1d053bd6e; surface.blp `drag_handle` /
    // -- `drop_overlay` / `surface_drop_target`) ---------------------------

    /// PORTS: `surface.zig:3671-3684` `closureShouldDragHandleBeShown` (bind-time
    /// closure; `&self` first like the other closures).
    #[template_callback]
    fn should_drag_handle_be_shown(&self, config: Option<Config>, is_split: bool) -> bool {
        let Some(config) = config else {
            return false;
        };

        // `switch (config.get().@"drag-handle")` (74f91d1b4's `drag-handle`
        // config option).
        match config.get().drag_handle {
            rustty_config::value_types::DragHandle::Always => true,
            rustty_config::value_types::DragHandle::Auto => is_split,
            rustty_config::value_types::DragHandle::Never => false,
        }
    }

    /// PORTS: `surface.zig:3686-3699` `surfaceDragPrepare` — the DragSource
    /// `prepare` closure: a uint64 content provider carrying this surface's
    /// core id.
    #[template_callback]
    fn surface_drag_prepare(
        _src: &gtk4::DragSource,
        _x: f64,
        _y: f64,
        this: &Surface,
    ) -> gdk4::ContentProvider {
        // TODO: Use a static content provider once we make `self.core()`
        // available immediately when constructing the surface widget
        //
        // `self.core().?.id` — the reference `.?` unwrap (panic -> abort in a C
        // callback per DG5).
        let core = this.core().expect("surface core is initialized");
        // SAFETY: a non-null `core()` result is a live, published core surface.
        let id = unsafe { (*core).id };
        gdk4::ContentProvider::for_value(&id.to_value())
    }

    /// PORTS: `surface.zig:3701-3745` `surfaceDragBegin` — set a 0.2×-scaled
    /// `WidgetPaintable` snapshot of the surface as the drag icon.
    #[template_callback]
    fn surface_drag_begin(src: &gtk4::DragSource, _drag: &gdk4::Drag, this: &Surface) {
        // The scale of the preview
        const PREVIEW_SCALE: f32 = 0.2;

        // Snapshot the entire surface widget as the icon preview
        let paintable = gtk4::WidgetPaintable::new(Some(this));

        // Center the preview
        let width = this.width();
        let height = this.height();
        let mid_x = width as f32 / 2.0;
        let mid_y = height as f32 / 2.0;

        // Create a snapshot to render the scaled paintable
        let snap = gtk4::Snapshot::new();
        snap.scale(PREVIEW_SCALE, PREVIEW_SCALE);
        paintable.snapshot(&snap, f64::from(width), f64::from(height));

        // `snap.freeToPaintable(null)` — `to_paintable` consumes the snapshot
        // (the "free" half) and returns the rendered paintable.
        if let Some(scaled) = snap.to_paintable(None) {
            src.set_icon(
                Some(&scaled),
                (mid_x * PREVIEW_SCALE) as i32,
                (mid_y * PREVIEW_SCALE) as i32,
            );
        } else {
            // The scaling process somehow failed.
            // Use the original paintable as a fail-safe
            log::warn!(target: LOG_SCOPE, "preview scaling failed - falling back to original paintable");
            src.set_icon(Some(&paintable), mid_x as i32, mid_y as i32);
        }
    }

    /// PORTS: `surface.zig:3747-3774` `surfaceDrop` — resolve the dropped
    /// surface id, then `SplitTree::move_split` it onto this surface in the
    /// pointer-derived direction.
    ///
    /// PORT NOTE: reference defect (R13), `UPSTREAM-DEFECTS.md` UD295. The
    /// reference's handler is `callconv(.c) void`, but `GtkDropTarget::drop` is a
    /// `gboolean` signal ("whether the drop was accepted"), so GTK reads an
    /// undefined return value on every surface drag-and-drop (a C
    /// function-type mismatch). In the port a `()` handler on a `gboolean` signal
    /// makes glib-rs panic inside the C callback, which ABORTS the process:
    /// MEASURED on the VM (task L-2) by emitting `drop` on this target ("Closure
    /// returned no value but the caller expected a value of type gboolean",
    /// then "panic in a function that cannot unwind"). The port returns `true`
    /// when the split was moved and `false` on every early return, the signal's
    /// documented meaning.
    #[template_callback]
    fn surface_drop(
        _tgt: &gtk4::DropTarget,
        v: &glib::Value,
        x: f64,
        y: f64,
        this: &Surface,
    ) -> bool {
        // `v.getUint64()`.
        let Ok(dropped_id) = v.get::<u64>() else {
            return false;
        };
        // `self.core().?.app.findSurfaceByID(dropped_id) orelse return`.
        let core = this.core().expect("surface core is initialized");
        // SAFETY: a non-null `core()` result is a live core surface; its `app`
        // backref is the live core App (app-thread dispatch).
        let Some(dropped) = (unsafe { (*(*core).app).findSurfaceByID(dropped_id) }) else {
            return false;
        };
        // `dropped.rt_surface.gobj()`.
        // SAFETY: `findSurfaceByID` returns a validated live surface whose
        // `rt_surface` points at the pinned apprt surface in the widget private.
        let Some(from) = (unsafe { (*(*dropped).rt_surface).widget() }) else {
            return false;
        };

        let Some(st) = ext::ancestor::<SplitTree>(this) else {
            log::warn!(target: LOG_SCOPE, "surface is not placed in a split tree");
            return false;
        };

        let dir = this.calc_drop_direction(x, y);

        // The only error that could happen here is an OOM,
        // and in that case we're already milliseconds away from crashing, so...
        if st.move_split(&from, this, dir).is_err() {
            return false;
        }

        // Clean up overlay state
        this.set_drop_overlay_direction(None);
        true
    }

    /// PORTS: `surface.zig:3776-3781` `surfaceDropLeave`.
    #[template_callback]
    fn surface_drop_leave(_tgt: &gtk4::DropTarget, this: &Surface) {
        // Hide overlay
        this.set_drop_overlay_direction(None);
    }

    /// PORTS: `surface.zig:3783-3793` `surfaceDropMotion` — recalculate the
    /// drop region and light the overlay gradient.
    #[template_callback]
    fn surface_drop_motion(
        _tgt: &gtk4::DropTarget,
        x: f64,
        y: f64,
        this: &Surface,
    ) -> gdk4::DragAction {
        // Recalculate the drop region
        let dir = this.calc_drop_direction(x, y);
        this.set_drop_overlay_direction(Some(dir));
        gdk4::DragAction::MOVE
    }

    /// PORTS: `surface.zig:3795-3809` `propDropValue` (`notify::value` on the
    /// preloading `surface_drop_target`).
    ///
    /// Reject the drop if we're dropping a surface onto itself.
    /// Note that we cannot implement this via the `accept` signal,
    /// since the decision of whether to accept or deny a drop is dependent
    /// on the payload (i.e. the surface being dropped). This is
    /// well-documented in GTK docs.
    #[template_callback]
    fn notify_drop_value(tgt: &gtk4::DropTarget, _pspec: &glib::ParamSpec, this: &Surface) {
        let Some(core_surface) = this.core() else {
            return;
        };
        let Some(value) = tgt.value() else {
            return;
        };
        let Ok(surface_id) = value.get::<u64>() else {
            return;
        };
        // SAFETY: a non-null `core()` result is a live core surface.
        if unsafe { (*core_surface).id } == surface_id {
            tgt.reject();
        }
    }
}

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
mod tests {
    use super::*;
    use crate::gtk::gresource;

    /// S87-F4: the `font-size-request` boxed type is registered under the
    /// reference's name, `GhosttyFontDesiredSize` (`src/font/face.zig:63-68`).
    /// Headless: registering a boxed GType needs no display.
    #[test]
    fn font_size_request_gtype_name() {
        use glib::prelude::StaticType as _;
        assert_eq!(
            FontSizeRequest::static_type().name(),
            "GhosttyFontDesiredSize"
        );
    }

    /// NEGATIVE CONTROL (`P3-W11 T5` = `P3-W8 T5` Step 3, oracle `5ea45919d`).
    ///
    /// The accumulator arm and the forwarding arm are selected by one
    /// predicate; a test of only the accumulator arm passes against the
    /// pre-`5ea45919d` code, which ran the accumulator for EVERY `.surface`
    /// scroll and dropped every other unit. This asserts the full table,
    /// including both new `false` rows (option off with a touchpad; option on
    /// with a wheel) that select the forwarding arm.
    ///
    /// Headless: `ScrollUnit` is a plain `gdk4` enum, so no display or
    /// `gtk4::init()` is involved (gate `G-GTKINIT`).
    #[test]
    fn horizontal_tab_scroll_predicate() {
        use gdk4::ScrollUnit;

        // The only combination that switches tabs.
        assert!(Surface::horizontal_tab_scroll_applies(
            true,
            ScrollUnit::Surface
        ));

        // Option OFF with a touchpad: NEW behaviour — forwarded, not swallowed.
        assert!(!Surface::horizontal_tab_scroll_applies(
            false,
            ScrollUnit::Surface
        ));

        // A mouse wheel: forwarded regardless of the option. Before
        // `5ea45919d` both of these returned early with `false` and the event
        // never reached the terminal.
        assert!(!Surface::horizontal_tab_scroll_applies(
            true,
            ScrollUnit::Wheel
        ));
        assert!(!Surface::horizontal_tab_scroll_applies(
            false,
            ScrollUnit::Wheel
        ));
    }

    /// PROVING TEST (W16-GTK-01): the `defaultTermioEnv` baseline is
    /// Flatpak-aware (`surface.zig:1596-1599`, oracle `b988efcfe`). Under
    /// Flatpak the child env starts EMPTY; otherwise it is the inherited
    /// process env. Before the fix the baseline was unconditionally
    /// `environ_map()`, so the `is_flatpak = true` arm below failed (the
    /// sandbox env leaked to every child).
    ///
    /// Headless: no widget, no display, no live Flatpak sandbox — the
    /// predicate is an argument (gate `G-GTKINIT`).
    #[test]
    fn termio_env_baseline_is_flatpak_aware() {
        let host = Surface::termio_env_baseline(false);
        // The inherited env is non-empty under any test runner (PATH is always
        // set), which is what makes the Flatpak arm below a real assertion and
        // not a vacuous one.
        assert_eq!(host, rustty_global::environ_map());
        assert!(
            !host.is_empty(),
            "off Flatpak the baseline must be the inherited process env",
        );

        let flatpak = Surface::termio_env_baseline(true);
        assert!(
            flatpak.is_empty(),
            "under Flatpak the baseline env must be empty, got {} entries",
            flatpak.len()
        );
    }

    /// The `#[template]` literal must equal the const-folded resolver
    /// (`surface.zig:3600-3604`), so the two never drift.
    #[test]
    fn template_resource_matches_blueprint() {
        assert_eq!(TEMPLATE_RESOURCE, gresource::blueprint(1, 2, "surface"));
    }

    /// PORTS: the `Surface` template instantiation (`surface.zig:3608-3620`).
    /// Display-dependent (GTK + a registered `GhosttyApplication` default for the
    /// config/allocator fallback in `init`); marked `#[ignore]`, run via
    /// `cargo test -p rustty-apprt --features gtk -- --ignored`.
    ///
    /// Instantiates a `Surface` and asserts all 13 template children bind
    /// (non-null) and that `gl_area` is a `gtk::GLArea`.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn surface_template_children_bind() {
        // F2: a silent skip reports `1 passed` for a test that never ran.
        crate::gtk::init_for_display_test("surface_template_children_bind");
        let _ = libadwaita::init();
        crate::gtk::register_resources();

        // A default `GhosttyApplication` must exist for `init`'s
        // `Application::default()` config/allocator fallback.
        let _app = crate::gtk::class::application::Application::build_default_for_test();

        let surface = Surface::new(SurfaceOverrides::default());
        let imp = surface.imp();

        // All 13 template children are bound (non-null).
        assert!(imp.gl_area.is_bound(), "gl_area");
        assert!(imp.url_left.is_bound(), "url_left");
        assert!(imp.url_right.is_bound(), "url_right");
        assert!(imp.child_exited_overlay.is_bound(), "child_exited_overlay");
        assert!(imp.context_menu.is_bound(), "context_menu");
        assert!(imp.error_page.is_bound(), "error_page");
        assert!(imp.progress_bar_overlay.is_bound(), "progress_bar_overlay");
        assert!(imp.resize_overlay.is_bound(), "resize_overlay");
        assert!(imp.search_overlay.is_bound(), "search_overlay");
        assert!(imp.key_state_overlay.is_bound(), "key_state_overlay");
        assert!(imp.terminal_page.is_bound(), "terminal_page");
        assert!(imp.drop_target.is_bound(), "drop_target");
        assert!(imp.im_context.is_bound(), "im_context");

        // `gl_area` is a GLArea (the render context source).
        let gl_area: &gtk4::GLArea = &imp.gl_area;
        assert_eq!(gl_area.type_(), gtk4::GLArea::static_type());
    }

    /// F-011 KAT (`surface.zig:1602-1606`): the default termio env keeps the
    /// launch-time `LANG` reported by `Application::saved_language`
    /// (`application.zig:475-478`) and removes `LANG` only when it was absent at
    /// launch. Against the pre-fix hardcoded-`None` accessor the `Some` arm is
    /// unreachable — every child env lost `LANG` — and this fails.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn default_termio_env_keeps_launch_lang() {
        use rustty_global::EnvMapExt as _;

        // F2: a silent skip reports `1 passed` for a test that never ran.
        crate::gtk::init_for_display_test("surface (display-requiring test)");
        let _ = libadwaita::init();
        crate::gtk::register_resources();

        let app = crate::gtk::class::application::Application::build_default_for_test();
        let surface = Surface::new(SurfaceOverrides::default());

        // `LANG` captured at launch → pinned into the child env.
        app.imp()
            .saved_language
            .replace(Some(std::ffi::CString::new("de_DE.UTF-8").unwrap()));
        let env = surface.default_termio_env();
        assert_eq!(
            env.getv(b"LANG"),
            Some(b"de_DE.UTF-8".as_slice()),
            "the launch-time LANG is kept in the default child env"
        );

        // `LANG` absent at launch → removed from the child env.
        app.imp().saved_language.replace(None);
        let env = surface.default_termio_env();
        assert_eq!(
            env.getv(b"LANG"),
            None,
            "LANG absent at launch is removed from the default child env"
        );
    }

    /// PORTS: `surface.zig:4122-4127` `test "computeFraction"`. Display-free.
    #[test]
    fn surface_compute_fraction() {
        assert_eq!(compute_fraction(100), 1.0);
        assert_eq!(compute_fraction(255), 1.0);
        assert_eq!(compute_fraction(0), 0.0);
        assert_eq!(compute_fraction(50), 0.5);
    }

    /// PORTS: `surface.zig:4419-4448` `test "command and shell integration
    /// overrides"` (3650bef4e; 7 `expectEqual` assertions, carried
    /// assertion-for-assertion). Display-free — pure over `Config`.
    ///
    /// NEGATIVE-CONTROL NOTE (assessment §5): the gtk modules have no G1b row,
    /// so this test is INVISIBLE to the gate — it is named here explicitly as
    /// the B5 rider test and runs in the `--features gtk` lane.
    #[test]
    fn command_and_shell_integration_overrides() {
        // var config = try configpkg.Config.default(testing.allocator);
        // defer config.deinit();
        let mut config = rustty_config::Config::default_config();

        // config.@"shell-integration" = .nushell;
        // try applyCommandOverrides(&config, .{ .shell = "vim" }, null);
        // try testing.expectEqual(.detect, config.@"shell-integration");
        config.shell_integration = ShellIntegration::Nushell;
        apply_command_overrides(&mut config, Some(&Command::Shell(b"vim".to_vec())), None);
        assert_eq!(ShellIntegration::Detect, config.shell_integration);

        // config.@"shell-integration" = .none;
        // try applyCommandOverrides(&config, .{ .shell = "vim" }, null);
        // try testing.expectEqual(.none, config.@"shell-integration");
        config.shell_integration = ShellIntegration::None;
        apply_command_overrides(&mut config, Some(&Command::Shell(b"vim".to_vec())), None);
        assert_eq!(ShellIntegration::None, config.shell_integration);

        // try applyCommandOverrides(&config, .{ .shell = "nu" }, .nushell);
        // try testing.expectEqual(.nushell, config.@"shell-integration");
        apply_command_overrides(
            &mut config,
            Some(&Command::Shell(b"nu".to_vec())),
            Some(ShellIntegration::Nushell),
        );
        assert_eq!(ShellIntegration::Nushell, config.shell_integration);

        // try applyCommandOverrides(&config, .{ .shell = "vim" }, .none);
        // try testing.expectEqual(.none, config.@"shell-integration");
        apply_command_overrides(
            &mut config,
            Some(&Command::Shell(b"vim".to_vec())),
            Some(ShellIntegration::None),
        );
        assert_eq!(ShellIntegration::None, config.shell_integration);

        // config.@"shell-integration" = .nushell;
        // try applyCommandOverrides(&config, null, null);
        // try testing.expectEqual(.nushell, config.@"shell-integration");
        config.shell_integration = ShellIntegration::Nushell;
        apply_command_overrides(&mut config, None, None);
        assert_eq!(ShellIntegration::Nushell, config.shell_integration);

        // config.@"shell-integration" = .none;
        // try applyCommandOverrides(&config, null, .nushell);
        // try testing.expectEqual(.nushell, config.@"shell-integration");
        config.shell_integration = ShellIntegration::None;
        apply_command_overrides(&mut config, None, Some(ShellIntegration::Nushell));
        assert_eq!(ShellIntegration::Nushell, config.shell_integration);

        // try applyCommandOverrides(&config, null, .none);
        // try testing.expectEqual(.none, config.@"shell-integration");
        apply_command_overrides(&mut config, None, Some(ShellIntegration::None));
        assert_eq!(ShellIntegration::None, config.shell_integration);
    }

    /// **Invariant 9 KAT** — a key event routes through the IM context FIRST, and a
    /// simple commit that arrives DURING a keypress (`in_keyevent == .not_composing`,
    /// spelled [`ImKeyEvent::True`]) is BUFFERED for the key encoder rather than
    /// committed as IME text (`surface.zig:1303,3159-3191`). This is the ordering
    /// contract at the heart of `keyEvent`: `filterKeypress` runs before any core
    /// `keyCallback`, and its re-entrant `commit` associates the text with the key
    /// event. Display-gated for the real `IMMulticontext`; core surface is `None`
    /// so the non-keyevent path is a safe no-op.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn key_press_routes_through_im_context_first() {
        let surface = display_surface();
        let imp = surface.imp();

        // During a keypress that is NOT composing, a simple IM commit ("a") is
        // buffered for the key encoder — NOT sent to the core surface — so the
        // key event can carry the text alongside the keycode.
        imp.ime.borrow_mut().in_keyevent = ImKeyEvent::True;
        Surface::im_commit(&imp.im_context, "a", &surface);
        {
            let ime = imp.ime.borrow();
            assert_eq!(
                ime.im_len, 1,
                "commit buffered exactly one byte during keyevent"
            );
            assert_eq!(&ime.im_buf[..1], b"a", "buffered the committed text");
        }

        // Outside a keypress the same commit is NOT buffered (it would be routed
        // to the core surface, which is `None` here, so it is a no-op): the buffer
        // length stays 0.
        imp.ime.borrow_mut().im_len = 0;
        imp.ime.borrow_mut().in_keyevent = ImKeyEvent::False;
        Surface::im_commit(&imp.im_context, "b", &surface);
        assert_eq!(
            imp.ime.borrow().im_len,
            0,
            "a commit outside a keyevent is not buffered for the encoder"
        );
    }

    /// UD90 (R13, task R13-D): an input-method commit of exactly 128 bytes
    /// during a keypress. The reference's `im_len: u7` cannot hold 128, which its
    /// own `str.len > im_buf.len` guard admits, so `@intCast` traps (ReleaseFast:
    /// the length becomes 0 and the text is lost). The port buffers all 128
    /// bytes; 129 still takes the reference's warn-and-return. Display-gated for
    /// the real `IMMulticontext`, as the test above.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn a_128_byte_commit_during_a_keypress_is_buffered_whole() {
        let surface = display_surface();
        let imp = surface.imp();

        // 64 two-byte characters: exactly the 128-byte buffer.
        let text = "\u{e9}".repeat(64);
        assert_eq!(128, text.len());
        imp.ime.borrow_mut().in_keyevent = ImKeyEvent::True;
        Surface::im_commit(&imp.im_context, &text, &surface);
        {
            let ime = imp.ime.borrow();
            assert_eq!(128, ime.im_len, "the whole commit is buffered");
            assert_eq!(text.as_bytes(), &ime.im_buf[..128]);
        }

        // One byte more is the reference's own "not enough buffer space" arm.
        imp.ime.borrow_mut().im_len = 0;
        let long = format!("{text}a");
        Surface::im_commit(&imp.im_context, &long, &surface);
        assert_eq!(0, imp.ime.borrow().im_len, "129 bytes are not buffered");
    }

    /// Sets up GTK + a default `GhosttyApplication` and returns a fresh `Surface`.
    /// Returns `None` when no display is available (the caller returns early).
    #[cfg(test)]
    /// F2: this used to return `None` when GTK could not init, and every caller
    /// spelled `let Some(surface) = display_surface() else { return; }` — nine
    /// `#[ignore]`d tests that each reported `1 passed` without running. It now
    /// returns the `Surface` and FAILS loudly instead.
    fn display_surface() -> Surface {
        crate::gtk::init_for_display_test("surface (display_surface)");
        let _ = libadwaita::init();
        crate::gtk::register_resources();
        let _app = crate::gtk::class::application::Application::build_default_for_test();
        Surface::new(SurfaceOverrides::default())
    }

    /// UD295 (R13): `surface_drop` answers `GtkDropTarget::drop`'s `gboolean`.
    /// A `()` handler made glib-rs panic inside the C callback and abort the
    /// process on every surface drop. A value the handler does not accept (not a
    /// surface id) must come back `false`, the rejected-drop answer, without
    /// aborting.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib surface_drop_answers -- --ignored"]
    fn surface_drop_answers_the_gboolean_drop_signal() {
        let surface = display_surface();
        let target = surface.imp().surface_drop_target.get();
        let accepted = target.emit_by_name::<bool>(
            "drop",
            &[
                &glib::value::BoxedValue("not-a-surface-id".to_value()),
                &10.0f64,
                &10.0f64,
            ],
        );
        assert!(
            !accepted,
            "a value that is not a surface id is not accepted"
        );
    }

    /// S54-F3: the `mouse-shape` / `focused` pspecs and the `clipboard-write`
    /// signal carry the reference's types and defaults (`surface.zig:154-170,
    /// 222-240, 480-492`): `mouse-shape` is `GhosttyMouseShape` defaulting to
    /// `.text`, `focused` defaults to `false` (the FIELD still starts `true`), and
    /// `clipboard-write`'s first parameter is `GhosttyApprtClipboard`.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib surface_pspec -- --ignored"]
    fn surface_pspec_types_and_defaults_match_the_reference() {
        let surface = display_surface();

        let shape = surface.find_property("mouse-shape").expect("mouse-shape");
        assert_eq!(shape.value_type(), MouseShapeEnum::static_type());
        assert_eq!(
            shape.default_value().get::<MouseShapeEnum>().ok(),
            Some(MouseShapeEnum::Text)
        );

        let focused = surface.find_property("focused").expect("focused");
        assert_eq!(focused.default_value().get::<bool>().ok(), Some(false));
        assert!(surface.focused(), "the private field still starts true");

        let id =
            glib::subclass::signal::SignalId::lookup("clipboard-write", Surface::static_type())
                .expect("clipboard-write is registered");
        let params: Vec<glib::Type> = id.query().param_types().iter().map(|t| t.type_()).collect();
        assert_eq!(
            params,
            [ClipboardEnum::static_type(), String::static_type()]
        );
    }

    /// S56-F3: a non-confirmed clipboard write emits `clipboard-write` with the
    /// payload's RAW bytes (`surface.zig:4117-4122`, `text.ptr`); a non-UTF-8
    /// OSC 52 payload is not U+FFFD-substituted. The handler reads the argument
    /// the way `window.rs`'s does, and must fire exactly once.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib clipboard_write_signal -- --ignored"]
    fn clipboard_write_signal_carries_the_raw_payload() {
        let surface = display_surface();
        let seen: std::rc::Rc<RefCell<Vec<Vec<u8>>>> = std::rc::Rc::default();
        let sink = seen.clone();
        surface.connect_local("clipboard-write", false, move |args| {
            sink.borrow_mut()
                .push(raw_string_bytes(&args[2]).expect("a non-NULL text"));
            None
        });

        surface.set_clipboard(
            Clipboard::Standard,
            &[ClipboardContent {
                mime: b"text/plain",
                data: b"caf\xe9",
            }],
            false,
        );
        assert_eq!(*seen.borrow(), [b"caf\xe9".to_vec()]);
    }

    /// UD172 (R13), the confirmation half of S56-F3: a non-UTF-8 OSC 52 payload
    /// that needs confirmation must preview as its U+FFFD-substituted text. The
    /// reference inserts the raw bytes into the `GtkTextBuffer`, whose UTF-8
    /// precondition fails (a `Gtk-CRITICAL`, measured), leaving the preview empty
    /// and a confirm that writes an empty clipboard.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib clipboard_confirmation_preview -- --ignored"]
    fn clipboard_confirmation_preview_substitutes_invalid_utf8() {
        let surface = display_surface();
        let window = gtk4::Window::new();
        window.set_child(Some(&surface));

        surface.set_clipboard(
            Clipboard::Standard,
            &[ClipboardContent {
                mime: b"text/plain",
                data: b"caf\xe9",
            }],
            true,
        );
        let dialog = LAST_CLIPBOARD_CONFIRMATION
            .with(|last| last.borrow_mut().take())
            .expect("confirm = true builds a confirmation dialog");
        let buffer = dialog
            .get_clipboard_contents()
            .expect("the dialog carries the preview buffer");
        let text: String = buffer.property("text");
        assert_eq!(text, "caf\u{FFFD}");
        window.destroy();
    }

    /// S56-F4: `glareaResize`'s debug line carries `window_scale`
    /// (`surface.zig:3452-3470`): 0 while the surface has no root, and the root's
    /// `GdkSurface` scale factor once it is in a realized window.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib gl_resize_debug -- --ignored"]
    fn gl_resize_debug_log_carries_the_window_scale() {
        let surface = display_surface();
        let scale = surface.imp().gl_area.scale_factor();

        // No root: `getRoot() orelse break :scale 0`.
        let cap = crate::log_capture::start();
        surface.gl_resize_debug_log(10, 20);
        let unrooted = cap.messages();
        drop(cap);
        assert_eq!(
            unrooted,
            [(
                log::Level::Debug,
                LOG_SCOPE.to_owned(),
                format!("gl resize width=10 height=20 scale={scale} window_scale=0"),
            )]
        );

        // In a realized window: the root's native GdkSurface exists.
        let window = gtk4::Window::new();
        window.set_child(Some(&surface));
        WidgetExt::realize(&window);
        let gdk_surface = window
            .upcast_ref::<gtk4::Native>()
            .surface()
            .expect("a realized window has a GdkSurface");
        let window_scale = gdk_surface.scale_factor();
        assert!(window_scale > 0, "a real GdkSurface scale is positive");

        let cap = crate::log_capture::start();
        surface.gl_resize_debug_log(30, 40);
        let rooted = cap.messages();
        drop(cap);
        let scale = surface.imp().gl_area.scale_factor();
        assert_eq!(
            rooted,
            [(
                log::Level::Debug,
                LOG_SCOPE.to_owned(),
                format!("gl resize width=30 height=40 scale={scale} window_scale={window_scale}"),
            )]
        );
        window.destroy();
    }

    /// S55-F4: `propMouseShape`'s switch (`surface.zig:2482-2517`), every arm, the
    /// pairs copied from the reference in its arm order. Display-free: the map is
    /// a pure function of the typed shape; the property itself is pinned by
    /// `surface_pspec_types_and_defaults_match_the_reference`.
    #[test]
    fn mouse_shape_cursor_names_match_the_reference_switch() {
        let reference: [(MouseShape, &str); 34] = [
            (MouseShape::Default, "default"),
            (MouseShape::Help, "help"),
            (MouseShape::Pointer, "pointer"),
            (MouseShape::ContextMenu, "context-menu"),
            (MouseShape::Progress, "progress"),
            (MouseShape::Wait, "wait"),
            (MouseShape::Cell, "cell"),
            (MouseShape::Crosshair, "crosshair"),
            (MouseShape::Text, "text"),
            (MouseShape::VerticalText, "vertical-text"),
            (MouseShape::Alias, "alias"),
            (MouseShape::Copy, "copy"),
            (MouseShape::NoDrop, "no-drop"),
            (MouseShape::Move, "move"),
            (MouseShape::NotAllowed, "not-allowed"),
            (MouseShape::Grab, "grab"),
            (MouseShape::Grabbing, "grabbing"),
            (MouseShape::AllScroll, "all-scroll"),
            (MouseShape::ColResize, "col-resize"),
            (MouseShape::RowResize, "row-resize"),
            (MouseShape::NResize, "n-resize"),
            (MouseShape::EResize, "e-resize"),
            (MouseShape::SResize, "s-resize"),
            (MouseShape::WResize, "w-resize"),
            (MouseShape::NeResize, "ne-resize"),
            (MouseShape::NwResize, "nw-resize"),
            (MouseShape::SeResize, "se-resize"),
            (MouseShape::SwResize, "sw-resize"),
            (MouseShape::EwResize, "ew-resize"),
            (MouseShape::NsResize, "ns-resize"),
            (MouseShape::NeswResize, "nesw-resize"),
            (MouseShape::NwseResize, "nwse-resize"),
            (MouseShape::ZoomIn, "zoom-in"),
            (MouseShape::ZoomOut, "zoom-out"),
        ];
        for (shape, name) in reference {
            assert_eq!(mouse_shape_cursor_name(shape), name, "{shape:?}");
            // And through the GObject enum the property carries.
            assert_eq!(
                mouse_shape_cursor_name(MouseShape::from(MouseShapeEnum::from(shape))),
                name
            );
        }
    }

    /// S55-F1: `filterSnapPaths` (`surface.zig:1667-1740`) over a synthetic env.
    /// Covers every branch: `SNAP_*` removed, the `SNAP` var itself removed (its own
    /// value is a snap path), a colon list partially filtered and re-joined in
    /// order, a list filtered to nothing removed, an empty snap var skipped rather
    /// than matching everything, and `TERMINFO` / product-prefixed keys untouched
    /// even when their value is a snap path. Display-free.
    #[test]
    fn filter_snap_paths_matches_the_reference() {
        use rustty_global::EnvMap;

        let mut env = EnvMap::default();
        let product_key = format!("{}_RESOURCES_DIR", rustty_identity::ENV_PREFIX);
        for (k, v) in [
            ("SNAP", "/snap/rustty/12"),
            ("SNAP_USER_DATA", "/home/u/snap/rustty/12"),
            ("SNAP_DATA", ""),
            (
                "PATH",
                "/snap/rustty/12/bin:/usr/bin:/home/u/snap/rustty/12/x:/bin",
            ),
            ("LD_LIBRARY_PATH", "/snap/rustty/12/lib"),
            ("HOME", "/home/u"),
            ("TERMINFO", "/snap/rustty/12/share/terminfo"),
            (product_key.as_str(), "/snap/rustty/12/share/rustty"),
        ] {
            env.put(k.as_bytes(), v.as_bytes());
        }

        filter_snap_paths(&mut env);

        let mut got: Vec<(String, String)> = env
            .iter()
            .map(|(k, v)| {
                (
                    String::from_utf8(k.clone()).unwrap(),
                    String::from_utf8(v.clone()).unwrap(),
                )
            })
            .collect();
        got.sort();
        let mut want = vec![
            ("HOME".to_string(), "/home/u".to_string()),
            ("PATH".to_string(), "/usr/bin:/bin".to_string()),
            (
                "TERMINFO".to_string(),
                "/snap/rustty/12/share/terminfo".to_string(),
            ),
            (
                product_key.clone(),
                "/snap/rustty/12/share/rustty".to_string(),
            ),
        ];
        want.sort();
        assert_eq!(got, want);
    }

    /// S54-F1: `setParent` inherits `parent.rt_surface.surface.getPwd()` — the
    /// parent WIDGET's `pwd` property (`surface.zig:812`), reached from the core
    /// surface's `rt_surface`. A live core surface cannot be built in this tier, so
    /// the lookup `set_parent` makes is driven directly with a real widget's pinned
    /// apprt surface: the pwd comes back, and a second widget with none gives None.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib parent_widget_pwd -- --ignored"]
    fn parent_widget_pwd_reads_the_parent_widget_property() {
        let parent = display_surface();
        let other = Surface::new(SurfaceOverrides::default());
        parent.set_pwd(Some("/srv/inherited"));

        // SAFETY: both `rt()` pointers are the widgets' pinned apprt surfaces, alive
        // for this scope.
        let got = unsafe { parent_widget_pwd(parent.rt().cast_mut()) };
        assert_eq!(got.as_deref(), Some("/srv/inherited"));
        let none = unsafe { parent_widget_pwd(other.rt().cast_mut()) };
        assert_eq!(none, None);
    }

    /// S55-F2: `setBellRinging` (`surface.zig:2296-2316`). Pins the four things the
    /// generated setter lost: the change gate (no `notify::bell-ringing` for a write
    /// that does not change the value), the focused guard, the per-call `ringBell`
    /// for every `true` even while already ringing, and the freeze that holds back
    /// notifications raised by `bell` handlers until the write is done.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib set_bell_ringing -- --ignored"]
    fn set_bell_ringing_gates_notify_and_freezes() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let surface = display_surface();
        let events: Rc<RefCell<Vec<&'static str>>> = Rc::default();

        let sink = events.clone();
        surface.connect_notify_local(Some("bell-ringing"), move |_, _| {
            sink.borrow_mut().push("notify::bell-ringing");
        });
        let sink = events.clone();
        surface.connect_notify_local(Some("zoom"), move |_, _| {
            sink.borrow_mut().push("notify::zoom");
        });
        let sink = events.clone();
        surface.connect_local("bell", false, move |args| {
            let this = args[0]
                .get::<Surface>()
                .expect("bell is emitted on a Surface");
            // A notification raised from inside a bell handler...
            this.set_zoom(!this.zoom());
            // ...must not be delivered until `setBellRinging` thaws.
            sink.borrow_mut().push("bell handler returned");
            None
        });
        let take = || std::mem::take(&mut *events.borrow_mut());

        // Unchanged value: no notify at all.
        surface.set_bell_ringing_inner(false);
        surface.set_bell_ringing_inner(false);
        assert_eq!(take(), Vec::<&str>::new());

        // Focused (the struct default): the bell rings, the state does not change.
        assert!(surface.focused());
        surface.set_bell_ringing_inner(true);
        assert_eq!(take(), ["bell handler returned", "notify::zoom"]);
        assert!(!surface.bell_ringing());

        // Unfocused: one state change, and a second ring notifies nothing new.
        surface.imp().focused.set(false);
        surface.set_bell_ringing_inner(true);
        // Both notifications come after the handler returned; their relative order
        // is GLib's notify-queue dispatch order at thaw, not the port's.
        let mut got = take();
        got[1..].sort_unstable();
        assert_eq!(
            got,
            [
                "bell handler returned",
                "notify::bell-ringing",
                "notify::zoom"
            ]
        );
        surface.set_bell_ringing_inner(true);
        assert_eq!(take(), ["bell handler returned", "notify::zoom"]);
        assert!(surface.bell_ringing());

        surface.set_bell_ringing_inner(false);
        assert_eq!(take(), ["notify::bell-ringing"]);
    }

    /// S54-F2: `commandFinished`'s bell goes through `setBellRinging(true)`
    /// (`surface.zig:1190`), so a second long command finishing while the bell is
    /// already ringing rings again but does not re-notify `bell-ringing`.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib command_finished_bell -- --ignored"]
    fn command_finished_bell_goes_through_set_bell_ringing() {
        use std::cell::Cell;
        use std::rc::Rc;

        use rustty_config::value_types::{Duration, NotifyOnCommandFinish};

        let surface = display_surface();
        let mut cfg = rustty_config::Config::default_config();
        cfg.notify_on_command_finish = NotifyOnCommandFinish::Always;
        cfg.notify_on_command_finish_action.bell = true;
        cfg.notify_on_command_finish_action.notify = false;
        cfg.notify_on_command_finish_after = Duration { duration: 0 };
        surface.imp().config.replace(Some(Config::new(&cfg)));
        surface.imp().focused.set(false);

        let bells = Rc::new(Cell::new(0u32));
        let notifies = Rc::new(Cell::new(0u32));
        let b = bells.clone();
        surface.connect_local("bell", false, move |_| {
            b.set(b.get() + 1);
            None
        });
        let n = notifies.clone();
        surface.connect_notify_local(Some("bell-ringing"), move |_, _| n.set(n.get() + 1));

        let finished = rustty_apprt_api::action::CommandFinished {
            exit_code: Some(0),
            duration: Duration {
                duration: 5_000_000_000,
            },
        };
        assert!(surface.command_finished(finished));
        assert!(surface.command_finished(finished));
        assert_eq!(bells.get(), 2, "every finished command rings");
        assert_eq!(notifies.get(), 1, "only the first is a state change");
        assert!(surface.bell_ringing());
    }

    /// **Invariant 6 / §5.2** — `dispose` is idempotent (the null-guards). Populates
    /// an OWNED row (`config`) and a glib source id, calls `dispose()` twice, and
    /// asserts no panic/double-free and that every touched field is nulled. Before
    /// the T14 free-sites landed, `dispose` left these fields `Some` — RED.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn surface_dispose_is_idempotent() {
        let surface = display_surface();
        let imp = surface.imp();

        // `config` is populated by `init`'s app fallback; also install a source id.
        assert!(imp.config.borrow().is_some(), "config seeded by init");
        let id = glib::timeout_add_seconds_local(3600, || glib::ControlFlow::Break);
        imp.progress_bar_timer.replace(Some(id));

        // Dispose twice — the second call must not double-free or panic.
        imp.dispose();
        imp.dispose();

        assert!(imp.config.borrow().is_none(), "config nulled after dispose");
        assert!(
            imp.progress_bar_timer.borrow().is_none(),
            "progress_bar_timer nulled after dispose"
        );
    }

    /// **Invariant 11** — `dispose` removes every glib source id. Installs all three
    /// (`progress_bar_timer`, `idle_rechild`, `pending_horizontal_scroll_reset`) and
    /// asserts they are nulled after a single `dispose`. Before T14, `dispose` never
    /// touched them — RED.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn surface_dispose_removes_all_source_ids() {
        let surface = display_surface();
        let imp = surface.imp();

        imp.progress_bar_timer
            .replace(Some(glib::timeout_add_seconds_local(3600, || {
                glib::ControlFlow::Break
            })));
        imp.idle_rechild
            .replace(Some(glib::timeout_add_seconds_local(3600, || {
                glib::ControlFlow::Break
            })));
        imp.pending_horizontal_scroll_reset
            .replace(Some(glib::timeout_add_seconds_local(3600, || {
                glib::ControlFlow::Break
            })));

        imp.dispose();

        assert!(
            imp.progress_bar_timer.borrow().is_none(),
            "progress_bar_timer"
        );
        assert!(imp.idle_rechild.borrow().is_none(), "idle_rechild");
        assert!(
            imp.pending_horizontal_scroll_reset.borrow().is_none(),
            "pending_horizontal_scroll_reset"
        );
    }

    /// **Invariant 3** — `core_surface` realize ordering. The core surface is NOT
    /// constructed eagerly at widget-construction time: [`Surface::core`] is `None`
    /// until the lazy `initSurface` runs on the FIRST GLArea resize
    /// (`surface.zig:3247-3257,3400-3403`), never on construction or realize. The
    /// live realize → lazy-init drive (which needs the T16 input/render signal
    /// handlers, a bound core app, a pty, and a GL context) is exercised
    /// end-to-end by T19 first-light; here we pin the structural ordering — nothing
    /// builds a core surface before that resize.
    ///
    /// A default `GhosttyApplication` is registered (for `init`'s config fallback);
    /// display-gated only for that. Before T15 the field simply didn't exist; this
    /// pins that the lifecycle never constructs it eagerly.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn surface_core_is_none_before_first_resize() {
        let surface = display_surface();

        // No core surface exists before the first resize (**invariant 3**). The
        // apprt seam agrees: `core()` on the widget is `None`.
        assert!(
            surface.core().is_none(),
            "core None before the first resize (not eagerly constructed)"
        );
    }

    /// **Invariant 3 — the resize DRIVER.** Emits a real GTK `resize` on the child
    /// `GLArea`, exercising the `swapped` template-callback trampoline end to end
    /// (`surface.blp` → [`Surface::gl_resize`]). This is the regression guard for
    /// the argument-order defect: gtk4-rs delivers the swapped vector
    /// `[self, width, height, GtkGLArea]` (self first, emitting widget LAST), so a
    /// leading `_area: &GLArea` param reads `values[1]` (an `i32`) and the nounwind
    /// FFI trampoline ABORTS the process (SIGABRT) on the first resize — before
    /// this test any real GTK resize killed first-light. Here we assert (a) the
    /// signal reaches the handler at all (no abort) and (b) the emitted dimensions
    /// are cached, proving width/height unpacked in the right slots.
    ///
    /// On first resize with no core surface, `gl_resize` falls through to
    /// `init_surface` (invariant 3 — lazy on first RESIZE, not realize). This
    /// structural harness does not `bind_core_app` (`build_default_for_test`), so
    /// `init_surface` bails at the unbound-app guard and `core()` stays `None`; the
    /// LIVE construction (bound core app + pty + GL renderer) is T19 first-light,
    /// blocked on the renderer-seam un-gating. The point pinned here is that a real
    /// resize reaches the lazy-init leg without aborting.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn surface_init_on_first_resize() {
        let surface = display_surface();
        let imp = surface.imp();

        assert!(surface.core().is_none(), "no core surface before resize");

        // Drive the real signal through the swapped trampoline. With the buggy
        // `(&self, _area, width, height)` order this line SIGABRTs.
        let (w, h) = (640_i32, 480_i32);
        imp.gl_area.emit_by_name::<()>("resize", &[&w, &h]);

        // The handler cached exactly what we emitted — width/height came from
        // `values[1]`/`values[2]`, not the trailing GLArea.
        assert_eq!(
            imp.size.get(),
            SurfaceSize {
                width: 640,
                height: 480
            },
            "gl_resize cached the emitted width/height (correct swapped-arg order)"
        );

        // Lazy init was reached but bails (no core app bound in this harness), so
        // the core surface stays None. Live construction is T19.
        assert!(
            surface.core().is_none(),
            "init bails at the unbound-app guard in the structural harness"
        );
    }

    /// **T16 render-contract leg — the render DRIVER.** Emits a real GTK `render`
    /// on the child `GLArea`, exercising the `swapped` trampoline into
    /// [`Surface::gl_render`]. The `render` signal carries a `GdkGLContext`, so
    /// under `swapped` the delivered vector is `[self, GdkGLContext, GtkGLArea]`. A
    /// buggy `(&self, _area: &GLArea)` handler reads `values[1]` (a `GdkGLContext`)
    /// as a `GLArea` and the nounwind trampoline ABORTS. With the correct
    /// `(&self)` shape the trailing context / area are ignored positionally.
    ///
    /// No core surface is bound here, so `gl_render` returns `true` on its
    /// no-surface guard (`:3333-3336`) without touching GL — the assertion is
    /// simply that the emission returns instead of aborting the process.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn surface_render_signal_does_not_abort() {
        let surface = display_surface();
        let imp = surface.imp();

        // A null `GdkGLContext` value (typed as GLContext so the emit type-checks);
        // `gl_render` never dereferences it on the no-surface path.
        let ctx = None::<gdk4::GLContext>;
        let handled = imp.gl_area.emit_by_name::<bool>("render", &[&ctx]);

        // No core surface ⇒ `gl_render` reports the frame handled (returns 1/true)
        // and, crucially, the swapped `[self, ctx, area]` vector did not abort.
        assert!(
            handled,
            "gl_render handled the no-surface frame without aborting"
        );
        assert!(
            surface.core().is_none(),
            "render did not construct a core surface"
        );
    }

    /// **T17 apprt-shim forwarders** — the apprt `Surface` (`gtk::surface::Surface`)
    /// backref forwards the read accessors to the owning widget: `get_size` returns
    /// the widget's CACHED size (`Surface.zig:52-54` → `surface.zig:1578-1586`, the
    /// value `gl_resize` stamps), and `get_content_scale` / `get_cursor_pos`
    /// resolve too. Reads them back through the seam pointer (`self.rt()`), proving
    /// the whole `apprt::Surface -> class::Surface` forward and the `constructed`
    /// backref wiring (`surface.zig:1793`). Before T15/T17 the shim
    /// `unimplemented!()`d every one of these — RED.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn surface_shim_read_accessors_forward() {
        use rustty_apprt_api::Surface as _;

        let surface = display_surface();

        // Stamp a known size into the cache (what `gl_resize` does), then read it
        // back through the apprt-surface seam pointer.
        let sentinel = SurfaceSize {
            width: 321,
            height: 123,
        };
        surface.imp().size.set(sentinel);
        let cursor = CursorPos { x: 4.0, y: 7.0 };
        surface.imp().cursor_pos.set(cursor);

        // SAFETY: `rt()` points at the widget's pinned embedded apprt surface,
        // whose backref was wired in `constructed`; the widget is alive here.
        let rt: &ApprtSurface = unsafe { &*surface.rt() };

        let size = rt
            .get_size()
            .expect("get_size forwards through the backref");
        assert_eq!(
            size, sentinel,
            "apprt Surface::get_size returns cached size"
        );

        let pos = rt.get_cursor_pos().expect("get_cursor_pos forwards");
        assert_eq!(
            pos, cursor,
            "apprt Surface::get_cursor_pos returns cached pos"
        );

        // The content-scale forwarder resolves (value is display-dependent).
        assert!(rt.get_content_scale().is_ok(), "get_content_scale forwards");

        // The clipboard-support forwarder is `true` for all three variants
        // (`Surface.zig:60-71`).
        assert!(rt.supports_clipboard(Clipboard::Standard));
        assert!(rt.supports_clipboard(Clipboard::Selection));
        assert!(rt.supports_clipboard(Clipboard::Primary));
    }

    /// **T16 — the render contract.** The app-thread chain the renderer thread
    /// triggers (invariant 1, `MUST_DRAW_FROM_APP_THREAD`):
    ///
    ///   `performAction(.render)` → [`action_render`](crate::gtk::class::application::action_render)
    ///   → [`ApprtSurface::redraw`] → `gl_area.queue_render()` → GTK emits `render`
    ///   on the app thread → [`Surface::gl_render`] → `drawFrame(true)`.
    ///
    /// Hops 1-3 (renderer thread → app mailbox → `core_app.tick` → `drainMailbox`
    /// → `redrawSurface` → `rt_app.performAction(.{ .surface }, .render, {})`) live
    /// in rustty-core (`app.rs:439,469-481`) and are exercised there. This test
    /// drives **hop 4** (the T16 code): a `.render` action on a surface target whose
    /// opaque `rt_surface` handle points at THIS widget's pinned apprt surface — the
    /// exact shape `redrawSurface` hands `performAction` — and runs every line of
    /// `action_render` → `redraw()` → `queue_render()` on a live widget. It then
    /// drives **hop 5** by emitting the `render` signal GTK delivers on the app
    /// thread after `queue_render`, asserting `gl_render` runs and reports "handled".
    ///
    /// The LIVE causal delivery (frame clock → real GL context → `drawFrame(true)`
    /// pixel) needs a mapped window, a working GL driver, and a lazily-built core
    /// surface (bound core app + pty + renderer) — that is T19 first-light. Here
    /// `core_surface` is `None`, so `gl_render` returns `true` on its no-surface
    /// guard (`:3333-3336`) before touching GL, and no window is mapped (mapping
    /// fires the not-yet-ported focus/input template callbacks — T17). Before T16
    /// wired the `.render` arm, `action_render` did not exist — RED (would not
    /// compile); with `action_render` a no-op the surface never re-renders.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn render_contract_chain() {
        use rustty_apprt_api::{CoreSurfaceHandle, Surface as _, Target};

        let surface = display_surface();
        let app_thread = std::thread::current().id();

        // A surface-target handle whose `rt_surface` points at this widget's pinned
        // apprt surface — what `rustty_core::app::redrawSurface` builds before it
        // calls `rt_app.performAction(.{ .surface = core }, .render, {})`.
        struct FakeCore {
            rt: *mut std::ffi::c_void,
        }
        impl CoreSurfaceHandle for FakeCore {
            fn rt_surface_ptr(&self) -> *mut std::ffi::c_void {
                self.rt
            }
        }
        let handle = FakeCore {
            rt: surface.rt().cast_mut().cast::<std::ffi::c_void>(),
        };

        // The `.app` arm is a no-op (`application.zig:2483`); it must not touch a
        // surface (or panic) with no widget in play.
        crate::gtk::class::application::action_render(Target::<FakeCore>::App);

        // Hop 4 (the T16 code): the `.surface` arm reaches through `rt_surface_ptr`
        // to the apprt surface and calls `redraw()` → `gl_area.queue_render()`.
        // Runs every line of `action_render`/`redraw` on a live widget (no panic;
        // `queue_render` on an unrealized GLArea is a safe no-op here).
        crate::gtk::class::application::action_render(Target::Surface(&handle));

        // Confirm the handle's `rt_surface` pointer — the one `action_render`
        // dereferenced — resolves to THIS widget: the apprt-surface backref round-
        // trips a sentinel stamped on the widget's cached size.
        let sentinel = SurfaceSize {
            width: 333,
            height: 111,
        };
        surface.imp().size.set(sentinel);
        // SAFETY: `handle.rt` is `surface.rt()`, the widget's pinned embedded apprt
        // surface, alive for this scope.
        let rt: &ApprtSurface = unsafe { &*handle.rt.cast::<ApprtSurface>() };
        assert_eq!(
            rt.get_size().expect("backref resolves"),
            sentinel,
            "action_render's rt_surface handle resolves to this widget"
        );

        // Hop 5: GTK delivers `render` on the app thread after `queue_render` →
        // `gl_render` → `drawFrame(true)`. We drive that emission synchronously (the
        // live frame-clock delivery is T19). No core surface ⇒ the no-surface guard
        // returns `true` before touching GL.
        let ctx = None::<gdk4::GLContext>;
        let handled = surface
            .imp()
            .gl_area
            .emit_by_name::<bool>("render", &[&ctx]);
        assert!(handled, "gl_render reported the frame handled");
        // The whole render leg ran synchronously on the app/main thread — the only
        // thread allowed to draw a GLArea (invariant 1).
        assert_eq!(
            std::thread::current().id(),
            app_thread,
            "the render leg stayed on the app/main thread"
        );
    }

    /// **Invariant 5** — `finalize` removes the surface from the app (`deleteSurface`)
    /// BEFORE `deinit`ing the core surface (`surface.zig:1925-1940`). Exercises the
    /// extracted ordering directly (a live `CoreSurface` cannot be built in this
    /// tier); both closures record into a shared log and we assert the order. No
    /// display needed. Before the helper existed the test did not compile — RED.
    #[test]
    fn surface_finalize_deletes_from_app_before_deinit() {
        use std::cell::RefCell;
        let order: RefCell<Vec<&'static str>> = RefCell::new(Vec::new());

        finalize_core_surface(
            true,
            || order.borrow_mut().push("delete"),
            || order.borrow_mut().push("deinit"),
        );
        assert_eq!(*order.borrow(), ["delete", "deinit"]);

        // Absent core surface ⇒ neither runs (the `if (priv.core_surface)` guard).
        order.borrow_mut().clear();
        finalize_core_surface(
            false,
            || order.borrow_mut().push("delete"),
            || order.borrow_mut().push("deinit"),
        );
        assert!(
            order.borrow().is_empty(),
            "no teardown when core surface absent"
        );
    }

    /// **Cross-thread SOUNDNESS regression — the render-thread panic (invariant 5).**
    ///
    /// Reproduces, DETERMINISTICALLY and without a live GL renderer, the exact
    /// teardown hazard the [`AtomicPtr<CoreSurface>`] storage + join-before-clear
    /// finalize ordering (this file's imp `Drop`) prevents. A background "renderer
    /// thread" hammers a published `core_surface` pointer — the widget [`Surface::core`]
    /// cross-thread read is a single atomic load, feeding the apprt seam's non-null
    /// assert (`crate::gtk::surface::Surface::core`, `Surface.zig:24-28`). The main
    /// thread runs the finalize sequence: the `deinit_and_destroy` closure must STOP +
    /// JOIN the reader (as `CoreSurface::deinit` joins the render/io threads) BEFORE
    /// clearing the pointer to null.
    ///
    /// The ORIGINAL bug cleared first: `self.core_surface.borrow_mut().take()` (→
    /// `None`) and only THEN `boxed.deinit()` (the join) — so the still-running
    /// renderer thread saw a cleared pointer and hit
    /// `expect("...uninitialized surface")` (the `thread 'renderer' panicked` line).
    /// Here the reader records whether it EVER observed null; with the corrected
    /// order it never does. Reversing the two lines of the closure below makes this
    /// test fail — it is a live regression guard, not a tautology.
    #[test]
    fn finalize_joins_threads_before_clearing_core_surface() {
        use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
        use std::sync::Arc;
        use std::time::Duration;

        // A non-null sentinel standing in for the published, heap-stable core surface
        // pointer. NEVER dereferenced — the reader only null-checks it, exactly as the
        // widget `core()` atomic load feeds the apprt seam's non-null assert.
        let sentinel: *mut CoreSurface = std::ptr::NonNull::<CoreSurface>::dangling().as_ptr();
        let core_surface = Arc::new(AtomicPtr::new(sentinel));
        let stop = Arc::new(AtomicBool::new(false));
        let saw_null = Arc::new(AtomicBool::new(false));

        let reader = {
            let core_surface = Arc::clone(&core_surface);
            let stop = Arc::clone(&stop);
            let saw_null = Arc::clone(&saw_null);
            std::thread::spawn(move || {
                // The renderer thread's loop: read `core()` continuously until it is
                // told to stop and joined (mirroring `renderer_thr.join()`).
                while !stop.load(Ordering::Acquire) {
                    if core_surface.load(Ordering::Acquire).is_null() {
                        saw_null.store(true, Ordering::Release);
                    }
                }
            })
        };

        // Let the reader spin on the published pointer before teardown starts.
        std::thread::sleep(Duration::from_millis(20));

        // The finalize ordering under test (mirrors the imp `Drop` deinit closure):
        // JOIN the reader FIRST, THEN clear the pointer.
        finalize_core_surface(
            !core_surface.load(Ordering::Acquire).is_null(),
            || { /* `deleteSurface`: reads core() (still non-null); no clear */ },
            || {
                // `CoreSurface::deinit`: stop + JOIN the render thread FIRST …
                stop.store(true, Ordering::Release);
                reader.join().unwrap();
                // … and ONLY NOW clear the pointer (`alloc.destroy` +
                // `priv.core_surface = null`, `surface.zig:1937-1939`).
                core_surface.store(std::ptr::null_mut(), Ordering::Release);
            },
        );

        assert!(
            !saw_null.load(Ordering::Acquire),
            "renderer thread observed a cleared core_surface before being joined — \
             the teardown cleared the pointer before joining (invariant 5 violated)"
        );
    }
}
