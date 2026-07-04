# GPU R-d — Chunk 3a: `DerivedConfig` prerequisites for `GenericRenderer`

> **COUNT CORRECTION (post-validation):** the reference `generic.zig` DerivedConfig has **8** `TerminalColor`-typed fields (not 7), so **10** non-hoistable config-typed fields (8 TerminalColor + `Path` + `RepeatablePath`), and 6 hoisted + 1 `AlphaBlending` + 10 = **17** total. Earlier prose in this doc (and commits bd2c2eb/82e6a34) said ×7 / 9 — an arithmetic slip; the ported CODE correctly declares all 8 fields. Counts corrected throughout.


**Sub-decision architect deliverable.** Parents: `GPU_RD_BLUEPRINT.md` (ACCEPT-WITH-FIXES),
`GPU_RD_TRAIT_DESIGN.md` (monomorphic decision, §D struct field list). This doc pins the
*prerequisite* work that must land before `generic.zig`'s `Renderer` struct (§D) can be
authored: the `config -> renderer` cycle break for every `DerivedConfig` config-typed field, and
the missing renderer-local types the struct names. It is written to be adversarially reviewed and
was written *because* `GPU_RD_TRAIT_DESIGN.md §D` gives a **naive** field list that is unbuildable
as-stated — it names `apprt::surface::Mailbox` (does not exist + cycles), `AtomicHealth`/
`DisplayLink` (do not exist), and it implies all ~17 config-typed `DerivedConfig` fields are hoisted
like `AlphaBlending` when **9 of them are not hoistable**. This doc resolves each seam explicitly.

Scope note: R1 (PORT_RULES) requires 100% behavioral/comment/test fidelity. R4 requires the crate
graph stay ACYCLIC (gate `check_acyclic.sh`); the `config -> renderer` back-edge
(`Message::ChangeConfig`, `rustty-config/Cargo.toml`) means **the renderer can name no
`rustty_config::*` type**, and `rustty-apprt-api` depends on `rustty-renderer`
(`rustty-apprt-api/Cargo.toml:33`) so **the renderer can name no `apprt` type either**.

---

## Chunking

- **Part A (implemented in this task):** hoist the 6 *clean* config value-types
  (`WindowColorspace`, `WindowPaddingColor`, `BackgroundImagePosition`, `BackgroundImageFit`,
  `FontShapingBreak`, `BackgroundBlur`) to `rustty-render-types`, repeating the Chunk-2A
  `AlphaBlending` hoist (commit `5b9eb65`) adapted per type shape. Touches **only**
  `rustty-config` + `rustty-render-types`. No renderer edit.
- **Part B (IMPLEMENTED — `crates/rustty-renderer/src/generic.rs`):** author `AtomicHealth` + the
  macOS `DisplayLink` placeholder, uncomment `terminal::Scrollbar`, port `renderer::Options` (minus
  its apprt-typed fields) and the monomorphic `Renderer` struct (§D) with the surface-mailbox seam,
  the generic-`C` `DerivedConfig`, and the nested `SwapChain`/`FrameState`/`CustomShaderState`.
- **Part C (IMPLEMENTED):** the Chunk-3a method set — `SwapChain::{init,deinit,next_frame,
  release_frame}` (semaphore backpressure), `FrameState::{init,resize}`,
  `CustomShaderState::{init,resize,swap}`, and the `Renderer::{init,deinit}` deferred seams. The big
  CPU methods (`updateFrame`/`rebuildCells`/`drawFrame`/uniforms math + `initShaders`/`deinitShaders`/
  `updateFontGrid*`) are NOT declared (a later chunk). See **Part C** below for the concrete decisions.

Part A landed first; Parts B+C landed on top of the hoisted `rustty-render-types` types. All gates
green: `cargo build -p rustty-renderer`, `cargo build --workspace`, `cargo fmt --check`, `cargo
clippy -p rustty-renderer --all-targets -- -D warnings` (NO dead_code), `cargo test -p
rustty-renderer` (28 pass), `check_acyclic.sh`, `check_test_counts.py`, `check_comment_coverage.py`.

---

## Part A — the `DerivedConfig` config-type hoists (IMPLEMENTED)

### A.0 Why only 6, and the exact precedent

`generic.zig`'s `DerivedConfig` (536–651) names these config value-types as field types:
`FontShapingBreak`, `TerminalColor`×8 (`cursor_color`/`cursor_text`/`selection_background`/
`selection_foreground`/`search_background`/`search_foreground`/`search_selected_background`/
`search_selected_foreground`), `WindowPaddingColor`, `RepeatablePath` (`custom_shaders`), `Path`
(`bg_image`), `BackgroundImagePosition`, `BackgroundImageFit`, `WindowColorspace`, `AlphaBlending`
(done, Chunk 2A), `BackgroundBlur`. The renderer cannot name any of them (cycle). Two faithful
fixes exist (both are in-tree precedents): **hoist** the type to the `rustty-render-types` leaf
(depended on by BOTH config and renderer, so no new edge), or **generic-`C` associated extraction**
(the `renderer/Thread.zig` `DerivedConfig::init<C>` precedent, `thread.rs:69`).

A type is **hoistable** iff it is a plain value with **no dependency the leaf cannot take** and its
foreign-trait impls can be relocated without an orphan-rule violation. Exactly the 6 above qualify
(+ `AlphaBlending`, done). The other 10 do **not** (§B.1) and take the generic-`C` route.

The precedent is commit `5b9eb65` (`AlphaBlending`), verified end-to-end:
1. Move the type def (+ its doc/`// pub const …` comments) to `rustty-render-types/src/lib.rs`.
2. Move its `DeepEqual` impl out of `rustty-config/src/deep_equal.rs` into render-types (orphan
   rule: `DeepEqual` is a `rustty-datastruct` trait, so once the type is foreign to config the impl
   must live in the type's home crate; render-types already depends on datastruct).
3. `pub use rustty_render_types::X;` in `value_types.rs`, **keeping the `// pub const X = …`
   reference comment above the re-export** so config comment-coverage keeps a copy (see A.4).
4. The config-facing parse/format/C-API machinery **stays in config**, converted from
   inherent/macro methods on the (now-foreign) type to **config-local free fns** (the orphan rule
   forbids inherent `impl X` on a foreign type; a free fn over a foreign type is legal). Call sites
   flip to the free-fn macro arms.

### A.1 Per-type shape (they are NOT all bare enums)

| type | shape | config machinery to relocate |
|---|---|---|
| `WindowColorspace` | bare enum | `bare_enum_names!` -> 3 free fns |
| `WindowPaddingColor` | bare enum | `bare_enum_names!` -> 3 free fns |
| `BackgroundImagePosition` | bare enum | `bare_enum_names!` -> 3 free fns |
| `BackgroundImageFit` | bare enum | `bare_enum_names!` -> 3 free fns |
| `FontShapingBreak` | packed struct `{ cursor: bool = true }` | `Default`, `PackedStruct` (parse), `to_c_uint` (C-API), `FormatEntry` (format) |
| `BackgroundBlur` | `union(enum) { false, true, macos-glass-regular, macos-glass-clear, radius: u8 }` | `parse_cli`, `enabled`, `cval`, `format_entry`, `FormatEntry`, hand-`DeepEqual` |

**The 4 bare enums** are byte-for-byte the `AlphaBlending` case. For each: drop the
`bare_enum_names!(X { … })` invocation; add config-local free fns `x_string_to_enum`, `x_to_name`,
`x_to_name_z` (the exact 3 methods that macro emitted, verbatim literals); flip:
- parse: `field_enum!(x, X::string_to_enum)` -> `field_enum!(x, x_string_to_enum)`
- format: `f_enum!(x)` -> `f_enum!(x, x_to_name)` (existing 2-arg arm)
- C-API: `g_enum!(x)` -> `g_enum_fn!(x, x_to_name_z)` (existing arm)

**`FontShapingBreak`** hits three orphan walls the bare enums do not — its parse/format/C-API go
through traits/macros that live in *other* crates:
- `PackedStruct` is defined in **`rustty-args`** (`args.rs:1033`), not config. `impl PackedStruct
  for FontShapingBreak` is legal today (local type) but orphans once the type is foreign. Fix: a
  **config-local newtype** `FontShapingBreakParse(FontShapingBreak)` carries the `PackedStruct`
  contract (local type -> legal) and the parse handler becomes
  `parse_packed_struct::<FontShapingBreakParse>(value)?.0`. This reuses the single ported
  `parse_packed_struct` (no logic/comment duplication). Pure cycle-break glue (R1 "no-loss, not
  no-addition").
- `to_c_uint` is emitted by the config macro `packed_to_c_uint!` as an **inherent** `impl
  FontShapingBreak` — orphans. Fix: config-local free fn `font_shaping_break_to_c_uint(v) ->
  c_uint` + a new `g_packed_fn!` C-API arm (mirrors the existing `g_enum_fn!`); flip
  `g_packed!(font_shaping_break)` -> `g_packed_fn!(font_shaping_break, font_shaping_break_to_c_uint)`.
- `FormatEntry` is defined in **`rustty-cfg-fmt`** (`formatter.rs:94`), not config — `impl
  FormatEntry for FontShapingBreak` orphans. Fix: config-local free fn
  `font_shaping_break_format_entry` (calls `rustty_cfg_fmt::format_entry_packed_struct`, which
  config already imports) + a new `f_entry!(f, fn)` 2-arg arm; flip `f_entry!(font_shaping_break)`
  -> `f_entry!(font_shaping_break, font_shaping_break_format_entry)`.
- `Default` (`{ cursor: true }`) is a std trait -> **moves to render-types** with the type (orphan).
- `DeepEqual` (currently in the `deep_equal_via_eq!` struct-arm list) -> **moves to render-types**.

**`BackgroundBlur`**: `FormatEntry` orphans (as above) and its inherent methods must relocate:
- `parse_cli`, `enabled`, `cval`, `format_entry` -> config-local free fns
  `background_blur_{parse_cli,enabled,cval,format_entry}` (**kept in config**, see A.4 — their bodies
  carry distinctive reference comments that must stay in config-provenance).
- `impl FormatEntry for BackgroundBlur` -> removed; `f_entry!(background_blur)` ->
  `f_entry!(background_blur, background_blur_format_entry)`.
- parse: `field_parsecli_self!(background_blur)` -> `field_parsecli_self!(background_blur,
  background_blur_parse_cli)` (new fn-form arm calling `bg_blur_parse_cli(&mut self.$f, value)`).
- C-API: `g_cval!(background_blur)` -> `g_cval_fn!(background_blur, background_blur_cval)` (new arm).
- hand-written `impl DeepEqual for BackgroundBlur` (union arm) -> **moves to render-types**
  (`a.deep_equal(b)` for the `u8` radius resolves via datastruct, already a render-types dep).
- `background_blur_enabled` currently has **no caller** in-tree (the apprt blur-enable path is not
  yet ported); annotate `#[allow(dead_code)]` + PORT NOTE (sanctioned staged-port pattern) so the
  `-D warnings` gate stays green. It is R1-ported verbatim, not stubbed.

### A.2 Why `enabled`/`cval`/`to_c_uint` stay in config and are NOT moved to render-types

`AlphaBlending::is_linear` moved to render-types because **the renderer calls it**
(`generic.zig:740` `blending.isLinear()`). By contrast the renderer reads `background_blur` by
matching its variants (`generic.zig:1399`) and `font_shaping_break` by reading `.cursor`
(`:2672`) — it **never** calls `enabled`/`cval`/`to_c_uint`. Those are C-API/apprt projections whose
only callers are config's `c_get`. Two independent reasons keep them in config:
1. **Comment coverage (G3b).** `check_comment_coverage.py` is *per-crate*: every distinctive
   `Config.zig` reference comment must appear somewhere in `rustty-config`'s `.rs` files. `cval`
   carries verbatim reference sentences (e.g. *"I hate sentinel values like this but this is only
   for our macOS application currently…"*) and `enabled` (*"We treat these as true because they
   both imply some blur!"*). Moving them to render-types would drop those from config's provenance
   -> RED. `is_linear` was safe to move only because its inline comments are sub-distinctive and a
   duplicate `// pub const AlphaBlending = …` stayed in config.
2. **Consumer-driven minimalism.** render-types should receive only what a render-types *consumer*
   needs to name/call. The renderer needs the *type*; it does not call these fns.

So render-types receives, per type, **only**: the type def, its `DeepEqual` impl, and (for
`FontShapingBreak`) its `Default`. Everything else stays config-local.

### A.3 Acyclicity (R4) — no new edge

The 6 types are plain values. Their render-types footprint pulls in **no new dependency**:
`DeepEqual` is `rustty-datastruct` (already a render-types dep), `Default` is std, `c_uint`/`i16`
are std. Critically none of the 6 touches `rustty_vt`/`rustty_args`/`rustty_cfg_fmt` in its *type or
DeepEqual* — the args/cfg-fmt couplings live only in the config-local parse/format free fns, which
stay in config. So `check_acyclic` stays GREEN (this is exactly why these 6 are the hoistable set;
contrast `TerminalColor` in §B.1, whose `to_terminal_rgb` returns `rustty_vt::color::RGB` and would
force `render-types -> vt`, a cycle since `vt -> render-types` already exists,
`rustty-vt/Cargo.toml:48`).

### A.4 Parity-gate attribution (G3b / test counts) — kept GREEN by construction

- **Type-def comments** (`// pub const X = …`, and `/// See "font-shaping-break" for
  documentation`) are distinctive `Config.zig` comments. They move to render-types *with the type*,
  and a **duplicate is retained in config above each `pub use`** (the `AlphaBlending` pattern) so
  config coverage keeps a copy. render-types coverage is unaffected (it only checks
  `renderer/size.zig`; extra comments are harmless).
- **Method comments** (parse/format/enabled/cval bodies) stay in config on the free fns -> config
  provenance preserved.
- **Test counts.** The adversary premise *"None of these six carry a Zig `test{}`"* is **wrong**:
  `Config.zig` has `test "parse BackgroundBlur"` (ported at `value_types.rs:8076`
  `parse_background_blur`). But the conclusion holds: that test is `Config.zig`-provenance ->
  **stays in config**, its calls flip from `v.parse_cli(…)` to `background_blur_parse_cli(&mut v,
  …)`. No test moves; `check_test_counts` (config_struct / renderer_size) is unchanged -> GREEN.
  The other 5 types have no `test{}`.

---

## Part B — the `GenericRenderer` struct prerequisites (DESIGNED, deferred)

### B.1 The 10 NON-hoistable `DerivedConfig` fields — generic-`C` associated extraction

`TerminalColor` (8 fields), `Path` (`bg_image`), `RepeatablePath` (`custom_shaders`) are **not
hoistable**:
- `TerminalColor = enum { Color(Color), CellForeground, CellBackground }` and `Color::to_terminal_rgb()`
  returns `rustty_vt::color::RGB` (`value_types.rs:1822,2021`). Hoisting it drags `Color`, which
  drags `vt::color::RGB` -> `render-types -> vt` = **cycle** (`vt -> render-types` exists). ✗
- `Path`/`RepeatablePath` are heap/arena types over `rustty-args` + `rustty-cfg-fmt` + `Allocator`
  (`path.rs`); hoisting adds `render-types -> args/cfg-fmt` edges and breaks the leaf invariant. ✗

Route: **`DerivedConfig` is ported generic over `C`** (exact `renderer/Thread.zig`
`DerivedConfig::init<C>` precedent, `thread.rs:69`; `Message::ChangeConfig` binds `C =
rustty_config::Config` in `core` higher up). The 3 field *types* that cannot be named become
`C`-associated: the struct stores the **already-rustty-nameable** results, not the config types —
`TerminalColor` values are stored as their renderer-facing form and `custom_shaders`/`bg_image` as
renderer-local `link`/`image` inputs. Because `DerivedConfig::init` is out of Part-A/Part-B scope
(it lands with the draw path), these fields are **inhabited, default-constructible placeholders with
a PORT NOTE** in the Part-B struct, exactly like the existing `thread::DerivedConfig` (`thread.rs:53`).

**GOLDEN-GATE PROOF (why a naive placeholder is unsafe, and the required fix).** Enumerated reads of
the config-typed `DerivedConfig` fields on the Chunk-4 offscreen draw path (`generic.zig`):
`min_contrast` (725,1864), `colorspace` (739,1867), `blending` (740,741,1868,1869,1886,1901,1905) —
pinned by FIX-1 and satisfied by the **hoisted** `AlphaBlending`/`WindowColorspace` + the `f32`
`min_contrast`. **But the draw path ALSO reads the `TerminalColor` fields**: `cursor_text` (2061,
2539), `selection_background`/`selection_foreground` (2071,2081,2808,2852), `cursor_color` (2480),
`search_background`/`search_foreground`/`search_selected_*` (2814,2820,2858,2864), and
`padding_color` (2344,2634), `bg_image*` (1749,1981–2000). Therefore **any golden corpus that
exercises a cursor/selection/search-colored cell reads a real `TerminalColor`**; a
zero/`undefined` placeholder would render wrong pixels and fail the gate. **Conclusion:** when
`DerivedConfig::init` and the golden gate land (Chunk 4), the 10 fields MUST carry **real values via
the generic-`C` extraction** (`init<C>` reading `config.@"cursor-color"` etc., concrete-bound in
core), NOT a placeholder. For Part B (struct only, `init` deferred) the inhabited placeholder is
sound *because nothing reads it yet* — the PORT NOTE records the Chunk-4 obligation. The default
`padding_color`/`bg_image_*` fields (hoisted enums / deferred `Path`) follow the same rule.

### B.2 Surface-mailbox seam (`GPU_RD_TRAIT_DESIGN.md §D is wrong here)

§D writes `surface_mailbox: apprt::surface::Mailbox`. This is **unbuildable**: (a) `apprt::surface::
Mailbox` does not exist — apprt-api replaced the reference struct with the `AppMailbox` **trait**
(`apprt-api/src/surface.rs:181`); (b) `rustty-apprt-api -> rustty-renderer`
(`apprt-api/Cargo.toml:33`), so a `renderer -> apprt-api` edge cycles (`check_acyclic` RED).
`generic.zig` pushes exactly two messages onto it: `.scrollbar` (`generic.zig:1454`) and
`.renderer_health` (`generic.zig:1735`), both inside `drawFrame`. Since `drawFrame` (the only
reader/writer) is **out of Chunk-3a scope**, Part B **defers `surface_mailbox` as an inhabited,
default-constructible placeholder field with a PORT NOTE** (the `thread::DerivedConfig` precedent) —
NOT typed `apprt::surface::Mailbox`. When `drawFrame` lands it routes through a **renderer-local
mailbox trait** mirroring only those two messages, bound to apprt-api's `AppMailbox` concretely in
`core` (the `DerivedConfig::init<C>` precedent). Correspondingly, `renderer::Options` is ported in
Part B **without** its three apprt-typed fields (`surface_mailbox: apprt.surface.Mailbox`,
`rt_surface: *apprt.Surface`, `thread: *renderer.Thread`, per `renderer/Options.zig`); they are
deferred/routed through the same seam.

### B.3 Missing renderer-local types to author (Part B)

- **`AtomicHealth`** — port of `std.atomic.Value(Health)` (`generic.zig:203` `= .{ .raw = .healthy
  }`; `drawFrame` load/store at 1461ff). `Health` already exists (`rustty-renderer/src/lib.rs:123`,
  `enum(c_int) { healthy, unhealthy }`). Author a newtype over `AtomicU8`/`AtomicI32` in
  `rustty-renderer` with `seq_cst` load/store and `new(Health::Healthy)`.
- **`DisplayLink`** — `generic.zig:39` `switch(os){ .macos => *macos.video.DisplayLink, else =>
  void }`, field `display_link: ?DisplayLink = null` (`:200`). Author a macOS placeholder type +
  `display_link: Option<DisplayLink>` defaulting `None` (unimplemented seam; the CVDisplayLink loop
  driver lands with `loopEnter`).
- **`terminal::Scrollbar`** — the top-level alias is commented out at `rustty-vt/src/lib.rs:128`
  (`// pub const Scrollbar = PageList.Scrollbar;`); the struct exists `pub` at
  `pagelist/grow.rs:688`. Part B uncomments the alias (or names `pagelist::grow::Scrollbar`).

### B.4 Dead-code (`-D warnings`) for the deferred struct internals

The nested `SwapChain`/`FrameState`/`CustomShaderState` are private in Zig; their methods
(`init`/`deinit`/`nextFrame`/`releaseFrame`/`resize`/`swap`) are reachable only from
`Renderer::init`/`deinit`/`drawFrame`, all deferred — so rustc sees "never used". The `FrameState`
`_modified: usize` fields (`grayscale_modified`/`color_modified`/`target_config_modified`/
`bg_image_buffer_modified`) are written in `init`, read only in `drawFrame` -> "field never read".
Part B annotates these types/fields `#[allow(dead_code)]` + a PORT NOTE citing the staged port,
**or** makes them `pub` (pub items in a lib crate are dead-code-exempt). Required for the gate.

### B.5 Confirmed-OK (verified, no change needed)

- `frame_sema: Semaphore::with_permits(SWAP_CHAIN_COUNT)` compiles — `with_permits` is a `pub const
  fn` (`rustty-zigstd/src/thread.rs:187`).
- `[FrameState; SWAP_CHAIN_COUNT]` is expressible monomorphically as `[FrameState; 3]` with `const
  SWAP_CHAIN_COUNT: usize = 3` (§D array-length note, monomorphic arm).
- `generic.zig` has **zero** `test{}` blocks -> R1 test-fidelity is trivially met;
  `check_test_counts` is moot for the struct port.
- The non-config struct fields are all renderer-nameable: `background`/`foreground =
  rustty_vt::color::RGB`, `bold_color = rustty_vt::style::BoldColor`, `font_styles = rustty_font`,
  `links = renderer-local link::Set`.
- `crate::metal::Metal`, `RenderState::empty()`, `SearchMatches`/`SearchMatch` all exist.

---

## Part C — implementation decisions (IMPLEMENTED, `generic.rs`)

Concrete choices made when landing the struct + method set, all gate-green:

- **Dead-code (`-D warnings`) — the "make pub" route, ZERO `#[allow(dead_code)]`.** §B.4 offered two
  options; the implementation takes the second (make the private nested types `pub`). `Renderer`,
  `SwapChain`, `FrameState`, `CustomShaderState` (and `HighlightTag`/`Error`/`Defunct`/`DisplayLink`/
  `SurfaceMailbox`) are `pub` with `pub` fields + `pub` methods, reachable via `crate::generic::*`
  (`pub mod generic`). Pub items reachable from the crate root are dead-code-exempt, so NO per-item
  `#[allow(dead_code)]` is needed even though every method is only reached from the deferred
  `Renderer::init`/`drawFrame`. The Zig-private → Rust-pub widening is documented per type (PORT
  NOTE). `DerivedConfig` fields are likewise `pub` (the reference reads `config.<field>`). The lone
  `#[allow(clippy::extra_unused_type_parameters)]` is on `DerivedConfig::init<C>` (the deferred seam's
  unused `C`).
- **Cross-platform split.** `generic.rs` is NOT `#[cfg]`-gated: `DerivedConfig` + its placeholders +
  `Options` + `AtomicHealth` name no Metal type and stay cross-platform (so the cross-platform
  `message.rs` `Message::ChangeConfig` can name `DerivedConfig`). The Metal-dependent items (`Renderer`,
  `SwapChain`, `FrameState`, `CustomShaderState`, `Error`, `Defunct`, `DisplayLink`, `SurfaceMailbox`,
  `HighlightTag`) live in an inner `#[cfg(target_os = "macos")] mod macos_impl` with a glob re-export.
  `pub use generic::Renderer;` at the crate root is macOS-gated; `pub use generic::DerivedConfig;` is not.
- **`RendererDerivedConfig` placeholder replaced.** The uninhabited `pub enum RendererDerivedConfig {}`
  in `lib.rs` is removed; `message.rs`'s `Box<RendererDerivedConfig>` becomes `Box<DerivedConfig>` (the
  real inhabited struct). `initChangeConfig`/the `deinit` arm stay deferred (config-coupled), so
  `ChangeConfig` is still not constructible via the public API — comments updated to say so.
- **Aggregate `Error` (macOS).** The reference `!T` inferred unions map to `enum Error { MetalFailed,
  OutOfMemory }` with `From` impls for the buffer/texture/sampler/target leaf errors + the font
  `Atlas` `OutOfMemory`. This is the R1 cycle-break glue that lets `?` compose the leaf errors (the
  richer whole-draw-path `RendererError` is a later chunk). `Renderer::init` also returns it.
- **deinit chain → Rust `Drop`.** The Metal resource leaves (Buffer/Texture/Sampler/Target) release
  via `Retained`/`CFRetained` `Drop` — none expose a manual `deinit`. So the reference
  `FrameState.deinit`/`CustomShaderState.deinit` (which call `.deinit()` on each field) map to
  automatic field `Drop` (no method; verbatim Zig bodies preserved as comments). `SwapChain::deinit`
  keeps the meaningful logic — the `defunct` double-free guard + the `for (0..buf_count)
  frame_sema.wait()` in-flight-draw drain — and the per-frame release is the `frames`-array `Drop`
  after it returns.
- **Semaphore wiring.** `frame_sema: Semaphore::with_permits(SWAP_CHAIN_COUNT)` is set in
  `SwapChain::init`; `next_frame` does `frame_sema.wait()` + `(frame_index + 1) % SWAP_CHAIN_COUNT`;
  `release_frame` does `frame_sema.post()`; `deinit` drains `SWAP_CHAIN_COUNT` waits. `const
  SWAP_CHAIN_COUNT: usize = Metal::SWAP_CHAIN_COUNT` (= 3, monomorphic). `[FrameState;
  SWAP_CHAIN_COUNT]` is built fallibly via a `Vec` → `try_into` (no stable `array::try_from_fn`),
  preserving the `for (&result.frames)` loop.
- **`FrameState::init` atlas realization.** Zig passes a throwaway `*const font.Atlas`
  (`.{ .data = undefined, .size = 1, .format = ... }`) to `initAtlasTexture` (which reads only
  `.size`/`.format`). rustty's `Atlas` has private fields (only `Atlas::init(alloc, size, format)` is
  public), so the throwaway is realized as a real 1×1 `Atlas` over `Global`; the transient allocation
  is immaterial (texture takes size=1/format only) and frees on drop. Documented PORT NOTE.
- **`Renderer::{init,deinit}` deferred seams.** Both are `pub`, `unimplemented!()`, with the full
  verbatim Zig bodies preserved as comments (R1/R5). `init` needs `GraphicsAPI::init` over a live
  surface (the same windowing-host seam as `Metal::init`), the `RepeatablePath`-placeholder
  `custom_shaders` read, `font.Shaper.init`, and the deferred CPU methods — all a later chunk.

---

## Gate plan (Part A)

`cargo build --workspace`, `cargo test --workspace` (config tests incl. `parse_background_blur`,
format, changed, c_get_background_blur stay green), `cargo fmt --check`, `cargo clippy --workspace
--all-targets -- -D warnings`, `check_acyclic.sh` (no new edge — A.3), `check_test_counts.py` (no
test moves — A.4), `check_comment_coverage.py` (distinctive Config.zig comments retained in config —
A.4). STOP + surface a precise blocker if any cannot go green honestly (R5). No `git commit`.
