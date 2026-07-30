//! rustty-apprt::gtk::i18n
//!
//! PORTS: the GTK apprt's use of `internal_os.i18n._` (`src/os/i18n.zig:26-31`),
//! which the reference calls directly at ~25 code-side UI sites (e.g.
//! `close_confirmation_dialog.zig:182-198`, `window.zig:1067,1611,1613,1805`).
//!
//! Why a wrapper exists here at all: the reference's `_` returns
//! `[*:0]const u8`, which Zig hands straight to the GTK setters. The Rust binding
//! `rustty_os::i18n::gettext_` returns `&'static CStr`, but every gtk-rs setter
//! wants `&str`, so each call site would otherwise repeat the same
//! `.to_str()`-with-fallback dance. [`tr`] is that dance, once.
//!
//! Two properties this file is responsible for, both load-bearing:
//!
//! * **It must never panic.** A `.mo` file is user-supplyable data (it is read
//!   from `<share>/locale`), so a catalog carrying non-UTF-8 bytes must degrade to
//!   the untranslated msgid rather than abort a terminal mid-session.
//! * **The msgid must stay literal at the call site.** A gettext msgid is a lookup
//!   KEY, so it can never be composed (`format!`) and it must be extractable by
//!   `xgettext`. `xtask update-translations` therefore passes `--keyword=tr`
//!   alongside `--keyword=gettext_`, and
//!   `crates/rustty-apprt/tests/i18n_msgid_coverage.rs` asserts every msgid passed
//!   to `tr` as a literal exists in the committed catalogs (and conversely that
//!   every catalog msgid the reference sources from code is looked up somewhere).
//!
//! NOTE for anyone editing this file: it must contain no literal `tr`-call whose
//! argument is a string literal, because both `xgettext` and the coverage test
//! would then treat that string as a real runtime msgid. The unit test below
//! therefore goes through a named `const`.

/// Translate `msgid` through the Rustty gettext domain, as `&str`.
///
/// PORTS: `internal_os.i18n._(msgid)` (`src/os/i18n.zig:26-31`) →
/// `rustty_os::i18n::gettext_`. Returns the untranslated `msgid` when no
/// translation exists (gettext's own contract) or when the catalog's bytes are not
/// valid UTF-8 (see the module docs — never panics).
#[must_use]
pub fn tr(msgid: &'static std::ffi::CStr) -> &'static str {
    let translated = rustty_os::i18n::gettext_(msgid);
    translated
        .to_str()
        .or_else(|_| msgid.to_str())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without a bound catalog (or with an untranslated msgid) `tr` is the
    /// identity — which is precisely why the *coverage* test, not this one, is the
    /// gate on translatability. Asserted here only so a regression that made `tr`
    /// return an empty string or panic is caught.
    /// Not a literal argument on purpose — see the module note.
    const PROBE: &std::ffi::CStr = c"a msgid no catalog carries";

    #[test]
    fn tr_falls_through_to_the_msgid() {
        assert_eq!(tr(PROBE), "a msgid no catalog carries");
    }
}
