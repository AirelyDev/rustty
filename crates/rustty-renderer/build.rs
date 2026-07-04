//! Build script for rustty-renderer.
//!
//! Mirrors ghostty's `src/renderer/shaders/MetallibStep.zig`: on macOS it
//! compiles the vendored Metal shader source `src/shaders/shaders.metal` into a
//! `.metallib` with two `xcrun` calls —
//!
//!     xcrun -sdk macosx metal    -o $OUT_DIR/shaders.ir     -c src/shaders/shaders.metal
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

use std::path::PathBuf;
use std::process::Command;

/// The Metal shader source we compile / embed.
const SHADER_SRC: &str = "src/shaders/shaders.metal";

fn main() {
    // Declare the cfg we may set so `--check-cfg` (CI's `-D warnings`) does not
    // warn about an unexpected cfg name.
    println!("cargo::rustc-check-cfg=cfg(rustty_metallib)");

    // Recompile the metallib when the shader source changes.
    println!("cargo::rerun-if-changed={SHADER_SRC}");

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

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR set by cargo"));
    let ir = out_dir.join("shaders.ir");
    let metallib = out_dir.join("rustty.metallib");

    // xcrun -sdk macosx metal -o $OUT_DIR/shaders.ir -c src/shaders/shaders.metal
    if !run(Command::new("xcrun")
        .args(["-sdk", "macosx", "metal", "-o"])
        .arg(&ir)
        .arg("-c")
        .arg(SHADER_SRC))
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
