//! Deterministic static-archive discovery for the xcframework combine step.
//!
//! PORTS: `ghostty/src/build/GhosttyLib.zig:52-57` (the enumerated
//! `deps.add(lib)` + `getEmittedBin()` list handed to the combine step) and
//! `ghostty/src/build/LibtoolStep.zig:54-78` (`normalizeArchive`).
//!
//! Why this module exists (INTEGRATION_PARITY §2.4/§2.5): xtask used to find
//! the `-sys` crates' archives with a one-level `read_dir` over
//! `target/<t>/xcframework/build/*/out/*.a`. That is a function of leftover
//! filesystem state, not of the dependency graph, and it fails two ways:
//!
//!   1. a crate whose archive lands in a NESTED OUT_DIR subdirectory (the
//!      cmake-style `out/lib/libfoo.a`) is silently missed -> undefined
//!      symbols at app-link;
//!   2. several hash-suffixed build dirs for the same crate leave more than one
//!      matching `.a`, so libtool folds stale/duplicate objects -> duplicate or
//!      wrong-version symbols.
//!
//! Cargo already knows the exact answer. Under `--message-format=json` every
//! build script reports the `cargo:rustc-link-lib` / `cargo:rustc-link-search`
//! directives it emitted, and the staticlib target reports its own output
//! filenames. This module turns that stream into an explicit set — the direct
//! analogue of the oracle's enumerated dep list — and treats an archive it
//! cannot locate as a HARD ERROR rather than a silent omission.

use std::path::{Path, PathBuf};
use std::process::Command;

/// One `cargo:rustc-link-lib=static=<name>` a build script emitted, together
/// with every `cargo:rustc-link-search` path that same script emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeArchiveRequest {
    /// Cargo's package id, used only for error messages.
    pub package_id: String,
    /// The library name as declared (`z` for `static=z`), i.e. without the
    /// `lib` prefix and `.a` suffix.
    pub lib_name: String,
    /// The search paths that script emitted, in emission order.
    pub search_paths: Vec<PathBuf>,
}

impl NativeArchiveRequest {
    /// Resolve to a concrete `lib<name>.a`, using `exists` to probe the
    /// filesystem (injected so the resolution rules are unit-testable).
    ///
    /// The first search path that actually holds the archive wins. Failing to
    /// find it is an error, never a skip.
    pub fn resolve_with(&self, exists: &dyn Fn(&Path) -> bool) -> Result<PathBuf, String> {
        let file = format!("lib{}.a", self.lib_name);
        for dir in &self.search_paths {
            let candidate = dir.join(&file);
            if exists(&candidate) {
                return Ok(candidate);
            }
        }
        Err(format!(
            "package {} declared `static={}` but {file} was not found in any of its \
             link-search paths ({:?}). The combined archive would be missing its symbols.",
            self.package_id, self.lib_name, self.search_paths
        ))
    }

    /// [`Self::resolve_with`] against the real filesystem.
    pub fn resolve(&self) -> Result<PathBuf, String> {
        self.resolve_with(&|p: &Path| p.exists())
    }
}

/// Everything the combine step needs out of one cargo JSON message stream.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ParsedArtifacts {
    /// `.a` outputs of `staticlib` targets (rustty's own Rust staticlib).
    pub staticlibs: Vec<PathBuf>,
    /// Static archives the `-sys` build scripts declared, de-duplicated per
    /// (package, lib name) in first-seen order.
    pub native: Vec<NativeArchiveRequest>,
}

/// Parse a newline-delimited `cargo build --message-format=json` stream.
///
/// Unparseable lines and messages of other `reason`s are skipped: cargo mixes
/// `compiler-message`, `build-finished`, etc. into the same stream, and none of
/// them describe archives.
pub fn parse_artifacts(json: &str) -> Result<ParsedArtifacts, String> {
    let mut out = ParsedArtifacts::default();
    // (package_id, lib_name) pairs already recorded, so a script that emits the
    // same `static=` directive twice contributes one entry.
    let mut seen: Vec<(String, String)> = Vec::new();

    for line in json.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let msg: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match msg.get("reason").and_then(|r| r.as_str()) {
            Some("compiler-artifact") => {
                let is_staticlib = msg
                    .get("target")
                    .and_then(|t| t.get("kind"))
                    .and_then(|k| k.as_array())
                    .is_some_and(|kinds| kinds.iter().any(|k| k.as_str() == Some("staticlib")));
                if !is_staticlib {
                    continue;
                }
                let files = msg.get("filenames").and_then(|f| f.as_array());
                for f in files.into_iter().flatten() {
                    let Some(s) = f.as_str() else { continue };
                    if s.ends_with(".a") {
                        out.staticlibs.push(PathBuf::from(s));
                    }
                }
            }
            Some("build-script-executed") => {
                let package_id = msg
                    .get("package_id")
                    .and_then(|p| p.as_str())
                    .unwrap_or("<unknown package>")
                    .to_string();

                // `linked_paths` are raw `cargo:rustc-link-search` values:
                // either `native=/dir` (or another kind prefix) or a bare dir.
                let search_paths: Vec<PathBuf> = msg
                    .get("linked_paths")
                    .and_then(|p| p.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| PathBuf::from(strip_kind(s)))
                            .collect()
                    })
                    .unwrap_or_default();

                let libs = msg.get("linked_libs").and_then(|l| l.as_array());
                for lib in libs.into_iter().flatten() {
                    let Some(spec) = lib.as_str() else { continue };
                    // Only `static=` entries name an archive we must fold.
                    // `framework=`, `dylib=` and bare names are system links
                    // the app resolves normally.
                    let Some(name) = spec.strip_prefix("static=") else {
                        continue;
                    };
                    // Cargo allows `static:+bundle=name` style modifiers; take
                    // the part after the last `=`.
                    let name = name.rsplit('=').next().unwrap_or(name);
                    let key = (package_id.clone(), name.to_string());
                    if seen.contains(&key) {
                        continue;
                    }
                    seen.push(key);
                    out.native.push(NativeArchiveRequest {
                        package_id: package_id.clone(),
                        lib_name: name.to_string(),
                        search_paths: search_paths.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Strip a `kind=` prefix from a raw cargo link directive value
/// (`native=/dir` -> `/dir`; a bare `/dir` is returned unchanged).
fn strip_kind(value: &str) -> &str {
    match value.split_once('=') {
        Some((_, rest)) => rest,
        None => value,
    }
}

/// Run `cargo build … --message-format=json` and return its stdout.
///
/// `args` are the cargo arguments AFTER `build` (e.g. `["-p", "rustty-apprt-c",
/// "--profile", "xcframework"]`); `--message-format=json` is appended here so
/// no caller can forget it. `envs` are extra environment variables to set
/// (`MACOSX_DEPLOYMENT_TARGET`). stderr is inherited so the human still sees
/// cargo's progress and warnings.
pub fn cargo_build_json(
    root: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<String, String> {
    let mut cmd = Command::new("cargo");
    cmd.arg("build").args(args).arg("--message-format=json");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd
        .current_dir(root)
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| format!("spawn cargo build: {e}"))?;
    if !out.status.success() {
        return Err(format!("cargo build exited with {}", out.status));
    }
    String::from_utf8(out.stdout).map_err(|e| format!("cargo JSON output was not UTF-8: {e}"))
}

/// Copy `src` to `dst` and run `/usr/bin/ranlib` on the copy.
///
/// PORTS `ghostty/src/build/LibtoolStep.zig:54-78` `normalizeArchive`, verbatim
/// in intent (the oracle runs `/bin/cp "$1" "$2" && /usr/bin/ranlib "$2"` on
/// EVERY source archive before handing it to `libtool`).
///
/// Why: newer Xcode `libtool` can silently DROP 64-bit archive members when the
/// input archive layout is not what it expects — producing a combined archive
/// that is missing symbols, nondeterministically, with no error. `ranlib`
/// rewrites the archive with a regenerated symbol table, normalizing the
/// layout. rustty is more exposed than the oracle here, because every archive it
/// folds comes from a `cc`/`ar` build script — exactly the layout the oracle's
/// comment warns about (INTEGRATION_PARITY §2.5).
///
/// The copy matters: the sources are cargo's own build outputs and must not be
/// rewritten in place, or cargo's freshness tracking sees them change.
pub fn normalize_archive(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let _ = std::fs::remove_file(dst);
    std::fs::copy(src, dst)
        .map_err(|e| format!("copy {} -> {}: {e}", src.display(), dst.display()))?;
    let status = Command::new("/usr/bin/ranlib")
        .arg(dst)
        .status()
        .map_err(|e| format!("spawn /usr/bin/ranlib: {e}"))?;
    if !status.success() {
        return Err(format!("ranlib {} exited with {status}", dst.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// A realistic two-line slice of `cargo build --message-format=json`: one
    /// `compiler-artifact` for the staticlib target, one `build-script-executed`
    /// for a `-sys` crate. Shapes verified against a live cargo run (2026-07-28).
    const SAMPLE: &str = r#"{"reason":"compiler-artifact","package_id":"path+file:///w/crates/rustty-apprt-c#0.1.0","target":{"kind":["staticlib"],"crate_types":["staticlib"],"name":"rustty_apprt_c"},"filenames":["/w/target/aarch64-apple-darwin/xcframework/librustty_apprt_c.a"],"fresh":false}
{"reason":"build-script-executed","package_id":"path+file:///w/crates-sys/zlib-sys#0.1.0","linked_libs":["static=z"],"linked_paths":["native=/w/target/aarch64-apple-darwin/xcframework/build/zlib-sys-abc/out"],"cfgs":[],"env":[],"out_dir":"/w/target/aarch64-apple-darwin/xcframework/build/zlib-sys-abc/out"}"#;

    #[test]
    fn parses_staticlib_and_one_native_archive() {
        let p = parse_artifacts(SAMPLE).unwrap();
        assert_eq!(
            p.staticlibs,
            vec![PathBuf::from("/w/target/aarch64-apple-darwin/xcframework/librustty_apprt_c.a")]
        );
        assert_eq!(p.native.len(), 1);
        assert_eq!(p.native[0].lib_name, "z");
        assert_eq!(
            p.native[0].search_paths,
            vec![PathBuf::from(
                "/w/target/aarch64-apple-darwin/xcframework/build/zlib-sys-abc/out"
            )]
        );
    }

    /// `framework=`, `dylib=` and bare (system) libs are NOT archives to fold.
    #[test]
    fn ignores_non_static_link_kinds() {
        let json = r#"{"reason":"build-script-executed","package_id":"p#1","linked_libs":["framework=CoreFoundation","dylib=pthread","m"],"linked_paths":["native=/w/out"],"cfgs":[],"env":[],"out_dir":"/w/out"}"#;
        let p = parse_artifacts(json).unwrap();
        assert!(p.native.is_empty());
    }

    /// A `-sys` crate that emits the same `static=` lib twice (or two search
    /// paths) must yield exactly ONE request — the §2.4 duplicate-fold hazard.
    #[test]
    fn dedups_repeated_static_libs_per_package() {
        let json = r#"{"reason":"build-script-executed","package_id":"p#1","linked_libs":["static=onig","static=onig"],"linked_paths":["native=/w/a","native=/w/b"],"cfgs":[],"env":[],"out_dir":"/w/a"}"#;
        let p = parse_artifacts(json).unwrap();
        assert_eq!(p.native.len(), 1);
        assert_eq!(p.native[0].lib_name, "onig");
        assert_eq!(
            p.native[0].search_paths,
            vec![PathBuf::from("/w/a"), PathBuf::from("/w/b")]
        );
    }

    /// Non-JSON lines (cargo prints nothing else on stdout under
    /// --message-format=json, but be robust) and unrelated reasons are skipped.
    #[test]
    fn skips_unparseable_and_unrelated_lines() {
        let json = "not json\n{\"reason\":\"build-finished\",\"success\":true}\n";
        let p = parse_artifacts(json).unwrap();
        assert!(p.staticlibs.is_empty());
        assert!(p.native.is_empty());
    }

    /// A NESTED OUT_DIR (cmake-style `out/lib/libfoo.a`) resolves, because the
    /// search path cargo reports is the nested dir itself — the §2.4
    /// "non-recursive glob misses it" hazard, gone by construction.
    #[test]
    fn resolves_nested_out_dir_search_path() {
        let req = NativeArchiveRequest {
            package_id: "p#1".to_string(),
            lib_name: "onig".to_string(),
            search_paths: vec![PathBuf::from("/w/out/lib")],
        };
        let exists = |p: &Path| p == Path::new("/w/out/lib/libonig.a");
        assert_eq!(
            req.resolve_with(&exists).unwrap(),
            PathBuf::from("/w/out/lib/libonig.a")
        );
    }

    /// The first search path that HAS the archive wins; earlier misses are
    /// skipped rather than treated as failures.
    #[test]
    fn resolve_picks_first_existing_search_path() {
        let req = NativeArchiveRequest {
            package_id: "p#1".to_string(),
            lib_name: "z".to_string(),
            search_paths: vec![PathBuf::from("/w/a"), PathBuf::from("/w/b")],
        };
        let exists = |p: &Path| p == Path::new("/w/b/libz.a");
        assert_eq!(req.resolve_with(&exists).unwrap(), PathBuf::from("/w/b/libz.a"));
    }

    /// `normalize_archive` must produce a copy WITH a regenerated symbol table
    /// and leave the source alone. Built from a real object file so the
    /// assertion exercises the real archive format (macOS only — the fix is a
    /// Darwin `libtool` interoperability workaround).
    #[test]
    #[cfg(target_os = "macos")]
    fn normalize_archive_regenerates_the_symbol_table() {
        use std::process::Command;

        let tmp = std::env::temp_dir().join(format!("ws9-ranlib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let c = tmp.join("p.c");
        std::fs::write(&c, b"int ws9_probe(void){return 42;}\n").unwrap();
        let o = tmp.join("p.o");
        assert!(Command::new("clang")
            .args(["-c", "-o"])
            .arg(&o)
            .arg(&c)
            .status()
            .unwrap()
            .success());

        // `ar rcS` deliberately omits the symbol table (`S` = no index) — the
        // "unexpected layout" shape LibtoolStep.zig warns about.
        let src = tmp.join("libws9probe.a");
        assert!(Command::new("/usr/bin/ar")
            .arg("rcS")
            .arg(&src)
            .arg(&o)
            .status()
            .unwrap()
            .success());

        let toc = |p: &Path| -> String {
            let out = Command::new("/usr/bin/ar").arg("-t").arg(p).output().unwrap();
            String::from_utf8_lossy(&out.stdout).to_string()
        };
        assert!(
            !toc(&src).contains("__.SYMDEF"),
            "precondition: the source archive must have no symbol table"
        );

        let dst = tmp.join("0-libRusttyKit.a");
        normalize_archive(&src, &dst).unwrap();

        assert!(
            toc(&dst).contains("__.SYMDEF"),
            "normalized archive must carry a regenerated symbol table"
        );
        assert!(
            !toc(&src).contains("__.SYMDEF"),
            "the source archive must not be modified in place"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// An unlocatable declared archive is a HARD ERROR — never a silent skip.
    /// Silently dropping it is exactly how §2.4's undefined symbols appear.
    #[test]
    fn resolve_errors_when_archive_is_missing() {
        let req = NativeArchiveRequest {
            package_id: "path+file:///w/crates-sys/zlib-sys#0.1.0".to_string(),
            lib_name: "z".to_string(),
            search_paths: vec![PathBuf::from("/w/a")],
        };
        let err = req.resolve_with(&|_| false).unwrap_err();
        assert!(err.contains("libz.a"), "error must name the archive: {err}");
        assert!(err.contains("zlib-sys"), "error must name the package: {err}");
    }
}
