//! rustty-apprt::gtk::winproto::wayland::hotkeys
//!
//! PORTS: ghostty `src/apprt/gtk/winproto/wayland/Hotkeys.zig` (164 loc) —
//! "Global shortcuts backed by the vicinae-hotkey Wayland protocol."
//!
//! One [`Entry`] per `global:` keybind the config declares. Each entry asks the
//! compositor for its combination through `vicinae_hotkey_manager_v1.bind` and then
//! lives on the answer: `bound` (active), `denied(reason, message)` /
//! `revoked(reason, message)` (dead — surfaced to the user through
//! [`GlobalShortcuts`]'s `bind-failed` signal, which raises a desktop
//! notification), `pressed` (re-emit the action on the `trigger` signal).
//!
//! Unlike the XDG portal — a session-scoped, user-mediated, PERSISTED grant — a
//! vicinae hotkey is ephemeral, per-object and client-chosen, and the compositor
//! answers each request individually. That per-binding failure channel is the whole
//! reason the `bind-failed` half of the series exists: without it a denied binding
//! is a keybind that silently does nothing.
//!
//! # Divergence: where the listener state lives (FORCED by wayland-rs)
//!
//! The reference installs a per-object listener with a `*Entry` userdata
//! (`Hotkeys.zig:60`), so `Hotkeys` can own the entries wherever it likes — it
//! lives in `wayland.App` (`wayland.zig:31`). wayland-rs instead dispatches every
//! event of a queue through `Dispatch<Proxy, UserData> for D`, where `D` is the ONE
//! state type the queue was created with (here [`Globals`], see
//! `queue_source.rs`) and `UserData: Send + Sync + 'static`. A `*Entry` equivalent
//! is therefore impossible twice over: the state must be reachable from `Globals`,
//! and the userdata cannot be a non-`Send` back-pointer.
//!
//! So: `Hotkeys` is a field of [`Globals`] (reached by [`super::App`] through the
//! shared `Rc<RefCell<Pump>>`), and the per-proxy userdata is a [`HotkeyId`] —
//! a monotonically increasing token that never repeats, so a late event for a
//! cleared entry resolves to nothing instead of aliasing a NEW entry the way a
//! bare index would.
//!
//! # Divergence: emissions are DEFERRED out of the dispatch borrow
//!
//! The reference emits `trigger` / `bind-failed` from inside the listener
//! (`Hotkeys.zig:152,161`). rustty cannot: the listener runs inside
//! `queue_source`'s `pump.try_borrow_mut()`, and a `trigger` emission runs an
//! arbitrary keybind action — including `toggle_quick_terminal`, which reaches
//! `winproto::Window::init` → `pump.borrow()` and would panic on the live
//! `RefMut`. Emissions are therefore queued on [`Hotkeys::pending`] and drained by
//! the queue source AFTER it drops the borrow, in dispatch order. The reference has
//! no such constraint because Zig's listener runs on GDK's default queue with no
//! borrow flag held; the observable ordering (same main-loop turn, same sequence)
//! is unchanged.

use wayland_client::{Connection, Dispatch, QueueHandle};

use rustty_config::Config as CoreConfig;
use rustty_input::binding::{Action, Trigger, TriggerKey, Value};

use crate::gtk::class::global_shortcuts::{BindFailed, GlobalShortcuts};
use crate::gtk::key;

use super::globals::Globals;
use super::vicinae::vicinae_hotkey_manager_v1::{Modifiers, VicinaeHotkeyManagerV1};
use super::vicinae::vicinae_hotkey_v1::{self, VicinaeHotkeyV1};

// const log = std.log.scoped(.winproto_wayland_hotkeys);
const LOG_SCOPE: &str = "winproto_wayland_hotkeys";

/// The per-proxy userdata: which [`Entry`] an event belongs to. See the module
/// docs for why this is a token rather than the reference's `*Entry`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyId(u64);

/// A signal emission the dispatch handler produced but must NOT run inline.
/// Drained by `queue_source`'s trampoline once the pump borrow is released.
pub enum Emission {
    /// `entry.shortcuts.emitTrigger(&entry.action)` (`Hotkeys.zig:161`).
    Trigger(GlobalShortcuts, Action),
    /// `self.shortcuts.emitBindFailed(&.{...})` (`Hotkeys.zig:75-80`).
    BindFailed(GlobalShortcuts, BindFailed),
}

impl Emission {
    /// Run the emission. Called from the main loop with no wayland borrow live.
    pub fn emit(&self) {
        match self {
            Emission::Trigger(gs, action) => gs.emit_trigger(action),
            Emission::BindFailed(gs, failure) => gs.emit_bind_failed(failure),
        }
    }
}

/// PORTS: `Hotkeys.zig:22-81` `Entry`.
struct Entry {
    /// The userdata token this entry's proxy carries (rustty-only; see module docs).
    id: HotkeyId,

    /// PORTS: `Hotkeys.zig:23-24` `hotkey` — "Null once the binding was denied or
    /// revoked." Also null between `bind`'s append and its request (`:117`).
    hotkey: Option<VicinaeHotkeyV1>,

    /// PORTS: `Hotkeys.zig:25` `trigger`.
    trigger: Trigger,

    /// PORTS: `Hotkeys.zig:26` `action`.
    action: Action,

    /// PORTS: `Hotkeys.zig:27` `shortcuts: *GlobalShortcuts`.
    ///
    /// PORT NOTE: a WEAK reference, not the reference's raw pointer. `Hotkeys`
    /// lives in the app's winproto state and `GlobalShortcuts` is owned by the same
    /// `Application`, so a strong ref would keep it alive past its own `dispose`
    /// (the reference's raw pointer would simply dangle there). Upgrading at
    /// emission time is the safe equivalent on every reachable path.
    shortcuts: glib::WeakRef<GlobalShortcuts>,
}

impl Entry {
    /// PORTS: `Hotkeys.zig:29-32` `deinit`.
    fn deinit(&mut self) {
        // `if (self.hotkey) |hotkey| hotkey.destroy();`
        if let Some(hotkey) = self.hotkey.take() {
            hotkey.destroy();
        }
    }

    /// PORTS: `Hotkeys.zig:34-66` `bind`.
    fn bind(
        &mut self,
        manager: &VicinaeHotkeyManagerV1,
        qh: &QueueHandle<Globals>,
        app_id: &str,
    ) -> Result<(), BindError> {
        // "The only time this will return an error is when `trigger.key` is
        // `catch_all`, which is guarded in the public `bind` function"
        // (`Hotkeys.zig:39-41`). The reference spells the guarantee `orelse
        // unreachable`; the port returns the error instead of aborting, because a
        // compositor-facing path must never be able to abort the terminal even if a
        // future `bind` regressed the guard.
        //
        // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD91. The
        // guarantee does not hold: `keysymFromTrigger` is also null for the 44
        // `input.Key` values GTK's keymap does not map (counting `unidentified`;
        // e.g. `audio_volume_up`,
        // `browser_back`, `intl_backslash`), so `keybind = global:<one of them>`
        // traps once a compositor offers `vicinae_hotkey_manager_v1`. The
        // `NoKeysym` return takes the reference's own `bind` loop warning and
        // skips just this entry.
        let Some(keysym) = key::keysym_from_trigger(self.trigger) else {
            return Err(BindError::NoKeysym);
        };

        // var desc_buf: [256]u8 = undefined;
        // const desc = std.fmt.bufPrintZ(&desc_buf, "{f}", .{self.action}) catch "";
        //
        // CORRECTION (task T-D, S61-F5): a PORT NOTE here said "rustty's formatter
        // allocates, so no description is ever truncated to \"\"", and the full
        // description was sent. The reference's 256-byte buffer is reproduced by
        // [`bind_description`].
        let desc = bind_description(&self.action);

        // `manager.bind(keysym, .{...}, null, app_id.ptr, desc.ptr)` (`:46-55`).
        // `seat = null` — "request it on all of the client's seats", which is what
        // the protocol tells most clients to pass.
        let mut mods = Modifiers::empty();
        if self.trigger.mods.shift {
            mods |= Modifiers::Shift;
        }
        if self.trigger.mods.ctrl {
            mods |= Modifiers::Ctrl;
        }
        if self.trigger.mods.alt {
            mods |= Modifiers::Alt;
        }
        if self.trigger.mods.super_ {
            mods |= Modifiers::Super;
        }

        let hotkey = manager.bind(
            keysym,
            mods,
            None,
            app_id.to_owned(),
            desc,
            qh,
            // `hotkey.setListener(*Entry, hotkeyListener, self)` (`:64`) — in
            // wayland-rs the listener IS the `Dispatch` impl selected by this
            // queue handle + userdata pair, so binding and listener-install are
            // one step.
            self.id,
        );
        self.hotkey = Some(hotkey);
        Ok(())
    }

    /// PORTS: `Hotkeys.zig:68-80` `fail`.
    fn fail(&mut self, message: Vec<u8>, revoked: bool) -> Option<Emission> {
        // `self.hotkey.?.destroy(); self.hotkey = null;` (`:69-70`). `take()`
        // rather than the reference's `.?` unwrap: a compositor is wire-free to
        // send `denied` twice, and aborting on a peer's malformed sequence is not
        // acceptable in a terminal.
        if let Some(hotkey) = self.hotkey.take() {
            hotkey.destroy();
        }

        // `self.shortcuts.emitBindFailed(&.{ .trigger, .action, .message,
        // .revoked })` (`:72-77`) — deferred, see the module docs.
        let shortcuts = self.shortcuts.upgrade()?;
        Some(Emission::BindFailed(
            shortcuts,
            BindFailed {
                trigger: self.trigger,
                action: self.action.clone(),
                message,
                revoked,
            },
        ))
    }
}

/// Why an [`Entry::bind`] request could not be issued. PORTS the reference's
/// inferred `!void` error set, whose only member is the `keysymFromTrigger`
/// `unreachable` (`Hotkeys.zig:41`); the port makes it an error (see there).
#[derive(Debug, Clone, Copy)]
enum BindError {
    /// The trigger has no keysym representation.
    NoKeysym,
}

/// PORTS: `Hotkeys.zig` (the `@This()` struct) — the entry table.
///
/// The reference stores this in `wayland.App` (`wayland.zig:31`); rustty stores it
/// in [`Globals`] because that is the wayland-rs dispatch state (module docs).
#[derive(Default)]
pub struct Hotkeys {
    /// PORTS: `Hotkeys.zig:17` `app_id: [:0]const u8` (the reference `dupeZ`s it
    /// in `init`, `:84`; Rust owns the `String`).
    app_id: String,

    /// PORTS: `Hotkeys.zig:18` `entries: std.ArrayList(Entry)`.
    entries: Vec<Entry>,

    /// The [`HotkeyId`] counter (rustty-only; module docs).
    next_id: u64,

    /// Emissions produced by the dispatch handler, drained after the pump borrow
    /// is released (module docs).
    pending: Vec<Emission>,

    /// OBSERVABILITY (not in the reference): how many `bound` events were
    /// dispatched. Sibling of `Globals::activation_done_count` and there for the
    /// same reason — `bound` has no state effect at all (the reference only logs
    /// it, `Hotkeys.zig:145`), so without this counter a wire test could only
    /// assert `bound_count() == entries`, which is ALREADY true the instant
    /// [`Entry::bind`] issued the request and would therefore pass on a
    /// compositor that never answered. Read through [`Hotkeys::bound_event_count`].
    bound_events: u32,
}

impl std::fmt::Debug for Hotkeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hotkeys")
            .field("app_id", &self.app_id)
            .field("entries", &self.entries.len())
            .finish()
    }
}

impl Hotkeys {
    /// PORTS: `Hotkeys.zig:83-88` `init`.
    #[must_use]
    pub fn new(app_id: &str) -> Hotkeys {
        Hotkeys {
            app_id: app_id.to_owned(),
            ..Hotkeys::default()
        }
    }

    /// PORTS: `Hotkeys.zig:90-96` `deinit` — "Must leave the entries in a valid
    /// empty state: clear may still be called after deinit during application
    /// teardown." Rust frees `app_id` on drop, so the reference's
    /// `alloc.free(self.app_id)` has no counterpart.
    pub fn deinit(&mut self) {
        // `for (self.entries.items) |*entry| entry.deinit();`
        for entry in &mut self.entries {
            entry.deinit();
        }
        // `self.entries.clearAndFree(self.alloc);` — full teardown: release the
        // list's backing allocation (fd47b15cd, the DebugAllocator leak fix),
        // leaving a valid empty list behind.
        self.entries = Vec::new();
    }

    /// PORTS: `Hotkeys.zig:97-100` `clear`.
    pub fn clear(&mut self) {
        for entry in &mut self.entries {
            entry.deinit();
        }
        self.entries.clear();
    }

    /// PORTS: `Hotkeys.zig:102-136` `bind`.
    pub fn bind(
        &mut self,
        manager: &VicinaeHotkeyManagerV1,
        qh: &QueueHandle<Globals>,
        shortcuts: &GlobalShortcuts,
        config: &CoreConfig,
    ) {
        // `self.clear()` (`:108`).
        self.clear();

        // The entry-selection half (`:110-129`), factored out so it is testable
        // with no compositor — see [`select_entries`].
        for (trigger, action) in select_entries(config) {
            let id = HotkeyId(self.next_id);
            self.next_id += 1;
            // `self.entries.append(self.alloc, .{ .hotkey = null, ... }) catch {}`
            // (`:124-129`): allocation failure silently drops the binding; Rust's
            // `push` aborts on OOM, which is the workspace-wide allocator contract.
            self.entries.push(Entry {
                id,
                hotkey: None,
                trigger,
                action,
                shortcuts: {
                    let w = glib::WeakRef::new();
                    w.set(Some(shortcuts));
                    w
                },
            });
        }

        // `for (self.entries.items) |*entry| entry.bind(...) catch |err|
        // log.warn(...)` (`:131-135`).
        let app_id = std::mem::take(&mut self.app_id);
        for entry in &mut self.entries {
            if let Err(err) = entry.bind(manager, qh, &app_id) {
                // log.warn("failed to request hotkey trigger={f} err={}", .{
                //     entry.trigger,
                //     err,
                // });
                //
                // CORRECTION (task T-D, S61-F4): this printed
                // `failed to request hotkey err={err:?}` (no trigger) through an
                // `eprintln!("warn(gtk): ...")` bypass.
                log::warn!(target: LOG_SCOPE, "failed to request hotkey trigger={} err={err:?}",
                    trigger_desc(&entry.trigger));
            }
        }
        self.app_id = app_id;
    }

    /// PORTS: `Hotkeys.zig:138-164` `hotkeyListener`, routed by [`HotkeyId`]
    /// instead of the reference's `*Entry` userdata (module docs).
    fn on_event(&mut self, id: HotkeyId, event: vicinae_hotkey_v1::Event) {
        let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) else {
            // A late event for an entry `clear` already dropped. The reference
            // cannot reach this state (it destroys the proxy, and destroyed
            // proxies stop dispatching) and neither can rustty; ignoring is the
            // same no-op either way.
            return;
        };

        match event {
            // `.bound => log.debug("hotkey bound action={f}", ...)` (`:145`).
            vicinae_hotkey_v1::Event::Bound => {
                // .bound => log.debug("hotkey bound action={f}", .{entry.action}),
                log::debug!(target: LOG_SCOPE, "hotkey bound action={}", desc(&entry.action));
                self.bound_events = self.bound_events.saturating_add(1);
            }

            // `.denied => |v| { log.warn(...); entry.fail(v.message, false); }`
            // (`:147-154`).
            vicinae_hotkey_v1::Event::Denied { reason, message } => {
                // log.warn("hotkey denied action={f} reason={} message={s}", .{
                //     entry.action,
                //     v.reason,
                //     v.message,
                // });
                log::warn!(target: LOG_SCOPE, "hotkey denied action={} reason={:?} message={}",
                    desc(&entry.action),
                    reason.into_result().map(|r| r as u32),
                    message,);
                let emission = entry.fail(message.into_bytes(), false);
                self.pending.extend(emission);
            }

            // `.revoked => |v| { log.warn(...); entry.fail(v.message, true); }`
            // (`:156-163`).
            vicinae_hotkey_v1::Event::Revoked { reason, message } => {
                // log.warn("hotkey revoked action={f} reason={} message={s}", .{
                //     entry.action,
                //     v.reason,
                //     v.message,
                // });
                log::warn!(target: LOG_SCOPE, "hotkey revoked action={} reason={:?} message={}",
                    desc(&entry.action),
                    reason.into_result().map(|r| r as u32),
                    message,);
                let emission = entry.fail(message.into_bytes(), true);
                self.pending.extend(emission);
            }

            // `.pressed => entry.shortcuts.emitTrigger(&entry.action)` (`:165`),
            // deferred out of the dispatch borrow (module docs).
            vicinae_hotkey_v1::Event::Pressed { .. } => {
                if let Some(gs) = entry.shortcuts.upgrade() {
                    let action = entry.action.clone();
                    self.pending.push(Emission::Trigger(gs, action));
                }
            }

            // `.released => {}` (`:166`).
            //
            // The generated event enum is exhaustive over version 1, exactly like
            // the reference's switch; a future protocol bump that adds an event
            // will make this match fail to compile, which is the correct alarm for
            // a testing-phase protocol.
            vicinae_hotkey_v1::Event::Released { .. } => {}
        }
    }

    /// Take the queued emissions. Called by `queue_source`'s trampoline once the
    /// pump borrow is dropped (module docs). rustty-only.
    pub fn take_pending(&mut self) -> Vec<Emission> {
        std::mem::take(&mut self.pending)
    }

    /// The number of live (not denied/revoked) bindings — rustty-only observability
    /// so a KAT can assert that binding actually took effect rather than that no
    /// error was logged.
    #[must_use]
    pub fn bound_count(&self) -> usize {
        self.entries.iter().filter(|e| e.hotkey.is_some()).count()
    }

    /// The number of entries the last [`bind`](Self::bind) selected, live or not.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// The number of `bound` events dispatched since construction (rustty-only
    /// observability; see the field docs for why `bound_count` alone cannot stand
    /// in for it).
    #[must_use]
    pub fn bound_event_count(&self) -> u32 {
        self.bound_events
    }
}

/// PORTS: `Hotkeys.zig:110-129` — the entry-SELECTION half of `bind`, split out
/// because it is pure over a [`CoreConfig`] and needs no compositor. That is where
/// essentially all of the porting risk in `Hotkeys` lives, so it is unit-tested
/// directly (see this module's tests) rather than only through the wire path.
///
/// Selection order is the config's binding order and is preserved, because it is
/// the order the requests are issued in.
#[must_use]
pub fn select_entries(config: &CoreConfig) -> Vec<(Trigger, Action)> {
    let mut out = Vec::new();

    for (trigger, value) in config.keybind.set.iter_bindings() {
        // `.leader => continue`, `inline .leaf, .leaf_chained => |leaf| leaf.generic()`
        // (`:113-116`).
        let leaf = match value {
            Value::Leader(_) => continue,
            Value::Leaf(l) => l.generic(),
            Value::LeafChained(l) => l.generic(),
        };

        // `if (!leaf.flags.global) continue;` (`:117`).
        if !leaf.flags.global {
            continue;
        }

        // "Catch all global keybinds don't really make sense" (`:118-119`).
        if trigger.key == TriggerKey::CatchAll {
            continue;
        }

        // "Only single-action global keybinds are supported, as in the portal
        // implementation." (`:121-123`).
        let actions = leaf.actions_slice();
        if actions.len() != 1 {
            continue;
        }

        out.push((*trigger, actions[0].clone()));
    }

    out
}

/// The action in its parse/format spelling, for the log lines the reference writes
/// with `{f}`.
fn desc(action: &Action) -> String {
    crate::gtk::class::global_shortcuts::action_description(action)
}

/// `{f}` of a `Binding.Trigger` (`Binding.zig:2062-2080` `Trigger.format`).
fn trigger_desc(trigger: &Trigger) -> String {
    let mut w = rustty_cfg_fmt::AllocatingWriter::init();
    let _ = trigger.format(&mut w);
    String::from_utf8_lossy(w.written()).into_owned()
}

/// `var desc_buf: [256]u8` (`Hotkeys.zig:42`).
const DESC_BUF_LEN: usize = 256;

/// `std.fmt.bufPrintZ(&desc_buf, "{f}", .{self.action}) catch ""`
/// (`Hotkeys.zig:42-43`): the action's `{f}` text when it fits the 256-byte
/// buffer WITH its NUL sentinel (255 bytes at most), else the empty description.
/// The limit is on the formatted BYTES, measured before any UTF-8 decoding.
fn bind_description(action: &Action) -> String {
    let mut w = rustty_cfg_fmt::AllocatingWriter::init();
    let _ = action.format(&mut w);
    let bytes = w.written();
    if bytes.len() < DESC_BUF_LEN {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        String::new()
    }
}

/// PORTS: `Hotkeys.zig:138-167` `hotkeyListener` — the listener install
/// (`:64` `setListener`) and the listener body are ONE thing in wayland-rs: this
/// `Dispatch` impl is what `manager.bind(.., qh, id)` selects.
impl Dispatch<VicinaeHotkeyV1, HotkeyId> for Globals {
    fn event(
        state: &mut Self,
        _proxy: &VicinaeHotkeyV1,
        event: vicinae_hotkey_v1::Event,
        id: &HotkeyId,
        _conn: &Connection,
        _qh: &QueueHandle<Globals>,
    ) {
        state.hotkeys_mut().on_event(*id, event);
    }
}

// The manager itself emits no events (`vicinae-hotkey-v1.xml`: the interface has
// requests only), so a no-op dispatch is the whole listener surface.
wayland_client::delegate_noop!(Globals: ignore VicinaeHotkeyManagerV1);

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(binds: &[&str]) -> CoreConfig {
        let mut config = CoreConfig::default_config();
        for b in binds {
            config
                .keybind
                .set
                .parse_and_put(b.as_bytes())
                .expect("keybind parses");
        }
        config
    }

    fn descs(selected: &[(Trigger, Action)]) -> Vec<String> {
        selected.iter().map(|(_, a)| desc(a)).collect()
    }

    /// SERVER-side bindings for `vicinae_hotkey_v1`, generated by `wayland-scanner`
    /// from the SAME vendored `protocols/vicinae-hotkey-v1.xml` the client bindings
    /// in `super::super::vicinae` are generated from. `wayland-server` 0.31 is the
    /// sibling of the vendored `wayland-client` 0.31, so the two halves speak the
    /// same wire by construction rather than by a hand-written encoder that could
    /// agree with a mis-generated client.
    ///
    /// DEV-ONLY: `wayland-server` is a dev-dependency and this module is
    /// `#[cfg(test)]`, so nothing here reaches the shipped dependency graph.
    mod fake_compositor_proto {
        // Machine-written, exactly like the client half (`vicinae.rs`).
        #![allow(missing_docs, dead_code, clippy::all, clippy::pedantic)]

        use wayland_server;
        use wayland_server::protocol::*;

        pub mod __interfaces {
            use wayland_server::protocol::__interfaces::*;
            wayland_scanner::generate_interfaces!("protocols/vicinae-hotkey-v1.xml");
        }
        use self::__interfaces::*;

        wayland_scanner::generate_server_code!("protocols/vicinae-hotkey-v1.xml");
    }

    /// One `vicinae_hotkey_manager_v1.bind` request as the fake compositor received
    /// it off the wire.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct WireBind {
        keysym: u32,
        modifiers: u32,
        /// `seat` is `allow-null`; the port passes `None` ("all of the client's
        /// seats", `Entry::bind`), which must survive the round trip.
        seat_is_null: bool,
        app_id: String,
        description: String,
    }

    /// The fake compositor: it advertises `vicinae_hotkey_manager_v1`, records every
    /// `bind` it is asked for, and keeps the server-side hotkey resources so the
    /// test can drive `bound` / `pressed` / `denied` / `revoked` back at the client.
    #[derive(Default)]
    struct FakeCompositor {
        binds: Vec<WireBind>,
        hotkeys: Vec<fake_compositor_proto::vicinae_hotkey_v1::VicinaeHotkeyV1>,
        /// How many `vicinae_hotkey_v1.destroy` requests arrived — the wire-visible
        /// half of `Entry::fail`'s `hotkey.destroy()`.
        destroys: usize,
    }

    /// No per-client bookkeeping is needed; the trait's methods all default.
    struct NoClientData;
    impl wayland_server::backend::ClientData for NoClientData {}

    impl
        wayland_server::GlobalDispatch<
            fake_compositor_proto::vicinae_hotkey_manager_v1::VicinaeHotkeyManagerV1,
            (),
        > for FakeCompositor
    {
        fn bind(
            _state: &mut Self,
            _handle: &wayland_server::DisplayHandle,
            _client: &wayland_server::Client,
            resource: wayland_server::New<
                fake_compositor_proto::vicinae_hotkey_manager_v1::VicinaeHotkeyManagerV1,
            >,
            _global_data: &(),
            data_init: &mut wayland_server::DataInit<'_, Self>,
        ) {
            data_init.init(resource, ());
        }
    }

    impl
        wayland_server::Dispatch<
            fake_compositor_proto::vicinae_hotkey_manager_v1::VicinaeHotkeyManagerV1,
            (),
        > for FakeCompositor
    {
        fn request(
            state: &mut Self,
            _client: &wayland_server::Client,
            _resource: &fake_compositor_proto::vicinae_hotkey_manager_v1::VicinaeHotkeyManagerV1,
            request: fake_compositor_proto::vicinae_hotkey_manager_v1::Request,
            _data: &(),
            _dhandle: &wayland_server::DisplayHandle,
            data_init: &mut wayland_server::DataInit<'_, Self>,
        ) {
            use fake_compositor_proto::vicinae_hotkey_manager_v1::Request;
            match request {
                Request::Bind {
                    id,
                    keysym,
                    modifiers,
                    seat,
                    app_id,
                    description,
                } => {
                    let hotkey = data_init.init(id, ());
                    state.binds.push(WireBind {
                        keysym,
                        modifiers: modifiers
                            .into_result()
                            .expect("modifiers must be a known bitfield value")
                            .bits(),
                        seat_is_null: seat.is_none(),
                        app_id,
                        description,
                    });
                    state.hotkeys.push(hotkey);
                }
                Request::Destroy => {}
            }
        }
    }

    impl wayland_server::Dispatch<fake_compositor_proto::vicinae_hotkey_v1::VicinaeHotkeyV1, ()>
        for FakeCompositor
    {
        fn request(
            state: &mut Self,
            _client: &wayland_server::Client,
            _resource: &fake_compositor_proto::vicinae_hotkey_v1::VicinaeHotkeyV1,
            request: fake_compositor_proto::vicinae_hotkey_v1::Request,
            _data: &(),
            _dhandle: &wayland_server::DisplayHandle,
            _data_init: &mut wayland_server::DataInit<'_, Self>,
        ) {
            match request {
                fake_compositor_proto::vicinae_hotkey_v1::Request::Destroy => state.destroys += 1,
            }
        }
    }

    /// Stand in for GDK: read the shared `wl_display` socket and dispatch only the
    /// HOST connection's queue, leaving the events destined for rustty's PRIVATE
    /// registry queue demarshalled-but-undispatched.
    ///
    /// This is the whole reason the harness owns two connections. In production GDK
    /// owns the `wl_display` and reads its fd; rustty's [`Globals`] live on a
    /// guest-mode backend with its own queue, which only
    /// [`super::super::queue_source::WlQueueSource`] ever dispatches (`queue_source.rs`).
    /// A harness that called `read()` on rustty's OWN queue would dispatch it as a
    /// side effect, leave `prepare_read()` reporting "not pending", and the GSource —
    /// the component under test — would never fire at all.
    fn gdk_read(host: &wayland_client::Connection) {
        if let Some(guard) = host.prepare_read() {
            // WouldBlock simply means nothing new has arrived yet.
            let _ = guard.read();
        }
    }

    /// Drive both peers until `cond` holds, or fail.
    ///
    /// Each turn does what the real app's halves do: push the compositor's queued
    /// events onto the socket, let "GDK" demarshal them onto rustty's private queue
    /// ([`gdk_read`]), then run ONE main-loop turn so
    /// [`super::super::queue_source::WlQueueSource`] is what dispatches them and
    /// drains the deferred emissions. Driving the queue directly here would bypass
    /// the GSource and prove strictly less.
    fn turn_until(
        display: &mut wayland_server::Display<FakeCompositor>,
        server: &mut FakeCompositor,
        host: &wayland_client::Connection,
        what: &str,
        mut cond: impl FnMut(&FakeCompositor) -> bool,
    ) {
        let ctx = glib::MainContext::default();
        for _ in 0..3000 {
            if cond(server) {
                return;
            }
            display.flush_clients().expect("compositor flush");
            gdk_read(host);
            ctx.iteration(false);
            // Requests the client sent from inside the GSource dispatch (notably
            // `Entry::fail`'s `hotkey.destroy()`) reach the compositor here.
            let _ = display.dispatch_clients(server);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("timed out waiting for {what}");
    }

    /// F-W4, the WIRE path — V22's whole reason for existing.
    ///
    /// A minimal fake compositor (`wayland-server` 0.31, bindings generated from the
    /// same vendored XML) advertises `vicinae_hotkey_manager_v1` over a socket pair,
    /// the real [`Globals`] registry binds it, [`Hotkeys::bind`] issues the requests,
    /// and the compositor answers with all four events.
    ///
    /// The `denied`/`revoked` arms are the point: `Entry::fail` → `emit_bind_failed`
    /// is the ONLY channel a rejected global bind has, and a wire test that only
    /// proved `bound` would pass with that whole path dead — which is precisely the
    /// silent no-op this series exists to stop. The negative control recorded in
    /// V22 (make `Entry::fail` return `None`) makes exactly the `denied`/`revoked`
    /// assertions below go red.
    ///
    /// Needs NO display and NO compositor, so it is a normal (not `#[ignore]`d,
    /// not headed) unit test. Transport note: a `UnixStream::pair` is used instead
    /// of a filesystem socket + `WAYLAND_DISPLAY`, because mutating the process
    /// environment from one test of a multi-threaded test binary is a data race —
    /// the wire, the XML and both generated halves are identical either way.
    #[test]
    fn hotkeys_wire_path_bound_pressed_denied_revoked() {
        use std::cell::RefCell as StdRefCell;
        use std::os::unix::net::UnixStream;
        use std::rc::Rc;
        use std::sync::Arc;

        use glib::prelude::ObjectExt as _;

        use super::super::queue_source::{Pump, WlQueueSource};
        use crate::gtk::class::command_palette::ActionBoxed;
        use fake_compositor_proto::vicinae_hotkey_manager_v1::VicinaeHotkeyManagerV1 as SrvManager;
        use fake_compositor_proto::vicinae_hotkey_v1::{DenyReason, RevokeReason};

        const APP_ID: &str = "com.mitchellh.rustty";

        // ---- the fake compositor -------------------------------------------------
        let (server_sock, client_sock) = UnixStream::pair().expect("socket pair");
        let mut display: wayland_server::Display<FakeCompositor> =
            wayland_server::Display::new().expect("wayland-server display");
        let mut dh = display.handle();
        dh.create_global::<FakeCompositor, SrvManager, ()>(1, ());
        let _client = dh
            .insert_client(server_sock, Arc::new(NoClientData))
            .expect("insert client");
        let mut server = FakeCompositor::default();

        // ---- the real client, built exactly as `wayland::App::init` builds it ----
        // `host` stands in for GDK: it OWNS the `wl_display` and is the only reader
        // of its socket. rustty's registry connection is then a guest-mode backend
        // over that same foreign display with its own private queue — literally the
        // `Backend::from_foreign_display` line in `wayland::App::init`.
        let host = wayland_client::Connection::from_socket(client_sock).expect("host connection");
        let backend = unsafe {
            wayland_backend::sys::client::Backend::from_foreign_display(
                host.backend().display_ptr(),
            )
        };
        let conn = wayland_client::Connection::from_backend(backend);
        let mut event_queue = conn.new_event_queue::<Globals>();
        let qh = event_queue.handle();
        let registry = conn.display().get_registry(&qh, ());
        let mut globals = Box::new(Globals::new(conn.clone(), qh.clone(), registry, APP_ID));

        // The registry roundtrip (`Globals.zig:84`), hand-interleaved because both
        // peers live on this thread.
        for _ in 0..16 {
            event_queue.flush().expect("client flush");
            display
                .dispatch_clients(&mut server)
                .expect("server dispatch");
            display.flush_clients().expect("server flush");
            gdk_read(&host);
            event_queue
                .dispatch_pending(&mut globals)
                .expect("client dispatch");
            if globals.vicinae_hotkey_manager().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let manager = globals
            .vicinae_hotkey_manager()
            .cloned()
            .expect("the registry must bind the advertised vicinae_hotkey_manager_v1");

        // ---- issue the binds (`App::bind_global_shortcuts`) ----------------------
        let config = config_with(&[
            "global:super+q=new_window",
            "ctrl+shift+t=new_tab",
            "global:super+w=close_surface",
            "global:ctrl+alt+shift+f12=toggle_quick_terminal",
        ]);
        let selected = select_entries(&config);
        assert_eq!(
            selected.len(),
            3,
            "precondition: three global single-action binds are selected"
        );

        let gs = GlobalShortcuts::new();
        globals.hotkeys_mut().bind(&manager, &qh, &gs, &config);
        assert_eq!(globals.hotkeys().entry_count(), 3);

        event_queue.flush().expect("client flush");
        display
            .dispatch_clients(&mut server)
            .expect("server dispatch");

        // ASSERTION 1: one `bind` per selected entry, carrying the keysym and the
        // modifier bits `key::keysym_from_trigger` / the `Modifiers` mapping produce.
        let expected_binds: Vec<WireBind> = selected
            .iter()
            .map(|(trigger, action)| {
                let mut modifiers = 0u32;
                if trigger.mods.shift {
                    modifiers |= 1;
                }
                if trigger.mods.ctrl {
                    modifiers |= 2;
                }
                if trigger.mods.alt {
                    modifiers |= 4;
                }
                if trigger.mods.super_ {
                    modifiers |= 8;
                }
                WireBind {
                    keysym: key::keysym_from_trigger(*trigger).expect("trigger has a keysym"),
                    modifiers,
                    seat_is_null: true,
                    app_id: APP_ID.to_owned(),
                    description: desc(action),
                }
            })
            .collect();
        assert_eq!(
            server.binds, expected_binds,
            "each selected entry must reach the compositor as one bind request \
             carrying its keysym, modifier bits, null seat, app_id and description"
        );

        // ---- hand the queue to the main-loop GSource ----------------------------
        // From here on NOTHING dispatches the queue directly: every event below is
        // dispatched by `WlQueueSource`'s trampoline, which is also what drains the
        // deferred `trigger` / `bind-failed` emissions.
        let pump = Rc::new(StdRefCell::new(Pump {
            event_queue,
            globals,
        }));
        let _source = WlQueueSource::attach(&pump);

        let triggers: Rc<StdRefCell<Vec<Action>>> = Rc::new(StdRefCell::new(Vec::new()));
        let triggers_cb = triggers.clone();
        gs.connect_local("trigger", false, move |args| {
            let boxed: ActionBoxed = args[1].get().expect("trigger carries an ActionBoxed");
            triggers_cb.borrow_mut().push(boxed.0);
            None
        });

        let failures: Rc<StdRefCell<Vec<BindFailed>>> = Rc::new(StdRefCell::new(Vec::new()));
        let failures_cb = failures.clone();
        gs.connect_local("bind-failed", false, move |args| {
            let boxed: BindFailed = args[1].get().expect("bind-failed carries a BindFailed");
            failures_cb.borrow_mut().push(boxed);
            None
        });

        // ---- ASSERTION 2: `bound` ------------------------------------------------
        for hotkey in &server.hotkeys {
            hotkey.bound();
        }
        {
            let p = pump.clone();
            turn_until(
                &mut display,
                &mut server,
                &host,
                "three bound events",
                |_| p.borrow().globals.hotkeys().bound_event_count() == 3,
            );
        }
        assert_eq!(
            pump.borrow().globals.hotkeys().bound_count(),
            3,
            "all three bindings are live after bound"
        );

        // ---- ASSERTION 3: `pressed` re-emits the action on `trigger` -------------
        server.hotkeys[0].pressed(7, 1234);
        {
            let t = triggers.clone();
            turn_until(
                &mut display,
                &mut server,
                &host,
                "a trigger emission",
                |_| !t.borrow().is_empty(),
            );
        }
        assert_eq!(
            *triggers.borrow(),
            vec![selected[0].1.clone()],
            "pressed must re-emit exactly the bound action on the trigger signal, \
             drained by the queue GSource"
        );

        // ---- ASSERTION 4a: `denied` -> bind-failed(revoked = false) --------------
        const DENIED_MSG: &str = "policy forbids this combination";
        server.hotkeys[1].denied(DenyReason::NotPermitted, DENIED_MSG.to_owned());
        {
            let f = failures.clone();
            turn_until(
                &mut display,
                &mut server,
                &host,
                "a bind-failed emission for the denied binding",
                |_| !f.borrow().is_empty(),
            );
        }
        assert_eq!(
            failures.borrow()[0],
            BindFailed {
                trigger: selected[1].0,
                action: selected[1].1.clone(),
                message: DENIED_MSG.as_bytes().to_vec(),
                revoked: false,
            },
            "denied must reach bind-failed with the compositor's message BYTES and \
             revoked = false"
        );
        assert_eq!(
            pump.borrow().globals.hotkeys().bound_count(),
            2,
            "a denied binding is no longer live"
        );

        // ---- ASSERTION 4b: `revoked` -> bind-failed(revoked = true) --------------
        const REVOKED_MSG: &str = "the user removed this shortcut";
        server.hotkeys[0].revoked(RevokeReason::Removed, REVOKED_MSG.to_owned());
        {
            let f = failures.clone();
            turn_until(
                &mut display,
                &mut server,
                &host,
                "a bind-failed emission for the revoked binding",
                |_| f.borrow().len() >= 2,
            );
        }
        assert_eq!(
            failures.borrow()[1],
            BindFailed {
                trigger: selected[0].0,
                action: selected[0].1.clone(),
                message: REVOKED_MSG.as_bytes().to_vec(),
                revoked: true,
            },
            "revoked must reach bind-failed with the compositor's message BYTES and \
             revoked = true"
        );
        assert_eq!(
            pump.borrow().globals.hotkeys().bound_count(),
            1,
            "only the untouched binding is still live"
        );

        // The two failed entries released their objects on the wire
        // (`Entry::fail`'s `hotkey.destroy()`), and nothing else fired.
        turn_until(
            &mut display,
            &mut server,
            &host,
            "both failed hotkeys to be destroyed on the wire",
            |s| s.destroys >= 2,
        );
        assert_eq!(
            triggers.borrow().len(),
            1,
            "no further trigger emissions after the failures"
        );
        assert_eq!(
            failures.borrow().len(),
            2,
            "exactly one bind-failed per failed binding"
        );
    }

    /// UD91 (R13, task R13-D): a `global:` bind on one of the physical keys GTK's
    /// keymap has no keysym for (`audio_volume_up` here; 44 `input.Key` values
    /// at the pin, counting `unidentified`). The reference's `Entry.bind` says only `catch_all` lacks a keysym
    /// and spells it `orelse unreachable` (`Hotkeys.zig:40`), so such a config
    /// line traps once a compositor offers `vicinae_hotkey_manager_v1`. The port
    /// returns `NoKeysym`, which the reference's own `bind` loop logs and skips:
    /// the other binds still go out. Wire-level, as the test above.
    #[test]
    fn hotkeys_bind_skips_a_trigger_with_no_keysym() {
        use std::os::unix::net::UnixStream;
        use std::sync::Arc;

        use fake_compositor_proto::vicinae_hotkey_manager_v1::VicinaeHotkeyManagerV1 as SrvManager;

        let (server_sock, client_sock) = UnixStream::pair().expect("socket pair");
        let mut display: wayland_server::Display<FakeCompositor> =
            wayland_server::Display::new().expect("wayland-server display");
        let mut dh = display.handle();
        dh.create_global::<FakeCompositor, SrvManager, ()>(1, ());
        let _client = dh
            .insert_client(server_sock, Arc::new(NoClientData))
            .expect("insert client");
        let mut server = FakeCompositor::default();

        let host = wayland_client::Connection::from_socket(client_sock).expect("host connection");
        let backend = unsafe {
            wayland_backend::sys::client::Backend::from_foreign_display(
                host.backend().display_ptr(),
            )
        };
        let conn = wayland_client::Connection::from_backend(backend);
        let mut event_queue = conn.new_event_queue::<Globals>();
        let qh = event_queue.handle();
        let registry = conn.display().get_registry(&qh, ());
        let mut globals = Box::new(Globals::new(
            conn.clone(),
            qh.clone(),
            registry,
            "com.mitchellh.rustty",
        ));
        for _ in 0..16 {
            event_queue.flush().expect("client flush");
            display
                .dispatch_clients(&mut server)
                .expect("server dispatch");
            display.flush_clients().expect("server flush");
            gdk_read(&host);
            event_queue
                .dispatch_pending(&mut globals)
                .expect("client dispatch");
            if globals.vicinae_hotkey_manager().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let manager = globals
            .vicinae_hotkey_manager()
            .cloned()
            .expect("the registry must bind the advertised vicinae_hotkey_manager_v1");

        let config = config_with(&[
            "global:audio_volume_up=new_window",
            "global:super+q=new_tab",
        ]);
        let selected = select_entries(&config);
        assert_eq!(2, selected.len(), "precondition: both binds are selected");
        let no_keysym: Vec<_> = selected
            .iter()
            .filter(|(t, _)| key::keysym_from_trigger(*t).is_none())
            .collect();
        assert_eq!(
            1,
            no_keysym.len(),
            "precondition: exactly one selected trigger has no keysym"
        );
        assert!(matches!(no_keysym[0].0.key, TriggerKey::Physical(_)));

        let gs = GlobalShortcuts::new();
        let cap = crate::log_capture::start();
        globals.hotkeys_mut().bind(&manager, &qh, &gs, &config);
        let messages = cap.messages();
        drop(cap);
        assert_eq!(2, globals.hotkeys().entry_count());

        // S61-F4: the reference's bind-loop warning names the TRIGGER
        // (`Hotkeys.zig:131-135`, `trigger={f}`), under the file's scope, once, for
        // the skipped bind only.
        assert_eq!(
            messages,
            [(
                log::Level::Warn,
                LOG_SCOPE.to_owned(),
                "failed to request hotkey trigger=audio_volume_up err=NoKeysym".to_owned(),
            )]
        );

        event_queue.flush().expect("client flush");
        display
            .dispatch_clients(&mut server)
            .expect("server dispatch");
        let q = selected
            .iter()
            .find_map(|(t, _)| key::keysym_from_trigger(*t))
            .expect("super+q has a keysym");
        assert_eq!(
            vec![q],
            server.binds.iter().map(|b| b.keysym).collect::<Vec<_>>(),
            "only the bind with a keysym reaches the compositor"
        );
    }

    /// S61-F5: `Entry.bind`'s description is `bufPrintZ` into a 256-byte buffer,
    /// `catch ""` (`Hotkeys.zig:42-43`): 255 formatted bytes are sent, 256 become
    /// the empty description. The boundary is found by formatting, not assumed.
    #[test]
    fn hotkeys_bind_description_fits_the_reference_buffer() {
        let text = |n: usize| Action::Text(vec![b'x'; n]);
        // The `text:` action's `{f}` form, measured at two lengths so the payload
        // length that lands on exactly 255 formatted bytes can be solved for.
        let overhead = desc(&text(1)).len() - 1;
        assert_eq!(desc(&text(2)).len(), overhead + 2, "precondition: linear");
        let at_255 = text(255 - overhead);
        let at_256 = text(256 - overhead);
        assert_eq!(desc(&at_255).len(), 255);
        assert_eq!(desc(&at_256).len(), 256);

        assert_eq!(bind_description(&at_255), desc(&at_255));
        assert_eq!(bind_description(&at_256), "");
    }

    /// S61-F5, on the wire: an over-long action reaches the compositor with the
    /// EMPTY description, and a short one with its full `{f}` text.
    #[test]
    fn hotkeys_bind_sends_an_empty_description_for_an_over_long_action() {
        use std::os::unix::net::UnixStream;
        use std::sync::Arc;

        use fake_compositor_proto::vicinae_hotkey_manager_v1::VicinaeHotkeyManagerV1 as SrvManager;

        let (server_sock, client_sock) = UnixStream::pair().expect("socket pair");
        let mut display: wayland_server::Display<FakeCompositor> =
            wayland_server::Display::new().expect("wayland-server display");
        let mut dh = display.handle();
        dh.create_global::<FakeCompositor, SrvManager, ()>(1, ());
        let _client = dh
            .insert_client(server_sock, Arc::new(NoClientData))
            .expect("insert client");
        let mut server = FakeCompositor::default();

        let host = wayland_client::Connection::from_socket(client_sock).expect("host connection");
        let backend = unsafe {
            wayland_backend::sys::client::Backend::from_foreign_display(
                host.backend().display_ptr(),
            )
        };
        let conn = wayland_client::Connection::from_backend(backend);
        let mut event_queue = conn.new_event_queue::<Globals>();
        let qh = event_queue.handle();
        let registry = conn.display().get_registry(&qh, ());
        let mut globals = Box::new(Globals::new(
            conn.clone(),
            qh.clone(),
            registry,
            "com.mitchellh.rustty",
        ));
        for _ in 0..16 {
            event_queue.flush().expect("client flush");
            display
                .dispatch_clients(&mut server)
                .expect("server dispatch");
            display.flush_clients().expect("server flush");
            gdk_read(&host);
            event_queue
                .dispatch_pending(&mut globals)
                .expect("client dispatch");
            if globals.vicinae_hotkey_manager().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let manager = globals
            .vicinae_hotkey_manager()
            .cloned()
            .expect("the registry must bind the advertised vicinae_hotkey_manager_v1");

        let long = format!("global:super+a=text:{}", "x".repeat(300));
        let config = config_with(&[&long, "global:super+b=new_tab"]);
        let selected = select_entries(&config);
        assert_eq!(2, selected.len(), "precondition: both binds are selected");

        let gs = GlobalShortcuts::new();
        globals.hotkeys_mut().bind(&manager, &qh, &gs, &config);
        event_queue.flush().expect("client flush");
        display
            .dispatch_clients(&mut server)
            .expect("server dispatch");

        assert_eq!(2, server.binds.len(), "both binds reach the compositor");
        let mut descriptions: Vec<&str> = server
            .binds
            .iter()
            .map(|b| b.description.as_str())
            .collect();
        descriptions.sort_unstable();
        assert_eq!(descriptions, ["", "new_tab"]);
    }

    /// F-W3's codegen gate: the bindings really were generated from the vendored
    /// XML, with the wire names and version the compositor will be matched on. The
    /// interface NAME is what `Globals::bind_global` compares the registry's
    /// advertised string against, so a codegen that silently produced a differently
    /// named interface would make the arm permanently inert with no other symptom.
    #[test]
    fn hotkeys_generated_interfaces_match_the_vendored_xml() {
        use wayland_client::Proxy as _;

        assert_eq!(
            VicinaeHotkeyManagerV1::interface().name,
            "vicinae_hotkey_manager_v1"
        );
        assert_eq!(VicinaeHotkeyManagerV1::interface().version, 1);
        assert_eq!(VicinaeHotkeyV1::interface().name, "vicinae_hotkey_v1");
        assert_eq!(VicinaeHotkeyV1::interface().version, 1);

        // The modifier bits are wire values (`vicinae-hotkey-v1.xml`
        // `modifiers`: shift=1 ctrl=2 alt=4 super=8), not an ordering the
        // generator is free to choose.
        assert_eq!(Modifiers::Shift.bits(), 1);
        assert_eq!(Modifiers::Ctrl.bits(), 2);
        assert_eq!(Modifiers::Alt.bits(), 4);
        assert_eq!(Modifiers::Super.bits(), 8);
    }

    /// PORTS: `Hotkeys.zig:110-129`. Only `global:` leaves are selected, and the
    /// config's order is preserved (it is the order the bind requests go out in).
    #[test]
    fn hotkeys_select_entries_takes_only_global_leaves() {
        let config = config_with(&[
            "ctrl+shift+t=new_tab",
            "global:super+q=new_window",
            "global:super+w=close_surface",
        ]);
        assert_eq!(
            descs(&select_entries(&config)),
            vec!["new_window".to_owned(), "close_surface".to_owned()],
            "non-global binds must be skipped and global order preserved"
        );
    }

    /// "Catch all global keybinds don't really make sense"
    /// (`Hotkeys.zig:118-119`). The catch-all bind below IS global and IS
    /// single-action, so it passes every OTHER filter — only the `catch_all` guard
    /// rejects it. Asserted against a control bind that must survive, so a
    /// select_entries that returned nothing at all cannot pass.
    #[test]
    fn hotkeys_select_entries_skips_catch_all() {
        // Precondition: the catch-all bind really is present, global and single —
        // otherwise this test would be vacuous (it would pass for the wrong reason).
        let config = config_with(&["global:catch_all=new_window", "global:super+q=new_tab"]);
        let global_leaves = config
            .keybind
            .set
            .iter_bindings()
            .filter(|(_, v)| match v {
                Value::Leaf(l) => l.flags.global,
                Value::LeafChained(l) => l.flags.global,
                Value::Leader(_) => false,
            })
            .count();
        assert_eq!(
            global_leaves, 2,
            "precondition: both binds are global leaves, so only the catch_all \
             guard can reject one of them"
        );

        assert_eq!(
            descs(&select_entries(&config)),
            vec!["new_tab".to_owned()],
            "the catch_all global bind must be skipped and the other kept"
        );
    }

    /// "Only single-action global keybinds are supported, as in the portal
    /// implementation." (`Hotkeys.zig:121-123`). `chain=` appends a second action
    /// to the previous bind, turning its leaf into a `leaf_chained` while KEEPING
    /// its `global` flag — the exact shape the guard exists for.
    #[test]
    fn hotkeys_select_entries_skips_multi_action() {
        let single = config_with(&["global:super+q=new_window"]);
        assert_eq!(
            descs(&select_entries(&single)),
            vec!["new_window".to_owned()]
        );

        let multi = config_with(&["global:super+q=new_window", "chain=new_tab"]);
        // Precondition: THAT bind (not just any binding in the default config)
        // really did become a global multi-action leaf.
        let want = Trigger {
            key: TriggerKey::Unicode('q' as u32),
            mods: rustty_vt::input::key_mods::Mods {
                super_: true,
                ..Default::default()
            },
        };
        let (_, value) = multi
            .keybind
            .set
            .iter_bindings()
            .find(|(t, _)| **t == want)
            .expect("the super+q binding exists");
        match value {
            Value::LeafChained(l) => {
                assert!(l.flags.global, "precondition: chained leaf stays global");
                assert_eq!(l.actions.len(), 2, "precondition: two actions");
            }
            other => panic!("precondition: expected a leaf_chained, got {other:?}"),
        }

        assert!(
            select_entries(&multi).is_empty(),
            "a global bind with more than one action must not be selected"
        );
    }

    /// PORTS: fd47b15cd "gtk: free hotkeys memory on app teardown" — `deinit`
    /// must release the entries list's backing allocation (the reference's
    /// `clearAndFree`), not just clear it. The reference's proof is the
    /// DebugAllocator leak report on teardown; the Rust-observable equivalent
    /// is the `Vec`'s capacity going to zero on deinit.
    #[test]
    fn hotkeys_deinit_frees_entries_storage() {
        let config = config_with(&["global:super+q=new_window"]);
        let (trigger, action) = select_entries(&config)
            .into_iter()
            .next()
            .expect("one global bind selected");

        let mut hotkeys = Hotkeys::new("com.mitchellh.rustty");
        hotkeys.entries.push(Entry {
            id: HotkeyId(0),
            hotkey: None,
            trigger,
            action,
            shortcuts: glib::WeakRef::new(),
        });
        assert!(
            hotkeys.entries.capacity() > 0,
            "precondition: the entries list holds a live allocation"
        );

        hotkeys.deinit();
        // "Must leave the entries in a valid empty state: clear may still be
        // called after deinit during application teardown."
        assert!(
            hotkeys.entries.is_empty(),
            "deinit leaves a valid empty state"
        );
        hotkeys.clear();
        assert_eq!(
            hotkeys.entries.capacity(),
            0,
            "deinit must free the entries' backing allocation (the reference's clearAndFree)"
        );
    }
}
