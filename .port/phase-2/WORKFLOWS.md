# rustty port — workflow scripts (`.port/phase-2/wf-*.js`)

This directory collects the **multi-agent orchestration scripts** that drove the
port, one `wf-<milestone>.js` per Workflow-tool run. They are the executable
provenance of the proven method — **design → adversary → port → verify** — that
every substantive milestone went through (see `PORT_RULES.md`; the paired *design
artifacts* are the `*_BLUEPRINT.md` / `*_DESIGN.md` docs beside them).

## Convention

- **Location + name:** `.port/phase-2/wf-<short-milestone-name>.js` (flat, `wf-`
  prefix, the Workflow run-id suffix stripped). One file per orchestration run.
- **Reproducible:** re-run any of them with
  `Workflow({scriptPath: ".port/phase-2/wf-<name>.js"})`. Each script is
  self-contained plain JS: an `export const meta = {...}` header + a body using
  `agent()` / `parallel()` / `pipeline()` / `phase()`, with the agent prompts
  inline. They reference `REPO=/Users/gael/projects/rustty` and the read-only
  reference `REF=/Users/gael/projects/ghostty` as absolute paths.
- **Going forward:** new workflow scripts are authored **directly here** (write
  the `.js` to `.port/phase-2/wf-<name>.js`, then `Workflow({scriptPath})`) rather
  than left in the session/job scratch dirs the Workflow tool otherwise persists
  to (those are ephemeral — the job scratch dir is cleaned when the job is
  deleted, which is how this collection came to be needed).

## Index (by tier — filenames are self-describing)

- **Phase 0/1 — harness + gates (3):** `wf-phase0-reference-oracle`,
  `wf-phase0-oracle-baseline`, `wf-phase1-workspace-gates`.
- **VT foundation — datastruct / simd / zigstd / allocators (batches) (~19):**
  `wf-phase2-batch{1,2,3,4}-*`, `wf-phase2-inc1-datastruct`,
  `wf-phase2-zigstd-memory-model`, `wf-alloc-*`, `wf-tracking-alloc`.
- **VT engine — the crown jewel `rustty-vt` (~52):** `wf-phase2-vt{1,2,3,3b,3c}-*`
  + `wf-vt3c-*` … `wf-vt16-tmux`, `wf-clearcells-sb-fix`,
  `wf-screen-tests-recovery`, `wf-vt-simd-wiring`.
- **VT C-ABI — `rustty-vt-c` (2 + designs):** `wf-vtc-*`,
  `wf-d2-vtc-abi-design-panel`, `wf-d3-vtc-trampoline-seam`,
  `wf-vtc-soundness-fixes`.
- **Config — `rustty-config` (6):** `wf-config-{scoping-panel,c1-leaves,c2-struct,
  c2b-continue,c2c-passes,c3-capi}`.
- **Font pure — `rustty-font` (6):** `wf-font-f0-*`, `wf-font-sprite*`,
  `wf-font-glyf`, `wf-d1-apc-glyf-design-panel`.
- **Font `-sys` + shaper (8):** `wf-freetype-sys-vendor`, `wf-harfbuzz-sys-vendor`,
  `wf-oniguruma-sys`, `wf-vendor-*`, `wf-font-shaper-f3-faces`,
  `wf-font-shaper-f4*`.
- **Mid-tier leaves (3):** `wf-cfgfmt-input`, `wf-stage-c-args`,
  `wf-stage-c-terminfo`.
- **Integration — core / apprt / cli / termio (10):** `wf-core-{design,wave1,wave2}`,
  `wf-embedded-cabi`, `wf-apprt-embedded-design`, `wf-integration-tier`,
  `wf-cli-{design,port}`, `wf-help-strings`, `wf-heavy-tier-scoping`.
- **Event loop — `rustty-xev` (D6) (4):** `wf-xev-design`, `wf-xev-milestone-a`,
  `wf-xev-milestone-b-design`, `wf-xev-milestone-b`.
- **Renderer + GPU R-d (5):** `wf-renderer-ra`, `wf-gpu-rd-design`,
  `wf-gpu-rd-chunk2-metal`, `wf-gpu-rd-p1-image-overlay`,
  `wf-gpu-rd-chunk3a-renderer-skeleton`.

> Note: the collection spans several work sessions and predates this manifest, so
> the early hand-curated `wf-vt*` names and the later `wf-<meta.name>` names
> coexist; a few may be adjacent re-runs of the same milestone. The paired
> `*_DESIGN.md` / `*_BLUEPRINT.md` docs are the authoritative record of each
> milestone's decisions.
</content>
