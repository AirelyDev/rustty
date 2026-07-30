//! xtask `resources` sub-generator: the remaining build-phase share/ folders
//! `share/{man,locale,bat}` plus `share/{APP}/{doc,webdata}` for the macOS
//! Swift-app Copy-Resources phase.
//!
//! PORTS (build-orchestration analog, R4 — independent of parity order):
//!   * `share/man` + `share/{APP}/doc` — ghostty `src/build/GhosttyDocs.zig`
//!     (the REAL generated path, not `installDummy`), driven by the ported
//!     `src/build/mdgen`.
//!   * `share/{APP}/webdata` — ghostty `src/build/GhosttyWebdata.zig`, driven by
//!     the ported `src/build/webgen`.
//!   * `share/locale` — ghostty `src/build/GhosttyI18n.zig`.
//!   * `share/bat`   — ghostty `src/build/GhosttyResources.zig` (sublime syntax
//!     install to `share/bat/syntaxes`).
//!
//! None of these is runtime-critical for a basic terminal window; the macOS
//! `Rustty.xcodeproj` Copy-Resources build phase references `../zig-out/share/
//! {man,locale,bat}` and only requires the folders to EXIST so the copy step
//! succeeds. Each folder is emitted REAL and VALID (R5): never faked content.
//!
//! DEFERRAL SUMMARY (durable, disjoint record — mirrored in
//! `.port/phase-2/DEFERRALS.swiftapp-misc.md`):
//!
//!  1. **man + doc + webdata** — the `src/build/mdgen/**` + `src/build/webgen/**`
//!     deferral is CLOSED (WS13, 2026-07-30). `gen_man` emits real
//!     `share/man/man1/{APP}.1` + `share/man/man5/{APP}.5`; `gen_doc` emits
//!     `share/{APP}/doc/{APP}.{1,5}.{md,html}`; `gen_webdata` emits
//!     `share/{APP}/webdata/{config,actions,commands}.mdx`. All of it is
//!     GENERATED from the live help corpora via `rustty_extra::mdgen` (the port
//!     of `src/build/mdgen`) and `rustty_extra::webgen` (the port of
//!     `src/build/webgen`), so the pages carry the full auto-generated
//!     `# COMMAND LINE ACTIONS` / `# CONFIGURATION OPTIONS` / `# KEYBIND ACTIONS`
//!     catalogs — one entry per CLI action, config field and keybind action.
//!
//!     What used to be here — and why it is gone: a hand-written
//!     `MAN1_BODY`/`MAN5_BODY` roff transcription that carried the prose but
//!     DEFERRED the three generated catalogs, telling readers to run
//!     `+show-config --default --docs` instead. Its recorded un-defer trigger
//!     was "add a man-page generator to `rustty-extra` … porting mdgen's three
//!     `gen*` walks, and call it here" — which is exactly what WS13 T6/T7/T8
//!     did. (Two claims in the old text were also stale by the time it was
//!     read: the pages had not pointed at `ghostty +show-config` since the
//!     rebrand, and `rustty-extra` had long since become an xtask dependency.)
//!
//!     The reference converts mdgen's markdown with `pandoc --to man` /
//!     `--to html` (`GhosttyDocs.zig:60-88`). pandoc is installed on NEITHER
//!     build host, so the conversion is done by `rustty_extra::mdgen::render`,
//!     a Rust markdown→roff/HTML renderer covering the closed subset mdgen
//!     produces. See that module's header for the full tradeoff argument
//!     (requiring pandoc was rejected on the same grounds WS9 rejected linking
//!     Homebrew's libintl: do not add a host dependency a stock machine lacks).
//!
//!     Two intentional divergences from the reference, both recorded on
//!     `gen_man`: rustty always emits REAL pages rather than
//!     `GhosttyDocs.installDummy`'s `share/man/.placeholder` (the macOS
//!     Copy-Resources phase ships `share/man` unconditionally), and the `.TH`
//!     title now follows [`crate::APP`] so `--features identity-oracle`
//!     produces `.TH "GHOSTTY" "1"` to match the `ghostty.1` filename — before
//!     WS13 the filename flipped but the title stayed `RUSTTY`.
//!
//!  2. **locale** — REAL compiled gettext catalogs (un-deferred: the `po/*.po`
//!     corpus now exists and is `msgfmt`-clean). Mirroring ghostty
//!     (`GhosttyI18n.zig` `init`: `msgfmt -o - po/<locale>.po` captured to
//!     `share/locale/<locale>/LC_MESSAGES/<domain>.mo`), we compile every
//!     `po/<locale>.po` into
//!     `share/locale/<locale>/LC_MESSAGES/io.airely.rustty.mo` via the system
//!     `msgfmt`. The `.mo` domain filename is [`DOMAIN`] (= the app BUNDLE_ID,
//!     `io.airely.rustty`); `<locale>` is the `.po` basename (`de`, `ja`,
//!     `zh_CN`, …). The `.pot` template and any `README*.md` in `po/` are
//!     skipped. Kept `std`-only (shells out to `msgfmt`, like the other
//!     generators here) so `gen_misc` compiles regardless of concurrent
//!     `xtask/Cargo.toml` edits.
//!
//!  3. **bat** — the sublime-syntax generator IS ported
//!     (`rustty_extra::sublime::syntax() -> Vec<u8>`, golden-tested byte-exact
//!     against the committed `rustty.sublime-syntax` fixture). We write REAL
//!     content: the committed, golden byte-for-byte source of that generator,
//!     embedded via `include_bytes!` from `crates/rustty-extra/testdata/`. This
//!     keeps `gen_misc` self-contained (pure `std`, no xtask→rustty-extra Cargo
//!     dep) so it compiles regardless of the concurrent Cargo.toml edits made
//!     by the sibling resource-generator agents.
//!     Optional follow-up (not a deferral of content — content is real & valid):
//!     the land agent MAY switch this to a live `rustty_extra::sublime::syntax()`
//!     call once `rustty-extra` is a wired xtask dependency; the emitted bytes
//!     are identical either way (guaranteed by `sublime::syntax`'s golden test).
//!     R2 IDENTITY NOTE (Task 16): the emitted syntax file + its install
//!     filename now carry the rustty identity (`rustty.sublime-syntax`,
//!     `file_extensions: - rustty`, `scope: source.rustty`) matching
//!     `config.rustty` — previously this hardcoded `ghostty`, which meant
//!     bat's `file_extensions:` match never fired for the actual shipped
//!     config file (bat matches by the file's own YAML fields, not by the
//!     syntax file's basename). No `identity-oracle` dual-profile exists for
//!     this content (single rustty-branded generator output, matching
//!     `rustty_extra::vim`'s Task 18 precedent).

use std::fs;
use std::path::Path;
use std::process::Command;

/// gettext domain filename for the compiled `.mo` catalogs — matches the app
/// BUNDLE_ID (`io.airely.rustty`, via `rustty-build-config`). Installed as
/// `share/locale/<locale>/LC_MESSAGES/{DOMAIN}.mo`, which is where the runtime
/// `bindtextdomain(BUNDLE_ID, <share>/locale)` looks. ghostty's analog is
/// `com.mitchellh.ghostty` (`GhosttyI18n.zig`).
const DOMAIN: &str = "io.airely.rustty";

/// The real rustty sublime-syntax bytes: byte-identical to what
/// `rustty_extra::sublime::syntax()` emits (that generator's `syntax_matches_golden`
/// test pins it to exactly this file). Embedding the committed source keeps this
/// generator self-contained (no Cargo dep) while shipping authentic content.
const SUBLIME_SYNTAX: &[u8] =
    include_bytes!("../../crates/rustty-extra/testdata/rustty.sublime-syntax");

/// Generate the remaining build-phase resource folders under `share_dir`:
/// `man/`, `locale/`, and `bat/`.
///
/// `share_dir` is the `…/zig-out/share` root the `resources` subcommand creates.
/// Returns `Ok(())` after all three subtrees exist and are populated
/// minimal-but-valid (R5). Any I/O failure is surfaced as a `String` error
/// naming the offending path.
pub fn gen_misc(share_dir: &Path) -> Result<(), String> {
    gen_man(share_dir)?;
    gen_doc(share_dir)?;
    gen_webdata(share_dir)?;
    gen_locale(share_dir)?;
    gen_bat(share_dir)?;
    Ok(())
}

/// `share/man` — the port of `GhosttyDocs.zig:20-88`'s real (non-`installDummy`)
/// path: `man1/{APP}.1` + `man5/{APP}.5`, where `{APP}` = [`crate::APP`].
///
/// The markdown comes from [`rustty_extra::mdgen`] (the port of
/// `src/build/mdgen`, including the auto-generated `COMMAND LINE ACTIONS` /
/// `CONFIGURATION OPTIONS` / `KEYBIND ACTIONS` catalogs), and the roff comes
/// from [`rustty_extra::mdgen::render`], which stands in for the reference's
/// `pandoc --to man` — pandoc exists on neither build host; see that module's
/// header for the tradeoff argument.
///
/// Two intentional divergences from the reference:
///   * rustty ALWAYS emits real pages, never `GhosttyDocs.installDummy`'s
///     `share/man/.placeholder`, because the macOS Copy-Resources phase ships
///     `share/man` unconditionally.
///   * the `.TH` title is derived from [`crate::APP`] rather than hardcoded, so
///     `--features identity-oracle` produces `.TH "GHOSTTY" "1"` alongside the
///     `ghostty.1` FILENAME. Before WS13 the title said `RUSTTY` in both
///     profiles while the filename flipped — a real, if cosmetic, profile bug.
///
/// Verify with `man -M <share>/man <app>` / `man -M <share>/man 5 <app>`.
fn gen_man(share_dir: &Path) -> Result<(), String> {
    let app = crate::APP;
    let title = app.to_uppercase();

    let man1_dir = share_dir.join("man").join("man1");
    mkdirp(&man1_dir)?;
    let page1 = rustty_extra::mdgen::render::markdown_to_roff_with_title(
        &rustty_extra::mdgen::man_1_markdown(),
        Some(title.as_bytes()),
    );
    write_file(&man1_dir.join(format!("{app}.1")), &page1)?;

    let man5_dir = share_dir.join("man").join("man5");
    mkdirp(&man5_dir)?;
    let page5 = rustty_extra::mdgen::render::markdown_to_roff_with_title(
        &rustty_extra::mdgen::man_5_markdown(),
        Some(title.as_bytes()),
    );
    write_file(&man5_dir.join(format!("{app}.5")), &page5)
}

/// `share/{APP}/doc` — the markdown and HTML halves of `GhosttyDocs.zig:60-88`
/// (`pandoc --to html` plus the `.md` source it installs alongside).
fn gen_doc(share_dir: &Path) -> Result<(), String> {
    let app = crate::APP;
    let doc_dir = share_dir.join(app).join("doc");
    mkdirp(&doc_dir)?;

    for (section, md) in [
        (1u8, rustty_extra::mdgen::man_1_markdown()),
        (5u8, rustty_extra::mdgen::man_5_markdown()),
    ] {
        write_file(&doc_dir.join(format!("{app}.{section}.md")), &md)?;
        write_file(
            &doc_dir.join(format!("{app}.{section}.html")),
            &rustty_extra::mdgen::render::markdown_to_html(&md),
        )?;
    }
    Ok(())
}

/// `share/{APP}/webdata` — the port of `GhosttyWebdata.zig:44-48,74-79,113-116`:
/// the three `.mdx` documents the reference's `src/build/webgen/main_*.zig`
/// executables emit, generated here by [`rustty_extra::webgen`].
fn gen_webdata(share_dir: &Path) -> Result<(), String> {
    let webdata_dir = share_dir.join(crate::APP).join("webdata");
    mkdirp(&webdata_dir)?;
    write_file(
        &webdata_dir.join("config.mdx"),
        &rustty_extra::webgen::config_mdx(),
    )?;
    write_file(
        &webdata_dir.join("actions.mdx"),
        &rustty_extra::webgen::actions_mdx(),
    )?;
    write_file(
        &webdata_dir.join("commands.mdx"),
        &rustty_extra::webgen::commands_mdx(),
    )
}

/// `share/locale` — compile every `po/<locale>.po` gettext catalog into
/// `share/locale/<locale>/LC_MESSAGES/{DOMAIN}.mo` via the system `msgfmt`,
/// mirroring ghostty's install step (`GhosttyI18n.zig` `init`). `<locale>` is
/// the `.po` basename; the `.pot` template and any `README*.md` are skipped.
/// See the module DEFERRAL SUMMARY (item 2) for the un-defer rationale.
fn gen_locale(share_dir: &Path) -> Result<(), String> {
    let locale_dir = share_dir.join("locale");
    mkdirp(&locale_dir)?;

    let po_dir = crate::workspace_root().join("po");
    let entries =
        fs::read_dir(&po_dir).map_err(|e| format!("read_dir {}: {e}", po_dir.display()))?;

    // Collect the `.po` catalogs (skip the `.pot` template + `README*.md`),
    // sorted for deterministic output.
    let mut po_files: Vec<std::path::PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("po"))
        .collect();
    po_files.sort();

    for po in &po_files {
        let locale = po
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("non-UTF-8 po basename: {}", po.display()))?;

        // share/locale/<locale>/LC_MESSAGES/io.airely.rustty.mo
        let lc_dir = locale_dir.join(locale).join("LC_MESSAGES");
        mkdirp(&lc_dir)?;
        let mo = lc_dir.join(format!("{DOMAIN}.mo"));

        // ghostty: `msgfmt -o - po/<locale>.po` captured to the `.mo`. We write
        // straight to the destination with `-o <mo>` (functionally identical).
        let status = Command::new("msgfmt")
            .env("PATH", msgfmt_path())
            .arg("-o")
            .arg(&mo)
            .arg(po)
            .status()
            .map_err(|e| {
                format!("spawn msgfmt (is gettext on PATH? try /opt/homebrew/opt/gettext/bin): {e}")
            })?;
        if !status.success() {
            return Err(format!(
                "msgfmt failed ({status}) compiling {} -> {}",
                po.display(),
                mo.display()
            ));
        }
    }

    Ok(())
}

/// `PATH` for spawning `msgfmt`: the inherited `PATH` with Homebrew's keg-only
/// gettext `bin` prepended when present, so `cargo run -p xtask -- resources`
/// resolves `msgfmt` on macOS without the caller pre-adjusting `PATH`.
fn msgfmt_path() -> String {
    let inherited = std::env::var("PATH").unwrap_or_default();
    const BREW_GETTEXT: &str = "/opt/homebrew/opt/gettext/bin";
    if Path::new(BREW_GETTEXT).is_dir() && !inherited.split(':').any(|p| p == BREW_GETTEXT) {
        if inherited.is_empty() {
            BREW_GETTEXT.to_string()
        } else {
            format!("{BREW_GETTEXT}:{inherited}")
        }
    } else {
        inherited
    }
}

/// `share/bat/syntaxes/rustty.sublime-syntax` — REAL content: the golden,
/// byte-exact source of the ported `rustty_extra::sublime::syntax()` generator.
/// Mirrors ghostty's install layout (`share/bat/syntaxes`, GhosttyResources.zig),
/// but carries the rustty product identity (Task 16 — the config file is
/// `config.rustty`, so both the filename and the YAML `file_extensions:`/
/// `scope:` fields inside must say `rustty` for bat to actually highlight it).
fn gen_bat(share_dir: &Path) -> Result<(), String> {
    let syntaxes_dir = share_dir.join("bat").join("syntaxes");
    mkdirp(&syntaxes_dir)?;
    write_file(&syntaxes_dir.join("rustty.sublime-syntax"), SUBLIME_SYNTAX)
}

fn mkdirp(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("create_dir_all {}: {e}", dir.display()))
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| format!("write {}: {e}", path.display()))
}
