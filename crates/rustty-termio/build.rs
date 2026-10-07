//! build.rs — RUSTTY-ONLY (N506): `cfg(rustty_asan)` when this crate is built
//! under `-Zsanitizer=address` (cargo exposes rustc's `sanitize="address"` cfg
//! to build scripts as `CARGO_CFG_SANITIZE`; `cfg(sanitize)` itself is
//! unstable). It selects `Command::asan_posix_spawn` on macOS, which spawns
//! the pty child without `fork`, so the macOS ASan runtime's allocator locks
//! around `fork` cannot deadlock the io thread. The same shape as
//! `rustty-cli`'s build.rs (N495). Normal builds never set it.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(rustty_asan)");
    println!("cargo::rerun-if-env-changed=CARGO_CFG_SANITIZE");
    println!("cargo::rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_SANITIZE").is_ok_and(|v| v.split(',').any(|s| s == "address")) {
        println!("cargo::rustc-cfg=rustty_asan");
    }
}
