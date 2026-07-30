//! Asset build pipeline for the native GTK4/libadwaita apprt backend.
//!
//! PORTS the reference's TWO build helper binaries plus its `SharedDeps`
//! wiring, collapsed into one build script (behavior-preserving, not
//! line-preserving):
//!   * `src/apprt/gtk/build/blueprint.zig` — the adw-version gate (`:58-67`,
//!     invariant 7), the `blueprint-compiler --version` gate (`:69-121`) and
//!     `compile --output` (`:123-173`).
//!   * `src/apprt/gtk/build/gresource.zig` — `genRoot`/`genIcons`/`genUi`
//!     (`:212-277`) reproduced verbatim in output *shape*
//!     (`compressed="true"`, `preprocess="xml-stripblanks"`, the
//!     `{size}x{size}@2` icon aliases).
//!   * `src/build/SharedDeps.zig` `gtkNgDistResources` (`:870-967`) — the
//!     `glib-compile-resources` `--generate-source`/`--generate-header` calls
//!     and the `cc` link of the generated `*_resources.c`.
//!   * `src/build/GhosttyI18n.zig` (`:33-43`) — `msgfmt` of each `po/*.po` into
//!     `share/locale/<locale>/LC_MESSAGES/<domain>.mo`.
//!
//! # R5 / guard
//!
//! The whole pipeline runs only when BOTH `CARGO_FEATURE_GTK` and
//! `CARGO_CFG_TARGET_OS == "linux"` hold — the exact gate the `gtk` module
//! carries (`lib.rs`). On macOS, or a default (no-`gtk`) Linux build, this
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

/// Minimum `blueprint-compiler` version. Reference: `blueprint.zig:33-37`
/// (`required_blueprint_version = 0.16.0`).
const REQUIRED_BLUEPRINT: (u64, u64, u64) = (0, 16, 0);

/// Shared tail of the `blueprint-compiler` diagnostics. Reference:
/// `blueprint.zig:14-26` (`blueprint_compiler_help`), rebranded to rustty.
const BLUEPRINT_HELP: &str = "\n\nWhen building from a source checkout, rustty requires \
version 0.16 or newer of `blueprint-compiler` as a build-time dependency. Please install \
it, ensure that it is available on your PATH, and then retry building rustty.";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // R5 guard: only the native Linux+gtk build runs the asset pipeline.
    let gtk = env::var_os("CARGO_FEATURE_GTK").is_some();
    let linux = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux");
    if !(gtk && linux) {
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
    // blueprint-compiler must be >= 0.16 (blueprint.zig:69-121).
    let adw = adw_version();
    blueprint_compiler_version_gate();

    // 1. Compile each `.blp` -> `OUT_DIR/ui/{maj}.{min}/{name}.ui`.
    let mut ui_files: Vec<(&Blueprint, PathBuf)> = Vec::with_capacity(BLUEPRINTS.len());
    for bp in BLUEPRINTS {
        // Invariant 7 (blueprint.zig:58-67): the installed adwaita must be new
        // enough to compile this blueprint's `{major}.{minor}` dialect.
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
    build.file(&c_path);
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

/// Port of `blueprint.zig:69-121`: require `blueprint-compiler >= 0.16`,
/// with the reference's not-found and wrong-version diagnostics.
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

/// Port of `blueprint.zig:123-173`: `blueprint-compiler compile --output`.
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

    // genRoot (gresource.zig:212-237): the css, compressed, aliased to the bare
    // filename, bodies relative to --sourcedir (assets root).
    s.push_str(&format!("  <gresource prefix=\"{resource_path}\">\n"));
    for name in CSS {
        s.push_str(&format!(
            "    <file compressed=\"true\" alias=\"{name}\">css/{name}</file>\n"
        ));
    }
    s.push_str("  </gresource>\n");

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

    // genUi (gresource.zig:242-277): each compiled .ui, compressed +
    // xml-stripblanks, aliased {maj}.{min}/{name}.ui; the body is the absolute
    // OUT_DIR path (outside --sourcedir).
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
