//! build.rs — RUSTTY-ONLY (P7-W14): `cfg(rustty_asan)` when this crate is
//! compiled with `-Zsanitizer=address` (cargo exposes rustc's
//! `sanitize="address"` cfg to build scripts as `CARGO_CFG_SANITIZE`;
//! `cfg(sanitize)` itself is unstable). `tests/smoke.rs` needs it: on Linux an
//! instrumented cdylib leaves the `__asan_*` runtime to the executable, so the C
//! smoke program has to be linked with `-fsanitize=address` there.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(rustty_asan)");
    println!("cargo::rerun-if-env-changed=CARGO_CFG_SANITIZE");
    if std::env::var("CARGO_CFG_SANITIZE").is_ok_and(|v| v.split(',').any(|s| s == "address")) {
        println!("cargo::rustc-cfg=rustty_asan");
    }
}
