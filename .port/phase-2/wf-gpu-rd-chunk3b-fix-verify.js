export const meta = {
  name: 'gpu-rd-chunk3b-fix-verify',
  description: 'Independent adversarial verification of the Chunk-3b aliasing FIX (rebuild_row shaper-hook UB + add_cursor UB + the dropped R1 comment), which the prior verify panel flagged as blockers. Three parallel lenses: (1) aliasing/UB re-audit incl. re-running the isolated Miri harness under SB+TB; (2) R1 comment/line fidelity of the raw-self conversion vs the Zig reference; (3) gates + no-regression. UNCOMMITTED working tree.',
  phases: [{ title: 'Verify-fix' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'

const VERDICT_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['lens', 'verdict', 'findings', 'attacked_hardest'],
  properties: {
    lens: { type: 'string' },
    verdict: { type: 'string', enum: ['PASS', 'CONCERNS', 'FAIL'] },
    attacked_hardest: { type: 'array', items: { type: 'string' } },
    findings: {
      type: 'array',
      items: {
        type: 'object', additionalProperties: false,
        required: ['severity', 'file', 'issue'],
        properties: {
          severity: { type: 'string', enum: ['blocker', 'major', 'minor'] },
          file: { type: 'string' }, issue: { type: 'string' },
        },
      },
    },
  },
}

const COMMON = [
  'REPO=' + REPO + '. REFERENCE (read-only)=' + REF + ' (Zig 0.15.2, src/renderer/generic.zig + row.zig).',
  'CONTEXT: GPU R-d Chunk 3b (the CPU cell-fill in crates/rustty-renderer/src/generic.rs, mod macos_impl) was ported, but a prior verify panel found (a) rebuild_row\'s shaper run loop was Stacked/Tree-Borrows UB — the RunIteratorHook{shaper:*mut Shaper} alias into self.font_shaper was popped by the interleaved `self.font_shaper.shape(...)` (sibling reborrow) and `self.add_glyph(...)` (&mut *self whole-renderer receiver reborrow), then re-dereffed by run_iter.next(); (b) add_cursor(&mut self, cursor_state: *const render::Cursor) was SB-UB — the &mut self receiver popped the cursor_state alias into inline self.terminal_state.cursor before deref; (c) one dropped R1 comment `// Get our row data from our state` (generic.zig:2364).',
  'THE FIX (now in the working tree, to verify): (1) the in-loop helpers add_glyph/add_underline/add_overline/add_strikethrough + add_cursor were converted from `&mut self` methods to raw-self associated fns `unsafe fn add_*(self_raw: *mut Renderer, ...)`, mirroring the reference `self: *Self`, using ONLY narrow `(*self_raw).field` access (never a whole `&mut *self_raw`); (2) `shape` is now invoked through the hook\'s OWN raw pointer `unsafe { (*run_iter.hooks.shaper).shape(run) }` (a child reborrow that does not pop the hook), never `self.font_shaper.shape`; (3) rebuild_row establishes `let self_raw: *mut Renderer = self;` and calls the helpers via it; (4) add_cursor is called as `Renderer::add_cursor(self_raw, cursor_ptr, ...)`; (5) the dropped comment was re-added at the per-row AoS read.',
  'PROOF ARTIFACT: an isolated non-FFI Miri harness at ' + REPO + '/.port/phase-2/mirihar-chunk3b/ reproduces the exact borrow structure. `cargo +nightly miri run -- <candidate>` (and with MIRIFLAGS=-Zmiri-tree-borrows) partitions candidates: row_current/row_shaper_idiom/row_rawsibling = UB (both models); row_rawself/row_surgical/row_persistent/row_realistic = CLEAN; cursor_current = SB-UB; cursor_rawself/cursor_hoist = CLEAN. The shipped generic.rs must match the CLEAN structure.',
  'Rules: ' + REPO + '/.port/PORT_RULES.md — R1 (100% fidelity: every reference line/comment/test{} preserved; the raw-self conversion must be behavior-identical — only `self.X`->`(*self_raw).X` + unsafe wrapping, NO value/branch/line changed). G3b comment-coverage SKIPS rustty-renderer, so the adversary/verify is the ONLY R1 backstop — check by hand.',
].join('\n')

const LENSES = [
  {
    key: 'aliasing',
    text: 'ALIASING / UNDEFINED-BEHAVIOR RE-AUDIT. (1) Read the shipped rebuild_row/add_cursor/add_glyph/add_underline/add_overline/add_strikethrough/add_preedit_cell in crates/rustty-renderer/src/generic.rs. CONFIRM the Miri-clean structure holds EVERYWHERE: shape is ALWAYS via `(*run_iter.hooks.shaper).shape(...)` (never self.font_shaper.shape in live code); every in-loop add_* is a raw-self assoc fn taking `self_raw: *mut Renderer` called via self_raw; NO raw-self body ever binds a whole `&mut *self_raw` (only narrow `(*self_raw).field`); add_cursor takes raw self + `*const render::Cursor`. HUNT for ANY remaining sibling reborrow of font_shaper, whole-Renderer `&mut *self`/`&mut *self_raw`, or raw-into-inline-self alias crossed by a `&mut self` call, between a hook creation and its last use. (2) grep to prove there is NO live-code `self.font_shaper.shape(` or `self.add_glyph(`/`self.add_underline(`/`self.add_cursor(` etc (matches must be in `//` comments only). (3) RE-RUN the Miri harness yourself: `cd .port/phase-2/mirihar-chunk3b && for c in row_current row_rawself row_surgical row_realistic cursor_current cursor_rawself; do cargo +nightly miri run --quiet -- $c; MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run --quiet -- $c; done` — confirm row_current/cursor_current report UB and the shipped-structure candidates are CLEAN under BOTH models. Verify the harness main.rs FAITHFULLY mirrors the real borrow shape (hook = *mut Shaper from run_iterator; next() derefs it; shape() borrows+mutates the shaper; add_glyph raw-self narrow). A blocker = any real residual UB path in generic.rs, or the harness not faithfully modeling it.',
  },
  {
    key: 'fidelity',
    text: 'R1 COMMENT + LINE FIDELITY OF THE FIX. The raw-self conversion must be BEHAVIOR-IDENTICAL to the pre-fix code and faithful to the Zig reference. For EACH converted method (add_underline 3059 / add_overline 3099 / add_strikethrough 3131 / add_glyph 3161 / add_cursor 3224 in generic.zig) and the rebuild_row 2610 / rebuild_cells 2307 / add_preedit_cell 3313 call sites: diff the shipped Rust vs the reference. CONFIRM: (a) NO reference comment was dropped or truncated by the conversion (every `//` comment that was present pre-fix is still present — the SAFETY notes were ADDED, none removed); (b) NO value/branch/argument/line changed — the ONLY changes are `&mut self`->`self_raw: *mut Renderer`, `self.X`->`(*self_raw).X`, `unsafe {}` wrapping, and `self.add_*(...)`->`unsafe { Renderer::add_*(self_raw, ...) }`; every CellText/RenderOptions field, every color/coordinate/alpha, the render_glyph/render_codepoint args, and the control flow are byte-identical; (c) the dropped `// Get our row data from our state` (generic.zig:2364) is now present at the per-row read; (d) the SAFETY doc-comment on rebuild_row no longer contains the FALSE claim "TRANSIENT &mut self ... completes BEFORE the next run_iter.next()" and instead states the true mechanism. A blocker = any dropped reference comment or any behavioral divergence introduced by the conversion.',
  },
  {
    key: 'gates',
    text: 'GATES + NO-REGRESSION. RUN and report real tails: `cargo build --workspace`; `cargo clippy -p rustty-renderer --all-targets -- -D warnings`; `cargo fmt -p rustty-renderer -- --check`; `cargo test -p rustty-renderer` (must include generic::macos_impl::tests::rebuild_cells_fills_bg_fg_cursor + rebuild_overlay_builds_and_clears passing); `bash .port/tools/gates/check_acyclic.sh`; `python3 .port/tools/gates/check_test_counts.py`. Confirm the whole workspace still builds (no regression from the raw-self signature changes / the lib.rs `pub mod row;`). A blocker = any gate not honestly green.',
  },
]

phase('Verify-fix')
const verdicts = await parallel(LENSES.map((l) => () =>
  agent(
    'ADVERSARIAL verifier for the GPU R-d Chunk-3b aliasing FIX (UNCOMMITTED working tree). Be skeptical; find REAL, evidenced problems only. LENS:\n' + l.text + '\n\n' + COMMON + '\nReport the schema: lens, verdict (PASS/CONCERNS/FAIL), findings (severity/file/issue), attacked_hardest (the 2 things you tried hardest to break). If clean, empty findings + PASS.',
    { label: 'verify-fix:' + l.key, phase: 'Verify-fix', agentType: 'general-purpose', schema: VERDICT_SCHEMA })))

const all = verdicts.filter(Boolean).flatMap((v) => (v.findings || []).map((f) => ({ ...f, lens: v.lens })))
const blockers = all.filter((f) => f.severity === 'blocker')
log('3b fix-verify: ' + verdicts.filter(Boolean).map((v) => v.verdict).join(', ') + ' | ' + blockers.length + ' blockers, ' + all.length + ' findings')
return { verdicts, findings: all, blockers }
