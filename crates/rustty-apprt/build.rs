//! Asset build pipeline for the native GTK4/libadwaita apprt backend.
//!
//! PORTS the reference's TWO build helper binaries plus its `SharedDeps`
//! wiring, collapsed into one build script (behavior-preserving, not
//! line-preserving):
//!   * `src/apprt/gtk/build/blueprint.zig` — the adw-version gate (`:55-69`,
//!     invariant 7) and the `blueprint-compiler --version` gate (`:71-109`).
//!     Pre-ef7edc4a2 this file also had the `compile --output` step
//!     (`:123-173`); that commit dropped it (the wrapper now only writes a
//!     cache stamp, see its module doc below) and moved the actual
//!     `blueprint-compiler compile --output` invocation into
//!     `src/build/SharedDeps.zig`'s `gtkNgDistResourcesUncached`
//!     (`:1090-1105`) — this script already ran `blueprint-compiler` directly
//!     rather than through a compiled wrapper (`compile_blueprint` below), so
//!     upstream converged on the same shape here; no behaviour change.
//!   * `src/apprt/gtk/build/gresource.zig` — `genRoot`/`genIcons`/`genUi`
//!     (`:149-262` post-ef7edc4a2, was `:212-277`) reproduced verbatim in
//!     output *shape* (`compressed="true"`, `preprocess="xml-stripblanks"`,
//!     the `{size}x{size}@2` icon aliases). ef7edc4a2 also rewrote `genUi` to
//!     drop its argv-supplied UI file list and per-file `error.BlueprintNotFound`
//!     arm in favor of an `inline for` over the compile-time `blueprints` list
//!     — again converging on what this script already did (see the PORT NOTE
//!     at `compile_blueprint`'s call site below).
//!   * `src/build/SharedDeps.zig` `gtkNgDistResources`/
//!     `gtkNgDistResourcesUncached` (`:1005-1184` post-ef7edc4a2, was
//!     `gtkNgDistResources` alone at `:870-967`) — the
//!     `glib-compile-resources` `--generate-source`/`--generate-header` calls
//!     and the `cc` link of the generated `*_resources.c`. ef7edc4a2 also
//!     memoized this function on the `*std.Build` (a file-scope
//!     `AutoHashMapUnmanaged`) and switched the gresource XML's `<file>`
//!     bodies from absolute Zig-cache paths to paths relative to a
//!     `--sourcedir`, so repeated builds share one compiled-`.ui` directory
//!     instead of invalidating on every unrelated cache-path shuffle — a
//!     Zig-build-cache concern with no Cargo analogue (`cargo` already keys
//!     this script's own outputs off `OUT_DIR` + `rerun-if-changed`, never off
//!     an executable's link bytes), so nothing here changes.
//!   * `src/build/GhosttyI18n.zig` (`:33-43`) — `msgfmt` of each `po/*.po` into
//!     `share/locale/<locale>/LC_MESSAGES/<domain>.mo`.
//!
//! # The two helpers' own module docs
//!
//! `blueprint.zig:1-11` (post-ef7edc4a2; pre-ef7edc4a2 it was `:1-2` below):
//! Checks that `libadwaita` is at least the given version and that
//! `blueprint-compiler` is on the PATH and new enough. The blueprints
//! themselves are compiled by `blueprint-compiler` directly from the build
//! system; see `gtkNgDistResources` in `src/build/SharedDeps.zig`.
//!
//! Usage: blueprint.zig <major> <minor> <stamp>
//!
//! Example: blueprint.zig 1 5 blueprint-check.stamp
//!
//! `<stamp>` is written when every check passes, so the build system has an
//! output to cache this step by.
//!
//! Pre-ef7edc4a2 this instead read:
//!
//! Compiles a blueprint file using `blueprint-compiler`. This performs
//! additional checks to ensure that various minimum versions are met.
//!
//! Usage: blueprint.zig <major> <minor> <output> <input>
//!
//! Example: blueprint.zig 1 5 output.ui input.blp
//!
//! `gresource.zig:1-5`:
//! This file contains a binary helper that builds our gresource XML
//! file that we can then use with `glib-compile-resources`.
//!
//! This binary is expected to be run from the Rustty source root.
//! Litmus test: `src/apprt/gtk` should exist relative to the pwd.
//!
//! (PORT NOTE: here both helpers run inside this build script rather than as
//! separate binaries, so there is no argv — the `blueprint.zig:6-8`
//! `Usage:`/`Example:` lines above describe a command line rustty does not have;
//! its `<major> <minor> <stamp>` are `compile_blueprint`'s inputs here, minus
//! the stamp file — this script has no separate cache-stamp step, since Cargo
//! already caches the whole build script's output on `rerun-if-changed`, not
//! on a compiled helper's link bytes (see the `SharedDeps.zig` bullet above) —
//! and no cwd precondition: paths are resolved against `--sourcedir`, see
//! "R2 / paths" below.)
//!
//! # R5 / guard
//!
//! The whole pipeline runs only when BOTH `CARGO_FEATURE_GTK` and
//! `CARGO_CFG_TARGET_OS in {"linux", "freebsd"}` hold — the exact gate the `gtk`
//! module carries (`lib.rs`; W2D/F-002 widened it to the two OSes whose default
//! app runtime is `.gtk`, `apprt/runtime.zig:18`). On macOS, or a default
//! (no-`gtk`) Linux/FreeBSD build, this
//! script returns immediately: no `blueprint-compiler` is spawned, no `.c` is
//! linked, and the compiled artifact is byte-behavior-identical.
//!
//! # R2 / paths
//!
//! The gresource prefixes and icon aliases are baked from
//! `rustty_identity::BUNDLE_ID` (the single source of truth `gtk::build_info`
//! also reads), never a hardcoded `com.mitchellh.ghostty`.
//!
//! The reference gresource generator runs with cwd = the source root, so its
//! `ui_path`/`css_path` (`gresource.zig:15,18`) are `src/apprt/gtk/{ui,css}`
//! and the `<file>` bodies it writes are relative to that cwd. rustty vendors
//! the assets under `<crate>/assets`, drives `glib-compile-resources` with
//! `--sourcedir <crate>/assets`, and so writes css/icon `<file>` bodies
//! relative to *that* root (`css/…`, `images/gnome/…`). The compiled `.ui`
//! outputs live in `OUT_DIR`, outside the sourcedir, so — exactly like the
//! reference, whose build system passes absolute generated `.ui` paths — their
//! `<file>` bodies are absolute and bypass `--sourcedir`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A blueprint definition: minimum adwaita `{major}.{minor}` and file stem.
/// Reference: `gresource.zig:65-69` (`Blueprint`).
struct Blueprint {
    major: u16,
    minor: u16,
    name: &'static str,
}

/// The 19 blueprints embedded into the gresource (AD-3: 19, not 21 — the
/// `1.0/clipboard-confirmation-dialog` and `1.2/debug-warning` legacy entries
/// are dropped by the `>= 4.14 / >= 1.5` baseline pin). Mirrors
/// `src/gtk/gresource.rs::BLUEPRINTS`; `build.rs` is a separate compilation
/// unit and cannot import the crate's own library, so the manifest is
/// duplicated here — `tests/gtk_assets.rs` asserts the library copy matches the
/// vendored `.blp` on disk, and this list must be kept in lock-step with it.
const BLUEPRINTS: &[Blueprint] = &[
    Blueprint {
        major: 1,
        minor: 4,
        name: "clipboard-confirmation-dialog",
    },
    Blueprint {
        major: 1,
        minor: 2,
        name: "close-confirmation-dialog",
    },
    Blueprint {
        major: 1,
        minor: 2,
        name: "config-errors-dialog",
    },
    Blueprint {
        major: 1,
        minor: 3,
        name: "debug-warning",
    },
    Blueprint {
        major: 1,
        minor: 5,
        name: "imgui-widget",
    },
    Blueprint {
        major: 1,
        minor: 5,
        name: "inspector-widget",
    },
    Blueprint {
        major: 1,
        minor: 5,
        name: "inspector-window",
    },
    Blueprint {
        major: 1,
        minor: 2,
        name: "resize-overlay",
    },
    Blueprint {
        major: 1,
        minor: 2,
        name: "search-overlay",
    },
    Blueprint {
        major: 1,
        minor: 2,
        name: "key-state-overlay",
    },
    Blueprint {
        major: 1,
        minor: 5,
        name: "split-tree",
    },
    Blueprint {
        major: 1,
        minor: 5,
        name: "split-tree-split",
    },
    Blueprint {
        major: 1,
        minor: 2,
        name: "surface",
    },
    Blueprint {
        major: 1,
        minor: 5,
        name: "surface-scrolled-window",
    },
    Blueprint {
        major: 1,
        minor: 3,
        name: "surface-child-exited",
    },
    Blueprint {
        major: 1,
        minor: 5,
        name: "tab",
    },
    Blueprint {
        major: 1,
        minor: 5,
        name: "title-dialog",
    },
    Blueprint {
        major: 1,
        minor: 5,
        name: "window",
    },
    Blueprint {
        major: 1,
        minor: 5,
        name: "command-palette",
    },
];

/// CSS files in `assets/css`. Reference: `gresource.zig:58-63` (`css`).
const CSS: &[&str] = &[
    "style.css",
    "style-dark.css",
    "style-hc.css",
    "style-hc-dark.css",
];

/// Icon sizes embedded into the gresource; each yields a 1x (`{size}.png`) and
/// a 2x (`{size*2}.png`) source. Reference: `gresource.zig:24` (`icon_sizes`).
const ICON_SIZES: &[u32] = &[16, 32, 128, 256, 512, 1024];

/// Minimum `blueprint-compiler` version. Reference: `blueprint.zig:36-40`
/// (`required_blueprint_version = 0.16.0`; `:36-40` post-ef7edc4a2 — the
/// citation moved with the rewritten module doc above it, text unchanged).
const REQUIRED_BLUEPRINT: (u64, u64, u64) = (0, 16, 0);

/// Shared tail of the `blueprint-compiler` diagnostics. Reference:
/// `blueprint.zig:16-28` (`blueprint_compiler_help`; `:16-28` post-ef7edc4a2,
/// text unchanged), rebranded to rustty.
const BLUEPRINT_HELP: &str = "\n\nWhen building from a source checkout, rustty requires \
version 0.16 or newer of `blueprint-compiler` as a build-time dependency. Please install \
it, ensure that it is available on your PATH, and then retry building rustty.";

#[path = "src/gtk_targets.rs"]
mod gtk_targets;

/// `rustty_gtk_x11` / `rustty_gtk_wayland`: which winproto arms are built
/// (S65-F6; see `src/gtk_targets.rs`). Probes the system GTK4 only for a
/// native gtk build.
fn gtk_winproto_cfgs(native_gtk: bool) {
    println!("cargo::rustc-check-cfg=cfg(rustty_gtk_x11)");
    println!("cargo::rustc-check-cfg=cfg(rustty_gtk_wayland)");
    println!("cargo:rerun-if-env-changed=RUSTTY_GTK_X11");
    println!("cargo:rerun-if-env-changed=RUSTTY_GTK_WAYLAND");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    if !native_gtk {
        return;
    }
    let probed = gtk_targets::parse_targets(gtk_targets::probe().as_deref());
    let x11 = gtk_targets::arm_enabled(
        env::var_os("CARGO_FEATURE_X11").is_some(),
        env::var("RUSTTY_GTK_X11").ok().as_deref(),
        probed.x11,
    );
    let wayland = gtk_targets::arm_enabled(
        env::var_os("CARGO_FEATURE_WAYLAND").is_some(),
        env::var("RUSTTY_GTK_WAYLAND").ok().as_deref(),
        probed.wayland,
    );
    if x11 {
        println!("cargo::rustc-cfg=rustty_gtk_x11");
    }
    if wayland {
        println!("cargo::rustc-cfg=rustty_gtk_wayland");
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // PORTS: `build.zig`'s `wayland_scanner.addCustomProtocol(b.path(
    // "src/apprt/gtk/winproto/wayland/protocols/vicinae-hotkey-v1.xml"))` — the
    // custom-protocol codegen input for `vicinae_hotkey_v1`, which has no
    // crates.io binding.
    //
    // wayland-rs 0.31 does the scanning in the `wayland_scanner::generate_*!`
    // PROC MACROS (`src/gtk/winproto/wayland/vicinae.rs`), not in a build script,
    // so all this script owes the codegen is the staleness edge: a proc macro that
    // reads a file gets no automatic `rerun-if-changed`, and without this line an
    // edit to the vendored XML would NOT rebuild the crate. Emitted before the
    // gtk-target guard below because the `wayland` feature is what selects the
    // codegen, and unconditional emission is harmless (the file always exists).
    if env::var_os("CARGO_FEATURE_WAYLAND").is_some() {
        println!("cargo:rerun-if-changed=protocols/vicinae-hotkey-v1.xml");
    }

    // R5 guard: only a native gtk build runs the asset pipeline. The OS half
    // mirrors `lib.rs`'s `any(target_os = "linux", target_os = "freebsd")` gate
    // (W2D/F-002) — the two targets `Runtime.default()` maps to `.gtk`
    // (`apprt/runtime.zig:14-24`).
    let gtk = env::var_os("CARGO_FEATURE_GTK").is_some();
    let gtk_target = matches!(
        env::var("CARGO_CFG_TARGET_OS").as_deref(),
        Ok("linux") | Ok("freebsd")
    );
    gtk_winproto_cfgs(gtk && gtk_target);
    if !(gtk && gtk_target) {
        return;
    }

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let assets = manifest_dir.join("assets");

    // R2: identity from the single source of truth.
    let base_app_id = rustty_identity::BUNDLE_ID;
    let resource_path = resource_path(base_app_id);

    // Version gates: the adw version backs invariant 7 (per-blueprint below);
    // blueprint-compiler must be >= 0.16 (blueprint.zig:71-109 post-ef7edc4a2,
    // was `:69-121` before that commit trimmed the wrapper down to a version
    // check that writes a cache stamp — see BLUEPRINT_HELP/REQUIRED_BLUEPRINT
    // above and `compile_blueprint` below for the compile step it used to gate).
    let adw = adw_version();
    blueprint_compiler_version_gate();

    // 1. Compile each `.blp` -> `OUT_DIR/ui/{maj}.{min}/{name}.ui`.
    //
    // Collect the UI files that are passed in as arguments.
    // (gresource.zig:128 pre-ef7edc4a2. PORT NOTE: there are no arguments here
    // — this script compiles every manifest blueprint itself and collects each
    // output paired with its `Blueprint`, so the pre-refactor `genUi`'s
    // per-file manifest lookup and its `error.BlueprintNotFound` arm
    // (`gresource.zig:252-269` pre-ef7edc4a2) could not miss. ef7edc4a2 later
    // rewrote `genUi` (now `:228-262`) the same way: an `inline for` over the
    // compile-time `blueprints` list with no argv and no `BlueprintNotFound`
    // arm — upstream converged on what this script already did.)
    let mut ui_files: Vec<(&Blueprint, PathBuf)> = Vec::with_capacity(BLUEPRINTS.len());
    for bp in BLUEPRINTS {
        // Invariant 7 (blueprint.zig:55-69 post-ef7edc4a2, was `:58-67`): the
        // installed adwaita must be new enough to compile this blueprint's
        // `{major}.{minor}` dialect.
        if (u64::from(bp.major), u64::from(bp.minor)) > adw {
            panic!(
                "`libadwaita` is too old: {}.{} required to compile ui/{}.{}/{}.blp, but the \
                 system libadwaita-1 is {}.{}",
                bp.major, bp.minor, bp.major, bp.minor, bp.name, adw.0, adw.1,
            );
        }

        let src = assets.join(format!("ui/{}.{}/{}.blp", bp.major, bp.minor, bp.name));
        println!("cargo:rerun-if-changed={}", src.display());

        let out = out_dir.join(format!("ui/{}.{}/{}.ui", bp.major, bp.minor, bp.name));
        fs::create_dir_all(out.parent().expect("ui output has a parent"))
            .expect("create ui output dir");
        compile_blueprint(&src, &out);
        ui_files.push((bp, out));
    }

    // 2. Emit the gresource XML.
    let xml_path = out_dir.join("rustty.gresource.xml");
    fs::write(&xml_path, gen_xml(&resource_path, base_app_id, &ui_files))
        .expect("write gresource xml");
    // The list of filepaths that we depend on. Used for the build
    // system to have proper caching.
    // (gresource.zig:71-96 `file_inputs`: the `.blp` inputs are emitted in step
    // 1 above, the css and icon inputs here, as `rerun-if-changed` edges.)
    for name in CSS {
        println!(
            "cargo:rerun-if-changed={}",
            assets.join("css").join(name).display()
        );
    }
    for &size in ICON_SIZES {
        for src in [size, size * 2] {
            println!(
                "cargo:rerun-if-changed={}",
                assets.join(format!("images/gnome/{src}.png")).display()
            );
        }
    }

    // 3. glib-compile-resources -> generated .c / .h (SharedDeps.zig:932-957).
    let c_path = out_dir.join("rustty_resources.c");
    let h_path = out_dir.join("rustty_resources.h");
    glib_compile_source(&assets, &xml_path, &c_path);
    glib_compile_header(&assets, &xml_path, &h_path);

    // 4. cc-compile the generated blob into the crate (SharedDeps.zig:750-753).
    let mut build = cc::Build::new();
    // Task F-1 (N353): clang-cl, not cl.exe, on an MSVC target (the reference
    // compiles every C unit with Zig's clang); a no-op elsewhere.
    build.prefer_clang_cl_over_msvc(true).file(&c_path);
    for inc in gio_include_dirs() {
        build.include(inc);
    }
    build.compile("rustty_resources");

    // 5. Validity-check every po/*.po with `msgfmt --check` (WS13 T9). The
    //    INSTALL half of GhosttyI18n.zig:33-43 is `xtask resources` ->
    //    `res_misc::gen_locale`; see `check_translations` for why this leg is a
    //    check and no longer an emit.
    check_translations(&manifest_dir);
}

/// `/` + bundle id with every `.` replaced by `/` — the reverse-DNS path form
/// the reference literal `/com/mitchellh/ghostty` encodes (`info.zig:12`),
/// derived here from the bundle id (R2). Matches `build_info::RESOURCE_PATH`.
fn resource_path(bundle_id: &str) -> String {
    let mut s = String::with_capacity(bundle_id.len() + 1);
    s.push('/');
    for ch in bundle_id.chars() {
        s.push(if ch == '.' { '/' } else { ch });
    }
    s
}

/// The installed `libadwaita-1` `{major, minor}` from pkg-config. This is
/// rustty's runtime-shared-library analogue of the reference's comptime
/// `adwaita_version` (`blueprint.zig:28-32`, read from `adwaita.h`); it backs
/// invariant 7.
fn adw_version() -> (u64, u64) {
    let out = Command::new("pkg-config")
        .args(["--modversion", "libadwaita-1"])
        .output()
        .expect("run pkg-config --modversion libadwaita-1");
    assert!(
        out.status.success(),
        "pkg-config --modversion libadwaita-1 failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = String::from_utf8_lossy(&out.stdout);
    let (major, minor, _) = parse_version(v.trim());
    (major, minor)
}

/// Port of `blueprint.zig:71-109` (post-ef7edc4a2, was `:69-121`): require
/// `blueprint-compiler >= 0.16`, with the reference's not-found and
/// wrong-version diagnostics.
fn blueprint_compiler_version_gate() {
    let out = match Command::new("blueprint-compiler").arg("--version").output() {
        Ok(out) => out,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            panic!("`blueprint-compiler` not found.{BLUEPRINT_HELP}");
        }
        Err(e) => panic!("failed to run `blueprint-compiler --version`: {e}"),
    };
    assert!(
        out.status.success(),
        "`blueprint-compiler --version` exited with a failure status"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let version = parse_version(stdout.trim());
    assert!(
        version >= REQUIRED_BLUEPRINT,
        "`blueprint-compiler` is the wrong version (found {}.{}.{}).{BLUEPRINT_HELP}",
        version.0,
        version.1,
        version.2
    );
}

/// Port of `blueprint-compiler compile --output`. Pre-ef7edc4a2 this lived at
/// `blueprint.zig:123-173`, in the same wrapper as the version gates above;
/// that commit removed compilation from the wrapper entirely (it now only
/// writes a cache stamp, see `blueprint.zig`'s module doc quoted above), and
/// the reference now runs `blueprint-compiler compile --output` directly from
/// the build graph in `SharedDeps.zig`'s `gtkNgDistResourcesUncached`
/// (`:1090-1105`) — this script already called `blueprint-compiler` directly
/// here rather than through a compiled wrapper, so upstream converged on the
/// same shape; no behaviour change on rustty's side.
fn compile_blueprint(input: &Path, output: &Path) {
    let out = match Command::new("blueprint-compiler")
        .arg("compile")
        .arg("--output")
        .arg(output)
        .arg(input)
        .output()
    {
        Ok(out) => out,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            panic!("`blueprint-compiler` not found.{BLUEPRINT_HELP}");
        }
        Err(e) => panic!("failed to run `blueprint-compiler compile`: {e}"),
    };
    assert!(
        out.status.success(),
        "`blueprint-compiler` failed for {}:\n{}",
        input.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Build the gresource XML with the three sections `genRoot`/`genIcons`/`genUi`
/// generate (`gresource.zig:212-277`), all prefixed by the identity resource
/// path.
fn gen_xml(resource_path: &str, base_app_id: &str, ui_files: &[(&Blueprint, PathBuf)]) -> String {
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gresources>\n");

    // Generate the resources at the root prefix.
    //
    // genRoot (gresource.zig:212-237): the css, compressed, aliased to the bare
    // filename, bodies relative to --sourcedir (assets root).
    s.push_str(&format!("  <gresource prefix=\"{resource_path}\">\n"));
    for name in CSS {
        s.push_str(&format!(
            "    <file compressed=\"true\" alias=\"{name}\">css/{name}</file>\n"
        ));
    }
    s.push_str("  </gresource>\n");

    // Generate the icon resources. This works by looking up all the icons
    // specified by `icon_sizes` in `images/icons/`. They are asserted to exist
    // by trying to access the file.
    //
    // (PORT NOTE: the existence assertions are hoisted to
    // `tests/gtk_assets.rs`; `glib-compile-resources` also fails on a missing
    // `<file>`.)
    //
    // genIcons (gresource.zig:170-208): the icons at 1x and 2x, uncompressed,
    // aliased icons/{s}x{s}[@2]/apps/<app-id>.png.
    s.push_str(&format!("  <gresource prefix=\"{resource_path}/icons\">\n"));
    for &size in ICON_SIZES {
        s.push_str(&format!(
            "    <file alias=\"{size}x{size}/apps/{base_app_id}.png\">images/gnome/{size}.png</file>\n"
        ));
        let two = size * 2;
        s.push_str(&format!(
            "    <file alias=\"{size}x{size}@2/apps/{base_app_id}.png\">images/gnome/{two}.png</file>\n"
        ));
    }
    s.push_str("  </gresource>\n");

    // Generate all the UI resources. This works by looking up all the
    // blueprint files in `${ui_path}/{major}.{minor}/{name}.blp` and
    // assuming these will be
    //
    // /// Generate all the UI resources.
    // ///
    // /// Each file is written as a *relative* path, `{major}.{minor}/{name}.ui`,
    // /// which `glib-compile-resources` resolves against a `--sourcedir`. An
    // /// absolute path into the Zig cache would change whenever the step that
    // /// produced the `.ui` did, and this XML with it.
    // ///
    // /// The compiled `.ui` files are not inputs here for the same reason. We
    // /// check that the blueprint *sources* exist instead, as `genRoot` and
    // /// `genIcons` do for the CSS and icons.
    // fn genUi(io: std.Io, writer: *std.Io.Writer) !void {
    //     // Two `comptimePrint` calls per blueprint exceed the default quota.
    //     @setEvalBranchQuota(100_000);
    //
    // PORT NOTE (gresource.zig:218-230 at the Phase-7 pin, ef7edc4a2): quoted, not
    // followed. The relative-path form exists for the Zig build cache, whose step
    // outputs move with their inputs; this build script compiles the blueprints
    // into OUT_DIR itself, in the same run that writes this XML, so the absolute
    // OUT_DIR body below is as stable as the XML (both are rewritten together).
    // There is no comptime evaluation quota in Rust.
    //
    // genUi (gresource.zig:242-277): each compiled .ui, compressed +
    // xml-stripblanks, aliased {maj}.{min}/{name}.ui; the body is the absolute
    // OUT_DIR path (outside --sourcedir). The reference's per-file manifest
    // lookup ends in a not-found arm (`gresource.zig:264-268`) that cannot fire
    // here, because each `ui` is already paired with its `Blueprint` (step 1):
    //   // The for loop never broke which means it didn't find
    //   // a matching blueprint for this input.
    //   return error.BlueprintNotFound;
    s.push_str(&format!("  <gresource prefix=\"{resource_path}/ui\">\n"));
    for (bp, ui) in ui_files {
        s.push_str(&format!(
            "    <file compressed=\"true\" preprocess=\"xml-stripblanks\" alias=\"{}.{}/{}.ui\">{}</file>\n",
            bp.major,
            bp.minor,
            bp.name,
            ui.display()
        ));
    }
    s.push_str("  </gresource>\n");

    s.push_str("</gresources>\n");
    s
}

/// `glib-compile-resources --c-name rustty --generate-source`.
///
/// `--sourcedir` is set to the assets root so the XML's relative css/icon
/// `<file>` bodies resolve (SharedDeps relies on cwd = repo root;
/// `SharedDeps.zig:932-947`).
///
/// Faithful to the reference, `--manual-register` is NOT passed: the generated
/// `.c` initializes the static `GResource` via a `__attribute__((constructor))`
/// at load, so `rustty_get_resource()` returns a valid pointer that
/// [`register_resources`](../src/gtk.rs) then hands to `gio::resources_register`
/// — the exact `resourcesRegister(get_resource())` shape of the class-init block
/// (`class/application.zig:1867-1881`). (With `--manual-register` the constructor
/// is suppressed and `get_resource()` would return null until a separate
/// `rustty_register_resource()` call — not the shape the reference / brief use.)
fn glib_compile_source(sourcedir: &Path, xml: &Path, target: &Path) {
    run_glib_compile(&["--generate-source"], sourcedir, xml, target);
}

/// `glib-compile-resources --c-name rustty --generate-header` (SharedDeps.zig
/// :949-957). The header only declares `rustty_get_resource`, so it takes no
/// `--manual-register`.
fn glib_compile_header(sourcedir: &Path, xml: &Path, target: &Path) {
    run_glib_compile(&["--generate-header"], sourcedir, xml, target);
}

fn run_glib_compile(mode: &[&str], sourcedir: &Path, xml: &Path, target: &Path) {
    let out = Command::new("glib-compile-resources")
        .args(["--c-name", "rustty"])
        .args(mode)
        .arg("--sourcedir")
        .arg(sourcedir)
        .arg("--target")
        .arg(target)
        .arg(xml)
        .output()
        .expect("run glib-compile-resources");
    assert!(
        out.status.success(),
        "glib-compile-resources {mode:?} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The `-I` include directories `gio-2.0` needs; the generated resource `.c`
/// `#include <gio/gio.h>`. Read from pkg-config (the gio libraries themselves
/// are linked by the crate's `gio` dependency at final link).
fn gio_include_dirs() -> Vec<PathBuf> {
    let out = Command::new("pkg-config")
        .args(["--cflags", "gio-2.0"])
        .output()
        .expect("run pkg-config --cflags gio-2.0");
    assert!(
        out.status.success(),
        "pkg-config --cflags gio-2.0 failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .filter_map(|flag| flag.strip_prefix("-I").map(PathBuf::from))
        .collect()
}

/// Port-adjacent VALIDITY CHECK (not an install step).
///
/// `GhosttyI18n.zig:33-43` compiles `po/*.po` and INSTALLS the result into
/// `share/locale`. In rustty that install is `xtask resources` ->
/// `res_misc::gen_locale`, which is what actually ships and what the macOS
/// Copy-Resources phase references.
///
/// This `build.rs` leg used to ALSO run `msgfmt`, writing `.mo` files into
/// `OUT_DIR/locale/<locale>/LC_MESSAGES/<domain>.mo` — and nothing anywhere
/// read them (`grep -rn OUT_DIR crates/rustty-apprt/src crates/rustty-apprt/tests`
/// -> no hits). That made it a vacuous artifact: it burned build time producing
/// output no code path could observe, while the only real value it delivered
/// was the side effect of `msgfmt` failing the build on a malformed catalog.
///
/// It now delivers exactly that value and nothing else: `msgfmt --check` per
/// catalog, output to the null device. `--check` is strictly stronger than the
/// old plain `-o` invocation: it turns on `--check-header` and `--check-domain`
/// as well as `--check-format`. Measured — a catalog missing
/// `Project-Id-Version` / `MIME-Version` / `Content-Transfer-Encoding` passes
/// plain `msgfmt -o` with `rc=0` and silence, and is diagnosed by
/// `msgfmt --check`.
///
/// Demonstrated FAIL arm (WS13 T9, on the Linux+gtk host where this leg runs):
/// appending an entry whose `msgstr` format specifier disagrees with its
/// `msgid` makes `cargo build -p rustty-apprt --features gtk` fail with
/// `msgfmt --check rejected …/po/de.po: … format specifications in 'msgid' and
/// 'msgstr' for argument 1 are not the same`.
///
/// Missing `po/` remains a warn-and-skip: the gresource pipeline, not
/// translations, is this build script's gate.
fn check_translations(manifest_dir: &Path) {
    let po_dir = manifest_dir.join("../../po");
    let entries = match fs::read_dir(&po_dir) {
        Ok(entries) => entries,
        Err(_) => {
            println!(
                "cargo:warning=po directory not found at {}; skipping translation checks",
                po_dir.display()
            );
            return;
        }
    };

    let mut po_files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("po"))
        .collect();
    po_files.sort();

    for po in &po_files {
        println!("cargo:rerun-if-changed={}", po.display());

        let out = Command::new("msgfmt")
            .arg("--check")
            .arg("--statistics")
            .arg("-o")
            .arg(if cfg!(windows) { "NUL" } else { "/dev/null" })
            .arg(po)
            .output()
            .expect("run msgfmt");
        assert!(
            out.status.success(),
            "msgfmt --check rejected {}:\n{}",
            po.display(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// Parse a `major.minor.patch` version string (ignoring any pre-release/build
/// suffix and any leading program-name token). Missing components default to 0.
fn parse_version(s: &str) -> (u64, u64, u64) {
    let token = s.split_whitespace().last().unwrap_or(s);
    let core = token.split(['-', '+']).next().unwrap_or(token);
    let mut it = core.split('.');
    let major = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    let minor = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    let patch = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}
