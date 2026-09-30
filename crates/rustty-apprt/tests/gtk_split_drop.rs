//! SPLIT DRAG-AND-DROP KAT (task L-4b, NEW-FINDINGS N338): dropping one live
//! surface's id on another surface of the same split tree moves the split.
//!
//! `Surface::surface_drop` (`surface.zig:3759-3786` `surfaceDrop`) resolves the
//! dropped core-surface id with `App::findSurfaceByID`, then
//! `SplitTree::move_split`s the dropped surface next to the target in the
//! direction the pointer position picks. Until N338 the handler read GTK's
//! `G_TYPE_VALUE` signal parameter (a box) as the dropped value, so every real
//! drop failed `getUint64` and returned `false`; no test had reached
//! `findSurfaceByID` -> `move_split` -> `true`.
//!
//! The test builds the real product chain (`Window` -> `Tab` -> `SplitTree`),
//! splits the first surface A to the right (new surface B), waits until both
//! surfaces have live core surfaces, and emits `drop` on B's split drop target
//! (the `DropTarget` whose only gtype is `u64`) with A's core id boxed as GTK
//! passes it, at B's bottom edge. It asserts the handler answers `true` and that
//! the tree changed from `A | B` (horizontal) to B above A (vertical).
//!
//! Run (a display and GL 4.3 core are needed, as for `gtk_window_live.rs`):
//!   LIBGL_ALWAYS_SOFTWARE=1 cargo test -p rustty-apprt --features gtk,x11,wayland \
//!     --test gtk_split_drop -- --ignored
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk")`.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]

use std::time::{Duration, Instant};

use allocator_api2::alloc::Global;
use gio::prelude::*;
use gtk4::prelude::*;

use rustty_apprt::gtk::class::application::Application;
use rustty_apprt::gtk::class::overrides::TabOverrides;
use rustty_apprt::gtk::class::surface::{Surface, SurfaceOverrides};
use rustty_apprt::gtk::class::tab::Tab;
use rustty_apprt::gtk::class::window::Window;
use rustty_apprt::gtk::{self, register_resources};
use rustty_config::command::Command;
use rustty_core::app::App as CoreApp;
use rustty_datastruct::split_tree::{Direction, Layout, Node};

/// Every `DropTarget` among `widget`'s own controllers.
fn drop_targets(widget: &gtk4::Widget) -> Vec<gtk4::DropTarget> {
    let controllers = widget.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|i| controllers.item(i))
        .filter_map(|obj| obj.downcast::<gtk4::DropTarget>().ok())
        .collect()
}

/// The surface's split drop target: the one whose only gtype is `u64`
/// (`surface.zig:1857` `surface_drop_target.setGtypes(&.{ uint64 })`).
fn split_drop_target(surface: &Surface) -> gtk4::DropTarget {
    let mut found: Vec<gtk4::DropTarget> = Vec::new();
    let mut stack = vec![surface.clone().upcast::<gtk4::Widget>()];
    while let Some(w) = stack.pop() {
        found.extend(
            drop_targets(&w)
                .into_iter()
                .filter(|dt| dt.types().as_slice() == [glib::Type::U64]),
        );
        let mut child = w.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            stack.push(c);
        }
    }
    assert_eq!(found.len(), 1, "the surface has one split drop target");
    found.pop().expect("one split drop target")
}

/// The root split of the tree: (layout, left leaf, right leaf).
///
/// `get_tree` returns a copy whose leaves are its own boxed refs, freed when the
/// copy drops, so the leaves are cloned out (a strong ref each) while it lives.
fn root_split(tab: &Tab) -> (Layout, Surface, Surface) {
    let tree = tab.get_split_tree().get_tree().expect("the tab has a tree");
    let nodes = &tree.0.nodes;
    let Node::Split(split) = &nodes[0] else {
        panic!("the root is a split");
    };
    let leaf = |i: usize| match &nodes[i] {
        // SAFETY: a leaf of `tree`, alive until `tree` drops below.
        Node::Leaf(v) => unsafe { (**v).clone() },
        Node::Split(_) => panic!("both children of the root are leaves"),
    };
    (
        split.layout,
        leaf(split.left.idx()),
        leaf(split.right.idx()),
    )
}

#[test]
#[ignore = "headed GTK + GL; run: LIBGL_ALWAYS_SOFTWARE=1 cargo test -p rustty-apprt \
            --features gtk,x11,wayland --test gtk_split_drop -- --ignored"]
fn dropping_a_surface_on_another_moves_the_split() {
    rustty_apprt::init().expect("initialize rustty global state");

    let mut core_app: Box<CoreApp<gtk::App>> =
        CoreApp::<gtk::App>::create(Global).expect("core App::create");
    let core_ptr: *mut CoreApp<gtk::App> = &mut *core_app;
    let app = match gtk::App::init(core_ptr.cast()) {
        Ok(rt) => rt,
        Err(err) => match err {},
    };
    let application: Application = app.application().clone();
    application.upcast_ref::<gio::Application>().set_default();
    application
        .register(gio::Cancellable::NONE)
        .expect("register the application");
    let app_ptr: *mut std::ffi::c_void = std::ptr::from_ref(&app).cast_mut().cast();
    application.bind_rt_app(app_ptr);
    application.bind_core_app::<gtk::App>();
    register_resources();

    let sh = || Some(Command::Direct(vec![b"/bin/sh".to_vec()]));
    let window = Window::new(&application, None);
    application
        .bind_property("config", &window, "config")
        .build();
    window.new_tab_for_window(
        None,
        TabOverrides {
            command: sh(),
            shell_integration: None,
            working_directory: None,
            title: None,
        },
    );
    let tab_view = window.get_tab_view();
    let page = tab_view.selected_page().expect("the new page is selected");
    let tab = page.child().downcast::<Tab>().expect("a Tab");
    let a: Surface = tab.get_active_surface().expect("the tab's first surface");

    // Split A to the right: the tree is `A | B`.
    tab.get_split_tree()
        .new_split(
            Direction::Right,
            Some(&a),
            SurfaceOverrides {
                command: sh(),
                shell_integration: None,
                working_directory: None,
                title: None,
            },
        )
        .expect("split A to the right");
    let (layout, left, right) = root_split(&tab);
    assert!(matches!(layout, Layout::Horizontal), "A | B is horizontal");
    assert!(left == a, "A is on the left");
    let b: Surface = right;
    assert!(b != a, "B is a new surface");

    window.set_default_size(800, 480);
    window.present();
    let ctx = glib::MainContext::default();
    let keepalive =
        glib::timeout_add_local(Duration::from_millis(50), || glib::ControlFlow::Continue);

    // Both surfaces need live cores (A's id must resolve through
    // `findSurfaceByID`; `surface_drop` unwraps B's `core()`), and B an
    // allocated size for `calc_drop_direction`.
    let deadline = Instant::now() + Duration::from_secs(30);
    while (a.core().is_none() || b.core().is_none() || b.height() == 0) && Instant::now() < deadline
    {
        ctx.iteration(true);
    }
    let a_core = a
        .core()
        .expect("A's core surface (is LIBGL_ALWAYS_SOFTWARE=1 set?)");
    assert!(b.core().is_some(), "B's core surface");
    assert!(b.width() > 0 && b.height() > 0, "B is allocated");
    // SAFETY: a live, published core surface.
    let a_id: u64 = unsafe { (*a_core).id };

    // Drop A on B at B's bottom edge: `calc_drop_direction` picks Down.
    let target = split_drop_target(&b);
    let x = f64::from(b.width()) / 2.0;
    let y = f64::from(b.height()) - 1.0;
    let accepted =
        target.emit_by_name::<bool>("drop", &[&glib::value::BoxedValue(a_id.to_value()), &x, &y]);
    assert!(accepted, "surface_drop accepted the drop (the split moved)");

    let (layout, top, bottom) = root_split(&tab);
    assert!(
        matches!(layout, Layout::Vertical),
        "the tree is now vertical"
    );
    assert!(top == b, "B is on top");
    assert!(bottom == a, "A moved below B");

    // Teardown as `gtk_window_live.rs`: the live-shell surfaces are forgotten
    // (the documented termio `Subprocess::drop` gap).
    keepalive.remove();
    std::mem::forget(target);
    std::mem::forget(a);
    std::mem::forget(b);
    std::mem::forget(tab);
    std::mem::forget(page);
    std::mem::forget(tab_view);
    std::mem::forget(window);
    std::mem::forget(application);
    std::mem::forget(app);
    std::mem::forget(core_app);
}
