//! rustty-apprt::gtk::class::config
//!
//! PORTS: ghostty `src/apprt/gtk/class/config.zig` (175 loc). A `glib::Object`
//! subclass (`GhosttyConfig`) that wraps a `rustty_config::Config` **value** so
//! the tier's ~12 `?*Config` fields can share one refcounted object
//! (PREPORT_GTK §1.8, `config.zig:69`: `Config.Private.config: CoreConfig` is
//! class-OWNED). When the object is finalized, the wrapped config is dropped
//! with it (`config.zig:136-143` `finalize` -> `config.deinit()`; in Rust the
//! embedded value's `Drop` runs automatically at subclass finalize — there is
//! no GObject reference held in this class, so no explicit `dispose` unref is
//! required, unlike the SHARED `?*Config` fields elsewhere in the tier).
//!
//! # Idiom map (`class.zig` -> gtk4-rs, see [`super`])
//!
//! * `Common(Self, Private)` (`config.zig:145`) -> `#[glib::object_subclass]`
//!   on the `imp` `ObjectSubclass` impl; `Private` (`config.zig:68-72`) -> the
//!   `imp::Config` instance struct.
//! * `gobject.ext.defineClass(.{ .name = "GhosttyConfig" })` (`config.zig:27`)
//!   -> `const NAME: &str = "GhosttyConfig"` (the GType name is preserved 1:1
//!   so the registered type matches the reference).
//! * `ref`/`unref` (`config.zig:147-148`) -> automatic: the gtk4-rs wrapper's
//!   `Clone`/`Drop` refcount (class doc Invariant 12).
//! * the two read-only computed properties `has-diagnostics` /
//!   `diagnostics-buffer` (`config.zig:34-66`, registered `:158-161`) ->
//!   `#[derive(glib::Properties)]` with custom getters on `PhantomData` marker
//!   fields (no backing storage — both are derived from the wrapped config).
//!   `diagnostics-buffer`'s `getter_transfer = .full` (`config.zig:46`) is the
//!   default for an owned `Option<gtk4::TextBuffer>` return — the caller
//!   receives a fresh buffer whose one strong ref it owns.

use glib::prelude::*;
use glib::subclass::prelude::*;

use rustty_config::Config as CoreConfig;

mod imp {
    use super::*;
    use std::cell::OnceCell;
    use std::marker::PhantomData;

    /// PORTS: `config.zig:68-72` `Config.Private`. The wrapped config is stored
    /// in a `OnceCell` so `get` can hand back a `&CoreConfig` borrowed from the
    /// instance (matching the oracle's `get(self) *const CoreConfig`,
    /// `config.zig:90-92`): the value is set exactly once in `new` (the
    /// reference clones into the freshly-`newInstance`'d private, `config.zig:79
    /// -83`) and never reassigned, so a set-once cell is the faithful, and
    /// borrow-checker-friendly, spelling. The two `PhantomData` fields carry no
    /// storage; they only anchor the derived, computed properties.
    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::Config)]
    pub struct Config {
        /// PORTS: `config.zig:69` `config: CoreConfig` (class-OWNED value).
        pub config: OnceCell<CoreConfig>,

        /// PORTS: `config.zig:51-65` property `has-diagnostics` (read-only,
        /// getter `hasDiagnostics`).
        #[property(get = Self::has_diagnostics)]
        has_diagnostics: PhantomData<bool>,

        /// PORTS: `config.zig:35-49` property `diagnostics-buffer` (read-only,
        /// nullable, getter `diagnosticsBuffer`, `getter_transfer = .full`).
        #[property(get = Self::diagnostics_buffer, nullable)]
        diagnostics_buffer: PhantomData<Option<gtk4::TextBuffer>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Config {
        // PORTS: `config.zig:28` `.name = "GhosttyConfig"`. Preserve the GType
        // name verbatim so the registered type is identical to the reference.
        const NAME: &'static str = "GhosttyConfig";
        type Type = super::Config;
        // ParentType defaults to `glib::Object` (`config.zig:26` `Parent =
        // gobject.Object`).
    }

    #[glib::derived_properties]
    impl ObjectImpl for Config {}

    impl Config {
        /// PORTS: `config.zig:103-106` `hasDiagnostics`. `true` iff the wrapped
        /// config carries any diagnostics.
        fn has_diagnostics(&self) -> bool {
            match self.config.get() {
                // `!config._diagnostics.empty()` (`config.zig:105`).
                Some(config) => !config._diagnostics.empty(),
                // Not-yet-`new`'d (should not happen post-construction): no
                // diagnostics.
                None => false,
            }
        }

        /// PORTS: `config.zig:110-134` `diagnosticsBuffer`. Renders the wrapped
        /// config's diagnostics into a fresh `gtk4::TextBuffer`, one per line,
        /// or `None` when there are none (`config.zig:112`
        /// `if (config._diagnostics.empty()) return null;`). The returned
        /// buffer is owned by the caller (`getter_transfer = .full`).
        fn diagnostics_buffer(&self) -> Option<gtk4::TextBuffer> {
            use gtk4::prelude::TextBufferExt;

            let config = self.config.get()?;
            if config._diagnostics.empty() {
                return None;
            }

            // `gtk.TextBuffer.new(null)` (`config.zig:114`).
            let text_buf = gtk4::TextBuffer::new(None);

            // PORT NOTE: the oracle formats into a fixed `[4095:0]u8` stack
            // buffer (`config.zig:117`) — a Zig stack-size constraint, not
            // semantics — and truncates anything longer. Rust formats into a
            // growable `Vec<u8>` per diagnostic (no truncation), which is
            // behavior-preserving for the common short-diagnostic case and
            // strictly more faithful for long ones. `Diagnostic::format` writes
            // UTF-8; a lossy decode guards the `insert_at_cursor(&str)` binding.
            for diag in config._diagnostics.items() {
                let mut buf: Vec<u8> = Vec::new();
                diag.format(&mut buf);
                let text = String::from_utf8_lossy(&buf);
                // `text_buf.insertAtCursor(&buf, ...)` then `"\n"`
                // (`config.zig:129-130`).
                text_buf.insert_at_cursor(&text);
                text_buf.insert_at_cursor("\n");
            }

            Some(text_buf)
        }
    }
}

glib::wrapper! {
    /// Wraps a loaded [`rustty_config::Config`] in a `glib::Object` so it can be
    /// reference counted and shared across the tier. PORTS: `config.zig:23-164`
    /// `pub const Config`.
    ///
    /// It is highly recommended NOT to hold a reference to this object longer
    /// than necessary — a config is (relatively) large. Receivers that need to
    /// outlive it should copy what they need out of [`Config::get`] instead
    /// (`config.zig:13-22`).
    pub struct Config(ObjectSubclass<imp::Config>);
}

impl Config {
    /// PORTS: `config.zig:78-86` `new`. Creates a new `GhosttyConfig` from a
    /// loaded configuration. This **clones** the given configuration
    /// (`config.zig:83` `priv.config = try config.clone(alloc)`), so it is safe
    /// for the caller to drop the original after this call.
    #[must_use]
    pub fn new(config: &CoreConfig) -> Self {
        // `gobject.ext.newInstance(Self, .{})` (`config.zig:79`) sinks the
        // floating ref via `glib::Object::new` (Invariant 12).
        let obj: Self = glib::Object::new();
        // The private is empty until we set it once here. `clone_config` is the
        // rustty port of `CoreConfig.clone` (a deep, in-memory copy). A freshly
        // `new`'d instance's `OnceCell` is always empty, so this set always
        // succeeds; the returned `Err` carries the un-`Debug` `CoreConfig`, so
        // assert via `is_err` rather than `expect`.
        if obj.imp().config.set(config.clone_config()).is_err() {
            unreachable!("GhosttyConfig::new sets the config exactly once");
        }
        obj
    }

    /// PORTS: `config.zig:90-92` `get`. Borrows the wrapped configuration. It is
    /// unsafe to store this or use it beyond the lifetime of this object; the
    /// borrow is tied to `&self`.
    #[must_use]
    pub fn get(&self) -> &CoreConfig {
        self.imp()
            .config
            .get()
            .expect("GhosttyConfig::get called before new set the config")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PORTS: `config.zig:168` `test "GhosttyConfig"`. Non-display: a plain
    /// `glib::Object` subclass (parent `glib::Object`, not a `Widget`)
    /// instantiates headless — no `gtk4::init`/display required — so this runs
    /// in the normal gate. The oracle asserts no leaks under the testing
    /// allocator; the Rust analogue is that `obj`'s `glib::Object` refcount
    /// drops to zero at scope end, finalizing the subclass and dropping the
    /// embedded `CoreConfig` value.
    #[test]
    fn ghostty_config_new_and_get() {
        let config: CoreConfig = CoreConfig::default_config();
        let obj = Config::new(&config);
        // `get` hands back a borrow of the embedded (cloned) config.
        let _got: &CoreConfig = obj.get();
        drop(obj);
    }

    /// A default config carries no diagnostics, so `has-diagnostics` is `false`
    /// and `diagnostics-buffer` is `None`. Exercises the two computed
    /// properties through the GObject property system (PORTS the getters
    /// `config.zig:103-134` and their registration `:158-161`). Non-display:
    /// reading these properties on a headless plain-`Object` subclass needs no
    /// display — `diagnostics-buffer` returns `None` here and never constructs
    /// a `TextBuffer`.
    #[test]
    fn ghostty_config_no_diagnostics() {
        let config: CoreConfig = CoreConfig::default_config();
        let obj = Config::new(&config);

        assert!(
            obj.get()._diagnostics.empty(),
            "default config is diagnostic-free"
        );
        let has: bool = obj.property("has-diagnostics");
        assert!(!has, "a default config has no diagnostics");
        let buffer: Option<gtk4::TextBuffer> = obj.property("diagnostics-buffer");
        assert!(buffer.is_none(), "no diagnostics -> null buffer");
    }
}
