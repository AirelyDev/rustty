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
    ApprtError, Clipboard, ClipboardContent, ClipboardRequest, ContentScale, CursorPos,
    NewSurfaceContext, SurfaceSize,
};
use rustty_config::command::Command;
use rustty_config::value_types::{ClipboardAccess, WorkingDirectory};
use rustty_core::surface::{Error as CoreSurfaceError, InputEffect};
use rustty_font_shaper::face::DesiredSize;
use rustty_vt::input::key::{Action, Key, KeyEvent};
use rustty_vt::input::key_mods::Mods;
use rustty_vt::input::mouse::{Button, ButtonState, ScrollMods};
use rustty_vt::mouse::Shape as MouseShape;

use crate::gtk::app::App;
use crate::gtk::ext;
use crate::gtk::gsettings;
use crate::gtk::key;
use crate::gtk::surface::Surface as ApprtSurface;

use super::application::Application;
use super::clipboard_confirmation_dialog::{ClipboardConfirmationDialog, ClipboardRequestBoxed};
use super::config::Config;
use super::inspector_window::InspectorWindow;
use super::key_state_overlay::KeyStateOverlay;
use super::resize_overlay::ResizeOverlay;
use super::search_overlay::SearchOverlay;
use super::surface_child_exited::SurfaceChildExited;

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

/// PORTS: `surface.zig:657` `in_keyevent: IMKeyEvent`. Whether we are currently
/// inside a key-event dispatch (gates the IME state transitions, **invariant 9**).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImKeyEvent {
    /// Not in a key event (`IMKeyEvent.false`).
    #[default]
    False,
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
/// `privateBoxedFieldAccessor`).
#[derive(Debug, Clone, Copy, glib::Boxed)]
#[boxed_type(name = "GhosttyFontSizeRequest", nullable)]
pub struct FontSizeRequest(pub DesiredSize);

/// PORTS: `surface.zig:747-752` the `new` overrides — the caller-supplied
/// command / working-directory / title that seed a freshly created surface.
/// (No `Debug`: `rustty_config::command::Command` is not `Debug`.)
#[derive(Default)]
pub struct SurfaceOverrides {
    /// PORTS: `surface.zig:748` `command: ?configpkg.Command`.
    pub command: Option<Command>,
    /// PORTS: `surface.zig:749` `working_directory: ?[:0]const u8`.
    pub working_directory: Option<String>,
    /// PORTS: `surface.zig:750` `title: ?[:0]const u8` (the `title-override`
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
        /// PORTS: `surface.zig:69-79,675` `bell-ringing` (privateShallow).
        #[property(get, set)]
        pub bell_ringing: Cell<bool>,

        /// PORTS: `surface.zig:82-91,572` `config` (`?*Config`, SHARED; obj
        /// accessor: get `.none` refs, set `.full` unrefs prior).
        #[property(get, set, nullable)]
        pub config: RefCell<Option<Config>>,

        /// PORTS: `surface.zig:94-109,667` `child-exited`.
        #[property(get, set)]
        pub child_exited: Cell<bool>,

        /// PORTS: `surface.zig:112-121,575` `default-size` (`?*Size`, OWNED boxed).
        #[property(get, set, nullable)]
        pub default_size: RefCell<Option<Size>>,

        /// PORTS: `surface.zig:124-139,687` `error`.
        #[property(name = "error", get, set)]
        pub error: Cell<bool>,

        /// PORTS: `surface.zig:142-152,583` `font-size-request` (OWNED boxed).
        #[property(get, set, nullable)]
        pub font_size_request: RefCell<Option<FontSizeRequest>>,

        /// PORTS: `surface.zig:154-170,611` `focused` (struct default `true`).
        #[property(get, set, default = true)]
        pub focused: Cell<bool>,

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

        /// PORTS: `surface.zig:190-200,579` `min-size` (`?*Size`, OWNED boxed).
        #[property(get, set, nullable)]
        pub min_size: RefCell<Option<Size>>,

        /// PORTS: `surface.zig:222-240,586` `mouse-shape`.
        ///
        /// PORT NOTE (concern): the reference registers `terminal.MouseShape` as a
        /// GObject enum (`GhosttyMouseShape`); rustty's `mouse::Shape` deliberately
        /// STRIPPED that registration to the apprt layer (rustty-vt R3 note). Until
        /// the mouse-input task (T16/T17) needs the enum GType, this property
        /// carries the shape's `#[repr(i32)]` discriminant, which round-trips to
        /// [`MouseShape`] losslessly; the property NAME and notify semantics
        /// (`notify::mouse-shape`) are preserved.
        #[property(name = "mouse-shape", get, set)]
        pub mouse_shape: Cell<i32>,

        /// PORTS: `surface.zig:202-220,589` `mouse-hidden`.
        #[property(get, set)]
        pub mouse_hidden: Cell<bool>,

        /// PORTS: `surface.zig:242-253,592` `mouse-hover-url` (string; getter
        /// `.none`, setter `.full`).
        #[property(get, set, nullable)]
        pub mouse_hover_url: RefCell<Option<String>>,

        /// PORTS: `surface.zig:255-266,601` `pwd` (string).
        #[property(get, set, nullable)]
        pub pwd: RefCell<Option<String>>,

        /// PORTS: `surface.zig:268-279,604` `title` (string).
        #[property(get, set, nullable)]
        pub title: RefCell<Option<String>>,

        /// PORTS: `surface.zig:281-292,607` `title-override` (string).
        #[property(name = "title-override", get, set, nullable)]
        pub title_override: RefCell<Option<String>>,

        /// PORTS: `surface.zig:294-310,619` `zoom`.
        #[property(get, set)]
        pub zoom: Cell<bool>,

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
        /// PORTS: `surface.zig:623` `gl_area` — the GL context source.
        #[template_child]
        pub gl_area: TemplateChild<gtk4::GLArea>,
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

        /// PORTS: `surface.zig:653` `size: apprt.SurfaceSize` — cached metrics.
        pub size: Cell<SurfaceSize>,
        /// PORTS: `surface.zig:654` `cursor_pos: apprt.CursorPos`.
        pub cursor_pos: Cell<CursorPos>,

        /// PORTS: `surface.zig:657-661` the IME state group (**invariant 9**).
        pub ime: RefCell<ImeState>,

        /// PORTS: `surface.zig:664` `precision_scroll`.
        pub precision_scroll: Cell<bool>,

        /// PORTS: `surface.zig:670` `progress_bar_timer: ?c_uint` — glib source id
        /// (OWNED handle; `Source.remove` in dispose, **invariant 11**, T14).
        pub progress_bar_timer: RefCell<Option<glib::SourceId>>,

        /// PORTS: `surface.zig:681` `bell_media: ?*gtk.MediaFile` (OWNED; unref T14).
        pub bell_media: RefCell<Option<gtk4::MediaFile>>,

        /// PORTS: `surface.zig:690` `idle_rechild: ?c_uint` (OWNED source; T14).
        pub idle_rechild: RefCell<Option<glib::SourceId>>,

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

        /// PORTS: `surface.zig:709` `key_sequence: ArrayListUnmanaged([:0]const u8)`
        /// (OWNED; free each + deinit, T15). Backs the `key-sequence` property.
        pub key_sequence: RefCell<Vec<CString>>,
        /// PORTS: `surface.zig:710` `key_tables` (OWNED; T15). Backs `key-table`.
        pub key_tables: RefCell<Vec<CString>>,

        /// PORTS: `surface.zig:721` `context: apprt.surface.NewSurfaceContext`.
        pub context: Cell<NewSurfaceContext>,

        /// PORTS: `surface.zig:724` `gtk_enable_primary_paste` (struct default true).
        pub gtk_enable_primary_paste: Cell<bool>,

        /// PORTS: `surface.zig:728` `suppress_left_mouse_release`.
        pub suppress_left_mouse_release: Cell<bool>,

        /// PORTS: `surface.zig:731` `pending_horizontal_scroll`.
        pub pending_horizontal_scroll: Cell<f64>,

        /// PORTS: `surface.zig:735` `pending_horizontal_scroll_reset: ?c_uint`
        /// (OWNED source; T14).
        pub pending_horizontal_scroll_reset: RefCell<Option<glib::SourceId>>,

        /// PORTS: `surface.zig:738` `overrides.command: ?configpkg.Command`
        /// (OWNED; `deinit` T15).
        pub overrides_command: RefCell<Option<Command>>,
        /// PORTS: `surface.zig:739` `overrides.working_directory: ?[:0]const u8`
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
                mouse_shape: Cell::new(MouseShape::Default as i32),
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
                bell_media: RefCell::new(None),
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
                    // `bell` (`:448-457`).
                    glib::subclass::Signal::builder("bell").build(),
                    // `close-request` (`:468-477`).
                    glib::subclass::Signal::builder("close-request").build(),
                    // `clipboard-read` (`:495-504`).
                    glib::subclass::Signal::builder("clipboard-read").build(),
                    // `clipboard-write` (`:480-492`), params `(apprt.Clipboard,
                    // [*:0]const u8)`. PORT NOTE: `apprt.Clipboard` has no GObject type
                    // yet, so its param rides as the enum's `i32` here. Emitted by
                    // `Clipboard::set` after a non-confirm write (surface.rs `set_clipboard`).
                    glib::subclass::Signal::builder("clipboard-write")
                        .param_types([i32::static_type(), String::static_type()])
                        .build(),
                    // `init` (`:507-516`).
                    glib::subclass::Signal::builder("init").build(),
                    // `menu` (`:519-528`).
                    glib::subclass::Signal::builder("menu").build(),
                    // `present-request` (`:532-541`).
                    glib::subclass::Signal::builder("present-request").build(),
                    // `toggle-fullscreen` (`:545-554`).
                    glib::subclass::Signal::builder("toggle-fullscreen").build(),
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

            // Private-field defaults (`surface.zig:1793-1801`). Most are already the
            // struct defaults ([`Default`]); we re-assert the ones `init` sets.
            self.ime.replace(ImeState::default());
            self.cursor_pos.set(CursorPos { x: 0.0, y: 0.0 });
            // `priv.mouse_shape = .text` (`surface.zig:1796`).
            self.mouse_shape.set(MouseShape::Text as i32);
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

            // IME state (`surface.zig:1811-1813`) — already reset above.

            // Read the GTK primary-paste setting (`surface.zig:1816`).
            self.gtk_enable_primary_paste
                .set(gsettings::get_bool(gsettings::Key::GtkEnablePrimaryPaste).unwrap_or(true));

            // Drop-target gtypes IN ORDER (`surface.zig:1818-1826`) — the order is
            // load-bearing: FileList, then File, then string.
            self.drop_target.set_types(&[
                gdk4::FileList::static_type(),
                gio::File::static_type(),
                String::static_type(),
            ]);

            // `setCursorFromName("text")` (`surface.zig:1829`).
            obj.set_cursor_from_name(Some("text"));

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

            // `bell_media` unref+null (`surface.zig:1863-1866`).
            drop(self.bell_media.take());

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

    impl WidgetImpl for Surface {}
    impl BinImpl for Surface {}
    impl ScrollableImpl for Surface {}

    impl Surface {
        /// PORTS: `surface.zig:2141-2145` `getKeySequence` (transfer `.full`).
        /// Builds a fresh [`ext::StringList`] from the owned key-sequence.
        fn key_sequence_prop(&self) -> Option<ext::StringList> {
            Some(string_list_from(&self.key_sequence.borrow()))
        }

        /// PORTS: `surface.zig:2148-2152` `getKeyTable` (transfer `.full`).
        fn key_table_prop(&self) -> Option<ext::StringList> {
            Some(string_list_from(&self.key_tables.borrow()))
        }

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
            eprintln!("warn(gtk): unable to remove {what}");
        }
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
        // `priv.overrides = .{ .command = …clone, .working_directory = …dupeZ }`
        // (`surface.zig:759-762`).
        imp.overrides_command.replace(overrides.command);
        imp.overrides_working_directory.replace(
            overrides
                .working_directory
                .and_then(|wd| CString::new(wd).ok()),
        );

        obj
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

    /// PORTS: `surface.zig:781-816` `setParent`. Extracts init values from a
    /// parent core surface **only before this surface is realized**
    /// (**invariant 3**): the font size is always inherited, and the working
    /// directory is inherited when the config says so.
    ///
    /// # Safety
    /// `parent` must point to a live [`CoreSurface`].
    pub unsafe fn set_parent(&self, parent: *mut CoreSurface, context: NewSurfaceContext) {
        let imp = self.imp();

        // "We can only set a parent before surface realization." (`:788-793`).
        if !imp.core_surface.load(Ordering::Acquire).is_null() {
            eprintln!("setParent called after surface is already realized");
            return;
        }

        // Store the context so the lazy `initSurface` (T15) can use it (`:795-796`).
        imp.context.set(context);

        // SAFETY: caller guarantees `parent` is live.
        let parent_ref: &CoreSurface = unsafe { &*parent };

        // Inherit the font size (`surface.zig:798-803`).
        imp.font_size_request
            .replace(Some(FontSizeRequest(parent_ref.font_size)));
        self.notify("font-size-request");

        // The remainder needs a config (`:806-815`).
        if let Some(config_obj) = imp.config.borrow().as_ref() {
            if rustty_apprt_api::should_inherit_working_directory(context, config_obj.get()) {
                let alloc = allocator_api2::alloc::Global;
                if let Some(pwd) = parent_ref.pwd(alloc) {
                    imp.pwd.replace(String::from_utf8(pwd).ok());
                    self.notify("pwd");
                }
            }
        }
    }

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
        // Map the shape (carried as its `i32` discriminant) to a CSS cursor name
        // and set it (`:2389-2429`).
        let name = mouse_shape_cursor_name(self.imp().mouse_shape.get());
        self.upcast_ref::<gtk4::Widget>()
            .set_cursor_from_name(Some(name));
    }

    /// PORTS: `surface.zig:1500-1507` `close`. Emits `close-request`; the window
    /// tier decides what to do. The apprt seam's `close(process_alive)` discards
    /// `process_alive` and calls this (`Surface.zig:35-38`).
    pub fn close(&self) {
        self.emit_by_name::<()>("close-request", &[]);
    }

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
                // Free all stored strings and clear (`:944-948`).
                imp.key_sequence.borrow_mut().clear();
            }
        }
    }

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

    /// PORTS: `surface.zig:1030-1113` `setProgressReport`. Drive the
    /// `progress_bar_overlay` (a `gtk.ProgressBar` in `surface.blp`) visible +
    /// fraction/pulse from an OSC 9;4 progress report, with a 15s watchdog timer
    /// that removes bars stalled by bad-actor programs.
    pub fn set_progress_report(&self, value: rustty_vt::osc::ProgressReport) {
        use rustty_vt::osc::ProgressReportState;
        let imp = self.imp();

        // Stop the timer no matter what — if removing we're done, otherwise we
        // restart it below (`:1036-1043`).
        remove_source_id(imp.progress_bar_timer.take(), "progress bar timer");

        // Config gate: `progress-style` off => hide + return (`:1045-1051`).
        if let Some(config) = imp.config.borrow().as_ref() {
            if !config.get().progress_style {
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
            // Fixed value if provided, otherwise pulse; clear the `error` class
            // (`:1063-1070`).
            ProgressReportState::Set => {
                progress_bar.remove_css_class("error");
                match value.progress {
                    Some(progress) => progress_bar.set_fraction(compute_fraction(progress)),
                    None => progress_bar.pulse(),
                }
            }
            // As `.set`, but with the `error` class (`:1074-1081`).
            ProgressReportState::Error => {
                progress_bar.add_css_class("error");
                match value.progress {
                    Some(progress) => progress_bar.set_fraction(compute_fraction(progress)),
                    None => progress_bar.pulse(),
                }
            }
            // Unknown progress — pulse (`:1085-1087`).
            ProgressReportState::Indeterminate => progress_bar.pulse(),
            // Keep the bar on screen; set a fixed value if provided, but don't
            // pulse (`:1093-1097`).
            ProgressReportState::Pause => {
                if let Some(progress) = value.progress {
                    progress_bar.set_fraction(compute_fraction(progress));
                }
            }
        }

        // All non-remove states lead to visibility (`:1100-1102`).
        progress_bar.set_visible(true);

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
        // Keep the property up to date for anyone listening (`:1513-1519`).
        self.set_child_exited(true);

        // Populate the banner (`:1528`).
        self.imp()
            .child_exited_overlay
            .set_child_exited_data(Some(value));
        true
    }

    /// PORTS: `surface.zig:2472-2509` `ringBell`. Ring the terminal bell: emit the
    /// `bell` signal, fan out to the `tab.ring-bell`/`win.ring-bell` widget actions
    /// (system beep, title bell, urgency — `window.rs`/`tab.rs`), play the custom
    /// audio bell, and light the bell border via the `bell-ringing` property.
    ///
    /// PORT NOTE: the reference's `ringBell` is invoked by `setBellRinging(true)`
    /// (`surface.zig:2216`); rustty keeps `set_bell_ringing` as the plain property
    /// write and hosts the full bell here, so the `.ring_bell` apprt action
    /// (`application.rs`) and `commandFinished` both funnel through this one method.
    pub fn ring_bell(&self) {
        // `signals.bell.impl.emit(...)` (`:2472-2478`) — fire any bell subscriber
        // BEFORE the action fan-out.
        self.emit_by_name::<()>("bell", &[]);

        // Action fan-out (`:2480-2481`): the `tab.ring-bell` / `win.ring-bell`
        // widget actions do the system beep, title bell, and attention/urgency
        // (`window.rs:1968`, `tab.rs:410`). They bubble up from the surface widget.
        let _ = self.activate_action("tab.ring-bell", None);
        let _ = self.activate_action("win.ring-bell", None);

        // The custom audio bell (`:2484-2509`).
        self.ring_bell_audio();

        // The `bell-ringing` property write (`:2221`); the border's bind-time
        // `should_border_be_shown` closure re-evaluates, lighting the bell border.
        self.set_bell_ringing(true);
    }

    /// PORTS: `surface.zig:2484-2509` — the audio-bell block of `ringBell`. Plays
    /// the configured custom bell sound (when `bell-features.audio` is set and a
    /// `bell-audio-path` is configured), reusing one cached `MediaFile` per
    /// surface to avoid the GStreamer pipeline leak (see `media.rs`). The
    /// border/system-beep/urgency of `ringBell` are the sibling steps in
    /// [`Surface::ring_bell`].
    pub fn ring_bell_audio(&self) {
        use std::os::unix::ffi::OsStrExt;

        let imp = self.imp();
        let cfg = imp.config.borrow();
        let Some(config) = cfg.as_ref() else {
            return;
        };
        let config = config.get();

        // `if (config.@"bell-features".audio)` (`:2487`).
        if !config.bell_features.audio {
            return;
        }

        // `const config_path = config.@"bell-audio-path" orelse break :audio`
        // (`:2488-2492`): the `Path` union carries the optional/required flag.
        let Some(config_path) = config.bell_audio_path.as_ref() else {
            return;
        };
        let (path_bytes, required) = match config_path {
            rustty_config::path::Path::Optional(p) => (p.as_slice(), false),
            rustty_config::path::Path::Required(p) => (p.as_slice(), true),
        };

        // `std.math.clamp(config.@"bell-audio-volume", 0.0, 1.0)` (`:2494-2498`).
        let volume = config.bell_audio_volume.clamp(0.0, 1.0);

        let path = std::path::Path::new(std::ffi::OsStr::from_bytes(path_bytes));

        // Reuse one MediaFile per surface, rebuilt only when the path changes
        // (`:2506-2508`). `bell_media_file` frees any stale MediaFile and returns
        // the current slot value (possibly None if the path is inaccessible), so
        // `bell_media` never dangles.
        let current = imp.bell_media.borrow_mut().take();
        let media_file = crate::gtk::media::bell_media_file(current, path, required);
        if let Some(media_file) = &media_file {
            crate::gtk::media::play_bell(media_file, volume);
        }
        imp.bell_media.replace(media_file);
    }

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

        // Content scale -> DPI (`:2104-2113`). `default_dpi` = 72 on macOS else 96
        // (`rustty-font-shaper/face/mod.rs` `default_dpi`).
        let content_scale = self.get_content_scale();
        let default_dpi: f32 = if cfg!(target_os = "macos") { 72.0 } else { 96.0 };
        let font_size = DesiredSize {
            points: config.font_size,
            xdpi: (content_scale.x * default_dpi) as u16,
            ydpi: (content_scale.y * default_dpi) as u16,
        };

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

        // The GTK widget scale factor (`:1536-1546`).
        let scale = imp.gl_area.scale_factor();
        let gtk_scale: f32 = if scale <= 0 {
            eprintln!(
                "warn(gtk): gtk_widget_get_scale_factor returned a non-positive number: {scale}"
            );
            1.0
        } else {
            scale as f32
        };

        // The gtk-xft-dpi font-DPI scale (`:1548-1572`). gtk-xft-dpi is the font
        // DPI × 1024; divide by 1024 then by the 96 default to derive a scale.
        let xft_dpi_scale: f32 = match gsettings::get_c_int(gsettings::Key::GtkXftDpi) {
            None => {
                eprintln!("warn(gtk): gtk-xft-dpi was not set, using default value");
                1.0
            }
            Some(dpi) if dpi <= 0 => {
                eprintln!("warn(gtk): gtk-xft-dpi has invalid value ({dpi}), using default");
                1.0
            }
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
        self.imp().size.get()
    }

    /// PORTS: `surface.zig:1588-1590` `getCursorPos`.
    #[must_use]
    pub fn get_cursor_pos(&self) -> CursorPos {
        self.imp().cursor_pos.get()
    }

    /// PORTS: `surface.zig:1994-1996` `getTitle`. The stored surface title (the
    /// terminal-set title, before the user override).
    #[must_use]
    pub fn get_title(&self) -> Option<String> {
        self.imp().title.borrow().clone()
    }

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
            eprintln!("warn(gtk): can't send notification because there is no core surface");
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

        // We set the notification ID to the body content so an identical follow-up
        // replaces the prior notification (`:1774-1777`).
        app.upcast_ref::<gio::Application>()
            .send_notification(Some(&body_str), &notification);
    }

    /// PORTS: `surface.zig:988-1031` `setScrollbar`. Update the `Gtk.Scrollable`
    /// vadjustment in place from the core's scroll state.
    pub fn set_scrollbar(&self, scrollbar: rustty_vt::pagelist::grow::Scrollbar) {
        let imp = self.imp();

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
            if cfg.notify_on_command_finish == NotifyOnCommandFinish::Unfocused
                && self.focused()
            {
                return true;
            }
        }

        // Duration gate (`:1161`).
        if value.duration.lte(cfg.notify_on_command_finish_after) {
            return true;
        }

        let action = cfg.notify_on_command_finish_action;

        // `if (action.bell) self.setBellRinging(true)` (`:1165`). In rustty the full
        // bell lives in [`Surface::ring_bell`] (see its PORT NOTE).
        if action.bell {
            self.ring_bell();
        }

        // `if (action.notify) { ... sendDesktopNotification(title, body) }`
        // (`:1167-1200`).
        if action.notify {
            // Title (`:1170-1178`). PORT NOTE (i18n): the reference wraps each in
            // `i18n._(...)`; rustty has no runtime `_()` binding yet (see the i18n
            // PORT NOTE in `close_confirmation_dialog.rs`) and emits the literal.
            let title = match value.exit_code {
                None => "Command Finished",
                Some(0) => "Command Succeeded",
                Some(_) => "Command Failed",
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

    /// PORTS: `surface.zig:1714-1724` `clipboardRequest` → delegates to
    /// `Clipboard.request` (`surface.zig:3855-3897`). Request data from the
    /// clipboard (a READ); this completes asynchronously and delivers the result
    /// to the core via `completeClipboardRequest`.
    ///
    /// Returns `Ok(true)` if the request was started, `Ok(false)` if the clipboard
    /// doesn't contain text (letting performable keybinds pass through), matching
    /// the reference's early-`false` return.
    pub fn clipboard_request(
        &self,
        clipboard_type: Clipboard,
        state: ClipboardRequest,
    ) -> Result<bool, ApprtError> {
        // Get our requested clipboard (`surface.zig:3861-3864`).
        let clipboard = self.clipboard_for(clipboard_type);

        // For paste requests, check if the clipboard has a text format available.
        // This is a synchronous check that allows performable keybinds to pass
        // through when the clipboard contains non-text content, e.g. images
        // (`surface.zig:3869-3875`).
        if state == ClipboardRequest::Paste
            && !clipboard.formats().contains_type(String::static_type())
        {
            eprintln!("debug(gtk): clipboard has no text format, not starting paste request");
            return Ok(false);
        }

        // Important: we ref `self` (clone the GObject — a cheap refcount bump) into
        // the async closure so the surface can't be freed while a clipboard read is
        // outstanding. The reference `self.ref()`s into the `Request` userdata
        // (`surface.zig:3882-3886`); the gtk4-rs closure captures the clone instead.
        let this = self.clone();

        // Read (`surface.zig:3890-3894`). The completion mirrors `clipboardReadText`
        // (`surface.zig:4042-4105`).
        clipboard.read_text_async(
            gio::Cancellable::NONE,
            move |result| {
                let text = match result {
                    Ok(Some(text)) => text,
                    // No text on the clipboard — nothing to deliver.
                    Ok(None) => return,
                    Err(err) => {
                        eprintln!("warn(gtk): failed to read clipboard err={err}");
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
                match unsafe { &mut *core }.completeClipboardRequest(
                    state,
                    text.as_bytes(),
                    false,
                ) {
                    Ok(()) => {
                        // `Surface.signals.@"clipboard-read".impl.emit(...)`
                        // (`surface.zig:4099-4104`).
                        this.emit_by_name::<()>("clipboard-read", &[]);
                    }

                    // error.UnsafePaste, error.UnauthorizedPaste => confirmation
                    // (`surface.zig:4079-4088`). DEFERRED to Phase B.
                    Err(
                        CoreSurfaceError::UnsafePaste | CoreSurfaceError::UnauthorizedPaste,
                    ) => {
                        this.show_clipboard_confirmation(state, &text);
                    }

                    Err(err) => {
                        eprintln!("warn(gtk): failed to complete clipboard request err={err}");
                    }
                }
            },
        );

        // Return true (request started) (`surface.zig:3896`).
        Ok(true)
    }

    /// PORTS: `surface.zig:1726-1738` `setClipboard` → delegates to `Clipboard.set`
    /// (`surface.zig:3760-3847`). Set the clipboard contents (a WRITE).
    pub fn set_clipboard(
        &self,
        clipboard_type: Clipboard,
        contents: &[ClipboardContent<'_>],
        confirm: bool,
    ) {
        // Grab our plaintext content for use in confirmation dialogs and signals.
        // We always expect one to exist (`surface.zig:3768-3774`).
        let Some(text_content) = contents
            .iter()
            .find(|c| c.mime == b"text/plain")
        else {
            return;
        };
        let text = String::from_utf8_lossy(text_content.data).into_owned();

        // If no confirmation is necessary, set the clipboard (`surface.zig:3777`).
        if !confirm {
            let clipboard = self.clipboard_for(clipboard_type);

            // Build one `ContentProvider` per content, then union them all
            // (`surface.zig:3784-3826`). The reference falls back to
            // `clipboard.setText(text)` only on an allocation failure; Rust's `Vec`
            // aborts on OOM, so the fallback path is unreachable here.
            let mut providers: Vec<gdk4::ContentProvider> = Vec::with_capacity(contents.len());
            for content in contents {
                let bytes = glib::Bytes::from(content.data);
                if content.mime == b"text/plain" {
                    // Add an explicit UTF-8 encoding parameter to the text/plain
                    // type. The default charset when there is none is ASCII, and
                    // lots of things look for UTF-8 specifically. Some clients pick
                    // the FIRST match in the order we set here then garble bare
                    // 'text/plain' with non-ASCII UTF-8, so offer UTF-8 first
                    // (`surface.zig:3792-3817`).
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
            if let Err(err) = clipboard.set_content(Some(&all)) {
                eprintln!("warn(gtk): failed to set clipboard content err={err}");
            }

            // `Surface.signals.@"clipboard-write".impl.emit(...)` with
            // `(clipboard_type, text)` (`surface.zig:3832-3837`). PORT NOTE:
            // `apprt.Clipboard` has no GObject type in this tier, so the param
            // rides as the enum's `i32` (matching the signal declaration at
            // `surface.rs:568`).
            self.emit_by_name::<()>(
                "clipboard-write",
                &[&(clipboard_type as i32), &text],
            );

            return;
        }

        // confirm == true → showClipboardConfirmation (`surface.zig:3842-3846`).
        // DEFERRED to Phase B.
        self.show_clipboard_confirmation(ClipboardRequest::Osc52Write(clipboard_type), &text);
    }

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
                ClipboardRequest::Paste => {}
            }
        }

        // Get our text from the dialog's clipboard-contents buffer (`:4002-4014`).
        let Some(buffer) = dialog.get_clipboard_contents() else {
            return;
        };
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);

        // `surface.completeClipboardRequest(req.*, text, true)` (`:4016-4022`).
        // `confirmed = true`. Raw bytes, never a lossy String (R3).
        if let Err(err) =
            unsafe { &mut *core }.completeClipboardRequest(req.0, text.as_bytes(), true)
        {
            eprintln!("warn(gtk): failed to complete clipboard request err={err}");
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
                // PORT NOTE: the reference `@panic("paste should not be able to be
                // remembered")` (`:4038`); a paste request can never set the
                // remember switch to reach here. In Rust we log instead of aborting.
                ClipboardRequest::Paste => {
                    eprintln!("warn(gtk): paste request should not be able to be remembered");
                }
            }
        }
    }

    /// PORTS: `surface.zig:1592-1641` `defaultTermioEnv`. The default environment
    /// for the surface's termio subprocess: the inherited process env with the
    /// language override applied and the GTK / desktop-activation vars stripped so
    /// they don't leak to the child.
    ///
    /// PORT NOTE (deferred): the snap-path filtering (`:1621-1626`, comptime
    /// `build_config.snap` — false in this build) lands with its own tier. The
    /// winproto `addSubprocessEnv` hook and the quick-terminal env var
    /// (`:1629-1638`) ARE ported below.
    #[must_use]
    pub fn default_termio_env(&self) -> std::collections::HashMap<String, String> {
        let app = Application::default();
        let mut env = rustty_os::env::getEnvMap();

        // Language override (`:1598-1602`).
        match app.saved_language() {
            Some(language) => {
                env.insert("LANG".to_string(), language);
            }
            None => {
                env.remove("LANG");
            }
        }

        // Don't leak GTK debug / desktop-activation vars to children
        // (`:1604-1617`).
        for key in [
            "GDK_DEBUG",
            "GDK_DISABLE",
            "GSK_RENDERER",
            "GIO_LAUNCHED_DESKTOP_FILE",
            "GIO_LAUNCHED_DESKTOP_FILE_PID",
            "DBUS_STARTER_ADDRESS",
            "DBUS_STARTER_BUS_TYPE",
            "INVOCATION_ID",
            "JOURNAL_STREAM",
            "NOTIFY_SOCKET",
        ] {
            env.remove(key);
        }

        // `if (ext.getAncestor(Window, self.as(gtk.Widget))) |window| { try
        // window.winproto().addSubprocessEnv(&env); if (window.isQuickTerminal())
        // try env.put("GHOSTTY_QUICK_TERMINAL", "1"); }` (`surface.zig:1629-1638`).
        //
        // The reference's own comment calls this a hack that ties the Surface to the
        // Window class, and says the right fix is a signal whose handler mutates the
        // EnvMap; it punts because boxing the EnvMap is awkward. Ported as-is (R1).
        if let Some(window) = crate::gtk::ext::ancestor::<crate::gtk::class::window::Window>(self) {
            if let Err(_err) = window.winproto_mut().add_subprocess_env(&mut env) {
                eprintln!("warn(gtk): winproto failed to add subprocess env");
            }

            if window.is_quick_terminal() {
                // R2: product-namespaced, unlike `WINDOWID` above.
                env.insert(
                    format!("{}_QUICK_TERMINAL", rustty_identity::ENV_PREFIX),
                    "1".to_string(),
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

        // The block below is all related to input method handling. See the
        // function comment for high level details and the comments within the
        // block for specifics (`:1253-1343`).
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

            // Pass the event through the input method which returns true if
            // handled. Confusingly, not all events handled by the input method
            // result in this returning true so we have to maintain some
            // additional state about whether we were composing or not to
            // determine if we should proceed with key encoding.
            //
            // Cases where the input method does not mark the event as handled:
            //
            // - If we change the input method via keypress while we have preedit
            //   text, the input method will commit the pending text but will not
            //   mark it as handled. We use the `.composing` state to detect this
            //   case.
            //
            // - If we switch input methods (i.e. via ctrl+shift with fcitx), the
            //   input method will handle the key release event but will not mark
            //   it as handled. I don't know any way to detect this case so it
            //   will result in a key event being sent to the key callback. For
            //   Kitty text encoding, this will result in modifiers being
            //   triggered despite being technically consumed. At the time of
            //   writing, both Kitty and Alacritty have the same behavior. I know
            //   of no way to fix this (`:1282-1302`).
            let im_handled = imp.im_context.filter_keypress(&event);

            // Read the (possibly IME-mutated) state, then reset `in_keyevent` to
            // `.false` — the block-scoped `defer` at `:1280`, which fires AFTER
            // the early-return arms below read `in_keyevent`.
            let (im_composing, in_keyevent, im_len) = {
                let ime = imp.ime.borrow();
                (ime.im_composing, ime.in_keyevent, ime.im_len)
            };
            imp.ime.borrow_mut().in_keyevent = ImKeyEvent::False;

            // If the input method handled the event, you would think we would
            // never proceed with key encoding for Ghostty but that is not the
            // case. Input methods will handle basic character encoding like
            // typing "a" and we want to associate that with the key event. So we
            // have to check additional state to determine if we exit
            // (`:1310-1336`).
            if im_handled {
                // If we are composing then we're in a preedit state and do not
                // want to encode any keys. For example: type a deadkey such as
                // single quote on a US international keyboard layout (`:1316-1319`).
                if im_composing {
                    return true;
                }

                // If we were composing and now we're not, it means that we
                // committed the text. We also don't want to encode a key event
                // for this. Example: enable Japanese input method, press "konn"
                // and then press enter. The final enter should not be encoded and
                // "konn" (in hiragana) should be written as "こん" (`:1321-1326`).
                if in_keyevent == ImKeyEvent::Composing {
                    return true;
                }

                // Not composing and our input method buffer is empty. This could
                // mean that the input method reacted to this event by activating
                // an onscreen keyboard or something equivalent. We don't know.
                // But the input method handled it and didn't give us text so we
                // will just assume we should not encode this. This handles a real
                // scenario when ibus starts the emoji input method (super+.)
                // (`:1328-1335`).
                if im_len == 0 {
                    return true;
                }
            }

            // At this point, for the sake of explanation of internal state: it is
            // possible that im_len > 0 and im_composing == false. This means that
            // we received a commit event from the input method that we want
            // associated with the key event. This is common: its how basic
            // character translation for simple inputs like "a" work (`:1338-1342`).
        }

        // We always reset the length of the im buffer. There's only one scenario
        // we reach this point with im_len > 0 and that's if we received a commit
        // event from the input method. We don't want to keep that state around
        // since we've handled it here. `defer priv.im_len = 0` (`:1349`) — the
        // guard resets on every return past this point.
        let _im_len_reset = ImLenReset(&imp.ime);

        // Get the keyvals for this event (`:1352-1357`).
        let keyval_unicode = key::keyval_to_unicode(keyval);
        let keyval_unicode_unshifted = key::keyval_unicode_unshifted(
            imp.gl_area.upcast_ref::<gtk4::Widget>(),
            key_event,
            keycode,
        );

        // We want to get the physical unmapped key to process physical keybinds
        // (`:1359-1379`).
        let physical_key = key::physical_key(keycode, keyval);

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

        // If we have no UTF-8 text, we try to convert our keyval to a text value.
        // We have to do this because GTK will not process "Ctrl+Shift+1" (on US
        // keyboards) as "Ctrl+!" but instead as "". But the keyval is set
        // correctly so we can at least extract that (`:1409-1424`).
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
            Err(_err) => {
                eprintln!("warn(gtk): error in key callback");
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
                    // the core in any way (`:1454-1456`).
                    self.set_bell_ringing(false);

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

        // The focus callback runs on an idle source because libghostty callbacks
        // can trigger focus loss and deadlock if the lock is held during the
        // callback (`:2761,2768-2783`). The reference `ref`s the widget and
        // `unref`s in the callback; capturing an owned strong ref here is the same
        // (dropped at the closure's end).
        let obj = self.clone();
        glib::idle_add_local_once(move || {
            let Some(core) = obj.core() else {
                return;
            };
            // SAFETY: `core` points at this widget's boxed, initialized core
            // surface (pinned in the imp); the idle source runs on the app thread.
            if unsafe { &mut *core }
                .focusCallback(obj.imp().focused.get())
                .is_err()
            {
                eprintln!("warn(gtk): error in focus callback");
            }
        });

        self.notify("focused");

        // Bell stops ringing as soon as we gain focus (`:2764-2765`). The
        // reference's `setBellRinging(false)` reduces here to the generated
        // property setter (set + notify-on-change); the extra `ringBell` /
        // freeze-notify semantics only matter for `setBellRinging(true)`, which
        // this tier never calls — they land with the bell task (T16).
        if focused {
            self.set_bell_ringing(false);
        }
    }

    /// PORTS: `surface.zig:3315-3319` `updateMapped`.
    fn update_mapped(&self, mapped: bool) {
        self.imp().mapped.set(mapped);
        self.notify("mapped");
    }

    /// PORTS: `surface.zig:3321-3326` `updateOcclusion`.
    fn update_occlusion(&self, visible: bool) {
        let Some(core) = self.core() else {
            return;
        };
        // SAFETY: `core` is the widget's boxed, initialized core surface; called
        // on the app thread from map/unmap.
        if unsafe { &mut *core }.occlusionCallback(visible).is_err() {
            eprintln!("warn(gtk): error in occlusion callback");
        }
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

        // If we have resize overlays enabled, setup an idler to show that. We do
        // this in an idle tick because doing it during the resize results in
        // flickering (`:3503-3519`).
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        let grid = unsafe { &*core }.size.grid();
        let text = format!("{} x {}", grid.columns, grid.rows);
        imp.resize_overlay.set_label(Some(&text));
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

        // Make the context current so we can call GL functions (`:3416-3424`).
        imp.gl_area.make_current();
        if let Some(err) = imp.gl_area.error() {
            eprintln!("warn(gtk): failed to make GL context current: {err}");
            eprintln!("warn(gtk): this error is usually due to a driver or gtk bug");
            eprintln!("warn(gtk): common cause: https://gitlab.gnome.org/GNOME/gtk/-/issues/4950");
            return Err(InitError::GlArea);
        }

        let app = Application::default();
        let alloc = app.allocator();

        // The core app / rt app backrefs (`app.core()` / `app.rt()`). In rustty
        // these are reconstituted from the singleton's type-erased cells; when
        // unbound (widget tests) we cannot construct a surface — bail (the lazy
        // init simply doesn't complete, leaving `core_surface` None).
        let Some(core_app) = app_core_ptr() else {
            eprintln!("warn(gtk): core app not bound; cannot initialize surface");
            return Err(InitError::Surface);
        };
        let Some(rt_app) = app_rt_ptr() else {
            eprintln!("warn(gtk): rt app not bound; cannot initialize surface");
            return Err(InitError::Surface);
        };

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

        // Initialize our surface configuration (`:3438-3442`), applying the
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

        // `if (priv.overrides.command) |c| config.command = c.clone(...)` (`:3445-3447`).
        if let Some(cmd) = imp.overrides_command.borrow().as_ref() {
            config.command = Some(cmd.clone());
        }
        // `if (priv.overrides.working_directory) |wd| ...` (`:3448-3453`).
        if let Some(wd) = imp.overrides_working_directory.borrow().as_ref() {
            let mut wd_val = WorkingDirectory::Path(wd.to_bytes().to_vec());
            wd_val.finalize();
            config.working_directory = Some(wd_val);
        }
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

        if init_res.is_err() {
            eprintln!("warn(gtk): failed to initialize surface");
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

// PORT NOTE (concern): the reference's typed `getMouseShape`/`setMouseShape`
// (`surface.zig:2176-2184`) over `terminal.MouseShape`, plus the
// `GhosttyMouseShape` GObject enum, are deferred to the mouse-input task
// (T16/T17) — see the `mouse-shape` property's PORT NOTE. The generated
// `mouse_shape()`/`set_mouse_shape()` `i32` accessors carry the shape meanwhile.

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
        eprintln!("warn(gtk): unable to perform prompt title action err={err:?}");
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
    let value = action.state().and_then(|s| s.get::<bool>()).unwrap_or(false);
    action.set_state(&(!value).to_variant());
}

// The 39 template callbacks (`surface.zig:3623-3661`). Per the placeholder-leaf
// doctrine most are `unimplemented!()` (Tasks 16/17/19 fill them); the three
// reached during template instantiation return safe values (see the module doc).
/// Map a [`MouseShape`] discriminant (as carried by the `mouse-shape` property) to
/// its W3C/CSS cursor name for `gtk.Widget.set_cursor_from_name`.
///
/// PORTS: the `switch (priv.mouse_shape)` in `surface.zig:2389-2427` (`propMouseShape`).
/// Indexed by the shape's `#[repr(i32)]` discriminant in `mouse::Shape` declaration
/// order (`rustty-vt/src/mouse.rs:70-105`); an out-of-range value falls back to
/// `"default"`.
fn mouse_shape_cursor_name(repr: i32) -> &'static str {
    // Declaration order of `mouse::Shape` (`mouse.rs:71-104`), each mapped to the
    // reference's cursor name (`surface.zig:2390-2426`).
    const NAMES: [&str; 34] = [
        "default",      // Default
        "context-menu", // ContextMenu
        "help",         // Help
        "pointer",      // Pointer
        "progress",     // Progress
        "wait",         // Wait
        "cell",         // Cell
        "crosshair",    // Crosshair
        "text",         // Text
        "vertical-text", // VerticalText
        "alias",        // Alias
        "copy",         // Copy
        "move",         // Move
        "no-drop",      // NoDrop
        "not-allowed",  // NotAllowed
        "grab",         // Grab
        "grabbing",     // Grabbing
        "all-scroll",   // AllScroll
        "col-resize",   // ColResize
        "row-resize",   // RowResize
        "n-resize",     // NResize
        "e-resize",     // EResize
        "s-resize",     // SResize
        "w-resize",     // WResize
        "ne-resize",    // NeResize
        "nw-resize",    // NwResize
        "se-resize",    // SeResize
        "sw-resize",    // SwResize
        "ew-resize",    // EwResize
        "ns-resize",    // NsResize
        "nesw-resize",  // NeswResize
        "nwse-resize",  // NwseResize
        "zoom-in",      // ZoomIn
        "zoom-out",     // ZoomOut
    ];
    usize::try_from(repr)
        .ok()
        .and_then(|i| NAMES.get(i).copied())
        .unwrap_or("default")
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
        this.set_bell_ringing(false);

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

        if button == Button::Middle && !imp.gtk_enable_primary_paste.get() {
            return;
        }

        let mods = key::translate_mods(event.modifier_state());
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        let consumed =
            match unsafe { (*core).mouseButtonCallback(ButtonState::Press, button, mods) } {
                Ok(consumed) => consumed,
                Err(_err) => {
                    eprintln!("warn(gtk): error in mouse button callback");
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

        if button == Button::Middle && !imp.gtk_enable_primary_paste.get() {
            return;
        }

        let mods = key::translate_mods(event.modifier_state());
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        let consumed =
            match unsafe { (*core).mouseButtonCallback(ButtonState::Release, button, mods) } {
                Ok(consumed) => consumed,
                Err(_err) => {
                    eprintln!("warn(gtk): error in mouse button callback");
                    return;
                }
            };

        // Trigger the on-screen keyboard if we have no selection, and the mouse
        // event hasn't been intercepted by the callback. It's better to do this
        // here rather than within the core callback since we have direct access
        // to the underlying gdk.Event here (`:2892-2901`).
        // SAFETY: `core` is the initialized core surface.
        if !consumed
            && button == Button::Left
            && !unsafe { (*core).hasSelection() }
            && !this.show_on_screen_keyboard(Some(&event))
        {
            eprintln!("warn(gtk): failed to activate the on-screen keyboard");
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
        // event without the cursor actually moving (GLArea resized under the
        // mouse; window title update). To prevent incorrect behavior, we'll only
        // grab focus and continue with callback logic if the cursor has actually
        // moved (`:2919-2930`).
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
            if unsafe { (*core).cursorPosCallback(pos, Some(mods)) }.is_err() {
                eprintln!("warn(gtk): error in cursor pos callback");
            }
        }
    }
    /// PORTS: `surface.zig:2955-2976` `ecMouseLeave`.
    #[template_callback]
    fn mouse_leave(ec_motion: &gtk4::EventControllerMotion, this: &Surface) {
        let Some(event) = ec_motion.current_event() else {
            return;
        };

        // If we have a core surface then we can send the cursor pos callback with
        // an invalid position to indicate the mouse left (`:2962-2975`).
        if let Some(core) = this.core() {
            let mods = key::translate_mods(event.modifier_state());
            let pos = CursorPos { x: -1.0, y: -1.0 };
            // SAFETY: `core` is the widget's boxed, initialized core surface.
            if unsafe { (*core).cursorPosCallback(pos, Some(mods)) }.is_err() {
                eprintln!("warn(gtk): error in cursor pos callback");
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
        // We invert (`* -1`) because we apply natural scrolling to the values
        // (`:3009-3016`); written `-x * m` to satisfy clippy::neg_multiply.
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        if unsafe { (*core).scrollCallback(-sx * multiplier, -sy * multiplier, scroll_mods) }
            .is_err()
        {
            eprintln!("warn(gtk): error in scroll callback");
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
    /// PORTS: `surface.zig:3025-3066` `ecMouseScrollHorizontal`.
    #[template_callback]
    fn scroll_horizontal(
        ec: &gtk4::EventControllerScroll,
        x: f64,
        _y: f64,
        this: &Surface,
    ) -> bool {
        let imp = this.imp();

        match ec.unit() {
            gdk4::ScrollUnit::Surface => {}
            _ => return false,
        }

        let pending = imp.pending_horizontal_scroll.get() + x;
        imp.pending_horizontal_scroll.set(pending);

        if pending.abs() < 120.0 {
            remove_source_id(
                imp.pending_horizontal_scroll_reset.take(),
                "pending horizontal scroll reset source",
            );

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

        remove_source_id(
            imp.pending_horizontal_scroll_reset.take(),
            "pending horizontal scroll reset source",
        );
        imp.pending_horizontal_scroll.set(0.0);

        true
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
                    if shell_escape_writer
                        .writeAll(path.as_os_str().as_bytes())
                        .is_err()
                    {
                        eprintln!("err(gtk): unable to write path to buffer");
                        continue;
                    }
                    if shell_escape_writer.writeAll(b"\n").is_err() {
                        eprintln!("err(gtk): unable to write to buffer");
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
                if shell_escape_writer
                    .writeAll(path.as_os_str().as_bytes())
                    .is_err()
                {
                    eprintln!("err(gtk): unable to write path to buffer");
                    return false;
                }
                if shell_escape_writer.writeAll(b"\n").is_err() {
                    eprintln!("err(gtk): unable to write to buffer");
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

    /// PORTS: `surface.zig:3898-3931` `Clipboard.paste`. Paste explicit text
    /// directly into the surface, regardless of the actual clipboard contents.
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

        // `surface.completeClipboardRequest(.paste, text, false)` (`:3906-3910`).
        // SAFETY: `core` is the widget's boxed, initialized core surface;
        // `completeClipboardRequest` takes `&mut self` (mirror the deref used by
        // the async `clipboard_request` completion above).
        match unsafe { &mut *core }.completeClipboardRequest(
            ClipboardRequest::Paste,
            text,
            false,
        ) {
            Ok(()) => {}

            // error.UnsafePaste, error.UnauthorizedPaste => confirmation
            // (`:3911-3921`).
            Err(CoreSurfaceError::UnsafePaste | CoreSurfaceError::UnauthorizedPaste) => {
                self.show_clipboard_confirmation(
                    ClipboardRequest::Paste,
                    &String::from_utf8_lossy(text),
                );
            }

            Err(err) => {
                eprintln!("warn(gtk): failed to complete clipboard request err={err}");
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
        eprintln!("debug(gtk): realize");
        let imp = self.imp();

        // Make the GL area current so we can detect any OpenGL errors; on error
        // we can't render and switch to the error state (`:3234-3245`).
        imp.gl_area.make_current();
        if let Some(err) = imp.gl_area.error() {
            eprintln!("warn(gtk): failed to make GL context current: {err}");
            eprintln!(
                "warn(gtk): this error is almost always due to a library, driver, or GTK issue"
            );
            eprintln!("warn(gtk): common cause: https://ghostty.org/docs/help/gtk-opengl-context");
            self.set_error(true);
            return;
        }

        // If we already have an initialized surface, notify it; otherwise we
        // initialize it on the first resize (`:3247-3257`).
        if let Some(core) = self.core() {
            // SAFETY: `core` is the widget's boxed, initialized core surface; the
            // renderer is aliased raw by the render thread (W5), so use raw-self.
            match unsafe {
                rustty_core::renderer_seam::Renderer::displayRealized(&raw mut (*core).renderer)
            } {
                Ok(()) => self.redraw(),
                Err(_err) => eprintln!("warn(gtk): core displayRealized failed"),
            }
        }

        // Set up our input method. This creates a strong ref back to ourselves,
        // released in unrealize (`:3259-3262`).
        imp.im_context.set_client_widget(Some(self));
    }

    /// PORTS: `surface.zig:3265-3297` `glareaUnrealize`.
    #[template_callback]
    fn gl_unrealize(&self, _area: &gtk4::GLArea) {
        eprintln!("debug(gtk): unrealize");
        let imp = self.imp();

        if let Some(core) = self.core() {
            // No guarantee the context is current on unrealize (`:3274-3276`).
            imp.gl_area.make_current();
            if let Some(err) = imp.gl_area.error() {
                eprintln!("warn(gtk): gl_area_make_current failed in unrealize msg={err}");
                eprintln!("warn(gtk): OpenGL resources and memory likely leaked");
                return;
            }
            // SAFETY: `core` is the widget's boxed, initialized core surface;
            // raw-self per the render-thread aliasing contract (W5).
            unsafe {
                rustty_core::renderer_seam::Renderer::displayUnrealized(&raw mut (*core).renderer);
            }
        }

        // Unset our input method (`:3295-3296`).
        imp.im_context.set_client_widget(gtk4::Widget::NONE);
    }

    /// PORTS: `surface.zig:3299-3305` `glareaMap`.
    #[template_callback]
    fn gl_map(&self, _area: &gtk4::GLArea) {
        self.update_mapped(true);
        self.update_occlusion(true);
    }

    /// PORTS: `surface.zig:3307-3313` `glareaUnmap`.
    #[template_callback]
    fn gl_unmap(&self, _area: &gtk4::GLArea) {
        self.update_mapped(false);
        self.update_occlusion(false);
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
        // No surface ⇒ we failed to initialize; nothing to draw (`:3333-3336`,
        // returns 1 / `true`: handled).
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
            Err(_err) => {
                eprintln!("warn(gtk): failed to draw frame");
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
    #[template_callback]
    fn gl_resize(&self, width: i32, height: i32) {
        let imp = self.imp();

        // Debug output to understand what GTK is telling us (`:3352-3369`). The
        // window (gdk-surface) scale is elided from the log here.
        eprintln!(
            "debug(gtk): gl resize width={width} height={height} scale={}",
            imp.gl_area.scale_factor(),
        );

        // Store our cached size (`:3371-3379`).
        let new_size = SurfaceSize {
            width: width as u32,
            height: height as u32,
        };
        let changed = !imp.size.get().eql(&new_size);
        imp.size.set(new_size);

        // If our surface is realized, we send callbacks (`:3381-3398`).
        if let Some(core) = self.core() {
            // We also update the content scale because there is no signal for
            // content-scale change and it seems to trigger a resize event
            // (`:3383-3387`) — this note is load-bearing.
            // SAFETY: `core` is the widget's boxed, initialized core surface.
            if unsafe { &mut *core }
                .contentScaleCallback(self.get_content_scale())
                .is_err()
            {
                eprintln!("warn(gtk): error in content scale callback");
            }

            if changed {
                // SAFETY: as above.
                if unsafe { &mut *core }.sizeCallback(new_size).is_err() {
                    eprintln!("warn(gtk): error in size callback");
                }
                self.resize_overlay_schedule();
            }

            return;
        }

        // If we don't have a surface, then we initialize it (`:3400-3403`).
        if self.init_surface().is_err() {
            eprintln!("warn(gtk): surface failed to initialize");
        }
    }

    // -- IMMulticontext (surface.blp:253-256) ------------------------------
    /// PORTS: `surface.zig:3076-3087` `imPreeditStart`.
    #[template_callback]
    fn im_preedit_start(_ctx: &gtk4::IMMulticontext, this: &Surface) {
        // Start our composing state for the input method and reset our input
        // buffer to empty (`:3082-3086`).
        let mut ime = this.imp().ime.borrow_mut();
        ime.im_composing = true;
        ime.im_len = 0;
    }
    /// PORTS: `surface.zig:3089-3126` `imPreeditChanged`.
    #[template_callback]
    fn im_preedit_changed(ctx: &gtk4::IMMulticontext, this: &Surface) {
        // Any preedit change should mark that we're composing (`:3095-3101`).
        this.imp().ime.borrow_mut().im_composing = true;

        // We can't set our preedit on our surface unless we're realized. We do
        // this now because we want to still keep our input method state coherent
        // (`:3103-3106`).
        let Some(core) = this.core() else {
            return;
        };

        // Get our pre-edit string that we'll use to show the user (`:3108-3116`).
        let (preedit, _attrs, _pos) = ctx.preedit_string();

        // Update our preedit state in Ghostty core (`:3118-3125`).
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        if unsafe { (*core).preeditCallback(Some(preedit.as_bytes())) }.is_err() {
            eprintln!("warn(gtk): error in preedit callback");
        }
    }
    /// PORTS: `surface.zig:3128-3143` `imPreeditEnd`.
    #[template_callback]
    fn im_preedit_end(_ctx: &gtk4::IMMulticontext, this: &Surface) {
        // End our composing state for GTK, allowing us to commit the text
        // (`:3134-3136`).
        this.imp().ime.borrow_mut().im_composing = false;

        // End our preedit state in Ghostty core (`:3138-3142`).
        let Some(core) = this.core() else {
            return;
        };
        // SAFETY: `core` is the widget's boxed, initialized core surface.
        if unsafe { (*core).preeditCallback(None) }.is_err() {
            eprintln!("warn(gtk): error in preedit callback");
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

        // We need to handle commit specially if we're in a key event.
        // Specifically, GTK will send us a commit event for basic key encodings
        // like "a" (on a US layout keyboard). We don't want to treat this as IME
        // committed text because we want to associate it with a key event
        // (`:3159-3192`).
        let in_keyevent = imp.ime.borrow().in_keyevent;
        match in_keyevent {
            // If we're not in a key event then this commit is from some other
            // source (i.e. on-screen keyboard, tablet, etc.) and we want to
            // commit the text to the core surface (`:3165-3168`).
            ImKeyEvent::False => {}

            // If we're in a composing state and in a key event then this key
            // event is resulting in a commit of multiple keypresses and we don't
            // want to encode it alongside the keypress (`:3170-3173`).
            ImKeyEvent::Composing => {}

            // If we're not composing then this commit is just a normal key
            // encoding and we want our key event to handle it so that Ghostty can
            // be aware of the key event alongside the text (`:3175-3191`,
            // `.not_composing`). See the [`ImKeyEvent`] naming note in `key_event`.
            ImKeyEvent::True => {
                let mut ime = imp.ime.borrow_mut();
                if str_bytes.len() > ime.im_buf.len() {
                    eprintln!("warn(gtk): not enough buffer space for input method commit");
                    return;
                }

                // Copy our committed text to the buffer (`:3185-3187`).
                ime.im_buf[..str_bytes.len()].copy_from_slice(str_bytes);
                // **Invariant 9**: `im_len ≤ 128` (the reference's `u7` bound).
                debug_assert!(str_bytes.len() <= ime.im_buf.len());
                ime.im_len = str_bytes.len() as u8;

                return;
            }
        }

        // If we reach this point from above it means we're composing OR not in a
        // keypress. In either case, we want to commit the text given to us
        // because that's what GTK is asking us to do (`:3194-3198`).

        // Committing ends composing state (`:3200-3201`).
        imp.ime.borrow_mut().im_composing = false;

        // We can't set our preedit on our surface unless we're realized
        // (`:3203-3225`).
        if let Some(core) = this.core() {
            // End our preedit state. Well-behaved input methods do this for us by
            // triggering a preedit-end event but some do not (ibus 1.5.29)
            // (`:3207-3211`).
            // SAFETY: `core` is the widget's boxed, initialized core surface.
            if unsafe { (*core).preeditCallback(None) }.is_err() {
                eprintln!("warn(gtk): error in preedit callback");
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
            if unsafe { (*core).keyCallback(event) }.is_err() {
                eprintln!("warn(gtk): error in key callback");
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
        // Closes the surface with no confirmation. NON-swapped signal: the emitting
        // ChildExited overlay is arg 0, `this: &Surface` is last (PORT NOTE above).
        this.close();
    }
    #[template_callback]
    fn context_menu_closed(_popover: &gtk4::PopoverMenu, this: &Surface) {
        // PORTS: `surface.zig:2608-2618` `contextMenuClosed(_: *gtk.PopoverMenu, self)`.
        // When the context menu closes, GTK moves focus back to the tab bar if there
        // are tabs — not what we want; grab it back on the surface (the GLArea).
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
            // Ensure an opaque background so the error message is readable
            // (`:2316-2322`).
            widget.add_css_class("background");
        } else {
            // Parent widgets set it appropriately otherwise (`:2323-2326`).
            widget.remove_css_class("background");
        }

        // We need to set our child property on an idle tick, because the error
        // property can be triggered by signals that are in the middle of widget
        // mapping and changing our child during that time results in a hard gtk
        // crash (`:2328-2335`). A weak backref keeps the source non-owning (removed
        // in `dispose`, **invariant 11**).
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
        // If we're not hidden we just trigger the mouse shape logic to handle
        // setting the proper mouse shape (`:2373-2375`).
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

        // When vadjustment is first set, we setup the signal group lazily. This
        // makes it so that if we don't use scrollbars, we never pay the memory
        // cost of this (`:2448-2461`). The typed `gobject.SignalGroup` binding is
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
                // PORTS: `vadjValueChanged` (`:2430-2439`). This will trigger for
                // every single pixel change in the adjustment, but our core
                // surface handles the noise from this so that identical rows are
                // cheap. `values[0]` is the emitting `gtk.Adjustment`.
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
                    eprintln!("warn(gtk): error performing scroll_to_row action err={err:?}");
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
    /// PORTS: `surface.zig:829-848` `closureShouldBorderBeShown`. `false` unless
    /// the bell is ringing AND the config's `bell-features.border` is set.
    #[template_callback]
    fn should_border_be_shown(&self, config: Option<Config>, bell_ringing: bool) -> bool {
        // The bell isn't ringing at creation — return early to avoid the
        // config-race warning (`surface.zig:836-841`).
        if !bell_ringing {
            return false;
        }
        let Some(config) = config else {
            eprintln!(
                "config unavailable for computing whether border should be shown, likely bug"
            );
            return false;
        };
        // `config.@"bell-features".border` (`surface.zig:848`).
        config.get().bell_features.border
    }

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
            eprintln!("warn(gtk): unable to perform end_search action err={err:?}");
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
            eprintln!("warn(gtk): unable to perform search action err={err:?}");
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
            rustty_input::binding::Action::NavigateSearch(rustty_input::binding::NavigateSearch::Next),
        ) {
            eprintln!("warn(gtk): unable to perform navigate_search action err={err:?}");
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
            eprintln!("warn(gtk): unable to perform navigate_search action err={err:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtk::gresource;

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
        if gtk4::init().is_err() {
            return;
        }
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

    /// PORTS: `surface.zig:4122-4127` `test "computeFraction"`. Display-free.
    #[test]
    fn surface_compute_fraction() {
        assert_eq!(compute_fraction(100), 1.0);
        assert_eq!(compute_fraction(255), 1.0);
        assert_eq!(compute_fraction(0), 0.0);
        assert_eq!(compute_fraction(50), 0.5);
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
        let Some(surface) = display_surface() else {
            return;
        };
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

    /// Sets up GTK + a default `GhosttyApplication` and returns a fresh `Surface`.
    /// Returns `None` when no display is available (the caller returns early).
    #[cfg(test)]
    fn display_surface() -> Option<Surface> {
        if gtk4::init().is_err() {
            return None;
        }
        let _ = libadwaita::init();
        crate::gtk::register_resources();
        let _app = crate::gtk::class::application::Application::build_default_for_test();
        Some(Surface::new(SurfaceOverrides::default()))
    }

    /// **Invariant 6 / §5.2** — `dispose` is idempotent (the null-guards). Populates
    /// an OWNED row (`config`) and a glib source id, calls `dispose()` twice, and
    /// asserts no panic/double-free and that every touched field is nulled. Before
    /// the T14 free-sites landed, `dispose` left these fields `Some` — RED.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn surface_dispose_is_idempotent() {
        let Some(surface) = display_surface() else {
            return;
        };
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
        let Some(surface) = display_surface() else {
            return;
        };
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
        let Some(surface) = display_surface() else {
            return;
        };

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
        let Some(surface) = display_surface() else {
            return;
        };
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
        let Some(surface) = display_surface() else {
            return;
        };
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

        let Some(surface) = display_surface() else {
            return;
        };

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

        let Some(surface) = display_surface() else {
            return;
        };
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
