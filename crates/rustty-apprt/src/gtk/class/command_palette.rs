//! rustty-apprt::gtk::class::command_palette
//!
//! PORTS: ghostty `src/apprt/gtk/class/command_palette.zig` (`GhosttyCommandPalette`).
//!
//! **INHABITED placeholder (created by L5.3 T20 → filled by its own L5.3 task).**
//! [`class::window::Window`] holds a `command_palette: WeakRef<CommandPalette>`
//! (PREPORT_GTK §1.2, `window.zig:256`) and, in `toggleCommandPalette`
//! (`window.zig:2009-2049`), lazily constructs one, binds its `config`, connects
//! its `trigger` signal, and toggles it. For the window's `WeakRef<CommandPalette>`
//! field to be typed 1:1 with the reference (and for `dispose`'s
//! `command_palette.set(null)`, `window.zig:1267`, to compile), this GType must
//! exist NOW. The real command-palette dialog UI (`ui/1.5/command-palette.blp`)
//! and the `trigger`/`config` machinery land with its own task; until then
//! [`class::window::Window`]'s `toggleCommandPalette` is an INHABITED leaf, so the
//! placeholder only needs to be a registered, instantiable GObject.
//!
//! [`class::window::Window`]: crate::gtk::class::window::Window

use libadwaita::subclass::prelude::*;

mod imp {
    use super::*;

    /// PORTS: `command_palette.zig` `pub const CommandPalette`. Placeholder
    /// instance struct — the reference `Private` lands with the command-palette task.
    #[derive(Default)]
    pub struct CommandPalette;

    #[glib::object_subclass]
    impl ObjectSubclass for CommandPalette {
        // PORTS: `command_palette.zig` `.name = "GhosttyCommandPalette"`.
        const NAME: &'static str = "GhosttyCommandPalette";
        type Type = super::CommandPalette;
        // The reference parent is `adw.Dialog`; a bare `adw::Bin` is a sufficient
        // registered placeholder (never instantiated in this tier — see module doc).
        type ParentType = libadwaita::Bin;
    }

    impl ObjectImpl for CommandPalette {}
    impl WidgetImpl for CommandPalette {}
    impl BinImpl for CommandPalette {}
}

glib::wrapper! {
    /// PORTS: `command_palette.zig` `pub const CommandPalette`. A registered-GType
    /// placeholder (see the module doc) so `WeakRef<CommandPalette>` is typed 1:1.
    pub struct CommandPalette(ObjectSubclass<imp::CommandPalette>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for CommandPalette {
    fn default() -> Self {
        glib::Object::new()
    }
}
