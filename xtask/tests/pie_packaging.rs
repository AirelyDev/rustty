//! `-Dpie` in the packaging (N422, `Config.zig:408-412`): the option defaults
//! to true for a system package. The reference's nix package and both flatpak
//! manifests build with `--system`, so here they set `RUSTTY_PIE=true`; its
//! snap does not, so the snap sets nothing and links the non-PIE default.

use std::path::PathBuf;

fn read(rel: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

#[test]
fn system_packages_build_pie_and_the_snap_does_not() {
    let nix = read("nix/package.nix");
    assert!(
        nix.lines().any(|l| l.trim() == "RUSTTY_PIE = \"true\";"),
        "nix/package.nix sets RUSTTY_PIE = \"true\""
    );
    for m in [
        "flatpak/io.airely.rustty.yml",
        "flatpak/io.airely.rustty-debug.yml",
    ] {
        let text = read(m);
        assert_eq!(
            text.lines()
                .filter(|l| l.trim() == "RUSTTY_PIE: \"true\"")
                .count(),
            1,
            "{m} sets RUSTTY_PIE: \"true\" once"
        );
    }
    assert!(
        !read("snap/snapcraft.yaml").contains("RUSTTY_PIE"),
        "the snap is not a system package upstream (no --system)"
    );
}
