//! WHOLE-FILE source guard: the PageList inspector widget must never restore a
//! compressed page. [WS19 T7 — audit row O4]
//!
//! WHY THIS LIVES IN A SIBLING FILE, NOT NEXT TO THE CODE.
//! The pre-existing guard (`src/widgets/pagelist.rs`'s `ws7_tests`) does
//! `include_str!("pagelist.rs")` and then asserts over a SUBSTRING — the page-tree
//! loop only. WS7 D6's own deferral predicted that weaker form, and it is exactly what
//! shipped: the loop was clean while **six** `Node::page(cell.node)` calls sat
//! elsewhere in the same file (`:1701, :1703, :1759, :1889, :2163, :2379`), so
//! inspecting a single cell in a compressed region silently decompressed it, and the
//! green guard said nothing. A whole-file assertion cannot live in the file it checks:
//! the assertion's own string literal `"Node::page("` would appear in the file and
//! satisfy itself. So it lives here.
//!
//! WHAT THE PROPERTY PROTECTS.
//! `Node::page()` RESTORES a compressed node to resident as a side effect. A single
//! call reachable from inspector draw code therefore decompresses scrollback merely
//! because a human opened a tab — undoing WS7 with no visible symptom, no panic and no
//! other failing test. The oracle guards against this structurally
//! (`oracle@main:src/inspector/widgets/pagelist.zig:14`): its `InspectedCell` struct
//! deliberately OMITS the node, so an inspector helper *cannot* reach a restoring
//! accessor even by accident. rustty's port mirrors that.

/// The widget source, read at compile time from the crate root so this file never
/// contains a copy of the forbidden pattern in a position that could satisfy the
/// assertion it makes about a *different* file.
const WIDGET_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/widgets/pagelist.rs"
));

/// The forbidden accessor, assembled at runtime so this file does not itself contain
/// the literal being searched for. (Belt and braces: the search target is another
/// file, but a future refactor that moved this test into `pagelist.rs` would then fail
/// loudly rather than pass vacuously.)
fn forbidden() -> String {
    format!("{}::{}(", "Node", "page")
}

/// True for a line that is nothing but a comment (`//`, `///`, `//!`).
///
/// The property being guarded is about CALL SITES, not prose: the PORT NOTES in the
/// widget deliberately name the forbidden accessor in order to explain *why* it is
/// forbidden, and a guard that banned the words would push that explanation out of the
/// code — the opposite of useful. Note this is a whole-line test only; a comment
/// TRAILING real code does not exempt that line, so `foo(); // Node::page` still
/// counts, and so does any code line however it is formatted.
fn is_comment_only(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

#[test]
fn pagelist_widget_never_restores_a_compressed_page_anywhere_in_the_file() {
    let needle = forbidden();
    let hits: Vec<(usize, &str)> = WIDGET_SRC
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains(&needle) && !is_comment_only(l))
        .map(|(i, l)| (i + 1, l.trim()))
        .collect();

    assert!(
        hits.is_empty(),
        "WS19 T7: the PageList inspector widget must reach pages through \
         `page_preserving_state` / `page_if_resident` / `rows` / `storage`, never \
         `Node::page(` — which RESTORES a compressed page as a side effect, so a \
         single call decompresses scrollback just because someone opened the \
         inspector tab. {} offending line(s):\n{}",
        hits.len(),
        hits.iter()
            .map(|(n, l)| format!("  src/widgets/pagelist.rs:{n}: {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Anti-vacuity: prove the assertion above is actually looking at the widget source.
/// If `include_str!` ever resolved to an empty or wrong file, the test above would
/// pass while checking nothing — the classic vacuous green.
#[test]
fn the_guard_is_actually_reading_the_widget_source() {
    assert!(
        WIDGET_SRC.len() > 50_000,
        "the widget source read by this guard is implausibly small ({} bytes) — \
         include_str! is probably pointing at the wrong file, which would make the \
         whole-file guard VACUOUS",
        WIDGET_SRC.len()
    );
    assert!(
        WIDGET_SRC.contains("pub struct InspectedCell"),
        "the widget source does not contain `InspectedCell`, the node-free cell \
         aggregate this guard exists to protect — either the port regressed or this \
         guard is reading the wrong file"
    );
    // And prove the NON-restoring accessors are the ones actually in use, so the
    // file cannot satisfy the guard by simply not touching pages at all.
    for accessor in [
        "page_preserving_state(",
        "page_if_resident(",
        "Node::rows(",
        "Node::storage(",
    ] {
        assert!(
            WIDGET_SRC.contains(accessor),
            "the widget source no longer uses `{accessor}` — the guard would then be \
             satisfied by a file that reads no pages at all, which is vacuous"
        );
    }
}
