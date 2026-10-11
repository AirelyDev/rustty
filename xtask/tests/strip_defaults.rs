//! `-Dstrip`'s per-optimize default (N422, `src/build/Config.zig:414-422`):
//!
//! ```text
//! config.strip = b.option(bool, "strip", "Strip the final executable. Default
//!     true for fast and small releases") orelse switch (optimize) {
//!     .Debug => false,
//!     .ReleaseSafe => false,
//!     .ReleaseFast, .ReleaseSmall => true,
//! };
//! ```
//!
//! Every Cargo profile of the workspace maps to one Zig optimize mode by the
//! rule `rustty_build_config::mode_string_for` applies (opt-level 0 = Debug,
//! `s`/`z` = ReleaseSmall, otherwise debug-assertions = ReleaseSafe, else
//! ReleaseFast). This reads the root manifest, resolves each profile through
//! its `inherits` chain and Cargo's built-in defaults, and asserts the strip
//! setting the artifacts `-Dstrip` reaches get is the reference default for
//! that mode. It also pins the packages the option does not reach.

use std::collections::BTreeMap;
use std::path::PathBuf;

type Table = BTreeMap<String, String>;

/// `[profile.<name>]` tables (profile-wide keys only) and
/// `[profile.<name>.package.<pkg>]` tables, from the root manifest.
fn profiles() -> (BTreeMap<String, Table>, BTreeMap<(String, String), Table>) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let text = std::fs::read_to_string(root.join("Cargo.toml")).expect("read Cargo.toml");
    let mut profiles: BTreeMap<String, Table> = BTreeMap::new();
    let mut packages: BTreeMap<(String, String), Table> = BTreeMap::new();
    enum Cur {
        None,
        Profile(String),
        Package(String, String),
    }
    let mut cur = Cur::None;
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some(h) = l.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
            let parts: Vec<&str> = h.split('.').collect();
            cur = match parts.as_slice() {
                ["profile", name] => {
                    profiles.entry(name.to_string()).or_default();
                    Cur::Profile(name.to_string())
                }
                ["profile", name, "package", pkg] => {
                    let key = (name.to_string(), pkg.trim_matches('"').to_string());
                    packages.entry(key.clone()).or_default();
                    Cur::Package(key.0, key.1)
                }
                _ => Cur::None,
            };
            continue;
        }
        let Some((k, v)) = l.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim().to_string(), v.trim().trim_matches('"').to_string());
        match &cur {
            Cur::Profile(p) => {
                profiles.get_mut(p).unwrap().insert(k, v);
            }
            Cur::Package(p, pkg) => {
                packages
                    .get_mut(&(p.clone(), pkg.clone()))
                    .unwrap()
                    .insert(k, v);
            }
            Cur::None => {}
        }
    }
    (profiles, packages)
}

/// Cargo's built-in value of `key` for a root profile (`dev` / `release`, and
/// `test` / `bench`, which inherit them).
fn builtin(root: &str, key: &str) -> Option<&'static str> {
    match (root, key) {
        ("dev", "opt-level") => Some("0"),
        ("dev", "debug-assertions") => Some("true"),
        ("dev", "strip") => Some("none"),
        ("release", "opt-level") => Some("3"),
        ("release", "debug-assertions") => Some("false"),
        // With no `debug` set, Cargo's release default strips debuginfo.
        ("release", "strip") => Some("debuginfo"),
        _ => None,
    }
}

/// A key's effective value for `profile`, following `inherits`.
fn resolve(profiles: &BTreeMap<String, Table>, profile: &str, key: &str) -> String {
    let mut p = profile.to_string();
    loop {
        if let Some(v) = profiles.get(&p).and_then(|t| t.get(key)) {
            return v.clone();
        }
        let parent = match p.as_str() {
            "dev" | "release" => None,
            "test" => Some("dev".to_string()),
            "bench" => Some("release".to_string()),
            _ => profiles.get(&p).and_then(|t| t.get("inherits")).cloned(),
        };
        match parent {
            Some(next) => p = next,
            None => return builtin(&p, key).expect("a built-in default").to_string(),
        }
    }
}

/// The Zig optimize mode a profile stands for (`mode_string_for`).
fn mode(profiles: &BTreeMap<String, Table>, profile: &str) -> &'static str {
    let opt = resolve(profiles, profile, "opt-level");
    if opt == "0" {
        return "Debug";
    }
    if opt == "s" || opt == "z" {
        return "ReleaseSmall";
    }
    if resolve(profiles, profile, "debug-assertions") == "true" {
        "ReleaseSafe"
    } else {
        "ReleaseFast"
    }
}

/// `true` / `"symbols"` strip the symbol table (what `.strip = true` does);
/// `"debuginfo"` / `"none"` / `false` leave it.
fn strips_symbols(v: &str) -> bool {
    v == "true" || v == "symbols"
}

#[test]
fn every_profile_strips_exactly_when_the_reference_mode_does() {
    let (profiles, _) = profiles();
    // The profiles the executable and the embedding library are built with.
    let mut seen = Vec::new();
    for name in [
        "dev",
        "release",
        "release-fast",
        "release-safe",
        "xcframework",
    ] {
        let m = mode(&profiles, name);
        let want = matches!(m, "ReleaseFast" | "ReleaseSmall");
        let got = resolve(&profiles, name, "strip");
        assert_eq!(
            strips_symbols(&got),
            want,
            "profile `{name}` is {m}: -Dstrip defaults to {want}, but strip = {got:?}"
        );
        seen.push((name, m));
    }
    // Anti-vacuity: the mapping really covers the modes the reference switches
    // on, so both arms of the default were asserted.
    assert!(seen.contains(&("dev", "Debug")), "{seen:?}");
    assert!(seen.contains(&("release", "ReleaseFast")), "{seen:?}");
    assert!(seen.contains(&("release-safe", "ReleaseSafe")), "{seen:?}");
}

/// `build.zig`'s test executable sets `.strip = false`; `cargo bench` builds
/// its executables with the `bench` profile.
#[test]
fn bench_profile_does_not_strip() {
    let (profiles, _) = profiles();
    assert!(!strips_symbols(&resolve(&profiles, "bench", "strip")));
}

/// `cargo test --release` builds its test executables with the `release`
/// profile, which strips, so the lanes that run it turn the strip off (the
/// reference's test executable is `.strip = false`): G-REL exports the
/// override before its first cargo step, and ci.sh's `--release` lz4 run
/// passes it on the same command.
#[test]
fn release_test_lanes_run_unstripped() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let lane = std::fs::read_to_string(root.join(".port/tools/gates/check_release_lane.sh"))
        .expect("read check_release_lane.sh");
    let export = lane
        .find("\nexport CARGO_PROFILE_RELEASE_STRIP=false\n")
        .expect("G-REL exports CARGO_PROFILE_RELEASE_STRIP=false");
    // The main lane's steps, by their exact text: `run_exclusive` (defined
    // earlier in the file) also runs `cargo test --release`, so the first bare
    // match is inside that function (N562, `e0fb9a6e8`). Its call site must
    // come after the export too.
    let build = lane
        .find("run_watchdogged cargo build --release \"${SCOPE_ARGS[@]}\"")
        .expect("G-REL's build step");
    let test = lane
        .find("run_watchdogged cargo test --release \"${SCOPE_ARGS[@]}\"")
        .expect("G-REL's test step");
    let exclusive = lane
        .find("then run_exclusive;")
        .expect("G-REL's exclusive-unit call site");
    assert!(
        export < build && export < test && export < exclusive,
        "the export must precede every release test step"
    );

    let ci = std::fs::read_to_string(root.join(".port/tools/ci.sh")).expect("read ci.sh");
    let runs: Vec<&str> = ci
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with('#')
                && !t.starts_with("echo")
                && t.contains("cargo test")
                && t.contains("--release")
        })
        .collect();
    assert!(!runs.is_empty(), "ci.sh runs `cargo test ... --release`");
    for l in runs {
        assert!(
            l.contains("CARGO_PROFILE_RELEASE_STRIP=false cargo test"),
            "ci.sh release test run is stripped: {l}"
        );
    }
}

/// `-Dstrip` reaches `GhosttyExe.zig:20` and `GhosttyLib.zig:31,102` only.
/// Everything else linked in release keeps Cargo's default (`debuginfo`), and
/// the exe (`rustty`) and the embedding library (`rustty-apprt-c`) take the
/// profile's value (no override that would make `CARGO_PROFILE_RELEASE_STRIP`
/// — the `-Dstrip=` override — stop reaching them).
#[test]
fn only_the_exe_and_embedding_library_follow_the_option() {
    let (_, packages) = profiles();
    let excepted: Vec<&str> = packages
        .iter()
        .filter(|((p, _), t)| p == "release" && t.contains_key("strip"))
        .map(|((_, pkg), t)| {
            assert_eq!(t["strip"], "debuginfo", "{pkg}");
            pkg.as_str()
        })
        .collect();
    assert_eq!(
        excepted,
        [
            "rustty-bench",
            "rustty-synthetic",
            "rustty-vt-c",
            "rustty-wasm",
            "rustty-web-canvas",
            "xtask"
        ]
    );
    for pkg in ["rustty", "rustty-apprt-c"] {
        assert!(
            !packages.keys().any(|(_, p)| p == pkg),
            "`{pkg}` must take the profile's strip (the -Dstrip option)"
        );
    }
}
