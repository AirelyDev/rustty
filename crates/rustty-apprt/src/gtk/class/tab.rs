//! rustty-apprt::gtk::class::tab
//!
//! PORTS: ghostty `src/apprt/gtk/class/tab.zig` (`GhosttyTab`).
//!
//! A single tab: a `gtk.Box` hosting a [`SplitTree`]. PREPORT_GTK §1.4
//! lifetimes: `config` SHARED (`RefCell<Option<Config>>`, unref'd in dispose),
//! `title`/`title_override`/`tooltip` OWNED strings (freed on the imp's `Drop` =
//! finalize), `split_tree` TEMPLATE_CHILD.
//!
//! [`SplitTree`]: crate::gtk::class::split_tree::SplitTree

use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::glib::Properties;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::CompositeTemplate;
use libadwaita as adw;

use rustty_apprt_api::NewSurfaceContext;
use rustty_config::command::Command;
use rustty_config::value_types::ShellIntegration;

use super::config::Config;
use super::split_tree::{SplitTree, Tree};
use super::surface::{CoreSurface, Surface, SurfaceOverrides};
use crate::gtk::class::application::Application;
use crate::gtk::ext;

/// `const log = std.log.scoped(.gtk_ghostty_window)` (`tab.zig:20`) — the
/// reference's tab file logs under the WINDOW scope. Rebranded per
/// `.port/tools/PARITY_NAMING.md`, the same value as `window.rs`'s `LOG_SCOPE`.
///
/// CORRECTION (task 20): this file's four diagnostics were
/// `eprintln!("warn(gtk): ...")`, a facade bypass with the wrong scope baked
/// into the text; they go through the `log` facade now, which is also what lets
/// a test observe which of them fired (S57-F2).
const LOG_SCOPE: &str = "gtk_rustty_window";

/// PORTS: `tab.zig:190-196` the `new` overrides struct.
#[derive(Default)]
pub struct TabOverrides {
    /// PORTS: `tab.zig:191` `command`.
    pub command: Option<Command>,
    /// PORTS: `tab.zig:192` `shell_integration` (B5, 3650bef4e).
    pub shell_integration: Option<ShellIntegration>,
    /// PORTS: `tab.zig:193` `working_directory`.
    pub working_directory: Option<String>,
    /// PORTS: `tab.zig:194` `title`.
    pub title: Option<String>,
}

mod imp {
    use super::*;

    /// PORTS: `tab.zig:158-175` `Tab.Private`.
    #[derive(CompositeTemplate, Properties)]
    #[template(resource = "/io/airely/rustty/ui/1.5/tab.ui")]
    #[properties(wrapper_type = super::Tab)]
    pub struct Tab {
        /// PORTS: `tab.zig:39-55` `active-surface` (computed from the split tree).
        #[property(name = "active-surface", get = Self::get_active_surface_prop, nullable)]
        pub active_surface_prop: std::marker::PhantomData<Option<Surface>>,

        /// PORTS: `tab.zig:57-67,160` `config` (`?*Config`, SHARED).
        #[property(get, set, nullable)]
        pub config: RefCell<Option<Config>>,

        /// PORTS: `tab.zig:69-85` `split-tree` (the template child, computed getter).
        #[property(name = "split-tree", get = Self::get_split_tree_prop, nullable)]
        pub split_tree_prop: std::marker::PhantomData<Option<SplitTree>>,

        /// PORTS: `tab.zig:87-103` `surface-tree` (computed from the split tree).
        #[property(name = "surface-tree", get = Self::get_surface_tree_prop, nullable)]
        pub surface_tree_prop: std::marker::PhantomData<Option<Tree>>,

        /// PORTS: `tab.zig:105-116,169` `tooltip`.
        #[property(get, set, nullable)]
        pub tooltip: RefCell<Option<String>>,

        /// PORTS: `tab.zig:118-129,163` `title`.
        #[property(get, set, nullable)]
        pub title: RefCell<Option<String>>,

        /// PORTS: `tab.zig:130-141,166` `title-override`.
        #[property(name = "title-override", get, set, nullable)]
        pub title_override: RefCell<Option<String>>,

        /// PORTS: `tab.zig:172` `split_tree` template child.
        #[template_child]
        pub split_tree: TemplateChild<SplitTree>,
    }

    impl Default for Tab {
        fn default() -> Self {
            Self {
                active_surface_prop: std::marker::PhantomData,
                config: RefCell::new(None),
                split_tree_prop: std::marker::PhantomData,
                surface_tree_prop: std::marker::PhantomData,
                tooltip: RefCell::new(None),
                title: RefCell::new(None),
                title_override: RefCell::new(None),
                split_tree: TemplateChild::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Tab {
        const NAME: &'static str = "GhosttyTab";
        type Type = super::Tab;
        // PORTS: `tab.zig:25` `pub const Parent = gtk.Box`.
        type ParentType = gtk4::Box;

        fn class_init(klass: &mut Self::Class) {
            SplitTree::ensure_type();
            Surface::ensure_type();
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Tab {
        /// PORTS: `tab.zig:144-156` the `close-request` signal.
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("close-request").build()])
        }

        /// PORTS: `tab.zig:229-234` `init` — action map.
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().init_action_map();
        }

        /// PORTS: `tab.zig:327-343` `dispose`.
        fn dispose(&self) {
            self.config.borrow_mut().take();
            self.dispose_template();
            // NOTE: the reference frees `tooltip`/`title`/`title_override` in
            // `finalize` (`:345-364`); in Rust those `RefCell<Option<String>>`
            // drop when the imp is freed at finalize.
        }
    }

    impl WidgetImpl for Tab {}
    impl BoxImpl for Tab {}

    impl Tab {
        fn get_active_surface_prop(&self) -> Option<Surface> {
            self.obj().get_active_surface()
        }
        fn get_split_tree_prop(&self) -> Option<SplitTree> {
            Some(self.split_tree.get())
        }
        fn get_surface_tree_prop(&self) -> Option<Tree> {
            self.obj().get_surface_tree()
        }
    }
}

glib::wrapper! {
    /// PORTS: `tab.zig:22` `pub const Tab`.
    pub struct Tab(ObjectSubclass<imp::Tab>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

#[gtk4::template_callbacks]
impl Tab {
    /// PORTS: `tab.zig:190-227` `new`.
    #[must_use]
    pub fn new(config: Option<&Config>, overrides: TabOverrides) -> Self {
        let tab: Self = glib::Object::new();
        let imp = tab.imp();

        // config = passed config (ref'd) or the app's config (`:201-208`).
        let cfg = match config {
            Some(c) => Some(c.clone()),
            None => Application::default().get_config(),
        };
        *imp.config.borrow_mut() = cfg;
        tab.notify("config");

        // Create the initial surface in the split tree (`:213-224`).
        imp.split_tree
            .new_split(
                rustty_datastruct::split_tree::Direction::Right,
                None,
                SurfaceOverrides {
                    command: overrides.command,
                    shell_integration: overrides.shell_integration,
                    working_directory: overrides.working_directory,
                    title: overrides.title,
                },
            )
            // TODO: We should make our "no surfaces" state more aesthetically
            // pleasing and show something like an "Oops, something went wrong"
            // message. For now, this is incredibly unlikely.
            .expect("oom creating initial surface");

        tab
    }

    /// PORTS: `tab.zig:236-249` `initActionMap`.
    fn init_action_map(&self) {
        use crate::gtk::ext::actions::{add_as_group, Action};
        let s = glib::VariantType::new("s").expect("valid variant type");
        let actions = [
            Action::init("close", action_close, Some(s)),
            Action::init("ring-bell", action_ring_bell, None),
            Action::init("next-page", action_next_page, None),
            Action::init("previous-page", action_previous_page, None),
            Action::init("prompt-tab-title", action_prompt_tab_title, None),
        ];
        let _ = add_as_group(self, "tab", &actions);
    }

    /// PORTS: `tab.zig:180-182` `setParent`.
    ///
    /// # Safety
    /// `parent` must point to a live core surface.
    pub unsafe fn set_parent(&self, parent: *mut CoreSurface) {
        unsafe { self.set_parent_with_context(parent, NewSurfaceContext::Tab) };
    }

    /// PORTS: `tab.zig:184-188` `setParentWithContext`.
    ///
    /// # Safety
    /// `parent` must point to a live core surface.
    pub unsafe fn set_parent_with_context(
        &self,
        parent: *mut CoreSurface,
        context: NewSurfaceContext,
    ) {
        if let Some(surface) = self.get_active_surface() {
            // SAFETY: forwarded from the caller's contract.
            unsafe { surface.set_parent(parent, context) };
        }
    }

    /// PORTS: `tab.zig:256-262` `setTitleOverride`. Applies the reference's
    /// empty-string → `null` normalization on top of the generated `title-override`
    /// property setter. (Consumed by the title dialog, an INHABITED leaf until T23.)
    pub fn apply_title_override(&self, title: Option<&str>) {
        self.set_title_override(title.filter(|t| !t.is_empty()).map(str::to_owned));
    }

    /// PORTS: `tab.zig:271-283` `promptTabTitle` + `titleDialogSet` (`:263-270`).
    /// The initial value is `title_override orelse title` (`tab.zig:273`); on the
    /// dialog's `set` signal, apply the empty-string → `None` normalization.
    pub fn prompt_tab_title(&self) {
        use super::title_dialog::{Target, TitleDialog};

        // PORTS: `tab.zig:273` `priv.title_override orelse priv.title`.
        let initial = self.title_override().or_else(|| self.title());
        let dialog = TitleDialog::new(Target::Tab, initial.as_deref());

        // PORTS: `tab.zig:274-280` — connect `titleDialogSet` to `set`.
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
                    // PORTS: `tab.zig:268-269` `titleDialogSet`.
                    this.set_title_override(if title.is_empty() { None } else { Some(title) });
                    None
                }
            ),
        );

        // PORTS: `tab.zig:282`.
        dialog.present(self.upcast_ref::<gtk4::Widget>());
    }

    /// PORTS: `tab.zig:287-289` `getActiveSurface`. Does not add a ref beyond the
    /// property clone.
    #[must_use]
    pub fn get_active_surface(&self) -> Option<Surface> {
        self.get_split_tree().get_active_surface()
    }

    /// PORTS: `tab.zig:292-295` `getSurfaceTree`.
    #[must_use]
    pub fn get_surface_tree(&self) -> Option<Tree> {
        self.imp().split_tree.get_tree()
    }

    /// PORTS: `tab.zig:298-301` `getSplitTree`.
    #[must_use]
    pub fn get_split_tree(&self) -> SplitTree {
        self.imp().split_tree.get()
    }

    /// PORTS: `tab.zig:305-308` `getNeedsConfirmQuit`.
    #[must_use]
    pub fn get_needs_confirm_quit(&self) -> bool {
        self.get_split_tree().get_needs_confirm_quit()
    }

    /// PORTS: `tab.zig:311-316` `getTabView`.
    fn get_tab_view(&self) -> Option<adw::TabView> {
        ext::ancestor::<adw::TabView>(self)
    }

    /// PORTS: `tab.zig:319-322` `getTabPage`.
    fn get_tab_page(&self) -> Option<adw::TabPage> {
        let tab_view = self.get_tab_view()?;
        Some(tab_view.page(self))
    }

    //---------------------------------------------------------------
    // Template callbacks

    /// PORTS: `tab.zig:388-394` `propActiveSurface`.
    #[template_callback(name = "notify_active_surface")]
    fn on_notify_active_surface(_split_tree: &SplitTree, _pspec: &glib::ParamSpec, this: &Tab) {
        this.notify("active-surface");
    }

    /// PORTS: `tab.zig:368-386` `propSplitTree`.
    #[template_callback(name = "notify_tree")]
    fn on_notify_tree(_split_tree: &SplitTree, _pspec: &glib::ParamSpec, this: &Tab) {
        this.notify("surface-tree");

        // If our tree is empty, request close (`:375-385`).
        let empty = this.get_surface_tree().is_none_or(|tree| tree.0.is_empty());
        if empty {
            this.emit_by_name::<()>("close-request", &[]);
        }
    }

    /// PORTS: `tab.zig:480-536` `closureComputedTitle`. The template scope passes
    /// the instance first.
    #[template_callback(name = "computed_title")]
    #[allow(clippy::too_many_arguments)]
    fn computed_title(
        _this: &Tab,
        config: Option<Config>,
        terminal: Option<String>,
        surface_override: Option<String>,
        tab_override: Option<String>,
        zoomed: bool,
        bell_ringing: bool,
    ) -> Option<String> {
        // The plain title (`:496-509`). Default sourced from rustty-identity (R2),
        // NOT the reference's hardcoded "Ghostty".
        let default = rustty_identity::APP_NAME_TITLECASE.to_owned();
        let config_title: Option<String> = config
            .as_ref()
            .and_then(|c| c.get().title.as_ref())
            .map(|t| String::from_utf8_lossy(t).into_owned());
        let plain = tab_override
            .or(surface_override)
            .or(terminal)
            .or(config_title)
            .unwrap_or(default);

        // Without a config, just return the plain title (`:515-518`).
        let Some(config) = config else {
            // log.warn("config unavailable for computed title, likely bug", .{});
            log::warn!(target: LOG_SCOPE, "config unavailable for computed title, likely bug");
            return Some(plain);
        };
        let config = config.get();

        let mut buf = String::new();
        // Bell prefix (`:525-527`).
        if bell_ringing && config.bell_features.title {
            buf.push_str("\u{1f514} ");
        }
        // Zoom prefix (`:530-532`).
        if zoomed {
            buf.push_str("\u{1f50d} ");
        }
        buf.push_str(&plain);
        Some(buf)
    }
}

// -- Actions (tab.zig:396-478) -----------------------------------------------

/// PORTS: `tab.zig:398-433` `actionClose`.
fn action_close(_a: &gio::SimpleAction, args: Option<&glib::Variant>, this: &Tab) {
    // const param = param_ orelse { log.warn(...); return; };
    let Some(param) = args else {
        // log.warn("tab.close-tab called without a parameter", .{});
        log::warn!(target: LOG_SCOPE, "tab.close-tab called without a parameter");
        return;
    };

    // var str: ?[*:0]const u8 = null;
    // param.get("&s", &str);
    //
    // `&s` asks for type `s` exactly; any other variant leaves `str` null. It is
    // read here but only CHECKED after the tab view lookup, as in the reference.
    let str = if param.is_type(glib::VariantTy::STRING) {
        param.str()
    } else {
        None
    };

    // const tab_view = self.getTabView() orelse return;
    // const page = tab_view.getPage(self.as(gtk.Widget));
    //
    // CORRECTION (task 20, S57-F2): the port checked the mode string BEFORE this
    // lookup, so a tab with no `adw.TabView` ancestor and a non-string parameter
    // logged `invalid mode provided to tab.close-tab`, where the reference returns
    // silently. It also accepted `o`/`g` variants (`Variant::str`) that `&s` rejects.
    let Some(tab_view) = this.get_tab_view() else {
        return;
    };
    let page = tab_view.page(this);

    // std.meta.stringToEnum(apprt.action.CloseTabMode, std.mem.span(str orelse { ... }))
    let Some(mode_str) = str else {
        // log.warn("invalid mode provided to tab.close-tab", .{});
        log::warn!(target: LOG_SCOPE, "invalid mode provided to tab.close-tab");
        return;
    };

    // Delegate to our parent to handle this, since this will emit
    // a close-page signal that the parent can intercept.
    // apprt.action.CloseTabMode tag names (`:412-432`).
    match mode_str {
        "this" => tab_view.close_page(&page),
        "other" => tab_view.close_other_pages(&page),
        "right" => tab_view.close_pages_after(&page),
        other => {
            // Need to be defensive here since actions can be triggered externally.
            // log.warn("invalid mode provided to tab.close-tab: {s}", .{str.?});
            log::warn!(target: LOG_SCOPE, "invalid mode provided to tab.close-tab: {other}");
        }
    }
}

/// PORTS: `tab.zig:435-441` `actionPromptTabTitle`.
fn action_prompt_tab_title(_a: &gio::SimpleAction, _args: Option<&glib::Variant>, this: &Tab) {
    this.prompt_tab_title();
}

/// PORTS: `tab.zig:443-458` `actionRingBell`.
fn action_ring_bell(_a: &gio::SimpleAction, _args: Option<&glib::Variant>, this: &Tab) {
    let Some(page) = this.get_tab_page() else {
        return;
    };
    if page.is_selected() {
        return;
    }
    page.set_needs_attention(true);
}

/// PORTS: `tab.zig:460-468` `actionNextPage`.
fn action_next_page(_a: &gio::SimpleAction, _args: Option<&glib::Variant>, this: &Tab) {
    if let Some(tab_view) = this.get_tab_view() {
        tab_view.select_next_page();
    }
}

/// PORTS: `tab.zig:470-478` `actionPreviousPage`.
fn action_previous_page(_a: &gio::SimpleAction, _args: Option<&glib::Variant>, this: &Tab) {
    if let Some(tab_view) = this.get_tab_view() {
        tab_view.select_previous_page();
    }
}

impl Default for Tab {
    fn default() -> Self {
        glib::Object::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtk::class::application::Application;
    use std::ffi::c_void;

    fn bootstrap_app() -> Application {
        let mut rt_app_slot: u8 = 0;
        let mut core_app_slot: u8 = 0;
        let app = Application::new(
            std::ptr::addr_of_mut!(rt_app_slot).cast::<c_void>(),
            std::ptr::addr_of_mut!(core_app_slot).cast::<c_void>(),
        );
        app.upcast_ref::<gio::Application>().set_default();
        app
    }

    /// The live KAT: a freshly created [`Tab`] hosts a [`SplitTree`] holding
    /// exactly one surface (`tab.zig:213-224` `new` → `newSplit`). Run
    /// individually under the real display —
    ///   cargo test -p rustty-apprt --features gtk \
    ///     tab_hosts_split_tree_with_one_surface -- --ignored --exact
    #[test]
    #[ignore = "requires a display + a single GTK main thread; run with --exact"]
    fn tab_hosts_split_tree_with_one_surface() {
        let app = bootstrap_app();
        let tab = Tab::new(None, TabOverrides::default());

        // The tab's split tree exposes one active surface.
        assert!(
            tab.get_active_surface().is_some(),
            "a freshly created tab must host an active surface"
        );

        // The surface tree holds exactly one leaf.
        let tree = tab
            .get_surface_tree()
            .expect("the tab must have a non-empty surface tree");
        let mut count = 0usize;
        let mut it = tree.0.iterator();
        while it.next().is_some() {
            count += 1;
        }
        assert_eq!(count, 1, "the initial tab must have exactly one surface");

        // The split tree lives under the tab and reports it is not split.
        assert!(!tab.get_split_tree().property::<bool>("is-split"));

        drop(tab);
        let ctx = glib::MainContext::default();
        while ctx.iteration(false) {}
        app.deinit();
    }

    /// S57-F2: `tab.close-tab` looks the tab view up BEFORE checking the mode
    /// string (`tab.zig:406-423`). With no `adw.TabView` ancestor a non-string
    /// parameter returns silently; inside one, the same parameter warns
    /// `invalid mode provided to tab.close-tab`. The second half is the positive
    /// control: it proves the handler reaches the check the first half is silent
    /// about. A no-parameter call warns in both positions (it precedes the lookup).
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib close_tab_checks -- --ignored"]
    fn close_tab_checks_the_tab_view_before_the_mode() {
        use glib::prelude::ToVariant;

        crate::gtk::init_for_display_test("close_tab_checks_the_tab_view_before_the_mode");
        let _ = libadwaita::init();
        crate::gtk::register_resources();

        let action = gio::SimpleAction::new("close-tab", None);
        let not_a_string = 7u32.to_variant();
        let warn = |msg: &str| (log::Level::Warn, LOG_SCOPE.to_string(), msg.to_string());

        let tab = Tab::default();

        let cap = crate::log_capture::start();
        action_close(&action, Some(&not_a_string), &tab);
        action_close(&action, None, &tab);
        let detached = cap.messages();
        drop(cap);
        assert_eq!(
            detached,
            vec![warn("tab.close-tab called without a parameter")],
            "no tab view: the non-string parameter must return before the mode check"
        );

        let tab_view = adw::TabView::new();
        tab_view.append(&tab);
        let cap = crate::log_capture::start();
        action_close(&action, Some(&not_a_string), &tab);
        let attached = cap.messages();
        drop(cap);
        assert_eq!(
            attached,
            vec![warn("invalid mode provided to tab.close-tab")]
        );
    }
}
