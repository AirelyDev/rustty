# RENDERER SURFACE — init / resize / config-swap (fill the deferred surface)

Milestone: fill the deferred `rustty-renderer` config/resize surface —
`DerivedConfig::init<C>`, `changeConfig`, `setScreenSize`, `setFontGrid` (+ the
`markDirty` prerequisite) — while `Renderer::init`'s live-surface attach and the
background-image pipeline stay documented seams.

Reference (READ-ONLY): ghostty `src/renderer/generic.zig` (Zig 0.15.2) —
`DerivedConfig.init` :574, `Renderer.init` :658, `changeConfig` :1841,
`setScreenSize` :1919, `setFontGrid` :1070, `updateScreenSizeUniforms` :1938,
`updateFontGridUniforms` :1115, `markDirty` :997, `prepBackgroundImage` :1747.

Ported files: `crates/rustty-renderer/src/{generic,message,thread}.rs`,
`crates/rustty-render-types/src/lib.rs`, `crates/rustty-config/src/{value_types,config_struct}.rs`.

Rules: PORT_RULES.md R1 (100% fidelity, verbatim bodies preserved as comments),
R4 (ACYCLIC — renderer MUST NOT gain a `rustty-config` edge; `.port/tools/gates/check_acyclic.sh`),
R5 (still-deferred ⇒ `unimplemented!()` seam + verbatim body). G3b comment-coverage
SKIPS rustty-renderer (in progress) → adversary/verify are the only R1 backstop.

> **ADVERSARY REVIEW FOLDED (verdict SOUND-WITH-FIXES, all findings verified
> against source):** (1) `change_config` called a non-existent
> `prep_background_image` — now landed as an explicit R5 `unimplemented!()` seam
> with the verbatim `prepBackgroundImage` body (§3.1); (2) `change_config` leaked
> the `draw_mutex` on the error path — the bare lock/unlock (thread.rs:79/:106, no
> RAII guard) now replicates `defer self.draw_mutex.unlock()` by unlocking on EVERY
> exit (§4.3), because a scope-guard borrowing `&self.draw_mutex` would conflict
> with the `&mut self` calls in the body; (3) `set_font_grid`'s
> `font_shaper_cache.deinit()` takes NO argument (cache.rs:112 `deinit(&mut self)`)
> — corrected in §4.2; (4) RISK-5 confirmed correct (config `RepeatableString.list`
> is `Vec<Vec<u8>>` of non-NUL bytes, value_types.rs:2759/2791 `value.to_vec()`).

---

## 0. The acyclic crux, restated

`rustty-config` **depends on** `rustty-renderer` (`rustty-config/Cargo.toml:48`,
the `Message::ChangeConfig` back-edge). Therefore the renderer may **never name a
`rustty_config` type**. `DerivedConfig::init` reads ~30 `config.*` fields; the
config-coupled seams are already generic over a bare `C`
(`pub fn init<C>(alloc_gpa, config: &C)`), with the concrete `C = rustty_config::Config`
bound higher up. The make-or-break is the accessor mechanism: a renderer-defined
trait whose every return type is renderer-nameable, impl-ed by `Config` **in
rustty-config** (acyclic — config already deps renderer).

### KEY RESULT — zero new render-types hoists required

Every field `DerivedConfig::init` reads maps to a return type the renderer can
already name, because the prior chunks did the hoisting work:

- The 6 config value-types + `AlphaBlending` were hoisted to `rustty-render-types`
  in Chunk 3a Part A (`WindowPaddingColor`, `WindowColorspace`, `BackgroundImagePosition`,
  `BackgroundImageFit`, `FontShapingBreak`, `BackgroundBlur`, `AlphaBlending`).
- `Color` / `TerminalColor` are **renderer-local** (`generic.rs:76/107`) — they name
  `rustty_vt::color::RGB` (renderer already deps vt); hoisting them WOULD cycle.
- `BoldColor` is `rustty_vt::style::BoldColor` (vt is a renderer dep).
- `link` items are `rustty_input::link::Link` (input is a renderer dep).
- `Path` / `RepeatablePath` stay **inhabited placeholders** (`generic.rs:120/130`);
  the background-image + custom-shader features are deferred end-to-end
  (`prepBackgroundImage` + wuffs decode + the real arena `Path` all deferred), so
  these two fields are **not** given accessors — see §3.

No field's only type is a config-crate type once the above are accounted for ⇒
**no new hoist**, `check_acyclic.sh` stays green.

---

## 1. THE ACYCLIC MECHANISM — `trait RendererConfig`

Defined in `rustty-renderer` (proposed home: a new `crate::config` module, or the
top of `generic.rs` next to `DerivedConfig`). One accessor per real config field
`DerivedConfig::init` reads. All return types are renderer-nameable (proven above).

```rust
// crates/rustty-renderer/src/config.rs  (new module; re-exported from lib.rs)
use std::ffi::CString;
use rustty_render_types::{
    AlphaBlending, BackgroundBlur, BackgroundImageFit, BackgroundImagePosition,
    FontShapingBreak, WindowColorspace, WindowPaddingColor,
};
use rustty_vt::style::BoldColor;
use rustty_input::link::Link as InputLink;
use crate::generic::{Color, TerminalColor}; // renderer-local (name RGB, no cycle)

/// The accessor surface `DerivedConfig::init<C>` reads to extract the renderer's
/// derived configuration WITHOUT naming any `rustty_config` type (R4/G7). The
/// concrete `C = rustty_config::Config` impls this IN rustty-config (acyclic:
/// config already deps renderer). Return types are all renderer-nameable
/// (render-types value-types, renderer-local Color/TerminalColor, vt BoldColor,
/// input Link, or std).
pub trait RendererConfig {
    // Scalars / flags (raw config values — the reference's clamps/casts stay in
    // DerivedConfig::init so the arithmetic is faithful in the renderer body).
    fn font_thicken(&self) -> bool;
    fn font_thicken_strength(&self) -> u8;
    fn background_opacity(&self) -> f64;      // init clamps @max(0,@min(1,x))
    fn background_opacity_cells(&self) -> bool;
    fn cursor_opacity(&self) -> f64;          // init clamps @max(0,@min(1,x))
    fn faint_opacity(&self) -> f64;           // init: @intFromFloat(@ceil(x*255))
    fn minimum_contrast(&self) -> f64;        // init: @floatCast -> f32
    fn background_image_opacity(&self) -> f32;
    fn background_image_repeat(&self) -> bool;
    fn window_vsync(&self) -> bool;
    fn scroll_to_bottom_output(&self) -> bool; // config.@"scroll-to-bottom".output

    // font-feature: config `RepeatableString` (Vec<Vec<u8>>) bridged to the
    // renderer's `Vec<CString>` (the [:0]const u8 null-termination). The clone
    // collapses into the owned Vec.
    fn font_features(&self) -> Vec<CString>;

    // font-style-{bold,italic,bold-italic}: config `FontStyle` enum; the
    // reference compares `!= .false`. The comparison is encapsulated in the impl
    // (FontStyle carries a name payload → not renderer-nameable); accessor
    // returns the resulting bool.
    fn font_style_bold(&self) -> bool;         // config.@"font-style-bold" != .false
    fn font_style_italic(&self) -> bool;
    fn font_style_bold_italic(&self) -> bool;

    // Colors — renderer-local Color/TerminalColor (config maps its own
    // Color/TerminalColor -> the renderer's; init calls Color::to_terminal_rgb).
    fn background(&self) -> Color;             // init: .to_terminal_rgb()
    fn foreground(&self) -> Color;             // init: .to_terminal_rgb()
    fn cursor_color(&self) -> Option<TerminalColor>;
    fn cursor_text(&self) -> Option<TerminalColor>;
    fn selection_background(&self) -> Option<TerminalColor>;
    fn selection_foreground(&self) -> Option<TerminalColor>;
    fn search_background(&self) -> TerminalColor;
    fn search_foreground(&self) -> TerminalColor;
    fn search_selected_background(&self) -> TerminalColor;
    fn search_selected_foreground(&self) -> TerminalColor;
    // bold-color: config maps `?BoldColor` via .toTerminal() -> vt BoldColor.
    fn bold_color(&self) -> Option<BoldColor>;

    // Hoisted render-types value-types (config re-exports each; its field IS the
    // hoisted type, so the accessor returns it directly, by value/Copy).
    fn font_shaping_break(&self) -> FontShapingBreak;
    fn window_padding_color(&self) -> WindowPaddingColor;
    fn background_image_position(&self) -> BackgroundImagePosition;
    fn background_image_fit(&self) -> BackgroundImageFit;
    fn window_colorspace(&self) -> WindowColorspace;
    fn alpha_blending(&self) -> AlphaBlending;
    fn background_blur(&self) -> BackgroundBlur;

    // link.links.items -> `link::Set::from_config(alloc, &[InputLink])`.
    // Borrow tied to &self; renderer names InputLink (input is a dep).
    fn links(&self) -> &[InputLink];
}
```

### 1.1 Full field → accessor → return-type table

| # | ref config field (`config.@"…"`) | accessor | return type | nameable because | transform site |
|---|---|---|---|---|---|
| 1 | `font-thicken` | `font_thicken` | `bool` | std | — |
| 2 | `font-thicken-strength` | `font_thicken_strength` | `u8` | std | — |
| 3 | `font-feature`.clone.list | `font_features` | `Vec<CString>` | std | impl bridges `Vec<Vec<u8>>`→`Vec<CString>` |
| 4 | `font-style-bold` | `font_style_bold` | `bool` | std | impl: `!= .false` |
| 5 | `font-style-italic` | `font_style_italic` | `bool` | std | impl: `!= .false` |
| 6 | `font-style-bold-italic` | `font_style_bold_italic` | `bool` | std | impl: `!= .false` |
| 7 | `font-shaping-break` | `font_shaping_break` | `FontShapingBreak` | render-types (hoisted) | — |
| 8 | `cursor-color` | `cursor_color` | `Option<TerminalColor>` | renderer-local | impl maps config TerminalColor |
| 9 | `cursor-text` | `cursor_text` | `Option<TerminalColor>` | renderer-local | impl maps |
| 10 | `cursor-opacity` | `cursor_opacity` | `f64` | std | init: `@max(0,@min(1,x))` |
| 11 | `background` | `background` | `Color` | renderer-local | init: `.to_terminal_rgb()` |
| 12 | `background-opacity` | `background_opacity` | `f64` | std | init: clamp |
| 13 | `background-opacity-cells` | `background_opacity_cells` | `bool` | std | — |
| 14 | `foreground` | `foreground` | `Color` | renderer-local | init: `.to_terminal_rgb()` |
| 15 | `selection-background` | `selection_background` | `Option<TerminalColor>` | renderer-local | impl maps |
| 16 | `selection-foreground` | `selection_foreground` | `Option<TerminalColor>` | renderer-local | impl maps |
| 17 | `search-background` | `search_background` | `TerminalColor` | renderer-local | impl maps |
| 18 | `search-foreground` | `search_foreground` | `TerminalColor` | renderer-local | impl maps |
| 19 | `search-selected-background` | `search_selected_background` | `TerminalColor` | renderer-local | impl maps |
| 20 | `search-selected-foreground` | `search_selected_foreground` | `TerminalColor` | renderer-local | impl maps |
| 21 | `bold-color` | `bold_color` | `Option<BoldColor>` | `rustty_vt::style` | impl: `.toTerminal()` |
| 22 | `faint-opacity` | `faint_opacity` | `f64` | std | init: `@intFromFloat(@ceil(x*255))` |
| 23 | `minimum-contrast` | `minimum_contrast` | `f64` | std | init: `@floatCast` → f32 |
| 24 | `window-padding-color` | `window_padding_color` | `WindowPaddingColor` | render-types (hoisted) | — |
| 25 | `background-image-opacity` | `background_image_opacity` | `f32` | std | — |
| 26 | `background-image-position` | `background_image_position` | `BackgroundImagePosition` | render-types (hoisted) | — |
| 27 | `background-image-fit` | `background_image_fit` | `BackgroundImageFit` | render-types (hoisted) | — |
| 28 | `background-image-repeat` | `background_image_repeat` | `bool` | std | — |
| 29 | `link`.links.items | `links` | `&[rustty_input::link::Link]` | input (dep) | init: `link::Set::from_config` |
| 30 | `window-vsync` | `window_vsync` | `bool` | std | — |
| 31 | `window-colorspace` | `window_colorspace` | `WindowColorspace` | render-types (hoisted) | — |
| 32 | `alpha-blending` | `alpha_blending` | `AlphaBlending` | render-types (hoisted) | — |
| 33 | `background-blur` | `background_blur` | `BackgroundBlur` | render-types (hoisted) | — |
| 34 | `scroll-to-bottom`.output | `scroll_to_bottom_output` | `bool` | std | impl: `.output` field |
| — | `custom-shader`.clone | **no accessor** | — | placeholder `RepeatablePath` (deferred) | init constructs placeholder |
| — | `background-image`.clone | **no accessor** | — | placeholder `Path` (deferred) | init sets `None` |

**NEW HOIST NEEDED: none.** All 34 return types are already renderer-nameable.

### 1.2 The rustty-config impl (acyclic — lands IN rustty-config)

`rustty-config` already deps `rustty-renderer`, so it may name `RendererConfig`
and impl it for its own local `Config` (orphan rule OK: impl in the type's home
crate). No new edge (`check_acyclic.sh` unchanged). Sketch:

```rust
// crates/rustty-config/src/…  (config deps renderer — acyclic)
impl rustty_renderer::config::RendererConfig for Config {
    fn font_thicken(&self) -> bool { self.font_thicken }
    fn font_features(&self) -> Vec<std::ffi::CString> {
        self.font_feature.list.iter()
            .map(|s| std::ffi::CString::new(s.clone()).expect("no interior NUL"))
            .collect()
    }
    fn font_style_bold(&self) -> bool { self.font_style_bold != FontStyle::False }
    fn background(&self) -> rustty_renderer::generic::Color {
        // config Color -> renderer Color (both {r,g,b}: u8)
        rustty_renderer::generic::Color { r: self.background.r, g: self.background.g, b: self.background.b }
    }
    fn cursor_color(&self) -> Option<rustty_renderer::generic::TerminalColor> {
        self.cursor_color.map(map_terminal_color) // config TerminalColor -> renderer TerminalColor
    }
    fn bold_color(&self) -> Option<rustty_vt::style::BoldColor> {
        self.bold_color.map(|b| b.to_terminal())
    }
    fn window_padding_color(&self) -> WindowPaddingColor { self.window_padding_color } // re-exported hoist
    fn links(&self) -> &[rustty_input::link::Link] { &self.link.links }
    fn scroll_to_bottom_output(&self) -> bool { self.scroll_to_bottom.output }
    // … one line each for the remaining fields …
}
```

The impl is compile-verified by `cargo build -p rustty-config`. NOTE: this impl is
NOT yet exercised by core — core carries its OWN `renderer_seam::DerivedConfig`
placeholder (`rustty-core/src/renderer_seam.rs:78`), which will be replaced by the
real `rustty_renderer::DerivedConfig` in a later integration step. Landing the
config impl now is the end-to-end acyclic proof + keeps the real type instantiable
from a real `Config`.

---

## 2. SCOPE — what lands, what stays a seam

### LANDS NOW (acyclic + offscreen-testable)

1. **`trait RendererConfig`** (§1) + the rustty-config impl (§1.2).
2. **`DerivedConfig::init<C: RendererConfig>`** (§3) — full ~30-field extraction
   via accessors; the two placeholder fields (`custom_shaders`, `bg_image`)
   constructed as documented placeholders (§3).
3. **`markDirty`** (`mark_dirty`) — tiny prerequisite (ref :997): sets
   `self.terminal_state.dirty = Dirty::Full`.
4. **`setScreenSize`** (`set_screen_size`) — padding + `update_screen_size_uniforms`
   (§4.1). Minimal in this reference.
5. **`setFontGrid`** (`set_font_grid`) — grid swap + frames-modified reset + cell
   size + `update_font_grid_uniforms` + `mark_dirty` (§4.2).
6. **`changeConfig`** (`change_config`) — shaper reinit + uniform/blending updates
   + swap targets (§4.3). Its single `prep_background_image()` call routes to the
   NEW `prep_background_image` R5 seam landed alongside it (§3.1); the call is
   provably unreachable at runtime under the `bg_image == None` invariant (§4.3 /
   RISK-2). RECOMMENDED to land (precedent: the landed `draw_frame` already routes
   its inspector arm to `unimplemented!()`, `generic.rs:3228`).
7. **`prep_background_image`** (`prepBackgroundImage`, ref :1747) — a NEW R5
   `unimplemented!()` seam whose verbatim reference body is preserved as a comment
   (§3.1). It exists ONLY so `change_config` compiles and its deferred-but-
   unreachable call is a real, documented seam (adversary blocking-#1 fix). It is
   distinct from the whole background-image pipeline, which stays deferred (below).
   `update_bg_image_buffer` already exists (`generic.rs:3470`) — only
   `prep_background_image` was missing.

### STAYS A SEAM (live-surface / whole-feature deferrals)

- **`Renderer::init`** (ref :658) — needs `GraphicsAPI::init(alloc, options)` over
  a **live NSView/IOSurfaceLayer** (windowing-host seam, no offscreen path) +
  `prepBackgroundImage`. NOTE: its `DerivedConfig::init<C>` input is now
  satisfiable, but the surface-attach + prep blockers remain. The offscreen tests
  already bypass `init` by building `Renderer` field-by-field (`test_renderer`
  `generic.rs:4699`), so **no offscreen-init split is added** (it would only
  duplicate `test_renderer`). `Renderer::deinit` stays deferred with it (draw-path
  teardown).
- **`prepBackgroundImage`** (ref :1747) — reads the real arena `Path`
  (`.required/.optional` slice), opens the file, decodes PNG/JPEG via `wuffs`
  (not ported). Whole background-image pipeline deferred.
- **`Message::initChangeConfig<C>`** (message.rs) + **`thread::DerivedConfig::init<C>`**
  (thread.rs) — stay deferred as a unit: `initChangeConfig` calls
  `thread::DerivedConfig::init`, whose body reads `config.@"custom-shader-animation"`
  into `thread::DerivedConfig`'s **deferred** `custom_shader_animation` field
  (thread.rs — the field itself is un-ported). Not blocked by the renderer
  `DerivedConfig::init` (that half is now landable); blocked by the thread-tier
  field. Keep both `unimplemented!()`.

### Gate-green boundary

`cargo build --workspace` + `cargo test -p rustty-renderer` + `cargo build -p rustty-config`
green; `check_acyclic.sh` + `check_leaks.sh` green (no new edge, no new FFI). The
9 `generic.rs` tests + the 40 renderer tests continue to pass, plus the new mock-C
+ resize tests (§5).

---

## 3. `DerivedConfig::init<C>` — faithful body

Replaces the `unimplemented!()` at `generic.rs:284`. The verbatim reference body
(preserved as the comment at `generic.rs:207-282`) maps line-for-line; the two
placeholder-typed fields are the only divergence (documented deferral).

```rust
pub fn init<C: RendererConfig>(
    alloc_gpa: Global,
    config: &C,
) -> Result<DerivedConfig, DerivedConfigError> {
    // var arena = ArenaAllocator.init(alloc_gpa);  (Global is ZST — the arena
    // backs the reference's string clones; here custom_shaders/bg_image are
    // placeholders and font_features is an owned Vec, so the arena owns nothing
    // but is preserved as a struct field for fidelity.)
    let arena = ArenaAllocator::init();
    let _ = alloc_gpa;

    // Copy our shaders.
    // DEFERRED (placeholder): `custom_shaders = config.@"custom-shader".clone(alloc)`.
    // `RepeatablePath` is an inhabited ZST placeholder (custom-shader feature is
    // deferred end-to-end); the real clone lands when `Path`/`RepeatablePath`
    // become real types. (generic.rs:122-130, DEFERRALS.md)
    let custom_shaders = RepeatablePath;

    // Copy our background image.
    // DEFERRED (placeholder): `bg_image = if config.@"background-image" |bg|
    // bg.clone else null`. The background-image pipeline (real `Path` +
    // `prepBackgroundImage` + wuffs decode) is deferred; constructed `None` so
    // the whole feature is a coherent no-op (keeps `changeConfig`'s
    // `bg_image_changed` false ⇒ the deferred prep is unreachable, §4.3).
    let bg_image: Option<Path> = None;

    // Copy our font features (clone -> owned Vec<CString>).
    let font_features = config.font_features();

    // Get our font styles.
    let mut font_styles = StyleStatus::init_fill(true);
    font_styles.set(Style::Bold, config.font_style_bold());
    font_styles.set(Style::Italic, config.font_style_italic());
    font_styles.set(Style::BoldItalic, config.font_style_bold_italic());

    // Our link configs.
    let links = link::Set::from_config(Global, config.links())
        .map_err(|_| DerivedConfigError)?;

    Ok(DerivedConfig {
        background_opacity: config.background_opacity().clamp(0.0, 1.0),
        background_opacity_cells: config.background_opacity_cells(),
        font_thicken: config.font_thicken(),
        font_thicken_strength: config.font_thicken_strength(),
        font_features,
        font_styles,
        font_shaping_break: config.font_shaping_break(),

        cursor_color: config.cursor_color(),
        cursor_text: config.cursor_text(),
        cursor_opacity: config.cursor_opacity().clamp(0.0, 1.0),

        background: config.background().to_terminal_rgb(),
        foreground: config.foreground().to_terminal_rgb(),
        bold_color: config.bold_color(),
        faint_opacity: (config.faint_opacity() * 255.0).ceil() as u8,

        min_contrast: config.minimum_contrast() as f32,
        padding_color: config.window_padding_color(),

        selection_background: config.selection_background(),
        selection_foreground: config.selection_foreground(),
        search_background: config.search_background(),
        search_foreground: config.search_foreground(),
        search_selected_background: config.search_selected_background(),
        search_selected_foreground: config.search_selected_foreground(),

        custom_shaders,
        bg_image,
        bg_image_opacity: config.background_image_opacity(),
        bg_image_position: config.background_image_position(),
        bg_image_fit: config.background_image_fit(),
        bg_image_repeat: config.background_image_repeat(),
        links,
        vsync: config.window_vsync(),
        colorspace: config.window_colorspace(),
        blending: config.alpha_blending(),
        background_blur: config.background_blur(),
        scroll_to_bottom_on_output: config.scroll_to_bottom_output(),
        arena,
    })
}
```

FIDELITY NOTES (R1): the reference `@max(0,@min(1,x))` → `f64::clamp(0.0,1.0)`
(identical for non-NaN; config opacities are parsed finite); `@intFromFloat(@ceil(x*255))`
→ `(x*255.0).ceil() as u8`; `@floatCast(f64)→f32` → `as f32`. `StyleStatus::set`
enum tag names (`Style::Bold` etc.) per the ported `codepoint_resolver`. The `!= .false`
comparison lives in the config impl (§1) because `FontStyle` is not renderer-nameable.

The signature bound changes `<C>` → `<C: RendererConfig>`. **No compile-caller
breaks**: core uses its own `renderer_seam::DerivedConfig`, and the renderer's
`Message::initChangeConfig<C>` (deferred body) does not call this. The mock-C test
(§5) and the rustty-config impl are the instantiators.

### 3.1 `prep_background_image` — R5 seam (adversary blocking-#1 fix)

`change_config` (§4.3) calls `self.prep_background_image()?`, but no such method
exists in `generic.rs` today (verified: only comment references at
`generic.rs:1178/:1318/:1329`; `update_bg_image_buffer` already exists at :3470).
Without this method `change_config` would not compile. So this milestone lands
`prep_background_image` as an R5 `unimplemented!()` seam whose verbatim reference
body (`generic.zig:1747`) is preserved as a comment — matching the whole-feature
deferral of the background-image pipeline (real `Path`, wuffs decode). The call in
`change_config` is provably unreachable (`bg_image == None` ⇒ `bg_image_changed ==
false`, §3/§4.3/RISK-2), so the seam's `unimplemented!()` never executes at runtime.

```rust
// fn prepBackgroundImage(self: *Self) !void {  (ref generic.zig:1747)
//     // Then we try to load the background image if we have a path.
//     if (self.config.bg_image) |p| load_background: {
//         const path = switch (p) { .required, .optional => |slice| slice };
//         // Open the file
//         var file = std.fs.openFileAbsolute(path, .{}) catch |err| {
//             log.warn("error opening background image file \"{s}\": {}", .{ path, err });
//             break :load_background;
//         };
//         defer file.close();
//         // Read it
//         const contents = file.readToEndAlloc(self.alloc, std.math.maxInt(u32)) catch |err| {
//             log.warn("error reading background image file \"{s}\": {}", .{ path, err });
//             break :load_background;
//         };
//         defer self.alloc.free(contents);
//         // Figure out what type it probably is.
//         const file_type = switch (FileType.detect(contents)) {
//             .unknown => FileType.guessFromExtension(std.fs.path.extension(path)),
//             else => |t| t,
//         };
//         // Decode it if we know how.
//         const image_data = switch (file_type) {
//             .png => try wuffs.png.decode(self.alloc, contents),
//             .jpeg => try wuffs.jpeg.decode(self.alloc, contents),
//             .unknown => { log.warn("Cannot determine file type ...", .{path}); break :load_background; },
//             else => |f| { log.warn("Unsupported file type {} ...", .{ f, path }); break :load_background; },
//         };
//         const image: imagepkg.Image = .{ .pending = .{
//             .width = image_data.width, .height = image_data.height,
//             .pixel_format = .rgba, .data = image_data.data.ptr } };
//         // If we have an existing background image, replace it. Otherwise set directly.
//         if (self.bg_image) |*img| { img.markForReplace(self.alloc, image); }
//         else { self.bg_image = image; }
//     } else {
//         // If we don't have a background image path, mark for unload if we have one.
//         if (self.bg_image) |*img| img.markForUnload();
//     }
// }
fn prep_background_image(&mut self) -> Result<(), Error> {
    unimplemented!("prepBackgroundImage — background-image pipeline deferred \
        (real Path + wuffs decode); verbatim body above. Unreachable while \
        DerivedConfig.bg_image is always None (§3/§4.3/RISK-2).")
}
```

---

## 4. RESIZE + CONFIG-SWAP WALK

**Correction to the task's resize assumption:** in THIS reference generic.zig,
`setScreenSize` is **minimal** — it does NOT resize the Target/FrameStates. The
Target/FrameState resize is **lazy inside `drawFrame`** (ref :1493-1544, already
landed): each frame compares `frame.target.width != self.size.screen.width` and
calls `frame.resize(...)` on demand. `FrameState::resize` (`generic.rs:1074`) and
`CustomShaderState::resize` (`generic.rs:1155`) are already ported and exercised by
the landed `draw_frame`. **No new resize plumbing is needed** for the two setters.

### 4.1 `set_screen_size` (ref :1919)

```rust
pub fn set_screen_size(&mut self, size: Size) {
    self.draw_mutex.lock();
    // `defer self.draw_mutex.unlock()`: this body is INFALLIBLE (no `?`), so the
    // single unlock at the natural end is reached on every non-panic path —
    // faithful to the reference defer, which (Zig, no unwind) likewise only runs
    // on the normal return. No error path exists to leak the lock (contrast §4.3).
    self.size.padding = size.padding; // only the padding is needed; rest derived
    self.update_screen_size_uniforms(); // landed (generic.rs:3423)
    // log.debug("screen size size={}", …);
    self.draw_mutex.unlock();
}
```

`update_screen_size_uniforms` is already landed. Offscreen-testable directly.

### 4.2 `set_font_grid` (ref :1070)

```rust
pub fn set_font_grid(&mut self, grid: *mut SharedGrid) {
    self.draw_mutex.lock();
    // `defer self.draw_mutex.unlock()`: INFALLIBLE body (no `?`) ⇒ the end unlock
    // is reached on every non-panic path (same as §4.1).
    self.font_grid = grid;                        // swap the raw handle

    // Reset per-frame atlas sync counters (GPU doesn't touch this — no GPU lock).
    for frame in &mut self.swap_chain.frames {
        frame.grayscale_modified = 0;
        frame.color_modified = 0;
    }

    // metrics never recalculated ⇒ no shared-grid lock needed.
    let metrics = unsafe { (*grid).metrics };
    self.grid_metrics = metrics;

    // Reset the shaper cache (old-font shape info may be invalid).
    let font_shaper_cache = ShaperCache::init();
    self.font_shaper_cache.deinit();              // deinit(&mut self) — NO arg (cache.rs:112)
    self.font_shaper_cache = font_shaper_cache;

    self.size.cell = CellSize { width: metrics.cell_width, height: metrics.cell_height };
    self.update_font_grid_uniforms();             // landed (generic.rs:3407)
    self.mark_dirty();                            // NEW prerequisite (§2.3)
    self.draw_mutex.unlock();
}
```

### 4.3 `change_config` (ref :1841)

Signature decision: the reference `changeConfig(self, config: *DerivedConfig)`
does a shallow `self.config = config.*` and relies on the caller's later
`Message.deinit` — a Zig move-by-copy. In Rust `DerivedConfig` is non-`Copy`
(owns `ArenaAllocator`/`Vec`/`link::Set`), so port it as **by-value ownership
transfer**: `pub fn change_config(&mut self, config: DerivedConfig) -> Result<(), Error>`.
The `Message::ChangeConfig` payload holds `Box<DerivedConfig>` (message.rs:96), so
the caller passes ownership (the reference `v.impl.deinit()` collapses into the
old-config drop).

**`defer self.draw_mutex.unlock()` fidelity (adversary blocking-#2 fix).** The
reference opens with `defer self.draw_mutex.unlock()` (generic.zig:1843), so the
mutex is released on EVERY exit — including the `try font.Shaper.init(...)` error
return. The ported `thread::Mutex` is bare lock/unlock with NO RAII guard
(thread.rs:79/:106). A scope-guard struct holding `&self.draw_mutex` is NOT viable
here: it would hold a shared borrow of a `self` field across the body's `&mut self`
calls (`update_bg_image_buffer`, `mark_dirty`, field writes), which the borrow
checker rejects once those methods reborrow `self` mutably. So `defer` is
replicated by **explicit unlock on every early return** (unlock-before-`?`) plus
the unlock at the natural end. The only fallible op before the end is
`Shaper::init`; `prep_background_image` is unreachable (§3.1). Body:

```rust
pub fn change_config(&mut self, config: DerivedConfig) -> Result<(), Error> {
    self.draw_mutex.lock();
    // `defer self.draw_mutex.unlock()` replicated via explicit unlock on EVERY
    // exit (see prose above — a &self.draw_mutex guard would conflict with the
    // &mut self calls below).

    // Always redo the font shaper (font features may have changed).
    {
        // unlock-before-`?`: on Shaper::init error, release the mutex, THEN return.
        let font_shaper = match Shaper::init(shape::Options {
            features: config.font_features.iter()
                .map(|c| c.as_bytes().to_vec()).collect(),
            ..Default::default()
        }) {
            Ok(s) => s,
            Err(e) => { self.draw_mutex.unlock(); return Err(e.into()); } // defer-equivalent
        };
        self.font_shaper.deinit();
        self.font_shaper = font_shaper;
    }
    // Reset shaper cache (new font).
    let font_shaper_cache = ShaperCache::init();
    self.font_shaper_cache.deinit();          // deinit(&mut self) — NO arg (cache.rs:112)
    self.font_shaper_cache = font_shaper_cache;

    self.uniforms.min_contrast = config.min_contrast;
    self.uniforms.bools.use_display_p3 = config.colorspace == WindowColorspace::DisplayP3;
    self.uniforms.bools.use_linear_blending = config.blending.is_linear();
    self.uniforms.bools.use_linear_correction = config.blending == AlphaBlending::LinearCorrected;

    let bg_image_config_changed =
        self.config.bg_image_fit != config.bg_image_fit
        || self.config.bg_image_position != config.bg_image_position
        || self.config.bg_image_repeat != config.bg_image_repeat
        || self.config.bg_image_opacity != config.bg_image_opacity;

    // bg_image is always None (feature deferred, §3) ⇒ this is always false.
    let bg_image_changed = match (&self.config.bg_image, &config.bg_image) {
        (Some(old), Some(new)) => old != new,   // Path: PartialEq
        (Some(_), None) => true,
        (None, Some(_)) => true,
        (None, None) => false,
    };

    let old_blending = self.config.blending;
    // RepeatablePath placeholder ⇒ always equal ⇒ always false (custom shaders deferred).
    let custom_shaders_changed = self.config.custom_shaders != config.custom_shaders;

    // self.config.deinit(); self.config = config.*  →  replace + drop-old:
    let mut old = core::mem::replace(&mut self.config, config);
    old.deinit();

    // DEFERRED CALL (unreachable under the bg_image==None invariant, §3/§3.1): the
    // real prep lands with prepBackgroundImage + real Path. `prep_background_image`
    // is the R5 seam (§3.1). Uses unlock-before-`?` for defer-fidelity even though
    // this branch never executes (bg_image_changed is always false).
    if bg_image_changed {
        if let Err(e) = self.prep_background_image() {
            self.draw_mutex.unlock();         // defer-equivalent (unreachable)
            return Err(e);
        }
    }
    if bg_image_config_changed { self.update_bg_image_buffer(); } // landed (3470)

    self.mark_dirty();

    let blending_changed = old_blending != self.config.blending;
    if blending_changed {
        self.api.blending = self.config.blending;
        self.reinitialize_shaders = true;
        self.target_config_modified = self.target_config_modified.wrapping_add(1); // +%=
    }
    if custom_shaders_changed { self.reinitialize_shaders = true; }
    self.draw_mutex.unlock();                 // defer-equivalent (natural end)
    Ok(())
}
```

FIDELITY NOTE: reference reads `config.*` AFTER moving into `self.config`
(`self.config.bg_image_fit`); the Rust `replace` moves first, so the trailing
reads use `self.config` (== new config) — the same values. `+%=` →
`wrapping_add`.

### 4.4 Aliasing hazards — NONE new

- `set_font_grid`: `grid: *mut SharedGrid` is a raw-pointer arg to an **external**
  `SharedGrid` (not reborrowed from `&mut self`); `(*grid).metrics` is a shared
  read through a raw ptr with no `&mut` alias to `*grid` ⇒ no Stacked-Borrows
  violation. This is NOT the `rust-mut-args-strongly-protected` pattern (that is
  `&mut Page/Row/[Cell]` re-read across `assert_integrity`); no
  `&mut`-crossing-raw-alias arises ⇒ **no mirihar-chunk3b proof needed**.
- `change_config` / `set_screen_size`: all through `&mut self`; `core::mem::replace`
  is the safe ownership transfer.

---

## 5. TESTABILITY

The renderer **cannot** dep `rustty-config` even as a dev-dependency (that IS the
cycle). So test `DerivedConfig::init<C>` with a **mock `C`** — a `#[cfg(test)]`
struct in the renderer test module impl-ing `RendererConfig`, returning fixed
values — and assert the extracted fields. Reuse the `derived_config_for_test`
values (`generic.rs:4639`) as the expected oracle.

```rust
#[cfg(test)]
struct MockConfig; // returns fixed values matching derived_config_for_test()

impl RendererConfig for MockConfig {
    fn font_thicken(&self) -> bool { false }
    fn font_thicken_strength(&self) -> u8 { 0 }
    fn font_features(&self) -> Vec<CString> { Vec::new() }
    fn font_style_bold(&self) -> bool { true }   // != .false
    fn font_style_italic(&self) -> bool { true }
    fn font_style_bold_italic(&self) -> bool { true }
    fn background(&self) -> Color { Color { r: 0, g: 0, b: 0 } }
    fn foreground(&self) -> Color { Color { r: 0xFF, g: 0xFF, b: 0xFF } }
    fn cursor_color(&self) -> Option<TerminalColor> {
        Some(TerminalColor::Color(Color { r: 0xFF, g: 0, b: 0 })) }
    fn faint_opacity(&self) -> f64 { 1.0 }        // ceil(1.0*255)=255
    fn minimum_contrast(&self) -> f64 { 1.0 }
    fn background_opacity(&self) -> f64 { 1.0 }
    fn cursor_opacity(&self) -> f64 { 1.0 }
    fn window_padding_color(&self) -> WindowPaddingColor { WindowPaddingColor::Background }
    fn alpha_blending(&self) -> AlphaBlending { AlphaBlending::Native }
    fn links(&self) -> &[InputLink] { &[] }
    fn scroll_to_bottom_output(&self) -> bool { false }
    // … remaining accessors returning the fixture defaults …
}

#[test]
fn derived_config_init_extracts_fields() {
    let cfg = MockConfig;
    let d = DerivedConfig::init(Global, &cfg).expect("init");
    assert_eq!(d.font_thicken, false);
    assert_eq!(d.faint_opacity, 255);              // ceil(1.0*255)
    assert_eq!(d.min_contrast, 1.0f32);            // f64 -> f32
    assert_eq!(d.background, RGB { r: 0, g: 0, b: 0 }); // Color::to_terminal_rgb
    assert!(d.font_styles.get(Style::Bold));       // != .false ⇒ true
    assert_eq!(d.cursor_color, Some(TerminalColor::Color(Color { r: 0xFF, g: 0, b: 0 })));
    assert_eq!(d.blending, AlphaBlending::Native);
    assert!(d.bg_image.is_none());                 // deferred placeholder
    assert!(d.links.links.is_empty());
}
```

### Resize / config-swap tests (offscreen golden `Renderer`)

Reuse `test_renderer(cols, rows)` (`generic.rs:4699`) — builds an offscreen
`Renderer` field-by-field (skips cleanly with `None` on headless CI / no Metal
device). Mirror the existing `update_screen_size_uniforms_projection` /
`update_font_grid_uniforms_cell_size` tests (`generic.rs:5230/5260`).

```rust
#[test]
fn set_screen_size_updates_padding_and_uniforms() {
    let mut r = match test_renderer(10, 3) { Some(r) => r, None => return };
    let mut size = r.size;
    size.padding = Padding { top: 4, bottom: 4, left: 4, right: 4 };
    r.set_screen_size(size);
    assert_eq!(r.size.padding, Padding { top: 4, bottom: 4, left: 4, right: 4 });
    // uniforms recomputed with the new padding (projection uses -1*padding.left/top).
    let ts = r.size.terminal();
    assert_eq!(r.uniforms.projection_matrix, math::ortho2d(
        -1.0 * r.size.padding.left as f32, (ts.width + r.size.padding.right) as f32,
        (ts.height + r.size.padding.bottom) as f32, -1.0 * r.size.padding.top as f32));
}

#[test]
fn set_font_grid_swaps_grid_and_resets_modified() {
    let mut r = match test_renderer(10, 3) { Some(r) => r, None => return };
    // dirty a frame's modified counters, then confirm the reset.
    r.swap_chain.frames[0].grayscale_modified = 7;
    let grid = r.font_grid; // reuse the same live grid handle
    r.set_font_grid(grid);
    assert_eq!(r.swap_chain.frames[0].grayscale_modified, 0);
    assert_eq!(r.swap_chain.frames[0].color_modified, 0);
    let m = unsafe { (*grid).metrics };
    assert_eq!(r.size.cell, CellSize { width: m.cell_width, height: m.cell_height });
    assert_eq!(r.uniforms.cell_size, [m.cell_width as f32, m.cell_height as f32]);
    assert_eq!(r.terminal_state.dirty, Dirty::Full); // mark_dirty
}

#[test]
fn change_config_updates_blending_uniforms() {
    let mut r = match test_renderer(10, 3) { Some(r) => r, None => return };
    // Build a new DerivedConfig with linear-corrected blending + bg_image None
    // (so the deferred prep is unreachable).
    let mut d = derived_config_for_test();
    d.blending = AlphaBlending::LinearCorrected;
    d.min_contrast = 2.0;
    r.change_config(d).expect("change_config");
    assert!(r.uniforms.bools.use_linear_blending);
    assert!(r.uniforms.bools.use_linear_correction);
    assert_eq!(r.uniforms.min_contrast, 2.0);
    assert!(r.reinitialize_shaders);               // blending changed
    assert_eq!(r.target_config_modified, 1);       // +%= 1
    assert_eq!(r.terminal_state.dirty, Dirty::Full);
}
```

The rustty-config impl (§1.2) is verified separately by `cargo build -p rustty-config`
(no renderer→config dev-dep).

---

## 6. DEAD-CODE + NO-REGRESSION

- **Reachability / `-D warnings`:** `DerivedConfig::init` / `change_config` /
  `set_screen_size` / `set_font_grid` / `mark_dirty` are `pub` on the crate-root
  `Renderer` (dead-code-exempt, the Chunk-3a §B.4 "make pub" route). `RendererConfig`
  + its accessors are exercised by the `#[cfg(test)] MockConfig` impl and (in the
  config crate) the real `impl … for Config`. The mock-C test keeps
  `DerivedConfig::init` reachable within the renderer crate. The new private
  `prep_background_image` seam (§3.1) is referenced by `change_config`, so it is
  not dead code despite its `unimplemented!()` body.
- **`RendererConfig` bound addition** (`<C>` → `<C: RendererConfig>`) breaks no
  compile-caller: core uses `renderer_seam::DerivedConfig` (its own placeholder),
  and the deferred `Message::initChangeConfig<C>` does not call `init`.
- **Golden + existing 40 tests:** untouched. `update_screen_size_uniforms` /
  `update_font_grid_uniforms` / `update_bg_image_buffer` bodies are unchanged;
  `set_screen_size`/`set_font_grid` only ADD callers of them. `test_renderer` /
  `golden_renderer` field-literal builders are unchanged (they still bypass
  `Renderer::init`).
- **Gates:** `check_acyclic.sh` green (no new crate edge — the trait is
  renderer-defined, impl-ed in config which already deps renderer);
  `check_leaks.sh` green (no new FFI).

---

## 7. OPEN RISKS

- **RISK-1 (bg_image / custom_shaders fidelity):** the two placeholder fields are
  constructed as `None` / ZST rather than cloned. This is a **coherent whole-feature
  deferral** (background-image + custom-shader render paths are deferred end-to-end:
  real `Path`/`RepeatablePath`, `prepBackgroundImage`, wuffs decode, shader
  transpile). Documented in-body + DEFERRALS.md. The real clone lands when `Path`
  becomes a real type (renderer gains `rustty-args`/`rustty-cfg-fmt` edges — verify
  acyclic then — or a future hoist).
- **RISK-2 (`change_config` reaches a deferred call):** the `if bg_image_changed
  { prep_background_image()? }` line is statically present but provably unreachable
  (`bg_image` is always `None` on both sides ⇒ `bg_image_changed == false`). The
  called `prep_background_image` is now a real R5 seam (§3.1, adversary
  blocking-#1 fix), so `change_config` compiles and the deferred call is a
  documented-unreachable `unimplemented!()`. Precedent: the landed `draw_frame`
  inspector arm (`generic.rs:3228`). Verify the invariant holds (no code path sets
  `DerivedConfig.bg_image = Some`). If an adversary still rejects a landed method
  containing a reachable-in-principle `unimplemented!()`, **fallback: defer
  `change_config`** (keep it an `unimplemented!()` seam) and land only items 1-5
  + the §3.1 seam — the acyclic mechanism + init + the two setters still ship.
- **RISK-3 (`change_config` by-value signature):** diverges from the reference
  `*DerivedConfig` pointer (a Zig move-by-copy that Rust ownership forbids). By-value
  is the faithful Rust ownership transfer given `Message` holds `Box<DerivedConfig>`;
  flag for adversary sign-off. `Shaper::init` errors + `deinit` sigs are VERIFIED
  against the landed font-shaper surface: `ShaperCache::deinit(&mut self)` and
  `Shaper::deinit(&mut self)` both take NO allocator argument (cache.rs:112,
  harfbuzz.rs:191 / coretext.rs:437); confirm `shape::Options.features` field type
  at implementation time.
- **RISK-4 (config impl scope):** landing `impl RendererConfig for Config` in
  rustty-config now (vs deferring to core-migration) is recommended as the acyclic
  proof, but adds ~34 one-line accessors to rustty-config. If scope is tight, the
  trait + `DerivedConfig::init` + mock-C test can land renderer-side alone and the
  config impl deferred — but then the acyclic edge is only proven by the mock, not a
  real `Config`. RECOMMEND landing the config impl.
- **RISK-5 (`font_features` bridging) — CONFIRMED CORRECT by adversary:** config
  stores `Vec<Vec<u8>>` (no NUL) — verified `RepeatableString.list` is populated
  via `value.to_vec()` (value_types.rs:2759/2791), so the bytes never contain an
  interior NUL. The accessor builds `CString`s; `CString::new` errors only on an
  interior NUL, so `.expect()` is safe. Documented as required.
