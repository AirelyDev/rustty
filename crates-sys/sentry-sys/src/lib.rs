//! sentry-sys — FFI bindings to the vendored sentry-native 0.7.8 crash-reporting
//! SDK (`links = "sentry"`).
//!
//! PORT STATUS: vendored (ghostty's product configuration)
//!
//! R3: copies the SAME C library the reference uses (sentry-native 0.7.8) and
//! builds it standalone (`cc` + `bindgen`; see `build.rs`) with ghostty's
//! product options (`backend = .breakpad` — src/build/SharedDeps.zig passes
//! `.backend = .breakpad` whenever sentry is enabled, linking the pkg/breakpad
//! artifact (here: breakpad-sys) — and `transport = .none`). No pure-Rust
//! substitution during parity phases. The FFI is bindgen-generated from
//! `wrapper.h` — the same public header the reference's `pkg/sentry/c.zig`
//! @cImports (`sentry.h`).

// Link-force the breakpad client archive + C++ runtime into every consumer's
// final link (the reference links `breakpad_dep.artifact("breakpad")` into the
// same binary as libsentry — SharedDeps.zig/pkg/sentry `.breakpad` arm).
// breakpad-sys exposes no Rust items, so without this the unused rlib — and
// with it the `cargo:rustc-link-lib` directives its build script emitted —
// would be dropped from the link, leaving `sentry_backend_breakpad.o`'s
// `google_breakpad::ExceptionHandler` references unresolved.
extern crate breakpad_sys as _;

// The bindgen-generated module carries C naming conventions and machine-shaped
// code; suppress the whole surface (this is generated, not hand-authored — the
// clippy/rustc-lint gate applies to our code, not the generated bindings).
#[allow(
    non_camel_case_types,
    non_upper_case_globals,
    non_snake_case,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    dead_code,
    rustdoc::all
)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use bindings::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// R3 known-answer test: prove the vendored sentry-native 0.7.8 links and
    /// runs, exercising ONLY the pure value-API (no `sentry_init`, no
    /// transport/backend thread, no network).
    ///
    /// Mirrors the reference's `pkg/sentry/value.zig` value wrappers
    /// (`Value.initObject` → `sentry_value_new_object`, `Value.initString` →
    /// `sentry_value_new_string(_n)`, `Value.decref` → `sentry_value_decref`)
    /// plus a `sentry_value_get_type` known-answer:
    ///   * a fresh object value reports `SENTRY_VALUE_TYPE_OBJECT`;
    ///   * a fresh string value reports `SENTRY_VALUE_TYPE_STRING`;
    ///   * neither is null (`sentry_value_is_null` == 0);
    ///   * both tear down cleanly via `sentry_value_decref`.
    #[test]
    fn kat_sentry_value_api_roundtrip() {
        unsafe {
            // Object value.
            let obj = sentry_value_new_object();
            assert_eq!(
                sentry_value_get_type(obj),
                sentry_value_type_t_SENTRY_VALUE_TYPE_OBJECT,
                "new object value must have OBJECT type"
            );
            assert_eq!(
                sentry_value_is_null(obj),
                0,
                "object value must not be null"
            );

            // String value ("x"), via the null-terminated `sentry_value_new_string`.
            let s = sentry_value_new_string(c"x".as_ptr());
            assert_eq!(
                sentry_value_get_type(s),
                sentry_value_type_t_SENTRY_VALUE_TYPE_STRING,
                "new string value must have STRING type"
            );
            assert_eq!(sentry_value_is_null(s), 0, "string value must not be null");

            // Tear down (release the refcounts).
            sentry_value_decref(s);
            sentry_value_decref(obj);
        }
    }

    /// Backend-selection KAT (W15-04): the reference's product build selects
    /// `backend = .breakpad` (src/build/SharedDeps.zig passes
    /// `.backend = .breakpad` whenever sentry is enabled — pkg/sentry's
    /// standalone `orelse .inproc` default is never what the product builds),
    /// so the built `libsentry.a` (path injected by build.rs) must contain the
    /// `sentry_backend_breakpad` object and must NOT contain the
    /// `sentry_backend_inproc` one. Archive member names appear verbatim in
    /// the `ar` archive bytes (BSD long-name members inline, GNU via the `//`
    /// extended-name table), so a raw byte scan needs no `ar` binary.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn kat_breakpad_backend_selected() {
        let archive = env!("SENTRY_SYS_ARCHIVE");
        let bytes = std::fs::read(archive)
            .unwrap_or_else(|e| panic!("reading built archive {archive}: {e}"));
        assert!(
            bytes.starts_with(b"!<arch>\n"),
            "{archive} is not an ar archive"
        );
        let contains = |needle: &str| bytes.windows(needle.len()).any(|w| w == needle.as_bytes());
        assert!(
            contains("sentry_backend_breakpad"),
            "archive {archive} is missing the breakpad backend object \
             (reference: SharedDeps.zig `.backend = .breakpad`)"
        );
        assert!(
            !contains("sentry_backend_inproc"),
            "archive {archive} still contains the inproc backend object \
             (pkg/sentry's standalone default, NOT the product config)"
        );

        // Link-and-run proof: pull the backend object into this test binary by
        // constructing (and freeing) the backend vtable. `sentry__backend_new`
        // only allocates + zeroes the struct and installs the
        // startup/shutdown/except function pointers (no ExceptionHandler is
        // installed until `sentry_init` runs startup), but referencing it
        // forces the linker to resolve the object's
        // `google_breakpad::ExceptionHandler` references against the
        // breakpad-sys archive and the C++ runtime — a broken backend link
        // fails HERE instead of at first `sentry_init` in the product.
        // (Internal sentry-native symbols, declared by src/sentry_backend.h in
        // the vendored pin — not part of the bindgen'd public sentry.h
        // surface.)
        unsafe extern "C" {
            fn sentry__backend_new() -> *mut std::ffi::c_void;
            fn sentry__backend_free(backend: *mut std::ffi::c_void);
        }
        unsafe {
            let backend = sentry__backend_new();
            assert!(
                !backend.is_null(),
                "sentry__backend_new (breakpad backend) returned NULL"
            );
            sentry__backend_free(backend);
        }
    }
}
