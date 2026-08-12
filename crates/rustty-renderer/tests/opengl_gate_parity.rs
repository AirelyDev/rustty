//! WHOLE-CRATE source guard: the OpenGL backend's target gates must name FreeBSD
//! wherever they name Linux. [W2D / codex-vet F-003]
//!
//! WHAT THE PROPERTY PROTECTS.
//! The oracle picks the graphics API purely by "is this Darwin?":
//! `oracle@6ad1fe7d:src/renderer/backend.zig:20-21`
//! ```zig
//! if (target.os.tag.isDarwin()) return .metal;
//! return .opengl;
//! ```
//! and `src/renderer.zig:37-41` instantiates `GenericRenderer(OpenGL)` for that
//! `.opengl` arm. FreeBSD is a real windowed target at the pin
//! (`src/apprt/runtime.zig:18` — `.linux, .freebsd => .gtk`), so the oracle's
//! FreeBSD build resolves the whole OpenGL renderer. Rustty's L4 milestone shipped
//! that arm gated `target_os = "linux"` alone because the macOS-first plan put the
//! GTK tier last; the result (codex-vet F-003) was that `rustty_renderer::Renderer`
//! did not resolve AT ALL on FreeBSD — a silent, compile-time-only divergence that
//! no host test could see, because every gated arm simply vanished.
//!
//! The fix is a set of gate widenings spread over five files (crate root, `generic`,
//! `image`, `overlay`, `Cargo.toml`, `build.rs`). Gate sets that must move in
//! lock-step ROT: the L4 image twin and the overlay's `pending_image` leaf were each
//! widened to Linux in a later batch and each had to be re-widened here. This guard
//! makes the lock-step mechanical: any `cfg` predicate in the crate that names
//! `target_os = "linux"` must also name `target_os = "freebsd"`.
//!
//! WHY A SOURCE GUARD AND NOT A TYPE-LEVEL ASSERTION.
//! The property is about code that is NOT compiled on this host (or on any host
//! that can run a test today): a missing FreeBSD arm is invisible to a macOS or
//! Linux test binary by construction, and a `cargo check --target
//! x86_64-unknown-freebsd` of this crate cannot run to completion on the port's
//! dev hosts (the vendored `-sys` C dependencies need a FreeBSD sysroot). Reading
//! the gates as text is the strongest check that actually EXECUTES here.
//!
//! If a genuinely Linux-only gate is ever needed (something naming an API FreeBSD
//! does not have — an `eventfd`, an `epoll`), do NOT weaken this guard: add the
//! site to `LINUX_ONLY_ALLOWLIST` below with the reason, so the exception is
//! reviewed rather than absorbed.

use std::path::{Path, PathBuf};

/// Sites that are deliberately Linux-only, as `(file, substring of the line)`.
/// Empty today: every OpenGL-arm gate in this crate serves both targets.
const LINUX_ONLY_ALLOWLIST: &[(&str, &str)] = &[];

/// The cfg predicate fragment the guard looks for.
///
/// Assembled at runtime rather than written as one literal so that this file's own
/// prose (which necessarily quotes the gate) cannot be mistaken for a code site by
/// a future reader — the scan skips comments, but the assembly keeps the two
/// unambiguous.
fn linux_pred() -> String {
    format!("target_os = {}linux{}", '"', '"')
}

fn freebsd_pred() -> String {
    format!("target_os = {}freebsd{}", '"', '"')
}

/// Every source file the guard scans: all of `src/**/*.rs`, plus the manifest (its
/// `[target.'cfg(...)'.dependencies]` table gates the `gl`/`khronos-egl` deps) and
/// the build script (its `CARGO_CFG_TARGET_OS` branch stages the GLSL assets).
fn scanned_files() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut out = vec![root.join("Cargo.toml"), root.join("build.rs")];
    collect_rs(&root.join("src"), &mut out);
    out.sort();
    out
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {dir:?}: {e}"));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// True for a line that carries no code: a Rust `//`-comment or a TOML `#`-comment.
/// The guard is about GATES, not prose — every file here documents its gate in the
/// comment right above it, and those quotes must not be scanned.
fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") || t.starts_with('#') && !t.starts_with("#[") && !t.starts_with("#!")
}

/// The rule: a code line that names Linux in a cfg predicate names FreeBSD too.
///
/// One line per predicate is the shape rustfmt produces for every gate in this
/// crate (all of them fit in 100 columns). A hand-wrapped multi-line predicate
/// would fail this guard — correctly: it should be reformatted, since a wrapped
/// gate is exactly how a half-widened predicate hides.
#[test]
fn every_linux_gate_also_names_freebsd() {
    let linux = linux_pred();
    let freebsd = freebsd_pred();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let mut violations: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for file in scanned_files() {
        let src = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("read {file:?}: {e}"));
        let rel = file
            .strip_prefix(&root)
            .unwrap_or(&file)
            .to_string_lossy()
            .into_owned();

        for (i, line) in src.lines().enumerate() {
            if is_comment(line) || !line.contains(&linux) {
                continue;
            }
            if LINUX_ONLY_ALLOWLIST
                .iter()
                .any(|(f, needle)| *f == rel && line.contains(needle))
            {
                continue;
            }
            checked += 1;
            if !line.contains(&freebsd) {
                violations.push(format!("{}:{}: {}", rel, i + 1, line.trim()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "cfg gates naming Linux without FreeBSD (renderer/backend.zig:20-21 gives \
         EVERY non-Darwin native target the OpenGL backend; apprt/runtime.zig:18 makes \
         FreeBSD a real windowed target). Widen the predicate, or add the site to \
         LINUX_ONLY_ALLOWLIST with a reason:\n  {}",
        violations.join("\n  ")
    );

    // Anti-vacuity: the scan must actually have SEEN the gates. If the crate is
    // ever restructured so no predicate names Linux at all, this test would pass
    // while proving nothing.
    assert!(
        checked >= 4,
        "expected at least the four OpenGL-arm gate sites (crate root `opengl` + \
         `Renderer` export, `generic::linux_impl`, `image::linux_impl`) to be scanned, \
         saw {checked} — the guard has gone vacuous"
    );
}

/// The gates the FreeBSD renderer closure actually depends on, asserted by name so
/// that deleting one (rather than narrowing it) is caught too.
///
/// These four are the load-bearing set from the F-003 evidence: without any one of
/// them a FreeBSD build fails to resolve `rustty_renderer::Renderer`, which
/// `rustty-core`'s `renderer_seam` names by the exact complement of the macOS arm
/// (`#[cfg(not(target_os = "macos"))]`).
#[test]
fn the_four_opengl_arm_gates_are_present() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let gate = format!("any({}, {})", linux_pred(), freebsd_pred());

    // (file, the item the gate must sit on)
    let sites: &[(&str, &str)] = &[
        ("src/lib.rs", "pub mod opengl;"),
        ("src/lib.rs", "pub use generic::Renderer;"),
        ("src/generic.rs", "mod linux_impl;"),
        ("src/image.rs", "mod linux_impl;"),
    ];

    for (file, item) in sites {
        let src =
            std::fs::read_to_string(root.join(file)).unwrap_or_else(|e| panic!("read {file}: {e}"));
        let mut prev_gate = false;
        let mut found = false;
        for line in src.lines() {
            let t = line.trim();
            if t == *item && prev_gate {
                found = true;
                break;
            }
            if !is_comment(line) {
                prev_gate = t.starts_with("#[cfg(") && t.contains(&gate);
            }
        }
        assert!(
            found,
            "{file}: `{item}` must be immediately preceded by `#[cfg({gate})]` — that \
             is the `.opengl` arm of renderer.zig:37-41, and FreeBSD selects it \
             (backend.zig:20-21)"
        );
    }
}

/// The build script stages the GLSL assets `opengl::shaders` embeds; its target
/// predicate must be the SAME set as the `opengl` module's gate, or a FreeBSD build
/// compiles the module without its shaders.
#[test]
fn build_script_stages_glsl_for_freebsd_too() {
    let src = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("build.rs"))
        .expect("read build.rs");
    let code: String = src
        .lines()
        .filter(|l| !is_comment(l))
        .collect::<Vec<_>>()
        .join("\n");

    let linux_arm = format!("Ok({}linux{})", '"', '"');
    let freebsd_arm = format!("Ok({}freebsd{})", '"', '"');
    assert!(
        code.contains(&linux_arm) && code.contains(&freebsd_arm),
        "build.rs must stage the GLSL assets for both OpenGL targets"
    );
    // The pre-W2D shape was an equality test against Linux alone; a regression to it
    // would silently drop the FreeBSD assets.
    assert!(
        !code.contains(&format!("== {linux_arm}")),
        "build.rs still gates the GLSL step on Linux ALONE (`== {linux_arm}`); \
         `crate::opengl` compiles on FreeBSD too (backend.zig:20-21)"
    );
}
