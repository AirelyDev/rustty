//! rustty-apprt::gtk::class::command_palette
//!
//! PORTS: ghostty `src/apprt/gtk/class/command_palette.zig` (`GhosttyCommandPalette`
//! + `GhosttyCommand`).
//!
//! Two GObjects live in this file, mirroring the reference:
//!
//! * [`CommandPalette`] (`command_palette.zig:23-419`) — an `adw::Bin`
//!   (`command_palette.zig:26` `pub const Parent = adw.Bin`) whose template
//!   (`ui/1.5/command-palette.blp`) hosts the `Adw.Dialog` + search/list machinery.
//!   It owns a SHARED `config` and, on `notify::config`, (re)populates a
//!   `Gio.ListStore` (`source`) of [`Command`] objects from
//!   `config.command-palette-entry` (`command_palette.zig:144-207`). When the user
//!   activates an entry it emits `trigger` carrying the [`binding::Action`]
//!   (`command_palette.zig:49-64,330-364`); the owning [`Window`] performs it.
//!
//! * [`Command`] (`command_palette.zig:425-789`) — a `glib::Object` wrapper so a
//!   command can live in a `Gio.ListStore`. Holds a SHARED `config` and a
//!   `regular | jump` data union (`command_palette.zig:539-555`). The reference
//!   caches its computed strings in a per-`Command` arena; this port replaces the
//!   arena with owned `Option<String>` caches on the `RegularData`/`JumpData`
//!   (task-directed) — same lifetime (freed with the object), no arena needed.
//!
//! # PORT NOTE — jump commands are a documented placeholder
//!
//! `collectJumpCommands` (`command_palette.zig:225-243`) walks
//! `Application.default().core().surfaces` and calls [`Command::new_jump`] for each
//! live surface. This port tier keeps the core app pointer type-erased
//! (`application.rs` `core_app: *mut c_void`), so there is no way to iterate all
//! surfaces here yet, and [`Surface`] does not yet expose `getEffectiveTitle`/
//! `getPwd` (`surface.zig:2000-2041`). The `Command` *jump* variant is ported in
//! full (union, [`Command::new_jump`], [`Command::is_jump`],
//! [`Command::get_jump_surface`], and the jump arms of the title/description
//! getters) so the type is 1:1 with the reference, but
//! [`CommandPalette::collect_jump_commands`] is an inhabited no-op until surface
//! iteration + `getEffectiveTitle`/`getPwd` land. Regular (config keybind) commands
//! — the task's primary requirement — are populated in full.
//!
//! [`class::window::Window`]: crate::gtk::class::window::Window

use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::OnceLock;

use gtk4::glib;
use gtk4::glib::Properties;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::CompositeTemplate;
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;

use rustty_input::{binding, command};

use crate::gtk::app::App;
use crate::gtk::WeakRef;

use super::application::Application;
use super::config::Config;
use super::surface::Surface;
use super::window::Window;

/// A `#[glib::Boxed]` newtype so a [`binding::Action`] can cross the `trigger`
/// signal boundary as a `glib::Value`.
///
/// PORTS: the reference `trigger` signal carries `*const input.Binding.Action`
/// (`command_palette.zig:54-64`). A boxed newtype is the gtk4-rs spelling of a
/// signal-carried Rust value (mirrors `clipboard_confirmation_dialog.rs`
/// `ClipboardRequestBoxed` / `split_tree.rs` `Tree`).
#[derive(Clone, glib::Boxed)]
#[boxed_type(name = "GhosttyCommandPaletteAction", nullable)]
pub struct ActionBoxed(pub binding::Action);

mod imp {
    use super::*;

    /// PORTS: `command_palette.zig:66-87` `CommandPalette.Private`. A
    /// `CompositeTemplate` over `ui/1.5/command-palette.blp` binding the five
    /// template children (`command_palette.zig:390-394`) + the four instance
    /// callbacks (`command_palette.zig:397-401`).
    #[derive(Default, Properties, CompositeTemplate)]
    #[properties(wrapper_type = super::CommandPalette)]
    #[template(resource = "/io/airely/rustty/ui/1.5/command-palette.ui")]
    pub struct CommandPalette {
        /// PORTS: `command_palette.zig:36-46,68` the `config` property / private
        /// field (SHARED `?*Config`; obj accessor: get `.none` refs, set `.full`
        /// unrefs prior — automatic under gtk4-rs).
        #[property(get, set, nullable)]
        pub config: RefCell<Option<Config>>,

        /// The palette's self-owned strong reference.
        ///
        /// PORTS: `command_palette.zig:91-99` — `new` does `refSink` then hands the
        /// caller a `ref`, and the palette `unref`s itself in `dialogClosed`
        /// (`:289-291`). The [`Window`] only keeps a WEAK ref
        /// (`window.zig:2041`), so the palette must own its own life while shown.
        /// Here that self-ref is an owned clone set in [`super::CommandPalette::new`]
        /// and dropped by the `closed` callback (`command_palette.zig:289-291`).
        pub keepalive: RefCell<Option<super::CommandPalette>>,

        /// PORTS: `command_palette.zig:71` `dialog: *adw.Dialog`.
        #[template_child]
        pub dialog: TemplateChild<libadwaita::Dialog>,

        /// PORTS: `command_palette.zig:74` `search: *gtk.SearchEntry`.
        #[template_child]
        pub search: TemplateChild<gtk4::SearchEntry>,

        /// PORTS: `command_palette.zig:77` `view: *gtk.ListView`.
        #[template_child]
        pub view: TemplateChild<gtk4::ListView>,

        /// PORTS: `command_palette.zig:80` `model: *gtk.SingleSelection`.
        #[template_child]
        pub model: TemplateChild<gtk4::SingleSelection>,

        /// PORTS: `command_palette.zig:84` `source: *gio.ListStore` — the backing
        /// data store all commands are appended to.
        #[template_child]
        pub source: TemplateChild<gio::ListStore>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CommandPalette {
        // PORTS: `command_palette.zig:29` `.name = "GhosttyCommandPalette"`.
        const NAME: &'static str = "GhosttyCommandPalette";
        type Type = super::CommandPalette;
        // PORTS: `command_palette.zig:26` `pub const Parent = adw.Bin`.
        type ParentType = libadwaita::Bin;

        /// PORTS: `command_palette.zig:378-413` `Class.init`.
        fn class_init(klass: &mut Self::Class) {
            // `gobject.ext.ensureType(Command)` (`:379`) so the template's
            // `$GhosttyCommand` item-type resolves; register the boxed action GType
            // too (the `trigger` signal param needs it interned).
            super::Command::ensure_type();
            super::ActionBoxed::ensure_type();

            // `setTemplateFromResource(gresource.blueprint(1, 5,
            // "command-palette"))` + the child/callback bindings (`:380-401`).
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        /// PORTS: `command_palette.zig:105-118` `init` — `initTemplate`. The
        /// `notify::config` connection (`:109-117`) is wired in
        /// [`ObjectImpl::constructed`], once the wrapper exists to receive it.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for CommandPalette {
        /// PORTS: `command_palette.zig:49-64` `signals` — `trigger`, carrying the
        /// activated action.
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![glib::subclass::Signal::builder("trigger")
                    .param_types([super::ActionBoxed::static_type()])
                    .build()]
            })
        }

        /// PORTS: `command_palette.zig:109-117` — listen for config changes and
        /// (re)build the model. The reference connects this in `init`; here it is
        /// deferred to `constructed` because the handler is expressed on the wrapper.
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.connect_notify_local(Some("config"), |palette, _pspec| {
                palette.refresh_model();
            });
        }

        /// PORTS: `command_palette.zig:120-139` `dispose` — empty the source store,
        /// drop the config ref, dispose the template.
        fn dispose(&self) {
            self.source.remove_all();
            self.config.replace(None);
            self.dispose_template();
        }
    }

    impl WidgetImpl for CommandPalette {}
    impl BinImpl for CommandPalette {}
}

glib::wrapper! {
    /// PORTS: `command_palette.zig:23` `pub const CommandPalette`.
    pub struct CommandPalette(ObjectSubclass<imp::CommandPalette>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl CommandPalette {
    /// PORTS: `command_palette.zig:91-100` `new`. Creates a palette that owns its
    /// own life (see `imp::CommandPalette::keepalive`): the caller receives a
    /// strong ref and may drop it — the palette stays alive until its dialog is
    /// closed.
    #[must_use]
    pub fn new() -> Self {
        let obj: Self = glib::Object::new();
        // Self-owned strong ref (mirrors `refSink` + the palette holding its own
        // life, `command_palette.zig:94-99`). Cleared by the `closed` callback.
        obj.imp().keepalive.replace(Some(obj.clone()));
        obj
    }

    /// PORTS: `command_palette.zig:312-326` `toggle`. Show the palette modal over
    /// `window`, or hide it if already shown.
    pub fn toggle(&self, window: &Window) {
        let imp = self.imp();

        // If the dialog has been shown, close it (`:315-319`).
        if imp.dialog.is_realized() {
            self.close();
            return;
        }

        // Show the dialog (`:321-322`).
        imp.dialog.present(Some(window));

        // Focus the search bar (`:324-325`).
        imp.search.grab_focus();
    }

    /// PORTS: `command_palette.zig:284-287` `close`.
    fn close(&self) {
        let _ = self.imp().dialog.close();
    }

    /// PORTS: `command_palette.zig:144-179` `propConfig`. Rebuild the model from
    /// the current config's command-palette entries.
    fn refresh_model(&self) {
        let imp = self.imp();

        let config = imp.config.borrow().clone();
        let Some(config) = config else {
            // `log.warn("command palette does not have a config!")` (`:147-150`).
            eprintln!("warn(gtk): command palette does not have a config!");
            return;
        };

        // Clear existing binds (`:153`).
        imp.source.remove_all();

        let mut commands: Vec<Command> = Vec::new();

        // `collectJumpCommands` (`:162-164`) — documented placeholder (module doc).
        self.collect_jump_commands(&config, &mut commands);

        // `collectRegularCommands` (`:166`).
        self.collect_regular_commands(&config, &mut commands);

        // Sort commands (`:169-173`).
        commands.sort_by(|a, b| {
            if compare_commands(a, b) {
                std::cmp::Ordering::Less
            } else if compare_commands(b, a) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });

        // Append to the source store (`:175-178`).
        for cmd in &commands {
            imp.source.append(cmd);
        }
    }

    /// PORTS: `command_palette.zig:181-207` `collectRegularCommands`. Collect the
    /// config's command-palette entries, filtering out actions unsupported on GTK.
    fn collect_regular_commands(&self, config: &Config, commands: &mut Vec<Command>) {
        let cfg = config.get();
        for command in &cfg.command_palette_entry.value {
            // Filter unsupported/nonsensical GTK actions (`:194`).
            if !is_action_supported_on_gtk(&command.action) {
                continue;
            }
            // `Command.new(config, command)` (`:196`).
            commands.push(Command::new(config, command));
        }
    }

    /// PORTS: `command_palette.zig:225-243` `collectJumpCommands`. Collect jump
    /// commands for all surfaces across all windows.
    fn collect_jump_commands(&self, config: &Config, commands: &mut Vec<Command>) {
        // `const app = Application.default()` (`:232`). The allocator (`:233`) is
        // implicit in rustty (`Vec::push`).
        let app = Application::default();

        // Get all surfaces from the core app (`:236`). The `Application` GObject
        // stores its `core_app` type-erased as `*mut c_void`, so — unlike the
        // reference's comptime-monomorphized `app.core()` — reconstitute the
        // concrete `rustty_core::app::App<App>` from that pointer. No-op when the
        // core app is unbound (e.g. widget tests).
        let Some(core_app) = app.imp().core_app.get() else {
            return;
        };
        let core_app = core_app.as_ptr().cast::<rustty_core::app::App<App>>();

        // `for (core_app.surfaces.items) |apprt_surface|` (`:237-242`).
        // SAFETY: app-thread-only access to the surface backrefs the core app
        // owns (the same access pattern as `Application::bind_core_app`'s
        // color-scheme walk); each `*mut` apprt surface is live for the walk.
        for &apprt_surface in unsafe { &*core_app }.surfaces.iter() {
            // `const surface = apprt_surface.gobj()` (`:238`).
            let Some(surface) = (unsafe { &*apprt_surface }).widget() else {
                continue;
            };
            // `const cmd = Command.newJump(config, surface)` + `commands.append`
            // (`:239-241`).
            commands.push(Command::new_jump(config, &surface));
        }
    }

    /// PORTS: `command_palette.zig:330-364` `activated`. Emit `trigger` for the
    /// selected regular command, or focus the surface for a jump command.
    fn activated(&self, pos: u32) {
        let imp = self.imp();

        // Use the *visible* results model, not the source (`:333-334`).
        let object = imp.model.item(pos);

        // Close before running the action so we aren't replaced by another dialog
        // (`:336-341`).
        self.close();

        let Some(object) = object else {
            return;
        };
        let Ok(cmd) = object.downcast::<Command>() else {
            return;
        };

        // Jump commands focus their surface (`:346-351`).
        if cmd.is_jump() {
            if let Some(surface) = cmd.get_jump_surface() {
                surface.present();
            }
            return;
        }

        // Regular command — emit `trigger` with the action (`:353-363`).
        let Some(action) = cmd.get_action() else {
            return;
        };
        self.emit_by_name::<()>("trigger", &[&ActionBoxed(action)]);
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

// Template callbacks bound in `class_init` via `bind_template_instance_callbacks`.
//
// SIGNATURE GOTCHA: these are NON-swapped (the `.blp` callbacks carry no
// `swapped`), so gtk4-rs delivers `[emitter, ...signal_args, this]` — the emitting
// widget FIRST and the template instance (`this`) LAST, matching the reference's
// callconv(.c) signatures (`dialogClosed(_: *adw.Dialog, self: *CommandPalette)`,
// `rowActivated(_: *gtk.ListView, pos: c_uint, self: *CommandPalette)`). Writing
// these as `&self` compiles but ABORTS at runtime with a WrongValueType.
#[gtk4::template_callbacks]
impl CommandPalette {
    /// PORTS: `command_palette.zig:289-291` `dialogClosed` — the reference `unref`s
    /// itself here; this port drops the self-owned keepalive ref (its equivalent).
    #[template_callback]
    fn closed(_dialog: &libadwaita::Dialog, this: &CommandPalette) {
        this.imp().keepalive.replace(None);
    }

    /// PORTS: `command_palette.zig:293-296` `searchStopped` — ESC closes the palette.
    #[template_callback]
    fn search_stopped(_entry: &gtk4::SearchEntry, this: &CommandPalette) {
        this.close();
    }

    /// PORTS: `command_palette.zig:298-302` `searchActivated` — Enter activates the
    /// selected entry.
    #[template_callback]
    fn search_activated(_entry: &gtk4::SearchEntry, this: &CommandPalette) {
        let pos = this.imp().model.selected();
        this.activated(pos);
    }

    /// PORTS: `command_palette.zig:304-306` `rowActivated`.
    #[template_callback]
    fn row_activated(_view: &gtk4::ListView, pos: u32, this: &CommandPalette) {
        this.activated(pos);
    }
}

/// PORTS: `command_palette.zig:209-223` `isActionSupportedOnGtk`. Actions that are
/// unimplemented or don't make sense for GTK are filtered from the palette.
fn is_action_supported_on_gtk(action: &binding::Action) -> bool {
    !matches!(
        action,
        binding::Action::CloseAllWindows
            | binding::Action::ToggleSecureInput
            | binding::Action::CheckForUpdates
            | binding::Action::Redo
            | binding::Action::Undo
            | binding::Action::ResetWindowSize
            | binding::Action::ToggleWindowFloatOnTop
    )
}

/// PORTS: `command_palette.zig:245-282` `compareCommands`. Sort alphabetically by
/// title (case-insensitive, colon normalized so "Foo:" sorts before "Foo Bar:"),
/// with jump `sort_key` as the tie-breaker. Returns `true` iff `a < b`.
fn compare_commands(a: &Command, b: &Command) -> bool {
    let a_title = match a.title() {
        Some(t) => t,
        None => return false,
    };
    let b_title = match b.title() {
        Some(t) => t,
        None => return true,
    };

    let a_bytes = a_title.as_bytes();
    let b_bytes = b_title.as_bytes();

    // Compare case-insensitively with colon → tab normalization (`:253-264`).
    for i in 0..a_bytes.len().min(b_bytes.len()) {
        let a_char = if a_bytes[i] == b':' {
            b'\t'
        } else {
            a_bytes[i]
        };
        let b_char = if b_bytes[i] == b':' {
            b'\t'
        } else {
            b_bytes[i]
        };
        let a_lower = a_char.to_ascii_lowercase();
        let b_lower = b_char.to_ascii_lowercase();
        if a_lower != b_lower {
            return a_lower < b_lower;
        }
    }

    // Shorter (prefix) title first (`:267-269`).
    if a_bytes.len() != b_bytes.len() {
        return a_bytes.len() < b_bytes.len();
    }

    // Equal titles — tie-break by jump sort_key; a regular command returns false
    // (`:271-281`).
    let a_sort_key = match a.sort_key() {
        Some(k) => k,
        None => return false,
    };
    let b_sort_key = match b.sort_key() {
        Some(k) => k,
        None => return false,
    };
    a_sort_key < b_sort_key
}

// ===========================================================================
// GhosttyCommand
// ===========================================================================

mod imp_command {
    use super::*;

    /// PORTS: `command_palette.zig:539-555` `Command.Private.CommandData`. The
    /// `regular | jump` union; the reference's per-`Command` arena is replaced by
    /// owned `Option<String>` caches on each arm (task-directed).
    pub enum CommandData {
        /// PORTS: `command_palette.zig:544-548` `RegularData`.
        Regular(RegularData),
        /// PORTS: `command_palette.zig:550-555` `JumpData`.
        Jump(JumpData),
    }

    /// PORTS: `command_palette.zig:544-548` `RegularData`.
    pub struct RegularData {
        /// The (owned, cloned) command this wraps (`:545`).
        pub command: command::Command,
        /// Cached formatted accelerator string (`:546`).
        pub action: Option<String>,
        /// Cached formatted action key string (`:547`).
        pub action_key: Option<String>,
    }

    /// PORTS: `command_palette.zig:550-555` `JumpData`.
    pub struct JumpData {
        /// Weak ref to the surface this jump focuses (`:551`).
        pub surface: WeakRef<Surface>,
        /// Cached "Focus: {title}" (`:552`).
        pub title: Option<String>,
        /// Cached description / pwd (`:553`).
        pub description: Option<String>,
        /// Stable sort key (surface pointer) (`:554`).
        pub sort_key: usize,
    }

    /// PORTS: `command_palette.zig:425-556` `Command` + its `Private`. A plain
    /// `glib::Object` wrapper (`command_palette.zig:427` `Parent = gobject.Object`)
    /// so a command can live in a `Gio.ListStore`.
    #[derive(Default, Properties)]
    #[properties(wrapper_type = super::Command)]
    pub struct Command {
        /// PORTS: `command_palette.zig:439-449,533` the `config` property / private
        /// field (SHARED).
        #[property(get, set, nullable)]
        pub config: RefCell<Option<Config>>,

        /// PORTS: `command_palette.zig:451-469` property `action-key` (read-only,
        /// getter `propGetActionKey`, transfer `.none`).
        #[property(name = "action-key", get = Self::prop_get_action_key, nullable)]
        action_key: PhantomData<Option<String>>,

        /// PORTS: `command_palette.zig:471-489` property `action` (read-only,
        /// getter `propGetAction`).
        #[property(get = Self::prop_get_action, nullable)]
        action: PhantomData<Option<String>>,

        /// PORTS: `command_palette.zig:491-509` property `title` (read-only,
        /// getter `propGetTitle`).
        #[property(get = Self::prop_get_title, nullable)]
        title: PhantomData<Option<String>>,

        /// PORTS: `command_palette.zig:511-529` property `description` (read-only,
        /// getter `propGetDescription`).
        #[property(get = Self::prop_get_description, nullable)]
        description: PhantomData<Option<String>>,

        /// PORTS: `command_palette.zig:535` `data: CommandData`. `None` only in the
        /// transient window between `newInstance` and `new`/`newJump` setting it
        /// (both do so immediately).
        pub data: RefCell<Option<CommandData>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Command {
        // PORTS: `command_palette.zig:432` `.name = "GhosttyCommand"`.
        const NAME: &'static str = "GhosttyCommand";
        type Type = super::Command;
        // ParentType defaults to `glib::Object` (`command_palette.zig:427`).
    }

    #[glib::derived_properties]
    impl ObjectImpl for Command {}

    impl Command {
        /// PORTS: `command_palette.zig:637-655` `propGetActionKey`. The reference
        /// formats the FULL action (`{f}`) and caches it; here that formatted
        /// string is `command.cval().action` (the ported `Command.cval`), minus the
        /// trailing NUL. `null` for jump commands.
        fn prop_get_action_key(&self) -> Option<String> {
            let mut guard = self.data.borrow_mut();
            let Some(CommandData::Regular(r)) = guard.as_mut() else {
                return None;
            };
            if let Some(k) = &r.action_key {
                return Some(k.clone());
            }
            let c = r.command.cval().ok()?;
            let s = String::from_utf8_lossy(&c.action)
                .trim_end_matches('\0')
                .to_string();
            r.action_key = Some(s.clone());
            Some(s)
        }

        /// PORTS: `command_palette.zig:657-680` `propGetAction`. The accelerator
        /// string for this command's bound trigger (or `null`).
        fn prop_get_action(&self) -> Option<String> {
            let mut guard = self.data.borrow_mut();
            let Some(CommandData::Regular(r)) = guard.as_mut() else {
                return None;
            };
            if let Some(a) = &r.action {
                return Some(a.clone());
            }
            let config = self.config.borrow();
            let config = config.as_ref()?;
            // First trigger bound to this action, then trigger → accel (`:668-677`).
            let trigger = config.get().keybind.set.get_trigger(&r.command.action)?;
            let accel = crate::gtk::key::accel_from_trigger(trigger)?;
            r.action = Some(accel.clone());
            Some(accel)
        }

        /// PORTS: `command_palette.zig:682-706` `propGetTitle`.
        fn prop_get_title(&self) -> Option<String> {
            let mut guard = self.data.borrow_mut();
            match guard.as_mut()? {
                // Regular: the command title (`:685`).
                CommandData::Regular(r) => {
                    Some(String::from_utf8_lossy(&r.command.title).into_owned())
                }
                // Jump: "Focus: {effective_title}" (`:686-704`).
                CommandData::Jump(j) => {
                    if let Some(t) = &j.title {
                        return Some(t.clone());
                    }
                    let surface = j.surface.get()?;
                    let effective =
                        effective_title(&surface).unwrap_or_else(|| "Untitled".to_string());
                    let t = format!("Focus: {effective}");
                    j.title = Some(t.clone());
                    Some(t)
                }
            }
        }

        /// PORTS: `command_palette.zig:708-732` `propGetDescription`.
        fn prop_get_description(&self) -> Option<String> {
            let mut guard = self.data.borrow_mut();
            match guard.as_mut()? {
                // Regular: the command description (`:711`).
                CommandData::Regular(r) => {
                    Some(String::from_utf8_lossy(&r.command.description).into_owned())
                }
                // Jump: the pwd, unless it is already part of the title (`:712-730`).
                CommandData::Jump(j) => {
                    if let Some(d) = &j.description {
                        return Some(d.clone());
                    }
                    let surface = j.surface.get()?;
                    let title = effective_title(&surface).unwrap_or_else(|| "Untitled".to_string());
                    if let Some(pwd) = surface_pwd(&surface) {
                        if !title.contains(&pwd) {
                            j.description = Some(pwd.clone());
                            return Some(pwd);
                        }
                    }
                    None
                }
            }
        }
    }
}

glib::wrapper! {
    /// PORTS: `command_palette.zig:425` `pub const Command`.
    pub struct Command(ObjectSubclass<imp_command::Command>);
}

impl Command {
    /// PORTS: `command_palette.zig:558-574` `new`. Wrap a regular config command
    /// (deep-cloned, like the reference clones into its arena).
    #[must_use]
    pub fn new(config: &Config, command: &command::Command) -> Self {
        let obj: Self = glib::Object::builder().property("config", config).build();
        obj.imp()
            .data
            .replace(Some(imp_command::CommandData::Regular(
                imp_command::RegularData {
                    command: command.clone_in(),
                    action: None,
                    action_key: None,
                },
            )));
        obj
    }

    /// PORTS: `command_palette.zig:576-592` `newJump`. A command that focuses a
    /// specific surface.
    #[must_use]
    pub fn new_jump(config: &Config, surface: &Surface) -> Self {
        let obj: Self = glib::Object::builder().property("config", config).build();
        let weak: WeakRef<Surface> = WeakRef::default();
        weak.set(Some(surface));
        // `sort_key = @intFromPtr(surface)` (`:587`).
        let sort_key = surface.as_ptr() as usize;
        obj.imp().data.replace(Some(imp_command::CommandData::Jump(
            imp_command::JumpData {
                surface: weak,
                title: None,
                description: None,
                sort_key,
            },
        )));
        obj
    }

    /// PORTS: `command_palette.zig:739-745` `getAction`. A copy of the regular
    /// command's action (or `None` for a jump command).
    #[must_use]
    pub fn get_action(&self) -> Option<binding::Action> {
        match self.imp().data.borrow().as_ref()? {
            imp_command::CommandData::Regular(r) => Some(r.command.action.clone_action()),
            imp_command::CommandData::Jump(_) => None,
        }
    }

    /// PORTS: `command_palette.zig:747-751` `isJump`.
    #[must_use]
    pub fn is_jump(&self) -> bool {
        matches!(
            self.imp().data.borrow().as_ref(),
            Some(imp_command::CommandData::Jump(_))
        )
    }

    /// PORTS: `command_palette.zig:753-761` `getJumpSurface`. A strong ref to the
    /// jump surface, or `None` if it has been destroyed / this is not a jump.
    #[must_use]
    pub fn get_jump_surface(&self) -> Option<Surface> {
        match self.imp().data.borrow().as_ref()? {
            imp_command::CommandData::Jump(j) => j.surface.get(),
            imp_command::CommandData::Regular(_) => None,
        }
    }

    /// The jump `sort_key`, or `None` for a regular command
    /// (`command_palette.zig:272-280`, the `compareCommands` tie-breaker).
    fn sort_key(&self) -> Option<usize> {
        match self.imp().data.borrow().as_ref()? {
            imp_command::CommandData::Jump(j) => Some(j.sort_key),
            imp_command::CommandData::Regular(_) => None,
        }
    }
}

/// PORTS: `surface.zig:2000-2003` `getEffectiveTitle` — `title_override orelse
/// title`. Read through the surface's GObject properties.
fn effective_title(surface: &Surface) -> Option<String> {
    surface.title_override().or_else(|| surface.title())
}

/// PORTS: `surface.zig:2038-2041` `getPwd`.
fn surface_pwd(surface: &Surface) -> Option<String> {
    surface.pwd()
}
