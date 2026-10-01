//! PORTS: ghostty src/apprt/gtk/class/surface.zig — `Surface.Private`
//! (`surface.zig:570-745`, the `imp::Surface` instance struct with its
//! `CompositeTemplate` children and `glib::Properties`), the class-init
//! (`surface.zig:3593-3706`), and the `ObjectImpl`/`WidgetImpl`/`BinImpl`/
//! `ScrollableImpl` vfuncs.

use super::*;

/// PORTS: `surface.zig:570-745` `Surface.Private` — the tier's heart
/// (PREPORT_GTK §1.3). Every pointer-typed field is present with its §1.3
/// class: `TemplateChild<T>` for BORROW_FIELD rows (released en masse by
/// `dispose_template`, never unref'd by field), the OWNED rows at their
/// defaults (free-sites in T14/T15), and `core_surface` — the ONE OWNED
/// libghostty-core object — as `RefCell<Option<Box<CoreSurface>>>`, `None`
/// until the first render-surface resize (**invariant 3**).
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

    /// Whether the RenderSurface widget is mapped. Some operations like
    /// grabbing focus only work if a widget is mapped.
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
    /// The RenderSurface that displays the rendered output of the surface.
    /// PORTS: `surface.zig:626-627` `render_surface` (`2f0b65346`; the pin's
    /// `gl_area: *gtk.GLArea`).
    #[template_child]
    pub render_surface: TemplateChild<RenderSurface>,
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

    /// The core surface backing this GTK surface.
    ///
    /// This starts out null and unrealized and is initialized eagerly
    /// when we get our first resize event, since we don't know what
    /// size GTK will allocate for this widget beforehand. This will
    /// then be realized when the widget itself is realized.
    /// PORTS: `surface.zig:645-651` `core_surface: ?*CoreSurface` — the ONE
    /// OWNED libghostty-core object. **Invariant 3**: null until the first
    /// render-surface resize (T15); free-site (`deinit` + destroy) in the
    /// finalize `Drop`.
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
            render_surface: TemplateChild::default(),
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
        // `2f0b65346`: the template now instantiates `GhosttyRenderSurface`.
        RenderSurface::ensure_type();
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
                    std::alloc::dealloc(raw.cast::<u8>(), std::alloc::Layout::new::<CoreSurface>());
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
