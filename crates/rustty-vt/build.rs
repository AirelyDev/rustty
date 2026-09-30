//! rustty-vt build script: resolves the Kitty graphics option against the
//! target, as the reference's `Options.kittyGraphics(target)` does
//! (`src/terminal/build_options.zig:274-277`: `if (target.os.tag ==
//! .freestanding) return false; return self.features.kitty_graphics;`).
//!
//! It emits `cfg(rustty_kitty_graphics)` when the `kitty-graphics` feature is
//! on and the target is not freestanding. Code that needs the wuffs binding
//! (the kitty pixel swizzles and zlib decoder) is compiled under that cfg,
//! because `rustty-wuffs` is only linked there (Cargo.toml's target table), as
//! the reference links wuffs into the vt module only when Kitty graphics is on
//! (`src/build/GhosttyZig.zig:140-152`). `options::KITTY_GRAPHICS` states the
//! same predicate and a test pins that the two agree.
//!
//! Freestanding: Zig's `.freestanding` is Rust's `target_os = "none"` (any
//! arch, `3376153a4`) and `wasm32-unknown-unknown` (the reference's lib-vt wasm
//! artifact is `wasm32-freestanding`; Ruling 30, amending DECISIONS B8).

fn main() {
    println!("cargo::rustc-check-cfg=cfg(rustty_kitty_graphics)");
    println!("cargo:rerun-if-changed=build.rs");

    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let freestanding = os == "none" || (arch == "wasm32" && os == "unknown");
    let feature = std::env::var_os("CARGO_FEATURE_KITTY_GRAPHICS").is_some();
    if feature && !freestanding {
        println!("cargo::rustc-cfg=rustty_kitty_graphics");
    }
}
