//! Build script for rustty-renderer.
//!
//! Mirrors ghostty's `src/renderer/shaders/MetallibStep.zig`: on macOS it
//! compiles the vendored Metal shader source `src/shaders/shaders.metal` into a
//! `.metallib` with two `xcrun` calls —
//!
//!     xcrun -sdk macosx metal    -o $OUT_DIR/shaders.ir     -c src/shaders/shaders.metal -mmacos-version-min=13.0.0
//!     xcrun -sdk macosx metallib -o $OUT_DIR/rustty.metallib   $OUT_DIR/shaders.ir
//!
//! On success it sets the `rustty_metallib` cfg so the Chunk-1 runtime embeds
//! the compiled library with `include_bytes!` and loads it via
//! `newLibraryWithData` (the byte-faithful mirror of ghostty's
//! `@embedFile("ghostty_metallib")`). When the Metal toolchain is absent (bare
//! Command-Line-Tools without the Metal compiler component — a real CI case),
//! the metal calls are SKIPPED (not failed): the runtime falls back to
//! `newLibraryWithSource` over the same `shaders.metal` source, which produces
//! identical pixels without the toolchain (GPU_RD_BLUEPRINT §3). Non-macOS is a
//! no-op for the same reason.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The Metal shader source we compile / embed.
const SHADER_SRC: &str = "src/shaders/shaders.metal";

/// `Config.zig:789-797` `osVersionMin(.macos)`: "We support back to the
/// earliest officially supported version of macOS by Apple. EOL versions are
/// not supported." Formatted as the reference's `{f}` of the semver.
const MACOS_VERSION_MIN: &str = "13.0.0";

/// The directory holding the verbatim-vendored GLSL assets (Linux/OpenGL).
const GLSL_DIR: &str = "src/shaders/glsl";

fn main() {
    // Declare the cfg we may set so `--check-cfg` (CI's `-D warnings`) does not
    // warn about an unexpected cfg name.
    println!("cargo::rustc-check-cfg=cfg(rustty_metallib)");

    // Recompile the metallib when the shader source changes.
    println!("cargo::rerun-if-changed={SHADER_SRC}");

    // Linux/FreeBSD (i.e. OpenGL): pre-resolve the GLSL `#include`s and stage the
    // results in `$OUT_DIR/glsl/` for `opengl::shaders` to `include_bytes!`. This
    // ports ghostty's comptime `loadShaderCode`/`processIncludes`
    // (`src/renderer/opengl/shaders.zig:341-376`) to build time.
    //
    // W2D/F-003: the predicate must be the exact set of targets on which
    // `crate::opengl` is compiled (`any(target_os = "linux", target_os =
    // "freebsd")` — `renderer/backend.zig:20-21`'s non-Darwin `.opengl` arm),
    // otherwise the FreeBSD build compiles the module without its staged assets.
    if matches!(
        std::env::var("CARGO_CFG_TARGET_OS").as_deref(),
        Ok("linux") | Ok("freebsd")
    ) {
        resolve_glsl_includes();
    }

    // Metal shader compilation is macOS-only; elsewhere the source-compile
    // fallback covers it.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    // GATE on the Metal toolchain being present (`xcrun -sdk macosx -f metal`).
    // If absent, SKIP without setting the cfg — the runtime source fallback
    // covers it, so the build must not fail.
    if !metal_toolchain_available() {
        return;
    }

    // `-mmacos-version-min=<min_version>` (`MetallibStep.zig:29-56`), where
    // min_version is the target's `os_version_min`. An explicit
    // `MACOSX_DEPLOYMENT_TARGET` is the port's spelling of a user-supplied
    // minimum (rustc reads the same variable for the Rust objects);
    // otherwise it is `Config.zig:789-797` `osVersionMin(.macos)`, 13.0.0,
    // which `Config.zig:145-150` fills in whenever the query has none
    // (S64-F2). The value is exported so a test can check the metallib
    // header carries it.
    println!("cargo::rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    let min_version = std::env::var("MACOSX_DEPLOYMENT_TARGET")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| MACOS_VERSION_MIN.to_string());
    println!("cargo::rustc-env=RUSTTY_METALLIB_VERSION_MIN={min_version}");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR set by cargo"));
    let ir = out_dir.join("shaders.ir");
    let metallib = out_dir.join("rustty.metallib");

    // xcrun -sdk macosx metal -o $OUT_DIR/shaders.ir -c src/shaders/shaders.metal -mmacos-version-min=<min>
    if !run(Command::new("xcrun")
        .args(["-sdk", "macosx", "metal", "-o"])
        .arg(&ir)
        .arg("-c")
        .arg(SHADER_SRC)
        .arg(format!("-mmacos-version-min={min_version}")))
    {
        println!("cargo::warning=rustty-renderer: `xcrun metal` failed to compile {SHADER_SRC}; falling back to runtime newLibraryWithSource");
        return;
    }

    // xcrun -sdk macosx metallib -o $OUT_DIR/rustty.metallib $OUT_DIR/shaders.ir
    if !run(Command::new("xcrun")
        .args(["-sdk", "macosx", "metallib", "-o"])
        .arg(&metallib)
        .arg(&ir))
    {
        println!("cargo::warning=rustty-renderer: `xcrun metallib` failed; falling back to runtime newLibraryWithSource");
        return;
    }

    // Success: the embedded metallib is available at $OUT_DIR/rustty.metallib.
    println!("cargo::rustc-cfg=rustty_metallib");
}

/// True iff the Metal compiler component is installed (`xcrun -sdk macosx -f
/// metal` resolves the tool and exits 0).
fn metal_toolchain_available() -> bool {
    Command::new("xcrun")
        .args(["-sdk", "macosx", "-f", "metal"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run `cmd`, returning true on a zero exit status. A spawn error (e.g. `xcrun`
/// missing) is treated as failure (→ skip), never a panic.
fn run(cmd: &mut Command) -> bool {
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

/// Ports ghostty's comptime `loadShaderCode` (`opengl/shaders.zig:347-376`) to
/// build time: for every vendored `src/shaders/glsl/*.glsl`, inline its
/// `#include "…"` directives (only `common.glsl` is ever included) and write the
/// resolved, NUL-terminated result to `$OUT_DIR/glsl/*.glsl`. `opengl::shaders`
/// then `include_bytes!`s those as the `[:0]const u8` GLSL sources.
///
/// The trailing NUL makes each output a valid C string (the reference's
/// `[:0]const u8`); the vendored SOURCE files stay pure (NUL-free) so the
/// byte-for-byte reference-diff test compares clean bytes.
fn resolve_glsl_includes() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let src_dir = manifest.join(GLSL_DIR);
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR set by cargo"));
    let dst_dir = out_dir.join("glsl");
    std::fs::create_dir_all(&dst_dir).expect("create $OUT_DIR/glsl");

    // Rebuild when any asset changes.
    println!("cargo::rerun-if-changed={GLSL_DIR}");

    let entries = std::fs::read_dir(&src_dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", src_dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("glsl") {
            continue;
        }
        let name = path.file_name().expect("file name");
        println!("cargo::rerun-if-changed={}", path.display());

        let contents =
            std::fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        let mut resolved = process_includes(&contents, &src_dir);
        resolved.push(0); // NUL terminator (the reference's `[:0]const u8`).
        std::fs::write(dst_dir.join(name), &resolved)
            .unwrap_or_else(|e| panic!("writing resolved {}: {e}", name.to_string_lossy()));
    }
}

/// Ports `processIncludes` (`opengl/shaders.zig:352-376`) byte-for-byte: walk the
/// source, and at the first line-leading `#include "<file>"`, splice in the RAW
/// bytes of `<basedir>/<file>` (never re-processed — matching the reference's
/// `@embedFile` of the include), then recurse over the tail. `#` tokens that are
/// not line-leading (e.g. `common.glsl`'s in-comment mention) are skipped exactly
/// as the reference's `"\n#"` scan skips them.
fn process_includes(contents: &[u8], basedir: &Path) -> Vec<u8> {
    const NEEDLE: &[u8] = b"#include \"";
    let mut i = 0usize;
    loop {
        if contents[i..].starts_with(b"#include") {
            // The reference asserts the well-formed `#include "` shape.
            assert!(
                contents[i..].starts_with(NEEDLE),
                "malformed #include directive (expected `#include \"`)",
            );
            let start = i + NEEDLE.len();
            let end = start
                + contents[start..]
                    .iter()
                    .position(|&b| b == b'"')
                    .expect("unterminated #include path");
            let inc_name = std::str::from_utf8(&contents[start..end]).expect("include path utf8");
            let inc_path = basedir.join(inc_name);
            let included = std::fs::read(&inc_path)
                .unwrap_or_else(|e| panic!("reading include {}: {e}", inc_path.display()));

            let mut out = Vec::with_capacity(contents.len() + included.len());
            out.extend_from_slice(&contents[0..i]);
            out.extend_from_slice(&included);
            out.extend_from_slice(&process_includes(&contents[end + 1..], basedir));
            return out;
        }
        // Advance to the next line-leading `#`, mirroring `indexOfPos(contents, i,
        // "\n#")` (`shaders.zig:369-374`).
        match find_subslice(contents, i, b"\n#") {
            Some(j) => i = j + 1,
            None => break,
        }
    }
    contents.to_vec()
}

/// `std.mem.indexOfPos(u8, haystack, from, needle)` — the byte offset of the first
/// `needle` at or after `from`, if any.
fn find_subslice(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from > haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| from + p)
}
