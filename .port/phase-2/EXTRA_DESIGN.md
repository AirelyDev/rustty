# EXTRA_DESIGN — porting `src/extra/*.zig` (shell completions + editor syntax)

Design phase for the byte-faithful port of ghostty's `src/extra/`:

| ghostty file | LOC | tests | produces | reflects over |
|---|---|---|---|---|
| `bash.zig` | 337 | 0 | bash completion script | Config fields + Action + per-action options |
| `zsh.zig`  | 231 | 0 | zsh completion script  | Config fields + Action + per-action options |
| `fish.zig` | 202 | 1 (`getDescription`) | fish completion script | Config fields (+ help descriptions) + Action + per-action options |
| `vim.zig`  | 121 | 1 (`_ = syntax`) | vim syntax/ftdetect/ftplugin/compiler | Config field NAMES only |
| `sublime.zig` | 50 | 0 | Sublime syntax | Config field NAMES only |

All five are **comptime** generators: they `@typeInfo`-reflect over `Config`'s
struct fields and over the CLI `Action` enum (`@field(Action,name).options()`),
emitting a `pub const` byte string. `main_build_data.zig`'s `+gen-*` actions dump
those consts to files; the generated scripts must be **byte-identical** to what
ghostty emits (R1). The bin/`+gen-*` entry stays deferred (scope note); this chunk
lands the generator functions + fidelity tests + the reflection surface they need.

---

## 1. Disposition: **READY** (requires additive faithful reification — see §4)

Byte-exact reproduction **is** achievable. Every byte the generators emit is
present in already-ported rustty source (enum-variant name literals in
`value_types.rs`, packed-flag name literals in the `PackedStruct` impls, action
`Options` field names/types in `rustty-cli`, declaration orders in
`config_struct.rs`/`Action::ALL`). What is **not yet exposed** is the *ordered,
iterable* form of three of those facts (enum value lists, packed-flag lists,
per-action option specs). Exposing them is the **same faithful reification
pattern** rustty already uses for `Key` / `value_type` / `CONFIG_FIELDS` /
`AutoStruct::field_names` — it re-surfaces information that exists in the port, it
does **not** fabricate output bytes. Per the "do NOT hand-fake" rule: hand-faking
would be hardcoding completion strings; this is reifying the type structure, which
is legitimate and is exactly what `value_type` already does for `Value(key)`.

Hence **ready**, with the explicit precondition that §4's reification additions
land as part of (or just ahead of) the generator. If a reviewer prefers those
additions to be a separate landed prerequisite before `rustty-extra` is written,
that is a sequencing choice, not a fidelity blocker.

---

## 2. What each generator needs from reflection (exhaustive)

### 2a. Config side (bash `_handle_config`, zsh `_config`, fish top loop, vim, sublime)

Iterate `@typeInfo(Config).@"struct".fields` in **declaration order**, skipping
`field.name[0]=='_'`. For each field:

1. **field name** (hyphenated CLI spelling) — vim/sublime need ONLY this.
2. **name special-cases** (checked before type): `startsWith("font-family")` →
   fonts; `eql("theme")` → themes; `eql("working-directory")` → dirs. Generator-
   local string tests on the field name.
3. **`field.type == Config.RepeatablePath`** → files/`-F`/`_files`. Exact type match.
4. **`@typeInfo(field.type)` classification** (the else branch):
   - `.bool` (and `?bool` for the bash `config+=` line / fish `-r` test) → bool arm.
   - `.@"enum"` → emit **each enum value name in declaration order**, space-joined.
   - `.@"struct"` with **`!@hasDecl(parseCLI) && layout==.packed`** → emit **each
     packed field name in declaration order** as `name` (+ `no-name` in bash/zsh/
     fish flag lists).
   - everything else (incl. non-packed struct / struct-with-parseCLI / int / float
     / string / color / path) → the `return`/`( )`/blank catch-all arm.

fish additionally needs, per field, the **help description**:
`@hasDecl(help_strings.Config, name)` → `getDescription(help)`.

### 2b. Action side (bash `_handle_actions`, zsh `(rest)`, fish action loop)

- Iterate `@typeInfo(Action).@"enum".fields` in declaration order → `+<tag>`.
- Per action, `@field(Action,name).options()` → its `Options` struct; iterate
  `@typeInfo(options).@"struct".fields`, skip `_`-prefixed, and per option:
  - `.bool` → bool arm.
  - `.@"enum"` → value names in decl order.
  - `.optional` whose child is `.@"enum"` → value names in decl order.
  - name special-case `eql("config-file")` → files arm.
  - else → return/blank arm.
- Actions whose options struct has **0 non-`_` fields** are skipped in the
  bash/zsh per-action `case` blocks (`fields.len == 0` guard); fish skips `help`
  and `version` by name.
- Exercised enum-option paths (must work): `list-themes` `color: ColorScheme`,
  `show-face` `style: font.Style` (enum) + `presentation: ?font.Presentation`
  (optional enum). `validate-config`/`ssh`/`show-face` `config-file`/`config_file`
  name special-cases.

---

## 3. What the CURRENT rustty surface already exposes (sufficient parts)

| Need | Exposed by | Status |
|---|---|---|
| Config fields in decl order (hyphenated name + `Key`) | `config_struct::CONFIG_FIELDS: &[(&[u8], Key)]` (line 7699) | ✅ exists — but `pub(crate)`, must be re-exported (see §4.0) |
| `Key` decl-order enum + `string_to_enum` | `config_struct::Key` (line 115) | ✅ |
| Per-field concrete type identity | `config_struct::value_type(key) -> TypeId` (line 573) | ⚠️ opaque — usable for `bool`/`Option<bool>`/`RepeatablePath` equality tests, but yields **no** names/kinds |
| Config field help text (bytes) | `help_strings::config_help(key) -> Option<&'static [u8]>` | ✅ — `None` == `@hasDecl==false` exactly; fish `getDescription` runs on the bytes |
| Action variants in decl order + tag | `Action::ALL: [Action;18]` + `Action::tag_name()` (rustty-cli) | ✅ |
| Enum → its own name/lookup | `bare_enum_names!` → `to_name`/`string_to_enum` per enum | ⚠️ per-variant only; **no ordered `value_names()` list** |
| Packed struct → set one flag | `args::PackedStruct` `set_field`/`set_all` | ⚠️ **no ordered `field_names()` list** |
| Auto struct → field names | `args::AutoStruct::field_names()` (precedent!) | ✅ (proves the pattern) |

**Conclusion:** the surface is **partially** sufficient. Declaration-order names,
bool detection, `RepeatablePath` detection, Action variants, and fish help text
are all present. The **missing** pieces are the *ordered value/flag/option-field
lists* + the *per-field kind classification*. `value_type`'s `TypeId` cannot yield
enum values or packed-flag names or distinguish packed-flag-struct from struct-
with-parseCLI, and `args::Target`/`PackedStruct`/`bare_enum_names!` expose only
string-dispatch parse, not ordered iteration. `Action::options()` is currently a
commented-out stub (Zig returned a comptime *type*).

---

## 4. Reflection additions the design specifies (each a faithful analog)

Each addition re-exposes existing ported data as an ordered list — the established
rustty reification method (cf. `value_type` reifies `Value(key)`; `Key` reifies
`std.meta.fields`; `AutoStruct::field_names` reifies `info.fields`).

**4.0 — Expose `CONFIG_FIELDS`.** Make `config_struct::CONFIG_FIELDS` `pub` (or add
`pub fn config_fields() -> &'static [(&'static [u8], Key)]`). This is the literal
`@typeInfo(Config).@"struct".fields` iteration (already `_`-filtered at the const's
construction). vim/sublime need nothing else.

**4.1 — Ordered enum value names.** Extend `bare_enum_names!` (value_types.rs:175)
to also emit `pub const VALUE_NAMES: &'static [&'static [u8]]` (the same `$name`
literals, in macro/decl order). Faithful to `@typeInfo(E).@"enum".fields |f| f.name`.
Enums NOT built via the macro (`ConfirmCloseSurface`, `Fullscreen`, the hoisted
`WindowPaddingColor`, `ColorScheme`, `font::Style`, `font::Presentation`, …) get an
equivalent const/free-fn next to their def. **Spelling parity** with ghostty's
`f.name` is already guaranteed for the format/parse-fidelity-gated types
(`to_name` == `@tagName`), so `VALUE_NAMES` == the completion bytes.

**4.2 — Ordered packed-flag names.** Add `fn field_names() -> &'static [&'static
[u8]]` to `args::PackedStruct` (mirror `AutoStruct::field_names`); each impl returns
its `set_field` literals in decl order (e.g. `FontSyntheticStyle` →
`[b"bold", b"italic", b"bold-italic"]`). Faithful to the packed `info.fields |f|
f.name`. The bash/zsh/fish `no-` prefix is generator-local.

**4.3 — Per-field completion kind.** Add a `config_struct`-level match, parallel to
`value_type`, that reifies the `@typeInfo(field.type)` switch:
```
pub enum ConfigCompletionKind {
    Bool,                                   // .bool  (and ?bool)
    Enum(&'static [&'static [u8]]),         // .@"enum" → VALUE_NAMES
    PackedFlags(&'static [&'static [u8]]),  // packed, no parseCLI → field_names()
    RepeatablePath,                         // field.type == Config.RepeatablePath
    Other,                                  // struct-with-parseCLI / int / color / path / string …
}
pub fn config_completion_kind(key: Key) -> ConfigCompletionKind
```
One arm per `Key`, pulling `E::VALUE_NAMES` / `P::field_names()` from §4.1/§4.2.
This is the direct analog of `value_type`. (The font-family/theme/working-directory
NAME special-cases stay in the generator, matching ghostty's name-before-type
order.) The bash/zsh/fish/vim/sublime generators share this one classifier.

**4.4 — Per-action option specs (reify `Action.options()`).** In **rustty-cli**
(where the `Options` structs + the Action→Options mapping already live, exactly as
ghostty's `options()` did), add:
```
pub enum OptCompletionKind { Bool, Enum(&'static [&'static [u8]]), OptionalEnum(&'static [&'static [u8]]), Other }
pub struct OptSpec { pub name: &'static [u8], pub kind: OptCompletionKind } // name is the hyphenated CLI spelling; _-prefixed omitted
impl Action { pub fn option_specs(self) -> &'static [OptSpec] { … } }
```
One arm per action, each listing its `Options`' non-`_` fields in decl order with
kind. `show-face`'s `style`/`presentation` pull `font::Style`/`font::Presentation`
value-names (cli already deps font-shaper), so `rustty-extra` needs no font dep.
`config-file` name special-case stays generator-local. This is the faithful port of
the commented-out `options()` (whose stated purpose was "generating shell
completions without duplicating the Action→Options mapping").

---

## 5. Crate home (R4 acyclic): **new crate `rustty-extra`**

```
rustty-extra
  ├── rustty-config   (CONFIG_FIELDS, Key, config_completion_kind, config_help, VALUE_NAMES, PackedStruct::field_names)
  └── rustty-cli      (Action::ALL, tag_name, option_specs)   ── deps ──▶ rustty-config
```
`rustty-cli` already deps `rustty-config`; `rustty-extra` sits strictly **above**
both and nothing depends on it (it is the `+gen-*` leaf). **Acyclic.** ✅

Rationale for a crate over a module:
- Mirrors ghostty's own `src/extra/` namespace (structural fidelity).
- Pure byte producer → **R6 Miri-clean** in isolation; keeps rustty-cli focused on
  dispatch.
- The heavy reflection (§4.4 option specs) stays *in rustty-cli* next to the
  `Options` defs (as `options()` did), exposed via a small `pub` API — so the new
  crate only consumes stable public surfaces, no cross-crate `Options` field
  reflection.

Rejected: a module in `rustty-helpgen` — helpgen is the `build.rs` doc-extraction
leaf (below config); it cannot dep config/cli (would invert the DAG). Rejected: a
module in `rustty-cli` — workable, but bloats cli and loses the R6-pure isolation;
kept as the fallback if a new crate is undesired.

---

## 6. Module & API shape of `rustty-extra`

ghostty exposes `pub const completions/syntax = comptime…`. Rust cannot
comptime-fold a `const`, so these become **runtime pure fns returning `Vec<u8>`**
(byte-slices, **never** lossy `String` — completion scripts are bytes; the memory
`zig-byteslice-to-rust-rawbytes` applies):

```
rustty-extra/src/
  lib.rs
  bash.rs     → pub fn completions() -> Vec<u8>            // writeBashCompletions
  zsh.rs      → pub fn completions() -> Vec<u8>            // writeZshCompletions
  fish.rs     → pub fn completions() -> Vec<u8>            // writeCompletions
                fn get_description(help: &[u8]) -> Vec<u8> // + ported test
  vim.rs      → pub fn syntax() -> Vec<u8>; pub const FTDETECT/FTPLUGIN/COMPILER: &[u8]  // + ported `_ = syntax` test
  sublime.rs  → pub fn syntax() -> Vec<u8>                 // header ++ keywords ++ footer
```
Each `write*` fn takes `&mut Vec<u8>` (the `std.Io.Writer` analog) and transcribes
the reference line-for-line (R1), driving §4's classifiers. The `comptimeGenerate*`
two-pass counter/fixed-buffer wrapper collapses to a single `Vec` (the buffer-size
pre-count is a Zig comptime-alloc artifact with no Rust analog — noted in a PORT
NOTE, not reproduced).

The `+gen-bash-completions`/… CLI actions and any `build.rs` file-emit stay
**deferred** (task scope); `rustty-extra` provides the functions + tests only.

---

## 7. Fidelity gates (R1) and tests

- **Ported unit tests (R1 — every `test{}`):** fish `getDescription` (fish.zig:192)
  and vim `_ = syntax` (vim.zig:119) port verbatim into `fish.rs`/`vim.rs`.
- **Byte-exact golden diff (the real R1 gate — ghostty has NO full-script golden
  test, so the port supplies one):** capture ghostty's actual generated
  `bash`/`zsh`/`fish` completion scripts + `vim` syntax + `sublime` syntax as
  fixtures under `crates/rustty-extra/testdata/`, and assert
  `completions()`/`syntax()` equal them byte-for-byte. This is the ONLY check that
  catches an enum/flag/option **ordering** drift (§9 risk).
- **Comment coverage / test counts / acyclic / clippy -D / fmt / Miri** per the
  standard gate set (`check_acyclic.sh`, `check_test_counts.py`,
  `check_comment_coverage.py`, `cargo miri` on the pure crate).

---

## 8. Exact reflection mapping (reference → rustty)

| ghostty comptime | rustty runtime |
|---|---|
| `@typeInfo(Config).@"struct".fields` (skip `_`) | `config_struct::CONFIG_FIELDS` (§4.0) |
| `field.name` | `.0` of the `CONFIG_FIELDS` tuple (hyphenated) |
| `field.type == Config.RepeatablePath` | `config_completion_kind(key) == RepeatablePath` (§4.3) — internally `value_type(key)==TypeId::of::<RepeatablePath>()` |
| `switch(field.type){ bool, ?bool ... }` | `value_type(key) ∈ {bool, Option<bool>}` → `Bool` |
| `@typeInfo(field.type) == .@"enum"` + `info.fields\|f\|f.name` | `Enum(E::VALUE_NAMES)` (§4.1) |
| `.@"struct"` + `!@hasDecl(parseCLI) && layout==.packed` + `info.fields\|f\|f.name` | `PackedFlags(P::field_names())` (§4.2/§4.3) |
| else | `Other` |
| `@hasDecl(help_strings.Config, name)` / `@field(...)` | `config_help(key): Option<&[u8]>` (Some/None) |
| `getDescription(help)` | `fish::get_description(&[u8]) -> Vec<u8>` |
| `@typeInfo(Action).@"enum".fields` | `Action::ALL` |
| `field.name` (action) | `Action::tag_name()` |
| `@field(Action,name).options()` + `info.fields` (skip `_`) | `Action::option_specs()` (§4.4) |
| option `.bool` / `.@"enum"` / `.optional(enum)` / else | `OptCompletionKind::{Bool,Enum,OptionalEnum,Other}` |
| `eql("config-file", opt.name)` / `eql("theme")` / `startsWith("font-family")` / `eql("working-directory")` | generator-local name tests (unchanged) |

---

## 9. Key decisions & risks

**Decisions:** (1) new crate `rustty-extra` above config+cli. (2) runtime `Vec<u8>`
generators, not comptime consts. (3) reify the missing surface via §4 additions
(faithful analogs, authored next to the ported types). (4) golden-fixture byte diff
as the primary R1 gate (ghostty ships no such test).

**Risks:**
- **R-ORDER (primary):** enum-variant and packed-field ordering in
  `bare_enum_names!`/`PackedStruct` impls must equal ghostty's *declaration* order
  — it is NOT currently gated (format fidelity ignores order). A single transposed
  variant breaks byte-exactness silently. **Mitigation:** the §7 golden diff, which
  exercises every enum/flag in situ.
- **R-SPELL:** enum value spellings must equal `@tagName`; guaranteed for
  format/parse-gated `to_name` types, but the non-macro enums (`ColorScheme`,
  `font::Style/Presentation`, `ConfirmCloseSurface`, `Fullscreen`, hoisted
  `WindowPaddingColor`) need their `VALUE_NAMES` audited against ghostty source.
- **R-PACKED-CLASSIFY:** `config_completion_kind` must send packed-flag structs to
  `PackedFlags` and parseCLI/auto structs to `Other`, matching
  `!@hasDecl(parseCLI) && layout==.packed`. Authored per-field against the ghostty
  type; golden diff catches misclassification.
- **R-OPT-COVERAGE:** `option_specs` must include the enum options
  (`list-themes.color`, `show-face.style/presentation`) and the 0-field skip; golden
  diff covers it.
- **R-DEFER:** `+gen-*` CLI wiring deferred — record in DEFERRALS.md so the scripts
  are generated at packaging time.
