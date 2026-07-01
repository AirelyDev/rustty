//! sentry-sys — FFI bindings to the vendored sentry-native 0.7.8 crash-reporting
//! SDK (`links = "sentry"`).
//!
//! PORT STATUS: vendored (ghostty's fixed configuration)
//!
//! R3: copies the SAME C library the reference uses (sentry-native 0.7.8) and
//! builds it standalone (`cc` + `bindgen`; see `build.rs`) with ghostty's fixed
//! options (`backend = .inproc`, `transport = .none`). No pure-Rust substitution
//! during parity phases. The FFI is bindgen-generated from `wrapper.h` — the
//! same public header the reference's `pkg/sentry/c.zig` @cImports (`sentry.h`).

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
}
