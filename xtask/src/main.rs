//! xtask — build orchestration for rustty.
//!
//! PORTS: ghostty src/build/ (34 files) + helpgen.zig — build orchestration:
//! cbindgen headers, terminfo compilation, shell-integration/theme bundling,
//! man pages, i18n, dist tarball, macOS xcframework/.app.
//!
//! PORT STATUS: stub
//!
//! Scaffold only — no real logic yet (R1). Independent of the parity port order.

fn main() {
    // Stub. Subcommands (header-gen, terminfo, dist, ...) land as the build
    // pipeline is ported.
}
