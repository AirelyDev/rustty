//! rustty-apprt::gtk::class::split_tree
//!
//! PORTS: ghostty `src/apprt/gtk/class/split_tree.zig` (`GhosttySplitTree` +
//! `GhosttySplit`) and its surface-tree model.
//!
//! **INHABITED placeholder (created by L5.3 T20 → filled by L5.3 T21).**
//! [`class::window::Window`]'s `Class.init` `ensureType(SplitTree)`s this GType so
//! `ui/1.5/window.ui` (via the `$GhosttyTab` chain) resolves, and the window's
//! tab/split plumbing (`newTabPage`, `tabSplitTreeChanged`) names both the
//! `SplitTree` class, its `changed` signal, and the [`Tree`] surface model. The
//! real class (its `tree`/`tree_bin`/`last_focused` state and the `Paned`-backed
//! `Split`, PREPORT_GTK §1.5) plus the full [`Tree`] iterator land with T21; the
//! window methods that iterate a live [`Tree`] are INHABITED leaves until then.
//!
//! [`class::window::Window`]: crate::gtk::class::window::Window

use std::sync::OnceLock;

use libadwaita::subclass::prelude::*;

/// PORTS: `split_tree.zig` `Surface.Tree` — the surface split-tree model a
/// [`SplitTree`] holds and a [`super::tab::Tab`] exposes via `getSurfaceTree`.
///
/// **Placeholder (filled by L5.3 T21).** The reference is a recursive
/// zipper/tree of surfaces with an `iterator()` yielding each view
/// (`window.zig:770-836` consumes it). Until T21 lands the real model, this is an
/// opaque type: [`class::window::Window`] never obtains a live instance (the
/// placeholder [`super::tab::Tab::get_surface_tree`] returns `None`), so its
/// consumers — `connectSurfaceHandlers`/`disconnectSurfaceHandlers` — are
/// INHABITED leaves.
///
/// [`class::window::Window`]: crate::gtk::class::window::Window
#[derive(Debug)]
pub struct Tree {
    _private: (),
}

mod imp {
    use super::*;

    /// PORTS: `split_tree.zig:146` `SplitTree.Private`. Placeholder instance struct
    /// — the reference fields land with T21.
    #[derive(Default)]
    pub struct SplitTree;

    #[glib::object_subclass]
    impl ObjectSubclass for SplitTree {
        // PORTS: `split_tree.zig` `.name = "GhosttySplitTree"`.
        const NAME: &'static str = "GhosttySplitTree";
        type Type = super::SplitTree;
        // The reference parent is `adw.Bin`.
        type ParentType = libadwaita::Bin;
    }

    impl ObjectImpl for SplitTree {
        /// PORTS: `split_tree.zig` the `changed` signal — the one
        /// [`super::super::window::Window`]'s `newTabPage` connects
        /// (`window.zig:489-495`). Registered now so the type is complete; its
        /// `(old_tree, new_tree)` payload is modeled once the real [`Tree`] lands
        /// (T21), so the placeholder registers it parameterless.
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("changed").build()])
        }
    }
    impl WidgetImpl for SplitTree {}
    impl BinImpl for SplitTree {}
}

glib::wrapper! {
    /// PORTS: `split_tree.zig` `pub const SplitTree`. A registered-GType placeholder
    /// (`changed` signal) until L5.3 T21 (see the module doc).
    pub struct SplitTree(ObjectSubclass<imp::SplitTree>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl SplitTree {
    /// PORTS: `split_tree.zig` `getTree` (`window.zig:502` consumes it). Returns
    /// the current surface [`Tree`], or `None`. **Placeholder (T21):** always
    /// `None` until the real model lands.
    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn get_tree(&self) -> Option<Tree> {
        None
    }
}

impl Default for SplitTree {
    fn default() -> Self {
        glib::Object::new()
    }
}
