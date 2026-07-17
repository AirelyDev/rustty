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
//! * **(3) `core_surface` realize ordering** — `core_surface` starts `None` and is
//!   NOT constructed here; it is lazily built on the first GLArea resize (T15).
//!   [`Surface::core`] returns the `Option` unchanged.
//! * **(9) IME buffer consistency** — `in_keyevent`/`im_composing`/`im_buf`/`im_len`
//!   are kept as ONE [`ImeState`] struct, never scattered.
//! * **(10) `action_group` self-reference** — `init_action_map` installs the group
//!   via [`crate::gtk::ext::actions::add_as_group`], whose closures hold a WEAK
//!   backref to the widget (the group's lifetime ⊆ the widget's).
//! * **(12) floating-ref sink** — `glib::Object::builder().build()` sinks the
//!   floating ref exactly as `gobject.ext.newInstance` does.

use std::cell::{Cell, RefCell};
use std::ffi::CString;

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
use rustty_config::value_types::WorkingDirectory;
use rustty_font_shaper::face::DesiredSize;
use rustty_vt::mouse::Shape as MouseShape;

use crate::gtk::app::App;
use crate::gtk::ext;
use crate::gtk::gsettings;
use crate::gtk::surface::Surface as ApprtSurface;

use super::application::Application;
use super::config::Config;
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
        /// libghostty-core object. **Invariant 3**: `None` until the first GLArea
        /// resize (T15); free-site (`deinit` + destroy) in T14/T15.
        pub core_surface: RefCell<Option<Box<CoreSurface>>>,

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
                core_surface: RefCell::new(None),
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
                    // [*:0]const u8)`. PORT NOTE (concern): `apprt.Clipboard` has no
                    // GObject type yet, so its param rides as the enum's `i32` here;
                    // the signal is never emitted in this tier.
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
            let present = self.core_surface.borrow().is_some();
            super::finalize_core_surface(
                present,
                // Remove ourselves from the list of known surfaces in the app. We
                // do this before deinit in case a callback triggers searching for
                // this surface (`surface.zig:1926-1929`).
                || super::delete_surface_from_app(std::ptr::from_ref(&self.rt_surface).cast_mut()),
                || {
                    if let Some(mut boxed) = self.core_surface.borrow_mut().take() {
                        // `v.deinit(); alloc.destroy(v)` (**invariant 2**,
                        // `surface.zig:1936-1937`). `CoreSurface` has no `Drop`, so
                        // dropping the `Box` frees the heap slot WITHOUT re-running
                        // teardown — the faithful `alloc.destroy`.
                        //
                        // SAFETY: `present` implies a fully-initialized surface (a
                        // successful `Surface::init`); `deinit`'s contract is met and
                        // the slot is freed without being used again.
                        unsafe { boxed.deinit() };
                    }
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
            self.core_surface
                .borrow()
                .as_ref()
                .is_some_and(|core| core.readonly)
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

// PORTS: `gobject.SignalGroup.setTarget` (`surface.zig:1869`). The `glib`
// crate's typed `SignalGroup` and its `gobject_ffi::g_signal_group_set_target`
// binding are both gated behind the `v2_72` glib feature this build does not
// enable (see the `vadj_signal_group` field note), so `dispose` binds the
// libgobject-2.0 C symbol directly. It has existed since GLib 2.72; this build
// links 2.88. `GSignalGroup*` is ABI-compatible with `GObject*`.
extern "C" {
    fn g_signal_group_set_target(
        self_: *mut glib::gobject_ffi::GObject,
        target: *mut glib::gobject_ffi::GObject,
    );
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

    /// PORTS: `surface.zig:766-769` `core`. The core surface backing this widget,
    /// or `None` before the first GLArea resize (**invariant 3**). The returned
    /// pointer is valid only while `core_surface` stays `Some`.
    #[must_use]
    pub fn core(&self) -> Option<*mut CoreSurface> {
        self.imp()
            .core_surface
            .borrow_mut()
            .as_mut()
            .map(|boxed| std::ptr::from_mut::<CoreSurface>(boxed))
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
        if imp.core_surface.borrow().is_some() {
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

    /// PORTS: `surface.zig:1500-1507` `close`. Emits `close-request`; the window
    /// tier decides what to do. The apprt seam's `close(process_alive)` discards
    /// `process_alive` and calls this (`Surface.zig:35-38`).
    pub fn close(&self) {
        self.emit_by_name::<()>("close-request", &[]);
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

    /// PORTS: `Surface.zig:40-42` `cgroup` → `self.surface.cgroupPath()`.
    ///
    /// **Placeholder-leaf (L6, invariant 8):** the systemd cgroup single-instance
    /// work owns this; kept inhabited returning `None`.
    #[must_use]
    pub fn cgroup_path(&self) -> Option<String> {
        None
    }

    /// PORTS: `surface.zig:1714-1724` `clipboardRequest`.
    ///
    /// **Placeholder-leaf (clipboard tier):** the GTK `Clipboard` module
    /// (`Clipboard.request`) is unported, so no read request is started yet
    /// (returns `false`, the "did not start" result, which is safe).
    pub fn clipboard_request(
        &self,
        _clipboard_type: Clipboard,
        _state: ClipboardRequest,
    ) -> Result<bool, ApprtError> {
        Ok(false)
    }

    /// PORTS: `surface.zig:1726-1738` `setClipboard`.
    ///
    /// **Placeholder-leaf (clipboard tier):** `Clipboard.set` is unported.
    pub fn set_clipboard(
        &self,
        _clipboard_type: Clipboard,
        _contents: &[ClipboardContent<'_>],
        _confirm: bool,
    ) {
    }

    /// PORTS: `surface.zig:1592-1641` `defaultTermioEnv`. The default environment
    /// for the surface's termio subprocess: the inherited process env with the
    /// language override applied and the GTK / desktop-activation vars stripped so
    /// they don't leak to the child.
    ///
    /// PORT NOTE (deferred): the snap-path filtering (`:1621-1626`, comptime
    /// `build_config.snap` — false in this build) and the `Window` winproto
    /// `addSubprocessEnv` hook (`:1632-1638`, the `Window` class is unported) land
    /// with their respective tiers; the load-bearing env is complete here.
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

        env
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

    /// PORTS: `surface.zig:3491-3520` `resizeOverlaySchedule`.
    ///
    /// **Placeholder-leaf (resize-overlay task, T16):** reached only with a live
    /// core surface (post-init). The `ResizeOverlay.setLabel`/`schedule` methods
    /// land with that task; until then this only honors the early-return guards.
    fn resize_overlay_schedule(&self) {
        // `const surface = priv.core_surface orelse return` (`:3493`).
        if self.imp().core_surface.borrow().is_some() {
            // PORT NOTE (placeholder-leaf, T16): only with a live core surface —
            // format the grid size into the overlay label and schedule an idle
            // show (`:3495-3519`).
        }
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
            imp.core_surface.borrow().is_none(),
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
            // `errdefer app.core().deleteSurface(self.rt())` (`:3435`).
            // SAFETY: `core_app` is live; `rt_surface` still points at our pinned
            // embedded apprt surface.
            unsafe { &mut *core_app }.deleteSurface(rt_surface);
            // `slot` (still `MaybeUninit`) drops here, freeing the heap slot
            // without running `CoreSurface`'s teardown (`alloc.destroy`).
            return Err(InitError::Surface);
        }

        // Store it! (`:3477-3478`). Rebuild the `Box` over the SAME heap allocation
        // (now initialized), so `surface_ptr` (held by core's renderer/io threads)
        // stays valid — the pin.
        // SAFETY: `init_res` is `Ok`, so the slot's payload is fully initialized;
        // the raw pointer came from `Box::into_raw` and is reconstituted exactly
        // once as `Box<CoreSurface>` (layout-identical to `Box<MaybeUninit<_>>`).
        let boxed: Box<CoreSurface> =
            unsafe { Box::from_raw(Box::into_raw(slot).cast::<CoreSurface>()) };
        imp.core_surface.replace(Some(boxed));

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

/// PORTS: `surface.zig:1975-...` `actionPromptTitle`. **Placeholder-leaf (T16/17)**:
/// the title-prompt dialog lands with the surface-action tier; a bare action
/// invocation is a no-op until then.
fn action_prompt_title(
    _action: &gio::SimpleAction,
    _param: Option<&glib::Variant>,
    _self: &Surface,
) {
    // PORT NOTE (placeholder-leaf): opens the `TitleDialog`; wired in T16/T17.
}

/// PORTS: `surface.zig:...` `actionNotifyOnNextCommandFinish`. **Placeholder-leaf
/// (T16/17)**.
fn action_notify_on_next_command_finish(
    _action: &gio::SimpleAction,
    _param: Option<&glib::Variant>,
    _self: &Surface,
) {
    // PORT NOTE (placeholder-leaf): toggles the notify-on-next-command state.
}

// The 39 template callbacks (`surface.zig:3623-3661`). Per the placeholder-leaf
// doctrine most are `unimplemented!()` (Tasks 16/17/19 fill them); the three
// reached during template instantiation return safe values (see the module doc).
#[gtk4::template_callbacks]
impl Surface {
    // -- EventControllerFocus (surface.blp:47-50) --------------------------
    #[template_callback]
    fn focus_enter(&self) {
        unimplemented!("gtk::class::Surface::focus_enter lands in L5.2 T16/T17")
    }
    #[template_callback]
    fn focus_leave(&self) {
        unimplemented!("gtk::class::Surface::focus_leave lands in L5.2 T16/T17")
    }

    // -- EventControllerKey (surface.blp:52-55) ----------------------------
    #[template_callback]
    fn key_pressed(&self) -> bool {
        unimplemented!("gtk::class::Surface::key_pressed lands in L5.2 T16/T17")
    }
    #[template_callback]
    fn key_released(&self) {
        unimplemented!("gtk::class::Surface::key_released lands in L5.2 T16/T17")
    }

    // -- GestureClick (surface.blp:74-78) ----------------------------------
    #[template_callback]
    fn mouse_down(&self) {
        unimplemented!("gtk::class::Surface::mouse_down lands in L5.2 T16/T17")
    }
    #[template_callback]
    fn mouse_up(&self) {
        unimplemented!("gtk::class::Surface::mouse_up lands in L5.2 T16/T17")
    }

    // -- EventControllerMotion (surface.blp:69-72) -------------------------
    #[template_callback]
    fn mouse_motion(&self) {
        unimplemented!("gtk::class::Surface::mouse_motion lands in L5.2 T16/T17")
    }
    #[template_callback]
    fn mouse_leave(&self) {
        unimplemented!("gtk::class::Surface::mouse_leave lands in L5.2 T16/T17")
    }

    // -- EventControllerScroll (surface.blp:57-67) -------------------------
    #[template_callback]
    fn scroll_vertical(&self) -> bool {
        unimplemented!("gtk::class::Surface::scroll_vertical lands in L5.2 T16/T17")
    }
    #[template_callback]
    fn scroll_vertical_begin(&self) {
        unimplemented!("gtk::class::Surface::scroll_vertical_begin lands in L5.2 T16/T17")
    }
    #[template_callback]
    fn scroll_vertical_end(&self) {
        unimplemented!("gtk::class::Surface::scroll_vertical_end lands in L5.2 T16/T17")
    }
    #[template_callback]
    fn scroll_horizontal(&self) -> bool {
        unimplemented!("gtk::class::Surface::scroll_horizontal lands in L5.2 T16/T17")
    }

    // -- DropTarget (surface.blp:224-227) ----------------------------------
    #[template_callback]
    fn drop(&self) -> bool {
        unimplemented!("gtk::class::Surface::drop lands in L5.2 T16/T17")
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
    #[template_callback]
    fn gl_render(&self, _area: &gtk4::GLArea) -> bool {
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
    #[template_callback]
    fn gl_resize(&self, _area: &gtk4::GLArea, width: i32, height: i32) {
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
    #[template_callback]
    fn im_preedit_start(&self) {
        unimplemented!("gtk::class::Surface::im_preedit_start lands in L5.2 T16/T17")
    }
    #[template_callback]
    fn im_preedit_changed(&self) {
        unimplemented!("gtk::class::Surface::im_preedit_changed lands in L5.2 T16/T17")
    }
    #[template_callback]
    fn im_preedit_end(&self) {
        unimplemented!("gtk::class::Surface::im_preedit_end lands in L5.2 T16/T17")
    }
    #[template_callback]
    fn im_commit(&self) {
        unimplemented!("gtk::class::Surface::im_commit lands in L5.2 T16/T17")
    }

    // -- URL hover (surface.blp:171-174) -----------------------------------
    #[template_callback]
    fn url_mouse_enter(&self) {
        unimplemented!("gtk::class::Surface::url_mouse_enter lands in L5.2 T16/T17")
    }
    #[template_callback]
    fn url_mouse_leave(&self) {
        unimplemented!("gtk::class::Surface::url_mouse_leave lands in L5.2 T16/T17")
    }

    // -- child-exited / context menu ---------------------------------------
    #[template_callback]
    fn child_exited_close(&self) {
        unimplemented!("gtk::class::Surface::child_exited_close lands in L5.2 T16/T17")
    }
    #[template_callback]
    fn context_menu_closed(&self) {
        unimplemented!("gtk::class::Surface::context_menu_closed lands in L5.2 T16/T17")
    }

    // -- notify:: property handlers (surface.blp:235-240) ------------------
    /// PORTS: `surface.zig:2258-2308` `propConfig`. **Reached during `init`**
    /// (`:1832`), so this must not panic. No-op until L5.2 T16 applies the config
    /// to the resize-overlay (duration / position).
    // Renamed `on_notify_*` (blueprint name kept via `name = …`) so they don't
    // clash with the `glib::Properties`-generated `notify_<prop>()` emitters.
    #[template_callback(name = "notify_config")]
    fn on_notify_config(&self) {
        // PORT NOTE (placeholder, reachable): applies `config` to `resize_overlay`
        // (`surface.zig:2266-2307`); wired in T16.
    }
    /// PORTS: `surface.zig:2310-...` `propError`. **Reached from
    /// [`Surface::set_error`]** (the T15 GL-error path), so this must not panic.
    /// Ports the opaque-background CSS toggle (`:2318-2331`); the idle-tick child
    /// swap between the GLArea and the `error_page` (`onIdleRechild`, `:2333-...`)
    /// is deferred to T16 with the resize-overlay / error-page display work.
    #[template_callback(name = "notify_error")]
    fn on_notify_error(&self) {
        let widget = self.upcast_ref::<gtk4::Widget>();
        if self.imp().error.get() {
            // Ensure an opaque background so the error message is readable
            // (`:2318-2322`).
            widget.add_css_class("background");
        } else {
            // Parent widgets set it appropriately otherwise (`:2325-2330`).
            widget.remove_css_class("background");
        }

        // PORT NOTE (placeholder, T16): schedule `idle_rechild` to swap the visible
        // child to the `error_page` on an idle tick (`surface.zig:2333-...`,
        // `onIdleRechild`) — deferred with the error-page display work. The `error`
        // property + CSS are already authoritative for listeners.
    }
    #[template_callback(name = "notify_mouse_hover_url")]
    fn on_notify_mouse_hover_url(&self) {
        unimplemented!("gtk::class::Surface::notify_mouse_hover_url lands in L5.2 T16/T17")
    }
    #[template_callback(name = "notify_mouse_hidden")]
    fn on_notify_mouse_hidden(&self) {
        unimplemented!("gtk::class::Surface::notify_mouse_hidden lands in L5.2 T16/T17")
    }
    #[template_callback(name = "notify_mouse_shape")]
    fn on_notify_mouse_shape(&self) {
        unimplemented!("gtk::class::Surface::notify_mouse_shape lands in L5.2 T16/T17")
    }
    /// PORTS: `surface.zig:2540-...` `propVAdjustment`. **Reached during
    /// construction** — GtkScrollable's `vadjustment` is a construct property GTK
    /// sets to a default adjustment, firing `notify::vadjustment` — so this must
    /// not panic. No-op until L5.2 T15 wires the scroll `SignalGroup`.
    #[template_callback(name = "notify_vadjustment")]
    fn on_notify_vadjustment(&self) {
        // PORT NOTE (placeholder, reachable): installs the vadj `SignalGroup` and
        // syncs the scrollbar (`surface.zig` `propVAdjustment`); wired in T15.
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
    #[template_callback]
    fn search_stop(&self) {
        unimplemented!("gtk::class::Surface::search_stop lands in L5.2 T16/T17")
    }
    #[template_callback]
    fn search_changed(&self) {
        unimplemented!("gtk::class::Surface::search_changed lands in L5.2 T16/T17")
    }
    #[template_callback]
    fn search_next_match(&self) {
        unimplemented!("gtk::class::Surface::search_next_match lands in L5.2 T16/T17")
    }
    #[template_callback]
    fn search_previous_match(&self) {
        unimplemented!("gtk::class::Surface::search_previous_match lands in L5.2 T16/T17")
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
}
