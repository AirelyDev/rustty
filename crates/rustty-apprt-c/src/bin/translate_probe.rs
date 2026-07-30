//! WS13 child process for the `rustty_translate` KAT.
//!
//! Why a separate process rather than an in-test call: GNU gettext resolves the
//! message-catalog language from the ENVIRONMENT at lookup time, and an in-process
//! `setlocale()` alone is NOT enough — verified empirically against rustty's own
//! vendored libintl (WS9): with `LC_ALL`/`LANG` absent from `env` but
//! `setlocale(LC_ALL,"de_DE.UTF-8")` called, `dgettext` returned the msgid
//! UNTRANSLATED; with `LANG=de_DE.UTF-8` in `env` it returned the German string.
//! Setting env inside the test would also require `unsafe { std::env::set_var }`
//! under edition 2024 and would leak into sibling tests in the same binary.
//!
//! Usage: `translate_probe <resources_dir> <msgid>` — prints the translation to
//! stdout with no trailing newline.
//!
//! It is a `[[bin]]` rather than an `examples/` target purely so the test can find
//! it: Cargo exports `CARGO_BIN_EXE_<name>` for bins only. It is a TEST FIXTURE and
//! is not part of any shipped artifact.

fn main() {
    let mut args = std::env::args().skip(1);
    let resources_dir = args
        .next()
        .expect("usage: translate_probe <resources_dir> <msgid>");
    let msgid = args
        .next()
        .expect("usage: translate_probe <resources_dir> <msgid>");

    // The `ensureLocale` + `i18n.init` pair the real app runs
    // (`crates/rustty-apprt/src/lib.rs:299,322`).
    rustty_os::ensureLocale().expect("ensureLocale");
    rustty_os::i18n::init(&resources_dir).expect("i18n::init");

    let c = std::ffi::CString::new(msgid).expect("msgid has no interior NUL");
    let out = unsafe { rustty_apprt_c::rustty_translate(c.as_ptr()) };
    let out = unsafe { std::ffi::CStr::from_ptr(out) };
    print!("{}", out.to_string_lossy());
}
