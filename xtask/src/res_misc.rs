//! xtask `resources` sub-generator: the remaining build-phase share/ folders
//! `share/{man,locale,bat}` for the macOS Swift-app Copy-Resources phase.
//!
//! PORTS (build-orchestration analog, R4 — independent of parity order):
//!   * `share/man`   — ghostty `src/build/GhosttyDocs.zig` (`installDummy` path).
//!   * `share/locale` — ghostty `src/build/GhosttyI18n.zig`.
//!   * `share/bat`   — ghostty `src/build/GhosttyResources.zig` (sublime syntax
//!                     install to `share/bat/syntaxes`).
//!
//! These three are NOT runtime-critical for a basic terminal window; the macOS
//! `Ghostty.xcodeproj` Copy-Resources build phase references `../zig-out/share/
//! {man,locale,bat}` and only requires the folders to EXIST so the copy step
//! succeeds. Each folder is emitted minimal-but-VALID (R5): never faked content.
//!
//! DEFERRAL SUMMARY (durable, disjoint record — mirrored in
//! `.port/phase-2/DEFERRALS.swiftapp-misc.md`):
//!
//!  1. **man** — real man-page generation (ghostty `src/build/mdgen/mdgen.zig`,
//!     markdown → `man` via pandoc, installed to `share/man/man{1,5}/…`) is NOT
//!     ported. `rustty-helpgen` only ports the doc-comment *extraction*
//!     primitives (`extractDocComments`/`findCommonPrefix`), not the mdgen
//!     markdown→roff pipeline. We faithfully replicate ghostty's OWN macOS
//!     fallback: `GhosttyDocs.installDummy` writes `share/man/.placeholder`
//!     with the exact bytes `"emit-docs not true so no man pages"` when
//!     `emit-docs` is not set (the macOS-app path — build.zig:88-96). This is
//!     byte-faithful to the reference, not a fabrication.
//!     Un-defer trigger: porting `mdgen` (+ the `ghostty_1`/`ghostty_5` markdown
//!     headers/footers) → real `share/man/man{1,5}` pages.
//!
//!  2. **locale** — rustty i18n is a deferred BREAK. ghostty compiles gettext
//!     `.po` → `.mo` into `share/locale/<lang>/LC_MESSAGES/com.mitchellh.ghostty.mo`
//!     (`GhosttyI18n.zig`); rustty has no `.po` corpus / msgfmt pipeline ported
//!     yet. We emit a minimal-valid `share/locale/.placeholder` so the folder
//!     exists for Copy-Resources. No content is faked (no fake `.mo`).
//!     Un-defer trigger: porting the i18n `.po` corpus + `msgfmt` compile step.
//!
//!  3. **bat** — the sublime-syntax generator IS ported
//!     (`rustty_extra::sublime::syntax() -> Vec<u8>`, golden-tested byte-exact
//!     against ghostty's `ghostty.sublime-syntax`). We write REAL content:
//!     the committed, golden byte-for-byte source of that generator, embedded
//!     via `include_bytes!` from `crates/rustty-extra/testdata/`. This keeps
//!     `gen_misc` self-contained (pure `std`, no xtask→rustty-extra Cargo dep)
//!     so it compiles regardless of the concurrent Cargo.toml edits made by the
//!     sibling resource-generator agents.
//!     Optional follow-up (not a deferral of content — content is real & valid):
//!     the land agent MAY switch this to a live `rustty_extra::sublime::syntax()`
//!     call once `rustty-extra` is a wired xtask dependency; the emitted bytes
//!     are identical either way (guaranteed by `sublime::syntax`'s golden test).

use std::fs;
use std::path::Path;

/// The real ghostty sublime-syntax bytes: byte-identical to what
/// `rustty_extra::sublime::syntax()` emits (that generator's `syntax_matches_golden`
/// test pins it to exactly this file). Embedding the committed source keeps this
/// generator self-contained (no Cargo dep) while shipping authentic content.
const SUBLIME_SYNTAX: &[u8] =
    include_bytes!("../../crates/rustty-extra/testdata/ghostty.sublime-syntax");

/// Faithful copy of ghostty `GhosttyDocs.installDummy`'s placeholder bytes
/// (`src/build/GhosttyDocs.zig`), emitted on the macOS-app path when real
/// man pages are not generated.
const MAN_PLACEHOLDER: &[u8] = b"emit-docs not true so no man pages";

/// Generate the remaining build-phase resource folders under `share_dir`:
/// `man/`, `locale/`, and `bat/`.
///
/// `share_dir` is the `…/zig-out/share` root the `resources` subcommand creates.
/// Returns `Ok(())` after all three subtrees exist and are populated
/// minimal-but-valid (R5). Any I/O failure is surfaced as a `String` error
/// naming the offending path.
pub fn gen_misc(share_dir: &Path) -> Result<(), String> {
    gen_man(share_dir)?;
    gen_locale(share_dir)?;
    gen_bat(share_dir)?;
    Ok(())
}

/// `share/man` — replicate ghostty's macOS `installDummy` placeholder faithfully
/// (real mdgen man-page generation is deferred; see the module DEFERRAL SUMMARY).
fn gen_man(share_dir: &Path) -> Result<(), String> {
    let man_dir = share_dir.join("man");
    mkdirp(&man_dir)?;
    write_file(&man_dir.join(".placeholder"), MAN_PLACEHOLDER)
}

/// `share/locale` — minimal-valid folder only; rustty i18n is a deferred BREAK,
/// so NO `.mo` content is emitted (never faked). See the module DEFERRAL SUMMARY.
fn gen_locale(share_dir: &Path) -> Result<(), String> {
    let locale_dir = share_dir.join("locale");
    mkdirp(&locale_dir)?;
    write_file(
        &locale_dir.join(".placeholder"),
        b"i18n deferred (BREAK); no compiled .mo catalogs yet",
    )
}

/// `share/bat/syntaxes/ghostty.sublime-syntax` — REAL content: the golden,
/// byte-exact source of the ported `rustty_extra::sublime::syntax()` generator.
/// Mirrors ghostty's install layout (`share/bat/syntaxes`, GhosttyResources.zig).
fn gen_bat(share_dir: &Path) -> Result<(), String> {
    let syntaxes_dir = share_dir.join("bat").join("syntaxes");
    mkdirp(&syntaxes_dir)?;
    write_file(
        &syntaxes_dir.join("ghostty.sublime-syntax"),
        SUBLIME_SYNTAX,
    )
}

fn mkdirp(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("create_dir_all {}: {e}", dir.display()))
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| format!("write {}: {e}", path.display()))
}
