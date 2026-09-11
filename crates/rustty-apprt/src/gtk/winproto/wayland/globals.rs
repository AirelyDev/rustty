//! rustty-apprt::gtk::winproto::wayland::globals
//!
//! PORTS: ghostty `src/apprt/gtk/winproto/wayland/Globals.zig` (253 loc) — the
//! `wl_registry` binder. It enumerates the compositor's advertised globals during
//! the initial roundtrip and binds the ones rustty knows how to drive, installing
//! per-global event listeners at bind time. On GNOME/mutter only `wl_compositor`
//! and `xdg_activation_v1` are advertised; the KDE globals
//! (`org_kde_kwin_server_decoration_manager`, `org_kde_kwin_slide_manager`,
//! `kde_output_order_v1`) and `ext_background_effect_manager_v1` are absent and
//! bind to `None` — graceful, exactly as the reference's `EnumMap` leaves them
//! unset (`Globals.zig:100-103`).
//!
//! # Divergence from the reference
//!
//! * The reference stores the bound proxies type-erased in a
//!   `std.EnumMap(Tag, Binding)` keyed by a `Tag` enum and recovers the concrete
//!   type with a comptime `tag.Type()` cast (`Globals.zig:17,37-46,100-103`). Rust
//!   has no comptime type recovery, so rustty stores each proxy in its own typed
//!   `Option<…>` field and keeps a `HashMap<Tag, u32>` — the reference's map shape
//!   minus the type-erased proxy, i.e. keyed by [`Tag`] with the advertised `name`
//!   in the value (`Globals.zig:17,24-27`) — to route the `global_remove` event
//!   back to the right field (`Globals.zig:180-188`) and to detect
//!   re-advertisement of an already-bound global (`Globals.zig:161-170`).
//! * The reference installs listeners in `onGlobalAttached` AFTER binding
//!   (`Globals.zig:105-127`). In wayland-client the listener IS the
//!   [`Dispatch`] impl selected by the `QueueHandle` passed to `bind`, so binding
//!   and listener-install are one step; the two-phase `needs_roundtrip`
//!   (`Globals.zig:19-21,86-92`) is preserved so the events those globals emit at
//!   bind time (default decoration mode, blur capabilities, output order) are
//!   captured by a second roundtrip.

use std::collections::HashMap;

use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::{delegate_noop, Connection, Dispatch, Proxy, QueueHandle};

use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::{
    self, Capability, ExtBackgroundEffectManagerV1,
};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;
use wayland_protocols::xdg::activation::v1::client::xdg_activation_token_v1::{
    self, XdgActivationTokenV1,
};
use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1;

use wayland_client::protocol::wl_region::WlRegion;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_protocols_plasma::output_order::v1::client::kde_output_order_v1::{
    self, KdeOutputOrderV1,
};
use wayland_protocols_plasma::server_decoration::client::org_kde_kwin_server_decoration::OrgKdeKwinServerDecoration;
use wayland_protocols_plasma::server_decoration::client::org_kde_kwin_server_decoration_manager::{
    self, Mode, OrgKdeKwinServerDecorationManager,
};
use wayland_protocols_plasma::slide::client::org_kde_kwin_slide::OrgKdeKwinSlide;
use wayland_protocols_plasma::slide::client::org_kde_kwin_slide_manager::OrgKdeKwinSlideManager;

use super::hotkeys::Hotkeys;
use super::vicinae::vicinae_hotkey_manager_v1::VicinaeHotkeyManagerV1;

/// `const log = std.log.scoped(.winproto_wayland_globals)` (`Globals.zig:14`).
/// Carried as the `log::` facade's `target:` (`DECISION-logging.md` §6r4).
const LOG_SCOPE: &str = "winproto_wayland_globals";

/// `T.generated_version` for every global this file binds. The reference's
/// zig-wayland scanner is told the version to generate per global, and
/// `src/build/SharedDeps.zig:824-832` asks for 1 for all of them
/// (`wl_compositor`, `org_kde_kwin_server_decoration_manager`,
/// `org_kde_kwin_slide_manager`, `kde_output_order_v1`, `xdg_activation_v1`,
/// `ext_background_effect_manager_v1`, `vicinae_hotkey_manager_v1`). wayland-rs
/// has no such knob — `I::interface().version` is the protocol XML's maximum — so
/// the reference's number is carried here.
const GENERATED_VERSION: u32 = 1;

/// PORTS: `Globals.zig:29-47` `Tag`. Which advertised global a binding
/// corresponds to. Used as the key of the binding map, exactly as the reference
/// does (`Globals.zig:17`), to route the `global_remove` event
/// (`Globals.zig:180-188`) back to the matching typed field below and to detect
/// re-advertisement (`Globals.zig:161-170`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Tag {
    Compositor,
    XdgActivation,
    KdeDecorationManager,
    KdeSlideManager,
    KdeOutputOrder,
    ExtBackgroundEffect,
    /// PORTS: `Globals.zig:36` `vicinae_hotkey_manager`.
    VicinaeHotkeyManager,
}

/// PORTS: `Globals.zig:49-70` `State`. The scalar state derived from the bound
/// globals' events (as opposed to the proxies themselves).
#[derive(Debug, Default)]
pub struct State {
    /// PORTS: `Globals.zig:51-53` `primary_output_name`. Connector name of the
    /// primary output as reported by `kde_output_order_v1`.
    pub primary_output_name: Option<String>,

    /// PORTS: `Globals.zig:55-58` `output_order_done`. Tracks the output-order
    /// event cycle; initialized to `true` so the first `output` after binding is
    /// captured.
    output_order_done: bool,

    /// PORTS: `Globals.zig:60` `default_deco_mode`.
    pub default_deco_mode: Option<Mode>,

    /// PORTS: `Globals.zig:62` `bg_effect_capabilities`. rustty only reads the
    /// `blur` capability, so the whole bitfield collapses to this bool.
    pub bg_blur_supported: bool,
}

impl State {
    /// PORTS: `Globals.zig:65-69` `resetOutputOrder`.
    fn reset_output_order(&mut self) {
        self.primary_output_name = None;
        self.output_order_done = true;
    }
}

/// PORTS: `Globals.zig` (the `@This()` struct) — the app-wide Wayland registry
/// binder + bound-global store. Held in a `Box` by [`super::App`] so its address
/// is stable across the app's lifetime, matching the reference's heap allocation
/// (`Globals.zig:72-80`, "the listener expects a stable memory address").
pub struct Globals {
    /// The wayland-client connection wrapping GDK's foreign `wl_display`. Owned so
    /// the underlying libwayland event queue created by
    /// `Backend::from_foreign_display` stays alive for the app's lifetime. GDK
    /// owns the `wl_display` itself (§1.11) — this does NOT destroy it on drop
    /// (the backend was created with `owned = false`).
    conn: Connection,

    /// The queue handle every proxy (registry, globals, and the per-window child
    /// objects created by [`super::Window`]) is bound to.
    qh: QueueHandle<Globals>,

    /// The `wl_registry`. Kept alive so `global`/`global_remove` keep dispatching.
    _registry: WlRegistry,

    /// PORTS: `Globals.zig:19-21` `needs_roundtrip`. Set when a bound global will
    /// emit initial-state events that a second roundtrip must drain.
    pub needs_roundtrip: bool,

    /// PORTS: `Globals.zig:16` `state`.
    pub state: State,

    // The bound proxies. Each is `None` when the compositor did not advertise the
    // corresponding global (`Globals.zig:100-103`'s unset `EnumMap` entries).
    compositor: Option<WlCompositor>,
    xdg_activation: Option<XdgActivationV1>,
    kde_decoration_manager: Option<OrgKdeKwinServerDecorationManager>,
    kde_slide_manager: Option<OrgKdeKwinSlideManager>,
    kde_output_order: Option<KdeOutputOrderV1>,
    ext_background_effect: Option<ExtBackgroundEffectManagerV1>,
    /// PORTS: `Globals.zig:36,46` the `vicinae_hotkey_manager` entry — the
    /// `vicinae_hotkey_v1` global-hotkey factory. Absent on every mainstream
    /// compositor today (mutter/KWin/wlroots do not implement the protocol), which
    /// is exactly the graceful-absence case the `Option` already models.
    vicinae_hotkey_manager: Option<VicinaeHotkeyManagerV1>,

    /// PORTS: `wayland.zig:31` `hotkeys: Hotkeys` — stored HERE rather than in
    /// [`super::App`] because wayland-rs dispatches every event of the queue
    /// against this one state type. See `hotkeys.rs`'s module docs.
    hotkeys: Hotkeys,

    /// PORTS: `Globals.zig:17` `map: std.EnumMap(Tag, Binding)` — minus the
    /// type-erased `Binding.proxy`, which lives in the typed field above, leaving
    /// `Binding.name` (`Globals.zig:26`) as the value. Tag-keyed like the
    /// reference, so re-advertising an already-bound global REPLACES its entry
    /// (`Globals.zig:161-170`) instead of accumulating one entry per name.
    names: HashMap<Tag, u32>,

    /// OBSERVABILITY (not in the reference): count of `xdg_activation_token_v1.done`
    /// events dispatched by the token handler below. The reference has no analogue —
    /// its handler runs on GDK's default queue with no need to prove it fired. rustty
    /// cranks a PRIVATE queue via [`super::queue_source::WlQueueSource`], so the
    /// live-session KAT reads this counter to prove that post-init token replies
    /// actually dispatch on the real compositor (the whole point of the GSource).
    /// Read through [`super::App::activation_done_count`].
    pub activation_done_count: u32,
}

impl std::fmt::Debug for Globals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Globals")
            .field("needs_roundtrip", &self.needs_roundtrip)
            .field("state", &self.state)
            .field("has_compositor", &self.compositor.is_some())
            .field("has_xdg_activation", &self.xdg_activation.is_some())
            .field(
                "has_kde_decoration_manager",
                &self.kde_decoration_manager.is_some(),
            )
            .field("has_kde_slide_manager", &self.kde_slide_manager.is_some())
            .field("has_kde_output_order", &self.kde_output_order.is_some())
            .field(
                "has_ext_background_effect",
                &self.ext_background_effect.is_some(),
            )
            .finish()
    }
}

impl Globals {
    /// Construct the empty binder. Binding happens as the `wl_registry.global`
    /// events are dispatched during [`super::App::init`]'s roundtrip
    /// (`Globals.zig:82-83`: set the listener, then roundtrip).
    pub fn new(
        conn: Connection,
        qh: QueueHandle<Globals>,
        registry: WlRegistry,
        app_id: &str,
    ) -> Self {
        Self {
            conn,
            qh,
            _registry: registry,
            needs_roundtrip: false,
            state: State {
                output_order_done: true,
                ..State::default()
            },
            compositor: None,
            xdg_activation: None,
            kde_decoration_manager: None,
            kde_slide_manager: None,
            kde_output_order: None,
            ext_background_effect: None,
            vicinae_hotkey_manager: None,
            // `hotkeys: try .init(alloc, app_id)` (`wayland.zig:54`).
            hotkeys: Hotkeys::new(app_id),
            names: HashMap::new(),
            activation_done_count: 0,
        }
    }

    /// The connection (borrow) — used by [`super::Window::init`] to reconstruct the
    /// GDK-owned `wl_surface` proxy and to flush requests.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// The shared queue handle.
    pub fn qh(&self) -> &QueueHandle<Globals> {
        &self.qh
    }

    // PORTS: `Globals.zig:100-103` `get(tag)` — typed accessors for each bound
    // global (or `None` when absent).
    pub fn compositor(&self) -> Option<&WlCompositor> {
        self.compositor.as_ref()
    }
    pub fn xdg_activation(&self) -> Option<&XdgActivationV1> {
        self.xdg_activation.as_ref()
    }
    pub fn kde_decoration_manager(&self) -> Option<&OrgKdeKwinServerDecorationManager> {
        self.kde_decoration_manager.as_ref()
    }
    pub fn kde_slide_manager(&self) -> Option<&OrgKdeKwinSlideManager> {
        self.kde_slide_manager.as_ref()
    }
    pub fn ext_background_effect(&self) -> Option<&ExtBackgroundEffectManagerV1> {
        self.ext_background_effect.as_ref()
    }

    /// PORTS: `wayland.zig:73` `self.globals.get(.vicinae_hotkey_manager)`.
    pub fn vicinae_hotkey_manager(&self) -> Option<&VicinaeHotkeyManagerV1> {
        self.vicinae_hotkey_manager.as_ref()
    }

    /// The hotkey entry table (see `hotkeys.rs`'s module docs for why it lives here
    /// and not in [`super::App`]).
    pub fn hotkeys(&self) -> &Hotkeys {
        &self.hotkeys
    }

    pub fn hotkeys_mut(&mut self) -> &mut Hotkeys {
        &mut self.hotkeys
    }

    pub fn default_deco_mode(&self) -> Option<Mode> {
        self.state.default_deco_mode
    }
    pub fn bg_blur_supported(&self) -> bool {
        self.state.bg_blur_supported
    }

    /// PORTS: `Globals.zig:136-174` (the `.global` arm of `registryListener`). Bind
    /// a newly-advertised global if rustty knows how to drive it, at
    /// `T.generated_version` (`Globals.zig:152-156`) — [`GENERATED_VERSION`].
    ///
    /// CORRECTION (task 20, S61-F1): this bound at `min(advertised,
    /// I::interface().version)`, described as clamping the reference's generated
    /// version "so a compositor advertising a lower version than we support does
    /// not raise a protocol error". Two things were wrong. The reference never
    /// looks at the advertised version. And its generated version is not the
    /// protocol XML's: `src/build/SharedDeps.zig:824-832` generates EVERY one of
    /// these globals with `scanner.generate(name, 1)`, so the reference binds each
    /// at version 1, where the port bound e.g. `wl_compositor` at up to 6.
    //
    // PORT NOTE (task-14d-3): `log.debug("matched {}", .{T})` (`Globals.zig:150`)
    // is ONE reference site inside an `inline for` over the tag list, so it
    // executes once per matched interface. Rust has no `inline for` here — the
    // dispatch is an if-else chain — so the same one reference site becomes SEVEN
    // static call sites, one per branch. `{}` on a Zig TYPE prints the type name
    // (`wl.Compositor`); the closest available payload in wayland-rs is the
    // interface NAME (`wl_compositor`), which is what is printed. This is why this
    // file measures 10 emissions against 6 reference sites; it is not extra
    // diagnostic mass at runtime.
    //
    // UNRESTORED SITE `Globals.zig:157` (`error binding interface {s} error={}`):
    // `WlRegistry::bind` in wayland-rs returns the proxy directly and cannot fail
    // synchronously, so there is no `catch` arm to hang it on.
    fn bind_global(
        &mut self,
        registry: &WlRegistry,
        qh: &QueueHandle<Globals>,
        name: u32,
        interface: &str,
    ) {
        if interface == WlCompositor::interface().name {
            let v = GENERATED_VERSION;
            // log.debug("matched {}", .{T});
            log::debug!(target: LOG_SCOPE, "matched {}", WlCompositor::interface().name);
            let old = self.compositor.take();
            self.compositor = Some(registry.bind::<WlCompositor, (), Globals>(name, v, qh, ()));
            self.record_binding(Tag::Compositor, name, old.map(|p| p.id()));
        } else if interface == XdgActivationV1::interface().name {
            let v = GENERATED_VERSION;
            // log.debug("matched {}", .{T});
            log::debug!(target: LOG_SCOPE, "matched {}", XdgActivationV1::interface().name);
            let old = self.xdg_activation.take();
            self.xdg_activation =
                Some(registry.bind::<XdgActivationV1, (), Globals>(name, v, qh, ()));
            self.record_binding(Tag::XdgActivation, name, old.map(|p| p.id()));
        } else if interface == OrgKdeKwinServerDecorationManager::interface().name {
            let v = GENERATED_VERSION;
            // log.debug("matched {}", .{T});
            log::debug!(target: LOG_SCOPE, "matched {}", OrgKdeKwinServerDecorationManager::interface().name);
            let old = self.kde_decoration_manager.take();
            self.kde_decoration_manager = Some(
                registry.bind::<OrgKdeKwinServerDecorationManager, (), Globals>(name, v, qh, ()),
            );
            self.record_binding(Tag::KdeDecorationManager, name, old.map(|p| p.id()));
            // Emits `default_mode` at bind time (`Globals.zig:115-118`).
            self.needs_roundtrip = true;
        } else if interface == OrgKdeKwinSlideManager::interface().name {
            let v = GENERATED_VERSION;
            // log.debug("matched {}", .{T});
            log::debug!(target: LOG_SCOPE, "matched {}", OrgKdeKwinSlideManager::interface().name);
            let old = self.kde_slide_manager.take();
            self.kde_slide_manager =
                Some(registry.bind::<OrgKdeKwinSlideManager, (), Globals>(name, v, qh, ()));
            self.record_binding(Tag::KdeSlideManager, name, old.map(|p| p.id()));
        } else if interface == KdeOutputOrderV1::interface().name {
            let v = GENERATED_VERSION;
            // log.debug("matched {}", .{T});
            log::debug!(target: LOG_SCOPE, "matched {}", KdeOutputOrderV1::interface().name);
            let old = self.kde_output_order.take();
            self.kde_output_order =
                Some(registry.bind::<KdeOutputOrderV1, (), Globals>(name, v, qh, ()));
            self.record_binding(Tag::KdeOutputOrder, name, old.map(|p| p.id()));
            // Emits the initial output order at bind time (`Globals.zig:120-123`).
            self.needs_roundtrip = true;
        } else if interface == ExtBackgroundEffectManagerV1::interface().name {
            let v = GENERATED_VERSION;
            // log.debug("matched {}", .{T});
            log::debug!(target: LOG_SCOPE, "matched {}", ExtBackgroundEffectManagerV1::interface().name);
            let old = self.ext_background_effect.take();
            self.ext_background_effect =
                Some(registry.bind::<ExtBackgroundEffectManagerV1, (), Globals>(name, v, qh, ()));
            self.record_binding(Tag::ExtBackgroundEffect, name, old.map(|p| p.id()));
            // Emits `capabilities` at bind time (`Globals.zig:110-113`).
            self.needs_roundtrip = true;
        } else if interface == VicinaeHotkeyManagerV1::interface().name {
            // PORTS: `Globals.zig:46` — the `vicinae_hotkey_manager` arm of the
            // registry bind. The manager emits no events, so no extra roundtrip.
            let v = GENERATED_VERSION;
            // log.debug("matched {}", .{T});
            log::debug!(target: LOG_SCOPE, "matched {}", VicinaeHotkeyManagerV1::interface().name);
            let old = self.vicinae_hotkey_manager.take();
            self.vicinae_hotkey_manager =
                Some(registry.bind::<VicinaeHotkeyManagerV1, (), Globals>(name, v, qh, ()));
            self.record_binding(Tag::VicinaeHotkeyManager, name, old.map(|p| p.id()));
        }
    }

    /// PORTS: `Globals.zig:161-172` — the "if this global was already bound, then
    /// we also need to destroy the old binding" step plus the
    /// `map.put(tag, .{ .proxy, .name })` that records the new one.
    ///
    /// `superseded` is the proxy the caller just replaced, if any. Because the
    /// reference's map is Tag-keyed, a re-advertisement replaces the entry rather
    /// than adding a second one, and it runs `onGlobalRemoved(tag)` +
    /// `old.proxy.destroy()` first.
    fn record_binding(&mut self, tag: Tag, name: u32, superseded: Option<ObjectId>) {
        if self.names.insert(tag, name).is_some() {
            self.on_global_removed(tag);
            if let Some(id) = superseded {
                self.destroy_proxy(id);
            }
        }
    }

    /// PORTS: `Globals.zig:168,185` `proxy.destroy()`. That is `wl_proxy_destroy`:
    /// a client-side free of the proxy and its id, with no protocol request.
    /// wayland-rs's exact analogue is `Backend::destroy_object` — merely dropping
    /// the Rust handle would leave the object registered in the backend's object
    /// map, which is the leak the reference's `destroy()` avoids.
    fn destroy_proxy(&self, id: ObjectId) {
        // `InvalidId` means the object is already gone, which is not an error here
        // (the reference's `wl_proxy_destroy` has no failure mode either).
        let _ = self.conn.backend().destroy_object(&id);
    }

    /// PORTS: `Globals.zig:129-134` `onGlobalRemoved`. Reset the scalar state
    /// derived from a binding that is going away. Note this is called on
    /// re-advertisement too (`Globals.zig:166`), which is why it only resets
    /// derived state and never touches the proxy fields.
    ///
    /// PORT NOTE (S61-F2): this used to carry a second `Tag::ExtBackgroundEffect`
    /// arm clearing `state.bg_blur_supported`, labelled in-place as a deliberate
    /// DIVERGENCE. The reference's `onGlobalRemoved` has only the
    /// `.kde_output_order` arm and an `else => {}`: a withdrawn
    /// `ext_background_effect` manager leaves `bg_effect_capabilities` exactly
    /// as it was. R1 fidelity — the extra arm is removed.
    ///
    /// With one arm left this is written as `if let`, not a one-armed `match`:
    /// clippy's `single_match` is a hard error under the Linux lane's
    /// warnings-as-errors, and it is the same control flow either way. The
    /// reference's `switch` shape is preserved in the comment below.
    fn on_global_removed(&mut self, tag: Tag) {
        // switch (tag) {
        //     .kde_output_order => self.state.resetOutputOrder(self.alloc),
        //     else => {},
        // }
        if let Tag::KdeOutputOrder = tag {
            self.state.reset_output_order();
        }
    }

    /// PORTS: `Globals.zig:180-188` (the `.global_remove` arm) + `onGlobalRemoved`
    /// (`Globals.zig:129-134`). Unset every binding recorded under this `name`.
    /// The reference iterates its Tag-keyed map comparing `Binding.name`
    /// (`Globals.zig:182-183`); we iterate the same map the same way.
    fn remove_global(&mut self, name: u32) {
        let tags: Vec<Tag> = self
            .names
            .iter()
            .filter(|&(_, &n)| n == name)
            .map(|(&tag, _)| tag)
            .collect();
        for tag in tags {
            self.on_global_removed(tag);
            // `kv.value.proxy.destroy()` (`Globals.zig:185`) then
            // `self.map.remove(kv.key)` (`Globals.zig:186`).
            let id = match tag {
                Tag::Compositor => self.compositor.take().map(|p| p.id()),
                Tag::XdgActivation => self.xdg_activation.take().map(|p| p.id()),
                Tag::KdeDecorationManager => self.kde_decoration_manager.take().map(|p| p.id()),
                Tag::KdeSlideManager => self.kde_slide_manager.take().map(|p| p.id()),
                Tag::KdeOutputOrder => self.kde_output_order.take().map(|p| p.id()),
                Tag::ExtBackgroundEffect => self.ext_background_effect.take().map(|p| p.id()),
                Tag::VicinaeHotkeyManager => {
                    // The manager going away does NOT invalidate the hotkey objects
                    // it created ("Hotkey objects created through this manager are
                    // unaffected and remain valid", `vicinae-hotkey-v1.xml`), so the
                    // entries are left alone.
                    self.vicinae_hotkey_manager.take().map(|p| p.id())
                }
            };
            if let Some(id) = id {
                self.destroy_proxy(id);
            }
            self.names.remove(&tag);
        }
    }
}

/// PORTS: `Globals.zig:136-190` `registryListener`.
impl Dispatch<WlRegistry, ()> for Globals {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Globals>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                // The advertised version is not consulted: the reference binds at
                // `T.generated_version` (see `bind_global`'s CORRECTION).
                version: _,
            } => {
                // log.debug("found global {s}", .{v.interface});
                log::debug!(target: LOG_SCOPE, "found global {interface}");
                state.bind_global(registry, qh, name, &interface);
            }
            wl_registry::Event::GlobalRemove { name } => {
                state.remove_global(name);
            }
            _ => {}
        }
    }
}

/// PORTS: `Globals.zig:204-214` `decoManagerListener`.
impl Dispatch<OrgKdeKwinServerDecorationManager, ()> for Globals {
    fn event(
        state: &mut Self,
        _: &OrgKdeKwinServerDecorationManager,
        event: org_kde_kwin_server_decoration_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Globals>,
    ) {
        if let org_kde_kwin_server_decoration_manager::Event::DefaultMode { mode } = event {
            // The protocol carries the mode as a plain uint (`server-decoration.xml`
            // `default_mode`), so convert (`Globals.zig:211`).
            state.state.default_deco_mode = Mode::try_from(mode).ok();
        }
    }
}

/// PORTS: `Globals.zig:192-202` `bgEffectListener`.
impl Dispatch<ExtBackgroundEffectManagerV1, ()> for Globals {
    fn event(
        state: &mut Self,
        _: &ExtBackgroundEffectManagerV1,
        event: ext_background_effect_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Globals>,
    ) {
        if let ext_background_effect_manager_v1::Event::Capabilities { flags } = event {
            // `bg_effect_capabilities = cap.flags` (`Globals.zig:199`); rustty only
            // reads the `blur` bit.
            state.state.bg_blur_supported = flags
                .into_result()
                .map(|c| c.contains(Capability::Blur))
                .unwrap_or(false);
        }
    }
}

/// PORTS: `Globals.zig:216-252` `outputOrderListener`.
impl Dispatch<KdeOutputOrderV1, ()> for Globals {
    fn event(
        state: &mut Self,
        _: &KdeOutputOrderV1,
        event: kde_output_order_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Globals>,
    ) {
        match event {
            kde_output_order_v1::Event::Output { output_name } => {
                // Only the first output event after a `done` is the new primary
                // (`Globals.zig:222-225`).
                if !state.state.output_order_done {
                    return;
                }
                state.state.output_order_done = false;

                if output_name.is_empty() {
                    state.state.primary_output_name = None;
                    // log.warn("ignoring empty primary output name from kde_output_order_v1", .{});
                    log::warn!(
                        target: LOG_SCOPE,
                        "ignoring empty primary output name from kde_output_order_v1"
                    );
                } else {
                    // UNRESTORED SITE `Globals.zig:239` (`failed to allocate
                    // primary output name: {}`): the reference's `alloc.dupeZ`
                    // can fail; the Rust `String` here is already owned and
                    // allocation is infallible, so the arm does not exist.
                    // log.debug("primary output: {s}", .{name});
                    log::debug!(target: LOG_SCOPE, "primary output: {output_name}");
                    state.state.primary_output_name = Some(output_name);
                }
            }
            kde_output_order_v1::Event::Done => {
                if state.state.output_order_done {
                    // No output arrived since the previous done — drop any stale
                    // cached primary (`Globals.zig:243-247`).
                    state.state.reset_output_order();
                } else {
                    state.state.output_order_done = true;
                }
            }
            _ => {}
        }
    }
}

/// PORTS: `wayland.zig:423-443` `onActivationTokenEvent`. The activation token's
/// `done` carries the token string; on receipt we activate the surface. The
/// surface is the `UserData` associated with the token when [`super::Window`]
/// created it, so this handler is self-contained.
///
/// PORT NOTE: unlike the reference (`wayland.zig:439-441`), this handler does NOT
/// destroy the token — the owning [`super::Window`] destroys it in `set_urgent`/
/// `deinit` (single-owner destruction avoids a double `destroy`). The registry
/// queue is cranked by the `WlQueueSource` GSource (`queue_source.rs`), so this
/// now fires on a live compositor.
///
/// PORT NOTE (stale-token guard): the reference guards with `if token.getId() !=
/// current_token.getId()` before activating (`wayland.zig:430-433`), because
/// *libwayland* can still dispatch a `done` already queued for a token the client
/// just destroyed. This handler omits that ID check because the scenario is
/// structurally unreachable here: `set_urgent`/`deinit` call `token.destroy()`
/// (wayland.rs, mirroring `wayland.zig:237`), and `wayland-rs` removes a destroyed
/// proxy from its dispatch map — a stale token's `done` is never delivered. The
/// behavior is therefore identical (a superseded token never activates) without
/// tracking a per-surface "current token" that the reference's `*Window`-routed
/// handler has but this `Globals`-routed one does not.
///
/// task-14d-3: that also makes `wayland.zig:461`'s
/// `log.warn("received event for unknown activation token; ignoring", .{})` an
/// UNRESTORED SITE with no arm to hang it on — the branch it guards cannot be
/// entered here. Named explicitly so the file's log deficit has a written cause.
impl Dispatch<XdgActivationTokenV1, WlSurface> for Globals {
    fn event(
        state: &mut Self,
        _: &XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        surface: &WlSurface,
        _: &Connection,
        _: &QueueHandle<Globals>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token } = event {
            // Count the dispatched reply BEFORE activating so the KAT observes the
            // handler firing even on a compositor that would reject the activate.
            state.activation_done_count = state.activation_done_count.saturating_add(1);
            if let Some(activation) = state.xdg_activation.as_ref() {
                activation.activate(token, surface);
            }
        }
    }
}

// Globals rustty binds/creates but whose events it ignores. `delegate_noop!`
// installs an empty `Dispatch<_, ()>` so the `QueueHandle<Globals>` can carry
// them. `ignore` (rather than the panicking form) is used for the manager/child
// proxies that CAN emit events rustty deliberately drops (e.g. the per-surface
// `org_kde_kwin_server_decoration.mode` echo, `wayland.zig` never reads it).
delegate_noop!(Globals: ignore WlCompositor);
delegate_noop!(Globals: ignore XdgActivationV1);
delegate_noop!(Globals: ignore OrgKdeKwinSlideManager);
delegate_noop!(Globals: ignore OrgKdeKwinServerDecoration);
delegate_noop!(Globals: ignore OrgKdeKwinSlide);
delegate_noop!(Globals: ignore ExtBackgroundEffectSurfaceV1);
delegate_noop!(Globals: ignore WlRegion);

#[cfg(test)]
mod tests {
    use super::*;

    /// S61-F1: every global is bound at the reference's generated version, 1, not
    /// at the advertised or protocol-maximum version. Needs a live compositor and
    /// FAILS without one (a `return` would report a pass that proved nothing); any
    /// wlroots compositor advertises `wl_compositor` at 6, so an unfixed bind reads
    /// 6 here. Uses its own `wl_display` connection, so no GTK initialisation.
    #[test]
    #[ignore = "requires a Wayland compositor (WAYLAND_DISPLAY), e.g. headless sway"]
    fn globals_bind_at_the_generated_version() {
        let conn = Connection::connect_to_env().unwrap_or_else(|err| {
            panic!(
                "no Wayland compositor to connect to ({err}); this #[ignore]d test was \
                 asked for explicitly, so that is a FAILURE — start a headless sway and \
                 export WAYLAND_DISPLAY"
            )
        });
        let mut queue = conn.new_event_queue::<Globals>();
        let qh = queue.handle();
        let registry = conn.display().get_registry(&qh, ());
        let mut globals = Globals::new(conn.clone(), qh, registry, "io.airely.rustty.test");
        queue.roundtrip(&mut globals).expect("registry roundtrip");

        let compositor = globals
            .compositor()
            .expect("every compositor advertises wl_compositor");
        assert_eq!(compositor.version(), GENERATED_VERSION);
        assert_eq!(GENERATED_VERSION, 1);
        if let Some(activation) = globals.xdg_activation() {
            assert_eq!(activation.version(), 1);
        }
    }
}
