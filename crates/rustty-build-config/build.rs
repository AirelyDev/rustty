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
    i18n();
}

/// `-Di18n` (`src/build/Config.zig:263-271`, N421):
///
/// ```text
/// config.i18n = b.option(
///     bool,
///     "i18n",
///     "Enables gettext-based internationalization. Enabled by default only for macOS, and other Unix-like systems like Linux and FreeBSD when using glibc.",
/// ) orelse switch (target.result.os.tag) { ... };
/// ```
///
/// A bool option whose default depends on the target, so it is an environment
/// variable (`RUSTTY_I18N=true|false`) like `-Dversion-string`
/// (`RUSTTY_VERSION_STRING`) and `-Dgtk-x11` (`RUSTTY_GTK_X11`), not a pair of
/// additive features. Unset (or empty) means "not passed": the crate then takes
/// the per-target default (`build::i18n_default`). Any other value is refused,
/// as `zig build` refuses a non-bool `-Di18n`.
fn i18n() {
    println!("cargo:rerun-if-env-changed=RUSTTY_I18N");
    let option = match std::env::var("RUSTTY_I18N").ok().as_deref() {
        None | Some("") => "",
        Some("true") => "true",
        Some("false") => "false",
        Some(other) => panic!(
            "rustty-build-config: `-Di18n` is a bool: RUSTTY_I18N must be `true` or `false`, \
             not {other:?}"
        ),
    };
    println!("cargo:rustc-env=RUSTTY_I18N_OPTION={option}");
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
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
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
            // lets any value through to the compiler (`Config.zig:191-195` has
            // no validation). Refused here, LOUDLY rather than silently
            // replaced by the default (R14 spirit):
            //   * the CoreText backends off Darwin — they cannot link there
            //     (no CoreText), in the reference too;
            //   * `fontconfig_freetype` on an MSVC-ABI Windows target — the
            //     reference cannot build it there either (N286, measured on
            //     Windows 11 with Zig 0.16 at the pin): `Config.zig:100-111`
            //     makes MSVC the Windows ABI when none is named, and
            //     pkg/fontconfig/build.zig defines `HAVE_UNISTD_H` on every
            //     target, so `src/fcint.h:45` (2.18.3; `:44` in 2.14.2) fails
            //     with "'unistd.h' file not found" against the MSVC headers
            //     (re-measured at 2.18.3, P7-W10 T1). With an explicit GNU ABI
            //     (`-Dtarget=x86_64-windows-gnu`, MinGW headers) the reference
            //     built it at 2.14.2 and its fontconfig tests passed, so the
            //     port accepts it on `target_env = "gnu"`. At 2.18.3 the
            //     reference no longer compiles fontconfig there
            //     (`FcLocaleSetCurrent` is declared off Windows only); the port
            //     fixes that in `fontconfig-sys` (UD358).
            // (`web_canvas` has no feature: `font/main.zig:50` forces it on
            // wasm, and its face/shaper call browser externs.) CORRECTION
            // (S-14b, review M3): `fontconfig_freetype` on macOS and
            // `freetype_windows` off Windows were refused as "not built by the
            // port"; both build (the Windows directory scanner is portable
            // Rust, as the reference's is portable Zig) and are tested on macOS
            // by the G-FONTBACKEND lane.
            //
            // wasm is exempt: `font/main.zig:50` ignores the option there
            // (`.backend = if (wasm) .web_canvas else build_config.font_backend`),
            // so the value only reaches `build_config.font_backend` itself.
            let supported: &[&str] = if is_wasm {
                &[tag]
            } else if os == "macos" || os == "ios" {
                &[
                    "freetype",
                    "freetype_windows",
                    "fontconfig_freetype",
                    "coretext",
                    "coretext_freetype",
                    "coretext_harfbuzz",
                    "coretext_noshape",
                ]
            } else if os == "windows" && env == "gnu" {
                &["freetype", "freetype_windows", "fontconfig_freetype"]
            } else if os == "windows" {
                &["freetype", "freetype_windows"]
            } else {
                &["freetype", "freetype_windows", "fontconfig_freetype"]
            };
            if os == "windows" && env != "gnu" && tag == "fontconfig_freetype" {
                panic!(
                    "rustty-build-config: `-Dfont-backend=fontconfig_freetype` is not buildable \
                     for target os `{os}` env `{env}`: Fontconfig includes `unistd.h`, which the \
                     MSVC headers do not have (the reference fails the same way at \
                     `fontconfig/src/fcint.h:45`); use a GNU-ABI Windows target \
                     (`x86_64-pc-windows-gnu`, the reference's `-Dtarget=x86_64-windows-gnu`)."
                );
            }
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
