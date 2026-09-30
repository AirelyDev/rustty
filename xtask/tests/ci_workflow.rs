//! `ci.yml` against the reference `test.yml` (parity tail CI-1,
//! NEW-FINDINGS N376, N420, N421, N422, N429).
//!
//! The jobs themselves only run on GitHub's runners (VERIFICATION_DEBT V25);
//! what is provable here is their shape: every ported job exists, is wired
//! into `required`, and carries the steps its finding names. Each check is a
//! function that returns what is wrong, and every test also feeds it a
//! mutated copy of the real job, so a check that cannot fail is caught
//! (the negative control runs on every `cargo test`).
//!
//! `cargo test -p xtask --test ci_workflow`. YAML validity is actionlint's
//! (G-LINT); like `release_workflows.rs`, this reads the file with a small
//! indent scanner, not a YAML crate.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a parent dir")
        .to_path_buf()
}

fn ci_yml() -> String {
    let path = workspace_root().join(".github/workflows/ci.yml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

struct Job {
    /// The job's lines after its `  <name>:` header, up to the next header.
    body: String,
    /// The job's `needs:`, in the inline (`needs: a`, `needs: [a, b]`) or the
    /// block-list form.
    needs: Vec<String>,
}

/// Split the `jobs:` mapping into jobs (a two-space-indented `<name>:` line is
/// a job header).
fn parse_jobs(text: &str) -> BTreeMap<String, Job> {
    let mut jobs: BTreeMap<String, Job> = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut in_jobs = false;
    let mut in_needs_block = false;
    for line in text.lines() {
        if line.starts_with("jobs:") {
            in_jobs = true;
            continue;
        }
        if !in_jobs {
            continue;
        }
        if !line.starts_with(' ') && !line.trim().is_empty() && !line.starts_with('#') {
            break;
        }
        let is_header = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('#');
        if is_header {
            let name = line.trim().trim_end_matches(':').to_string();
            jobs.insert(
                name.clone(),
                Job {
                    body: String::new(),
                    needs: Vec::new(),
                },
            );
            current = Some(name);
            in_needs_block = false;
            continue;
        }
        let Some(name) = &current else { continue };
        let job = jobs.get_mut(name).expect("inserted above");
        job.body.push_str(line);
        job.body.push('\n');
        if in_needs_block {
            if let Some(item) = line.strip_prefix("      - ") {
                job.needs.push(item.trim().to_string());
                continue;
            }
            if !line.trim_start().starts_with('#') {
                in_needs_block = false;
            }
        }
        if let Some(rest) = line.strip_prefix("    needs:") {
            let rest = rest.trim();
            if rest.is_empty() {
                in_needs_block = true;
            } else {
                job.needs = rest
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
    }
    jobs
}

/// What is wrong with `required`: it must need every other job (the port's
/// `required` fails a skipped job too, so a job missing here is never
/// checked), and nothing that is not a job.
fn required_mismatch(jobs: &BTreeMap<String, Job>) -> Vec<String> {
    let Some(required) = jobs.get("required") else {
        return vec!["no `required` job".into()];
    };
    let needs: BTreeSet<&str> = required.needs.iter().map(String::as_str).collect();
    let mut out = Vec::new();
    for name in jobs.keys().filter(|n| n.as_str() != "required") {
        if !needs.contains(name.as_str()) {
            out.push(format!("`required` does not need `{name}`"));
        }
    }
    for n in &needs {
        if !jobs.contains_key(*n) {
            out.push(format!("`required` needs `{n}`, which is not a job"));
        }
    }
    out
}

/// The reference `test.yml` jobs parity tail CI-1 ported, by the port's job
/// name (the reference's name, with `libghostty` kept as the reference spells
/// it). Each `needs: ci`, the port of the reference's `needs: test`.
const PORTED_TEST_YML_JOBS: &[&str] = &[
    "build-examples-cmake-cross",
    "build-snap",
    "build-libghostty-vt",
    "build-libghostty-vt-wasm",
    "build-libghostty-vt-features",
    "build-libghostty-vt-android",
    "build-linux",
    "build-nix",
    "build-macos-xcode-27",
    "test-gtk",
    "test-simd",
    "test-sentry-linux",
    "test-i18n",
    "test-fuzz-libghostty",
    "test-pkg-linux",
    "test-debian-13",
];

fn ported_jobs_missing(jobs: &BTreeMap<String, Job>) -> Vec<String> {
    let mut out = Vec::new();
    for name in PORTED_TEST_YML_JOBS {
        match jobs.get(*name) {
            None => out.push(format!("no `{name}` job")),
            Some(j) if j.needs != ["ci"] => {
                out.push(format!("`{name}` needs {:?}, want [\"ci\"]", j.needs))
            }
            Some(_) => {}
        }
    }
    out
}

/// Every string in `want` must appear in `body`; returns the ones missing.
fn missing<'a>(body: &str, want: &[&'a str]) -> Vec<&'a str> {
    want.iter().copied().filter(|w| !body.contains(w)).collect()
}

fn job_body(jobs: &BTreeMap<String, Job>, name: &str) -> String {
    jobs.get(name)
        .unwrap_or_else(|| panic!("ci.yml has no `{name}` job"))
        .body
        .clone()
}

#[test]
fn job_parser_reads_inline_and_block_needs() {
    let sample = "\
jobs:
  required:
    needs:
      - a
      # a comment inside the list
      - b
    if: always()
  a:
    needs: ci
  b:
    needs: [ci, a]
  ci:
    steps: []
";
    let jobs = parse_jobs(sample);
    assert_eq!(jobs["required"].needs, ["a", "b"]);
    assert_eq!(jobs["a"].needs, ["ci"]);
    assert_eq!(jobs["b"].needs, ["ci", "a"]);
    assert!(jobs["ci"].needs.is_empty());
    // `ci` is missing from required's needs.
    assert_eq!(required_mismatch(&jobs), ["`required` does not need `ci`"]);
}

/// The reference's `required` needs every job; so must the port's, or a job
/// added here would never gate a merge.
#[test]
fn required_needs_every_job() {
    let text = ci_yml();
    let jobs = parse_jobs(&text);
    assert!(jobs.len() > 20, "parsed only {} jobs", jobs.len());
    assert_eq!(required_mismatch(&jobs), Vec::<String>::new());

    // Negative control: drop one ported job from `required.needs`.
    let mutated = text.replacen("      - test-i18n\n", "", 1);
    assert_ne!(mutated, text, "the control must change the file");
    let problems = required_mismatch(&parse_jobs(&mutated));
    assert_eq!(problems, ["`required` does not need `test-i18n`"]);
}

/// N376: every reference job CI-1 ported exists and runs after `ci`.
#[test]
fn ported_test_yml_jobs_exist_and_need_ci() {
    let text = ci_yml();
    assert_eq!(
        ported_jobs_missing(&parse_jobs(&text)),
        Vec::<String>::new()
    );

    // Negative controls: a renamed job, and a job that stops needing `ci`.
    let renamed = text.replacen("  test-pkg-linux:\n", "  test-pkg:\n", 1);
    assert_eq!(
        ported_jobs_missing(&parse_jobs(&renamed)),
        ["no `test-pkg-linux` job"]
    );
    let body = job_body(&parse_jobs(&text), "build-snap");
    let unneeded = text.replacen(&body, &body.replacen("    needs: ci\n", "", 1), 1);
    assert_eq!(
        ported_jobs_missing(&parse_jobs(&unneeded)),
        ["`build-snap` needs [], want [\"ci\"]"]
    );
}

/// N376 gap 20: the reference's `on:` (every push, every PR, dispatch) and
/// its concurrency group, which never cancels a `main` run.
fn trigger_problems(text: &str) -> Vec<&'static str> {
    let head = text.split("\njobs:").next().unwrap_or_default();
    let mut out = missing(
        head,
        &[
            "on:\n  push: {}\n  pull_request: {}\n  workflow_dispatch: {}\n",
            "group: ${{ github.workflow }}-${{ github.ref_name != 'main' && github.ref || github.run_id }}",
            "cancel-in-progress: true",
        ],
    );
    if head.contains("branches: [main]") {
        out.push("push is limited to main");
    }
    out
}

#[test]
fn triggers_and_concurrency_match_the_reference() {
    let text = ci_yml();
    assert_eq!(trigger_problems(&text), Vec::<&str>::new());
    // Negative controls: the pre-CI-1 trigger and group.
    let old_on = text.replacen("  push: {}\n", "  push:\n    branches: [main]\n", 1);
    assert!(!trigger_problems(&old_on).is_empty());
    let old_group = text.replacen(
        "${{ github.workflow }}-${{ github.ref_name != 'main' && github.ref || github.run_id }}",
        "ci-${{ github.ref }}",
        1,
    );
    assert!(!trigger_problems(&old_group).is_empty());
}

/// N429: the Android job is the reference's matrix with its setup-ndk pin,
/// builds each target through G-XTARGET with the NDK clang, and treats exit 3
/// (a named target not built) as a failure.
fn android_problems(body: &str) -> Vec<String> {
    let mut out: Vec<String> = missing(
        body,
        &[
            "nttld/setup-ndk@ed92fe6cadad69be94a966a7ee3271275e62f779 # v1.6.0",
            "ndk-version: r29",
            "add-to-path: false",
            "link-to-sdk: false",
            "local-cache: true",
            "- target: aarch64-linux-android\n            clang: aarch64-linux-android",
            "- target: x86_64-linux-android\n            clang: x86_64-linux-android",
            "- target: armv7-linux-androideabi\n            clang: armv7a-linux-androideabi",
            "ANDROID_NDK_HOME: ${{ steps.setup-ndk.outputs.ndk-path }}",
            "toolchains/llvm/prebuilt/linux-x86_64/bin",
            "export \"CC_${tv}=$bin/${CLANG}29-clang\"",
            "export \"CXX_${tv}=$bin/${CLANG}29-clang++\"",
            "export \"AR_${tv}=$bin/llvm-ar\"",
            "export \"CARGO_TARGET_${tu}_LINKER=$bin/${CLANG}29-clang\"",
            "bash .port/tools/gates/check_cross_targets.sh \"$TARGET\" || rc=$?",
            "exit \"$rc\"",
        ],
    )
    .into_iter()
    .map(|m| format!("missing `{m}`"))
    .collect();
    // Anything that would let BLOCKED (exit 3) pass.
    for bad in ["continue-on-error", "|| true", "exit 0", "rc=0; exit"] {
        if body.contains(bad) {
            out.push(format!("`{bad}` would let a BLOCKED run pass"));
        }
    }
    if body.contains("3) exit 0") || body.contains("3) ;;") {
        out.push("exit 3 is accepted".into());
    }
    out
}

#[test]
fn android_job_follows_n429() {
    let body = job_body(&parse_jobs(&ci_yml()), "build-libghostty-vt-android");
    assert_eq!(android_problems(&body), Vec::<String>::new());
    // Negative controls, one at a time.
    for (what, mutated) in [
        (
            "BLOCKED accepted",
            body.replacen(
                "|| rc=$?",
                "|| rc=$?\n          [ \"$rc\" -eq 3 ] && exit 0",
                1,
            ),
        ),
        (
            "continue-on-error",
            body.replacen(
                "      - name: Build\n",
                "      - name: Build\n        continue-on-error: true\n",
                1,
            ),
        ),
        (
            "wrong NDK",
            body.replacen("ndk-version: r29", "ndk-version: r27", 1),
        ),
        (
            "armv7 clang triple",
            body.replacen(
                "clang: armv7a-linux-androideabi",
                "clang: armv7-linux-androideabi",
                1,
            ),
        ),
    ] {
        assert_ne!(mutated, body, "{what}: the control must change the job");
        assert!(
            !android_problems(&mutated).is_empty(),
            "{what}: not reported"
        );
    }
}

/// N420 (B-2): `test-simd` runs the app tier's tests with SIMD on and off,
/// off being `--no-default-features` with the GTK app's features named again.
fn simd_problems(body: &str) -> Vec<&'static str> {
    missing(
        body,
        &[
            "simd: [\"true\", \"false\"]",
            "flags=(--no-default-features)",
            "cargo build -p rustty-apprt-c --locked \"${flags[@]}\"",
            "flags+=(--features \"rustty/gtk,rustty/wayland,rustty/x11\")",
            "-p rustty-simd -p rustty-core",
            "-p rustty-apprt -p rustty-cli -p rustty-bench -p rustty-apprt-c",
            "-p rustty \"${flags[@]}\"",
        ],
    )
}

#[test]
fn test_simd_job_runs_the_simd_off_leg() {
    let body = job_body(&parse_jobs(&ci_yml()), "test-simd");
    assert_eq!(simd_problems(&body), Vec::<&str>::new());
    let on_only = body.replacen("simd: [\"true\", \"false\"]", "simd: [\"true\"]", 1);
    assert_eq!(simd_problems(&on_only), ["simd: [\"true\", \"false\"]"]);
    let no_core = body.replacen("-p rustty-simd -p rustty-core", "-p rustty-simd", 1);
    assert_eq!(simd_problems(&no_core), ["-p rustty-simd -p rustty-core"]);
}

/// N421 (B-2): `test-i18n` builds the app and tests rustty-build-config with
/// RUSTTY_I18N (the `-Di18n` option) true and false.
fn i18n_problems(body: &str) -> Vec<&'static str> {
    missing(
        body,
        &[
            "i18n: [\"true\", \"false\"]",
            "RUSTTY_I18N: ${{ matrix.i18n }}",
            "cargo build -p rustty --locked",
            "cargo test -p rustty-build-config --locked",
        ],
    )
}

#[test]
fn test_i18n_job_builds_both_values() {
    let body = job_body(&parse_jobs(&ci_yml()), "test-i18n");
    assert_eq!(i18n_problems(&body), Vec::<&str>::new());
    let unset = body.replacen(
        "RUSTTY_I18N: ${{ matrix.i18n }}",
        "I18N: ${{ matrix.i18n }}",
        1,
    );
    assert_eq!(i18n_problems(&unset), ["RUSTTY_I18N: ${{ matrix.i18n }}"]);
}

/// N422 (B-2) and N376 gap 9: `build-nix` checks the release package is
/// stripped and the debug one is not, by `+version` mode and `nm`, on both
/// architectures, and builds the four librustty-vt variants.
fn nix_problems(body: &str) -> Vec<&'static str> {
    missing(
        body,
        &[
            "os: [ubuntu-latest, ubuntu-24.04-arm]",
            "nix build .#rustty-releasefast",
            "result/bin/rustty +version | grep -q '.ReleaseFast'",
            "nm result/bin/.rustty-wrapped 2>&1 | grep -q 'no symbols'",
            "nix build .#rustty-debug",
            "result/bin/rustty +version | grep -q '.Debug'",
            "nm result/bin/.rustty-wrapped 2>&1 | grep -q 'main_rustty'",
            "nix build .#librustty-vt-releasefast.tests.build-with-static",
            "nix build .#librustty-vt-releasefast-no-simd.tests.build-with-static",
            "nix build .#librustty-vt-debug.tests.build-with-static",
            "nix build .#librustty-vt-debug-no-simd.tests.build-with-static",
        ],
    )
}

#[test]
fn build_nix_job_checks_strip_and_mode() {
    let body = job_body(&parse_jobs(&ci_yml()), "build-nix");
    assert_eq!(nix_problems(&body), Vec::<&str>::new());
    let no_strip = body.replacen("grep -q 'no symbols'", "true", 1);
    assert_eq!(
        nix_problems(&no_strip),
        ["nm result/bin/.rustty-wrapped 2>&1 | grep -q 'no symbols'"]
    );
    // The package names the checks use must be flake outputs.
    let flake = std::fs::read_to_string(workspace_root().join("flake.nix")).unwrap();
    for pkg in [
        "rustty-releasefast =",
        "rustty-debug =",
        "librustty-vt-releasefast =",
        "librustty-vt-releasefast-no-simd =",
        "librustty-vt-debug =",
        "librustty-vt-debug-no-simd =",
    ] {
        assert!(flake.contains(pkg), "flake.nix has no `{pkg}`");
    }
    let vt = std::fs::read_to_string(workspace_root().join("nix/librustty-vt.nix")).unwrap();
    for test in [
        "sanity-check =",
        "pkg-config =",
        "build-with-shared =",
        "build-with-static =",
    ] {
        assert!(
            vt.contains(test),
            "nix/librustty-vt.nix has no passthru test `{test}`"
        );
    }
}

/// N376 gap 7: the reference's compile-only feature matrix, its nine tokens
/// in Cargo spelling, each built for wasm32 and compiled as tests.
fn features_problems(body: &str) -> Vec<&'static str> {
    missing(
        body,
        &[
            "features=\"snapshot formatter selection render-state input-encode color grid-introspection glyph-protocol kitty-graphics\"",
            "configs=\"simd\"",
            "configs=\"$configs simd,$f\"",
            "--target wasm32-unknown-unknown",
            "--features \"$cfg\" --no-run",
        ],
    )
}

#[test]
fn features_job_covers_the_reference_tokens() {
    let body = job_body(&parse_jobs(&ci_yml()), "build-libghostty-vt-features");
    assert_eq!(features_problems(&body), Vec::<&str>::new());
    // Every token is a real rustty-vt-c feature.
    let manifest =
        std::fs::read_to_string(workspace_root().join("crates/rustty-vt-c/Cargo.toml")).unwrap();
    for f in [
        "snapshot",
        "formatter",
        "selection",
        "render-state",
        "input-encode",
        "color",
        "grid-introspection",
        "glyph-protocol",
        "kitty-graphics",
        "simd",
    ] {
        assert!(
            manifest.contains(&format!("\n{f} = [")),
            "rustty-vt-c has no feature `{f}`"
        );
    }
    let no_tests = body.replacen("--features \"$cfg\" --no-run", "--features \"$cfg\"", 1);
    assert_eq!(
        features_problems(&no_tests),
        ["--features \"$cfg\" --no-run"]
    );
}

/// N376 gap 6: the wasm job builds both tiers, requires simd128 of both and
/// runs the allocator smoke test on both.
fn wasm_problems(body: &str) -> Vec<&'static str> {
    missing(
        body,
        &[
            "bash .port/tools/build-vt-wasm.sh\n",
            "bash .port/tools/build-vt-wasm.sh --config 'profile.release.opt-level=\"z\"'",
            "-o rustty-vt.wasm",
            "-o rustty-vt-small.wasm",
            "for artifact in rustty-vt.wasm rustty-vt-small.wasm; do",
            "--features=-simd \"${artifact}\"; then",
            "node test/wasm-alloc.mjs rustty-vt.wasm",
            "node test/wasm-alloc.mjs rustty-vt-small.wasm",
        ],
    )
}

#[test]
fn wasm_job_builds_and_checks_both_tiers() {
    let body = job_body(&parse_jobs(&ci_yml()), "build-libghostty-vt-wasm");
    assert_eq!(wasm_problems(&body), Vec::<&str>::new());
    let one = body.replacen("node test/wasm-alloc.mjs rustty-vt-small.wasm", "", 1);
    assert_eq!(
        wasm_problems(&one),
        ["node test/wasm-alloc.mjs rustty-vt-small.wasm"]
    );
    assert!(workspace_root().join("test/wasm-alloc.mjs").is_file());
    assert!(workspace_root()
        .join(".port/tools/build-vt-wasm.sh")
        .is_file());
}

/// N376 gaps 4, 11, 13, 15, 16, 18, 19, 21 and 3: each job runs the command
/// that ports the reference step.
#[test]
fn remaining_jobs_run_their_reference_commands() {
    let jobs = parse_jobs(&ci_yml());
    let table: &[(&str, &[&str])] = &[
        (
            "build-snap",
            &["cargo build -p rustty --features snap --locked"],
        ),
        (
            "build-libghostty-vt",
            &[
                "- target: aarch64-apple-darwin",
                "- target: x86_64-apple-darwin",
                "- target: aarch64-unknown-linux-gnu\n            os: ubuntu-24.04-arm",
                "- target: x86_64-unknown-linux-gnu",
                "- target: x86_64-unknown-linux-musl",
                "- target: x86_64-pc-windows-gnu",
                "cargo build -p rustty-vt-c --release --locked --target \"$TARGET\"",
            ],
        ),
        (
            "build-linux",
            &[
                "os: [ubuntu-latest, ubuntu-24.04-arm]",
                "cargo build -p rustty --locked",
            ],
        ),
        (
            "test-gtk",
            &[
                "x11: [\"true\", \"false\"]",
                "wayland: [\"true\", \"false\"]",
                "RUSTTY_GTK_X11: ${{ matrix.x11 }}",
                "RUSTTY_GTK_WAYLAND: ${{ matrix.wayland }}",
                "--no-default-features --features \"$RUSTTY_GTK_FEATURES\"",
            ],
        ),
        (
            "test-sentry-linux",
            &[
                "sentry: [\"true\", \"false\"]",
                "cargo build -p rustty --locked --features sentry",
            ],
        ),
        ("test-fuzz-libghostty", &["cargo +nightly fuzz build"]),
        (
            "test-pkg-linux",
            &["pkg: [\"wuffs\"]", "cargo test -p \"rustty-$PKG\" --locked"],
        ),
        (
            "test-debian-13",
            &[
                "--build-arg DISTRO_VERSION=13",
                "-f xtask/docker/debian/Dockerfile",
            ],
        ),
        (
            "build-macos-xcode-27",
            &[
                "sudo xcode-select -s /Applications/Xcode_27.app",
                "cargo run --release -p xtask -- xcframework",
            ],
        ),
        (
            "build-examples-cmake-cross",
            &[
                "rustup target add x86_64-pc-windows-gnu",
                "cd example/c-vt-cmake-cross",
                "-DFETCHCONTENT_SOURCE_DIR_RUSTTY=",
            ],
        ),
    ];
    for (job, want) in table {
        let body = job_body(&jobs, job);
        assert_eq!(missing(&body, want), Vec::<&str>::new(), "job `{job}`");
    }
    // Negative control: the table's check reports a dropped matrix row.
    let body = job_body(&jobs, "test-gtk");
    let cut = body.replacen("wayland: [\"true\", \"false\"]", "wayland: [\"true\"]", 1);
    assert_eq!(
        missing(&cut, &["wayland: [\"true\", \"false\"]"]),
        ["wayland: [\"true\", \"false\"]"]
    );
    // The paths the jobs name exist.
    for p in [
        "xtask/docker/debian/Dockerfile",
        "example/c-vt-cmake-cross/CMakeLists.txt",
        "crates/rustty-vt/fuzz/Cargo.toml",
        "crates/rustty-wuffs/Cargo.toml",
    ] {
        assert!(workspace_root().join(p).is_file(), "missing {p}");
    }
}

/// What is wrong with the `windows` job w.r.t. the two legs upstream dropped
/// (R4 §G / DECISIONS R6 / P7-W10 T4): non-empty means a leg was re-added, or
/// the replacement vt-lib test coverage is missing.
fn windows_leg_problems(body: &str) -> Vec<&'static str> {
    let mut out = Vec::new();
    if body.contains("- name: Test the workspace") {
        out.push("windows job has the removed `test-windows` leg (\"Test the workspace\" step)");
    }
    if body.contains("run: cargo test --workspace") {
        out.push("windows job runs a full-workspace test (the removed `test-windows` leg)");
    }
    if body.contains("- name: Build the apprt-c library for the GNU ABI") {
        out.push("windows job has the removed `build-libghostty-windows-gnu` leg");
    }
    if body.contains("x86_64-pc-windows-gnu") {
        out.push(
            "windows job targets the GNU ABI (the removed `build-libghostty-windows-gnu` leg)",
        );
    }
    // The replacement: vt-lib testing on Windows must not silently disappear
    // along with the removed full-workspace test.
    if !body.contains("cargo test -p rustty-vt-c --locked") {
        out.push("windows job lost its rustty-vt-c test coverage");
    }
    if !body.contains("cargo build -p rustty-vt-c --release --locked") {
        out.push("windows job lost its rustty-vt-c release build (build-libghostty-vt-windows)");
    }
    out
}

/// R4 §G / DECISIONS R6 / P7-W10 T4: upstream `380778e3c` dropped
/// `test-windows` and `build-libghostty-windows-gnu` from `test.yml`
/// (20abdb50a..12752b2ac — "libghostty-internal isn't really supposed to be
/// used in Windows anyway"). Rustty's `windows` job had ported both as the
/// "Test the workspace" step and the "Build the apprt-c library for the GNU
/// ABI" step; both are now removed, for parity. `build-libghostty-vt-windows`
/// is UNCHANGED upstream, so its `test-lib-vt` coverage (which used to ride
/// along inside the now-removed workspace test) gets its own scoped step.
#[test]
fn windows_job_drops_the_legs_upstream_dropped() {
    let jobs = parse_jobs(&ci_yml());
    let body = job_body(&jobs, "windows");
    assert_eq!(windows_leg_problems(&body), Vec::<&str>::new());

    // Negative controls: re-adding either leg must be caught by the same check.
    let readded_test = body.replacen(
        "- name: Build librustty-vt (release)",
        "- name: Test the workspace\n        shell: pwsh\n        run: cargo test --workspace --locked\n\n      - name: Build librustty-vt (release)",
        1,
    );
    assert_ne!(readded_test, body, "the control must change the text");
    assert_eq!(
        windows_leg_problems(&readded_test),
        [
            "windows job has the removed `test-windows` leg (\"Test the workspace\" step)",
            "windows job runs a full-workspace test (the removed `test-windows` leg)",
        ]
    );

    let readded_gnu = format!(
        "{body}\n      - name: Build the apprt-c library for the GNU ABI\n        run: |\n          rustup target add x86_64-pc-windows-gnu\n          cargo build -p rustty-apprt-c --target x86_64-pc-windows-gnu --locked\n"
    );
    assert_ne!(readded_gnu, body, "the control must change the text");
    assert_eq!(
        windows_leg_problems(&readded_gnu),
        [
            "windows job has the removed `build-libghostty-windows-gnu` leg",
            "windows job targets the GNU ABI (the removed `build-libghostty-windows-gnu` leg)",
        ]
    );

    // Negative control: dropping the replacement vt-lib test coverage is also caught.
    let dropped_vt_test = body.replacen(
        "- name: Test librustty-vt\n        shell: pwsh\n        run: cargo test -p rustty-vt-c --locked\n\n      ",
        "",
        1,
    );
    assert_ne!(dropped_vt_test, body, "the control must change the text");
    assert_eq!(
        windows_leg_problems(&dropped_vt_test),
        ["windows job lost its rustty-vt-c test coverage"]
    );
}
