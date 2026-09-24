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

/// `const log = std.log.scoped(.gtk_ghostty_config)` (`config.zig:11`).
///
/// NAMING CALL (task-14d-2). This is the FIRST `LOG_SCOPE` in the port whose
/// oracle spelling contains the product name — the other ~45 carry no brand
/// token — so it sets the precedent. It is REBRANDED, for three reasons:
///
///   1. `.port/tools/PARITY_NAMING.md` lists "runtime identity strings" under
///      "Becomes rustty". A scope tag is emitted into user-visible diagnostic
///      text (`warning(gtk_rustty_config): ...`), which is precisely that; a
///      hardcoded `ghostty` literal in new code is also what the plan's
///      residual-grep rule targets.
///   2. Byte-parity with the oracle's scope tag is not held in this subtree
///      anyway and is not lost here: the surrounding GTK files print
///      `warn(gtk)` where the oracle scope is `gtk_ghostty_surface` /
///      `gtk_ghostty_window`. There is no `identity-oracle` hook for log scopes
///      anywhere in the workspace, so nothing that flips strings for
///      oracle comparison is being bypassed.
///   3. The GType NAME below is deliberately NOT rebranded, and the two are not
///      in tension: `"GhosttyConfig"` is a REGISTERED TYPE IDENTITY that must
///      match the reference's `.name` for the type system to line up, while a
///      log scope is a label with no consumer that keys off it.
///
/// If a later stage decides scope tags should flow through `rustty-identity`
/// so `--features identity-oracle` can flip them back, this constant is the
/// place to start and the change is one line.
const LOG_SCOPE: &str = "gtk_rustty_config";

/// `var buf: [4095:0]u8` (`config.zig:117`). The reference formats EACH
/// diagnostic into this fixed buffer through `std.Io.Writer.fixed`, so a
/// diagnostic longer than 4095 bytes makes `diag.format` fail with
/// `error.WriteFailed` and is SKIPPED with a warning rather than shown
/// truncated. Diagnostic text embeds user-controlled config key/value/file-path
/// text, so the over-long case is reachable (S51-F2).
const DIAGNOSTICS_BUFFER_LEN: usize = 4095;

/// The single error `std.Io.Writer.fixed` can produce here. Zig's `{}` on it
/// renders `error.WriteFailed`; the port's crate-wide convention renders Rust's
/// `Debug`, i.e. `WriteFailed` (N11).
#[derive(Debug, PartialEq, Eq)]
enum DiagnosticFormatError {
    WriteFailed,
}

/// `writer.end = 0; diag.format(&writer)` against that fixed 4095-byte buffer
/// (`config.zig:119-126`). `Diagnostic::format` writes into a growable `Vec`,
/// which cannot fail, so the reference's CAPACITY is what has to be enforced:
/// anything past 4095 bytes is the `error.WriteFailed` arm.
fn format_diagnostic_bounded(
    diag: &rustty_args::diagnostics::Diagnostic,
) -> Result<Vec<u8>, DiagnosticFormatError> {
    let mut buf: Vec<u8> = Vec::new();
    diag.format(&mut buf);
    if buf.len() > DIAGNOSTICS_BUFFER_LEN {
        return Err(DiagnosticFormatError::WriteFailed);
    }
    Ok(buf)
}

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
        /// Returns whether this configuration has any diagnostics.
        ///
        /// PORTS: `config.zig:103-106` `hasDiagnostics`.
        fn has_diagnostics(&self) -> bool {
            match self.config.get() {
                // `!config._diagnostics.empty()` (`config.zig:105`).
                Some(config) => !config._diagnostics.empty(),
                // Not-yet-`new`'d (should not happen post-construction): no
                // diagnostics.
                None => false,
            }
        }

        /// Reads the diagnostics of this configuration as a TextBuffer,
        /// or returns null if there are no diagnostics.
        ///
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

            // `var buf: [4095:0]u8` + `writer: .fixed(&buf)`
            // (`config.zig:117-118`) -> `format_diagnostic_bounded`, which
            // enforces the same 4095-byte capacity. `Diagnostic::format` writes
            // UTF-8; a lossy decode guards the `insert_at_cursor(&str)` binding.
            // for (config._diagnostics.items()) |diag| {
            for diag in config._diagnostics.items() {
                //     writer.end = 0;
                //     diag.format(&writer) catch |err| {
                //         log.warn(
                //             "error writing diagnostic to buffer err={}",
                //             .{err},
                //         );
                //         continue;
                //     };
                let buf = match format_diagnostic_bounded(diag) {
                    Ok(buf) => buf,
                    Err(err) => {
                        log::warn!(
                            target: LOG_SCOPE,
                            "error writing diagnostic to buffer err={err:?}",
                        );
                        continue;
                    }
                };
                let text = String::from_utf8_lossy(&buf);
                //     text_buf.insertAtCursor(&buf, @intCast(writer.end));
                //     text_buf.insertAtCursor("\n", 1);
                text_buf.insert_at_cursor(&text);
                text_buf.insert_at_cursor("\n");
            }

            Some(text_buf)
        }
    }
}

glib::wrapper! {
    /// Wraps a `Rustty.Config` object in a GObject so it can be reference
    /// counted. When this object is freed, the underlying config is also freed.
    ///
    /// It is highly recommended to NOT take a reference to this object,
    /// since configuration takes up a lot of memory (relatively). Instead,
    /// receivers of this should usually create a `DerivedConfig` struct from
    /// this, copy any memory they require, and own that structure instead.
    ///
    /// This can also expose helpers to access configuration in ways that
    /// may be more ergonomic to GTK primitives.
    ///
    /// PORTS: `config.zig:23-164` `pub const Config` (wraps a loaded
    /// [`rustty_config::Config`]).
    pub struct Config(ObjectSubclass<imp::Config>);
}

impl Config {
    /// Create a new GhosttyConfig from a loaded configuration.
    ///
    /// This clones the given configuration, so it is safe for the
    /// caller to free the original configuration after this call.
    ///
    /// PORTS: `config.zig:78-86` `new` (`config.zig:83`
    /// `priv.config = try config.clone(alloc)`).
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

    /// Get the wrapped configuration. It's unsafe to store this or access
    /// it in any way that may live beyond the lifetime of this object.
    ///
    /// PORTS: `config.zig:90-92` `get`; the borrow is tied to `&self`.
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

    // This test verifies our memory management works as expected. Since
    // we use the testing allocator any leaks are detected.
    //
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

    // No reference `test {}` — PROVING test for S51-F2. `diagnosticsBuffer`
    // formats each diagnostic into a fixed `[4095:0]u8`
    // (`config.zig:117-126`); a diagnostic that does not fit makes
    // `diag.format` fail with `error.WriteFailed` and is SKIPPED with a
    // warning, not shown truncated and not shown in full. The port formatted
    // into a growable `Vec` and always inserted the whole text.
    //
    // Asserted on `format_diagnostic_bounded`, the capacity decision itself,
    // rather than through `diagnostics-buffer`: the property getter needs a
    // `gtk4::TextBuffer`, and the boundary is what the row is about. The
    // boundary is exact — `writer.end` may reach 4095, so 4095 fits and 4096
    // does not.
    #[test]
    fn diagnostics_buffer_skips_a_diagnostic_that_overflows_the_reference_buffer() {
        use rustty_args::diagnostics::Diagnostic;

        // `Diagnostic.format` writes `key`, then ": ", then `message` when a
        // key is present; with no key and no location it writes `message`
        // alone, so `message.len()` IS the formatted length here.
        let exact = Diagnostic {
            message: vec![b'x'; DIAGNOSTICS_BUFFER_LEN],
            ..Diagnostic::default()
        };
        let over = Diagnostic {
            message: vec![b'x'; DIAGNOSTICS_BUFFER_LEN + 1],
            ..Diagnostic::default()
        };

        assert_eq!(
            format_diagnostic_bounded(&exact).map(|b| b.len()),
            Ok(DIAGNOSTICS_BUFFER_LEN),
            "4095 bytes is the last length `std.Io.Writer.fixed(&buf)` accepts"
        );
        assert!(
            matches!(
                format_diagnostic_bounded(&over),
                Err(DiagnosticFormatError::WriteFailed)
            ),
            "4096 bytes overflows the reference's fixed buffer"
        );
    }
}
