//! build.rs — RUSTTY-ONLY (P7-W14, P7-W16b): `cfg(rustty_asan)` /
//! `cfg(rustty_msan)` / `cfg(rustty_hwasan)` when this crate is compiled with
//! `-Zsanitizer=address` / `-Zsanitizer=memory` / `-Zsanitizer=hwaddress`
//! (cargo exposes rustc's `sanitize="..."` cfg to build scripts as
//! `CARGO_CFG_SANITIZE`; `cfg(sanitize)` itself is unstable). `tests/smoke.rs`
//! needs them: on Linux an instrumented cdylib leaves the sanitizer runtime to
//! the executable, so the C smoke program has to be linked with the matching
//! `-fsanitize=` there.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(rustty_asan)");
    println!("cargo::rustc-check-cfg=cfg(rustty_msan)");
    println!("cargo::rustc-check-cfg=cfg(rustty_hwasan)");
    println!("cargo::rerun-if-env-changed=CARGO_CFG_SANITIZE");
    let sanitize = std::env::var("CARGO_CFG_SANITIZE").unwrap_or_default();
    if sanitize.split(',').any(|s| s == "address") {
        println!("cargo::rustc-cfg=rustty_asan");
    }
    if sanitize.split(',').any(|s| s == "memory") {
        println!("cargo::rustc-cfg=rustty_msan");
    }
    if sanitize.split(',').any(|s| s == "hwaddress") {
        println!("cargo::rustc-cfg=rustty_hwasan");
    }
}
