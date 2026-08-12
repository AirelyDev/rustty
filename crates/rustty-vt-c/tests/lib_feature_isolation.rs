//! lib_feature_isolation.rs — the lib-vt null decode_png default, proven from
//! the C-artifact crate's OWN build graph (codex-vet W36-B03).
//!
//! ORACLE: ghostty src/terminal/sys.zig:16-20 comptime-defaults `decode_png`
//! to `null` when `build_options.artifact == .lib`, and
//! src/build/GhosttyZig.zig:67-68 builds libghostty-vt with
//! `vt_options.artifact = .lib` unconditionally — the published lib-vt C
//! artifact NEVER has a built-in PNG decoder, regardless of what the app tier
//! links. Rustty maps the artifact split onto the crate graph instead of a
//! Zig build option: rustty-core (the SharedDeps `.ghostty` analog) enables
//! rustty-vt's `wuffs` feature; rustty-vt-c (the GhosttyZig `.lib` analog)
//! must NOT, so that an ISOLATED `cargo build -p rustty-vt-c` — the
//! artifact-publishing selection — compiles a rustty-vt whose
//! `DECODE_PNG_DEFAULT` is `None` (rustty-vt/src/sys.rs:62-76).
//!
//! THE HAZARD THIS TEST PINS: Cargo feature UNIFICATION. A combined selection
//! that includes both tiers (e.g. `cargo build --workspace`, or
//! `-p rustty-vt-c -p rustty-core`) resolves ONE rustty-vt with `wuffs`
//! unified in, silently flipping the C artifact's default decoder to
//! `Some(decode_png_wuffs)` — a divergence Ghostty cannot exhibit because its
//! lib artifact is a separate compilation. The publish rule is therefore:
//! librustty-vt artifacts come from isolated `-p rustty-vt-c` invocations
//! only. This test is the Rust half of that rule's enforcement; the build-
//! graph half is `.port/tools/gates/check_vtc_lib_isolation.sh` (G-VTCLIB),
//! which also owns RUNNING this test.
//!
//! WHY `#[ignore]` + own integration-test binary:
//!   - own process: the vt-c lib unit tests legitimately mutate the
//!     `rustty_vt::sys::decode_png` slot (the `set(.decode_png, ..)` C-API
//!     tests); this binary reads the pristine startup value.
//!   - `#[ignore]`: the assertion is a fact about the SELECTION the binary was
//!     compiled under, which the test cannot sense from inside. Under the
//!     gate's isolated `-p rustty-vt-c` run it MUST hold; under a combined
//!     `cargo test --workspace` run the unified graph makes it legitimately
//!     false (that build is not a release artifact). So the default sweep
//!     reports it honestly as ignored, and G-VTCLIB runs it `--ignored` under
//!     the one selection where it is binding. The gate's `--self-test` proves
//!     the fail arm by re-running it with `--features rustty-vt/wuffs`
//!     (simulated unification), which MUST go red.

/// Marker line the gate greps for (the lz4 `RUSTTY_LZ4_SLOW_RAN` anti-vacuity
/// pattern): "1 passed" alone cannot distinguish this assertion from a filter
/// typo selecting nothing, so the test names the fact it just proved.
#[test]
#[ignore = "binding only under isolated -p rustty-vt-c selection; run by .port/tools/gates/check_vtc_lib_isolation.sh (G-VTCLIB)"]
fn isolated_vtc_selection_keeps_lib_null_decode_default() {
    // The pristine startup default: sys.zig:16-20's `.lib => null` arm. If
    // this reads `Some`, a `wuffs`-unified rustty-vt was linked into the
    // C-artifact crate — the W36-B03 divergence.
    let installed = *rustty_vt::sys::decode_png.read().unwrap();
    assert!(
        installed.is_none(),
        "rustty-vt-c linked a rustty-vt with a built-in PNG decoder \
         (feature `wuffs` unified in). Ghostty's lib-vt artifact default is \
         ALWAYS null (sys.zig:16-20; GhosttyZig.zig:67-68 pins artifact=.lib). \
         Either a `wuffs` edge was added to rustty-vt-c's dep line, or this \
         binary was built under a COMBINED selection — publish/gate builds \
         must use isolated `cargo <cmd> -p rustty-vt-c` (see \
         .port/tools/gates/check_vtc_lib_isolation.sh, G-VTCLIB)."
    );
    println!("RUSTTY_VTC_LIB_NULL_DEFAULT_RAN decode_png=None");
}
