//! W2-05 proving tests for the release automation.
//!
//! The pipelines themselves (`.github/workflows/release-{tag,tip}.yml`,
//! `publish-tag.yml`) can only truly run on GitHub's infrastructure with
//! maintainer secrets, so what is provable HERE is the property the batch
//! actually promises: the automation exists, it is wired to real xtask
//! subcommands and real repo scripts, and it is INERT — nothing publishes
//! until a maintainer arms it with the `RUSTTY_RELEASE_ENABLED` repository
//! variable.
//!
//! These are ordinary host tests (`cargo test -p xtask --test
//! release_workflows`); they read the workflow files and exercise the shipped
//! `xtask` binary.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The arming switch every release job is gated on.
const GATE: &str = "vars.RUSTTY_RELEASE_ENABLED == 'true'";

/// Steps/actions that push bytes or refs somewhere the public can see them.
const PUBLISHING_MARKERS: &[&str] = &[
    "softprops/action-gh-release",
    "r2-upload-action",
    "git push --force origin tip",
];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a parent dir")
        .to_path_buf()
}

fn workflows_dir() -> PathBuf {
    workspace_root().join(".github/workflows")
}

/// The release pipelines, in the order a release flows through them.
const RELEASE_WORKFLOWS: &[&str] = &["release-tag.yml", "release-tip.yml", "publish-tag.yml"];

struct Job {
    /// Everything from the job's `  <name>:` header up to the next job header.
    body: String,
    /// Jobs named by this job's `needs:` key.
    needs: Vec<String>,
}

/// Split a workflow's `jobs:` mapping into its jobs.
///
/// Deliberately a small line/indent scanner rather than a YAML dependency:
/// the workspace has no YAML crate, and the only structure these assertions
/// need is "which lines belong to which job, and what does it `need`". The
/// workflows are validated as YAML separately by `actionlint`.
fn parse_jobs(text: &str) -> BTreeMap<String, Job> {
    let mut jobs: BTreeMap<String, Job> = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut in_jobs = false;
    for line in text.lines() {
        if line.starts_with("jobs:") {
            in_jobs = true;
            continue;
        }
        if !in_jobs {
            continue;
        }
        // A top-level key ends the `jobs:` mapping.
        if !line.starts_with(' ') && !line.trim().is_empty() && !line.starts_with('#') {
            break;
        }
        // `  <name>:` at exactly two spaces of indent is a job header.
        let is_job_header = line.len() > 2
            && line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('#');
        if is_job_header {
            let name = line.trim().trim_end_matches(':').to_string();
            jobs.insert(
                name.clone(),
                Job {
                    body: String::new(),
                    needs: Vec::new(),
                },
            );
            current = Some(name);
            continue;
        }
        if let Some(name) = &current {
            let job = jobs.get_mut(name).expect("job inserted above");
            job.body.push_str(line);
            job.body.push('\n');
            // `    needs: [a, b]` — the inline form these workflows use.
            if let Some(rest) = line.strip_prefix("    needs:") {
                job.needs = rest
                    .trim()
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

/// The set of jobs that carry `marker` in their own body, plus every job all
/// of whose `needs` are (transitively) in that set — a job GitHub can only
/// reach through guarded jobs is itself guarded.
///
/// Computed as a fixpoint rather than by recursive descent so a diamond
/// (`appcast` needs both `setup` and `build-macos`, and `build-macos` needs
/// `setup`) is not mistaken for a cycle.
fn guarded_jobs(jobs: &BTreeMap<String, Job>, marker: &str) -> BTreeSet<String> {
    let mut guarded: BTreeSet<String> = jobs
        .iter()
        .filter(|(_, j)| j.body.contains(marker))
        .map(|(n, _)| n.clone())
        .collect();
    loop {
        let mut changed = false;
        for (name, job) in jobs {
            if guarded.contains(name) || job.needs.is_empty() {
                continue;
            }
            if job.needs.iter().all(|n| guarded.contains(n)) {
                guarded.insert(name.clone());
                changed = true;
            }
        }
        if !changed {
            return guarded;
        }
    }
}

fn read_workflow(file: &str) -> String {
    let path = workflows_dir().join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The release automation exists at all — the W2-05 gap itself.
#[test]
fn release_workflows_exist() {
    for file in RELEASE_WORKFLOWS {
        let path = workflows_dir().join(file);
        assert!(
            path.is_file(),
            "missing release workflow {}",
            path.display()
        );
    }
    let tip = workspace_root().join(".github/scripts/rustty-tip");
    assert!(tip.is_file(), "missing {}", tip.display());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&tip).unwrap().permissions().mode();
        assert!(mode & 0o111 != 0, "{} is not executable", tip.display());
    }
}

/// INERT BY DEFAULT: every job of every release workflow is reachable only
/// behind the `RUSTTY_RELEASE_ENABLED` gate, so a fresh clone (or a fork) that
/// pushes a `v*` tag, or lands a commit on `main`, publishes nothing.
#[test]
fn every_release_job_is_gated_on_the_arming_variable() {
    for file in RELEASE_WORKFLOWS {
        let text = read_workflow(file);
        let jobs = parse_jobs(&text);
        assert!(!jobs.is_empty(), "{file}: parsed no jobs");
        let gated = guarded_jobs(&jobs, GATE);
        for name in jobs.keys() {
            assert!(
                gated.contains(name),
                "{file}: job `{name}` is not gated on {GATE} (directly or through `needs`)"
            );
        }
    }
}

/// …and the gate is not vacuous: these workflows really do contain publishing
/// steps, so the assertion above is guarding something.
#[test]
fn publishing_steps_exist_and_live_only_in_gated_jobs() {
    let mut found: BTreeSet<&str> = BTreeSet::new();
    for file in RELEASE_WORKFLOWS {
        let text = read_workflow(file);
        let jobs = parse_jobs(&text);
        let gated = guarded_jobs(&jobs, GATE);
        for (name, job) in &jobs {
            for marker in PUBLISHING_MARKERS {
                if job.body.contains(marker) {
                    found.insert(marker);
                    assert!(
                        gated.contains(name),
                        "{file}: publishing step `{marker}` in ungated job `{name}`"
                    );
                }
            }
        }
    }
    for marker in PUBLISHING_MARKERS {
        assert!(
            found.contains(marker),
            "no release workflow publishes via `{marker}` — the gating test would be vacuous"
        );
    }
}

/// A publishing job must never be reachable from a fork: the canonical
/// repository check is the second lock.
#[test]
fn publishing_jobs_also_pin_the_canonical_repository() {
    const REPO_GUARD: &str = "github.repository == 'AirelyDev/rustty'";
    for file in RELEASE_WORKFLOWS {
        let text = read_workflow(file);
        let jobs = parse_jobs(&text);
        let pinned = guarded_jobs(&jobs, REPO_GUARD);
        for (name, job) in &jobs {
            let publishes = PUBLISHING_MARKERS.iter().any(|m| job.body.contains(m));
            if publishes {
                assert!(
                    pinned.contains(name),
                    "{file}: publishing job `{name}` does not pin {REPO_GUARD} \
                     (directly or through `needs`)"
                );
            }
        }
    }
}

/// Every `cargo run -p xtask -- <sub>` the workflows drive is a subcommand the
/// shipped binary actually has. This is what keeps the pipelines from rotting
/// silently when xtask's CLI moves.
#[test]
fn workflows_only_call_real_xtask_subcommands() {
    let help = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("help")
        .output()
        .expect("run xtask help");
    assert!(help.status.success(), "xtask help failed");
    let help = String::from_utf8_lossy(&help.stdout).to_string();

    let mut checked = 0usize;
    for file in RELEASE_WORKFLOWS {
        let text = read_workflow(file);
        for (i, _) in text.match_indices("-p xtask -- ") {
            let rest = &text[i + "-p xtask -- ".len()..];
            let sub = rest
                .split_whitespace()
                .next()
                .expect("a subcommand follows the separator");
            assert!(
                help.contains(sub),
                "{file}: drives `xtask {sub}`, which `xtask help` does not list"
            );
            checked += 1;
        }
    }
    assert!(checked >= 2, "expected the workflows to drive xtask");
}

/// `release-info` behaves as the workflows assume: it writes the four
/// `key=value` outputs to `$GITHUB_OUTPUT`, and it refuses a tag that
/// disagrees with `Cargo.toml` (the check that has no oracle counterpart —
/// Ghostty takes the version from the tag, rustty from the manifest).
#[test]
fn release_info_writes_github_output_and_rejects_a_mismatched_tag() {
    let dir = std::env::temp_dir().join(format!("rustty-release-info-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("github_output");
    std::fs::write(&out, "").unwrap();

    let ok = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("release-info")
        .env("GITHUB_OUTPUT", &out)
        .output()
        .expect("run xtask release-info");
    assert!(
        ok.status.success(),
        "release-info failed: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let written = std::fs::read_to_string(&out).unwrap();
    for key in ["version=", "build=", "commit=", "commit_long="] {
        assert!(
            written.contains(key),
            "$GITHUB_OUTPUT missing {key}:\n{written}"
        );
    }
    assert_eq!(
        written,
        String::from_utf8_lossy(&ok.stdout),
        "stdout and $GITHUB_OUTPUT must agree"
    );

    // A tag that does not match the manifest is refused before anything is built.
    let bad = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["release-info", "--version", "v99.99.99"])
        .output()
        .expect("run xtask release-info");
    assert!(!bad.status.success(), "mismatched tag was accepted");
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(stderr.contains("does not match"), "{stderr}");

    // …but the escape hatch the publish workflow uses still works.
    let forced = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args([
            "release-info",
            "--version",
            "v99.99.99",
            "--no-workspace-check",
        ])
        .output()
        .expect("run xtask release-info");
    assert!(
        forced.status.success(),
        "{}",
        String::from_utf8_lossy(&forced.stderr)
    );
    assert!(String::from_utf8_lossy(&forced.stdout).contains("version=99.99.99"));

    // A malformed version is rejected (the oracle's "Validate Version Input").
    let malformed = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["release-info", "--version", "1.0.0"])
        .output()
        .expect("run xtask release-info");
    assert!(
        !malformed.status.success(),
        "`1.0.0` (no leading v) accepted"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The tip probe script behaves like the oracle's nushell one: exit 1 on a
/// miss, exit 2 on misuse. (Exit 0 needs a live artifact host, so it is not
/// asserted here.)
#[test]
#[cfg_attr(not(unix), ignore = "shell script")]
fn tip_probe_reports_a_miss_without_failing_the_shell() {
    let script = workspace_root().join(".github/scripts/rustty-tip");
    let miss = Command::new(&script)
        .arg("0000000000000000000000000000000000000000")
        // A host that cannot resolve stands in for "no such tip release".
        .env("RUSTTY_TIP_URL_BASE", "https://tip.invalid.rustty.test")
        .output()
        .expect("run rustty-tip");
    assert_eq!(miss.status.code(), Some(1), "{miss:?}");
    assert!(
        String::from_utf8_lossy(&miss.stderr).contains("does not have a corresponding tip release")
    );

    let misuse = Command::new(&script).output().expect("run rustty-tip");
    assert_eq!(misuse.status.code(), Some(2), "{misuse:?}");
}

/// PACKAGING.md documents the artifact URLs; the workflows must actually
/// produce those names at those hosts, or the doc is a promise nothing keeps.
#[test]
fn workflow_artifact_names_match_the_documented_urls() {
    let packaging = std::fs::read_to_string(workspace_root().join("PACKAGING.md")).unwrap();
    assert!(
        packaging.contains("https://rustty.sh/VERSION/rustty-VERSION.tar.gz"),
        "PACKAGING.md no longer documents the tagged tarball URL"
    );
    assert!(
        packaging.contains("rustty-source.tar.gz"),
        "PACKAGING.md no longer documents the tip tarball asset"
    );

    let tag = read_workflow("release-tag.yml");
    // The tagged pipeline stages `<version>/rustty-<version>.tar.gz` (+ .minisig).
    assert!(tag.contains("blob/${RUSTTY_VERSION}/rustty-${RUSTTY_VERSION}.tar.gz"));
    assert!(tag.contains("blob/${RUSTTY_VERSION}/rustty-${RUSTTY_VERSION}.tar.gz.minisig"));
    assert!(tag.contains("blob/${RUSTTY_VERSION}/rustty-source.tar.gz"));

    // The tip pipeline publishes `rustty-source.tar.gz` on the `tip` release.
    let tip = read_workflow("release-tip.yml");
    assert!(tip.contains("tag_name: tip"));
    assert!(tip.contains("rustty-source.tar.gz"));
}

/// Sanity check on the job parser itself, so a parser bug cannot turn the
/// gating assertions into silent passes.
#[test]
fn job_parser_finds_jobs_needs_and_ungated_jobs() {
    let sample = "\
name: X
on: push
jobs:
  a:
    if: vars.RUSTTY_RELEASE_ENABLED == 'true'
    steps:
      - run: echo a
  b:
    needs: [a]
    steps:
      - run: echo b
  d:
    needs: [a, b]
    steps:
      - run: echo d
  c:
    steps:
      - run: echo c
";
    let jobs = parse_jobs(sample);
    assert_eq!(
        jobs.keys().map(String::as_str).collect::<Vec<_>>(),
        ["a", "b", "c", "d"]
    );
    assert_eq!(jobs["b"].needs, ["a"]);
    assert_eq!(jobs["d"].needs, ["a", "b"]);
    let gated = guarded_jobs(&jobs, GATE);
    assert!(gated.contains("a"));
    assert!(gated.contains("b"));
    // The diamond (`d` needs both `a` and `b`, and `b` needs `a`) must resolve
    // as gated — a naive recursion with a shared visited-set reports it ungated.
    assert!(
        gated.contains("d"),
        "diamond dependency reported as ungated"
    );
    assert!(
        !gated.contains("c"),
        "an ungated job must be reported as ungated"
    );
}

/// Every workflow file in the repo is reachable by the same reader the tests
/// above use (guards against a release workflow being added elsewhere and
/// escaping these checks).
#[test]
fn no_other_workflow_publishes_release_artifacts() {
    let dir = workflows_dir();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path: PathBuf = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if RELEASE_WORKFLOWS.contains(&name.as_str()) {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("yml") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for marker in PUBLISHING_MARKERS {
            assert!(
                !text.contains(marker),
                "{name} publishes via `{marker}` but is not one of the gated release workflows"
            );
        }
    }
    assert!(Path::new(&dir).join("ci.yml").is_file());
}
