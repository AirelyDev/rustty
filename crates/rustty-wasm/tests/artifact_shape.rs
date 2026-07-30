//! WS14 T7 — asserts the SHAPE of the `main_wasm.zig` artifact.
//!
//! NOT a port (ghostty `main_wasm.zig` has 0 `test {}`): this is a rustty-only
//! gate over the deliverable `.wasm`, so "the artifact exists and carries the
//! reference's export set with no wasi dependency" is a checked claim rather than
//! a README sentence.
//!
//! The module is parsed HERE, in pure Rust, with no new dependency: only the
//! module header plus the import (id 2) and export (id 7) sections are decoded,
//! which is ~40 lines of LEB128. The alternative — asserting through the node
//! driver in `.port/tools/gates/ws14_webgl_browser_gate.mjs` — was rejected so
//! that `cargo test -p rustty-wasm -- --ignored` is a self-contained check that
//! needs neither node nor Chrome.
//!
//! `#[ignore]`d so `cargo test --workspace` does not require a wasm toolchain;
//! the WS14 browser gate runs it un-ignored.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <root>/crates/rustty-wasm
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn artifact_path() -> PathBuf {
    repo_root().join("target/wasm32-unknown-unknown/release/rustty_wasm.wasm")
}

/// Unsigned LEB128.
fn leb(bytes: &[u8], i: &mut usize) -> u64 {
    let mut result: u64 = 0;
    let mut shift = 0;
    loop {
        let b = bytes[*i];
        *i += 1;
        result |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return result;
        }
        shift += 7;
    }
}

fn name(bytes: &[u8], i: &mut usize) -> String {
    let len = leb(bytes, i) as usize;
    let s = String::from_utf8_lossy(&bytes[*i..*i + len]).into_owned();
    *i += len;
    s
}

/// `(imports as (module, field), export names)`.
fn parse_module(bytes: &[u8]) -> (Vec<(String, String)>, Vec<String>) {
    assert_eq!(&bytes[0..4], b"\0asm", "not a wasm module");
    assert_eq!(&bytes[4..8], &[1, 0, 0, 0], "unexpected wasm version");

    let mut imports = Vec::new();
    let mut exports = Vec::new();
    let mut i = 8usize;
    while i < bytes.len() {
        let id = bytes[i];
        i += 1;
        let size = leb(bytes, &mut i) as usize;
        let end = i + size;
        match id {
            // import section
            2 => {
                let n = leb(bytes, &mut i);
                for _ in 0..n {
                    let m = name(bytes, &mut i);
                    let f = name(bytes, &mut i);
                    imports.push((m, f));
                    let kind = bytes[i];
                    i += 1;
                    match kind {
                        // func: typeidx | table: reftype + limits | mem: limits
                        // global: valtype + mut
                        0x00 => {
                            leb(bytes, &mut i);
                        }
                        0x01 => {
                            i += 1;
                            let flags = bytes[i];
                            i += 1;
                            leb(bytes, &mut i);
                            if flags & 1 != 0 {
                                leb(bytes, &mut i);
                            }
                        }
                        0x02 => {
                            let flags = bytes[i];
                            i += 1;
                            leb(bytes, &mut i);
                            if flags & 1 != 0 {
                                leb(bytes, &mut i);
                            }
                        }
                        0x03 => {
                            i += 2;
                        }
                        other => panic!("unknown import kind {other}"),
                    }
                }
            }
            // export section
            7 => {
                let n = leb(bytes, &mut i);
                for _ in 0..n {
                    exports.push(name(bytes, &mut i));
                    i += 1; // kind
                    leb(bytes, &mut i); // index
                }
            }
            _ => {}
        }
        i = end;
    }
    (imports, exports)
}

#[test]
#[ignore = "needs the wasm artifact: run `.port/tools/build-main-wasm.sh --release` first, then `cargo test -p rustty-wasm -- --ignored`"]
fn artifact_shape() {
    let path = artifact_path();
    assert!(
        path.exists(),
        "missing {}\n\nBuild it with:\n    .port/tools/build-main-wasm.sh --release",
        path.display()
    );

    let bytes = std::fs::read(&path).expect("read artifact");
    let (imports, exports) = parse_module(&bytes);

    // Every import's module string is "env"; zero imports whose module mentions
    // wasi. This is the "true freestanding" property: the module needs a browser
    // `env` object and nothing else — no WASI shim, no libc.
    for (m, f) in &imports {
        assert_eq!(m, "env", "non-`env` import module: {m}.{f}");
        assert!(
            !m.to_lowercase().contains("wasi") && !f.to_lowercase().contains("wasi"),
            "wasi import: {m}.{f}"
        );
    }

    // The reference retention set's C-ABI exports: `config.zig`'s `Wasm` block and
    // `font/Atlas.zig`'s `Wasm` block. Names are UNPREFIXED by design (the JS host
    // binds them bare) — see the R2 exception in the WS14 plan.
    for want in [
        "config_new",
        "config_free",
        "config_load_string",
        "config_finalize",
        "atlas_new",
        "atlas_free",
        "atlas_reserve",
        "atlas_set",
        "atlas_grow",
        "atlas_clear",
        "atlas_debug_canvas",
    ] {
        assert!(
            exports.iter().any(|e| e == want),
            "missing export `{want}`; exports = {exports:?}"
        );
    }

    // `memory` must be exported so the host can read the atlas bytes back out.
    assert!(
        exports.iter().any(|e| e == "memory"),
        "the module must export its `memory`; exports = {exports:?}"
    );
}
