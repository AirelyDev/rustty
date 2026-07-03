# help_strings — implementation blueprint (Phase 2)

Ports ghostty's build-time `help_strings` doc tables (`src/build/HelpStrings.zig`
+ `src/helpgen.zig`) so the three deferred seams get real impls and their
un-defers land: the `explain_config` **R1-contract test**, `FileFormatter --docs`,
`list_keybinds --docs`, and the `+help` action text.

---

## TL;DR — decisions

1. **Generation:** `build.rs` codegen in each owning crate, backed by a shared
   **leaf** build-lib `rustty-helpgen` (a faithful port of
   `helpgen.extractDocComments` + `findCommonPrefix` + the field/variant/fn
   doc-scan). Regenerated from the `///` source every build → self-consistent by
   construction (R3), matching the reference which never commits `help_strings.zig`.
2. **Three tables, three homes:** `config_help` in rustty-config, `keybind_help`
   in rustty-input, `action_help` in rustty-cli — each next to the `///` source it
   extracts, each reachable by its consumers without a new runtime cycle.
3. **BLOCKER / real work:** the reference's **Config field `///` doc comments were
   NOT ported** into `config_struct.rs` (they are the "fill sub-batch" the source
   already flags). Keybind-action and CLI-action docs **are** present. Porting the
   ~209 Config field doc blocks (R3, scoped-G3b) is the prerequisite and is far
   larger than the generator itself. Without it the Config table is empty and the
   `explain` test fails on `font-size`.

---

## 1. Where the doc-comments live — and whether they are present

Reference (`helpgen.zig`, run by `HelpStrings.zig`) `@embedFile`s three sources,
walks the AST (token pattern: `.doc_comment` immediately above the target
`.identifier`/`fn run`), dedents, and emits three namespaces:

| Table | Reference source | Iterated over | Port location | Present? |
|---|---|---|---|---|
| `help_strings.Config` | `config/Config.zig` field `///` | non-`_` struct fields (209 documented; **3625** doc-decl lines) | `crates/rustty-config/src/config_struct.rs` fields | **NO** |
| `help_strings.Action` | each `cli/<action>.zig` `///` above `pub fn run` | `Action` enum (18) | `crates/rustty-cli/src/<action>.rs` `pub fn run` | **YES** (all 18) |
| `help_strings.KeybindAction` | `input/Binding.zig` union field `///` | non-`_` union fields (~85; **700** doc-decl) | `crates/rustty-input/src/binding.rs` `Action` variants | **YES** |

Verification evidence:

- **KeybindAction — present.** `binding.rs` `Action` variants carry their `///`
  docs; `copy_to_clipboard`'s full block is at `binding.rs:715-735`
  ("Copy the selected text to the clipboard… `mixed` (default)…"). `binding.rs`
  has 770 `///` lines ≥ reference 700; input landed scoped-G3b-green. ✓
- **Action (CLI) — present.** All 18 `crates/rustty-cli/src/*.rs` action modules
  have a `///` doc immediately above `pub fn run` (e.g. `explain_config.rs:70-84`). ✓
- **Config — ABSENT.** The `Config` struct starts at `config_struct.rs:811`; its
  fields are bare (`// @"font-size": f32 = …` reference comment, then
  `pub font_size: f32,` at :834) with **no `///` above them**. `font-size`'s
  reference doc (Config.zig:247-262, "Font size in points… 13.5pt @ 2px/pt = 27px…
  GNOME display scale…") has **zero** analog anywhere in `crates/` (whole-tree grep
  for `nearest integer pixel` / `13.5pt` / `high dpi display` / `GNOME display
  scale` = 0 hits). `config_struct.rs` has only 262 `///` lines total, all on
  value-types / the `Key` enum / methods — **none in the field region** (lines
  811–1300: zero `///`).
  - **Gate state:** whole-crate `check_comment_coverage.py --crate rustty-config
    --strict` is **RED** — 25 hard `MISSING ref comment` (fuzzy overlap with the
    ported value-type docs masks the rest), and the `font-size` doc line
    ("(e.g. increased or decreased the font size)…") is explicitly among them.
  - **Tracked:** `config_struct.rs:810` — *"PORT NOTE: per-field Pandoc doc
    comments are ported in the fill sub-batch."* This milestone **is** that fill
    sub-batch (its prerequisite).

> The task premise "the scoped-G3b gate enforced [the field docs are present]"
> holds for keybind/CLI-action docs but **not** for Config fields — those were
> deferred to the fill sub-batch. The generator cannot invent them.

---

## 2. Generation approach — decision

**Recommend (a) `build.rs` codegen**, one per owning crate, sharing a leaf
build-lib for the extraction logic. Rejected: (b) proc-macro, (c) committed data.

Why (a):

- **Faithful to the reference build graph.** `HelpStrings.zig` is a dedicated
  build step running `helpgen` which parses the *source file* and emits a
  generated module. A `build.rs` that scans the crate's `.rs` source and writes a
  table into `OUT_DIR` (`include!`d) is the line-for-line analog. Repo precedent:
  `rustty-{font-shaper,os,simd,build-config}` already ship `build.rs`; the
  `sys-generated-config-header` memory establishes the "generate into OUT_DIR,
  include" pattern.
- **Self-consistent by construction (R3).** The table is generated *from* the
  `///` comments on every build, so `table == ported comment text` cannot drift.
  The reference does **not** commit `help_strings.zig` — committing a generated
  module (c) deviates and reintroduces drift risk.
- **Extraction = faithful port, not `syn` semantics.** Port `extractDocComments`
  (`slice[3..]` to drop `///`, `findCommonPrefix` min-leading-space dedent, join
  with `\n`, drop trailing) as a small line scanner over contiguous `///` blocks —
  this mirrors ghostty's *lexer-level* token walk (`.doc_comment`→`.identifier`),
  not a semantic AST pass. Key each entry by the **original CLI name** exactly as
  ghostty keys by `field.name`, sourced from the already-landed `CONFIG_FIELDS`
  (`(b"font-size", Key::FontSize)`) / `Action::FIELD_NAMES`. **Do not** rely on
  rustc/`syn` `#[doc]` attribute text — its one-space trimming differs from `[3..]`
  and it exposes only the Rust ident `font_size`, not `font-size`.
- **vs (b) proc-macro:** more Rust-native and consistency-guaranteed, but less
  faithful to the reference's "build step parses source → emits module" model, and
  a derive on the struct still needs the ident→CLI-name rename map anyway. Not worth
  the fidelity cost.

Extraction spec (shared lib `rustty-helpgen`, port of `helpgen.zig`):
`extract_doc(source, kind, name) -> Option<Vec<u8>>` with `kind ∈ {field-ident,
union-field, fn-run}`; plus `find_common_prefix`. Each `build.rs` calls it over
its own source + `cargo:rerun-if-changed` on that source.

---

## 3. Placement, ownership, seams, acyclicity

Emit shape (Rust-native analog of `@hasDecl`+`@field`): each table is a generated
`fn <name>_help(key) -> Option<&'static [u8]>` (an exhaustive `match`; `None` ==
`@hasDecl == false`).

| Table | Generated in (scans) | Emitted fn | Consumers | Edge |
|---|---|---|---|---|
| Config | rustty-config (`config_struct.rs`) | `config_help(Key) -> Option<&[u8]>` | rustty-config (`formatter_file`), rustty-cli (`explain_config`, `show_config`) | cli→config exists ✓ |
| KeybindAction | rustty-input (`binding.rs`) | `keybind_help(&str) -> Option<&[u8]>` | rustty-input (`helpgen_actions`), rustty-config (`value_types::Keybinds::format_entry_docs`), rustty-cli | config→input **and** cli→input exist ✓ |
| Action | rustty-cli (`src/*.rs`) | `action_help(Action) -> &[u8]` | rustty-cli (`ghostty::help_text`) | cli-local ✓ |

**Acyclicity (G7 caveat):** `cargo_cycle_check.py` **includes**
`[build-dependencies]` edges. So `rustty-helpgen` must be a genuine **leaf**
(std-only; depends on no rustty crate) — then `config/input/cli → helpgen`
build-dep edges stay acyclic. (Alternative if a new crate is unwanted: inline the
~40-line extractor into each of the 3 `build.rs` files — zero new edges, at the
cost of triplication. Prefer the shared leaf: one source of truth, mirrors the
single `helpgen.zig`.)

Seams → real impls (all already scaffolded):

- **input `HelpStrings` trait** (`helpgen_actions.rs:93`): keep it as the
  injection seam (landed, 0-churn); add `RealHelpStrings` whose `has_decl`/`field`
  delegate to `keybind_help`. `NoHelpStrings` stays for the absent/test case.
  Callers (`list_keybinds` path) pass `RealHelpStrings`. `generate` signature
  unchanged.
- **config `formatter_file.rs:139-143`**: replace the `debug_assert!(!do_docs,…)`
  with `let do_docs = self.docs && config_help(key).is_some();` and emit the
  reference block (`# {line}` per `\n`-split of the help text; trailing `\n`).
- **config `value_types::Keybinds::format_entry_docs`**: the `docs=true` branch
  reads `keybind_help(name)` (`# {name}` + `#   {line}` lines).
- **cli `explain_config.rs`**: `explain_option` → `config_help(key)`;
  `explain_keybind` → `keybind_help(name)`. (Validation `Key::string_to_enum` /
  `FIELD_NAMES` membership already present.)
- **cli `ghostty.rs::help_text`**: return `action_help(self)`.

---

## 4. Un-defers landing in this milestone

- **(a) `explain_config` "explain" test — R1 CONTRACT, un-defer.** Restore the live
  `#[test]` (`explain_config.rs:288-305`) and drop `apc_deferred` 1→0 for
  `cli_explain_config` in `check_test_counts.py:277`. **Must pass.**
- **(b) `FileFormatter --docs`** (config) + **`show_config --docs`** (cli, 0-test) —
  real emission via `config_help`.
- **(c) `list_keybinds --docs`** (cli, 0-test) — via `Keybinds::format_entry_docs`
  → `keybind_help`.
- **(d) `+help` action text** (cli, 0-test) — `help_text` → `action_help`.
- **Prerequisite (largest item): port the 209 Config field `///` doc blocks** into
  `config_struct.rs` (the fill sub-batch), gated by scoped-G3b
  (`--ref-files config/Config.zig --port-glob …/config_struct.rs` → missing=0),
  which also clears the current whole-crate G3b RED.

Sequence: **port field docs → build shared `rustty-helpgen` + 3 build.rs → wire
seams → un-defer (a)–(d)**.

---

## 5. Test / verification

- **R1 proof:** `explain_config` "explain" passes un-deferred — asserts
  `explain_option(b"font-size").is_some()`, `explain_keybind(b"copy_to_clipboard")
  .is_some()`, plus the four negatives. This transitively proves the Config fill +
  both generators.
- **Generator unit tests (rustty-only, in each owning crate):**
  `config_help(Key::FontSize).is_some()` **and** a golden snapshot of the
  `font_size` entry byte-equal to the ported, dedented text (guards
  `find_common_prefix` parity); same for `keybind_help("copy_to_clipboard")` and a
  known `action_help`. These are the port's own regression net for the dedent/scan.
- **Consistency:** generated-from-source ⇒ no drift. Optional new gate: assert
  every reference-documented Config field (from `comments-census.json` /
  `Config.zig`) has a non-`None` `config_help` entry — this ties the fill
  sub-batch's completeness to the generator (catches a silently-unported field
  doc that the fuzzy G3b would mask).
- **R3 proof for the fill:** scoped-G3b on the field-doc fill = missing=0.

---

## 6. Risks

1. **The fill is the real work, and G3b under-reports it.** ~3300 doc lines / 209
   fields; the generator is small by comparison. Whole-crate G3b hard-flags only
   **25** lines (coincidental vocabulary overlap masks the rest) — **do not scope
   the fill to the 25**; port every field's doc. Use the census (Config.zig
   doc-decl = 3625) as the completeness target, and the optional gate in §5.
2. **Dedent/format parity.** Replicate `[3..]` + `findCommonPrefix` exactly:
   min-leading-space prefix across non-blank lines; blank `///` lines contribute
   nothing to the prefix and re-emit empty; indented example lines (e.g.
   `///   rustty +explain-config …`) keep their *relative* indent; consumers drop a
   trailing empty last line (`splitScalar` + peek). Scan raw `///` source, **not**
   `#[doc]` attrs. `[:0]const u8` NUL-termination → Rust `&[u8]` (content-faithful,
   no NUL needed).
3. **Field↔Key mapping.** Key the Config table by `Key` via the landed
   `CONFIG_FIELDS`; `_`-prefixed internals already excluded there. Undocumented
   field → `None` (== `@hasDecl` false), matching the reference.
4. **Keybind doc presence.** Verified present (770 ≥ 700); undocumented actions →
   `None`; `_`-variants skipped. Re-confirm no documented action regressed during
   wiring.
5. **build.rs vs committed-generated.** Use `build.rs` (regenerate, no drift,
   matches the reference's non-committed generation) + `rerun-if-changed`.
   Committed-generated rejected (drift + R3 self-consistency hazard).
6. **Action-table extraction differs** (doc above `pub fn run`, not a field): the
   scanner needs an `fn-run` mode; key by the action module directly (rustty
   collapses ghostty's `Action.file()` `-`→`_` filename map — the `.rs` module name
   already *is* the underscored name).
