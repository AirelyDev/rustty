# rustty-vt — Crown-Jewel Sub-Batch Plan (post-PageList)

PageList (vt-3c) is DONE (205 tests, faithful allocator). This is the topological
plan for the REST of rustty-vt. **Remaining ≈ 66k loc / ~2,000 tests** — essentially
the whole crown jewel is still ahead. Order: leaves-first, co-dependent clusters as
units (structs→methods→tests, the PageList discipline), Screen/Terminal last.

## Ordered sub-batches

| Batch | Modules | ~tests | Notes / risk |
|---|---|---|---|
| **vt-4** | selection_codepoints, cursor, focus, build_options+options, mouse (strip gobject), UTF8Decoder, Tabstops, dcs | ~21 | Trivial unblocked leaves, no cycles. **NEXT.** |
| ~~vt-5~~ | parse_table + Parser | 26 | **REORDERED after vt-6**: Parser EMBEDS osc.Parser (field + dispatch + osc tests) → osc must land first. Mutual cycle parse_table↔Parser stays struct/table-first. |
| **vt-6 (NEXT, before Parser)** | osc + encoding + **16** osc/parsers/* + kitty/color | ~243 | Parent↔child cycle, struct-first (osc.rs skeleton+stubs→fill). osc9(61)+semantic_prompt(64) dominate. (+hyperlink parser 8t, missed earlier.) |
| ~~vt-7~~ ✅ | kitty key/command/unicode/render/image + aggregators | ~47 | DONE. graphics_storage/exec + graphics_unicode render_placement/grid (+14 tests, 58 comments) DEFERRED to vt-12 — see DEFERRALS.md. Extended zlib-sys (full inflate) for decompressZlib. |
| vt-8 (after vt-9) | apc + apc/glyph/{response,request,Glossary,execute,glyph} | 67 | **External font/opentype dep** → sys-style hook seam. Only needed by vt-12, so AFTER stream. |
| **vt-9 (NEXT)** | stream | 38 | All deps DONE (Parser/osc/kitty/mouse/UTF8Decoder). Generic Stream<Handler> + TestHandler. No font risk. |
| **vt-10** | Screen+Selection+formatter+StringMap+ScreenSet — **STRUCTS only** (+ method sigs as todo!) | 0 | Breaks the 4-way cycle struct-first (PageList discipline). StringMap oniguruma → feature-gate. |
| **vt-11** | …same cluster — methods + tests | ~309 | Screen 188 + Selection 18 + formatter 100 + StringMap 3 + ScreenSet 2. XL. 2nd-biggest risk. |
| **vt-12** | Terminal ↔ stream_terminal (struct→method→test split 12a/b/c) + deferred kitty graphics_storage/exec + graphics_unicode render_placement/grid (DEFERRALS.md) | ~509 | **Biggest single unit** (Terminal 376 + stream_terminal 65 + 27). Mutual cycle. |
| vt-13 | render + SelectionGesture + highlight Tracked/track | 46 | Sits on full Screen/Terminal cluster. |
| vt-14 | input/{mouse,paste,key,mouse_encode,key_encode} | 130 | Needs Terminal + kitty/key. key_encode alone 92t. External config/function_keys seams. |
| vt-15 | search/* (gated) | 55 | Downstream of Screen/Selection/Terminal. |
| vt-16 | tmux/* (gated) | 114 | Last. Gated by options. |

## Top risks
1. **Terminal↔stream_terminal (vt-12)** — 13.2k loc / 441 tests, mutual cycle; split 12a/b/c.
2. **Screen 4-way SCC (vt-10/11)** — Screen↔Selection↔formatter↔StringMap, 18.7k loc, struct-first mandatory (the PageList trap at 3× scale).
3. **oniguruma C-dep** (StringMap) — feature-gate; tests SkipZigTest when off → mirror with #[cfg].
4. **apc font/opentype dep** (vt-8) — sys-style hook seam.
5. **formatter is INSIDE the Screen cycle** (not a leaf) — never port standalone.
6. **osc parent↔child cycle (vt-6)** — osc.zig types first, then 15 parsers.
7. **Hidden scope:** apc/kitty/osc/search/tmux/input subdirs were all 5-line stubs.

## Discipline (from PageList)
Each sub-batch: faithful (every line/comment/test, no tautology, no deferred-seam chains — port deps or cluster co-deps together). Gates per step: cargo build/test + clippy -D + fmt + scoped G3b (0 missing) + Miri (tractable) + ASan (`sanitizers.sh`) + tracking-allocator where pages are touched. Per-step commits. Adversarial faithfulness review per sub-batch. lib.rs PORT STATUS: stub until the whole crate lands.
