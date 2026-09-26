//! Deterministic fuzz-corpus replay — runs the reference fuzz corpora through
//! the ported harness entry points (`rustty_vt::fuzz_harness`, the port of
//! `test/fuzz-libghostty/src/fuzz_{osc,parser,stream}.zig`) in plain
//! `cargo test`, AFL/libFuzzer-free. Any panic is a real crash finding.
//!
//! The WHOLE reference corpus is mirrored in-tree at
//! `.port/phase-0/corpora/cvt/{osc,parser,stream}-{initial,cmin}/` — all 4,001
//! files, the hand-written seeds and the large afl-cmin edge-deduped sets
//! alike — and every one of them is replayed by `replay_port_full_corpus` with
//! no oracle checkout in the loop.
//!
//! It was not always so. The `*-cmin` tiers used to live only in the oracle,
//! reachable exclusively via `GHOSTTY_ORACLE`, so the deep corpus replayed on
//! one machine and never on CI. Mirroring them is what makes this test mean the
//! same thing everywhere; the reference commits all six tiers for the same
//! reason.
//!
//! Setting `GHOSTTY_ORACLE` additionally enables two provenance tests —
//! `port_corpus_matches_oracle_bytes` (every mirrored file is byte-identical)
//! and `replay_full_oracle_corpus` (replay from the oracle, so an oracle bump
//! that adds files is exercised before the mirror is refreshed):
//!
//! ```sh
//! GHOSTTY_ORACLE=/Users/gael/projects/ghostty cargo test -p rustty-vt \
//!   --test fuzz_corpus_replay
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use rustty_vt::fuzz_harness;

/// The copied `*-initial` seeds inside the port tree.
fn port_corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.port/phase-0/corpora/cvt")
}

/// RUSTTY-ONLY strict mode, the `RUSTTY_REQUIRE_GL` / `RUSTTY_REQUIRE_METAL`
/// pattern (`rustty-renderer/src/opengl/gl/test_support.rs`) applied to the
/// mirrored corpus (parity S63-F8).
///
/// The corpus lives under `.port/`, which is DELIBERATELY excluded from the
/// source tarball — so every test here used to fail outright when the extracted
/// tarball ran its own test suite, which nothing noticed while `xtask
/// distcheck` only BUILT the extracted tree. A clean skip when the corpus is
/// absent is the right answer for a tarball and the WRONG answer for the repo,
/// where a vanished corpus would quietly retire 4,001 replays. So: skip when it
/// is absent, and `RUSTTY_REQUIRE_FUZZ_CORPUS=1` — exported by `ci.sh` — turns
/// that skip into a hard failure.
fn corpus_required() -> bool {
    corpus_required_from(std::env::var_os("RUSTTY_REQUIRE_FUZZ_CORPUS").as_deref())
}

/// The env-independent core of [`corpus_required`], split out so the unit tests
/// can drive both arms WITHOUT mutating process-global environment.
fn corpus_required_from(value: Option<&std::ffi::OsStr>) -> bool {
    value == Some(std::ffi::OsStr::new("1"))
}

/// The strict-mode pivot: `true` when the caller should skip (the corpus is
/// absent and nobody demanded it), a PANIC under
/// `RUSTTY_REQUIRE_FUZZ_CORPUS=1`, `false` when the corpus is there.
#[track_caller]
fn skip_without_corpus(test_name: &str) -> bool {
    skip_without_corpus_impl(port_corpus_root().is_dir(), corpus_required(), test_name)
}

/// The env- and filesystem-independent core of [`skip_without_corpus`].
#[track_caller]
fn skip_without_corpus_impl(present: bool, required: bool, test_name: &str) -> bool {
    if present {
        return false;
    }
    assert!(
        !required,
        "the mirrored fuzz corpus is required but absent \
         (RUSTTY_REQUIRE_FUZZ_CORPUS=1): {test_name} would have skipped"
    );
    eprintln!(
        "no mirrored fuzz corpus at {}; skipping {test_name}",
        port_corpus_root().display()
    );
    true
}

/// PROVING tests for the strict mode: the pivot must be a no-op when the corpus
/// is present, a clean skip when it is absent, and a panic under
/// `RUSTTY_REQUIRE_FUZZ_CORPUS=1`. They drive the `*_from`/`*_impl` cores
/// directly — never `std::env::set_var` — so they are race-free under the
/// parallel harness and run everywhere.
#[test]
fn corpus_required_only_when_env_is_exactly_one() {
    use std::ffi::OsStr;
    assert!(!corpus_required_from(None));
    assert!(!corpus_required_from(Some(OsStr::new(""))));
    assert!(!corpus_required_from(Some(OsStr::new("0"))));
    assert!(!corpus_required_from(Some(OsStr::new("true"))));
    assert!(corpus_required_from(Some(OsStr::new("1"))));
}

#[test]
fn skip_pivot_does_not_skip_when_the_corpus_is_present() {
    assert!(!skip_without_corpus_impl(true, false, "some_test"));
    assert!(!skip_without_corpus_impl(true, true, "some_test"));
}

#[test]
fn skip_pivot_skips_when_the_corpus_is_absent_and_not_required() {
    assert!(skip_without_corpus_impl(false, false, "some_test"));
}

#[test]
#[should_panic(expected = "the mirrored fuzz corpus is required but absent")]
fn skip_pivot_panics_when_the_corpus_is_required() {
    skip_without_corpus_impl(false, true, "some_test");
}

/// The reference fuzz corpora (`test/fuzz-libghostty/corpus`), reachable only
/// when the caller exports GHOSTTY_ORACLE=<path-to-ghostty-checkout>.
fn oracle_corpus_root() -> Option<PathBuf> {
    let oracle = std::env::var_os("GHOSTTY_ORACLE")?;
    let root = PathBuf::from(oracle).join("test/fuzz-libghostty/corpus");
    if root.is_dir() {
        Some(root)
    } else {
        // An explicitly-set oracle that lacks the corpus is a harness wiring
        // bug, not a skip.
        panic!(
            "GHOSTTY_ORACLE is set but {} is not a directory",
            root.display()
        );
    }
}

/// [`oracle_corpus_root`] for the two `#[ignore]`d oracle tests: they were asked
/// for, so a missing GHOSTTY_ORACLE is a FAILURE naming the fix, never an
/// early-return `ok` (R14).
fn oracle_corpus_root_or_fail(test_name: &str) -> PathBuf {
    oracle_corpus_root().unwrap_or_else(|| {
        panic!(
            "{test_name} needs the Ghostty oracle checkout: set GHOSTTY_ORACLE=<path> \
             (it reads <path>/test/fuzz-libghostty/corpus)"
        )
    })
}

/// Replay every regular file in `dir` through `harness`, returning the file
/// count. Files are replayed in sorted order so a crash reproduces at a
/// stable index.
fn replay_dir(dir: &Path, harness: fn(&[u8])) -> usize {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read_dir({}): {}", dir.display(), e))
        .map(|entry| entry.expect("dir entry").path())
        .filter(|p| p.is_file())
        .collect();
    paths.sort();

    let mut count = 0usize;
    for path in &paths {
        let data = fs::read(path).unwrap_or_else(|e| panic!("read({}): {}", path.display(), e));
        // A panic inside the harness fails the test with the corpus file
        // named in the payload below.
        let result = std::panic::catch_unwind(|| harness(&data));
        assert!(
            result.is_ok(),
            "fuzz harness panicked on corpus file {}",
            path.display()
        );
        count += 1;
    }
    count
}

/// `replay_dir` restricted to the files `keep` accepts (used to skip the
/// prose README that lives alongside the crash reproducers).
fn replay_dir_filtered(dir: &Path, harness: fn(&[u8]), keep: fn(&Path) -> bool) -> usize {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read_dir({}): {}", dir.display(), e))
        .map(|entry| entry.expect("dir entry").path())
        .filter(|p| p.is_file() && keep(p))
        .collect();
    paths.sort();

    let mut count = 0usize;
    for path in &paths {
        let data = fs::read(path).unwrap_or_else(|e| panic!("read({}): {}", path.display(), e));
        let result = std::panic::catch_unwind(|| harness(&data));
        assert!(
            result.is_ok(),
            "fuzz harness panicked on crash reproducer {}",
            path.display()
        );
        count += 1;
    }
    count
}

/// The in-tree seed tier is load-bearing for the test: the seed dirs must
/// exist and hold at least the reference's hand-written seed counts
/// (osc 20, parser 50, stream 24). This is the PROVING half — it fails when
/// no harness consumes the corpus (and fails to compile without
/// `rustty_vt::fuzz_harness` at all).
#[test]
fn replay_port_initial_seeds_osc() {
    if skip_without_corpus("replay_port_initial_seeds_osc") {
        return;
    }
    let n = replay_dir(
        &port_corpus_root().join("osc-initial"),
        fuzz_harness::fuzz_osc,
    );
    assert!(n >= 20, "expected >= 20 osc-initial seeds, found {n}");
}

#[test]
fn replay_port_initial_seeds_parser() {
    if skip_without_corpus("replay_port_initial_seeds_parser") {
        return;
    }
    let n = replay_dir(
        &port_corpus_root().join("parser-initial"),
        fuzz_harness::fuzz_parser,
    );
    assert!(n >= 50, "expected >= 50 parser-initial seeds, found {n}");
}

#[test]
fn replay_port_initial_seeds_stream() {
    if skip_without_corpus("replay_port_initial_seeds_stream") {
        return;
    }
    let n = replay_dir(
        &port_corpus_root().join("stream-initial"),
        fuzz_harness::fuzz_stream,
    );
    assert!(n >= 24, "expected >= 24 stream-initial seeds, found {n}");
}

/// The crash-reproducer tier: every input in `crates/rustty-vt/fuzz/findings/`
/// that a live fuzzing run turned up and that has since been FIXED. These are
/// committed (unlike `fuzz/corpus/` and `fuzz/artifacts/`, both gitignored), so
/// replaying them here is what keeps each fix nailed down under plain
/// `cargo test`, with no nightly/cargo-fuzz toolchain in the loop.
///
/// They are deliberately NOT copied into `.port/phase-0/corpora/cvt/*-initial`:
/// those dirs are a byte-for-byte mirror of the oracle's own seed corpus
/// (`ghostty test/fuzz-libghostty/corpus/*-initial`) and adding port-local
/// files would break that provenance.
#[test]
fn replay_fixed_fuzz_findings_osc() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz/findings");
    let n = replay_dir_filtered(&dir, fuzz_harness::fuzz_osc, |p| {
        p.extension().and_then(|e| e.to_str()) != Some("md")
    });
    // Both known findings are fuzz_osc inputs; the count is a lower bound so
    // adding a new reproducer never needs a code change here.
    assert!(
        n >= 2,
        "expected >= 2 fuzz_osc crash reproducers, found {n}"
    );
}

/// Cross-harness seeding, mirroring live fuzzing where every corpus grows past
/// its home harness: the parser seeds are valid stream inputs (the stream
/// consumes byte 0 as the path selector) and the osc seeds are valid parser
/// inputs. Cheap (94 files) and catches cross-subsystem panics.
#[test]
fn replay_port_initial_seeds_cross() {
    if skip_without_corpus("replay_port_initial_seeds_cross") {
        return;
    }
    let root = port_corpus_root();
    replay_dir(&root.join("parser-initial"), fuzz_harness::fuzz_stream);
    replay_dir(&root.join("osc-initial"), fuzz_harness::fuzz_parser);
}

/// The six corpus tiers and the file count each must reach. Lower bounds, so
/// an oracle bump that ADDS corpus files never breaks the assertion.
type Tier<'a> = (&'a str, fn(&[u8]), usize);
const TIERS: [Tier<'static>; 6] = [
    ("osc-initial", fuzz_harness::fuzz_osc as fn(&[u8]), 20),
    ("osc-cmin", fuzz_harness::fuzz_osc, 20),
    ("parser-initial", fuzz_harness::fuzz_parser, 50),
    ("parser-cmin", fuzz_harness::fuzz_parser, 616),
    ("stream-initial", fuzz_harness::fuzz_stream, 24),
    ("stream-cmin", fuzz_harness::fuzz_stream, 3271),
];

/// The FULL corpus — all 4,001 files across all six tiers — replayed from the
/// PORT TREE, with no oracle checkout in the loop.
///
/// This test is why the `*-cmin` tiers are committed. Until they were, only the
/// 94 hand-written `*-initial` seeds were in-tree and the 3,907 afl-cmin
/// edge-deduped cases lived exclusively in the oracle, reachable only when
/// GHOSTTY_ORACLE was exported. That meant the deep corpus — the tier that
/// actually walks the parser's rare edges — replayed on precisely one machine
/// and never on CI, nor for anyone who cloned the repository. `cargo-fuzz`'s
/// own `fuzz/corpus/` is gitignored (it is a live, growing fuzzer working set),
/// so it could not fill the role either.
///
/// The reference commits all six tiers for the same reason; this mirrors that.
#[test]
fn replay_port_full_corpus() {
    if skip_without_corpus("replay_port_full_corpus") {
        return;
    }
    let root = port_corpus_root();
    let mut total = 0usize;
    for (dir, harness, min) in TIERS {
        let n = replay_dir(&root.join(dir), harness);
        assert!(n >= min, "expected >= {min} files in {dir}, found {n}");
        total += n;
    }
    assert!(
        total >= 4001,
        "expected >= 4001 in-tree corpus files total, found {total}"
    );
    eprintln!("replayed {total} in-tree corpus files clean");
}

/// Provenance cross-check: every in-tree corpus file is BYTE-IDENTICAL to the
/// oracle's copy. Guards the mirror against silent drift — including the
/// line-ending normalization that `.gitattributes` used to allow (see
/// `.port/tools/gates/check_eol_fidelity.sh`). Needs GHOSTTY_ORACLE, so it is
/// `#[ignore]`d and FAILS when asked for without it (R14); `G-EOL` performs the
/// same comparison in the gate suite.
#[test]
#[ignore = "needs the Ghostty oracle: GHOSTTY_ORACLE=<ghostty checkout> cargo test \
            -p rustty-vt --test fuzz_corpus_replay -- --ignored"]
fn port_corpus_matches_oracle_bytes() {
    let oracle_root = oracle_corpus_root_or_fail("port_corpus_matches_oracle_bytes");
    let port_root = port_corpus_root();
    let mut compared = 0usize;
    for (dir, _, _) in TIERS {
        for entry in
            fs::read_dir(port_root.join(dir)).unwrap_or_else(|e| panic!("read_dir({dir}): {e}"))
        {
            let path = entry.expect("dir entry").path();
            if !path.is_file() {
                continue;
            }
            let name = path.file_name().expect("file name");
            let theirs = oracle_root.join(dir).join(name);
            if !theirs.is_file() {
                continue;
            }
            assert_eq!(
                fs::read(&path).expect("read port seed"),
                fs::read(&theirs).expect("read oracle seed"),
                "corpus file {dir}/{} differs from the oracle",
                name.to_string_lossy()
            );
            compared += 1;
        }
    }
    assert!(
        compared >= 4001,
        "expected >= 4001 seeds compared against the oracle, compared {compared}"
    );
}

/// Full oracle-corpus replay: the same six dirs read from the ORACLE checkout
/// rather than the port tree. Retained alongside `replay_port_full_corpus` so
/// an oracle bump that adds corpus files is exercised before the mirror is
/// refreshed. Needs GHOSTTY_ORACLE, so it is `#[ignore]`d and FAILS when asked
/// for without it (R14).
#[test]
#[ignore = "needs the Ghostty oracle: GHOSTTY_ORACLE=<ghostty checkout> cargo test \
            -p rustty-vt --test fuzz_corpus_replay -- --ignored"]
fn replay_full_oracle_corpus() {
    let root = oracle_corpus_root_or_fail("replay_full_oracle_corpus");

    let mut total = 0usize;
    for (dir, harness, min) in TIERS {
        let n = replay_dir(&root.join(dir), harness);
        assert!(n >= min, "expected >= {min} files in {dir}, found {n}");
        total += n;
    }
    assert!(
        total >= 4001,
        "expected >= 4001 oracle corpus files total, found {total}"
    );
    eprintln!("replayed {total} oracle corpus files clean");
}
