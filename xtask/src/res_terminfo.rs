//! Terminfo + `share/{APP}` resource generation for the macOS Copy-Resources
//! phase (runtime-critical resource). `{APP}` is [`crate::APP`]: `rustty` by
//! default, `ghostty` under this xtask's own `identity-oracle` feature.
//!
//! PORTS: the terminfo branch of ghostty's `src/build/GhosttyResources.zig` — the
//! `+terminfo` `build_data_exe` run that emits the terminfo *source* followed by
//! the `tic -x -o share/terminfo` compile step (ghostty runs `tic` in its
//! `install` recipe). rustty already ported the terminfo model (`rustty-terminfo`:
//! `entry::rustty()` builds the `Source`, `Source::encode` writes tic-compilable
//! text), so this module drives those directly and shells out to `tic`,
//! resolved BY NAME on `PATH` exactly like ghostty's `RunStep` (W2-03: this
//! used to hardcode `/usr/bin/tic`, breaking Nix/prefix hosts where ncurses
//! lives elsewhere; ghostty never pins the path).
//!
//! W2-03 target layout: the generator is TARGET-AWARE like ghostty's
//! `cfg.target`-driven build. FreeBSD installs the tree under
//! `share/site-terminfo` (GhosttyResources.zig `terminfo_share_dir`, matching
//! the runtime lookup sentinels in `rustty-os`'s `resourcesdir.rs`); every
//! other target uses `share/terminfo`. Windows emits only the terminfo
//! *source* and skips `tic`/`infotocap` entirely (the oracle's
//! `if (os_tag == .windows) break :terminfo;` — those binaries don't exist
//! there). Since xtask runs on the HOST, [`gen_terminfo`] takes the target OS
//! as a parameter (`xtask resources --target-os <os>`, defaulting to the
//! host).
//!
//! R2 identity note (Task 18): `xterm-rustty` is rustty's CANONICAL primary
//! terminfo name (`rustty-identity::TERMINFO_PRIMARY_NAME`, the value of `TERM`
//! a native rustty build sets), and it is what `rustty_terminfo::entry::rustty()`
//! builds by DEFAULT. This module used to force the `identity-oracle` parity
//! spelling (`xterm-ghostty | ghostty | Ghostty`) on UNCONDITIONALLY via this
//! crate's `rustty-terminfo` dependency, on the obsolete rationale that the
//! reused macOS Swift app hard-coded `TERM=xterm-ghostty`. That app no longer
//! hardcodes ghostty post-rebrand, so the force was a real bug: the shipped
//! default-profile terminfo had ghostty content, and `TERM=xterm-rustty`
//! couldn't resolve. The default profile now compiles the canonical
//! `xterm-rustty | rustty | Rustty` entry; the oracle spelling remains
//! available for parity checks via this xtask crate's OWN `identity-oracle`
//! feature (forwarded to `rustty-terminfo/identity-oracle` — see
//! `xtask/Cargo.toml`).
//!
//! R5: the terminfo generator is fully ported, so terminfo carries no deferral.
//! In addition to the compiled DB, this module now also emits the terminfo
//! *source* (`share/terminfo/ghostty.terminfo`) and a `termcap` translation
//! (`share/terminfo/ghostty.termcap`) alongside it — the `emit_terminfo` /
//! `emit_termcap` installs of ghostty's `GhosttyResources.zig` (P1.3,
//! INTEGRATION_PARITY §2.8). Both are gated by [`Emit`], whose defaults are
//! the reference's: TRUE for Debug, FALSE for the release modes
//! (`xtask resources --optimize`, overridable with `--emit-terminfo` /
//! `--emit-termcap`). CORRECTION (S64-F8, 2026-09-23): this note used to say
//! "rustty emits them unconditionally so `infocmp`/Debug tooling has
//! byte-identical inputs regardless of profile". The termcap is produced by
//! `infotocap` (resolved by name on `PATH`) reading that same source (exactly
//! ghostty's `RunStep "infotocap" <source>`), whose stderr — capability-translation
//! warnings + the 1023-byte-truncation notices termcap inherently requires — is
//! captured and discarded, matching ghostty's `captureStdErr()`.
//!
//! R2 identity note (source/termcap filenames, Task 14/18): the compiled
//! entry's primary name now follows the SAME profile as everything else
//! (`xterm-rustty` by default, `xterm-ghostty` under `identity-oracle` — see
//! above), and so do the `{APP}.terminfo`/`{APP}.termcap` DUMP basenames: these
//! are just a human/debug-readable side artifact (`tic`/`infocmp` resolve
//! entries by the compiled DB content, never by this dump file's name), and
//! follow [`crate::APP`] like every other output filename: `rustty.terminfo`/
//! `rustty.termcap` by default, `ghostty.terminfo`/`ghostty.termcap` under
//! `identity-oracle`.
//!
//! The `share/{APP}/` tree's contents (shell-integration scripts, themes) are
//! emitted by the sibling generators `res_shell_integration` / `res_themes`
//! from the vendored trees under `crates/rustty-extra/` (WS1 — the pre-WS1
//! `DEFERRALS.md` placeholder note is gone); [`gen_app_root`] only guarantees
//! the root folder exists and scrubs any stale note from earlier builds.

use std::path::Path;
use std::process::Command;

/// The reference's `-Doptimize` mode, which picks the `emit_*` defaults
/// (`Config.zig:504-522`). `zig build` without `-Doptimize` is `Debug`, so
/// that is the default here too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Optimize {
    Debug,
    ReleaseSafe,
    ReleaseFast,
    ReleaseSmall,
}

impl Optimize {
    /// Parse the reference's spelling (`-Doptimize=ReleaseFast`).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Debug" => Some(Self::Debug),
            "ReleaseSafe" => Some(Self::ReleaseSafe),
            "ReleaseFast" => Some(Self::ReleaseFast),
            "ReleaseSmall" => Some(Self::ReleaseSmall),
            _ => None,
        }
    }
}

/// `cfg.emit_terminfo` / `cfg.emit_termcap`: whether the terminfo SOURCE and
/// its termcap translation are installed next to the compiled database
/// (`GhosttyResources.zig:43-77`). The compiled database is installed either
/// way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Emit {
    pub terminfo: bool,
    pub termcap: bool,
}

impl Emit {
    /// The defaults `Config.zig:504-522` gives when neither `-Demit-terminfo`
    /// nor `-Demit-termcap` is passed: both on for `Debug` and off for every
    /// release mode, except that a Windows target always installs the source
    /// (its runtime lookup keys off that file).
    pub fn defaults(target_os: &str, optimize: Optimize) -> Self {
        let debug = optimize == Optimize::Debug;
        Self {
            terminfo: target_os == "windows" || debug,
            termcap: debug,
        }
    }
}

/// The terminfo tree's directory name under `share/` for a target OS: FreeBSD
/// uses `site-terminfo` (its base system owns `share/terminfo`; third-party
/// entries belong in the local site tree), everything else `terminfo`.
///
/// Byte-for-byte the oracle's `terminfo_share_dir` pick
/// (`GhosttyResources.zig`: `if (os_tag == .freebsd) "site-terminfo" else
/// "terminfo"`), and the value the runtime lookup expects — `rustty-os`'s
/// `resourcesdir.rs` FreeBSD sentinels are `site-terminfo/<letter>/{rustty,
/// xterm-rustty}` under `local/share`, so a `share/terminfo` tree would be
/// undiscoverable there (W2-03).
fn terminfo_dir_name(target_os: &str) -> &'static str {
    if target_os == "freebsd" {
        "site-terminfo"
    } else {
        "terminfo"
    }
}

/// Compile the terminfo entry into `share/terminfo/…` (`share/site-terminfo/…`
/// on FreeBSD — see [`terminfo_dir_name`]) and create the `share/{APP}/`
/// resource root.
///
/// `share_dir` is the `share/` directory (e.g. `zig-out/share`), matching the
/// `gen_*` convention of the sibling resource modules. `target_os` is the
/// TARGET's OS in `std::env::consts::OS` spelling (`"linux"`, `"macos"`,
/// `"freebsd"`, `"windows"`, …) — the analogue of ghostty's
/// `cfg.target.result.os.tag`; xtask runs on the host, so cross-target trees
/// are selected via `xtask resources --target-os <os>`.
///
/// Pipeline (mirrors ghostty's `GhosttyResources.zig` terminfo install):
///   1. Build the `Source` via `rustty_terminfo::entry::rustty()` (identity-oracle
///      spelling — see the R2 note above) and `encode` it to tic source text.
///   2. Write that text to `share/<dir>/{APP}.terminfo` (the `emit_terminfo`
///      source install — see [`emit_termcap`]'s doc for the emit policy). On a
///      **windows** target, STOP here (plus the `share/{APP}` root): ghostty's
///      `if (os_tag == .windows) break :terminfo;` — `tic`/`infotocap` don't
///      exist there, and the runtime lookup sentinel on Windows is exactly this
///      source file (`terminfo/rustty.terminfo`, `resourcesdir.rs`).
///   3. Run `tic -x -o <share>/<dir> <source>` to compile it, producing
///      `share/<dir>/<subdir>/xterm-rustty` by default (`xterm-ghostty` under
///      `identity-oracle`; + the `rustty`/`Rustty` or `ghostty`/`Ghostty` alias
///      entries — same profile as [`crate::APP`], see the R2 note above).
///      macOS's case-insensitive filesystem makes `tic` use hashed
///      two-hex-digit subdirectories (e.g. `78/xterm-rustty`) rather than the
///      single-letter `x/…` layout used on Linux; both are valid terminfo trees.
///   4. Translate the source to `{APP}.termcap` via `infotocap` (see
///      [`emit_termcap`]).
///   5. Create the `share/{APP}/` root (see [`gen_app_root`]).
pub fn gen_terminfo(share_dir: &Path, target_os: &str, emit: Emit) -> Result<(), String> {
    // 1. Build + encode the terminfo source.
    let source = rustty_terminfo::entry::rustty();
    let mut src_bytes: Vec<u8> = Vec::new();
    source
        .encode(&mut src_bytes)
        .map_err(|e| format!("encode terminfo source: {e}"))?;
    if src_bytes.is_empty() {
        return Err("terminfo source encoded to empty output".to_string());
    }

    // The oracle writes the source into a build-cache WriteFiles dir
    // (`wf.addCopyFile(..., "ghostty.terminfo")`) that tic and infotocap read,
    // and INSTALLS it only `if (cfg.emit_terminfo)` (S64-F8). The private
    // work dir here is that cache copy.
    let work = std::env::temp_dir().join(format!(
        "rustty-xtask-terminfo-src-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&work).map_err(|e| format!("create {}: {e}", work.display()))?;
    let result = gen_terminfo_from(share_dir, target_os, emit, &source, &src_bytes, &work);
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// The body of [`gen_terminfo`] once the source is encoded; `work` is the
/// private directory the uninstalled source copy lives in.
fn gen_terminfo_from(
    share_dir: &Path,
    target_os: &str,
    emit: Emit,
    source: &rustty_terminfo::Source,
    src_bytes: &[u8],
    work: &Path,
) -> Result<(), String> {
    let src_path = work.join(format!("{}.terminfo", crate::APP));
    std::fs::write(&src_path, src_bytes)
        .map_err(|e| format!("write {}: {e}", src_path.display()))?;

    // 2. Install the SOURCE into the target-appropriate tree when
    //    `emit_terminfo` (P1.3 / INTEGRATION_PARITY §2.8).
    let terminfo_dir = share_dir.join(terminfo_dir_name(target_os));
    std::fs::create_dir_all(&terminfo_dir)
        .map_err(|e| format!("create {}: {e}", terminfo_dir.display()))?;
    if emit.terminfo {
        let installed = terminfo_dir.join(format!("{}.terminfo", crate::APP));
        std::fs::write(&installed, src_bytes)
            .map_err(|e| format!("write {}: {e}", installed.display()))?;
    }

    // Windows: no tic/infotocap binaries exist — the oracle breaks out of the
    // terminfo block right after the source install (`break :terminfo`), and
    // the Windows runtime lookup keys off the source file itself.
    if target_os == "windows" {
        gen_app_root(share_dir)?;
        return Ok(());
    }

    // 3. Compile with tic into the tree.
    run_tic(&terminfo_dir, &src_path)?;

    // Verify tic actually produced the compiled entry (runtime-critical resource).
    // The expected primary name follows the active profile — `xterm-rustty` by
    // default, `xterm-ghostty` under `identity-oracle` — read directly off the
    // `Source` we just compiled rather than a hardcoded literal (Task 18: this
    // used to hardcode `xterm-ghostty` unconditionally, which broke once the
    // default profile stopped being forced to emit ghostty content).
    let primary_name = String::from_utf8_lossy(&source.names[0]).into_owned();
    if !compiled_entry_exists(&terminfo_dir, &primary_name) {
        return Err(format!(
            "tic reported success but no compiled {primary_name} entry found under {}",
            terminfo_dir.display()
        ));
    }

    // 4. The termcap translation alongside the compiled DB (ghostty
    //    `if (cfg.emit_termcap)`).
    if emit.termcap {
        emit_termcap(&terminfo_dir, &src_path)?;
    }

    // 5. The share/{APP} resource root.
    gen_app_root(share_dir)?;

    Ok(())
}

/// Run `tic -x -o <out_dir> <src>`. `-x` includes the user-defined
/// (extended) capabilities rustty's entry relies on (`Smulx`, `Setulc`, `Sync`,
/// …). tic emits capability warnings to stderr for entries newer than the host
/// ncurses; those are non-fatal. We only fail on a non-zero exit.
///
/// W2-03: `tic` is resolved BY NAME on `PATH` — ghostty's `RunStep` args are
/// bare `"tic"` — never a hardcoded `/usr/bin/tic` (which breaks Nix/prefix
/// hosts whose ncurses lives elsewhere; where `/usr/bin` is on `PATH` the
/// behavior is unchanged).
fn run_tic(out_dir: &Path, src: &Path) -> Result<(), String> {
    let output = Command::new("tic")
        .arg("-x")
        .arg("-o")
        .arg(out_dir)
        .arg(src)
        .output()
        .map_err(|e| format!("spawn tic (must be on PATH): {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "tic exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Emit the `termcap` translation (`{APP}.termcap`) of the already-installed
/// terminfo source next to it, mirroring ghostty's `GhosttyResources.zig`
/// `emit_termcap` install (P1.3, INTEGRATION_PARITY §2.8).
///
/// `{APP}.termcap` — `infotocap <source>` (ghostty's `RunStep "infotocap"`,
/// resolved by name on `PATH` — W2-03, same as [`run_tic`]). termcap is a
/// strictly lossier legacy format bounded to 1023 bytes/entry, so `infotocap`
/// prints translation + truncation notices to stderr; we capture and DISCARD
/// them (ghostty `captureStdErr()`), failing only on a non-zero exit or empty
/// stdout.
fn emit_termcap(terminfo_dir: &Path, src_path: &Path) -> Result<(), String> {
    let output = Command::new("infotocap")
        .arg(src_path)
        .output()
        .map_err(|e| format!("spawn infotocap (must be on PATH): {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "infotocap exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if output.stdout.is_empty() {
        return Err("infotocap produced empty termcap output".to_string());
    }

    let cap_path = terminfo_dir.join(format!("{}.termcap", crate::APP));
    std::fs::write(&cap_path, &output.stdout)
        .map_err(|e| format!("write {}: {e}", cap_path.display()))?;
    Ok(())
}

/// True if tic produced a compiled entry named `primary_name` anywhere in the
/// terminfo tree (`xterm-rustty` by default, `xterm-ghostty` under
/// `identity-oracle` — Task 18). tic uses either single-letter (`x/xterm-…`) or
/// hashed (`78/xterm-…`) subdirectories depending on the host filesystem, so we
/// scan the immediate subdirectories rather than assuming one layout.
fn compiled_entry_exists(terminfo_dir: &Path, primary_name: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(terminfo_dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let sub = entry.path();
        if sub.is_dir() && sub.join(primary_name).exists() {
            return true;
        }
    }
    false
}

/// Create the `share/{APP}/` resource root the macOS Copy-Resources phase
/// references as `../zig-out/share/{APP}` (`{APP}` = [`crate::APP`]: `rustty`
/// by default, `ghostty` under `identity-oracle` — Task 14; this dir must
/// match the runtime lookup, which resolves the same way via
/// `rustty_os::resourcesdir`'s `app_subdir()` -> `rustty_identity::APP_NAME`).
///
/// The subtrees under it — `shell-integration/` and `themes/` — are populated
/// by the sibling generators (`res_shell_integration::gen_shell_integration`,
/// `res_themes::gen_themes`) from the REAL vendored trees in
/// `crates/rustty-extra/` (WS1); this fn only guarantees the root exists.
/// It also SCRUBS the pre-WS1 `DEFERRALS.md` placeholder note from stale
/// output dirs so it can never ship in the app bundle again.
fn gen_app_root(share_dir: &Path) -> Result<(), String> {
    let app_dir = share_dir.join(crate::APP);
    std::fs::create_dir_all(&app_dir).map_err(|e| format!("create {}: {e}", app_dir.display()))?;

    let stale_note = app_dir.join("DEFERRALS.md");
    match std::fs::remove_file(&stale_note) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("remove stale {}: {e}", stale_note.display())),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: Emit = Emit {
        terminfo: true,
        termcap: true,
    };

    /// `Config.zig:504-522`: source and termcap default on for Debug, off for
    /// the three release modes; a Windows target always installs the source.
    #[test]
    fn emit_defaults_follow_the_reference_config() {
        for os in ["macos", "linux", "freebsd"] {
            assert_eq!(Emit::defaults(os, Optimize::Debug), ALL);
            for opt in [
                Optimize::ReleaseSafe,
                Optimize::ReleaseFast,
                Optimize::ReleaseSmall,
            ] {
                assert_eq!(
                    Emit::defaults(os, opt),
                    Emit {
                        terminfo: false,
                        termcap: false
                    }
                );
            }
        }
        assert_eq!(
            Emit::defaults("windows", Optimize::ReleaseFast),
            Emit {
                terminfo: true,
                termcap: false
            }
        );
        assert_eq!(Optimize::parse("ReleaseSafe"), Some(Optimize::ReleaseSafe));
        assert_eq!(Optimize::parse("release"), None);
    }

    /// S64-F8: with `emit_terminfo` and `emit_termcap` off (the release
    /// defaults) the source and termcap are NOT installed
    /// (`GhosttyResources.zig:43-77`), but the compiled database still is:
    /// `tic` reads the source from outside the install tree.
    #[test]
    fn gen_terminfo_release_installs_only_the_compiled_database() {
        let base = std::env::temp_dir().join(format!(
            "rustty-xtask-tinfo-release-test-{}",
            std::process::id()
        ));
        let share = base.join("share");
        std::fs::create_dir_all(&share).unwrap();

        gen_terminfo(
            &share,
            "macos",
            Emit {
                terminfo: false,
                termcap: false,
            },
        )
        .expect("gen_terminfo release");

        let tree = share.join("terminfo");
        let primary_name =
            String::from_utf8(rustty_terminfo::entry::rustty().names[0].clone()).unwrap();
        assert!(compiled_entry_exists(&tree, &primary_name));
        assert!(!tree.join(format!("{}.terminfo", crate::APP)).exists());
        assert!(!tree.join(format!("{}.termcap", crate::APP)).exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The two gates are independent: termcap on, source off installs the
    /// termcap only.
    #[test]
    fn gen_terminfo_termcap_without_source() {
        let base = std::env::temp_dir().join(format!(
            "rustty-xtask-tinfo-caponly-test-{}",
            std::process::id()
        ));
        let share = base.join("share");
        std::fs::create_dir_all(&share).unwrap();

        gen_terminfo(
            &share,
            "linux",
            Emit {
                terminfo: false,
                termcap: true,
            },
        )
        .expect("gen_terminfo termcap only");

        let tree = share.join("terminfo");
        assert!(!tree.join(format!("{}.terminfo", crate::APP)).exists());
        assert!(tree.join(format!("{}.termcap", crate::APP)).is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// W2-03: the tree's directory name matches the runtime lookup
    /// (`rustty-os` `resourcesdir.rs` sentinels) per target, byte-for-byte the
    /// oracle's `terminfo_share_dir` pick: FreeBSD alone uses `site-terminfo`.
    #[test]
    fn terminfo_dir_name_matches_runtime_lookup() {
        assert_eq!(terminfo_dir_name("freebsd"), "site-terminfo");
        assert_eq!(terminfo_dir_name("linux"), "terminfo");
        assert_eq!(terminfo_dir_name("macos"), "terminfo");
        assert_eq!(terminfo_dir_name("windows"), "terminfo");
    }

    /// W2-03: a FreeBSD-target run lands the WHOLE tree (compiled DB + source +
    /// termcap) under `share/site-terminfo`, never `share/terminfo` — the
    /// layout `rustty-os`'s FreeBSD sentinels
    /// (`site-terminfo/<letter>/{rustty,xterm-rustty}` under `local/share`)
    /// actually probe. Runs the host tic, so this is host-independent.
    #[test]
    fn gen_terminfo_freebsd_uses_site_terminfo() {
        let base = std::env::temp_dir().join(format!(
            "rustty-xtask-tinfo-fbsd-test-{}",
            std::process::id()
        ));
        let share = base.join("share");
        std::fs::create_dir_all(&share).unwrap();

        gen_terminfo(&share, "freebsd", ALL).expect("gen_terminfo freebsd");

        let tree = share.join("site-terminfo");
        let primary_name = rustty_terminfo::entry::rustty().names[0].clone();
        let primary_name = String::from_utf8(primary_name).unwrap();
        assert!(
            compiled_entry_exists(&tree, &primary_name),
            "no compiled {primary_name} entry under {}",
            tree.display()
        );
        assert!(tree.join(format!("{}.terminfo", crate::APP)).is_file());
        assert!(tree.join(format!("{}.termcap", crate::APP)).is_file());
        // The non-FreeBSD layout must NOT appear (this is the exact
        // generator/consumer contradiction W2-03 fixed).
        assert!(
            !share.join("terminfo").exists(),
            "freebsd target must not emit share/terminfo"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// W2-03: a Windows-target run emits ONLY the terminfo source (plus the
    /// share/{APP} root) — the oracle's `break :terminfo` before the
    /// tic/infotocap steps, whose binaries don't exist on Windows. The Windows
    /// runtime sentinel is exactly this source file
    /// (`terminfo/rustty.terminfo`).
    #[test]
    fn gen_terminfo_windows_emits_source_only() {
        let base = std::env::temp_dir().join(format!(
            "rustty-xtask-tinfo-win-test-{}",
            std::process::id()
        ));
        let share = base.join("share");
        std::fs::create_dir_all(&share).unwrap();

        gen_terminfo(&share, "windows", ALL).expect("gen_terminfo windows");

        let tree = share.join("terminfo");
        let src = tree.join(format!("{}.terminfo", crate::APP));
        assert!(src.is_file(), "windows target must emit the source");
        assert!(std::fs::metadata(&src).unwrap().len() > 0);
        // No termcap, no compiled subdirectories: the source file must be the
        // tree's ONLY entry.
        assert!(!tree.join(format!("{}.termcap", crate::APP)).exists());
        let entries: Vec<_> = std::fs::read_dir(&tree)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(
            entries,
            vec![std::ffi::OsString::from(format!("{}.terminfo", crate::APP))],
            "windows tree must contain only the source file"
        );
        assert!(share.join(crate::APP).is_dir());

        let _ = std::fs::remove_dir_all(&base);
    }

    /// End-to-end: gen_terminfo compiles a real terminfo tree with tic and creates
    /// the share/{APP} root. Verifies the runtime-critical resource actually
    /// materializes (not just that the pipeline returns Ok).
    #[test]
    fn gen_terminfo_produces_compiled_entry() {
        let base =
            std::env::temp_dir().join(format!("rustty-xtask-tinfo-test-{}", std::process::id()));
        let share = base.join("share");
        std::fs::create_dir_all(&share).unwrap();

        gen_terminfo(&share, std::env::consts::OS, ALL).expect("gen_terminfo");

        // Compiled terminfo entry exists under the active profile's primary
        // name (`xterm-rustty` by default, `xterm-ghostty` under
        // `identity-oracle` — see the module R2 identity note).
        let primary_name = rustty_terminfo::entry::rustty().names[0].clone();
        let primary_name = String::from_utf8(primary_name).unwrap();
        assert!(
            compiled_entry_exists(&share.join("terminfo"), &primary_name),
            "no compiled {primary_name} entry under {}",
            share.join("terminfo").display()
        );
        // Terminfo SOURCE + termcap emitted alongside the compiled DB (P1.3),
        // named after `crate::APP` (Task 14).
        let src = share.join(format!("terminfo/{}.terminfo", crate::APP));
        let cap = share.join(format!("terminfo/{}.termcap", crate::APP));
        assert!(
            src.is_file(),
            "no {}.terminfo source at {}",
            crate::APP,
            src.display()
        );
        assert!(
            cap.is_file(),
            "no {}.termcap at {}",
            crate::APP,
            cap.display()
        );
        assert!(
            std::fs::metadata(&src).unwrap().len() > 0,
            "{}.terminfo source is empty",
            crate::APP
        );
        assert!(
            std::fs::metadata(&cap).unwrap().len() > 0,
            "{}.termcap is empty",
            crate::APP
        );

        // share/{APP} root exists; the pre-WS1 DEFERRALS.md placeholder is GONE
        // (shell-integration + themes ship real vendored content now).
        assert!(share.join(crate::APP).is_dir());
        assert!(!share.join(crate::APP).join("DEFERRALS.md").exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    /// Regenerating over a stale output dir SCRUBS the pre-WS1 placeholder note
    /// (so it can never ship in the app bundle again).
    #[test]
    fn gen_app_root_scrubs_stale_deferrals_note() {
        let base =
            std::env::temp_dir().join(format!("rustty-xtask-approot-test-{}", std::process::id()));
        let share = base.join("share");
        let app_dir = share.join(crate::APP);
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::write(app_dir.join("DEFERRALS.md"), b"stale placeholder").unwrap();

        gen_app_root(&share).expect("gen_app_root");

        assert!(app_dir.is_dir());
        assert!(
            !app_dir.join("DEFERRALS.md").exists(),
            "stale note must be scrubbed"
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
