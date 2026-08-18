//! I18N MSGID COVERAGE — the gate that makes "translations ship" falsifiable.
//!
//! A gettext msgid is a LOOKUP KEY. If a runtime call site spells a msgid even
//! one byte differently from the committed catalogs, gettext silently returns the
//! msgid unchanged and the string is permanently untranslatable — no crash, no
//! warning, no failing build. That is exactly the state WS13 found: three `.blp`
//! msgids still said "Ghostty" while all 31 catalogs said "Rustty".
//!
//! These tests read the SAME files the build and the runtime read, and assert the
//! msgid sets agree in both directions:
//!
//!   * every `_("…")` in `assets/ui/**/*.blp` exists as a msgid in the `.pot`;
//!   * every literal msgid looked up from `src/gtk/**/*.rs` (through
//!     `rustty_os::i18n::gettext_` or the `crate::gtk::i18n::tr` wrapper) exists as
//!     a msgid in the `.pot`;
//!   * every `.pot` msgid whose reference comment cites a code file (`*.zig:` /
//!     `*.rs:`, i.e. the oracle's code-side set) is actually looked up somewhere in
//!     `src/gtk/**/*.rs` or supplied by a `.blp` template — this is the half that
//!     catches an UNWRAPPED literal, which a one-directional check would miss
//!     entirely.
//!
//! Deliberately NOT feature-gated and pure `std`: it must run on macOS (where the
//! `gtk` feature does not build) as well as on Linux, since the `.blp` assets and
//! the catalogs are platform-independent data.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// Unescape the C escapes gettext uses in `msgid "…"` string bodies. The corpus
/// only ever uses `\"`, `\\` and `\n`, but the full single-char set is handled so a
/// future catalog cannot silently mis-parse.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Pull the body out of a `<prefix> "…"` line, or `None` if the line does not have
/// that shape.
fn quoted_body<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(prefix)?.trim_start();
    let rest = rest.strip_prefix('"')?;
    rest.strip_suffix('"')
}

/// Parse a `.po`/`.pot` byte stream into `msgid -> the reference comment block`.
/// Handles gettext's multi-line string continuation (`msgid ""` followed by bare
/// `"…"` lines).
fn parse_pot(bytes: &[u8]) -> BTreeMap<String, String> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = BTreeMap::new();
    let mut refs = String::new();
    let mut pending_refs = String::new();
    let mut msgid: Option<String> = None;
    let mut in_msgid = false;

    let flush =
        |out: &mut BTreeMap<String, String>, msgid: &mut Option<String>, refs: &mut String| {
            if let Some(id) = msgid.take() {
                if !id.is_empty() {
                    out.insert(id, std::mem::take(refs));
                } else {
                    refs.clear();
                }
            }
        };

    for line in text.lines() {
        let t = line.trim_end();
        if t.starts_with("#:") {
            pending_refs.push_str(t);
            pending_refs.push('\n');
            continue;
        }
        if t.starts_with("#~") {
            // obsolete entry — not a live msgid
            in_msgid = false;
            continue;
        }
        if let Some(body) = quoted_body(t, "msgid") {
            flush(&mut out, &mut msgid, &mut refs);
            msgid = Some(unescape(body));
            refs = std::mem::take(&mut pending_refs);
            in_msgid = true;
            continue;
        }
        if t.starts_with("msgstr") || t.starts_with("msgid_plural") {
            in_msgid = false;
            continue;
        }
        if in_msgid && t.starts_with('"') {
            if let Some(body) = quoted_body(t, "") {
                if let Some(id) = msgid.as_mut() {
                    id.push_str(&unescape(body));
                }
            }
            continue;
        }
        if t.is_empty() {
            flush(&mut out, &mut msgid, &mut refs);
            pending_refs.clear();
            in_msgid = false;
        }
    }
    flush(&mut out, &mut msgid, &mut refs);
    out
}

fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, ext, out);
        } else if p.extension().is_some_and(|e| e == ext) {
            out.push(p);
        }
    }
}

/// Scan `haystack` for every occurrence of `open` and return the double-quoted
/// literal that follows it, together with the 1-based line number.
fn scan_calls(text: &str, open: &str) -> Vec<(String, usize)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut idx = 0usize;
    while let Some(hit) = text[idx..].find(open) {
        let match_at = idx + hit;
        let start = match_at + open.len();
        idx = start;
        // Identifier-boundary: `tr(` must not match inside `from_str(`. This mirrors
        // xgettext's own token-based keyword matching.
        if match_at > 0 {
            let prev = bytes[match_at - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                continue;
            }
        }
        // Skip an optional Rust `c` string prefix and whitespace.
        let mut i = start;
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'c') {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'"' {
            continue;
        }
        i += 1;
        let mut lit = String::new();
        while i < bytes.len() {
            match bytes[i] {
                b'\\' if i + 1 < bytes.len() => {
                    lit.push('\\');
                    lit.push(bytes[i + 1] as char);
                    i += 2;
                }
                b'"' => break,
                _ => {
                    // Preserve multi-byte UTF-8 by slicing rather than byte-pushing.
                    let ch_len = utf8_len(bytes[i]);
                    lit.push_str(&text[i..i + ch_len]);
                    i += ch_len;
                }
            }
        }
        let line = text[..start].matches('\n').count() + 1;
        out.push((unescape(&lit), line));
        idx = i;
    }
    out
}

fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

/// All `_("…")` msgids in the blueprint templates, with `file:line` provenance.
fn blp_msgids(root: &Path) -> BTreeMap<String, String> {
    let mut files = Vec::new();
    walk(
        &root.join("crates/rustty-apprt/assets/ui"),
        "blp",
        &mut files,
    );
    assert!(
        !files.is_empty(),
        "no .blp files found — the walk is broken"
    );
    let mut out = BTreeMap::new();
    for f in files {
        let text = std::fs::read_to_string(&f).expect("read blp");
        for (msgid, line) in scan_calls(&text, "_(") {
            let rel = f.strip_prefix(root).unwrap_or(&f).display();
            out.entry(msgid).or_insert_with(|| format!("{rel}:{line}"));
        }
    }
    out
}

/// All translated msgids looked up from the GTK Rust sources, with `file:line`.
///
/// Two spellings count, and they are exactly the two `xtask update-translations`
/// passes to `xgettext` as `--keyword`s: `rustty_os::i18n::gettext_(c"…")` (the
/// direct port of the reference's `i18n._`) and `crate::gtk::i18n::tr(c"…")` (the
/// `&str`-returning wrapper the GTK setters need — see `src/gtk/i18n.rs`).
fn code_msgids(root: &Path) -> BTreeMap<String, String> {
    let mut files = Vec::new();
    walk(&root.join("crates/rustty-apprt/src/gtk"), "rs", &mut files);
    assert!(!files.is_empty(), "no .rs files found — the walk is broken");
    let mut out = BTreeMap::new();
    for f in files {
        let text = std::fs::read_to_string(&f).expect("read rs");
        for open in ["gettext_(", "tr("] {
            for (msgid, line) in scan_calls(&text, open) {
                let rel = f.strip_prefix(root).unwrap_or(&f).display();
                out.entry(msgid).or_insert_with(|| format!("{rel}:{line}"));
            }
        }
    }
    out
}

#[test]
fn blp_msgids_all_exist_in_pot() {
    let root = repo_root();
    let pot = parse_pot(&std::fs::read(root.join("po/io.airely.rustty.pot")).unwrap());
    assert!(
        pot.len() >= 75,
        "expected >=75 catalog msgids, found {}",
        pot.len()
    );
    let blp = blp_msgids(&root);
    assert!(
        blp.len() >= 51,
        "expected >=51 blueprint msgids, found {}",
        blp.len()
    );
    let missing: Vec<_> = blp
        .iter()
        .filter(|(m, _)| !pot.contains_key(*m))
        .map(|(m, where_)| format!("{where_}: {m:?}"))
        .collect();
    assert!(
        missing.is_empty(),
        "these blueprint msgids exist in no catalog, so they can NEVER translate \
         (fix the .blp to match the catalogs — the catalogs are authoritative, R2):\n{}",
        missing.join("\n")
    );
}

#[test]
fn code_msgids_all_wrapped_and_in_pot() {
    let root = repo_root();
    let pot = parse_pot(&std::fs::read(root.join("po/io.airely.rustty.pot")).unwrap());
    let code = code_msgids(&root);
    let blp = blp_msgids(&root);

    // Direction 1: nothing wrapped in code may be absent from the catalogs.
    let unknown: Vec<_> = code
        .iter()
        .filter(|(m, _)| !pot.contains_key(*m))
        .map(|(m, w)| format!("{w}: {m:?}"))
        .collect();
    assert!(
        unknown.is_empty(),
        "wrapped msgids absent from the catalogs:\n{}",
        unknown.join("\n")
    );

    // Direction 2 (the one that catches an unwrapped literal): every catalog entry
    // sourced from CODE (a `.zig:` reference from the oracle's own extraction, or a
    // `.rs:` reference once `xtask update-translations` has regenerated against the
    // port) must be looked up at runtime.
    //
    // The phase-4 upstream resync (oracle range 6ad1fe7d8..12967b68f) grew the
    // catalogs 79 -> 253 msgids, and the bulk of the new code-side entries are the
    // command-palette strings extracted from the oracle's `src/input/command.zig`
    // (mirrored here as `crates/rustty-input/src/command.rs` references). Their
    // call sites live in the INPUT crate — outside the GTK surface this test
    // walks — and the `i18n::N_` marking of `command.rs` lands with the input
    // wave (P4-E43 + the command-palette F work), not with the catalog mirror.
    // They are therefore split out of the GTK reachability check below, but their
    // block size is pinned so a regeneration that silently dropped the command
    // extraction (a lost `--keyword`, a bad path) cannot pass unnoticed.
    let command_block: BTreeSet<&String> = pot
        .iter()
        .filter(|(_, refs)| {
            refs.lines()
                .any(|r| r.contains("crates/rustty-input/") || r.contains("src/input/"))
        })
        .map(|(m, _)| m)
        .collect();
    assert_eq!(
        command_block.len(),
        180,
        "the command-palette msgid block changed size (was 180 at the phase-4 \
         mirror). If upstream added/removed palette commands and you re-mirrored \
         or regenerated, update this count deliberately."
    );
    let expected: BTreeSet<&String> = pot
        .iter()
        .filter(|(m, refs)| {
            !command_block.contains(m)
                && refs.lines().any(|r| {
                    (r.contains(".zig:") || r.contains(".rs:")) && !r.contains("nautilus")
                })
        })
        .map(|(m, _)| m)
        .collect();
    // The exact size is pinned so a regeneration that silently DROPPED code-side
    // references (e.g. a lost `--keyword`) cannot make the rest of this test
    // vacuous. 24, not the oracle's 25: `About Rustty` is the reference's
    // `window.zig:1824` string, and rustty's AD-3 collapse means it is supplied by
    // `window.blp:307` only — after `xtask update-translations` its `.pot` block no
    // longer carries any code reference. Changing this number is a deliberate act:
    // add or remove a code-side msgid, regenerate, then update it here.
    //
    // 24 -> 27 (`P3-W11 T4`, the Wayland hotkeys series): the three
    // `globalShortcutBindFailed` msgids ("The keybind was revoked by the system.",
    // "The keybind was denied by the system.", "Global keybind unavailable") land
    // WITH their call site in `class/application.rs`, never before it — an
    // unreachable msgid is exactly the shape Direction 2 below is written to fail on.
    //
    // 27 -> 31 (phase-4 upstream resync, oracle end state 12967b68f): the mirror
    // re-carries the oracle's code reference for "About Rustty" (`window.zig:1824`
    // -> `window.rs`; still satisfied by `window.blp` per the AD-3 note below),
    // and adds the three title-dialog msgids ("Change Terminal Title" /
    // "Change Tab Title" / "Change Window Title"), "Command Finished", and
    // "Editing configuration file"; "Command succeeded"/"Command failed" were
    // retitled upstream to "Command Succeeded"/"Command Failed" (same count).
    assert_eq!(
        expected.len(),
        31,
        "the code-side catalog msgid set changed (was 31). If you added/removed a \
         `tr(c\"…\")`/`gettext_(c\"…\")` call site and regenerated the .pot, update \
         this count deliberately; if you did not, extraction lost a keyword and \
         every other assertion here just became vacuous. Set: {expected:#?}"
    );

    // A handful of oracle code-side strings have no port call site because the
    // arm that produced them was collapsed by an architecture decision — e.g.
    // "About Rustty" comes from `window.zig:1824`, the pre-`adw` dialog arm that
    // AD-3 collapses in favor of `adw::AboutDialog`, so in rustty the msgid is
    // supplied by `assets/ui/1.5/window.blp:307` instead. Satisfying the msgid
    // from EITHER surface keeps the assertion honest (the string is still looked
    // up at runtime) without demanding a call site the port legitimately lacks.
    let reachable: BTreeSet<&String> = code.keys().chain(blp.keys()).collect();

    // Pending call sites from the phase-4 catalog mirror: msgids whose oracle
    // call site is in a GTK flow the port has not landed yet. Each entry names
    // the owning work item; remove it when that work lands (the assertion below
    // then starts enforcing it).
    //
    //   * "Editing configuration file" — the `0a183c923` "edit config in a
    //     Ghostty window" toast (`application.zig:2783`); lands with the
    //     P4-E25 / F-lane `open_config` payload port.
    //   * "Export" / "Export Terminal IO Events" — the `ad96613a8` inspector
    //     IO-event export file dialog (`application.zig:2259,2262`); lands with
    //     the F-lane inspector export port.
    let pending: BTreeSet<String> = [
        String::from("Editing configuration file"),
        String::from("Export"),
        String::from("Export Terminal IO Events"),
    ]
    .into_iter()
    .collect();

    let unwrapped: Vec<_> = expected
        .iter()
        .filter(|m| !reachable.contains(**m) && !pending.contains(**m))
        .map(|m| (*m).clone())
        .collect();
    assert!(
        unwrapped.is_empty(),
        "these msgids are translated in all 31 catalogs but no runtime call site looks \
         them up — they are shipped as untranslated English literals:\n{unwrapped:#?}"
    );
}
