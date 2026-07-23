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
//!   `Option<…>` field and keeps a `HashMap<u32, Tag>` only to route the
//!   `global_remove` event back to the right field (`Globals.zig:180-188`).
//! * The reference installs listeners in `onGlobalAttached` AFTER binding
//!   (`Globals.zig:105-127`). In wayland-client the listener IS the
//!   [`Dispatch`] impl selected by the `QueueHandle` passed to `bind`, so binding
//!   and listener-install are one step; the two-phase `needs_roundtrip`
//!   (`Globals.zig:19-21,86-92`) is preserved so the events those globals emit at
//!   bind time (default decoration mode, blur capabilities, output order) are
//!   captured by a second roundtrip.

use std::collections::HashMap;

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

use wayland_protocols_plasma::output_order::v1::client::kde_output_order_v1::{
    self, KdeOutputOrderV1,
};
use wayland_protocols_plasma::server_decoration::client::org_kde_kwin_server_decoration::OrgKdeKwinServerDecoration;
use wayland_protocols_plasma::server_decoration::client::org_kde_kwin_server_decoration_manager::{
    self, Mode, OrgKdeKwinServerDecorationManager,
};
use wayland_protocols_plasma::slide::client::org_kde_kwin_slide::OrgKdeKwinSlide;
use wayland_protocols_plasma::slide::client::org_kde_kwin_slide_manager::OrgKdeKwinSlideManager;
use wayland_client::protocol::wl_region::WlRegion;
use wayland_client::protocol::wl_surface::WlSurface;

/// PORTS: `Globals.zig:29-47` `Tag`. Which advertised global a binding
/// corresponds to. Kept only to route the `global_remove` event
/// (`Globals.zig:180-188`) back to the matching typed field below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tag {
    Compositor,
    XdgActivation,
    KdeDecorationManager,
    KdeSlideManager,
    KdeOutputOrder,
    ExtBackgroundEffect,
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

    /// Routes `global_remove(name)` back to the field it bound (`Globals.zig:180-188`).
    names: HashMap<u32, Tag>,

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
    pub fn new(conn: Connection, qh: QueueHandle<Globals>, registry: WlRegistry) -> Self {
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

    pub fn default_deco_mode(&self) -> Option<Mode> {
        self.state.default_deco_mode
    }
    pub fn bg_blur_supported(&self) -> bool {
        self.state.bg_blur_supported
    }

    /// PORTS: `Globals.zig:136-174` (the `.global` arm of `registryListener`). Bind
    /// a newly-advertised global if rustty knows how to drive it. Binds at
    /// `min(advertised, generated)` version — the reference binds at
    /// `T.generated_version` (`Globals.zig:149-153`); we clamp to the advertised
    /// version too so a compositor advertising a lower version than we support does
    /// not raise a protocol error.
    fn bind_global(
        &mut self,
        registry: &WlRegistry,
        qh: &QueueHandle<Globals>,
        name: u32,
        interface: &str,
        version: u32,
    ) {
        if interface == WlCompositor::interface().name {
            let v = version.min(WlCompositor::interface().version);
            self.compositor = Some(registry.bind::<WlCompositor, (), Globals>(name, v, qh, ()));
            self.names.insert(name, Tag::Compositor);
        } else if interface == XdgActivationV1::interface().name {
            let v = version.min(XdgActivationV1::interface().version);
            self.xdg_activation =
                Some(registry.bind::<XdgActivationV1, (), Globals>(name, v, qh, ()));
            self.names.insert(name, Tag::XdgActivation);
        } else if interface == OrgKdeKwinServerDecorationManager::interface().name {
            let v = version.min(OrgKdeKwinServerDecorationManager::interface().version);
            self.kde_decoration_manager = Some(
                registry.bind::<OrgKdeKwinServerDecorationManager, (), Globals>(name, v, qh, ()),
            );
            self.names.insert(name, Tag::KdeDecorationManager);
            // Emits `default_mode` at bind time (`Globals.zig:115-118`).
            self.needs_roundtrip = true;
        } else if interface == OrgKdeKwinSlideManager::interface().name {
            let v = version.min(OrgKdeKwinSlideManager::interface().version);
            self.kde_slide_manager =
                Some(registry.bind::<OrgKdeKwinSlideManager, (), Globals>(name, v, qh, ()));
            self.names.insert(name, Tag::KdeSlideManager);
        } else if interface == KdeOutputOrderV1::interface().name {
            let v = version.min(KdeOutputOrderV1::interface().version);
            self.kde_output_order =
                Some(registry.bind::<KdeOutputOrderV1, (), Globals>(name, v, qh, ()));
            self.names.insert(name, Tag::KdeOutputOrder);
            // Emits the initial output order at bind time (`Globals.zig:120-123`).
            self.needs_roundtrip = true;
        } else if interface == ExtBackgroundEffectManagerV1::interface().name {
            let v = version.min(ExtBackgroundEffectManagerV1::interface().version);
            self.ext_background_effect =
                Some(registry.bind::<ExtBackgroundEffectManagerV1, (), Globals>(name, v, qh, ()));
            self.names.insert(name, Tag::ExtBackgroundEffect);
            // Emits `capabilities` at bind time (`Globals.zig:110-113`).
            self.needs_roundtrip = true;
        }
    }

    /// PORTS: `Globals.zig:180-188` (the `.global_remove` arm) + `onGlobalRemoved`
    /// (`Globals.zig:129-134`). Unset the field whose global went away.
    ///
    /// PORT NOTE: the reference also `proxy.destroy()`s the removed binding
    /// (`Globals.zig:185`). Dropping a wayland-client proxy sends no protocol
    /// request (only the generated destructor methods do), so we simply drop it —
    /// the compositor has already withdrawn the global, so there is nothing to
    /// release. Behavior-equivalent for the graceful-teardown case.
    fn remove_global(&mut self, name: u32) {
        let Some(tag) = self.names.remove(&name) else {
            return;
        };
        match tag {
            Tag::Compositor => self.compositor = None,
            Tag::XdgActivation => self.xdg_activation = None,
            Tag::KdeDecorationManager => self.kde_decoration_manager = None,
            Tag::KdeSlideManager => self.kde_slide_manager = None,
            Tag::KdeOutputOrder => {
                self.kde_output_order = None;
                self.state.reset_output_order();
            }
            Tag::ExtBackgroundEffect => {
                self.ext_background_effect = None;
                self.state.bg_blur_supported = false;
            }
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
                version,
            } => {
                state.bind_global(registry, qh, name, &interface, version);
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
                } else {
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
