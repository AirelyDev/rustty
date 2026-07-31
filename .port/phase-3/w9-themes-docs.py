#!/usr/bin/env python3
"""P3-W9 T5 step 1 — update the theme-corpus provenance docs and the 534->592
count references after the re-vendor."""

EDITS = [
    (
        "crates/rustty-extra/themes/README.md",
        [
            (
                "- Upstream tarball: `ghostty-themes-release-20260608-160426-8c84dd1.tgz`\n"
                "  (`https://deps.files.ghostty.org/`), pinned in ghostty's `build.zig.zon`\n"
                "  under the `iterm2_themes` dependency\n"
                "  (hash `N-V-__8AAM7eAwCK4N5RemHoJXGZr7eQl49L6r8y1WJhzZGl`).\n",
                "- Upstream tarball: `ghostty-themes-release-20260720-153658-97e244c.tgz`\n"
                "  (`https://deps.files.ghostty.org/`), pinned in ghostty's `build.zig.zon`\n"
                "  under the `iterm2_themes` dependency\n"
                "  (hash `N-V-__8AAGxKBAAjYkT1jYUiOZDXQuj0u34ndIjAWxiR1KX9`).\n"
                "  Re-vendored by `P3-W9 T5` (oracle `4c1d69696` `b513f1b20` `02504bcce`\n"
                "  `07a56d08b` `dc8d38503`) from `…20260608-160426-8c84dd1`: **+58 themes,\n"
                "  4 content updates** (the four `Catppuccin *` files), 0 removals.\n",
            ),
            (
                "- **534** theme files, byte-identical to the upstream set (this `README.md` is\n"
                "  the only non-theme file here and is **excluded on emit** — mirroring ghostty's\n"
                "  `.md` exclusion, so the shipped count stays exactly 534).\n",
                "- **592** theme files, byte-identical to the upstream set (`README.md` and\n"
                "  `LICENSE.md` are the only non-theme files here and are **excluded on emit** —\n"
                "  mirroring ghostty's `.md` exclusion, so the shipped count stays exactly 592).\n"
                "\n"
                "## The digest gate (`G-THEMES`) — read this before re-vendoring\n"
                "\n"
                "`xtask/src/res_themes.rs` asserts the emitted corpus has exactly 592 files.\n"
                "That catches an added or deleted theme and **nothing else** — content drift\n"
                "inside an existing theme file is invisible to a count, and only ONE theme body\n"
                "is ever read by a test (`0x96f`, spot-checked for `palette = 0=`). Since the\n"
                "four `Catppuccin *` files changed CONTENT in this very bump, that is not a\n"
                "hypothetical.\n"
                "\n"
                "`crates/rustty-extra/themes.manifest` therefore carries a SHA-256 per file plus\n"
                "a total digest over the rows, checked by\n"
                "`.port/tools/gates/check_theme_corpus.py`. After a deliberate re-vendor, run\n"
                "`python3 .port/tools/gates/check_theme_corpus.py --write` and commit the\n"
                "manifest with the corpus.\n",
            ),
            (
                "emit filter, keeping the shipped set exactly 534 files like ghostty's).",
                "emit filter, keeping the shipped set exactly 592 files like ghostty's).",
            ),
            (
                "Do not hand-edit these theme files — they are upstream data. To refresh,\n"
                "re-vendor verbatim from the pinned `iterm2_themes` tarball.\n",
                "Do not hand-edit these theme files — they are upstream data. To refresh,\n"
                "re-vendor verbatim from the pinned `iterm2_themes` tarball\n"
                "(`.port/phase-3/w9-revendor-themes.py <extracted-dir>`, which preserves the\n"
                "single R2 filename rename below), then regenerate the digest manifest.\n"
                "\n"
                "**R2 filename rename (exactly one).** Upstream ships\n"
                "`Ghostty Default Style Dark`; rustty vendors it as\n"
                "`Rustty Default Style Dark`, contents byte-identical. It is the only identity\n"
                "token anywhere in the corpus — no theme BODY contains one (proven by the\n"
                "byte-compare in `.port/phase-3/w9-themes-diff.py`, which reported 0\n"
                "content changes outside the four Catppuccin files).\n",
            ),
        ],
    ),
    (
        "crates/rustty-extra/themes/LICENSE.md",
        [
            (
                "`ghostty-themes-release-20260608-160426-8c84dd1.tgz`",
                "`ghostty-themes-release-20260720-153658-97e244c.tgz`",
            ),
            ("count-identical (534 files) to ghostty's.", "count-identical (592 files) to ghostty's."),
        ],
    ),
    (
        "xtask/src/res_themes.rs",
        [
            (
                "the emitted count exactly equal to ghostty's shipped set (534 themes).",
                "the emitted count exactly equal to ghostty's shipped set (592 themes).",
            ),
            (
                "share/{APP}/themes/<Theme Name>   (534 iTerm2-derived .conf theme files)",
                "share/{APP}/themes/<Theme Name>   (592 iTerm2-derived .conf theme files)",
            ),
            (
                "    /// The vendored corpus is exactly ghostty's shipped set: 534 theme files\n"
                "    /// (upstream tarball minus `.md`), and the emitter reproduces that count.\n",
                "    /// The vendored corpus is exactly ghostty's shipped set: 592 theme files\n"
                "    /// (upstream tarball minus `.md`), and the emitter reproduces that count.\n"
                "    ///\n"
                "    /// A COUNT IS NOT A CONTENT CHECK. This assertion passes unchanged on a\n"
                "    /// corpus where a theme body has been corrupted — proven by executing that\n"
                "    /// exact mutation in `P3-W9 T5` step 3 (one byte flipped in `0x96f`: this\n"
                "    /// test still passed, `check_theme_corpus.py` went RED). Content is the\n"
                "    /// digest manifest's job: `crates/rustty-extra/themes.manifest` +\n"
                "    /// `.port/tools/gates/check_theme_corpus.py` (gate `G-THEMES`).\n",
            ),
            (
                "    /// exclusion keeps the SHIPPED set at exactly 534 files — parity with",
                "    /// exclusion keeps the SHIPPED set at exactly 592 files — parity with",
            ),
        ],
    ),
]

for path, subs in EDITS:
    s = open(path).read()
    for old, new in subs:
        n = s.count(old)
        assert n == 1, f"{path}: anchor occurs {n}x: {old[:60]!r}"
        s = s.replace(old, new)
    open(path, "w").write(s)
print("ok")
