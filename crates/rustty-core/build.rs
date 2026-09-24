//! Build script for rustty-core.
//!
//! `font.options.backend` (`-Dfont-backend`, S-14 / N266): a few core items and
//! tests depend on WHICH font backend is built — the `FreetypeLoadFlags` arm of
//! `Surface.DerivedConfig` (`if (FreetypeLoadFlags != void) ... else {}`), which
//! is a TYPE choice. `rustty-build-config` publishes the resolved backend as
//! `links` metadata, which reaches a DIRECT dependent's build script only; this
//! turns it into the same cfgs `rustty-font-shaper`'s build script emits:
//! `rustty_font_backend = "<@tagName>"` and `rustty_font_has_freetype`
//! (`backend.zig` `hasFreetype()`).

fn main() {
    println!(
        "cargo::rustc-check-cfg=cfg(rustty_font_backend, values(\"freetype\", \
         \"freetype_windows\", \"fontconfig_freetype\", \"coretext\", \
         \"coretext_freetype\", \"coretext_harfbuzz\", \"coretext_noshape\", \
         \"web_canvas\"))"
    );
    println!("cargo::rustc-check-cfg=cfg(rustty_font_has_freetype)");
    println!("cargo::rerun-if-env-changed=DEP_RUSTTY_BUILD_CONFIG_FONT_BACKEND");
    println!("cargo::rerun-if-changed=build.rs");

    let backend = if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        // `font/main.zig:50`: wasm is always `.web_canvas`.
        "web_canvas".to_string()
    } else {
        std::env::var("DEP_RUSTTY_BUILD_CONFIG_FONT_BACKEND")
            .expect("rustty-build-config publishes `font_backend` (links metadata)")
    };
    println!("cargo::rustc-cfg=rustty_font_backend=\"{backend}\"");
    // backend.zig `hasFreetype()`.
    if matches!(
        backend.as_str(),
        "freetype" | "freetype_windows" | "fontconfig_freetype" | "coretext_freetype"
    ) {
        println!("cargo::rustc-cfg=rustty_font_has_freetype");
    }
}
