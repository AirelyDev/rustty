//! Build script for rustty-font-shaper.
//!
//! Compiles the vendored `stb_image_resize` single-header library into a small
//! static object exposing `stbir_resize_uint8`. R3: this is the SAME C the
//! reference links (ghostty `src/stb/stb_image_resize.h`), copied into our own
//! `vendor/` tree and built with `cc` — no pure-Rust substitution. The freetype
//! face uses it to scale bitmap-strike glyphs (`FT_GLYPH_FORMAT_BITMAP`).

fn main() {
    font_backend();

    // WASM DIVERGENCE (`wasm32`): the freestanding web_canvas backend does not
    // use the FreeType face and therefore never links `stbir_resize_uint8`; skip
    // the C compile (freestanding wasm has no C toolchain wired here). The `stb`
    // module + freetype face are cfg-gated off on wasm (see lib.rs / face/mod.rs).
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        return;
    }

    println!("cargo:rerun-if-changed=vendor/stb_impl.c");
    println!("cargo:rerun-if-changed=vendor/stb_image_resize.h");

    cc::Build::new()
        .file("vendor/stb_impl.c")
        .include("vendor")
        // stb_image_resize.h is a public-domain single-header lib; silence its
        // (benign) warnings so they don't trip a -Werror-style CI.
        .warnings(false)
        .compile("rustty_stb_image_resize");
}

/// `font.options.backend` (`src/font/main.zig:45-51`):
///
/// ```text
/// .backend = if (builtin.target.cpu.arch.isWasm()) .web_canvas else build_config.font_backend,
/// ```
///
/// `build_config.font_backend` is `-Dfont-backend`, resolved by
/// `rustty-build-config`'s build script and handed over as its `links` metadata.
/// Emitted as cfgs so the comptime `switch (options.backend)` selectors
/// (`Face`, `Shaper`, `Library`, `Discover`, the `DeferredFace` payloads) can
/// choose TYPES:
///
/// * `rustty_font_backend = "<@tagName>"`, and
/// * `rustty_font_has_{freetype,coretext,fontconfig,harfbuzz}` — `backend.zig`'s
///   `hasFreetype()`/`hasCoretext()`/`hasFontconfig()`/`hasHarfbuzz()`.
fn font_backend() {
    const TAGS: [&str; 8] = [
        "freetype",
        "freetype_windows",
        "fontconfig_freetype",
        "coretext",
        "coretext_freetype",
        "coretext_harfbuzz",
        "coretext_noshape",
        "web_canvas",
    ];
    println!("cargo:rerun-if-env-changed=DEP_RUSTTY_BUILD_CONFIG_FONT_BACKEND");
    println!(
        "cargo:rustc-check-cfg=cfg(rustty_font_backend, values(\"{}\"))",
        TAGS.join("\", \"")
    );
    for flag in ["freetype", "coretext", "fontconfig", "harfbuzz"] {
        println!("cargo:rustc-check-cfg=cfg(rustty_font_has_{flag})");
    }

    let backend = if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        "web_canvas".to_string()
    } else {
        std::env::var("DEP_RUSTTY_BUILD_CONFIG_FONT_BACKEND").expect(
            "rustty-font-shaper: rustty-build-config did not publish `font_backend` \
             (its `links` metadata)",
        )
    };
    assert!(
        TAGS.contains(&backend.as_str()),
        "rustty-font-shaper: unknown font backend `{backend}`"
    );
    println!("cargo:rustc-cfg=rustty_font_backend=\"{backend}\"");

    // backend.zig `hasFreetype`/`hasCoretext`/`hasFontconfig`/`hasHarfbuzz`.
    let b = backend.as_str();
    let has_freetype = matches!(
        b,
        "freetype" | "freetype_windows" | "fontconfig_freetype" | "coretext_freetype"
    );
    let has_coretext = matches!(
        b,
        "coretext" | "coretext_freetype" | "coretext_harfbuzz" | "coretext_noshape"
    );
    let has_fontconfig = b == "fontconfig_freetype";
    let has_harfbuzz = matches!(
        b,
        "freetype"
            | "freetype_windows"
            | "fontconfig_freetype"
            | "coretext_freetype"
            | "coretext_harfbuzz"
    );
    for (flag, on) in [
        ("freetype", has_freetype),
        ("coretext", has_coretext),
        ("fontconfig", has_fontconfig),
        ("harfbuzz", has_harfbuzz),
    ] {
        if on {
            println!("cargo:rustc-cfg=rustty_font_has_{flag}");
        }
    }

    // The link-edge features (`freetype`/`harfbuzz`/`fontconfig`, default-on;
    // see Cargo.toml) must cover what the backend needs — the reference's
    // `SharedDeps.zig:267-292` links exactly `hasHarfbuzz`/`hasFreetype`/
    // `hasFontconfig`. One message here instead of a cascade of missing items.
    for (need, feature) in [
        (has_freetype, "FREETYPE"),
        (has_harfbuzz, "HARFBUZZ"),
        (has_fontconfig, "FONTCONFIG"),
    ] {
        if need && std::env::var_os(format!("CARGO_FEATURE_{feature}")).is_none() {
            panic!(
                "rustty-font-shaper: font backend `{backend}` needs the `{}` feature",
                feature.to_lowercase()
            );
        }
    }
}
