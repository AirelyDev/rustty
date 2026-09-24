//! build.rs — capture the Rust toolchain version at compile time.
//!
//! Reference analog: Zig exposes `builtin.zig_version_string` to every module;
//! `cli/version.zig` prints it as the toolchain line. Rust has no builtin, so
//! this build script asks the ACTUAL compiler cargo is driving (`$RUSTC`,
//! falling back to `rustc` on PATH) and bakes the answer into the crate env.

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let out = std::process::Command::new(&rustc)
        .arg("--version")
        .output()
        .expect("failed to run `rustc --version`");
    let full = String::from_utf8(out.stdout).expect("rustc --version is UTF-8");
    // "rustc 1.93.1 (hash date)" -> "1.93.1 (hash date)" (a bare version
    // string, mirroring `builtin.zig_version_string`).
    let trimmed = full.trim();
    let version = trimmed.strip_prefix("rustc ").unwrap_or(trimmed);
    println!("cargo:rustc-env=RUSTTY_RUSTC_VERSION={version}");
    println!("cargo:rerun-if-env-changed=RUSTC");

    // `builtin.mode` (parity S42-F3). Zig has four optimize modes and
    // `build_config.mode_string` reports whichever one the build used. The
    // crate can see `debug_assertions` on its own, but NOT the optimization
    // level — that is only visible here, as cargo's `OPT_LEVEL`, so bake it in.
    // Together the two reconstruct all four modes; see `mode_string`.
    let opt_level = std::env::var("OPT_LEVEL").unwrap_or_else(|_| "0".to_string());
    println!("cargo:rustc-env=RUSTTY_OPT_LEVEL={opt_level}");
    println!("cargo:rerun-if-env-changed=OPT_LEVEL");

    // `-Dversion-string` sets `options.app_version`, whose `MAJOR.MINOR.PATCH`
    // is `Config.version` (`fromOptions`). Only the explicit arm can change
    // those three numbers: every git-derived arm keeps the app version's own
    // (`Config.zig:290-338`). So this crate watches the variable but not git;
    // the git-derived version is `rustty-version`'s, a leaf crate, so a commit
    // does not recompile every crate that reads a build option.
    println!("cargo:rerun-if-env-changed=RUSTTY_VERSION_STRING");
    let explicit = std::env::var("RUSTTY_VERSION_STRING").unwrap_or_default();
    println!("cargo:rustc-env=RUSTTY_VERSION_EXPLICIT={explicit}");
    println!("cargo:rerun-if-changed=build.rs");

    font_backend();
}

/// `-Dfont-backend` (`src/build/Config.zig:191-195`):
///
/// ```text
/// config.font_backend = b.option(
///     FontBackend,
///     "font-backend",
///     "The font backend to use for discovery and rasterization.",
/// ) orelse FontBackend.default(target.result, wasm_target);
/// ```
///
/// The option is the `font-backend-*` feature set (at most one). This resolves
/// it, falls back to `Backend.default` (`src/font/backend.zig:39-63`) when none
/// is set, and publishes the tag as `links` metadata `font_backend`, which
/// reaches every DIRECT dependent's build script as
/// `DEP_RUSTTY_BUILD_CONFIG_FONT_BACKEND`. `rustty-font-shaper` keys its
/// `Face`/`Shaper`/`Library`/`Discover` selectors on it. (The crate itself reads
/// the same features through `cfg!` in `build::FONT_BACKEND_OPTION`; this script
/// is where the at-most-one and per-target checks live.)
fn font_backend() {
    // (feature env suffix, `@tagName(Backend)`), in `backend.zig` order.
    const FEATURES: [(&str, &str); 7] = [
        ("FONT_BACKEND_FREETYPE", "freetype"),
        ("FONT_BACKEND_FREETYPE_WINDOWS", "freetype_windows"),
        ("FONT_BACKEND_FONTCONFIG_FREETYPE", "fontconfig_freetype"),
        ("FONT_BACKEND_CORETEXT", "coretext"),
        ("FONT_BACKEND_CORETEXT_FREETYPE", "coretext_freetype"),
        ("FONT_BACKEND_CORETEXT_HARFBUZZ", "coretext_harfbuzz"),
        ("FONT_BACKEND_CORETEXT_NOSHAPE", "coretext_noshape"),
    ];
    let selected: Vec<&str> = FEATURES
        .iter()
        .filter(|(env, _)| std::env::var_os(format!("CARGO_FEATURE_{env}")).is_some())
        .map(|&(_, tag)| tag)
        .collect();
    if selected.len() > 1 {
        panic!(
            "rustty-build-config: `-Dfont-backend` takes ONE value, but the features \
             select {selected:?}. Enable at most one `font-backend-*` feature."
        );
    }

    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let is_wasm = arch == "wasm32";
    // `Backend.default(target, wasm_target)`.
    let default = if is_wasm {
        // `.browser => .web_canvas` (the only `WasmTarget`).
        "web_canvas"
    } else if os == "windows" {
        "freetype_windows"
    } else if os == "macos" || os == "ios" {
        // `target.os.tag.isDarwin()`.
        "coretext"
    } else {
        "fontconfig_freetype"
    };

    let backend = match selected.first() {
        None => default,
        Some(&tag) => {
            // The backends this port can build per target. The reference enum
            // lets any value through to the compiler; the arms outside this
            // table either cannot link on the target (CoreText off Darwin) or
            // are not built by the port there (`fontconfig_freetype` on macOS:
            // the vendored `fontconfig-sys` is gated off Darwin, as upstream CI
            // never builds it there either; `freetype_windows` off Windows: its
            // font-directory scanner is Windows-only code). Refused LOUDLY
            // rather than silently replaced by the default (R14 spirit).
            //
            // wasm is exempt: `font/main.zig:50` ignores the option there
            // (`.backend = if (wasm) .web_canvas else build_config.font_backend`),
            // so the value only reaches `build_config.font_backend` itself.
            let supported: &[&str] = if is_wasm {
                &[tag]
            } else if os == "macos" || os == "ios" {
                &[
                    "freetype",
                    "coretext",
                    "coretext_freetype",
                    "coretext_harfbuzz",
                    "coretext_noshape",
                ]
            } else if os == "windows" {
                &["freetype", "freetype_windows"]
            } else {
                &["freetype", "fontconfig_freetype"]
            };
            if !supported.contains(&tag) {
                panic!(
                    "rustty-build-config: `-Dfont-backend={tag}` is not buildable by the \
                     port for target os `{os}` arch `{arch}`; supported here: {supported:?} \
                     (default `{default}`)."
                );
            }
            tag
        }
    };

    println!("cargo:font_backend={backend}");
}
