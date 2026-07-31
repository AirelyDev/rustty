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
        let Some(keysym) = key::keysym_from_trigger(self.trigger) else {
            return Err(BindError::NoKeysym);
        };

        // `std.fmt.bufPrintZ(&desc_buf, "{f}", .{self.action}) catch ""`
        // (`:43-44`). PORT NOTE: the reference formats into a 256-byte stack
        // buffer and substitutes the EMPTY description when the action does not
        // fit; rustty's formatter allocates, so no description is ever truncated
        // to "". The description is advisory compositor-UI text either way.
        let desc = crate::gtk::class::global_shortcuts::action_description(&self.action);

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

    /// PORTS: `Hotkeys.zig:90-95` `deinit` — "Must leave the entries in a valid
    /// empty state: clear may still be called after deinit during application
    /// teardown." `clear` already does exactly that, and Rust frees `app_id` on
    /// drop, so this is `clear` (the reference's `alloc.free(self.app_id)` has no
    /// counterpart).
    pub fn deinit(&mut self) {
        self.clear();
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
                eprintln!("warn(gtk): failed to request hotkey err={err:?}");
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
                eprintln!("debug(gtk): hotkey bound action={}", desc(&entry.action));
            }

            // `.denied => |v| { log.warn(...); entry.fail(v.message, false); }`
            // (`:147-154`).
            vicinae_hotkey_v1::Event::Denied { reason, message } => {
                eprintln!(
                    "warn(gtk): hotkey denied action={} reason={:?} message={}",
                    desc(&entry.action),
                    reason.into_result().map(|r| r as u32),
                    message,
                );
                let emission = entry.fail(message.into_bytes(), false);
                self.pending.extend(emission);
            }

            // `.revoked => |v| { log.warn(...); entry.fail(v.message, true); }`
            // (`:156-163`).
            vicinae_hotkey_v1::Event::Revoked { reason, message } => {
                eprintln!(
                    "warn(gtk): hotkey revoked action={} reason={:?} message={}",
                    desc(&entry.action),
                    reason.into_result().map(|r| r as u32),
                    message,
                );
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

    /// The wire half's VERIFICATION DEBT, made visible in the test list rather than
    /// only in a document (`P3-W8 T7 Step 4`'s "interim marker").
    ///
    /// Recipe: stand up a minimal fake compositor in a `#[test]`-local
    /// `wayland-server` 0.31 (the sibling of the vendored `wayland-client`)
    /// advertising `vicinae_hotkey_manager_v1` on its own socket, point the client at
    /// it with `WAYLAND_DISPLAY`, and assert: `bind` → `bound`; `pressed` → the
    /// `trigger` signal carries the action; `denied` / `revoked` → the `bind-failed`
    /// signal fires carrying the compositor's reason bytes and the right `revoked`
    /// flag. ~a day of scaffolding; scheduled as its own task rather than pretended
    /// to be done. The graceful-ABSENCE half IS verified, on a live GNOME session:
    /// `tests/gtk_wayland_vicinae_hotkeys.rs`.
    #[test]
    #[ignore = "needs fake vicinae compositor"]
    fn hotkeys_wire_path_bound_pressed_denied_revoked() {
        unimplemented!("see this test's doc comment for the runnable recipe");
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
}
