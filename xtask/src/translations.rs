//! xtask translations — the `update-translations` subcommand.
//!
//! PORTS: the `createUpdateStep` half of ghostty's
//! `src/build/GhosttyI18n.zig`. That build step regenerates the gettext
//! template (`po/<domain>.pot`) from the current GTK-apprt source tree and the
//! Nautilus integration script, then `msgmerge`s every locale catalog in `po/`
//! against the fresh template so translators pick up added/removed strings.
//!
//! Retargeted for rustty identity (R2):
//!   * package-name        `com.mitchellh.ghostty`   -> `io.airely.rustty`
//!   * msgid-bugs-address  `m@mitchellh.com`         -> `gael@airely.io`
//!   * template output     `po/com.mitchellh.ghostty.pot` -> `po/io.airely.rustty.pot`
//!   * Nautilus script     `dist/linux/ghostty_nautilus.py` -> `dist/linux/rustty_nautilus.py`
//!   * GTK sources: ghostty walks the hardcoded `.blp` blueprint list plus
//!     every `.zig` under `src/apprt/gtk` (`GhosttyI18n.zig:80-141`). rustty's
//!     equivalents are `crates/rustty-apprt/assets/ui/**/*.blp` and
//!     `crates/rustty-apprt/src/gtk/**/*.rs`; the walk is scoped to exactly those
//!     two subtrees (see `collect_gtk_sources`).
//!
//! ── REGENERATION IS LIVE (WS13, 2026-07-29) ──────────────────────────────
//! The GTK apprt is no longer a placeholder: 21 `.blp` templates carry 51 unique
//! msgids and the code-side sites go through `rustty_os::i18n::gettext_` (via the
//! `crate::gtk::i18n::tr` wrapper). The old header warned that running this
//! command would gut the catalogs because extraction found nothing; that is no
//! longer true, and the guard is now mechanical rather than advisory:
//!
//!   * `crates/rustty-apprt/tests/i18n_msgid_coverage.rs` asserts every runtime
//!     msgid exists in the `.pot` and every oracle code-side catalog entry is
//!     actually looked up somewhere. Run it after any regeneration.
//!   * After running this command, `msgcmp po/<l>.po po/io.airely.rustty.pot`
//!     must be silent for all 32 locales and `msgattrib --only-obsolete` must
//!     produce an empty body — i.e. no translation was orphaned.
//!
//! Rust-side keywords: `_` is not a legal Rust fn name, so `os/i18n.zig`'s `_()`
//! is ported as `rustty_os::i18n::gettext_()` and the GTK apprt calls it through
//! the `&str`-returning wrapper `crate::gtk::i18n::tr()`. BOTH names must be
//! passed as `--keyword`s; without them every Rust call site is invisible to
//! extraction (measured: 0 `.rs` references before, 26 after).
//!
//! ── TOOLS ────────────────────────────────────────────────────────────────
//! `xgettext` and `msgmerge` are resolved from `PATH`. On macOS the Homebrew
//! gettext keg is keg-only, so prepend it first:
//!     export PATH="/opt/homebrew/opt/gettext/bin:$PATH"

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::workspace_root;

/// gettext domain == rustty bundle id (matches `po/io.airely.rustty.pot` and
/// the compiled `io.airely.rustty.mo` filename).
const DOMAIN: &str = "io.airely.rustty";
const MSGID_BUGS_ADDRESS: &str = "gael@airely.io";
const COPYRIGHT_HOLDER: &str = "Mitchell Hashimoto, Ghostty contributors, Rustty contributors";

/// The DECLARED locale set: the catalogs to merge on `update-translations`
/// AND the exact set of catalogs the `resources` build compiles and installs
/// (see [`crate::res_misc`]'s `gen_locale`). Mirrors `rustty-os`'s
/// `i18n_locales::locales` (the reference `GhosttyI18n.zig` imports that same
/// `locales` array and drives its `msgfmt`/install steps from it — declared-set
/// driven, NOT `po/` directory driven: a declared locale whose `.po` is missing
/// fails the build, and an undeclared extra `.po` is never installed).
///
/// Kept in lockstep with the `po/` directory; the `po_dir_matches_declared_locales`
/// test in [`crate::res_misc`] is the tripwire for drift.
pub(crate) const LOCALES: &[&str] = &[
    "zh_CN", "de", "fr", "ja", "nl", "nb", "ru", "uk", "pl", "ko_KR", "mk", "tr", "id", "es_BO",
    "es_AR", "es_ES", "pt_BR", "ca", "it", "bg", "ga", "hu", "he", "zh_TW", "hr", "lt", "lv", "vi",
    "kk", "be", "eu", "da", "sr", "sr@latin",
];

/// Recursively collect GTK-apprt source files (`.blp` + `.rs`) as paths
/// RELATIVE to the workspace root, sorted for determinism.
///
/// Faithful to upstream's rationale: the arguments handed to `xgettext` must be
/// repo-relative (not absolute) or the generated `.pot` embeds each developer's
/// checkout path and churns on every commit. Sorting mirrors ghostty's explicit
/// sort of the directory walk (walk order is not guaranteed).
fn collect_gtk_sources(root: &Path) -> Result<Vec<PathBuf>, String> {
    // The reference walks `src/apprt/gtk/**.zig` plus its hardcoded `.blp` list
    // (`GhosttyI18n.zig:80-141`). The port's equivalents are
    // `crates/rustty-apprt/src/gtk/**/*.rs` and `crates/rustty-apprt/assets/ui/**/*.blp`.
    // Scoping to these two subtrees (rather than the whole crate) keeps
    // `embedded.rs`, the C-ABI surface, and the winproto arms — none of which the
    // reference scans — out of the extraction, and roughly halves the xgettext
    // C-lexer noise (Rust lifetimes read as unterminated char constants).
    let apprt = root.join("crates/rustty-apprt");
    let mut out: Vec<PathBuf> = Vec::new();
    for sub in ["src/gtk", "assets/ui"] {
        let dir = apprt.join(sub);
        if dir.is_dir() {
            collect_recursive(&dir, &mut out)?;
        }
    }
    // Exclude the file that DEFINES the `tr` wrapper. xgettext's C lexer reads a
    // Rust lifetime (`&'static`) as an unterminated character constant, so at the
    // *definition* `pub fn tr(msgid: &'static CStr)` the lexer desynchronizes and
    // binds the next string literal it sees in the file as `tr`'s first argument —
    // extracting a msgid that no runtime call site ever looks up (measured: the
    // wrapper's own test probe leaked into the `.pot`). Real call sites pass the
    // literal directly, so they are unaffected. This file contains no runtime
    // msgid, so nothing is lost by skipping it.
    let wrapper_def = apprt.join("src/gtk/i18n.rs");
    out.retain(|p| p != &wrapper_def);
    // Make relative to the workspace root.
    let mut rel: Vec<PathBuf> = out
        .iter()
        .map(|p| {
            p.strip_prefix(root)
                .map(|r| r.to_path_buf())
                .unwrap_or_else(|_| p.clone())
        })
        .collect();
    rel.sort();
    Ok(rel)
}

fn collect_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("dir entry in {}: {e}", dir.display()))?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|e| format!("file_type {}: {e}", path.display()))?;
        if kind.is_dir() {
            // Skip build output dirs.
            if path.file_name().and_then(|s| s.to_str()) == Some("target") {
                continue;
            }
            collect_recursive(&path, out)?;
        } else if kind.is_file() {
            match path.extension().and_then(|s| s.to_str()) {
                Some("blp") | Some("rs") => out.push(path),
                _ => {}
            }
        }
    }
    Ok(())
}

/// Write a minimal, valid empty gettext template (header-only). Used when the
/// Nautilus script is absent, so the merge step still has a well-formed
/// `py.pot` to consume (rather than erroring on "no input file"). The GTK leg
/// never needs it: the command-palette source is always one of its inputs.
fn write_empty_pot(path: &Path) -> Result<(), String> {
    let body = "\
# SOME DESCRIPTIVE TITLE.\n\
msgid \"\"\n\
msgstr \"\"\n\
\"MIME-Version: 1.0\\n\"\n\
\"Content-Type: text/plain; charset=UTF-8\\n\"\n\
\"Content-Transfer-Encoding: 8bit\\n\"\n";
    std::fs::write(path, body).map_err(|e| format!("write empty pot {}: {e}", path.display()))
}

/// "For localization of command palette" (`GhosttyI18n.zig:152-155`): the
/// reference's `src/input/command.zig`, as a repo-relative path so the `.pot`
/// carries no checkout-specific prefix.
const COMMAND_PALETTE_PATH: &str = "crates/rustty-input/src/command.rs";

/// The GTK leg's `xgettext --language=C` invocation (`GhosttyI18n.zig:63-155`),
/// writing `gtk_pot` from the repo-relative `gtk_sources`. Run it with the
/// workspace root as the current directory.
fn gtk_xgettext_command(gtk_pot: &Path, gtk_sources: &[PathBuf]) -> Command {
    // --language=C silences xgettext's "unknown extension" errors on .blp/.rs;
    // --keyword=_ / --keyword=C_:1c,2 match rustty-os's `_()` / `C_()` gettext
    // wrappers (`C_` = pgettext: arg 1 is the context `c`, arg 2 the msgid).
    //
    // --force-po is a rustty adaptation NOT in upstream: with the placeholder
    // GTK apprt the scanned .rs files contain zero recognized `_()` call sites,
    // and without --force-po xgettext writes NO output file when it extracts no
    // messages — which would then break the merge step below on a missing
    // gtk.pot. --force-po forces a valid header-only .pot to always be written.
    // (Upstream never hits this because ghostty's live .blp/.zig always yield
    // messages.) It is harmless once real strings exist.
    let mut xg = Command::new("xgettext");
    xg.arg("--language=C")
        .arg("--from-code=UTF-8")
        .arg("--force-po")
        .arg("--keyword=_")
        // `--keyword=N_` (`GhosttyI18n.zig:69`) marks the command-palette
        // titles and descriptions: `command.rs` wraps each in `i18n::N_(b"…")`,
        // and the C lexer reads the `b` prefix as an identifier and still
        // extracts the literal (measured: 180 msgids).
        .arg("--keyword=N_")
        // rustty-specific: `_` is not a legal Rust fn name, so `os/i18n.zig`'s
        // `_()` is ported as `rustty_os::i18n::gettext_()`, called from the GTK
        // apprt through the `&str`-returning wrapper `crate::gtk::i18n::tr()`.
        // xgettext's C lexer tokenizes `i18n::gettext_(c"…")` / `tr(c"…")` such
        // that these keywords match (the `c"…"` literal prefix is harmless to
        // the C lexer). WITHOUT them every Rust call site is invisible to
        // extraction — measured: 0 `.rs:` references before, 26 after.
        .arg("--keyword=gettext_")
        .arg("--keyword=tr")
        .arg("--keyword=C_:1c,2")
        .arg("-o")
        .arg(gtk_pot);
    for src in gtk_sources {
        // Repo-relative paths (run from `root`) keep the .pot free of
        // absolute checkout paths.
        xg.arg(src);
    }
    // For localization of command palette
    xg.arg(COMMAND_PALETTE_PATH);
    xg
}

/// Port of `GhosttyI18n.createUpdateStep`: regenerate `po/<DOMAIN>.pot` from
/// the GTK-apprt sources + the Nautilus Python script, then `msgmerge` every
/// locale catalog against it.
///
/// SEE the module-level "DEFERRED-REGENERATION REALITY" note: while the GTK UI
/// is a placeholder this will gut the ported catalogs. Returns the path to the
/// regenerated template on success.
pub fn update_translations() -> Result<PathBuf, String> {
    let root = workspace_root();
    let po_dir = root.join("po");
    if !po_dir.is_dir() {
        return Err(format!("po/ dir not found at {}", po_dir.display()));
    }

    // Intermediate .pot files live in a throwaway temp dir; only the final
    // merged template + the per-locale catalogs are written into po/.
    let tmp = std::env::temp_dir().join(format!("rustty-i18n-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("create temp dir: {e}"))?;
    let gtk_pot = tmp.join("gtk.pot");
    let py_pot = tmp.join("py.pot");

    // ── (1) GTK leg: xgettext --language=C over the .blp + .rs sources. ──
    // The flags and their rationale are at `gtk_xgettext_command`.
    let gtk_sources = collect_gtk_sources(&root)?;
    run(
        gtk_xgettext_command(&gtk_pot, &gtk_sources).current_dir(&root),
        "xgettext (gtk)",
    )?;

    // ── (2) Python leg: xgettext --language=Python over the Nautilus script. ──
    // --force-po for the same robustness reason as the GTK leg above.
    let nautilus_rel = PathBuf::from("dist/linux/rustty_nautilus.py");
    if root.join(&nautilus_rel).is_file() {
        let mut xg = Command::new("xgettext");
        xg.arg("--language=Python")
            .arg("--from-code=UTF-8")
            .arg("--force-po")
            .arg("-o")
            .arg(&py_pot)
            .arg(&nautilus_rel);
        run(xg.current_dir(&root), "xgettext (python)")?;
    } else {
        write_empty_pot(&py_pot)?;
    }

    // ── (3) Merge py.pot + gtk.pot into the final template. ──
    //
    // py.pot MUST come first on the merge command line: xgettext derives the
    // merged charset from the first input, and the Python catalog is reliably
    // UTF-8 (upstream comment). Output is captured from stdout (`-o -`) and
    // written to po/<DOMAIN>.pot.
    let pot_out = po_dir.join(format!("{DOMAIN}.pot"));
    let mut merge = Command::new("xgettext");
    merge
        .arg("--add-comments=Translators")
        .arg(format!("--package-name={DOMAIN}"))
        .arg(format!("--msgid-bugs-address={MSGID_BUGS_ADDRESS}"))
        .arg(format!("--copyright-holder={COPYRIGHT_HOLDER}"))
        .arg("-o")
        .arg("-")
        .arg(&py_pot)
        .arg(&gtk_pot);
    let merged = capture(merge.current_dir(&root), "xgettext (merge)")?;
    std::fs::write(&pot_out, &merged).map_err(|e| format!("write {}: {e}", pot_out.display()))?;

    // ── (4) msgmerge every locale catalog against the fresh template. ──
    //
    // --quiet silences per-file stats; --no-fuzzy-matching keeps msgmerge from
    // inventing fuzzy guesses (matches upstream). Output captured from stdout
    // and written back over the catalog.
    for locale in LOCALES {
        let po = po_dir.join(format!("{locale}.po"));
        if !po.is_file() {
            return Err(format!("missing catalog {}", po.display()));
        }
        let mut mm = Command::new("msgmerge");
        mm.arg("--quiet")
            .arg("--no-fuzzy-matching")
            .arg(&po)
            .arg(&pot_out);
        let merged = capture(mm.current_dir(&root), &format!("msgmerge {locale}"))?;
        std::fs::write(&po, &merged).map_err(|e| format!("write {}: {e}", po.display()))?;
    }

    let _ = std::fs::remove_dir_all(&tmp);
    Ok(pot_out)
}

/// Run a command to completion, mapping spawn/exit failures to a `String`
/// error. The macOS PATH hint points at the keg-only Homebrew gettext.
fn run(cmd: &mut Command, what: &str) -> Result<(), String> {
    let status = cmd.status().map_err(|e| {
        format!("spawn {what}: {e} (on macOS: export PATH=\"/opt/homebrew/opt/gettext/bin:$PATH\")")
    })?;
    if !status.success() {
        return Err(format!("{what} exited with {status}"));
    }
    Ok(())
}

/// Run a command and capture stdout as bytes (the gettext `-o -` idiom), so we
/// never lose non-UTF-8 catalog bytes to a lossy String conversion.
fn capture(cmd: &mut Command, what: &str) -> Result<Vec<u8>, String> {
    let output = cmd.output().map_err(|e| {
        format!("spawn {what}: {e} (on macOS: export PATH=\"/opt/homebrew/opt/gettext/bin:$PATH\")")
    })?;
    if !output.status.success() {
        return Err(format!(
            "{what} exited with {} — {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The R14 strict-mode pivot for `xgettext`: a no-op by default, a panic
    /// under `RUSTTY_REQUIRE_XGETTEXT=1`, so a CI leg that lost the tool fails
    /// instead of reporting the test `ok`.
    #[track_caller]
    fn require_xgettext_or_skip(test_name: &str, err: &dyn std::fmt::Display) {
        require_xgettext_or_skip_impl(
            std::env::var_os("RUSTTY_REQUIRE_XGETTEXT").as_deref() == Some("1".as_ref()),
            test_name,
            err,
        );
    }

    #[track_caller]
    fn require_xgettext_or_skip_impl(required: bool, test_name: &str, err: &dyn std::fmt::Display) {
        assert!(
            !required,
            "xgettext required but unavailable (RUSTTY_REQUIRE_XGETTEXT=1): \
             {test_name} would have skipped: {err}"
        );
    }

    /// The pivot itself: a no-op when not required, a panic when required.
    #[test]
    fn xgettext_pivot_panics_only_when_required() {
        require_xgettext_or_skip_impl(false, "t", &"missing");
        let r = std::panic::catch_unwind(|| require_xgettext_or_skip_impl(true, "t", &"missing"));
        assert!(r.is_err());
    }

    /// `GhosttyI18n.zig:63-71` passes `--keyword=N_` and `:152-155` adds the
    /// command-palette source (`src/input/command.zig`, ported as
    /// `crates/rustty-input/src/command.rs`) to the GTK leg. S63-F1.
    #[test]
    fn gtk_xgettext_carries_the_n_keyword_and_the_command_palette() {
        let cmd = gtk_xgettext_command(Path::new("gtk.pot"), &[PathBuf::from("a.blp")]);
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        assert!(args.contains(&"--keyword=N_"), "{args:?}");
        assert_eq!(
            args.last(),
            Some(&COMMAND_PALETTE_PATH),
            "the command palette is the last input, after the GTK sources: {args:?}"
        );
        assert!(args.contains(&"a.blp"), "{args:?}");
    }

    /// Runs the real GTK-leg command over NO GTK sources, so every msgid in
    /// the output comes from the command-palette input and the `N_` keyword.
    /// 180 is the command-palette block the committed `.pot` carries
    /// (`crates/rustty-apprt/tests/i18n_msgid_coverage.rs` pins the same 180).
    /// Needs `xgettext` on `PATH`; without it the test prints that it was
    /// skipped, and under `RUSTTY_REQUIRE_XGETTEXT=1` (set by CI, whose legs
    /// install gettext for G-I18N) it fails instead (R14).
    #[test]
    fn gtk_xgettext_extracts_the_command_palette_msgids() {
        if let Err(e) = Command::new("xgettext").arg("--version").output() {
            require_xgettext_or_skip("gtk_xgettext_extracts_the_command_palette_msgids", &e);
            eprintln!("skipping gtk_xgettext_extracts_the_command_palette_msgids: xgettext is not on PATH ({e})");
            return;
        }
        let dir = std::env::temp_dir().join(format!("xtask-i18n-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pot = dir.join("gtk.pot");
        let status = gtk_xgettext_command(&pot, &[])
            .current_dir(workspace_root())
            .status()
            .unwrap();
        assert!(status.success());
        let body = std::fs::read_to_string(&pot).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        // Every `msgid` entry except the header's. (A wrapped long msgid also
        // starts `msgid ""`, so the header is subtracted, not filtered.)
        let msgids = body.lines().filter(|l| l.starts_with("msgid ")).count() - 1;
        assert_eq!(msgids, 180, "{body}");
        assert!(
            body.contains("#: crates/rustty-input/src/command.rs:")
                && body.contains("msgid \"Reset Terminal\""),
            "{body}"
        );
    }
}
