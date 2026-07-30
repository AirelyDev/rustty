//! rustty-apprt::gtk::class::window
//!
//! PORTS: ghostty `src/apprt/gtk/class/window.zig` (2163 loc) — the
//! `GhosttyWindow`, an `adw::ApplicationWindow` subclass that hosts the tab/split
//! chain. A `#[glib::object_subclass]` + `#[derive(CompositeTemplate)]` +
//! `glib::Properties` over `ui/1.5/window.blp` (AD-3: the modern arm, gtk≥4.14/
//! adw≥1.5, so the `supportsTabOverview()`/`supportsToolbarView()` conditionals
//! collapse away).
//!
//! # `Private` — PREPORT_GTK §1.2 LIFETIMES (`window.zig:218-270`)
//!
//! | field | class | rust |
//! |---|---|---|
//! | `tab_bindings` | OWNED | [`TabBindings`] (a `GBindingGroup`; `unref` on drop) — `:232` |
//! | `config` | SHARED | `RefCell<Option<Config>>` (refcounted; `unref` in dispose) — `:235` |
//! | `winproto` | embedded | `RefCell<winproto::Window>` (§1.11) — `:238` |
//! | `command_palette` | SHARED (weak) | [`WeakRef<CommandPalette>`] — `:256` |
//! | `context_menu_page` | BORROW_FIELD | `RefCell<Option<adw::TabPage>>` (owned by `tab_view`; set by `setup-menu`) — `:260` |
//! | `tab_overview`/`tab_bar`/`tab_view`/`toolbar`/`toast_overlay` | BORROW_FIELD | `TemplateChild<…>` (`disposeTemplate` frees) — `:263-267` |
//! | `handle_active_state_source` | OWNED(handle) | `RefCell<Option<SourceId>>` (**invariant 11**) — `:224` |
//! | `tab_overview_focus_timer` | OWNED(handle) | `RefCell<Option<SourceId>>` (**invariant 11**) — `:253` |
//!
//! # Cross-class dependencies (INHABITED placeholders / leaves)
//!
//! The window template instantiates `$GhosttyDebugWarning` and casts through
//! `$GhosttyTab`/`$GhosttySurface` (`window.blp:50,153`), and `Class.init`
//! `ensureType`s [`DebugWarning`]/[`SplitTree`]/[`Surface`]/[`Tab`]
//! (`window.zig:2097-2100`). The window's tab-*creation* and
//! surface-tree-*iteration* paths (`newTabPage`, `connectSurfaceHandlers`/
//! `disconnectSurfaceHandlers`, `tabViewPageAttached`/`Detached`,
//! `tabSplitTreeChanged`, the surface signal handlers, `surfaceInit`) are LIVE
//! against the real [`Tab::new`]/[`SplitTree`] model (T21): `new_window`/
//! `new_tab` host a running terminal in this window. Remaining INHABITED
//! leaves: the command-palette (`toggleCommandPalette`), inspector
//! (`toggleInspector`, L6), the `CloseConfirmationDialog` confirm paths (T23),
//! and the title-prompt (T23) — all off the create/host/render path.

use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::marker::PhantomData;

use gtk4::glib;
use gtk4::glib::Properties;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::CompositeTemplate;
use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;

use rustty_apprt_api::{Clipboard, NewSurfaceContext};
use rustty_config::value_types::{
    Fullscreen, GtkTabsLocation, GtkTitlebarStyle, GtkToolbarStyle, WindowNewTabPosition,
    WindowShowTabBar, WindowSubtitle, WindowTheme,
};
use rustty_input::binding;

use crate::gtk::i18n::tr;
use crate::gtk::{ext, gtk_version, winproto, WeakRef};

use super::application::Application;
use super::close_confirmation_dialog::{CloseConfirmationDialog, Target};
use super::command_palette::{ActionBoxed, CommandPalette};
use super::config::Config;
use super::debug_warning::DebugWarning;
use super::split_tree::{SplitTree, Tree};
use super::surface::Surface;
use super::tab::{Tab, TabOverrides};

// `GtkTitlebarStyle` is carried as its `#[repr(C)]` discriminant across the
// `titlebar-style` GObject property boundary (`.native == 0`, `.tabs == 1`).
const TITLEBAR_STYLE_NATIVE: i32 = 0;
const TITLEBAR_STYLE_TABS: i32 = 1;

mod imp {
    use super::*;

    /// PORTS: `window.zig:35-270` `Window` + its `Private` (`:218-270`). The
    /// registered properties (`window.zig:47-216`) are `glib::Properties` fields;
    /// the `Private` rows are the non-property fields below (PREPORT_GTK §1.2).
    #[derive(Default, CompositeTemplate, Properties)]
    #[template(resource = "/io/airely/rustty/ui/1.5/window.ui")]
    #[properties(wrapper_type = super::Window)]
    pub struct Window {
        // ---- Properties (window.zig:47-216) --------------------------------
        /// PORTS: `window.zig:52-68` `active-surface` (computed, getter transfer
        /// `.none`). The focus target for surface-scoped actions.
        #[property(name = "active-surface", get = Self::active_surface_prop, nullable)]
        pub active_surface_prop: PhantomData<Option<Surface>>,

        /// PORTS: `window.zig:70-80,235` `config` (`?*Config`, SHARED; obj accessor
        /// get `.none` / set `.full`).
        #[property(get, set, nullable)]
        pub config: RefCell<Option<Config>>,

        /// PORTS: `window.zig:82-99` `debug` (const `build_config.is_debug`).
        #[property(name = "debug", get = Self::debug_prop)]
        pub debug_prop: PhantomData<bool>,

        /// PORTS: `window.zig:120-133` `headerbar-visible` (computed).
        #[property(name = "headerbar-visible", get = Self::headerbar_visible)]
        pub headerbar_visible_prop: PhantomData<bool>,

        /// PORTS: `window.zig:135-151,221` `quick-terminal` (private field).
        #[property(name = "quick-terminal", get, set)]
        pub quick_terminal: Cell<bool>,

        /// PORTS: `window.zig:153-166` `tabs-autohide` (computed).
        #[property(name = "tabs-autohide", get = Self::tabs_autohide)]
        pub tabs_autohide_prop: PhantomData<bool>,

        /// PORTS: `window.zig:183-196` `tabs-visible` (computed).
        #[property(name = "tabs-visible", get = Self::tabs_visible)]
        pub tabs_visible_prop: PhantomData<bool>,

        /// PORTS: `window.zig:168-181` `tabs-wide` (computed).
        #[property(name = "tabs-wide", get = Self::tabs_wide)]
        pub tabs_wide_prop: PhantomData<bool>,

        /// PORTS: `window.zig:198-215` `toolbar-style` (computed `adw.ToolbarStyle`).
        #[property(name = "toolbar-style", get = Self::toolbar_style, builder(adw::ToolbarStyle::Raised))]
        pub toolbar_style_prop: PhantomData<adw::ToolbarStyle>,

        /// PORTS: `window.zig:101-118` `titlebar-style` (computed
        /// `GtkTitlebarStyle`). PORT NOTE (concern): the reference registers
        /// `GtkTitlebarStyle` as a GObject enum; rustty carries the shape's
        /// `#[repr(C)]` discriminant (`.native==0`/`.tabs==1`) across the property
        /// boundary — the `titlebar_style_is_tabs` closure (`window.blp:94,118`)
        /// consumes it as the same `i32`. The property NAME/notify are preserved.
        #[property(name = "titlebar-style", get = Self::titlebar_style)]
        pub titlebar_style_prop: PhantomData<i32>,

        // ---- Template children (BORROW_FIELD, window.zig:263-267) ----------
        /// PORTS: `window.zig:263` `tab_overview`.
        #[template_child]
        pub tab_overview: TemplateChild<adw::TabOverview>,
        /// PORTS: `window.zig:264` `tab_bar`.
        #[template_child]
        pub tab_bar: TemplateChild<adw::TabBar>,
        /// PORTS: `window.zig:265` `tab_view`.
        #[template_child]
        pub tab_view: TemplateChild<adw::TabView>,
        /// PORTS: `window.zig:266` `toolbar`.
        #[template_child]
        pub toolbar: TemplateChild<adw::ToolbarView>,
        /// PORTS: `window.zig:267` `toast_overlay`.
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,

        // ---- Private (non-property) state (window.zig:218-260) -------------
        /// PORTS: `window.zig:224` `handle_active_state_source` — glib source
        /// (**invariant 11**), removed in `dispose`.
        pub handle_active_state_source: RefCell<Option<glib::SourceId>>,

        /// PORTS: `window.zig:229` `window_decoration` — the per-window decoration
        /// override; `None` inherits the config.
        pub window_decoration: RefCell<Option<rustty_config::value_types::WindowDecoration>>,

        /// PORTS: `window.zig:232` `tab_bindings` — the OWNED `GBindingGroup`. Set
        /// in `constructed`, `set_source(None)` in `dispose`, `unref` on `Drop`.
        pub tab_bindings: RefCell<Option<TabBindings>>,

        /// PORTS: `window.zig:238` `winproto` — the per-window windowing protocol
        /// state (§1.11). `.none` until realized (`window.zig:312`).
        pub winproto: RefCell<winproto::Window>,

        /// PORTS: `window.zig:250` `surface_init` — whether any surface has
        /// initialized (gates default-size).
        pub surface_init: Cell<bool>,

        /// PORTS: `window.zig:253` `tab_overview_focus_timer` — glib source
        /// (**invariant 11**), removed in `dispose`.
        pub tab_overview_focus_timer: RefCell<Option<glib::SourceId>>,

        /// PORTS: `window.zig:256` `command_palette` — a WEAK reference
        /// (**invariant 4**): the referent can finalize out from under us.
        pub command_palette: WeakRef<CommandPalette>,

        /// PORTS: `window.zig:260` `context_menu_page` — the tab page the context
        /// menu was opened for (owned by `tab_view`; set by `setup-menu`).
        pub context_menu_page: RefCell<Option<adw::TabPage>>,

        /// PORT DIVERGENCE (documented; same mechanism as `split_tree.rs`
        /// `surface_handlers`): the reference disconnects a surface's window-level
        /// handlers via `signalHandlersDisconnectMatched(surface, .{.data=true},
        /// …, self)` (`window.zig:775-784,845-857`) — match-by-userdata. gtk4-rs
        /// closures don't register `self` as C userdata, so the window tracks the
        /// `(surface, id)` pairs it connects and disconnects exactly those. The
        /// closures hold only a WEAK window ref (no cycle); the strong `Surface`
        /// refs here drop at finalize.
        pub surface_handlers: RefCell<Vec<(Surface, glib::SignalHandlerId)>>,

        /// PORT DIVERGENCE (documented; see `surface_handlers`): the tab
        /// `close-request` handlers the reference disconnects by userdata in
        /// `tabViewPageDetached` (`window.zig:1574-1582`), tracked as
        /// `(tab, id)` pairs instead.
        pub tab_handlers: RefCell<Vec<(Tab, glib::SignalHandlerId)>>,

        /// Guards [`WidgetImpl`]'s `dispose_template` against a second call.
        /// GObject `dispose` is contractually idempotent and may run more than once
        /// (PREPORT_GTK §5.2), but `gtk_widget_dispose_template` is NOT — a second
        /// pass double-frees the template children. The reference never re-enters
        /// `dispose`; rustty makes it explicitly idempotent so a `g_object_run_dispose`
        /// (or any double dispose) is sound. No reference field — a rustty guard.
        pub template_disposed: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        // PORTS: `window.zig:40` `.name = "GhosttyWindow"`.
        const NAME: &'static str = "GhosttyWindow";
        type Type = super::Window;
        // PORTS: `window.zig:38` `pub const Parent = adw.ApplicationWindow`.
        type ParentType = adw::ApplicationWindow;

        /// PORTS: `window.zig:2096-2157` `Class.init`. `ensureType`s the four
        /// template subclasses so `ui/1.5/window.ui` resolves them (`:2097-2100`),
        /// then binds the template + the instance callbacks.
        fn class_init(klass: &mut Self::Class) {
            DebugWarning::ensure_type();
            SplitTree::ensure_type();
            Surface::ensure_type();
            Tab::ensure_type();

            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            // PORTS: `window.zig:296` `gtk.Widget.initTemplate`.
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Window {
        /// PORTS: `window.zig:295-352` `init` (instance-init tail). `init_template`
        /// already ran in [`ObjectSubclass::instance_init`]; the remainder — config
        /// fallback, winproto `.none`, the debug CSS class, the tab BindingGroup,
        /// the window icon, the action map, the maximize/fullscreen/title start
        /// states, the appearance sync, and the initial `active-surface` notify —
        /// runs here.
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Config fallback from the app (`window.zig:302-308`). Set the field
            // directly (no `notify::config`), matching the reference's
            // `priv.config = app.getConfig()`.
            if self.config.borrow().is_none() {
                let config = Application::default().get_config();
                self.config.replace(config);
            }

            // `priv.winproto = .none` (`window.zig:312`) — already the field default.

            // Debug CSS class (`window.zig:315-317`).
            if cfg!(debug_assertions) {
                obj.add_css_class("devel");
            }

            // Tab BindingGroup (`window.zig:321-322`): sync `title` from the active
            // tab to the window. **Invariant 12**: manually held (not sunk).
            let bindings = TabBindings::new();
            bindings.bind("title", obj.upcast_ref::<glib::Object>(), "title", 0);
            self.tab_bindings.replace(Some(bindings));

            // Window icon (`window.zig:326`): from the bundle id (R2 — rustty
            // identity, not the hardcoded reference id).
            obj.set_icon_name(Some(rustty_identity::BUNDLE_ID));

            // Actions (`window.zig:329`).
            self.init_action_map();

            // Start states from config (`window.zig:332-342`).
            {
                let cfg = self.config.borrow();
                if let Some(config) = cfg.as_ref() {
                    let c = config.get();
                    if c.maximize {
                        obj.maximize();
                    }
                    if !matches!(c.fullscreen, Fullscreen::False) {
                        obj.fullscreen();
                    }
                    if let Some(title) = &c.title {
                        obj.set_title(Some(&String::from_utf8_lossy(title)));
                    }
                }
            }

            // Sync appearance last (`window.zig:347`) — config load affects the
            // bindings set up in `initTemplate`.
            self.sync_appearance();

            // Kick the dynamic `active-surface` getter (`window.zig:351`).
            obj.notify("active-surface");
        }

        /// PORTS: `window.zig:1257-1285` `dispose`. Frees the OWNED §1.2 rows in
        /// the reference's order, then `disposeTemplate`; gtk4-rs chains the parent
        /// dispose after we return. Idempotent (`take()`/null-guarded).
        fn dispose(&self) {
            // `handle_active_state_source` remove+null (**invariant 11**,
            // `window.zig:1260-1265`).
            remove_source_id(
                self.handle_active_state_source.take(),
                "handle active state source",
            );

            // PORT NOTE (invariant 11, sound divergence): the reference's `dispose`
            // does NOT remove `tab_overview_focus_timer` (only `tabOverviewOpen`/
            // `tabOverviewFocusTimer` manage it), so a pending 500ms timer can
            // outlive the window and fire against a finalized `self` — a latent
            // use-after-free in the reference. In rustty the timer closure captures
            // the window; removing the source here (invariant 11: source ⇔ object
            // lifetime) is the sound re-expression.
            remove_source_id(
                self.tab_overview_focus_timer.take(),
                "tab overview focus timer",
            );

            // `command_palette.set(null)` (**invariant 4**, `window.zig:1267`).
            self.command_palette.set(None);

            // `config` unref+null (`window.zig:1269-1272`). Scope-end drop unrefs.
            drop(self.config.take());

            // `tab_bindings.setSource(null)` (`window.zig:1274`) — detach the source
            // but KEEP the group (its `unref` is the finalize `Drop`).
            if let Some(bindings) = self.tab_bindings.borrow().as_ref() {
                bindings.set_source(None::<&glib::Object>);
            }

            // `gtk.Widget.disposeTemplate(self, getGObjectType())`
            // (`window.zig:1276-1279`). Guarded (see `template_disposed`): only the
            // first `dispose` tears the template down.
            if !self.template_disposed.replace(true) {
                self.dispose_template();
            }
        }
    }

    impl Drop for Window {
        /// PORTS: `window.zig:1287-1296` `finalize`. In gtk4-rs the `imp` `Drop` IS
        /// the finalize hook: `tab_bindings.unref()` (`:1289`) is the field drop,
        /// and `winproto.deinit()` (`:1290`) runs explicitly.
        fn drop(&mut self) {
            // `priv.tab_bindings.unref()` (`window.zig:1289`) — the OWNED group's
            // `Drop` unrefs when the field drops below; we take it to make the
            // free-site explicit and ordered before the winproto teardown.
            drop(self.tab_bindings.take());
            // `priv.winproto.deinit()` (`window.zig:1290`).
            self.winproto.borrow_mut().deinit();
        }
    }

    impl WidgetImpl for Window {}
    impl WindowImpl for Window {}
    impl ApplicationWindowImpl for Window {}
    impl AdwApplicationWindowImpl for Window {}

    // -- Property getters (window.zig getters) -------------------------------
    impl Window {
        /// PORTS: `window.zig:903-906` `getActiveSurface`.
        fn active_surface_prop(&self) -> Option<Surface> {
            self.get_active_surface()
        }

        /// PORTS: `window.zig:92-94` `debug` getter — `build_config.is_debug`.
        #[allow(clippy::unused_self)]
        fn debug_prop(&self) -> bool {
            cfg!(debug_assertions)
        }

        /// PORTS: `window.zig:994-1025` `getHeaderbarVisible`.
        fn headerbar_visible(&self) -> bool {
            // Never with CSDs disabled (`:997-999`).
            if !self.winproto.borrow().client_side_decoration_enabled() {
                return false;
            }
            // Never as a quick terminal (`:1001-1002`).
            if self.quick_terminal.get() {
                return false;
            }
            // Never when fullscreen (`:1004-1005`).
            if self.obj().is_fullscreen() {
                return false;
            }
            let cfg = self.config.borrow();
            let Some(config) = cfg.as_ref() else {
                return true;
            };
            let c = config.get();
            // Conditionally hide when maximized (`:1011-1015`).
            if self.obj().is_maximized() && c.gtk_titlebar_hide_when_maximized {
                return false;
            }
            match c.gtk_titlebar_style {
                GtkTitlebarStyle::Tabs => false,
                GtkTitlebarStyle::Native => c.gtk_titlebar,
            }
        }

        /// PORTS: `window.zig:1027-1047` `getTabsAutohide`.
        fn tabs_autohide(&self) -> bool {
            let cfg = self.config.borrow();
            let Some(config) = cfg.as_ref() else {
                return true;
            };
            let c = config.get();
            match c.gtk_titlebar_style {
                GtkTitlebarStyle::Tabs => false,
                GtkTitlebarStyle::Native => match c.window_show_tab_bar {
                    WindowShowTabBar::Auto => true,
                    WindowShowTabBar::Always => false,
                    WindowShowTabBar::Never => true,
                },
            }
        }

        /// PORTS: `window.zig:1049-1069` `getTabsVisible`.
        fn tabs_visible(&self) -> bool {
            let cfg = self.config.borrow();
            let Some(config) = cfg.as_ref() else {
                return true;
            };
            let c = config.get();
            match c.gtk_titlebar_style {
                GtkTitlebarStyle::Tabs => {
                    if self.obj().is_maximized() && c.gtk_titlebar_hide_when_maximized {
                        return false;
                    }
                    true
                }
                GtkTitlebarStyle::Native => match c.window_show_tab_bar {
                    WindowShowTabBar::Always | WindowShowTabBar::Auto => true,
                    WindowShowTabBar::Never => false,
                },
            }
        }

        /// PORTS: `window.zig:1071-1075` `getTabsWide`.
        fn tabs_wide(&self) -> bool {
            let cfg = self.config.borrow();
            match cfg.as_ref() {
                Some(config) => config.get().gtk_wide_tabs,
                None => true,
            }
        }

        /// PORTS: `window.zig:1077-1085` `getToolbarStyle`.
        fn toolbar_style(&self) -> adw::ToolbarStyle {
            let cfg = self.config.borrow();
            let Some(config) = cfg.as_ref() else {
                return adw::ToolbarStyle::Raised;
            };
            match config.get().gtk_toolbar_style {
                GtkToolbarStyle::Flat => adw::ToolbarStyle::Flat,
                GtkToolbarStyle::Raised => adw::ToolbarStyle::Raised,
                GtkToolbarStyle::RaisedBorder => adw::ToolbarStyle::RaisedBorder,
            }
        }

        /// PORTS: `window.zig:1087-1091` `getTitlebarStyle`.
        fn titlebar_style(&self) -> i32 {
            let cfg = self.config.borrow();
            match cfg.as_ref() {
                Some(config) => match config.get().gtk_titlebar_style {
                    GtkTitlebarStyle::Native => TITLEBAR_STYLE_NATIVE,
                    GtkTitlebarStyle::Tabs => TITLEBAR_STYLE_TABS,
                },
                None => TITLEBAR_STYLE_NATIVE,
            }
        }
    }

    // -- Internal helpers (window.zig methods) -------------------------------
    impl Window {
        /// PORTS: `window.zig:903-906` `getActiveSurface`. Does not add a ref.
        pub(super) fn get_active_surface(&self) -> Option<Surface> {
            self.get_selected_tab()?.get_active_surface()
        }

        /// PORTS: `window.zig:959-965` `getSelectedTab`.
        pub(super) fn get_selected_tab(&self) -> Option<Tab> {
            let page = self.tab_view.selected_page()?;
            page.child().downcast::<Tab>().ok()
        }

        /// PORTS: `window.zig:967-984` `getNeedsConfirmQuit`.
        pub(super) fn get_needs_confirm_quit(&self) -> bool {
            let n = self.tab_view.n_pages();
            for i in 0..n {
                let page = self.tab_view.nth_page(i);
                let Ok(tab) = page.child().downcast::<Tab>() else {
                    eprintln!("warn(gtk): unexpected non-Tab child in tab view");
                    continue;
                };
                if tab.get_needs_confirm_quit() {
                    return true;
                }
            }
            false
        }

        /// PORTS: `window.zig:733-739` `toggleCssClass`.
        fn toggle_css_class(&self, class: &str, value: bool) {
            let obj = self.obj();
            if value {
                obj.add_css_class(class);
            } else {
                obj.remove_css_class(class);
            }
        }

        /// PORTS: `window.zig:742-752` `performBindingAction`.
        pub(super) fn perform_binding_action(&self, action: binding::Action) {
            let Some(surface) = self.get_active_surface() else {
                return;
            };
            let Some(core) = surface.core() else {
                return;
            };
            // SAFETY: `core` is the surface widget's boxed, initialized core surface
            // (published by `Surface::init`, cleared only after the render/io
            // threads join in finalize); this runs on the main thread.
            if let Err(_err) = unsafe { (*core).performBindingAction(action) } {
                eprintln!("warn(gtk): error performing binding action");
            }
        }

        /// PORTS: `window.zig:713-731` `syncActions`. Enables/disables the `copy`
        /// action based on the active surface's selection.
        pub(super) fn sync_actions(&self) {
            let has_selection = self
                .get_active_surface()
                .and_then(|s| s.core())
                // SAFETY: as in `perform_binding_action`.
                .is_some_and(|core| unsafe { (*core).hasSelection() });

            let obj = self.obj();
            let action_map = obj.upcast_ref::<gio::ActionMap>();
            if let Some(action) = action_map.lookup_action("copy") {
                if let Ok(simple) = action.downcast::<gio::SimpleAction>() {
                    simple.set_enabled(has_selection);
                }
            }
        }

        /// PORTS: `window.zig:760-764` `addToast`. Queues a 3s text toast.
        ///
        /// PORT NOTE (i18n): the reference wraps the title in `i18n._(...)` at each
        /// CALL SITE (`window.zig:1102,1663,1665`), not here — `addToast` itself
        /// takes an already-translated `[*:0]const u8`. rustty matches that: the
        /// callers pass [`crate::gtk::i18n::tr`]`(c"…")`.
        pub(super) fn add_toast(&self, title: &str) {
            let toast = adw::Toast::new(title);
            toast.set_timeout(3);
            self.toast_overlay.add_toast(toast);
        }

        /// PORTS: `window.zig:624-710` `syncAppearance`. Safe to call repeatedly.
        pub(super) fn sync_appearance(&self) {
            let obj = self.obj();

            // Reset the CSD/SSD style classes (`:634-640`).
            for class in ["ssd", "csd", "solid-csd", "no-border-radius"] {
                obj.remove_css_class(class);
            }

            let csd_enabled = self.winproto.borrow().client_side_decoration_enabled();
            obj.set_decorated(csd_enabled);

            if csd_enabled {
                // The same check GTK does internally (`:648-659`).
                let display = obj.upcast_ref::<gtk4::Widget>().display();
                if display.is_rgba() && display.is_composited() {
                    obj.add_css_class("csd");
                } else {
                    obj.add_css_class("solid-csd");
                }
            } else {
                obj.add_css_class("ssd");
                obj.add_css_class("no-border-radius");
            }

            // Trigger the config-dependent dynamic properties (`:668-679`).
            for key in [
                "headerbar-visible",
                "tabs-autohide",
                "tabs-visible",
                "tabs-wide",
                "toolbar-style",
                "titlebar-style",
            ] {
                obj.notify(key);
            }

            // The remainder uses the config (`:682`).
            {
                let cfg = self.config.borrow();
                let Some(config) = cfg.as_ref() else {
                    return;
                };
                let c = config.get();

                // Solid background only if opaque (`:685-688`).
                self.toggle_css_class("background", c.background_opacity >= 1.0);

                // `window-theme-ghostty` only below GTK 4.16 (`:693-697`). `.ghostty`
                // is rustty's `WindowTheme::Rustty`.
                self.toggle_css_class(
                    "window-theme-ghostty",
                    !gtk_version::at_least(4, 16, 0)
                        && matches!(c.window_theme, WindowTheme::Rustty),
                );

                // Move the tab bar to the configured location (`:700-704`).
                let tab_bar_widget = self.tab_bar.upcast_ref::<gtk4::Widget>();
                self.toolbar.remove(tab_bar_widget);
                match c.gtk_tabs_location {
                    GtkTabsLocation::Top => self.toolbar.add_top_bar(tab_bar_widget),
                    GtkTabsLocation::Bottom => self.toolbar.add_bottom_bar(tab_bar_widget),
                }
            }

            // Window-protocol-specific appearance sync (`:706-709`).
            if let Err(_err) = self.winproto.borrow_mut().sync_appearance() {
                eprintln!("warn(gtk): failed to sync winproto appearance");
            }
        }

        /// PORTS: `window.zig:864-891` `handleActiveState` — the debounced
        /// active/inactive reaction (the timeout body).
        pub(super) fn handle_active_state(&self) {
            self.handle_active_state_source.replace(None);
            let obj = self.obj();

            // Hide quick-terminal if set to autohide (`:870-879`).
            if self.quick_terminal.get() {
                let cfg = self.config.borrow();
                if let Some(config) = cfg.as_ref() {
                    if config.get().quick_terminal_autohide && !obj.is_active() && obj.is_visible()
                    {
                        drop(cfg);
                        self.toggle_visibility();
                    }
                }
            }

            // Only reset urgency if we're the active window (`:882-889`).
            if !obj.is_active() {
                return;
            }
            if let Err(_err) = self.winproto.borrow_mut().set_urgent(false) {
                eprintln!("warn(gtk): winproto failed to reset urgency");
            }
        }

        /// PORTS: `window.zig:615-618` `toggleVisibility`.
        pub(super) fn toggle_visibility(&self) {
            let obj = self.obj();
            let visible = obj.is_visible();
            obj.set_visible(!visible);
        }

        /// PORTS: `window.zig:607-612` `toggleTabOverview`.
        pub(super) fn toggle_tab_overview(&self) {
            let is_open = self.tab_overview.is_open();
            self.tab_overview.set_open(!is_open);
        }

        /// PORTS: `window.zig:355-383` `initActionMap`.
        fn init_action_map(&self) {
            use ext::actions::Action;
            let s_type = glib::VariantTy::STRING.to_owned();
            let actions = [
                Action::<super::Window>::init("about", action_about, None),
                Action::init("close", action_close, None),
                Action::init("close-tab", action_close_tab, Some(s_type)),
                Action::init("new-tab", action_new_tab, None),
                Action::init("new-window", action_new_window, None),
                Action::init("prompt-surface-title", action_prompt_surface_title, None),
                Action::init("prompt-tab-title", action_prompt_tab_title, None),
                Action::init(
                    "prompt-context-tab-title",
                    action_prompt_context_tab_title,
                    None,
                ),
                Action::init("ring-bell", action_ring_bell, None),
                Action::init("split-right", action_split_right, None),
                Action::init("split-left", action_split_left, None),
                Action::init("split-up", action_split_up, None),
                Action::init("split-down", action_split_down, None),
                Action::init("copy", action_copy, None),
                Action::init("paste", action_paste, None),
                Action::init("reset", action_reset, None),
                Action::init("clear", action_clear, None),
                Action::init(
                    "toggle-command-palette",
                    action_toggle_command_palette,
                    None,
                ),
                Action::init("toggle-inspector", action_toggle_inspector, None),
            ];
            ext::actions::add(self.obj().as_ref(), &actions);
        }

        // ---- Realize / winproto (window.zig:1301-1338) ---------------------
        pub(super) fn realize(&self) {
            let obj = self.obj();
            let app = Application::default();

            // Initialize our window protocol logic (`window.zig:1305-1314`).
            {
                let app_winproto = app.winproto();
                match winproto::Window::init(&app_winproto, obj.as_ref()) {
                    Ok(wp) => {
                        drop(app_winproto);
                        self.winproto.replace(wp);
                    }
                    Err(_err) => {
                        eprintln!("warn(gtk): failed to initialize window protocol");
                        return;
                    }
                }
            }

            // Resize notifications need the realized GDK surface
            // (`window.zig:1318-1333`).
            if let Some(gdk_surface) = obj.surface() {
                for detail in ["width", "height"] {
                    let obj_weak = obj.downgrade();
                    gdk_surface.connect_notify_local(Some(detail), move |_, _| {
                        if let Some(win) = obj_weak.upgrade() {
                            if let Err(_err) = win.imp().winproto.borrow_mut().resize_event() {
                                eprintln!("warn(gtk): winproto resize event failed");
                            }
                        }
                    });
                }
            }

            // Always sync appearance on realize (`window.zig:1337`).
            self.sync_appearance();
        }

        // ---- Window decoration (window.zig:920-956) ------------------------
        /// PORTS: `window.zig:928-945` `toggleWindowDecorations`.
        pub(super) fn toggle_window_decorations(&self) {
            if self.window_decoration.borrow().is_some() {
                self.set_window_decoration(None);
                return;
            }
            let cfg = self.config.borrow();
            let Some(config) = cfg.as_ref() else {
                return;
            };
            use rustty_config::value_types::WindowDecoration;
            let new = match config.get().window_decoration {
                WindowDecoration::None => WindowDecoration::Auto,
                WindowDecoration::Auto | WindowDecoration::Client | WindowDecoration::Server => {
                    WindowDecoration::None
                }
            };
            drop(cfg);
            self.set_window_decoration(Some(new));
        }

        /// PORTS: `window.zig:949-956` `setWindowDecoration`.
        fn set_window_decoration(&self, new: Option<rustty_config::value_types::WindowDecoration>) {
            self.window_decoration.replace(new);
            self.sync_appearance();
        }

        // ---- Tab creation / surface tree (window.zig:419-507, 766-859) ------
        /// PORTS: `window.zig:419-507` `newTabPage`. Creates a [`Tab`] (hosting a
        /// [`SplitTree`] with its initial [`Surface`]), parents it per `parent` +
        /// `context`, inserts it into `tab_view` at the configured position, binds
        /// the page's `title`/`tooltip`, wires the split-tree `changed` signal, and
        /// selects the page.
        pub(super) fn new_tab_page(
            &self,
            parent: Option<*mut super::CoreSurface>,
            context: NewSurfaceContext,
            overrides: TabOverrides,
        ) -> adw::TabPage {
            let tab_view = &self.tab_view;

            // Create our new tab object (`:434-441`). The same SHARED config is
            // read again below for the insert position (`:452-457`).
            let config = self.config.borrow().clone();
            let tab = Tab::new(config.as_ref(), overrides);

            if let Some(p) = parent {
                // For a new window's first tab, inherit the parent's initial size
                // hints (`:445-448`): `surfaceInit(p.rt_surface.gobj(), self)`.
                if context == NewSurfaceContext::Window {
                    // SAFETY: `p` is a live core surface (the callers' contract,
                    // as in `perform_binding_action`); its `rt_surface` points at
                    // the pinned apprt surface embedded in the parent widget's
                    // GObject private (memory `mirihar-surface-pin`).
                    if let Some(parent_widget) = unsafe { (*(*p).rt_surface).widget() } {
                        self.handle_surface_init(&parent_widget);
                    }
                }
                // `tab.setParentWithContext(p, context)` (`:449`).
                // SAFETY: `p` is a live core surface (callers' contract).
                unsafe { tab.set_parent_with_context(p, context) };
            }

            // Get the position that we should insert the new tab at (`:452-466`).
            let Some(config) = config else {
                // No config: just append at the end. Should never happen (`:453-457`).
                return tab_view.append(&tab);
            };
            let position = match config.get().window_new_tab_position {
                WindowNewTabPosition::Current => match tab_view.selected_page() {
                    Some(selected) => tab_view.page_position(&selected) + 1,
                    None => tab_view.n_pages(),
                },
                WindowNewTabPosition::End => tab_view.n_pages(),
            };

            // Add the page and select it (`:469-470`). NOTE: `insert` fires the
            // bound `page_attached` callback, which connects the tab's
            // `close-request` + the tree's surface handlers a first time; the
            // `changed` replay below re-runs `connect_surface_handlers`, whose
            // pre-disconnect dedupes (the reference's disconnect-matched shape).
            let page = tab_view.insert(&tab, position);
            tab_view.set_selected_page(&page);

            // Create some property bindings (`:473-485`).
            tab.bind_property("title", &page, "title")
                .sync_create()
                .build();
            tab.bind_property("tooltip", &page, "tooltip")
                .sync_create()
                .build();

            // Bind signals (`:487-495`): the split-tree `changed` → `tabSplitTreeChanged`.
            let split_tree = tab.get_split_tree();
            let this_weak = self.obj().downgrade();
            split_tree.connect_local("changed", false, move |args| {
                let old: Option<Tree> = args[1].get().expect("changed old tree is a boxed Tree");
                let new: Option<Tree> = args[2].get().expect("changed new tree is a boxed Tree");
                if let Some(win) = this_weak.upgrade() {
                    win.imp().tab_split_tree_changed(old.as_ref(), new.as_ref());
                }
                None
            });

            // Run an initial notification for the surface tree so we can setup
            // initial state (`:497-504`).
            self.tab_split_tree_changed(None, split_tree.get_tree().as_ref());

            page
        }

        /// PORTS: `window.zig:1775-1788` `tabSplitTreeChanged` — the split-tree
        /// `changed` reaction: drop the old tree's handlers, wire the new tree's.
        pub(super) fn tab_split_tree_changed(
            &self,
            old_tree: Option<&Tree>,
            new_tree: Option<&Tree>,
        ) {
            if let Some(tree) = old_tree {
                self.disconnect_surface_handlers(tree);
            }
            if let Some(tree) = new_tree {
                self.connect_surface_handlers(tree);
            }
        }

        /// PORTS: `window.zig:766-837` `connectSurfaceHandlers`. Iterates the
        /// [`Tree`] (the reference's `Surface.Tree` iterator, `:771-772`), wiring
        /// each surface's window-level signals. Each surface is first
        /// pre-disconnected (`:774-784` `signalHandlersDisconnectMatched`) so we
        /// never stack duplicate handlers; rustty's tracked-pairs mechanism (see
        /// the `surface_handlers` field) replaces the match-by-userdata call.
        pub(super) fn connect_surface_handlers(&self, tree: &Tree) {
            let obj = self.obj();
            for surface in tree_surfaces(tree) {
                // Before adding any new signal handlers, disconnect any that we
                // may have added before (`:773-784`).
                self.disconnect_handlers_for_surface(&surface);

                let mut ids: Vec<glib::SignalHandlerId> = Vec::new();

                // `present-request` → `surfacePresentRequest` (`:786-791`).
                let this_weak = obj.downgrade();
                ids.push(
                    surface.connect_local("present-request", false, move |args| {
                        let s: Surface =
                            args[0].get().expect("present-request emitter is a Surface");
                        if let Some(win) = this_weak.upgrade() {
                            win.imp().surface_present_request(&s);
                        }
                        None
                    }),
                );

                // `clipboard-write` → `surfaceClipboardWrite` (`:793-798`).
                let this_weak = obj.downgrade();
                ids.push(
                    surface.connect_local("clipboard-write", false, move |args| {
                        let clipboard_type: i32 =
                            args[1].get().expect("clipboard-write type is an i32");
                        let text: String = args[2].get().expect("clipboard-write text is a string");
                        if let Some(win) = this_weak.upgrade() {
                            win.imp().surface_clipboard_write(clipboard_type, &text);
                        }
                        None
                    }),
                );

                // `menu` → `surfaceMenu` (`:800-805`).
                let this_weak = obj.downgrade();
                ids.push(surface.connect_local("menu", false, move |_args| {
                    if let Some(win) = this_weak.upgrade() {
                        win.imp().sync_actions();
                    }
                    None
                }));

                // `toggle-fullscreen` → `surfaceToggleFullscreen` (`:807-812`).
                let this_weak = obj.downgrade();
                ids.push(
                    surface.connect_local("toggle-fullscreen", false, move |_args| {
                        if let Some(win) = this_weak.upgrade() {
                            win.imp().surface_toggle_fullscreen();
                        }
                        None
                    }),
                );

                // `toggle-maximize` → `surfaceToggleMaximize` (`:814-819`).
                let this_weak = obj.downgrade();
                ids.push(
                    surface.connect_local("toggle-maximize", false, move |_args| {
                        if let Some(win) = this_weak.upgrade() {
                            win.imp().surface_toggle_maximize();
                        }
                        None
                    }),
                );

                // `init` → `surfaceInit` — only registered while no surface has
                // initialized yet (`:821-833`; the handler re-checks the flag).
                if !self.surface_init.get() {
                    let this_weak = obj.downgrade();
                    ids.push(surface.connect_local("init", false, move |args| {
                        let s: Surface = args[0].get().expect("init emitter is a Surface");
                        if let Some(win) = this_weak.upgrade() {
                            win.imp().handle_surface_init(&s);
                        }
                        None
                    }));
                }

                let mut store = self.surface_handlers.borrow_mut();
                for id in ids {
                    store.push((surface.clone(), id));
                }
            }
        }

        /// PORTS: `window.zig:842-859` `disconnectSurfaceHandlers`. Called whenever
        /// a tree is no longer present in the window (tab detached / tree changed).
        pub(super) fn disconnect_surface_handlers(&self, tree: &Tree) {
            for surface in tree_surfaces(tree) {
                self.disconnect_handlers_for_surface(&surface);
            }
        }

        /// The rustty analogue of one `signalHandlersDisconnectMatched(surface,
        /// .{.data=true}, …, self)` call (`window.zig:775-784,852-857`): drain +
        /// disconnect every tracked handler pair for `surface`.
        fn disconnect_handlers_for_surface(&self, surface: &Surface) {
            let matched: Vec<(Surface, glib::SignalHandlerId)> = {
                let mut store = self.surface_handlers.borrow_mut();
                let mut matched = Vec::new();
                let mut i = 0;
                while i < store.len() {
                    if store[i].0 == *surface {
                        matched.push(store.swap_remove(i));
                    } else {
                        i += 1;
                    }
                }
                matched
            };
            for (s, id) in matched {
                s.disconnect(id);
            }
        }

        // ---- Surface signal handlers (window.zig:1645-1773) -----------------
        /// PORTS: `window.zig:1645-1666` `surfaceClipboardWrite`. Toast on a
        /// standard-clipboard write, when configured.
        fn surface_clipboard_write(&self, clipboard_type: i32, text: &str) {
            // We only toast for the standard clipboard (`:1652`).
            if clipboard_type != Clipboard::Standard as i32 {
                return;
            }
            // We only toast if configured to (`:1654-1660`).
            let title = {
                let cfg = self.config.borrow();
                let Some(config) = cfg.as_ref() else {
                    return;
                };
                if !config.get().app_notifications.clipboard_copy {
                    return;
                }
                // PORTS `:1663-1665`'s `i18n._(...)` lookups (the reference wraps
                // at the CALLER, not inside `addToast`).
                if text.is_empty() {
                    tr(c"Cleared clipboard")
                } else {
                    tr(c"Copied to clipboard")
                }
            };
            self.add_toast(title);
        }

        /// PORTS: `window.zig:1675-1719` `surfacePresentRequest`. Select the
        /// surface's tab, focus it, and raise the window.
        fn surface_present_request(&self, surface: &Surface) {
            let obj = self.obj();

            // Verify that this surface is actually in this window (`:1679-1697`).
            match ext::ancestor::<super::Window>(surface) {
                None => {
                    eprintln!("warn(gtk): present request called for non-existent surface");
                    return;
                }
                Some(win) if win != *obj => {
                    eprintln!("warn(gtk): present request called for surface in different window");
                    return;
                }
                Some(_) => {}
            }

            // Get the tab for this surface (`:1699-1706`).
            let Some(tab) = ext::ancestor::<Tab>(surface) else {
                eprintln!("warn(gtk): present request surface not found");
                return;
            };

            // Get the page that contains this tab and select it (`:1708-1712`).
            let page = self.tab_view.page(&tab);
            self.tab_view.set_selected_page(&page);

            // Grab focus (`:1714-1715`).
            surface.grab_focus();

            // Bring the window to the front (`:1717-1718`).
            obj.present();
        }

        /// PORTS: `window.zig:1722-1734` `surfaceToggleFullscreen`. The change is
        /// reacted to in the bound `propFullscreened` callback.
        fn surface_toggle_fullscreen(&self) {
            let obj = self.obj();
            if obj.is_fullscreen() {
                obj.unfullscreen();
            } else {
                obj.fullscreen();
            }
        }

        /// PORTS: `window.zig:1736-1748` `surfaceToggleMaximize`. The change is
        /// reacted to in the bound `propMaximized` callback.
        fn surface_toggle_maximize(&self) {
            let obj = self.obj();
            if obj.is_maximized() {
                obj.unmaximize();
            } else {
                obj.maximize();
            }
        }

        /// PORTS: `window.zig:1750-1773` `surfaceInit`. On the FIRST surface init,
        /// adopt the surface's default/min size as the window's.
        pub(super) fn handle_surface_init(&self, surface: &Surface) {
            // Make sure we init only once (`:1756-1758`).
            if self.surface_init.get() {
                return;
            }
            self.surface_init.set(true);

            let obj = self.obj();
            // Setup our default and minimum size (`:1760-1772`).
            if let Some(size) = surface.default_size() {
                obj.set_default_size(size.width as i32, size.height as i32);
            }
            if let Some(size) = surface.min_size() {
                obj.set_size_request(size.width as i32, size.height as i32);
            }
        }

        /// PORTS: `window.zig:1610-1617` `tabCloseRequest`. The reference notes:
        /// "TODO: connect close page handler to tab to check for confirmation" —
        /// the confirmation check runs in the bound `close_page` callback.
        pub(super) fn tab_close_request(&self, tab: &Tab) {
            let page = self.tab_view.page(tab);
            self.tab_view.close_page(&page);
        }

        // ---- Command palette / inspector -----------------------------------
        /// PORTS: `window.zig:2007-2049` `toggleCommandPalette`. Lazily build a
        /// [`CommandPalette`] (bind its config, connect its `trigger` signal, save a
        /// WEAK ref), then toggle it modal over this window.
        pub(super) fn toggle_command_palette(&self) {
            let obj = self.obj();

            // Reuse the saved palette if it is still alive, else create one
            // (`window.zig:2013-2045`).
            let palette = self.command_palette.get().unwrap_or_else(|| {
                // Create a fresh command palette (`:2015-2016`).
                let palette = CommandPalette::new();

                // Synchronize our config to the palette's config
                // (`:2018-2025`).
                obj.bind_property("config", &palette, "config")
                    .sync_create()
                    .build();

                // Listen to `trigger` to know when the user selected an option
                // (`:2027-2036`) → `signalCommandPaletteTrigger`
                // (`window.zig:2052-2055`), which performs the binding action.
                let this_weak = obj.downgrade();
                palette.connect_local("trigger", false, move |args| {
                    let boxed: ActionBoxed =
                        args[1].get().expect("trigger action is an ActionBoxed");
                    if let Some(win) = this_weak.upgrade() {
                        win.imp().perform_binding_action(boxed.0);
                    }
                    None
                });

                // Save a WEAK ref to avoid reference-counting cycles
                // (`:2038-2041`).
                self.command_palette.set(Some(&palette));

                palette
            });

            // Tell the palette to toggle itself, modal over our window
            // (`:2047-2048`).
            palette.toggle(&obj);
        }

        /// PORTS: `window.zig:2069-2072` `toggleInspector`. Resolve the active
        /// surface and toggle its terminal inspector window.
        pub(super) fn toggle_inspector(&self) {
            // `const surface = self.getActiveSurface() orelse return;` (`:2070`).
            let Some(surface) = self.get_active_surface() else {
                return;
            };
            // `_ = surface.controlInspector(.toggle);` (`:2071`).
            let _ = surface.control_inspector(rustty_apprt_api::action::Inspector::Toggle);
        }
    }

    /// PORTS: `gobject.BindingGroup` (`window.zig:232`, **invariant 12** OWNED).
    /// glib 0.22 gates its `glib::BindingGroup` binding behind the `v2_72` feature
    /// this build does not enable (the same reason [`super::super::surface`] binds
    /// `g_signal_group_set_target` directly), so this wraps the libgobject-2.0
    /// `GBindingGroup` C API. `GBindingGroup*` is ABI-compatible with `GObject*`.
    #[derive(Debug)]
    pub struct TabBindings {
        ptr: *mut glib::gobject_ffi::GObject,
    }

    impl TabBindings {
        /// PORTS: `gobject.BindingGroup.new` (`window.zig:321`). Owns the returned
        /// (transfer-full) ref.
        pub(super) fn new() -> Self {
            // SAFETY: `g_binding_group_new` returns a new full ref to a live
            // `GBindingGroup` (exists since GLib 2.72; this build links 2.88).
            let ptr = unsafe { g_binding_group_new() };
            Self { ptr }
        }

        /// PORTS: `gobject.BindingGroup.bind` (`window.zig:322`). Binds
        /// `source_property` of the (later-`set_source`d) source to `target_property`
        /// of `target`. `flags` is `GBindingFlags` (0 == default, matching the
        /// reference's `.{}`).
        pub(super) fn bind(
            &self,
            source_property: &str,
            target: &glib::Object,
            target_property: &str,
            flags: u32,
        ) {
            let sp = CString::new(source_property).expect("source property has no NUL");
            let tp = CString::new(target_property).expect("target property has no NUL");
            // SAFETY: `self.ptr` is a live `GBindingGroup`; `target` is a live
            // `GObject`; both C strings are NUL-terminated and outlive the call.
            unsafe {
                g_binding_group_bind(
                    self.ptr,
                    sp.as_ptr(),
                    target.as_ptr().cast(),
                    tp.as_ptr(),
                    flags,
                );
            }
        }

        /// PORTS: `gobject.BindingGroup.setSource` (`window.zig:1274,1506,1515`).
        /// `None` clears the source (the reference's `setSource(null)`).
        pub(super) fn set_source(&self, source: Option<&impl IsA<glib::Object>>) {
            let source_ptr = source.map_or(std::ptr::null_mut(), |s| {
                s.upcast_ref::<glib::Object>().as_ptr()
            });
            // SAFETY: `self.ptr` is a live `GBindingGroup`; `source_ptr` is null or a
            // live `GObject`.
            unsafe { g_binding_group_set_source(self.ptr, source_ptr.cast()) }
        }
    }

    impl Drop for TabBindings {
        /// PORTS: `priv.tab_bindings.unref()` (`window.zig:1289`, finalize).
        fn drop(&mut self) {
            // SAFETY: `self.ptr` is the sole full ref this wrapper owns (from
            // `g_binding_group_new`); dropping releases it exactly once.
            unsafe { glib::gobject_ffi::g_object_unref(self.ptr) }
        }
    }

    extern "C" {
        fn g_binding_group_new() -> *mut glib::gobject_ffi::GObject;
        fn g_binding_group_set_source(
            group: *mut glib::gobject_ffi::GObject,
            source: *mut glib::gobject_ffi::GObject,
        );
        fn g_binding_group_bind(
            group: *mut glib::gobject_ffi::GObject,
            source_property: *const std::os::raw::c_char,
            target: *mut glib::gobject_ffi::GObject,
            target_property: *const std::os::raw::c_char,
            flags: u32,
        );
    }

    /// Snapshot the surfaces of a [`Tree`] — the reference's `tree.iterator()`
    /// walk (`window.zig:771-772,849-850`), collected up front so no tree borrow
    /// is held while handlers connect/disconnect (which may re-enter).
    fn tree_surfaces(tree: &Tree) -> Vec<Surface> {
        let mut out = Vec::new();
        let mut it = tree.0.iterator();
        while let Some(entry) = it.next() {
            // SAFETY: leaf `view` pointers are live tree-owned boxes holding one
            // strong GObject ref each (the `View` impl in `split_tree.rs`).
            out.push(unsafe { &*entry.view }.clone());
        }
        out
    }

    /// Removes a glib source id in `dispose`, warning (not panicking) on failure —
    /// the faithful shape of the reference's `if (glib.Source.remove(id) == 0)
    /// log.warn(...)` (`window.zig:1261-1263`, **invariant 11**). `SourceId`'s own
    /// `Drop` does NOT remove the source, so we remove via raw `g_source_remove`.
    fn remove_source_id(id: Option<glib::SourceId>, what: &str) {
        if let Some(source) = id {
            let removed = unsafe { glib::ffi::g_source_remove(source.as_raw()) };
            if removed == glib::ffi::GFALSE {
                eprintln!(
                    "warn(gtk): unable to remove {what} source={}",
                    source.as_raw()
                );
            }
        }
    }
}

/// PORTS: `super::super::surface::CoreSurface` — the libghostty-core surface the
/// window drives binding actions against (`window.zig:747`).
use super::surface::CoreSurface;

glib::wrapper! {
    /// PORTS: `window.zig:35` `pub const Window` — the `GhosttyWindow`.
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends adw::ApplicationWindow, gtk4::ApplicationWindow, gtk4::Window, gtk4::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk4::Accessible, gtk4::Buildable,
                    gtk4::ConstraintTarget, gtk4::Native, gtk4::Root, gtk4::ShortcutManager;
}

impl Window {
    /// PORTS: `window.zig:272-293` `new`. Constructs over `app`; if a `title`
    /// override is given, sets it immediately so window inspectors see it before
    /// the surface syncs one (`:284-291`).
    #[must_use]
    pub fn new(app: &Application, title: Option<&str>) -> Self {
        let win: Self = glib::Object::builder().property("application", app).build();
        if let Some(title) = title {
            win.set_title(Some(title));
        }
        win
    }

    /// PORTS: `window.zig:386-388` `winproto`. Mutable access to this window's
    /// per-window windowing-protocol state. `pub(crate)` because the only consumer
    /// is [`crate::gtk::class::Surface::default_termio_env`] (`surface.zig:1633`);
    /// the reference returns a raw `*winproto.Window`, which a `RefMut` models
    /// faithfully here.
    pub(crate) fn winproto_mut(&self) -> std::cell::RefMut<'_, winproto::Window> {
        self.imp().winproto.borrow_mut()
    }

    /// PORTS: `window.zig:897-899` `isQuickTerminal`.
    #[must_use]
    pub fn is_quick_terminal(&self) -> bool {
        self.imp().quick_terminal.get()
    }

    /// PORTS: `window.zig:903-906` `getActiveSurface`. Does not add a ref.
    #[must_use]
    pub fn get_active_surface(&self) -> Option<Surface> {
        self.imp().get_active_surface()
    }

    /// PORTS: `window.zig:910-912` `getConfig`. Does not increase the ref count.
    #[must_use]
    pub fn get_config(&self) -> Option<Config> {
        self.imp().config.borrow().clone()
    }

    /// PORTS: `window.zig:915-917` `getTabView`.
    #[must_use]
    pub fn get_tab_view(&self) -> adw::TabView {
        self.imp().tab_view.get()
    }

    /// PORTS: `window.zig:919-925` `getWindowDecoration`.
    #[must_use]
    pub fn get_window_decoration(&self) -> rustty_config::value_types::WindowDecoration {
        let imp = self.imp();
        if let Some(v) = *imp.window_decoration.borrow() {
            return v;
        }
        let cfg = imp.config.borrow();
        cfg.as_ref()
            .map_or(rustty_config::value_types::WindowDecoration::Auto, |c| {
                c.get().window_decoration
            })
    }

    /// PORTS: `window.zig:928-945` `toggleWindowDecorations`.
    pub fn toggle_window_decorations(&self) {
        self.imp().toggle_window_decorations();
    }

    /// PORTS: `window.zig:393-395` `newTab`. Create a new tab with the given
    /// parent, inserted at the `window-new-tab-position` and selected.
    pub fn new_tab(&self, parent: Option<*mut CoreSurface>) {
        let _ = self
            .imp()
            .new_tab_page(parent, NewSurfaceContext::Tab, TabOverrides::default());
    }

    /// PORTS: `window.zig:397-417` `newTabForWindow`. The first tab of a new
    /// window (`.window` context: inherits the parent's initial size hints and
    /// window-context working-directory inheritance).
    pub fn new_tab_for_window(&self, parent: Option<*mut CoreSurface>, overrides: TabOverrides) {
        let _ = self
            .imp()
            .new_tab_page(parent, NewSurfaceContext::Window, overrides);
    }

    /// PORTS: `window.zig:607-612` `toggleTabOverview`.
    pub fn toggle_tab_overview(&self) {
        self.imp().toggle_tab_overview();
    }

    /// PORTS: `window.zig:615-618` `toggleVisibility`.
    pub fn toggle_visibility(&self) {
        self.imp().toggle_visibility();
    }

    /// PORTS: `window.zig:518-563` `selectTab`. Select the tab as requested.
    /// Returns true if the tab selection changed.
    pub fn select_tab(&self, n: SelectTab) -> bool {
        let tab_view = self.imp().tab_view.get();

        // Get our current tab numeric position (`:521-523`).
        let Some(selected) = tab_view.selected_page() else {
            return false;
        };
        let current = tab_view.page_position(&selected);

        // Get our total (`:526`).
        let total = tab_view.n_pages();

        let goto: i32 = match n {
            SelectTab::Previous => {
                if current > 0 {
                    current - 1
                } else {
                    total - 1
                }
            }
            SelectTab::Next => {
                if current < total - 1 {
                    current + 1
                } else {
                    0
                }
            }
            SelectTab::Last => total - 1,
            SelectTab::N(v) => {
                // 1-indexed (`:547-556`).
                if v == 0 {
                    return false;
                }
                let Ok(n_int) = i32::try_from(v) else {
                    return false;
                };
                (n_int - 1).min(total - 1)
            }
        };
        debug_assert!(goto >= 0);
        debug_assert!(goto < total);

        // If our target is the same as our current then we do nothing (`:561`).
        if goto == current {
            return false;
        }

        // Get the page and select it (`:563-566`).
        let page = tab_view.nth_page(goto);
        tab_view.set_selected_page(&page);

        true
    }

    /// PORTS: `window.zig:568-605` `moveTab`. Move the tab containing the given
    /// surface by the given amount. Returns whether this affected any tab
    /// positioning.
    pub fn move_tab(&self, surface: &Surface, amount: isize) -> bool {
        let tab_view = self.imp().tab_view.get();

        // If we have one tab we never move (`:576-577`).
        let total = tab_view.n_pages();
        if total == 1 {
            return false;
        }

        // Get the tab that contains the given surface (`:580-583`).
        let Some(tab) = ext::ancestor::<Tab>(surface) else {
            return false;
        };

        // Get the page position that contains the tab (`:586-587`).
        let page = tab_view.page(&tab);
        let pos = tab_view.page_position(&page);

        // Move it (`:590-602`).
        let desired_pos: i32 = {
            let initial = pos + amount as i32;
            let max = total - 1;
            if initial < 0 {
                max + initial + 1
            } else if initial > max {
                initial - max - 1
            } else {
                initial
            }
        };
        debug_assert!(desired_pos >= 0);
        debug_assert!(desired_pos < total);

        tab_view.reorder_page(&page, desired_pos)
    }
}

/// PORTS: `window.zig:508-513` `Window.SelectTab`. The tab to select in
/// [`Window::select_tab`].
#[derive(Clone, Copy, Debug)]
pub enum SelectTab {
    Previous,
    Next,
    Last,
    /// 1-indexed tab number.
    N(usize),
}

// -- Template callbacks (window.zig:2132-2152 bindTemplateCallback) ----------
#[gtk4::template_callbacks]
impl Window {
    /// PORTS: `window.zig:1301-1338` `windowRealize`.
    #[template_callback(name = "realize")]
    fn on_realize(_widget: &gtk4::Widget, this: &Window) {
        this.imp().realize();
    }

    /// PORTS: `window.zig:1340-1342` `btnNewTab`.
    #[template_callback(name = "new_tab")]
    fn on_new_tab(_button: &adw::SplitButton, this: &Window) {
        this.imp().perform_binding_action(binding::Action::NewTab);
    }

    /// PORTS: `window.zig:1344-1349` `tabOverviewCreateTab`. Creates a fresh tab
    /// page for the overview's "new tab" button — `newTabPage(active.core(),
    /// .tab, .none)`.
    #[template_callback(name = "overview_create_tab")]
    fn on_overview_create_tab(_overview: &adw::TabOverview, this: &Window) -> adw::TabPage {
        let parent = this.get_active_surface().and_then(|s| s.core());
        this.imp()
            .new_tab_page(parent, NewSurfaceContext::Tab, TabOverrides::default())
    }

    /// PORTS: `window.zig:1351-1381` `tabOverviewOpen`. On overview CLOSE, arm a
    /// 500ms timer to refocus the selected tab (libadwaita focus-loss workaround).
    #[template_callback(name = "overview_notify_open")]
    fn on_overview_notify_open(
        overview: &adw::TabOverview,
        _pspec: &glib::ParamSpec,
        this: &Window,
    ) {
        // We only care about close (`:1356-1357`).
        if overview.is_open() {
            return;
        }
        let imp = this.imp();
        // Remove any old timer (`:1370-1373`).
        if let Some(old) = imp.tab_overview_focus_timer.take() {
            let _ = unsafe { glib::ffi::g_source_remove(old.as_raw()) };
        }
        // Restart the 500ms timer (`:1375-1380`).
        let this_weak = this.downgrade();
        let source = glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            if let Some(win) = this_weak.upgrade() {
                let imp = win.imp();
                imp.tab_overview_focus_timer.replace(None);
                if let Some(surface) = imp.get_active_surface() {
                    surface.grab_focus();
                }
            }
            glib::ControlFlow::Break
        });
        imp.tab_overview_focus_timer.replace(Some(source));
    }

    /// PORTS: `window.zig:1400-1422` `windowCloseRequest`.
    #[template_callback(name = "close_request")]
    fn on_close_request(_window: &gtk4::Window, this: &Window) -> glib::Propagation {
        if this.imp().get_needs_confirm_quit() {
            // Show a confirmation dialog (`window.zig:1404-1417`). The
            // `close-request` signal → `closeConfirmationClose` (`:1424-1429`),
            // which destroys the window.
            let dialog = CloseConfirmationDialog::new(Target::Window);
            let this_weak = this.downgrade();
            dialog.connect_local("close-request", false, move |_args| {
                if let Some(win) = this_weak.upgrade() {
                    win.destroy();
                }
                None
            });
            // Present keeps the dialog referenced by GTK until dismissed.
            dialog.present(Some(this.upcast_ref::<gtk4::Widget>()));
            return glib::Propagation::Stop;
        }
        this.destroy();
        glib::Propagation::Proceed
    }

    /// PORTS: `window.zig:1459-1496` `tabViewClosePage`.
    #[template_callback(name = "close_page")]
    fn on_close_page(
        _tab_view: &adw::TabView,
        page: &adw::TabPage,
        this: &Window,
    ) -> glib::Propagation {
        let imp = this.imp();
        let Ok(tab) = page.child().downcast::<Tab>() else {
            return glib::Propagation::Proceed;
        };
        // Close immediately if no confirmation is needed (`:1471-1474`).
        if !tab.get_needs_confirm_quit() {
            imp.tab_view.close_page_finish(page, true);
            return glib::Propagation::Stop;
        }

        // Show a confirmation dialog (`window.zig:1476-1494`). `close-request` →
        // `closeConfirmationCloseTab` and `cancel` → `closeConfirmationCancelTab`
        // (`:1431-1457`) — both call `closePageFinish(page, …)`. The reference
        // re-resolves the tab_view via `getAncestor(page.child())`; we already
        // have it as `imp.tab_view`, so we clone it into the closures.
        let dialog = CloseConfirmationDialog::new(Target::Tab);
        let tab_view = imp.tab_view.get();
        let page_close = page.clone();
        let tab_view_close = tab_view.clone();
        dialog.connect_local("close-request", false, move |_args| {
            tab_view_close.close_page_finish(&page_close, true);
            None
        });
        let page_cancel = page.clone();
        dialog.connect_local("cancel", false, move |_args| {
            tab_view.close_page_finish(&page_cancel, false);
            None
        });
        // Present keeps the dialog referenced by GTK until dismissed.
        dialog.present(Some(&page.child()));
        glib::Propagation::Stop
    }

    /// PORTS: `window.zig:1522-1563` `tabViewPageAttached`. Connects the tab's
    /// `close-request` + its surface tree's window-level signals.
    #[template_callback(name = "page_attached")]
    fn on_page_attached(
        _tab_view: &adw::TabView,
        page: &adw::TabPage,
        _position: i32,
        this: &Window,
    ) {
        let imp = this.imp();
        // The attached page must be a Tab object (`:1529-1530`).
        let Ok(tab) = page.child().downcast::<Tab>() else {
            return;
        };

        // Attach listeners for the tab (`:1532-1539`): `close-request` →
        // `tabCloseRequest`, tracked as a `(tab, id)` pair (see `tab_handlers`).
        let this_weak = this.downgrade();
        let id = tab.connect_local("close-request", false, move |args| {
            let t: Tab = args[0].get().expect("close-request emitter is a Tab");
            if let Some(win) = this_weak.upgrade() {
                win.imp().tab_close_request(&t);
            }
            None
        });
        imp.tab_handlers.borrow_mut().push((tab.clone(), id));

        // Attach listeners for the surfaces (`:1541-1562`).
        if let Some(tree) = tab.get_surface_tree() {
            imp.connect_surface_handlers(&tree);
        }
    }

    /// PORTS: `window.zig:1565-1588` `tabViewPageDetached`. Disconnects the tab's
    /// handlers + its surface tree's window-level handlers.
    #[template_callback(name = "page_detached")]
    fn on_page_detached(
        _tab_view: &adw::TabView,
        page: &adw::TabPage,
        _position: i32,
        this: &Window,
    ) {
        let imp = this.imp();
        // We need the tab to disconnect the signals (`:1571-1573`).
        let Ok(tab) = page.child().downcast::<Tab>() else {
            return;
        };

        // `signalHandlersDisconnectMatched(tab, .{.data=true}, …, self)`
        // (`:1574-1582`) — drain + disconnect the tracked pairs for this tab.
        let matched: Vec<(Tab, glib::SignalHandlerId)> = {
            let mut store = imp.tab_handlers.borrow_mut();
            let mut matched = Vec::new();
            let mut i = 0;
            while i < store.len() {
                if store[i].0 == tab {
                    matched.push(store.swap_remove(i));
                } else {
                    i += 1;
                }
            }
            matched
        };
        for (t, id) in matched {
            t.disconnect(id);
        }

        // Remove the tree handlers (`:1584-1587`).
        if let Some(tree) = tab.get_surface_tree() {
            imp.disconnect_surface_handlers(&tree);
        }
    }

    /// PORTS: `window.zig:1637-1643` `setupTabMenu`. Records the page the context
    /// menu was opened for.
    #[template_callback(name = "setup_tab_menu")]
    fn on_setup_tab_menu(_tab_view: &adw::TabView, page: Option<&adw::TabPage>, this: &Window) {
        this.imp().context_menu_page.replace(page.cloned());
    }

    /// PORTS: `window.zig:1590-1607` `tabViewCreateWindow`. Opens a fresh window
    /// (no new tab) and hands back its tab view for the dragged-out page.
    #[template_callback(name = "tab_create_window")]
    fn on_tab_create_window(_tab_view: &adw::TabView, _this: &Window) -> adw::TabView {
        let win = Window::new(&Application::default(), None);
        win.present();
        win.imp().tab_view.get()
    }

    /// PORTS: `window.zig:1619-1636` `tabViewNPages`. Close the window when the
    /// last page is gone (unless the overview is open).
    #[template_callback(name = "notify_n_pages")]
    fn on_notify_n_pages(_tab_view: &adw::TabView, _pspec: &glib::ParamSpec, this: &Window) {
        let imp = this.imp();
        if imp.tab_view.n_pages() == 0 {
            if imp.tab_overview.is_open() {
                return;
            }
            this.close();
        }
    }

    /// PORTS: `window.zig:1498-1520` `tabViewSelectedPage`. Rebind the tab
    /// BindingGroup source to the newly selected tab.
    #[template_callback(name = "notify_selected_page")]
    fn on_notify_selected_page(_tab_view: &adw::TabView, _pspec: &glib::ParamSpec, this: &Window) {
        let imp = this.imp();
        // Always reset in case there are no pages (`:1506`).
        if let Some(bindings) = imp.tab_bindings.borrow().as_ref() {
            bindings.set_source(None::<&glib::Object>);
        }
        let Some(page) = imp.tab_view.selected_page() else {
            return;
        };
        let child = page.child();
        if let Some(bindings) = imp.tab_bindings.borrow().as_ref() {
            bindings.set_source(Some(child.upcast_ref::<glib::Object>()));
        }
        // Unmark needs-attention now that it's selected (`:1519`).
        page.set_needs_attention(false);
    }

    /// PORTS: `window.zig:1093-1107` `propConfig`. Toast on config reload, then
    /// resync appearance.
    #[template_callback(name = "notify_config")]
    fn on_notify_config(_window: &adw::ApplicationWindow, _pspec: &glib::ParamSpec, this: &Window) {
        let imp = this.imp();
        {
            let cfg = imp.config.borrow();
            if let Some(config) = cfg.as_ref() {
                if config.get().app_notifications.config_reload {
                    drop(cfg);
                    // PORTS `window.zig:1102` `addToast(i18n._("Reloaded the configuration"))`.
                    imp.add_toast(tr(c"Reloaded the configuration"));
                }
            }
        }
        imp.sync_appearance();
    }

    /// PORTS: `window.zig:1159-1165` `propFullscreened`.
    #[template_callback(name = "notify_fullscreened")]
    fn on_notify_fullscreened(
        _window: &adw::ApplicationWindow,
        _pspec: &glib::ParamSpec,
        this: &Window,
    ) {
        this.imp().sync_appearance();
    }

    /// PORTS: `window.zig:1109-1142` `propIsActive`. Debounce active-state changes
    /// with a 50ms idle-priority timeout.
    #[template_callback(name = "notify_is_active")]
    fn on_notify_is_active(_window: &gtk4::Window, _pspec: &glib::ParamSpec, this: &Window) {
        let imp = this.imp();
        if imp.handle_active_state_source.borrow().is_some() {
            return;
        }
        let this_weak = this.downgrade();
        let source = glib::timeout_add_local_full(
            std::time::Duration::from_millis(50),
            glib::Priority::DEFAULT_IDLE,
            move || {
                if let Some(win) = this_weak.upgrade() {
                    win.imp().handle_active_state();
                }
                glib::ControlFlow::Break
            },
        );
        imp.handle_active_state_source.replace(Some(source));
    }

    /// PORTS: `window.zig:1167-1173` `propMaximized`.
    #[template_callback(name = "notify_maximized")]
    fn on_notify_maximized(
        _window: &adw::ApplicationWindow,
        _pspec: &glib::ParamSpec,
        this: &Window,
    ) {
        this.imp().sync_appearance();
    }

    /// PORTS: `window.zig:1175-1189` `propMenuActive`. On menu activation, re-check
    /// action validity.
    #[template_callback(name = "notify_menu_active")]
    fn on_notify_menu_active(button: &gtk4::MenuButton, _pspec: &glib::ParamSpec, this: &Window) {
        // Baseline is >= 4.14, so the reference's 4.10 guard is always satisfied
        // (`window.zig:1181`).
        if !gtk_version::at_least(4, 10, 0) {
            return;
        }
        if !button.is_active() {
            return;
        }
        this.imp().sync_actions();
    }

    /// PORTS: `window.zig:1191-1209` `propQuickTerminal`. Initialize the quick
    /// terminal at the app layer when the property turns on.
    #[template_callback(name = "notify_quick_terminal")]
    fn on_notify_quick_terminal(
        _window: &adw::ApplicationWindow,
        _pspec: &glib::ParamSpec,
        this: &Window,
    ) {
        let imp = this.imp();
        if imp.surface_init.get() {
            eprintln!(
                "warn(gtk): quick terminal property can't be changed after surfaces initialized"
            );
            return;
        }
        if imp.quick_terminal.get() {
            if let Err(_err) = Application::default()
                .winproto_mut()
                .init_quick_terminal(this)
            {
                eprintln!("warn(gtk): failed to initialize quick terminal");
            }
        }
    }

    /// PORTS: `window.zig:1211-1227` `propScaleFactor`. Refresh winproto appearance
    /// on scale-factor changes (X11 needs it).
    #[template_callback(name = "notify_scale_factor")]
    fn on_notify_scale_factor(
        _window: &adw::ApplicationWindow,
        _pspec: &glib::ParamSpec,
        this: &Window,
    ) {
        if let Err(_err) = this.imp().winproto.borrow_mut().sync_appearance() {
            eprintln!("warn(gtk): failed to sync appearance after scale factor update");
        }
    }

    /// PORTS: `window.zig:1229-1237` `closureTitlebarStyleIsTab`. Closure: `this`
    /// FIRST (the GtkBuilder expression scope), then the titlebar-style value.
    #[template_callback(name = "titlebar_style_is_tabs")]
    fn closure_titlebar_style_is_tabs(_this: &Window, value: i32) -> bool {
        value == TITLEBAR_STYLE_TABS
    }

    /// PORTS: `window.zig:1239-1252` `closureSubtitle`. Closure: `this` FIRST, then
    /// the config + the active surface pwd. Returns the working-directory subtitle
    /// when configured, else `None`.
    #[template_callback(name = "computed_subtitle")]
    fn closure_computed_subtitle(
        _this: &Window,
        config: Option<Config>,
        pwd: Option<String>,
    ) -> Option<String> {
        let config = config?;
        match config.get().window_subtitle {
            WindowSubtitle::False => None,
            WindowSubtitle::WorkingDirectory => pwd,
        }
    }
}

// -- GTK action handlers (window.zig:1790-2083) ------------------------------
// Each receives the target `&Window` as ext::actions delivers it (invariant 10).

/// PORTS: `window.zig:1790-1832` `actionAbout`.
fn action_about(_action: &gio::SimpleAction, _param: Option<&glib::Variant>, this: &Window) {
    // AD-3: `adw_version.supportsDialogs()` collapses to the modern `adw` arm.
    // PORT NOTE (R2): name/icon/developer/version are sourced from
    // `rustty_identity`, NOT the reference's hardcoded "Ghostty" /
    // "Ghostty Developers" / `build_config.version_string`. The reference's
    // `website` ("https://ghostty.org") and `issue-url` (the ghostty GitHub
    // issues page) are DROPPED: `rustty_identity` carries no homepage/issue-url
    // constant, and emitting ghostty.org would leak the upstream identity into a
    // user-visible shipped dialog (R2 forbids this).
    // PORTS: `window.zig:1805` `i18n._("Ghostty Developers")`. R2 + gettext
    // interaction: a msgid is a lookup KEY, so this must be the literal the 31
    // catalogs carry (`po/io.airely.rustty.pot` `msgid "Rustty Developers"`) — the
    // `format!()`-composed string this used to build could never match, so the
    // credit line was permanently untranslatable. The `debug_assert` pins the
    // literal to the identity constant so the two cannot silently diverge. It is
    // scoped to the rustty profile by `PROFILE` (not by a feature cfg apprt does
    // not declare): under `identity-oracle` `APP_NAME_TITLECASE` is the
    // reference's `Ghostty` while the shipped catalogs stay rustty's, so no
    // agreement is expected there.
    if matches!(rustty_identity::PROFILE, rustty_identity::Profile::Rustty) {
        debug_assert_eq!(
            format!("{} Developers", rustty_identity::APP_NAME_TITLECASE),
            "Rustty Developers",
            "the `Rustty Developers` gettext msgid must stay in sync with \
             rustty_identity::APP_NAME_TITLECASE; update po/*.po together with it"
        );
    }
    let developer_name = tr(c"Rustty Developers");
    let dialog = adw::AboutDialog::builder()
        .application_name(rustty_identity::APP_NAME_TITLECASE)
        .application_icon(rustty_identity::BUNDLE_ID)
        .developer_name(developer_name)
        .version(rustty_identity::VERSION)
        .build();
    dialog.present(Some(this.upcast_ref::<gtk4::Widget>()));
}

/// PORTS: `window.zig:1834-1840` `actionClose`.
fn action_close(_action: &gio::SimpleAction, _param: Option<&glib::Variant>, this: &Window) {
    this.close();
}

/// PORTS: `window.zig:1842-1869` `actionCloseTab`.
fn action_close_tab(_action: &gio::SimpleAction, param: Option<&glib::Variant>, this: &Window) {
    let Some(param) = param else {
        eprintln!("warn(gtk): win.close-tab called without a parameter");
        return;
    };
    let Some(mode_str) = param.get::<String>() else {
        eprintln!("warn(gtk): invalid mode provided to win.close-tab");
        return;
    };
    // `std.meta.stringToEnum(CloseTabMode, ...)` (`:1855-1866`) — the reference's
    // `CloseTabMode::string_to_enum` is private, so match the tag names 1:1.
    let mode = match mode_str.as_str() {
        "this" => binding::CloseTabMode::This,
        "other" => binding::CloseTabMode::Other,
        "right" => binding::CloseTabMode::Right,
        other => {
            eprintln!("warn(gtk): invalid mode provided to win.close-tab: {other}");
            return;
        }
    };
    this.imp()
        .perform_binding_action(binding::Action::CloseTab(mode));
}

/// PORTS: `window.zig:1871-1877` `actionNewWindow`.
fn action_new_window(_action: &gio::SimpleAction, _param: Option<&glib::Variant>, this: &Window) {
    this.imp()
        .perform_binding_action(binding::Action::NewWindow);
}

/// PORTS: `window.zig:1879-1885` `actionNewTab`.
fn action_new_tab(_action: &gio::SimpleAction, _param: Option<&glib::Variant>, this: &Window) {
    this.imp().perform_binding_action(binding::Action::NewTab);
}

/// PORTS: `window.zig:1887-1897` `actionPromptContextTabTitle`.
fn action_prompt_context_tab_title(
    _action: &gio::SimpleAction,
    _param: Option<&glib::Variant>,
    this: &Window,
) {
    let imp = this.imp();
    let page = imp.context_menu_page.borrow().clone();
    let Some(page) = page else {
        return;
    };
    let Ok(tab) = page.child().downcast::<Tab>() else {
        return;
    };
    // PORTS: `window.zig:1896` `tab.promptTabTitle()`.
    tab.prompt_tab_title();
}

/// PORTS: `window.zig:1899-1905` `actionPromptSurfaceTitle`.
fn action_prompt_surface_title(
    _action: &gio::SimpleAction,
    _param: Option<&glib::Variant>,
    this: &Window,
) {
    this.imp()
        .perform_binding_action(binding::Action::PromptSurfaceTitle);
}

/// PORTS: `window.zig:1907-1913` `actionPromptTabTitle`.
fn action_prompt_tab_title(
    _action: &gio::SimpleAction,
    _param: Option<&glib::Variant>,
    this: &Window,
) {
    this.imp()
        .perform_binding_action(binding::Action::PromptTabTitle);
}

/// PORTS: `window.zig:1915-1921` `actionSplitRight`.
fn action_split_right(_action: &gio::SimpleAction, _param: Option<&glib::Variant>, this: &Window) {
    this.imp()
        .perform_binding_action(binding::Action::NewSplit(binding::SplitDirection::Right));
}

/// PORTS: `window.zig:1923-1929` `actionSplitLeft`.
fn action_split_left(_action: &gio::SimpleAction, _param: Option<&glib::Variant>, this: &Window) {
    this.imp()
        .perform_binding_action(binding::Action::NewSplit(binding::SplitDirection::Left));
}

/// PORTS: `window.zig:1931-1937` `actionSplitUp`.
fn action_split_up(_action: &gio::SimpleAction, _param: Option<&glib::Variant>, this: &Window) {
    this.imp()
        .perform_binding_action(binding::Action::NewSplit(binding::SplitDirection::Up));
}

/// PORTS: `window.zig:1939-1945` `actionSplitDown`.
fn action_split_down(_action: &gio::SimpleAction, _param: Option<&glib::Variant>, this: &Window) {
    this.imp()
        .perform_binding_action(binding::Action::NewSplit(binding::SplitDirection::Down));
}

/// PORTS: `window.zig:1947-1953` `actionCopy`.
fn action_copy(_action: &gio::SimpleAction, _param: Option<&glib::Variant>, this: &Window) {
    this.imp()
        .perform_binding_action(binding::Action::CopyToClipboard(
            binding::CopyToClipboard::Mixed,
        ));
}

/// PORTS: `window.zig:1955-1961` `actionPaste`.
fn action_paste(_action: &gio::SimpleAction, _param: Option<&glib::Variant>, this: &Window) {
    this.imp()
        .perform_binding_action(binding::Action::PasteFromClipboard);
}

/// PORTS: `window.zig:1963-1969` `actionReset`.
fn action_reset(_action: &gio::SimpleAction, _param: Option<&glib::Variant>, this: &Window) {
    this.imp().perform_binding_action(binding::Action::Reset);
}

/// PORTS: `window.zig:1971-1977` `actionClear`.
fn action_clear(_action: &gio::SimpleAction, _param: Option<&glib::Variant>, this: &Window) {
    this.imp()
        .perform_binding_action(binding::Action::ClearScreen);
}

/// PORTS: `window.zig:1979-2004` `actionRingBell`.
fn action_ring_bell(_action: &gio::SimpleAction, _param: Option<&glib::Variant>, this: &Window) {
    let imp = this.imp();
    let cfg = imp.config.borrow();
    let Some(config) = cfg.as_ref() else {
        return;
    };
    let c = config.get();

    // System bell (`:1987-1993`).
    if c.bell_features.system {
        if let Some(native) = this.surface() {
            native.beep();
        } else {
            eprintln!("warn(gtk): unable to get native surface from window");
        }
    }

    // Attention / urgency (`:1995-2003`).
    if c.bell_features.attention && !this.is_active() {
        drop(cfg);
        if let Err(_err) = imp.winproto.borrow_mut().set_urgent(true) {
            eprintln!("warn(gtk): winproto failed to set urgency");
        }
    }
}

/// PORTS: `window.zig:2058-2066` `actionToggleCommandPalette`.
fn action_toggle_command_palette(
    _action: &gio::SimpleAction,
    _param: Option<&glib::Variant>,
    this: &Window,
) {
    this.imp().toggle_command_palette();
}

/// PORTS: `window.zig:2075-2083` `actionToggleInspector`.
fn action_toggle_inspector(
    _action: &gio::SimpleAction,
    _param: Option<&glib::Variant>,
    this: &Window,
) {
    this.imp().toggle_inspector();
}

#[cfg(test)]
mod tests {
    use super::*;
    use glib::translate::ToGlibPtr;
    use std::ffi::c_void;

    /// Build a process-global default [`Application`] the way the app-lifecycle
    /// KAT does (`tests/gtk_application.rs`): opaque non-null `rt_app`/`core_app`
    /// placeholders (never dereferenced by `new`), then install as the default so
    /// [`Window`]'s `constructed` can resolve `Application::default()`.
    fn bootstrap_app() -> Application {
        let mut rt_app_slot: u8 = 0;
        let mut core_app_slot: u8 = 0;
        let rt_app = std::ptr::addr_of_mut!(rt_app_slot).cast::<c_void>();
        let core_app = std::ptr::addr_of_mut!(core_app_slot).cast::<c_void>();
        let app = Application::new(rt_app, core_app);
        app.upcast_ref::<gio::Application>().set_default();
        app
    }

    /// PORTS: the rustty live KAT for `window.zig` — the window chrome opens.
    ///
    /// Instantiates a [`Window`] over the [`Application`], asserts the five
    /// template children bound (`window.zig:263-267`), and that `present()` maps
    /// the window. Display-dependent: run individually under the real display —
    ///   xvfb-run -a cargo test -p rustty-apprt --features gtk \
    ///     window_template_binds_and_presents -- --ignored --exact
    #[test]
    #[ignore = "requires a display + a single GTK main thread; run with --exact"]
    fn window_template_binds_and_presents() {
        let app = bootstrap_app();
        let window = Window::new(&app, Some("rustty"));

        // The five template children resolved from `ui/1.5/window.ui`
        // (`window.zig:263-267`). `TemplateChild::get` panics if unbound, so a
        // successful `.get()` witnesses the binding.
        let imp = window.imp();
        let _tab_overview: adw::TabOverview = imp.tab_overview.get();
        let _tab_bar: adw::TabBar = imp.tab_bar.get();
        let _tab_view: adw::TabView = imp.tab_view.get();
        let _toolbar: adw::ToolbarView = imp.toolbar.get();
        let _toast_overlay: adw::ToastOverlay = imp.toast_overlay.get();

        // The title override is applied immediately (`window.zig:284-291`).
        assert_eq!(
            window.title().map(|s| s.to_string()).as_deref(),
            Some("rustty"),
            "the title override must be set before present"
        );

        // Present and pump the loop until the window maps.
        window.present();
        let context = glib::MainContext::default();
        for _ in 0..1000 {
            if window.is_mapped() {
                break;
            }
            context.iteration(false);
        }
        assert!(window.is_mapped(), "present() must map the window");

        window.destroy();
        while context.iteration(false) {}
        app.deinit();
    }

    /// PORTS: the invariant-11 KAT — `dispose` removes the window's glib source
    /// ids (`window.zig:1260-1265` + the rustty overview-timer soundness
    /// divergence). Arms both `handle_active_state_source` (via `notify::is-active`)
    /// and `tab_overview_focus_timer` (via the overview close), then disposes and
    /// asserts both are cleared. Run individually under the real display —
    ///   xvfb-run -a cargo test -p rustty-apprt --features gtk \
    ///     window_dispose_removes_source_ids -- --ignored --exact
    #[test]
    #[ignore = "requires a display + a single GTK main thread; run with --exact"]
    fn window_dispose_removes_source_ids() {
        let app = bootstrap_app();
        // Build the window WITHOUT associating it with the application: this KAT
        // finalizes the window directly (drop → `dispose` → `finalize`), and a
        // window registered with an app that never emitted `startup` trips GTK's
        // toplevel bookkeeping on teardown. `constructed` still resolves config from
        // `Application::default()` (installed by `bootstrap_app`), so the window is
        // fully built; it just isn't an app-window member.
        let window: Window = glib::Object::new();

        // Arm `handle_active_state_source`: `notify::is-active` fires the bound
        // `propIsActive` template callback, which schedules the 50ms timeout.
        window.notify("is-active");
        let active_raw = window
            .imp()
            .handle_active_state_source
            .borrow()
            .as_ref()
            .expect("notify::is-active must arm the handle-active-state source")
            .as_raw();

        // Arm `tab_overview_focus_timer`: `tabOverviewOpen` fires on `notify::open`
        // and — since the overview is closed (`is_open() == false`) — schedules the
        // 500ms focus timer. `notify` drives the same bound callback a real
        // open→close transition would (a `set_open` dance no-ops on an unmapped
        // window, which is why we notify directly). We do NOT retain a strong ref to
        // the overview child: its template bindings ref the window, so keeping it
        // alive past `drop(window)` would pin the window and prevent finalize.
        window.imp().tab_overview.notify("open");
        let overview_raw = window
            .imp()
            .tab_overview_focus_timer
            .borrow()
            .as_ref()
            .expect("closing the overview must arm the focus timer")
            .as_raw();

        // Both timer sources are live in the default context before dispose.
        let ctx = glib::MainContext::default();
        assert!(
            !unsafe {
                glib::ffi::g_main_context_find_source_by_id(ctx.to_glib_none().0, active_raw)
            }
            .is_null(),
            "the handle-active-state source must be registered before dispose"
        );

        // Finalize the window so `dispose` runs (**invariant 11**): `destroy` drops
        // GTK4's toplevel self-ref and `drop` drops ours, taking the refcount to
        // zero → `dispose` (removes both source ids) → `finalize`. `destroy` first
        // performs the proper GtkWindow teardown, so this is sound (unlike a bare
        // `g_object_run_dispose`, which corrupts GTK's bookkeeping).
        window.destroy();
        drop(window);
        while ctx.iteration(false) {}

        // After dispose, neither source id is registered any longer — dispose
        // removed them (had it not, the pending timeouts would still be found).
        assert!(
            unsafe {
                glib::ffi::g_main_context_find_source_by_id(ctx.to_glib_none().0, active_raw)
            }
            .is_null(),
            "dispose must remove handle_active_state_source (invariant 11)"
        );
        assert!(
            unsafe {
                glib::ffi::g_main_context_find_source_by_id(ctx.to_glib_none().0, overview_raw)
            }
            .is_null(),
            "dispose must remove tab_overview_focus_timer (invariant 11)"
        );

        app.deinit();
    }
}
