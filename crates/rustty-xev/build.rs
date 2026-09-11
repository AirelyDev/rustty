//! build.rs — make the deferral-ledger tripwire in `kqueue.rs` survive a source
//! tarball (parity S63-F8).
//!
//! `kqueue_file_io_on_thread_pool` is a skip-stub guarded by a compile-time
//! embed of `.port/phase-2/DEFERRALS.md`, so that deleting the ledger entry
//! fails the build rather than silently leaving a stub with no reason. That
//! embed was `include_str!`, which HARD-FAILS when the file is absent — and
//! `.port/` is deliberately excluded from the source tarball, so the shipped
//! tarball could not compile its own tests. Nothing noticed, because
//! `xtask distcheck` only BUILT the extracted tree; the reference's check step
//! runs `zig build test` in it.
//!
//! So the ledger is probed HERE instead: in the repo the tripwire is unchanged
//! (a missing entry still fails the test), and where there is no ledger to
//! check — a source tarball — the test skips that assertion instead of failing
//! to compile.

fn main() {
    let ledger =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.port/phase-2/DEFERRALS.md");
    println!("cargo:rerun-if-changed={}", ledger.display());

    let (present, ledgered) = match std::fs::read_to_string(&ledger) {
        Ok(contents) => (
            true,
            contents.contains("macOS `xev.File`/`TCP`/`UDP` watcher surface")
                && contents.contains("file IO on thread pool"),
        ),
        Err(_) => (false, false),
    };
    println!(
        "cargo:rustc-env=RUSTTY_XEV_DEFERRALS_PRESENT={}",
        u8::from(present)
    );
    println!(
        "cargo:rustc-env=RUSTTY_XEV_DEFERRAL_LEDGERED={}",
        u8::from(ledgered)
    );
}
