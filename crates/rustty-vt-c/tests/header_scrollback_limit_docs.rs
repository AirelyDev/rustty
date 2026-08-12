//! header_scrollback_limit_docs.rs — pins the public-header documentation of
//! the two scrollback limit options to the Ghostty oracle (codex-vet W12-03).
//!
//! ORACLE: ghostty include/ghostty/vt/terminal.h:968-1012 documents
//! `GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES` and
//! `..._SCROLLBACK_MAX_LINES` with THREE paragraphs each, not one:
//!
//!   1. the page-granularity ESTIMATE contract — libghostty only prunes up to
//!      a "page" boundary, a page is ~400KB, so the byte limit is only
//!      accurate within that delta and the retained LINE count "will almost
//!      always be higher than configured" (dozens to a hundred or so lines);
//!   2. the COMBINED-limit contract — when both limits are set the
//!      first-reached one wins, and because each depends on different external
//!      state (styles/graphemes for bytes, viewport width for lines) they are
//!      useful together;
//!   3. the lowering/zero/NULL paragraph.
//!
//! Rustty's header carried only (3). The omission was public-contract-only:
//! the ported PageList prunes at page granularity exactly as Ghostty does, so
//! C consumers sizing retained history were reading a header that understated
//! how much scrollback they actually get back.
//!
//! WHY A TEST FOR A COMMENT: `include/rustty/vt/*.h` is hand-maintained (no
//! cbindgen step regenerates it from the Rust `Option_` enum), so nothing in
//! the build graph notices when a doc paragraph is dropped or drifts from the
//! oracle. This test is that notice. It pins the two doc blocks BYTE-EXACTLY
//! — including the oracle's trailing spaces, which the rest of this header
//! preserves too (terminal.h:56-57, :80) and which the wave-12 header-parity
//! predicate (`.port/codex/line-audit/waves/probes/wave12-header-parity.py`,
//! byte-equality after Ghostty->Rustty identity renaming) requires. Every
//! expected line below is a separate literal so the trailing spaces sit INSIDE
//! the quotes and cannot be eaten by a whitespace-trimming editor or hook.
//!
//! The only identity normalization applied to the oracle text is the repo-wide
//! casing-faithful rename (Ghostty->Rustty, libghostty->librustty).

use std::path::PathBuf;

fn header_text() -> String {
    let path: PathBuf =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../include/rustty/vt/terminal.h");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Assert `block` appears in the header exactly once, as a contiguous run of
/// whole lines, reporting the first line that diverges when it does not.
fn assert_block(header: &str, what: &str, block: &[&str]) {
    let lines: Vec<&str> = header.lines().collect();
    let hits = lines
        .windows(block.len())
        .filter(|w| w.iter().zip(block).all(|(a, b)| a == b))
        .count();
    if hits == 1 {
        return;
    }

    // Anchor on the block's LAST line — the enumerator, the one line that is
    // unique in the file — and walk the block forward from there, so the
    // message names the first line that actually drifted rather than the first
    // `  /**` in the header.
    let last = block[block.len() - 1];
    let anchor = lines
        .iter()
        .position(|l| *l == last)
        .and_then(|end| (end + 1).checked_sub(block.len()));
    let detail = match anchor {
        None => format!("enumerator line {last:?} not present at all"),
        Some(start) => {
            let mut msg = String::new();
            for (i, want) in block.iter().enumerate() {
                let got = lines.get(start + i).copied().unwrap_or("<eof>");
                if got != *want {
                    msg = format!(
                        "diverges at header line {}:\n  want {want:?}\n  got  {got:?}",
                        start + i + 1
                    );
                    break;
                }
            }
            msg
        }
    };
    panic!(
        "include/rustty/vt/terminal.h: {what} doc block does not match the \
         Ghostty oracle (matches found: {hits}); {detail}"
    );
}

/// The oracle's page-granularity + combined-limit paragraphs must be present
/// on OPT_SCROLLBACK_MAX_BYTES (ghostty terminal.h:968-988).
#[test]
fn scrollback_max_bytes_doc_matches_oracle() {
    assert_block(
        &header_text(),
        "RUSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES",
        &[
            "  /**",
            "   * Set the maximum scrollback allocation in bytes.",
            "   *",
            "   * This is an estimate. Internally, librustty only prunes bytes up ",
            "   * to a \"page\"-granularity. A page is the minimum allocated unit of",
            "   * grid space within Rustty. A page at the time of writing these docs",
            "   * is about 400KB, so the byte limit will be within this delta.",
            "   *",
            "   * This works alongside the line limit configuration. If both are set,",
            "   * the first-reached limit is used first. Both limits are dependent",
            "   * on external state (byte limit can be reached with less lines if",
            "   * more styles are used for example, line limit can be reached with",
            "   * a narrower terminal viewport). So, they are useful together.",
            "   *",
            "   * Lowering the limit immediately removes eligible complete historical",
            "   * pages. A value of zero disables scrollback and erases retained history.",
            "   * A NULL value pointer removes the byte limit.",
            "   *",
            "   * Input type: size_t*",
            "   */",
            "  RUSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES = 27,",
        ],
    );
}

/// The oracle's page-granularity + combined-limit paragraphs must be present
/// on OPT_SCROLLBACK_MAX_LINES (ghostty terminal.h:990-1012). Note the oracle
/// repeats "works alongside the line limit configuration" verbatim in BOTH
/// entries; the port keeps the oracle's wording rather than "correcting" it.
#[test]
fn scrollback_max_lines_doc_matches_oracle() {
    assert_block(
        &header_text(),
        "RUSTTY_TERMINAL_OPT_SCROLLBACK_MAX_LINES",
        &[
            "  /**",
            "   * Set the maximum number of physical lines retained in scrollback.",
            "   *",
            "   * This is an estimate. Internally, librustty only prunes lines up ",
            "   * to a \"page\"-granularity. A page is the minimum allocated unit of",
            "   * grid space within Rustty. As a result, the actual available scrollback",
            "   * lines will almost always be higher than configured. The magnitude ",
            "   * of the difference depends on the number of used styles, graphemes, etc.",
            "   * since the row-count in a page is dynamic based on that. In general,",
            "   * it ranges from dozens to a hundred or so lines.",
            "   *",
            "   * This works alongside the line limit configuration. If both are set,",
            "   * the first-reached limit is used first. Both limits are dependent",
            "   * on external state (byte limit can be reached with less lines if",
            "   * more styles are used for example, line limit can be reached with",
            "   * a narrower terminal viewport). So, they are useful together.",
            "   *",
            "   * Lowering the limit immediately removes eligible complete historical",
            "   * pages. A NULL value pointer removes the line limit.",
            "   *",
            "   * Input type: size_t*",
            "   */",
            "  RUSTTY_TERMINAL_OPT_SCROLLBACK_MAX_LINES = 28,",
        ],
    );
}
