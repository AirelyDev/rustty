//! rustty-apprt::gtk::class::split_tree
//!
//! PORTS: ghostty `src/apprt/gtk/class/split_tree.zig` (`GhosttySplitTree` +
//! the internal `GhosttySplitTreeSplit`) and the `Surface.Tree` surface model.
//!
//! PREPORT_GTK §1.5 lifetimes: `tree: Option<Tree>` OWNED (freed on the imp's
//! `Drop` = finalize), `tree_bin` TEMPLATE_CHILD, `last_focused:
//! WeakRef<Surface>` (Invariant 4 — the referent can finalize out from under
//! us), `pending_close` handle, `rebuild_source`/`restore_focus_source`
//! `SourceId` (Invariant 11 — removed in `dispose`). The internal `Split`
//! carries a `handle`, its `paned` TEMPLATE_CHILD, and an `idle` `SourceId`.
//!
//! ## The `Surface.Tree` model
//! The reference `Surface.Tree = datastruct.SplitTree(Surface)`
//! (`class/surface.zig:59`). Here that is [`Tree`] — a `#[glib::Boxed]` newtype
//! over `rustty_datastruct::split_tree::SplitTree<Surface>` (boxed GType
//! `GhosttySplitTreeSurface`, matching the reference's
//! `"GhosttySplitTree" ++ typeName(View)`, `datastruct/split_tree.zig:1320-1332`).
//! [`Surface`] implements the datastruct [`View`] contract below.

use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::sync::OnceLock;

use gtk4::glib::Properties;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::CompositeTemplate;
use libadwaita::prelude::BinExt;
use libadwaita::subclass::prelude::*;

use rustty_datastruct::f16::F16;
use rustty_datastruct::split_tree::{
    Direction, Error as DsError, Goto, Handle, Layout, Node, Split as DsSplit, SplitTree as DsTree,
    View,
};

use rustty_apprt_api::NewSurfaceContext;

use super::surface::{Surface, SurfaceOverrides};
use super::surface_scrolled_window::SurfaceScrolledWindow;
use crate::gtk::class::application::Application;
use crate::gtk::class::close_confirmation_dialog::{CloseConfirmationDialog, Target};
use crate::gtk::ext;
use crate::gtk::weak_ref::WeakRef;

/// `const log = std.log.scoped(.gtk_ghostty_split_tree)` (`split_tree.zig:21`).
///
/// REBRANDED per the precedent task-14d-2 set at `gtk/class/config.rs` and now
/// recorded in `.port/tools/PARITY_NAMING.md`: a log scope tag is a runtime
/// identity string emitted into user-visible diagnostic text. The GType
/// registered NAME in this file is deliberately NOT rebranded.
const LOG_SCOPE: &str = "gtk_rustty_split_tree";

/// PORTS: `class/surface.zig:59` `pub const Tree = datastruct.SplitTree(Self)`.
///
/// A `#[glib::Boxed]` newtype so it can back the `?*Surface.Tree` GObject
/// properties (`tree`, `surface-tree`) and travel through `ext::boxed_copy`/
/// `boxed_free` exactly like the reference's `defineBoxed` type
/// (`datastruct/split_tree.zig:1315`).
#[derive(glib::Boxed)]
#[boxed_type(name = "GhosttySplitTreeSurface", nullable)]
pub struct Tree(pub DsTree<Surface>);

impl Clone for Tree {
    /// PORTS: the boxed `copy` func (`datastruct/split_tree.zig:1337-1345`) —
    /// clone the tree (ref'ing every view), OOM `@panic`s as in the reference.
    fn clone(&self) -> Self {
        Tree(
            self.0
                .clone_tree()
                .expect("split tree clone: out of memory"),
        )
    }
}

impl std::fmt::Debug for Tree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Tree(..)")
    }
}

impl Tree {
    /// Build a single-leaf tree from an existing surface. The tree takes its own
    /// reference to `surface` (`view_ref`), matching
    /// `Surface.Tree.init(alloc, surface)` (`split_tree.zig:250`).
    fn init_single(surface: &Surface) -> Result<Self, DsError> {
        // `init` reads through the pointer via `view_ref` (which clones/refs);
        // it does not take ownership of the passed pointer, so a temporary
        // clone is the faithful analogue of passing the caller's `*Surface`.
        let mut tmp = surface.clone();
        let inner = unsafe { DsTree::<Surface>::init(std::ptr::from_mut(&mut tmp)) }?;
        Ok(Tree(inner))
    }
}

/// PORTS: the `Surface` `View` contract the tree requires
/// (`datastruct/split_tree.zig:23-47`). In the reference `*Surface` is the
/// GObject instance pointer and `ref`/`unref` are GObject refcount ops
/// returning the same pointer. Here each `Node::Leaf` owns a
/// `Box<Surface>` — a heap slot holding exactly one strong GObject reference —
/// so `view_ref` clones the wrapper (bumping the GObject refcount) into a fresh
/// box and `view_unref` drops the box (releasing one ref). `eql` compares
/// GObject identity (glib wrapper `PartialEq` = pointer equality).
impl View for Surface {
    unsafe fn view_ref(view: *mut Self) -> Result<*mut Self, DsError> {
        let s: &Surface = unsafe { &*view };
        Ok(Box::into_raw(Box::new(s.clone())))
    }

    unsafe fn view_unref(view: *mut Self) {
        drop(unsafe { Box::from_raw(view) });
    }

    unsafe fn eql(a: *const Self, b: *const Self) -> bool {
        unsafe { *a == *b }
    }
}

/// Removes a glib source id, warning (not panicking) on failure — the faithful
/// shape of the reference's `if (glib.Source.remove(id) == 0) log.warn(...)`
/// (Invariant 11). `SourceId`'s own `Drop` does NOT remove the source.
fn remove_source_id(id: Option<glib::SourceId>, what: &str) {
    if let Some(source) = id {
        let removed = unsafe { glib::ffi::g_source_remove(source.as_raw()) };
        if removed == glib::ffi::GFALSE {
            // log.warn("unable to remove {s}", .{what});
            //
            // task-14d-3: the `source={}` suffix is gone — no pinned reference
            // site prints it (see this file's `unable to remove ...` sites);
            // `what` carries the reference's own wording, so each caller
            // reproduces its own reference message verbatim.
            log::warn!(target: LOG_SCOPE, "unable to remove {what}");
        }
    }
}

mod imp {
    use super::*;

    /// PORTS: `split_tree.zig:146` `SplitTree.Private`.
    #[derive(CompositeTemplate, Properties)]
    #[template(resource = "/io/airely/rustty/ui/1.5/split-tree.ui")]
    #[properties(wrapper_type = super::SplitTree)]
    pub struct SplitTree {
        /// PORTS: `split_tree.zig:40-56` `active-surface` (computed).
        #[property(name = "active-surface", get = Self::get_active_surface_prop, nullable)]
        pub active_surface_prop: PhantomData<Option<Surface>>,

        /// PORTS: `split_tree.zig:58-75` `has-surfaces` (computed).
        #[property(name = "has-surfaces", get = Self::get_has_surfaces)]
        pub has_surfaces_prop: PhantomData<bool>,

        /// PORTS: `split_tree.zig:77-94` `is-zoomed` (computed).
        #[property(name = "is-zoomed", get = Self::get_is_zoomed)]
        pub is_zoomed_prop: PhantomData<bool>,

        /// PORTS: `split_tree.zig:96-109` `tree` (boxed, custom get/set).
        #[property(name = "tree", get = Self::get_tree_value, set = Self::set_tree_value, nullable)]
        pub tree_prop: PhantomData<Option<super::Tree>>,

        /// PORTS: `split_tree.zig:111-128` `is-split` (computed).
        #[property(name = "is-split", get = Self::get_is_split)]
        pub is_split_prop: PhantomData<bool>,

        /// PORTS: `split_tree.zig:147-148` `tree` — the owned surface tree.
        pub tree: RefCell<Option<super::Tree>>,

        /// PORTS: `split_tree.zig:151` `tree_bin` template child.
        #[template_child]
        pub tree_bin: TemplateChild<libadwaita::Bin>,

        /// PORTS: `split_tree.zig:153-155` `last_focused` (Invariant 4).
        pub last_focused: WeakRef<Surface>,

        /// PORTS: `split_tree.zig:157-159` `rebuild_source` (Invariant 11).
        pub rebuild_source: RefCell<Option<glib::SourceId>>,

        /// PORTS: `split_tree.zig:161-166` `restore_focus_source` (Invariant 11).
        pub restore_focus_source: RefCell<Option<glib::SourceId>>,

        /// PORTS: `split_tree.zig:168-170` `pending_close`.
        pub pending_close: RefCell<Option<Handle>>,

        /// PORT DIVERGENCE (documented): the reference disconnects surface
        /// handlers via `signalHandlersDisconnectMatched(surface, .{.data=true},
        /// …, self)` (`split_tree.zig:395-403`) — match-by-userdata. gtk4-rs
        /// closures don't register `self` as C userdata, so we instead track the
        /// `(surface, id)` pairs we connect and disconnect exactly those. Same
        /// net effect (Invariant: every connected handler is disconnected before
        /// the old tree is freed).
        pub surface_handlers: RefCell<Vec<(Surface, glib::SignalHandlerId)>>,
    }

    impl Default for SplitTree {
        fn default() -> Self {
            Self {
                active_surface_prop: PhantomData,
                has_surfaces_prop: PhantomData,
                is_zoomed_prop: PhantomData,
                tree_prop: PhantomData,
                is_split_prop: PhantomData,
                tree: RefCell::new(None),
                tree_bin: TemplateChild::default(),
                last_focused: WeakRef::empty(),
                rebuild_source: RefCell::new(None),
                restore_focus_source: RefCell::new(None),
                pending_close: RefCell::new(None),
                surface_handlers: RefCell::new(Vec::new()),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SplitTree {
        const NAME: &'static str = "GhosttySplitTree";
        type Type = super::SplitTree;
        // PORTS: `split_tree.zig:26` `pub const Parent = gtk.Box`.
        type ParentType = gtk4::Box;

        fn class_init(klass: &mut Self::Class) {
            Surface::ensure_type();
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for SplitTree {
        /// PORTS: `split_tree.zig:131-143` the `changed` signal — `(old, new)`
        /// tree payload (both nullable boxed `Surface.Tree`).
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![glib::subclass::Signal::builder("changed")
                    .param_types([super::Tree::static_type(), super::Tree::static_type()])
                    .build()]
            })
        }

        /// PORTS: `split_tree.zig:175-184` `init` — action map + initial state.
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().init_action_map();
            *self.pending_close.borrow_mut() = None;
        }

        /// PORTS: `split_tree.zig:580-605` `dispose`.
        fn dispose(&self) {
            // `priv.last_focused.deinit()` (1dbc8ca30: the named
            // clear-before-free; see `weak_ref.rs` `deinit`).
            self.last_focused.deinit();
            remove_source_id(self.rebuild_source.borrow_mut().take(), "rebuild source");
            remove_source_id(
                self.restore_focus_source.borrow_mut().take(),
                "restore_focus source",
            );
            self.dispose_template();
            // NOTE: the reference frees `tree` in `finalize` (`:607-618`); in Rust
            // the `RefCell<Option<Tree>>` drops (unref'ing every view) when the imp
            // is freed at finalize — no explicit `finalize` override needed.
        }
    }

    impl SplitTree {
        // -- computed-property backers ------------------------------------
        pub(super) fn get_active_surface_prop(&self) -> Option<Surface> {
            self.obj().get_active_surface()
        }
        pub(super) fn get_has_surfaces(&self) -> bool {
            self.tree.borrow().as_ref().is_some_and(|t| !t.0.is_empty())
        }
        pub(super) fn get_is_zoomed(&self) -> bool {
            self.tree
                .borrow()
                .as_ref()
                .is_some_and(|t| t.0.zoomed.is_some())
        }
        pub(super) fn get_is_split(&self) -> bool {
            match self.tree.borrow().as_ref() {
                None => false,
                Some(t) => {
                    if t.0.is_empty() {
                        return false;
                    }
                    matches!(t.0.nodes[Handle::ROOT.idx()], Node::Split(_))
                }
            }
        }
        pub(super) fn get_tree_value(&self) -> Option<super::Tree> {
            self.tree.borrow().clone()
        }
        pub(super) fn set_tree_value(&self, value: Option<super::Tree>) {
            self.obj().set_tree_inner(value.as_ref());
        }
    }

    impl WidgetImpl for SplitTree {}
    impl BoxImpl for SplitTree {}
}

glib::wrapper! {
    /// PORTS: `split_tree.zig:23` `pub const SplitTree`.
    pub struct SplitTree(ObjectSubclass<imp::SplitTree>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

#[gtk4::template_callbacks]
impl SplitTree {
    /// PORTS: `split_tree.zig:186-200` `initActionMap`.
    fn init_action_map(&self) {
        use crate::gtk::ext::actions::{add_as_group, Action};
        let s = glib::VariantType::new("s").expect("valid variant type");
        let actions = [
            Action::init("new-split", action_new_split, Some(s)),
            Action::init("equalize", action_equalize, None),
            Action::init("zoom", action_zoom, None),
            Action::init("close-split", action_close_split, None),
        ];
        let _ = add_as_group(self, "split-tree", &actions);
    }

    /// PORTS: `split_tree.zig:211-286` `newSplit`.
    pub fn new_split(
        &self,
        direction: Direction,
        parent: Option<&Surface>,
        overrides: SurfaceOverrides,
    ) -> Result<(), DsError> {
        // Create our new surface (`:226-232`). `Surface::new` returns a sunk
        // (non-floating) strong ref (Invariant 12), so the reference's explicit
        // `refSink` + `defer unref` collapse to normal Rust ownership.
        let surface = Surface::new(overrides);

        // Inherit from the parent if asked (`:235-239`).
        if let Some(p) = parent {
            if let Some(core) = p.core() {
                // SAFETY: `core` is a live core surface owned by `p`.
                unsafe { surface.set_parent(core, NewSurfaceContext::Split) };
            }
        }

        // Bind is-split property for new surface (`:243-244`; B6, 1d053bd6e:
        // the inline `bindProperty` moved into `Surface.bindIsSplit` so a
        // cross-tree move can unbind the stale binding before re-binding).
        surface.bind_is_split(self);

        // Build the single-node tree (`:250`).
        let single = Tree::init_single(&surface)?;

        // Move focus to the new surface, restoring on failure (`:255-258`).
        let old_last = self.imp().last_focused.get();
        self.imp().last_focused.set(Some(&surface));

        // If we have no tree yet, this becomes our tree (`:261-264`).
        if self.imp().tree.borrow().is_none() {
            self.set_tree_inner(Some(&single));
            return Ok(());
        }

        // The handle we split relative to (`:268`).
        let handle = self.get_active_surface_handle().unwrap_or(Handle::ROOT);

        // Create the split (`:271-277`).
        let new_tree = {
            let tref = self.imp().tree.borrow();
            let old = tref.as_ref().expect("tree present (checked above)");
            match old
                .0
                .split(handle, direction, F16::from_f32(0.5), &single.0)
            {
                Ok(t) => t,
                Err(e) => {
                    // errdefer: restore last_focused (`:258`).
                    self.imp().last_focused.set(old_last.as_ref());
                    return Err(e);
                }
            }
        };

        // log.debug("new split at={} direction={} old_tree={f} new_tree={f}", .{ handle, direction, old_tree, &new_tree });
        //
        // PORT NOTE: Zig's `{f}` calls `Tree.format`, and the port has that exact
        // function as `SplitTree::format()` — so the two tree payloads are
        // rendered through it, NOT through `Debug` (`SplitTree` has no `Debug`
        // impl, and adding one would be both a cross-crate change and a LESS
        // faithful rendering). `{}` on the reference's `handle`/`direction` is
        // Zig's aggregate/enum formatter; `Debug` is this port's crate-wide
        // stand-in for that (N11). `old_tree` is read back out of the field
        // because the borrow taken above has been dropped.
        log::debug!(
            target: LOG_SCOPE,
            "new split at={handle:?} direction={direction:?} old_tree={} new_tree={}",
            self.imp()
                .tree
                .borrow()
                .as_ref()
                .map_or_else(|| "empty".to_string(), |t| t.0.format()),
            new_tree.format(),
        );

        self.set_tree_inner(Some(&Tree(new_tree)));
        Ok(())
    }

    /// PORTS: `split_tree.zig:288-331` `resize`.
    pub fn resize(&self, direction: Direction, amount: u16) -> Result<bool, DsError> {
        if amount == 0 {
            return Ok(false);
        }
        let Some(active) = self.get_active_surface_handle() else {
            return Ok(false);
        };
        let width = self.imp().tree_bin.width();
        let height = self.imp().tree_bin.height();
        if width == 0 || height == 0 {
            return Ok(false);
        }
        let width_f = f64::from(width);
        let height_f = f64::from(height);
        let amount_f = f64::from(amount);
        let ratio = match direction {
            Direction::Right => amount_f / width_f,
            Direction::Left => -(amount_f / width_f),
            Direction::Down => amount_f / height_f,
            Direction::Up => -(amount_f / height_f),
        };
        let layout = match direction {
            Direction::Left | Direction::Right => Layout::Horizontal,
            Direction::Up | Direction::Down => Layout::Vertical,
        };
        let new_tree = {
            let tref = self.imp().tree.borrow();
            let Some(old) = tref.as_ref() else {
                return Ok(false);
            };
            old.0.resize(active, layout, F16::from_f32(ratio as f32))?
        };
        self.set_tree_inner(Some(&Tree(new_tree)));
        Ok(true)
    }

    /// PORTS: `split_tree.zig:335-388` `goto`.
    pub fn goto(&self, to: Goto) -> bool {
        let Some(active) = self.get_active_surface_handle() else {
            return false;
        };

        // Compute the target + its surface (read-only borrow).
        let (target, surface) = {
            let tref = self.imp().tree.borrow();
            let Some(tree) = tref.as_ref() else {
                return false;
            };
            let target = match tree.0.goto(active, to) {
                Ok(Some(h)) => h,
                _ => return false,
            };
            if active == target {
                return false;
            }
            let surface = match &tree.0.nodes[target.idx()] {
                // SAFETY: leaf pointers are live tree-owned boxes.
                Node::Leaf(v) => unsafe { &**v }.clone(),
                Node::Split(_) => return false,
            };
            (target, surface)
        };

        surface.grab_focus();

        // last_focused (`:362-365`).
        let _old_last = self.imp().last_focused.get();
        self.imp().last_focused.set(Some(&surface));

        // Zoom handling (`:367-385`).
        let zoomed = self
            .imp()
            .tree
            .borrow()
            .as_ref()
            .is_some_and(|t| t.0.zoomed.is_some());
        if zoomed {
            let preserve = Application::default()
                .get_config()
                .is_some_and(|c| c.get().split_preserve_zoom.navigation);
            {
                let mut tref = self.imp().tree.borrow_mut();
                if let Some(tree) = tref.as_mut() {
                    if preserve {
                        tree.0.zoom(Some(target));
                    } else {
                        tree.0.zoomed = None;
                    }
                }
            }
            self.notify("tree");
            self.notify("is-zoomed");
        }

        true
    }

    /// Move the source split onto the target split in a given direction.
    /// The target split must be located within this tree.
    //
    // PORTS: `split_tree.zig:389-459` `moveSplit` (B6, 1d053bd6e). The
    // reference's `Application.default().allocator()` + the `defer
    // branch/after_split/after_remove.deinit()` trio collapse into Rust
    // ownership (scope-end drops); `getTree` here is the live borrow of the
    // widget's tree (the reference returns the live `?*Tree` pointer).
    pub fn move_split(
        &self,
        source: &Surface,
        target: &Surface,
        dir: Direction,
    ) -> Result<(), DsError> {
        let imp = self.imp();

        let (source_handle, after_split) = {
            let tref = imp.tree.borrow();
            let Some(target_tree) = tref.as_ref() else {
                return Ok(());
            };

            // This really shouldn't fail, but just in case
            let Some(target_handle) = target_tree.0.locate(target) else {
                log::warn!(target: LOG_SCOPE, "target is not placed in a split tree");
                return Ok(());
            };

            // Try to find the source within the current tree.
            // If it exists, then it's a local move; otherwise the logic gets more
            // complicated
            let source_handle = target_tree.0.locate(source);

            // First add the source into this tree.
            // We have to do this no matter what
            let branch = Tree::init_single(source)?;

            let after_split = Tree(target_tree.0.split(
                target_handle,
                dir,
                F16::from_f32(0.5),
                &branch.0,
            )?);

            (source_handle, after_split)
        };

        if let Some(handle) = source_handle {
            // Happy path: source and target are located within the same tree.
            // In that case, we just remove the existing source from this tree

            let after_remove = Tree(after_split.0.remove(handle)?);

            self.set_tree_inner(Some(&after_remove));
        } else {
            // :( Cross-tree moves are a bit more complicated.

            // TODO: Find a better way to access the split tree from here
            let Some(source_tree_widget) = ext::ancestor::<SplitTree>(source) else {
                log::warn!(target: LOG_SCOPE, "source is not placed in a split tree");
                return Ok(());
            };

            let new_source_tree = {
                let sref = source_tree_widget.imp().tree.borrow();
                let Some(source_tree) = sref.as_ref() else {
                    return Ok(());
                };

                // Remove the source from its own tree
                let Some(handle) = source_tree.0.locate(source) else {
                    return Ok(());
                };
                Tree(source_tree.0.remove(handle)?)
            };

            // Finally, set the final tree structures for both tree widgets
            source_tree_widget.set_tree_inner(Some(&new_source_tree));
            self.set_tree_inner(Some(&after_split));

            // Re-bind vital properties like `is-split`
            source.bind_is_split(self);
        }

        Ok(())
    }

    /// PORTS: `split_tree.zig:390-405` `disconnectSurfaceHandlers`.
    fn disconnect_surface_handlers(&self) {
        let handlers: Vec<(Surface, glib::SignalHandlerId)> =
            self.imp().surface_handlers.borrow_mut().drain(..).collect();
        for (surface, id) in handlers {
            surface.disconnect(id);
        }
    }

    /// PORTS: `split_tree.zig:407-434` `connectSurfaceHandlers`.
    fn connect_surface_handlers(&self) {
        // Snapshot the surfaces (releasing the tree borrow before connecting).
        let surfaces: Vec<Surface> = {
            let tref = self.imp().tree.borrow();
            let Some(tree) = tref.as_ref() else {
                return;
            };
            let mut out = Vec::new();
            let mut it = tree.0.iterator();
            while let Some(e) = it.next() {
                // SAFETY: leaf pointers are live tree-owned boxes.
                out.push(unsafe { &*e.view }.clone());
            }
            out
        };

        let mut store = self.imp().surface_handlers.borrow_mut();
        for surface in surfaces {
            // close-request → surfaceCloseRequest
            let weak = self.downgrade();
            let id = surface.connect_local("close-request", false, move |args| {
                let s: Surface = args[0].get().expect("close-request emitter is a Surface");
                if let Some(this) = weak.upgrade() {
                    this.surface_close_request(&s);
                }
                None
            });
            store.push((surface.clone(), id));

            // notify::focused → propSurfaceFocused
            let weak = self.downgrade();
            let id = surface.connect_notify_local(Some("focused"), move |s, _| {
                if let Some(this) = weak.upgrade() {
                    this.prop_surface_focused(s);
                }
            });
            store.push((surface.clone(), id));

            // notify::mapped → propSurfaceMapped
            let weak = self.downgrade();
            let id = surface.connect_notify_local(Some("mapped"), move |s, _| {
                if let Some(this) = weak.upgrade() {
                    this.prop_surface_mapped(s);
                }
            });
            store.push((surface.clone(), id));
        }
    }

    //---------------------------------------------------------------
    // Properties

    /// PORTS: `split_tree.zig:441-453` `getNeedsConfirmQuit`.
    #[must_use]
    pub fn get_needs_confirm_quit(&self) -> bool {
        let tref = self.imp().tree.borrow();
        let Some(tree) = tref.as_ref() else {
            return false;
        };
        let mut it = tree.0.iterator();
        while let Some(entry) = it.next() {
            // SAFETY: leaf pointers are live tree-owned boxes.
            let s = unsafe { &*entry.view };
            if let Some(core) = s.core() {
                // SAFETY: `core` is a live core surface owned by the surface.
                if unsafe { &mut *core }.needsConfirmQuit() {
                    return true;
                }
            }
        }
        false
    }

    /// PORTS: `split_tree.zig:457-461` `getActiveSurface`. Does not add a ref
    /// beyond the property clone.
    #[must_use]
    pub fn get_active_surface(&self) -> Option<Surface> {
        let handle = self.get_active_surface_handle()?;
        let tref = self.imp().tree.borrow();
        let tree = tref.as_ref()?;
        match &tree.0.nodes[handle.idx()] {
            // SAFETY: leaf pointers are live tree-owned boxes.
            Node::Leaf(v) => Some(unsafe { &**v }.clone()),
            Node::Split(_) => None,
        }
    }

    /// PORTS: `split_tree.zig:463-485` `getActiveSurfaceHandle`.
    fn get_active_surface_handle(&self) -> Option<Handle> {
        let tref = self.imp().tree.borrow();
        let tree = tref.as_ref()?;
        let mut it = tree.0.iterator();
        while let Some(entry) = it.next() {
            // SAFETY: leaf pointers are live tree-owned boxes.
            if unsafe { &*entry.view }.focused() {
                return Some(entry.handle);
            }
        }
        // Fall back to the most-previously-focused surface (`:474-482`).
        if let Some(v) = self.imp().last_focused.get() {
            let mut it = tree.0.iterator();
            while let Some(entry) = it.next() {
                // SAFETY: leaf pointers are live tree-owned boxes.
                if *unsafe { &*entry.view } == v {
                    return Some(entry.handle);
                }
            }
        }
        None
    }

    /// PORTS: `split_tree.zig:488-494` `getLastFocusedSurface`.
    #[must_use]
    pub fn get_last_focused_surface(&self) -> Option<Surface> {
        self.imp().last_focused.get()
    }

    /// PORTS: `split_tree.zig:506-510` `getTree`. Returns a copy of the tree
    /// (the property/boxed semantics; see the `Tree` doc).
    #[must_use]
    pub fn get_tree(&self) -> Option<Tree> {
        self.imp().tree.borrow().clone()
    }

    /// PORTS: `split_tree.zig:512-549` `setTree`. Clones the given tree in.
    pub fn set_tree_inner(&self, tree_: Option<&Tree>) {
        let imp = self.imp();

        // Normalize empty → None (`:520-524`).
        let new_norm: Option<Tree> = match tree_ {
            Some(t) if !t.0.is_empty() => Some(t.clone()),
            _ => None,
        };

        // Emit `changed(old, new)` so handlers see both values (`:528-533`).
        let old_for_signal = imp.tree.borrow().clone();
        self.emit_by_name::<()>("changed", &[&old_for_signal, &new_norm]);

        // Drop the old tree + its handlers (`:535-539`).
        if imp.tree.borrow().is_some() {
            self.disconnect_surface_handlers();
            *imp.tree.borrow_mut() = None;
        }

        // Adopt the new tree + wire handlers (`:541-546`).
        if let Some(nt) = new_norm {
            debug_assert!(imp.tree.borrow().is_none());
            debug_assert!(!nt.0.is_empty());
            *imp.tree.borrow_mut() = Some(nt);
            self.connect_surface_handlers();
        }

        self.notify("tree");
    }

    //---------------------------------------------------------------
    // Signal handlers

    /// PORTS: `split_tree.zig:696-739` `surfaceCloseRequest`.
    fn surface_close_request(&self, surface: &Surface) {
        let Some(core) = surface.core() else {
            return;
        };
        let imp = self.imp();
        *imp.pending_close.borrow_mut() = None;

        // Find the surface in the tree to verify this is valid and
        // set our pending close handle.
        //
        // (`:773-774`, B6/1d053bd6e: the open-coded iterator loop collapsed
        // into `tree.locate(surface)`.)
        let handle = {
            let tref = imp.tree.borrow();
            let Some(tree) = tref.as_ref() else {
                return;
            };
            match tree.0.locate(surface) {
                Some(h) => h,
                None => return,
            }
        };
        *imp.pending_close.borrow_mut() = Some(handle);

        // Close immediately if no confirmation is needed (`:720-727`).
        // SAFETY: `core` is a live core surface owned by `surface`.
        if !unsafe { &mut *core }.needsConfirmQuit() {
            self.close_confirmation_close();
            return;
        }

        // Show a confirmation dialog (`split_tree.zig:729-738`). The
        // `close-request` signal → `closeConfirmationClose` (implemented below).
        let dialog = CloseConfirmationDialog::new(Target::Surface);
        let weak = self.downgrade();
        dialog.connect_local("close-request", false, move |_args| {
            if let Some(this) = weak.upgrade() {
                this.close_confirmation_close();
            }
            None
        });
        // Present keeps the dialog referenced by GTK until dismissed.
        dialog.present(Some(self.upcast_ref::<gtk4::Widget>()));
    }

    /// PORTS: `split_tree.zig:741-782` `closeConfirmationClose`.
    fn close_confirmation_close(&self) {
        let imp = self.imp();
        let Some(handle) = imp.pending_close.borrow_mut().take() else {
            return;
        };

        // Compute the next focus target + the new tree (read-only borrow).
        let (new_tree, next_focus) = {
            let tref = imp.tree.borrow();
            let Some(tree) = tref.as_ref() else {
                return;
            };
            let next_handle = tree
                .0
                .goto(handle, Goto::Previous)
                .ok()
                .flatten()
                .or_else(|| tree.0.goto(handle, Goto::Next).ok().flatten());
            let next_focus: Option<Surface> = match next_handle {
                Some(nh) if nh != handle => match &tree.0.nodes[nh.idx()] {
                    // SAFETY: leaf pointers are live tree-owned boxes.
                    Node::Leaf(v) => Some(unsafe { &**v }.clone()),
                    Node::Split(_) => None,
                },
                _ => None,
            };
            let new_tree = match tree.0.remove(handle) {
                Ok(t) => t,
                Err(e) => {
                    log::warn!(target: LOG_SCOPE, "unable to remove surface from tree: {e:?}");
                    return;
                }
            };
            (new_tree, next_focus)
        };

        self.set_tree_inner(Some(&Tree(new_tree)));

        // Grab focus via last_focused (redrawn later) (`:779-781`).
        if let Some(v) = next_focus {
            imp.last_focused.set(Some(&v));
        }
    }

    /// PORTS: `split_tree.zig:784-797` `propSurfaceFocused`.
    fn prop_surface_focused(&self, surface: &Surface) {
        if !surface.focused() {
            return;
        }
        self.imp().last_focused.set(Some(surface));
        self.notify("active-surface");
    }

    /// PORTS: `split_tree.zig:799-815` `propSurfaceMapped`.
    fn prop_surface_mapped(&self, surface: &Surface) {
        if !surface.mapped() {
            return;
        }
        let imp = self.imp();
        if imp.restore_focus_source.borrow().is_none() {
            let weak = self.downgrade();
            let id = glib::idle_add_local(move || {
                if let Some(this) = weak.upgrade() {
                    this.on_restore_focus();
                }
                glib::ControlFlow::Break
            });
            *imp.restore_focus_source.borrow_mut() = Some(id);
        }
    }

    /// PORTS: `split_tree.zig:817-861` `propTree`. Template callback on the
    /// `notify::tree` of the template itself.
    #[template_callback(name = "notify_tree")]
    fn on_notify_tree(_emitter: &gtk4::Box, _pspec: &glib::ParamSpec, this: &SplitTree) {
        let imp = this.imp();

        let _guard = this.freeze_notify();
        this.notify("has-surfaces");
        this.notify("is-zoomed");

        // Drop any pending rebuild/restore sources (`:832-843`).
        remove_source_id(imp.rebuild_source.borrow_mut().take(), "rebuild source");
        remove_source_id(
            imp.restore_focus_source.borrow_mut().take(),
            "restore_focus source",
        );

        // Empty tree → clear immediately (`:845-851`).
        if imp.tree.borrow().is_none() {
            imp.tree_bin.set_child(gtk4::Widget::NONE);
            return;
        }

        // Otherwise, rebuild on idle so rapid changes debounce (`:853-860`).
        debug_assert!(imp.rebuild_source.borrow().is_none());
        let weak = this.downgrade();
        let id = glib::idle_add_local(move || {
            if let Some(this) = weak.upgrade() {
                this.on_rebuild();
            }
            glib::ControlFlow::Break
        });
        *imp.rebuild_source.borrow_mut() = Some(id);
    }

    /// PORTS: `split_tree.zig:863-897` `onRebuild`.
    fn on_rebuild(&self) {
        let imp = self.imp();
        imp.rebuild_source.borrow_mut().take();

        {
            let tref = imp.tree.borrow();
            let is_empty = tref.as_ref().is_none_or(|t| t.0.is_empty());
            if is_empty {
                imp.tree_bin.set_child(gtk4::Widget::NONE);
            } else {
                let t = tref.as_ref().expect("tree present");
                let current = t.0.zoomed.unwrap_or(Handle::ROOT);
                let widget = self.build_tree(&t.0, current);
                imp.tree_bin.set_child(Some(&widget));
                // `widget` (our ref) drops here; `tree_bin` holds its own.
            }
        }

        // Restore focus to the last-focused surface (`:883-888`).
        if let Some(v) = imp.last_focused.get() {
            v.grab_focus();
        }

        self.notify("is-split");
        self.notify("active-surface");
    }

    /// PORTS: `split_tree.zig:899-917` `onRestoreFocus`.
    fn on_restore_focus(&self) {
        let imp = self.imp();
        imp.restore_focus_source.borrow_mut().take();
        if let Some(v) = imp.last_focused.get() {
            if v.mapped() {
                v.grab_focus();
            }
        }
    }

    /// PORTS: `split_tree.zig:954-991` `buildTree`.
    ///
    /// PORT NOTE: the reference's `BuildTreeResult { widget, release_ref }`
    /// (`:926-952`) collapses here — every gtk4-rs widget value is a full
    /// (sunk) strong reference, so we always return an owned `Widget` and the
    /// caller drops it after re-parenting (`set_child`/`set_start_child` take
    /// their own ref). The reuse case still detaches from the old parent first.
    fn build_tree(&self, tree: &DsTree<Surface>, current: Handle) -> gtk4::Widget {
        match &tree.nodes[current.idx()] {
            Node::Leaf(v) => {
                // SAFETY: leaf pointers are live tree-owned boxes.
                let surface = unsafe { &**v };
                match ext::ancestor::<SurfaceScrolledWindow>(surface) {
                    None => {
                        // Not in a window yet — no reuse concern (`:964-970`).
                        glib::Object::builder::<SurfaceScrolledWindow>()
                            .property("surface", surface)
                            .build()
                            .upcast::<gtk4::Widget>()
                    }
                    Some(window) => {
                        // Keep alive while detaching from the old tree (`:973-975`).
                        let widget = window.upcast::<gtk4::Widget>();
                        Self::detach_widget(&widget);
                        widget
                    }
                }
            }
            Node::Split(s) => {
                let left = self.build_tree(tree, s.left);
                let right = self.build_tree(tree, s.right);
                let split = SplitTreeSplit::new(current, s, &left, &right);
                // `left`/`right` (our refs) drop here after being parented.
                split.upcast::<gtk4::Widget>()
            }
        }
    }

    /// PORTS: `split_tree.zig:1000-1033` `detachWidget`.
    fn detach_widget(widget: &gtk4::Widget) {
        let Some(parent) = widget.parent() else {
            return;
        };

        // Split → a Paned start/end child.
        if let Ok(paned) = parent.clone().downcast::<gtk4::Paned>() {
            if paned.start_child().as_ref() == Some(widget) {
                paned.set_start_child(gtk4::Widget::NONE);
                return;
            }
            if paned.end_child().as_ref() == Some(widget) {
                paned.set_end_child(gtk4::Widget::NONE);
                return;
            }
        }

        // Unsplit → an Adw.Bin child.
        if let Ok(bin) = parent.downcast::<libadwaita::Bin>() {
            if bin.child().as_ref() == Some(widget) {
                bin.set_child(gtk4::Widget::NONE);
                return;
            }
        }

        // Fallback.
        widget.unparent();
    }
}

// -- Actions (split_tree.zig:623-694) ----------------------------------------

/// PORTS: `split_tree.zig:623-652` `actionNewSplit`.
fn action_new_split(_a: &gio::SimpleAction, args: Option<&glib::Variant>, this: &SplitTree) {
    let Some(args) = args else {
        log::warn!(target: LOG_SCOPE, "split-tree.new-split called without a parameter");
        return;
    };
    let Some(dir_str) = args.str() else {
        return;
    };
    let direction = match dir_str {
        "left" => Direction::Left,
        "right" => Direction::Right,
        "down" => Direction::Down,
        "up" => Direction::Up,
        other => {
            log::warn!(target: LOG_SCOPE, "invalid split direction for split-tree.new-split: {other}");
            return;
        }
    };
    let active = this.get_active_surface();
    if let Err(e) = this.new_split(direction, active.as_ref(), SurfaceOverrides::default()) {
        log::warn!(target: LOG_SCOPE, "new split failed error={e:?}");
    }
}

/// PORTS: `split_tree.zig:654-668` `actionEqualize`.
fn action_equalize(_a: &gio::SimpleAction, _args: Option<&glib::Variant>, this: &SplitTree) {
    let new_tree = {
        let tref = this.imp().tree.borrow();
        let Some(old) = tref.as_ref() else {
            return;
        };
        match old.0.equalize() {
            Ok(t) => t,
            Err(e) => {
                log::warn!(target: LOG_SCOPE, "unable to equalize tree: {e:?}");
                return;
            }
        }
    };
    this.set_tree_inner(Some(&Tree(new_tree)));
}

/// PORTS: `split_tree.zig:670-685` `actionZoom`.
fn action_zoom(_a: &gio::SimpleAction, _args: Option<&glib::Variant>, this: &SplitTree) {
    let active = this.get_active_surface_handle();
    {
        let mut tref = this.imp().tree.borrow_mut();
        let Some(tree) = tref.as_mut() else {
            return;
        };
        if tree.0.zoomed.is_some() {
            tree.0.zoomed = None;
        } else {
            let Some(active) = active else {
                return;
            };
            if tree.0.zoomed == Some(active) {
                return;
            }
            tree.0.zoom(Some(active));
        }
    }
    this.notify("tree");
}

/// PORTS: `split_tree.zig:687-694` `actionCloseSplit`.
fn action_close_split(_a: &gio::SimpleAction, _args: Option<&glib::Variant>, this: &SplitTree) {
    let Some(surface) = this.get_active_surface() else {
        return;
    };
    surface.close();
}

impl Default for SplitTree {
    fn default() -> Self {
        glib::Object::new()
    }
}

// ============================================================================
// SplitTreeSplit — the internal gtk.Paned wrapper (split_tree.zig:1089-1375)
// ============================================================================

mod split_imp {
    use super::*;

    /// PORTS: `split_tree.zig:1113-1131` `SplitTreeSplit.Private`. The reference
    /// is an immutable widget with no GObject properties (`:1135-1137`).
    #[derive(CompositeTemplate)]
    #[template(resource = "/io/airely/rustty/ui/1.5/split-tree-split.ui")]
    pub struct SplitTreeSplit {
        /// PORTS: `split_tree.zig:1116` `handle`.
        pub handle: Cell<Handle>,
        /// Cache the split ratio. Looking it up is relatively expensive
        /// because we have to first walk the widget tree to find the
        /// SplitTree widget.
        ///
        /// PORTS: `split_tree.zig:1177-1180` `ratio` (90da3aba5, the split
        /// flicker fix).
        pub ratio: Cell<f64>,
        /// PORTS: `split_tree.zig:1183` `idle` source (Invariant 11).
        pub idle: RefCell<Option<glib::SourceId>>,
        /// PORTS: `split_tree.zig:1124` `max_changed`.
        pub max_changed: Cell<bool>,
        /// PORTS: `split_tree.zig:1125` `pos_changed`.
        pub pos_changed: Cell<bool>,
        /// PORTS: `split_tree.zig:1128` `paned` template child.
        #[template_child]
        pub paned: TemplateChild<gtk4::Paned>,
    }

    impl Default for SplitTreeSplit {
        fn default() -> Self {
            Self {
                handle: Cell::new(Handle::ROOT),
                ratio: Cell::new(0.0),
                idle: RefCell::new(None),
                max_changed: Cell::new(false),
                pos_changed: Cell::new(false),
                paned: TemplateChild::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SplitTreeSplit {
        const NAME: &'static str = "GhosttySplitTreeSplit";
        type Type = super::SplitTreeSplit;
        // PORTS: `split_tree.zig:1104` `pub const Parent = adw.Bin`.
        type ParentType = libadwaita::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SplitTreeSplit {
        /// PORTS: `split_tree.zig:1311-1329` `dispose`.
        fn dispose(&self) {
            remove_source_id(self.idle.borrow_mut().take(), "idle source");
            self.dispose_template();
        }
    }

    impl WidgetImpl for SplitTreeSplit {}
    impl BinImpl for SplitTreeSplit {}
}

glib::wrapper! {
    /// PORTS: `split_tree.zig:1101` `const SplitTreeSplit`. An internal,
    /// immutable widget wrapping `gtk.Paned` with ratio-based positioning.
    pub struct SplitTreeSplit(ObjectSubclass<split_imp::SplitTreeSplit>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

/// PORTS: `split_tree.zig:1242-1248` `SyncDirection` (90da3aba5). A
/// module-level enum (Rust cannot nest a type inside an inherent impl).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyncDirection {
    /// Update gtk.Paned widget to match ratio from split tree node.
    TreeToWidget,
    /// Update split tree node to match ratio from gtk.Paned widget.
    WidgetToTree,
}

#[gtk4::template_callbacks]
impl SplitTreeSplit {
    /// PORTS: `split_tree.zig:1202-1237` `new` (90da3aba5 end state).
    fn new(
        handle: Handle,
        split: &DsSplit,
        start_child: &gtk4::Widget,
        end_child: &gtk4::Widget,
    ) -> Self {
        let obj: Self = glib::Object::new();
        let imp = obj.imp();
        imp.handle.set(handle);
        // `priv.ratio = split.ratio` (`:1211`, 90da3aba5 — the reference's
        // `f16` ratio widened to f64, exactly the old tree-lookup's
        // `@floatCast`).
        imp.ratio.set(f64::from(split.ratio.to_f32()));

        let paned = imp.paned.get();
        paned.set_start_child(Some(start_child));
        paned.set_end_child(Some(end_child));
        paned.set_orientation(match split.layout {
            Layout::Horizontal => gtk4::Orientation::Horizontal,
            Layout::Vertical => gtk4::Orientation::Vertical,
        });

        // Request min width/height 1 for surfaces to prevent them from
        // becoming temporarily invisible in nested layouts. Otherwise
        // gtk.Paned might mark a surface as not visible and unmap it
        // for a single frame. See the comments below in propMaxPosition.
        // (`:1222-1231`, 90da3aba5.)
        if start_child.is::<SurfaceScrolledWindow>() {
            start_child.set_size_request(1, 1);
        }
        if end_child.is::<SurfaceScrolledWindow>() {
            end_child.set_size_request(1, 1);
        }

        // Signals and so on are setup in the template.

        obj
    }

    /// PORTS: `split_tree.zig:1242-1248` `SyncDirection` (90da3aba5).
    // Sync split ratio between the gtk.Paned widget and the split tree.
    fn sync_split_ratio(&self, direction: SyncDirection) {
        let imp = self.imp();
        let paned = imp.paned.get();

        // Current, min, and max positions as pixels.
        let pos = paned.position();
        let min: i32 = paned.property("min-position");
        let max: i32 = paned.property("max-position");

        // We don't use min but assert it's zero (`:1272`).
        debug_assert_eq!(min, 0);

        // If our max is zero then we can't do any math. I don't know
        // if this is possible but I suspect it can be if you make a nested
        // split completely minimized.
        if max == 0 {
            return;
        }

        // Determine our current ratio.
        let current_ratio = f64::from(pos) / f64::from(max);
        // `const desired_ratio: f64 = priv.ratio;` (`:1292` — the cached
        // ratio, no widget-tree walk; 90da3aba5).
        let desired_ratio = imp.ratio.get();

        // If our ratio is close enough to our desired ratio, then
        // we ignore the update. This is to avoid constant split updates
        // (`std.math.approxEqAbs(f64, ..., 0.001)`, `:1298-1303`).
        if (current_ratio - desired_ratio).abs() <= 0.001 {
            return;
        }

        match direction {
            SyncDirection::TreeToWidget => {
                // Update position in gtk.Paned to match desired ratio. Note that if
                // max-position is small, it might not be possible to accurately set
                // the desired ratio. E.g. with max-position=2 you can only have
                // ratios 0, 0.5 and 1.
                let desired_pos = (f64::from(max) * desired_ratio).round() as i32;
                paned.set_position(desired_pos);
            }
            SyncDirection::WidgetToTree => {
                // Update ratio of the split tree node. This is the most dangerous
                // part of this widget. We assume that this widget is always a child
                // of a SplitTree, we assume that our handle is valid, and we assume
                // the handle is always a split node.
                let Some(split_tree) = ext::ancestor::<SplitTree>(self) else {
                    log::warn!(target: LOG_SCOPE, "SplitTree ancestor not found");
                    return;
                };
                let handle = imp.handle.get();
                let mut tref = split_tree.imp().tree.borrow_mut();
                let Some(tree) = tref.as_mut() else {
                    log::warn!(target: LOG_SCOPE, "no tree data model set");
                    return;
                };
                debug_assert!(handle.idx() < tree.0.nodes.len());
                debug_assert!(matches!(&tree.0.nodes[handle.idx()], Node::Split(_)));
                tree.0
                    .resize_in_place(handle, F16::from_f32(current_ratio as f32));
                imp.ratio.set(current_ratio);
            }
        }
    }

    // We need to keep the split ratios from the tree datastructure and
    // widget tree in sync. Using the max-position and position properties
    // of the gtk.Paned widget, we can distinguish a resize from a manual
    // update (e.g. the user dragging the divider). If max-position changes,
    // we always have a widget resize and we sync the ratio directly from
    // the propMaxPosition function. Usually position will change as well
    // but it might not if the size change is small enough. If only position
    // changes, we have a manual human update and we sync the ratio here.
    //
    // This is a hack, it relies on the timing of property notifcations.
    // From looking at the GTK source code, it should not be possible that
    // we interpret a position change from a resize as a manual update.
    // When a gtk.Paned is resized, internally the gtk_paned_calc_position
    // function will change both max-position and position and synchronously
    // call our propMaxPosition and propPosition functions. I.e. when the
    // widget is resized, it should not be possible for onIdle to run before
    // we have been notified of both property changes.
    //
    /// PORTS: `split_tree.zig:1343-1386` `onIdle` (90da3aba5 end state).
    fn on_idle(&self) {
        let imp = self.imp();

        // Body runs first; the source/flags are cleared AFTER so a re-entrant
        // `set_position` (which fires `propPosition`) sees `idle != None` and
        // does not schedule a second idle (`:1364-1369`, the load-bearing defer
        // ordering).
        (|| {
            if imp.max_changed.get() {
                // Nothing to do, syncSplitRatio was already called
                // directly from propMaxPosition.
                return;
            }
            if !imp.pos_changed.get() {
                return;
            }

            // If only position changed, this is a manual human update and
            // we need to write our update back to the tree.
            self.sync_split_ratio(SyncDirection::WidgetToTree);
        })();

        imp.idle.borrow_mut().take();
        imp.max_changed.set(false);
        imp.pos_changed.set(false);
    }

    fn schedule_idle(&self) {
        let imp = self.imp();
        if imp.idle.borrow().is_none() {
            let weak = self.downgrade();
            let id = glib::idle_add_local(move || {
                if let Some(this) = weak.upgrade() {
                    this.on_idle();
                }
                glib::ControlFlow::Break
            });
            *imp.idle.borrow_mut() = Some(id);
        }
    }

    /// PORTS: `split_tree.zig:1388-1434` `propMaxPosition` (90da3aba5 end
    /// state).
    #[template_callback(name = "notify_max_position")]
    fn on_notify_max_position(
        _paned: &gtk4::Paned,
        _pspec: &glib::ParamSpec,
        this: &SplitTreeSplit,
    ) {
        this.imp().max_changed.set(true);

        // Widget got resized, update position to match desired ratio.
        //
        // Syncing the split ratio directly here ensures that every surface
        // gets the correct size allocated immediately. This works because
        // this callback runs during size allocation, so that we update the
        // divider position right after the gtk.Paned widget computes it and
        // right before it will use the position to compute the sizes of its
        // children. See the size_allocate and calc_position functions of the
        // gtk.Paned widget.
        //
        // If we synced the ratio in an idle callback, there might be some
        // flickering as surfaces are shown with the wrong size for a few
        // frames until our idle callback runs. Depending on the split layout
        // it might take multiple rounds of size allocation and idle callbacks
        // for every widget to get the correct size.
        //
        // Note that gtk.Paned calls Widget.setChildVisible(true/false) for
        // both children based on the initial divider position it computes.
        // If we then update the position here, we might sometimes have to
        // call setChildVisible again. This would be necessary if we updated
        // the position from initially 0 (max), where start (end) child is
        // invisible, to a value > 0 (< max). However, we can skip this because
        // set_position already queues another round of size allocation, during
        // which gtk.Paned will keep the correct position we set manually and
        // then call setChildVisible again based on that. The only problematic
        // situation is when we update the position from 0 to a value > 0,
        // because setChildVisible(false) will unmap the start child widget.
        // It will not be visible for a single frame which introduces some
        // flickering. To prevent this from happening we set a min size request
        // of 1 for all surfaces (see the "new" function above).
        this.sync_split_ratio(SyncDirection::TreeToWidget);

        // We still need the idle callback to clear the max_changed field
        // later, because propPosition might or might not still get called.
        this.schedule_idle();
    }

    /// PORTS: `split_tree.zig:1436-1450` `propPosition`.
    #[template_callback(name = "notify_position")]
    fn on_notify_position(_paned: &gtk4::Paned, _pspec: &glib::ParamSpec, this: &SplitTreeSplit) {
        this.imp().pos_changed.set(true);
        this.schedule_idle();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtk::class::application::Application;
    use glib::translate::ToGlibPtr;
    use std::ffi::c_void;

    /// Install a process-global default [`Application`] (mirrors the window KAT's
    /// `bootstrap_app`): opaque non-null `rt_app`/`core_app` placeholders (never
    /// dereferenced by `new`), then set as default so `constructed` /
    /// `Application::default()` resolve.
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

    fn source_registered(raw: u32) -> bool {
        let ctx = glib::MainContext::default();
        !unsafe { glib::ffi::g_main_context_find_source_by_id(ctx.to_glib_none().0, raw) }.is_null()
    }

    /// The invariant-11 KAT: `dispose` removes the split tree's `rebuild_source`
    /// glib source id (`split_tree.zig:583-588`). `new_split` sets a tree, whose
    /// `notify::tree` schedules the debounced rebuild on idle; we assert it is
    /// armed, then finalize the widget and assert it is gone. Run individually
    /// under the real display —
    ///   cargo test -p rustty-apprt --features gtk \
    ///     split_tree_rebuild_source_is_removed_on_dispose -- --ignored --exact
    #[test]
    #[ignore = "requires a display + a single GTK main thread; run with --exact"]
    fn split_tree_rebuild_source_is_removed_on_dispose() {
        let app = bootstrap_app();
        let st: SplitTree = glib::Object::new();

        // Setting a (non-empty) tree fires `notify::tree` → `propTree`, which
        // schedules the rebuild idle source WITHOUT running it (we never iterate).
        st.new_split(Direction::Right, None, SurfaceOverrides::default())
            .expect("new_split");
        let raw = st
            .imp()
            .rebuild_source
            .borrow()
            .as_ref()
            .expect("setting a tree must arm the rebuild source")
            .as_raw();
        assert!(
            source_registered(raw),
            "the rebuild source must be registered before dispose"
        );

        // Finalize the widget (unparented: last ref drop → dispose → finalize).
        drop(st);

        // dispose removed the pending idle (invariant 11): it is no longer found.
        assert!(
            !source_registered(raw),
            "dispose must remove rebuild_source (invariant 11)"
        );

        app.deinit();
    }

    /// The invariant-4 KAT: `last_focused` is a [`WeakRef`] — its referent can
    /// finalize out from under us and `get()` then returns `None` (no dangling)
    /// (`split_tree.zig:153-155,474`). Run individually under the real display —
    ///   cargo test -p rustty-apprt --features gtk \
    ///     split_tree_last_focused_weak_ref_survives_finalize -- --ignored --exact
    #[test]
    #[ignore = "requires a display + a single GTK main thread; run with --exact"]
    fn split_tree_last_focused_weak_ref_survives_finalize() {
        let app = bootstrap_app();
        let st: SplitTree = glib::Object::new();

        {
            let surface = Surface::new(SurfaceOverrides::default());
            st.imp().last_focused.set(Some(&surface));
            assert!(
                st.get_last_focused_surface().is_some(),
                "the weak ref resolves while the surface is alive"
            );
            // `surface` is the only strong ref; dropping it finalizes the surface
            // while our weak `last_focused` still points at it.
        }

        assert!(
            st.get_last_focused_surface().is_none(),
            "the weak ref must return None after the referent finalizes (invariant 4)"
        );

        drop(st);
        app.deinit();
    }
}
