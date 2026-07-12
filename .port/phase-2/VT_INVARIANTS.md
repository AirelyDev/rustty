# VT_INVARIANTS.md — paired-invariant catalog (detector D5)

**Detector:** D5 — paired-invariant catalog.
**Kills:** Bun **Class 4** — *coupled-emission-site invariant drift*
(oven-sh/bun#30678 / #30684: two individually-correct modules computed
"is Fast Refresh active" differently → client crash; no single-file diff
violated fidelity, so per-file review was blind).
**Spec:** `.port/phase-2/BUN_PORT_LEARNINGS.md` §4 (D5) and Class 4 (§ "Post-release
regression post-mortems"). **Rule set:** PORT_RULES.md pre-port tier checklist
item 2.

## What this catalog is

Every **predicate/flag consulted at 2+ producer/consumer sites** (especially
across a crate boundary) is a **named paired invariant**. For each we record:

1. **Predicate** — the exact property that must hold jointly across the sites.
2. **Producer site** — where the value is written/established. It carries (or
   should carry) a `debug_assert!` establishing the invariant locally — but only
   where that is a *faithful no-op addition* (an assert the reference already
   implies); otherwise the producer is documented, not edited.
3. **Consumer site** — where the value is read. It must **graceful-degrade** (do
   the safe/no-op thing) if the predicate is momentarily violated, never crash a
   downstream host — mirroring the reference's own tolerance.
4. **Integration test through the seam** — ONE test that drives producer → seam →
   consumer. Written in a disjoint new module when a real seam exists *in one
   crate*; otherwise the cross-crate wiring needed is noted and (where one
   already exists) the covering consumer-crate test is cited.

## Gate (review checklist — D5)

> **D5 review rule (BLOCKING at review, R5-invariant):** when a chunk touches
> **either end** of any invariant listed here — the producer *or* the consumer
> site (file path match), or adds a new predicate consulted at 2+ sites — the PR
> **must** update this catalog in the same change: adjust the file:line anchors,
> re-confirm the producer assert / consumer degrade, and keep the seam test
> green (or extend it). A chunk that touches an end without updating the catalog
> is a review reject. New cross-site predicates introduced by a tier are added
> here as part of that tier's pre-port gate (PORT_RULES pre-port checklist item
> 2).

The rule is mechanically self-checked by
`.port/tools/gates/check_paired_invariants.sh` (registered in `ci.sh` as `D5`,
PROGRESS class): it verifies this catalog is structurally complete, that every
cited producer/consumer anchor file still exists and still contains the cited
token, and that the in-crate seam tests named below still exist. The script is a
tripwire for silent drift; the human review rule above is the primary gate.

Anchors are `path:line (token)` — lines drift; the **token** is the durable
handle the checker greps for. Pinned against the working tree at catalog
creation.

---

## Seeded invariants

### INV-1 — dirty-tracking ↔ renderer damage

- **Predicate:** a grid row/screen marked dirty by the VT layer is redrawn by the
  renderer exactly once, and the renderer clears that dirty state after
  consuming it; a row not marked dirty is *not* rebuilt (the redraw-minimization
  contract). The renderer's own `Dirty::{False,Selection,Full}` damage state and
  per-row `row_data[y].dirty` mirror the VT dirty bits.
- **Producer (rustty-vt):** the per-`Row` dirty bit —
  `crates/rustty-vt/src/page.rs:978 (fn set_dirty)`; set across the write paths
  (e.g. `crates/rustty-vt/src/screen.rs:1822 ((*...page_row).set_dirty(true))`).
  Cleared as a set by `crates/rustty-vt/src/pagelist/access.rs:534
  (fn clear_dirty)`.
  *Producer assert status:* the dirty bit is a plain packed `bool`
  (`page.rs` bit 40) — there is no reference-implied predicate to assert at the
  setter (any row may be dirtied at any time), so **no debug_assert is added**
  (it would not be a faithful no-op). Documented only.
- **Consumer (rustty-renderer):** `crates/rustty-renderer/src/generic.rs:2900
  (if !rebuild && !...row_data[cvy].dirty)` and `:2948
  (row_data[y].dirty = false)` — the renderer reads the row dirty flag to skip
  clean rows and clears its own copy after rebuild. Graceful-degrade: a row seen
  dirty is simply rebuilt (never a crash); a missed dirty bit degrades to a
  stale cell, self-healing on the next `Dirty::Full` (`:2356 mark_dirty`).
- **Integration test through the seam:** **cross-crate** (VT produces, renderer
  consumes). A consumer-side integration test already exists in the renderer
  crate — `crates/rustty-renderer/src/generic.rs` `rebuild_cells_*` /
  `update_frame` tests exercise the row-dirty skip path. NEW dedicated end-to-end
  wiring (drive a real `Terminal` write → `update_frame` → assert only dirtied
  rows rebuilt) is tracked as cross-crate work; not added here to avoid editing
  the renderer crate's sources. **Status: seam covered by existing renderer
  tests; dedicated D5 e2e test = cross-crate TODO.**

### INV-2 — style-id refcount: Page ↔ style set

- **Predicate:** every non-default `style_id` stored in a cell has a matching
  live reference in that page's `RefCountedSet` (`page.styles`); the last
  `release` drops the living count to 0. Ids and refcounts never drift (no leak,
  no double-free).
- **Producer (rustty-vt):** `crates/rustty-vt/src/screen.rs:2779
  (fn manual_style_update)` → `page.styles.add`
  (`crates/rustty-vt/src/screen.rs:2829`) / `page.styles.release`
  (`crates/rustty-vt/src/screen.rs:2804`). The invariant IS asserted at the set:
  `crates/rustty-vt/src/ref_counted_set.rs:709 (assert(item.meta.ref_count >
  RefCount::ZERO))` in `fn release` (and the `> 0` / `>= n` asserts in
  `release_multiple`). *Producer assert status:* **already present and faithful**
  (mirrors the reference `RefCountedSet.release` assertions) — nothing added.
- **Consumer (rustty-vt):** cell style lookup + the integrity/clone paths that
  walk `page.styles.count()` (`crates/rustty-vt/src/page.rs`,
  `crates/rustty-vt/src/screen.rs:2095 ((*page).styles.release(base,
  (*cell).style_id()))`). Graceful-degrade: a lookup of a missing/default id
  returns the default style, never faults.
- **Integration test through the seam:** **in-crate (rustty-vt)** →
  `crates/rustty-vt/tests/d5_paired_invariants.rs`
  `fn style_id_refcount_page_style_set_seam` — drives `Terminal::set_attribute`
  (Bold → Unset) and asserts the active page's living style count goes 0 → 1 → 0.
  **Status: green.**

### INV-3 — kitty placements ↔ renderer atlas

- **Predicate:** when kitty image placements/images change, the storage's
  `dirty` flag is raised so the renderer re-uploads/re-lays the atlas exactly
  once, then clears it; an unchanged storage is not re-uploaded.
- **Producer (rustty-vt):** `crates/rustty-vt/src/kitty/graphics_storage.rs:59
  (pub dirty)` — set to `true` at every placement/image mutation
  (`:227`, `:279`, `:354`, `:479`, `:530`, `:558`, `:592`, ... `self.dirty =
  true`). *Producer assert status:* `dirty` is a bare change-notification `bool`
  with no reference-implied precondition to assert; **documented, not edited.**
- **Consumer (rustty-renderer):** `crates/rustty-renderer/src/image.rs:328
  (fn kitty_requires_update)` reads `(*screen).kitty_images.dirty`
  (`image.rs:336`); `crates/rustty-renderer/src/image.rs:466 (fn kitty_update)`
  re-lays the atlas and clears it at `image.rs:646
  ((*screen).kitty_images.dirty = false)`. Graceful-degrade: a spurious dirty
  just re-uploads identical images; a missed dirty self-heals on the next cell
  change that re-raises it.
- **Integration test through the seam:** **cross-crate** (VT storage produces,
  renderer atlas consumes). A consumer-side integration test already exists —
  `crates/rustty-renderer/src/generic.rs:7085` asserts `kitty_update` cleared
  `storage.dirty` through the seam. Dedicated D5 e2e wiring is a cross-crate
  TODO; not added here to avoid editing the renderer crate. **Status: seam
  covered by existing renderer test; dedicated D5 e2e test = cross-crate TODO.**

### INV-4 — mode-state ↔ key-encoder

- **Predicate:** the key encoder's output form matches the terminal mode state at
  encode time — DECCKM (`Mode::CursorKeys`) selects SS3 (`ESC O …`) vs CSI
  (`ESC [ …`) cursor keys; DECKPAM, DECBKM (backarrow), mode 1035/1036, and the
  live kitty-keyboard flags likewise. The mode the VT stream set and the mode the
  encoder reads must be the same value.
- **Producer (rustty-vt):** `crates/rustty-vt/src/modes.rs:69 (fn set)` on
  `Terminal::modes` (VT stream mode changes); the active screen's kitty-keyboard
  stack. *Producer assert status:* `ModeState::set` is a direct packed-flag
  write with no reference-implied precondition; **documented, not edited.**
- **Consumer (rustty-vt):** `crates/rustty-vt/src/input/key_encode.rs:105
  (fn from_terminal)` reads `t.modes.get(Mode::CursorKeys)` (etc.) and
  `(*t.screens.active).kitty_keyboard.current()` into `Options`;
  `crates/rustty-vt/src/input/key_encode.rs:129 (pub fn encode)` selects the
  encoding. Graceful-degrade: unknown/unset modes fall back to the legacy CSI
  encoding (`encode` → `legacy`), never an error.
- **Integration test through the seam:** **in-crate (rustty-vt)** →
  `crates/rustty-vt/tests/d5_paired_invariants.rs`
  `fn mode_state_key_encoder_seam` — sets `Mode::CursorKeys` on a real
  `Terminal`, runs `Options::from_terminal` + `encode(ArrowUp)`, and asserts
  reset → `ESC [ A`, set → `ESC O A`. **Status: green.**

---

## Adding a new invariant (procedure)

1. Confirm the predicate is consulted at **2+** sites (grep both ends).
2. Add a `### INV-N — <producer> ↔ <consumer>` block with all four fields and
   `path:line (token)` anchors.
3. Producer assert: add a `debug_assert!` **only** if it is a faithful no-op the
   reference already implies; else write "documented, not edited" with the
   reason.
4. Consumer: confirm the graceful-degrade path; cite it.
5. Seam test: if both ends are in one crate, add a `fn <name>_seam` to that
   crate's `tests/` (a `d5_*`-named file for VT), and cite it here. If
   cross-crate, note the wiring needed and cite any existing covering test.
6. Add the new INV-N id to the `EXPECTED_INV` list in
   `.port/tools/gates/check_paired_invariants.sh` so the tripwire covers it.

## Cross-crate seam-test backlog (D5 TODO)

- **INV-1** dirty ↔ renderer damage: dedicated e2e (Terminal write → renderer
  `update_frame` → assert selective rebuild).
- **INV-3** kitty placements ↔ atlas: dedicated e2e (place image via Terminal →
  renderer `kitty_update` → assert single atlas re-lay + dirty cleared).

Both currently rely on the consumer-crate (rustty-renderer) tests cited above;
they are logged here so the pre-port renderer/apprt tiers wire a first-class
producer→consumer test rather than leaving the seam implicit.
