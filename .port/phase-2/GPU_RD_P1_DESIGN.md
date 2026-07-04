# GPU R-d P1 — `image.zig` + `Overlay.zig` port design (Chunk-3 prerequisites)

**Architect deliverable.** Parents: `GPU_RD_BLUEPRINT.md` (adversarially reviewed,
ACCEPT-WITH-FIXES `[F10/F11]` — image.zig + Overlay.zig are named R-d prerequisites) and
`GPU_RD_TRAIT_DESIGN.md` (§D — the `GenericRenderer` fields `images: image::State`,
`bg_image: Option<image::Image>`, `bg_image_buffer: crate::shaders::BgImage`,
`overlay: Option<Overlay>`; decision RESOLVED **monomorphic**). Design only; no crate changes here.

Ports `src/renderer/image.zig` (958 loc) → `crates/rustty-renderer/src/image.rs` and
`src/renderer/Overlay.zig` (415 loc) → `crates/rustty-renderer/src/overlay.rs`. Both read in full.

Governing rules: R1 (100% fidelity — every line/comment ported; both files have **0** `test{}`
blocks, confirmed by grep, so there are **no** un-deferred reference tests to un-gate — this tier
is golden-image-gated), R3, R4 (acyclic — gate `check_acyclic.sh`), R5 (deferred bodies are
`unimplemented!("…")` seams with the verbatim Zig body preserved as a comment — the
placeholder-leaf-doctrine).

---

## §0 — Executive scope decision (read first)

**Monomorphic, doctrine-driven per-method split.** The offscreen golden (`GPU_RD_BLUEPRINT` §6)
renders **monochrome text on a solid background — no Kitty images, no overlay content**. Every
generic.zig call into this surface therefore exercises only the *container / dispatch / lifecycle*
methods over an **empty** image map and a `None` overlay. The heavy pixel-producing leaves (Kitty
pixel decode/upload via `wuffs`; the terminal-walk that computes Kitty placements; the z2d overlay
feature render) are **never run** by the golden and each depends on machinery that is either a hard
gap (`wuffs`) or absent from the ported z2d subset (`Context.setAntiAliasingMode`/`setHairline`).

Rule applied uniformly (mirrors the landed `RenderPass::step` / `Contents::add` precedent):
**dispatch, container, and lifecycle methods port FULLY; the heavy render/decode leaves they call
are R5 `unimplemented!()` seams.** A whole method is one or the other, never half. Prefer FULL
wherever the dependency is landed (R1); seam only the genuinely-heavy, golden-never-run leaf, each
with its verbatim Zig body preserved as a comment.

The split lands **9 of 11** `image` methods and **8 of 12** `Overlay` items as FULL ports; the
**6** seams are exactly the `wuffs`-dependent Kitty pixel path (`Image::convert`), the four
Kitty-placement terminal-walk methods, and the four z2d overlay-render methods — none reachable
by a no-image/no-overlay frame.

---

## §1 — Module placement, crate deps, acyclicity

### 1.1 Placement (macOS-gated, crate root)

```rust
// crates/rustty-renderer/src/lib.rs
#[cfg(target_os = "macos")] pub mod image;      // renderer/image.zig  -> image::State/Image/…
#[cfg(target_os = "macos")] pub mod overlay;    // renderer/Overlay.zig
#[cfg(target_os = "macos")] pub use overlay::Overlay;   // trait-design names `crate::Overlay`
```

Both are macOS-gated because `image::Image` embeds `crate::metal::Texture` (the `ready` variant),
and `overlay.rs` returns `image::Pending` from `pending_image` — so `overlay` is coupled to the
macOS-only `image` module. This matches the `#[cfg(target_os="macos")] pub mod metal;` gate already
in `lib.rs`; the whole monomorphic `GenericRenderer(Metal)` is macOS-only in R-d. Neither module
names `GenericRenderer` (§2), so both compile standalone before Chunk 3.

### 1.2 Crate dependency edges

| Edge | Status | Needed by | Acyclic? |
|---|---|---|---|
| `rustty-renderer → rustty-vt` | **already present** (`Cargo.toml:19`) | `image.zig`'s `terminal.Terminal` / `terminal.kitty.graphics.*` / `terminal.RenderState` | yes (vt is below renderer) |
| `rustty-renderer → rustty-z2d` | **MUST ADD** | `Overlay.zig`'s `z2d.Surface`/`Context`/`Pixel` | yes — see below |
| `rustty-renderer → rustty-font` | already present | `Image::Pending`/atlas already wired | yes |
| metal leaves + `crate::shaders` | already landed (Chunks 0/1/2) | `draw`/`upload`/`Image` | n/a (same crate) |

**The one new edge is `rustty-renderer → rustty-z2d`.** Add to `crates/rustty-renderer/Cargo.toml`
`[dependencies]`:

```toml
# Overlay.zig renders the debug overlay on the CPU via z2d (surface + context +
# pixel). rustty-z2d is a pure-Rust LEAF (its [dependencies] is empty) and is
# already an established sink below rustty-font (`rustty-font/Cargo.toml:42`), so
# this is a downward, acyclic edge parallel to font→z2d. No new FFI (pure Rust),
# so G6/check_leaks.sh is unaffected.
rustty-z2d = { workspace = true }
```

**Acyclicity (G7).** `rustty-z2d` declares an empty `[dependencies]` — it is a graph **sink**, so
no edge into `rustty-renderer` can exist and no cycle is possible. `cargo_cycle_check.py` (Tarjan
over `crates/*/Cargo.toml`) is unaffected; the font→vt layering assertion is untouched. `check_acyclic.sh`
stays GREEN. **No new FFI**: z2d is pure Rust, so `rustty-renderer`'s FFI-crate status (Metal/oniguruma/
glslang/spirv-cross) is unchanged for G6.

---

## §2 — Compile boundary + gate

**image.rs / overlay.rs compile against the LANDED leaves, without `GenericRenderer`.** Exactly as
the metal leaves do, the draw/upload methods take the api/pass **as parameters**; nothing here names
the not-yet-existing renderer struct. The boundary set:

- `crate::metal::{Metal, Texture, TextureOptions, Buffer, BufferOptions, Pipeline, RenderPass,
  Step, Draw, Attachment, AttachmentTarget}` and `crate::metal::api::MTLPrimitiveType`,
  `crate::metal::metal_impl::ImageTextureFormat` — all Chunk-1/2 landed.
- `crate::shaders::{Image, BgImage}` — landed (`shaders.rs:205`, `:224`).
- `crate::size::CellSize` / `crate::size::Size` — landed (render-types A1 hoist).
- `rustty_vt::terminal::{Terminal, RenderState}`, `rustty_vt::kitty::graphics::{Image,
  ImageStorage, unicode}` — landed and public (see §5.3 verification).
- `rustty_z2d::{surface::Surface, context::Context, pixel::{Pixel, Rgb, Rgba}}` — landed (new edge).
- the renderer's allocator convention (allocator-api2 `Allocator`, as `cell::Contents`/`state` use).

Method-signature substitutions (monomorphic; `GPU_RD_TRAIT_DESIGN` §F): `*GraphicsAPI` → `&Metal`,
`GraphicsAPI.Texture` → `crate::metal::Texture`, `GraphicsAPI.Pipeline` → `&crate::metal::Pipeline`
(borrow — the landed `Step.pipeline` is `&'a Pipeline`), `*GraphicsAPI.RenderPass` →
`&crate::metal::RenderPass`, `Buffer(shaders.Image)` → `crate::metal::Buffer<crate::shaders::Image>`.

**Gate for this task (Chunk-3 prerequisite):**
1. `cargo build -p rustty-renderer` on macOS (both modules compile against the leaves).
2. `check_acyclic.sh` GREEN (the new z2d edge).
3. Two `#[cfg(target_os="macos")] #[test]` **smoke tests** (no GenericRenderer, no golden yet):
   - `image_empty_lifecycle`: `let mut s = image::State::EMPTY; assert!(s.upload(alloc, &metal));
     s.overlay_update(alloc, None).unwrap(); s.deinit(alloc);` — exercises the FULL container path
     over an empty map (device from `MTLCreateSystemDefaultDevice()`, skip-on-`None`).
   - `overlay_surface_roundtrip`: `let mut o = Overlay::init(sz)?; let p = o.pending_image();
     assert_eq!(p.width, term.width); o.reset(); o.deinit();` — pure z2d, no GPU.
   `image::State::draw`'s iterate-empty path is proven end-to-end by the **golden** (Chunk 4,
   `drawFrame`), which calls `draw` x4 over the empty map and `overlay_update(None)`; that is the
   binding gate for this surface. Record in `DEFERRALS.md`: `image.zig`/`Overlay.zig` carry **0**
   reference `test{}` and are golden-gated; the 6 seams are R5-preserved.

---

## §3 — `image.rs` exact Rust shapes

```rust
//! PORTS: ghostty src/renderer/image.zig — Kitty-graphics + overlay image render state.
use std::collections::HashMap;
use std::time::Instant;
use allocator_api2::alloc::Allocator;   // the renderer's alloc convention (as cell.rs/state.rs)

use rustty_vt::terminal::{self, Terminal, RenderState};   // `terminal.*`
use crate::metal::{Metal, Texture, Buffer, Pipeline, RenderPass, Step, Draw, AttachmentTarget};
use crate::metal::api::MTLPrimitiveType;
use crate::shaders;                                       // shaders.Image vertex
use crate::size::CellSize;
use crate::overlay::Overlay;

/// Generic image rendering state for the renderer (image.zig `State`).
pub struct State {
    /// image.zig `images: ImageMap` — `AutoHashMapUnmanaged(Id, {image, transmit_time})`.
    pub images: ImageMap,
    /// Kitty placements (image.zig `kitty_placements`).
    pub kitty_placements: Vec<Placement>,
    pub kitty_bg_end: u32,
    pub kitty_text_end: u32,
    pub kitty_virtual: bool,
    pub overlay_placements: Vec<Placement>,
}

impl State {
    /// image.zig `pub const empty` (a `const`, not a fn → an associated const).
    pub const EMPTY: State = State {
        images: /* HashMap::new() is not const → use a small `fn empty()` ctor; see PORT NOTE */,
        kitty_placements: Vec::new(),
        kitty_bg_end: 0, kitty_text_end: 0, kitty_virtual: false,
        overlay_placements: Vec::new(),
    };
    // PORT NOTE: `HashMap::new()` is not `const`; provide `pub fn empty() -> State` returning the
    // above with `images: HashMap::new()` (behaviorally identical to `.empty`).

    pub fn deinit<A: Allocator + Copy>(&mut self, alloc: A);                                   // FULL
    pub fn upload<A: Allocator + Copy>(&mut self, alloc: A, api: &Metal) -> bool;              // FULL
    pub fn draw(&self, api: &Metal, pipeline: &Pipeline, pass: &RenderPass,
                placement_type: DrawPlacements);                                               // FULL
    pub fn overlay_update<A: Allocator + Copy>(&mut self, alloc: A,
                overlay: Option<Overlay>) -> Result<(), PrepImageError>;                       // FULL
    pub fn kitty_requires_update(&self, t: &Terminal) -> bool;                                 // FULL
    pub fn kitty_update<A: Allocator + Copy>(&mut self, alloc: A, t: &Terminal,
                cell_size: CellSize);                                                          // SEAM
    // --- FULL private helper (map bookkeeping; calls prep_for_upload → convert seam) ---
    fn prep_image<A: Allocator + Copy>(&mut self, alloc: A, id: Id,
                transmit_time: Instant, pending: Pending) -> Result<(), PrepImageError>;       // FULL
    // --- SEAM private helpers (Kitty terminal-walk; golden-never-run) ---
    fn prep_kitty_placement(&mut self, /*…*/) -> Result<(), PrepImageError>;                   // SEAM
    fn prep_kitty_virtual_placement(&mut self, /*…*/) -> Result<(), PrepImageError>;           // SEAM
    fn prep_kitty_image(&mut self, /*…*/) -> Result<(), PrepImageError>;                       // SEAM
}

/// image.zig `DrawPlacements` enum.
pub enum DrawPlacements { KittyBelowBg, KittyBelowText, KittyAboveText, Overlay }

#[derive(Clone, Copy)]
pub enum PrepImageError { OutOfMemory, ImageConversionError }   // image.zig `PrepImageError`

/// image.zig `Placement` (POD; grid x/y/z + dims + cell offsets + source rect).
#[derive(Clone, Copy)]
pub struct Placement {
    pub image_id: Id,
    pub x: i32, pub y: i32, pub z: i32,
    pub width: u32, pub height: u32,
    pub cell_offset_x: u32, pub cell_offset_y: u32,
    pub source_x: u32, pub source_y: u32, pub source_width: u32, pub source_height: u32,
}

/// image.zig `Id = union(enum) { kitty: u32, overlay }` → a Rust enum.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Id { Kitty(u32), Overlay }
impl Id { pub fn z_less_than(lhs: Id, rhs: Id) -> bool { /* FULL: tag-then-value */ } }

/// image.zig `ImageMap = AutoHashMapUnmanaged(Id, struct{image, transmit_time})`.
pub type ImageMap = HashMap<Id, ImageEntry>;
pub struct ImageEntry { pub image: Image, pub transmit_time: Instant }
// PORT NOTE: Zig sets `transmit_time = undefined` then overwrites in prep_image; Rust has no
// `undefined` → initialize with `Instant::now()` then overwrite (behaviorally identical; the
// value is never read before the overwrite). `std.time.Instant.now()` is `try` in Zig but
// `Instant::now()` is infallible in Rust → drop the `?` at the overlayUpdate call site.

/// image.zig `Image = union(enum)` — the per-image upload state machine.
pub enum Image {
    Pending(Pending),
    Replace(Replace),
    Ready(Texture),
    UnloadPending(Pending),
    UnloadReady(Texture),
    UnloadReplace(Replace),
}
pub struct Replace { pub texture: Texture, pub pending: Pending }

/// image.zig `Image.Pending` — raw pixel bytes awaiting GPU upload.
//
// PORT NOTE (zig-byteslice-to-rust-rawbytes + rust-mut-args memory): Zig `data: [*]u8` is a
// RAW pointer whose ownership is tracked by the enclosing `Image` variant (owned once `prep_image`
// `alloc.dupe`s it; borrowed when it points at a z2d/kitty buffer). Faithful port keeps it a raw
// `*mut u8` (NOT `Vec<u8>`) so the borrow-then-own duality and the `Overlay::pending_image`
// aliasing into the z2d surface buffer are preserved. `Image::deinit(alloc)` reconstructs
// `slice::from_raw_parts_mut(data, len())` and `alloc.deallocate`s it (mirrors `alloc.free(dataSlice())`).
#[derive(Clone, Copy)]
pub struct Pending {
    pub height: u32, pub width: u32,
    pub pixel_format: PixelFormat,
    pub data: *mut u8,     // Zig `[*]u8`
}
impl Pending {
    pub fn data_slice(&self) -> &mut [u8];  // `data[0..len()]` (unsafe from_raw_parts)
    pub fn len(&self) -> usize;             // width*height*pixel_format.bpp()
}

/// image.zig `Image.Pending.PixelFormat`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat { Gray, GrayAlpha, Rgb, Bgr, Rgba, Bgra }
impl PixelFormat { pub fn bpp(self) -> usize { /* 1/2/3/3/4/4 */ } }

impl Image {
    pub fn deinit<A: Allocator>(self, alloc: A);                    // FULL
    pub fn mark_for_unload(&mut self);                             // FULL
    pub fn mark_for_replace<A: Allocator>(&mut self, alloc: A, img: Image);  // FULL
    pub fn is_pending(&self) -> bool;                             // FULL
    pub fn has_texture(&self) -> bool;                           // FULL
    pub fn is_unloading(&self) -> bool;                         // FULL
    fn convert<A: Allocator>(&mut self, alloc: A) -> Result<(), WuffsError>;   // SEAM (wuffs)
    fn prep_for_upload<A: Allocator>(&mut self, alloc: A) -> Result<(), WuffsError>;  // FULL
    pub fn upload<A: Allocator>(&mut self, alloc: A, api: &Metal) -> Result<(), UploadError>; // FULL
    fn get_pending(&self) -> Option<Pending>;                  // FULL
    fn get_texture(&self) -> Option<&Texture>;                // FULL
    fn get_pending_pointer(&mut self) -> Option<&mut Pending>; // FULL
}
```

Notes on the FULL methods that touch the metal leaves:
- **`draw`** (FULL) iterates the selected `Placement` slice (empty in the golden → immediate return
  before any texture lookup). For each placement it looks up `self.images.get(&p.image_id)`, matches
  `Image::Ready | Image::UnloadReady => &tex` (else `continue`), builds a one-element
  `Buffer::<shaders::Image>::init_fill(api.image_buffer_options(), &[shaders::Image{ grid_pos,
  cell_offset, source_rect, dest_size }])` (the landed `crate::shaders::Image` fields match exactly),
  and issues `pass.step(Step{ pipeline, buffers: &[Some(buf.raw())], textures: &[Some(tex)],
  samplers: &[], draw: Draw{ kind: MTLPrimitiveType::TriangleStrip, vertex_count: 4,
  instance_count: 1 } })`. All deps landed. `&self` (Zig `*State` but no mutation).
- **`upload`** (FULL) iterates the map; `is_unloading` → `img.deinit(alloc)` + remove; `is_pending`
  → `img.upload(alloc, api)` (logs+`success=false` on error). Empty-map path is the golden's.
- **`overlay_update`** (FULL) — `None` branch (the golden's) marks any existing `Id::Overlay` for
  unload and returns; the `Some` branch (never hit) clears/reserves `overlay_placements`, calls
  `overlay.pending_image()` + `self.prep_image(alloc, Id::Overlay, Instant::now(), pending)`, and
  pushes the placement. Faithful port; `errdefer comptime unreachable` → a comment (Rust has no
  `errdefer`, and the tail is infallible).
- **`kitty_requires_update`** (FULL) — `unsafe { (*t.screens.active).kitty_images.dirty }
  || self.kitty_virtual`. `ScreenSet.active` is a `pub *mut Screen` (raw ptr) and `ImageStorage.dirty`
  is `pub` (verified), so the const read is a faithful raw-ptr deref (intrusive-ptr memory model).

---

## §4 — `image.rs` SCOPE TABLE

| Method | Scope | Golden hits? | Why |
|---|---|---|---|
| `State::empty` / `empty()` | **FULL** | yes (ctor) | POD; trivial |
| `State::deinit` | **FULL** | yes (teardown) | iterates map → `Image::deinit`; `Vec::clear` |
| `State::upload` | **FULL** | yes (iterate-empty) | loop + `is_unloading`/`is_pending`; empty map = no-op |
| `State::draw` | **FULL** | yes (iterate-empty ×4) | builds `Buffer<Image>` + `pass.step`; all deps landed |
| `State::overlay_update` | **FULL** | yes (`None` branch) | null-branch is the golden path; deps landed |
| `State::kitty_requires_update` | **FULL** | yes (guard, ⇒false) | light; raw-ptr read of `active.kitty_images.dirty` |
| `State::prep_image` | **FULL** | no | map bookkeeping (getOrPut/dupe/markForReplace); feasible |
| `State::kitty_update` | **SEAM** | no | terminal-walk (viewport pins, sort, bg/text split); heavy, never run |
| `State::prep_kitty_placement` | **SEAM** | no | `rect`/`pixelSize`/`pointFromPin` walk; feeds only kitty_update |
| `State::prep_kitty_virtual_placement` | **SEAM** | no | `unicode.placementIterator`/`renderPlacement`; never run |
| `State::prep_kitty_image` | **SEAM** | no | kitty-format switch (`.png => unreachable`); calls prep_image; reached only from seams |
| `Placement`, `Id`, `Id::z_less_than`, `ImageMap`, `DrawPlacements`, `PrepImageError` | **FULL** | — | pure types/helpers |
| `Image::{deinit, mark_for_unload, mark_for_replace, is_pending, has_texture, is_unloading, get_pending, get_texture, get_pending_pointer}` | **FULL** | teardown / dispatch | pure state-machine; deps landed |
| `Image::prep_for_upload` | **FULL** | no | `assert(is_pending); convert()` — 2 lines; compiles calling the convert seam |
| `Image::upload` | **FULL** | no | `prep_for_upload` + `Texture::init(api.image_texture_options(.rgba,true), …)` — all direct deps landed; only transitively reaches the convert seam |
| `Image::convert` | **SEAM** | no | **hard dep gap**: `wuffs.swizzle.{g,ga,rgb,bgr,bgra}ToRgba` — `wuffs` is not ported/available in any crate |
| `Pending::{data_slice, len}`, `PixelFormat::bpp` | **FULL** | — | pure |

**Seam count: 5** (`convert` + the four Kitty-placement methods). The seam boundary is precisely
`wuffs` (for `convert`) and the terminal-walk placement computation (never run by a no-image frame).
`Image::upload`/`prep_for_upload` stay FULL because their *direct* dependencies (`Texture::init`,
`api.image_texture_options`) are landed — only the transitive `convert` is stubbed, so the GPU-upload
wiring is faithfully ported and would run for already-RGBA data (e.g. the overlay). R5: each seam is
`unimplemented!("R-d image pixel path — <name>; golden renders no Kitty images")` with the verbatim
Zig body preserved as a leading block comment.

---

## §5 — `overlay.rs` exact Rust shapes

```rust
//! PORTS: ghostty src/renderer/Overlay.zig — the CPU/z2d debug overlay.
use rustty_z2d::surface::Surface;
use rustty_z2d::context::Context;
use rustty_z2d::pixel::{self, Pixel, Rgb, Rgba};   // z2d.Pixel / z2d.pixel.RGB / .RGBA
use rustty_vt::terminal::RenderState;              // `terminal.RenderState`
use crate::size::{Size, CellSize};                 // size.Size / size.CellSize
use crate::image::Pending;                         // image.zig `Image.Pending`

/// Overlay.zig `@This()` — the file-as-struct.
pub struct Overlay {
    pub surface: Surface,       // z2d.Surface
    pub cell_size: CellSize,    // size.CellSize
}

/// Overlay.zig `Color` — the three overlay colors. (union(enum) with no payload → plain enum.)
pub enum Color { Hyperlink, SemanticPrompt, SemanticInput }
impl Color {
    pub fn rgb(self) -> Rgb;                 // FULL — z2d.pixel.RGB literals
    pub fn rect_fill(self) -> Pixel;         // FULL — alpha_pixel(96)
    pub fn rect_border(self) -> Pixel;       // FULL — alpha_pixel(200)
    pub fn pixel(self) -> Pixel;             // FULL — self.rgb().as_pixel()
    fn alpha_pixel(self, alpha: u8) -> Pixel;// FULL — Rgba::from_pixel(self.pixel()); a=alpha; .multiply().as_pixel()
}

/// Overlay.zig `Feature`.
pub enum Feature { HighlightHyperlinks, SemanticPrompts }

/// Overlay.zig `InitError = Allocator.Error || error{InvalidDimensions}`.
pub enum InitError { OutOfMemory, InvalidDimensions }

impl Overlay {
    pub fn init(sz: Size) -> Result<Overlay, InitError>;      // FULL — see PORT NOTE (alloc dropped)
    pub fn deinit(&mut self);                                 // FULL
    pub fn pending_image(&self) -> Pending;                   // FULL
    pub fn reset(&mut self);                                  // FULL
    pub fn apply_features(&mut self, state: &RenderState, features: &[Feature]);  // FULL (dispatch)
    // --- SEAM render leaves (z2d Context methods missing + RenderState metadata + never run) ---
    fn highlight_hyperlinks(&mut self, state: &RenderState);              // SEAM
    fn highlight_semantic_prompts(&mut self, state: &RenderState);        // SEAM
    fn highlight_grid_rect(&mut self, /*x,y,w,h, border,fill*/) -> Result<(), OverlayError>; // SEAM
    fn highlight_pixel_rect(&mut self, /*x,y,w_px,h, border,fill*/) -> Result<(), OverlayError>; // SEAM
}
```

### 5.1 The z2d mapping (FULL items)

| Overlay.zig | rustty-z2d | present? |
|---|---|---|
| `z2d.Surface.initPixel(px, alloc, w, h)` | `Surface::init_pixel(px, w, h)` — **no alloc param** | yes (`surface.rs:364`) |
| `surface.deinit(alloc)` | `Surface::deinit(&mut self)` — no alloc | yes |
| `surface.getWidth/getHeight` | `get_width()/get_height() -> i32` | yes |
| `surface.paintPixel(px)` | `paint_pixel(px)` | yes |
| `surface.image_surface_rgba.buf.ptr` | `match &self.surface { Surface::ImageSurfaceRgba(s) => s.buf.as_ptr() as *mut u8, _ => unreachable }` (`buf` is `pub`) | yes |
| `z2d.Pixel` / `z2d.pixel.RGB` / `.RGBA` | `pixel::Pixel` / `pixel::Rgb` / `pixel::Rgba` | yes |
| `RGBA.fromPixel` / `.multiply()` / `.asPixel()` | `Rgba::from_pixel` / `.multiply()` / `.as_pixel()` | yes |

**PORT NOTE (init/deinit drop `alloc`):** the rustty-z2d subset dropped the Zig `Allocator` from
`Surface::init_pixel`/`deinit` (the z2d surface owns its `Vec<Rgba>` buffer). So `Overlay::init`
takes only `sz` and `deinit` takes only `&mut self`; the Chunk-3 `rebuildOverlay` caller drops the
`alloc` argument correspondingly. `init`'s `std.math.cast(i32, …) orelse return error.InvalidDimensions`
→ `i32::try_from(w).map_err(|_| InitError::InvalidDimensions)?`; the z2d `InvalidWidth/InvalidHeight`
→ `InitError::InvalidDimensions`, `OutOfMemory` → `InitError::OutOfMemory` (same switch mapping).

**`pending_image` (FULL)** returns `Pending{ width: get_width() as u32, height: get_height() as u32,
pixel_format: PixelFormat::Rgba, data: <ImageSurfaceRgba buf ptr> }`. This aliases the z2d surface
buffer (a *borrow* — `prep_image` `alloc.dupe`s it before it is ever owned by an `Image`), preserving
the Zig `@ptrCast(self.surface.image_surface_rgba.buf.ptr)` semantics. Optional cleanliness: add a
`Surface::rgba_buf(&self) -> &[Rgba]` accessor mirroring the landed `alpha8_buf()` — a trivial,
acyclic z2d addition; not required (the `buf` field is already `pub`).

### 5.2 Why the four render leaves are SEAMs (doubly forced)

1. **Never run by the golden** — no overlay features enabled ⇒ `rebuildOverlay` returns before
   `apply_features` is even called, and `State::draw(.overlay)` is only issued when `self.overlay
   != null`.
2. **Missing z2d Context methods** — `highlight_grid_rect`/`highlight_pixel_rect` call
   `ctx.setAntiAliasingMode(.none)` and `ctx.setHairline(true)`; the ported z2d `Context`
   (sprite/glyf subset) exposes `move_to/line_to/close_path/fill/stroke/set_source_to_pixel/
   set_line_width` but **NOT** `set_anti_aliasing_mode`/`set_hairline` (verified absent). Porting
   them fully would require extending the z2d Context subset — out of scope and unexercised.
3. **RenderState metadata surface unverified** — `highlight_*` read `state.row_data.slice().items(.raw/.cells)`
   and per-row `.hyperlink`/`.semantic_prompt` + per-cell `.hyperlink`/`.semantic_content`; the ported
   `RenderState.row_data: Vec<Row>` does not yet expose these accessors publicly.

`apply_features` itself is **FULL** (the `for f in features { match f { … } }` dispatch — trivial,
compiles calling the seam handlers, mirrors `State::draw`'s dispatch), so the Feature→handler mapping
is preserved verbatim; only the render bodies are seams. R5: each seam is `unimplemented!("R-d overlay
render — <name>; needs z2d AA/hairline Context + RenderState row/cell metadata; golden renders no
overlay")` with the verbatim Zig body as a comment.

### 5.3 rustty-vt surface verification (for the seam signatures + kitty_requires_update)

All types the ported signatures name are present and public (so even the seams compile):
- `ImageStorage.dirty: pub bool` (`graphics_storage.rs:59`); `image_by_id` (`:293`); `Placement`
  (`:875`); `Location` (`:898`); `pixel_size` (`:945`); `rect` (`:1050`); `PixelSize`/`GridSize`.
- `graphics_unicode::{placement_iterator (:34), PlacementIterator (:41), Placement (:139),
  render_placement (:173)}`; `graphics_render::Placement (:17)`.
- `Screen.kitty_images: pub ImageStorage` (`screen.rs:132`); `ScreenSet.active: pub *mut Screen`
  (`screen_set.rs:50`) — the raw-ptr deref used by `kitty_requires_update`.
- `pagelist::{get_top_left (mod.rs:1645), get_bottom_right (access.rs:504), point_from_pin (pin.rs:121)}`
  — the accessors the `kitty_update` seam body references.
- `terminal::RenderState` (`render.rs:66`) — Overlay's `apply_features` param.

---

## §6 — The seam mechanism (R5 / placeholder-leaf-doctrine)

Each seam is a whole method whose Rust body is `unimplemented!("<reason>")` and whose **verbatim Zig
source is preserved as a leading block comment** (line-for-line, so R1's line/comment fidelity holds
and a later chunk can un-seam by translating the preserved body). The seam’s **signature is fully
ported** (real Rust types mapped from the Zig prototype) so the module type-checks and the Chunk-3
`GenericRenderer` can name/call it. This is the landed precedent (`state::Inspector`,
`thread::DerivedConfig`, the metal `present`/async-branch seams). Record all 6 seams + the
0-`test{}`/golden-gated status in `.port/phase-2/DEFERRALS.md`.

---

## §7 — OPEN RISKS

1. **`wuffs` has no Rust home.** `Image::convert` is the only *hard* gap (grep: no wuffs crate; only
   `rustty-font` references it in a comment). If a later chunk needs real Kitty pixel rendering, a
   `wuffs`-equivalent swizzle (pure-Rust g/ga/rgb/bgr/bgra→rgba) must be ported first. For R-d the
   seam is correct — the golden uploads no non-RGBA pixels.
2. **`kitty_requires_update` const-active deref.** It reads `(*t.screens.active).kitty_images.dirty`
   through a `*mut Screen`. `active` is `pub` and the deref is `unsafe` but faithful (Zig does the
   same field read). Confirm no aliasing rule is violated when Chunk-3 `updateFrame` holds `&self`
   while dereferencing the terminal's `active` ptr (it is a distinct object; low risk, but Miri it
   when the golden's richer variant drives `updateFrame`). If the golden uses only the hand-built
   `Contents` path (§6 primary variant), this method is compiled but never executed.
3. **Allocator convention.** The signatures use `A: Allocator + Copy` (allocator-api2). Confirm this
   matches `cell::Contents`/`state`'s exact bound (they may use concrete `Global`); align to avoid a
   generics mismatch at the Chunk-3 call sites. `Pending.data` as raw `*mut u8` + manual
   `allocate`/`deallocate` must use the *same* allocator across dupe/free (Zig invariant); document
   the `Layout` (`len()` bytes, align 1) used for `deallocate`.
4. **`transmit_time` type coupling (seam-only).** `prep_kitty_image` (seam) maps the kitty
   `image.transmit_time` (rustty-vt type) into `prep_image`; confirm the vt kitty `Image.transmit_time`
   is (or converts to) `std::time::Instant` when that seam is later filled. Not a blocker now.
5. **z2d `ImageSurfaceRgba` variant assumption.** `pending_image` matches
   `Surface::ImageSurfaceRgba`; `Overlay::init` builds the surface via `init_pixel(Pixel::Rgba{…})`,
   which must yield that variant. Verify `init_pixel` dispatches an RGBA initial pixel to
   `ImageSurfaceRgba` (it should, by the `SurfaceType` enum); otherwise `pending_image`'s match arm
   is wrong.
6. **`Overlay::init` dropped-`alloc` ripple.** Chunk-3 `rebuildOverlay` (a `generic.zig` method, not
   in this task) calls `Overlay.init(alloc, self.size)` and `overlay.deinit(alloc)`; the Chunk-3 port
   must drop those `alloc` args to match this design. Flagged so the two chunks stay consistent.

---

## §8 — Return summary (the four asks)

- **Scope decision:** monomorphic, doctrine-driven — dispatch/container/lifecycle methods port FULL
  over the empty-map/`None`-overlay golden; the heavy pixel/terminal-walk/z2d-render leaves the
  golden never runs are R5 `unimplemented!()` seams. CORRECTION (post-port): the authoritative
  **method-level** seam count is **9** — 5 in `image.rs` (`convert` + `kitty_update` +
  `prep_kitty_placement` + `prep_kitty_virtual_placement` + `prep_kitty_image`) and 4 in
  `overlay.rs` (`highlight_{hyperlinks,semantic_prompts,grid_rect,pixel_rect}`). The "6" cited
  elsewhere in this doc counted at a coarser item granularity; DEFERRALS.md and the code (9
  `unimplemented!` methods) are authoritative.
- **Dep edges:** add **`rustty-renderer → rustty-z2d`** (Overlay's z2d) — a downward edge to a
  pure-Rust sink, acyclic (parallels the existing font→z2d), no new FFI; `rustty-renderer → rustty-vt`
  already present (image's terminal/kitty/RenderState). `check_acyclic.sh` stays GREEN.
- **Method split:** SEAMs = `Image::convert` (wuffs gap) + `kitty_update`/`prep_kitty_placement`/
  `prep_kitty_virtual_placement`/`prep_kitty_image` (terminal-walk) + the four `Overlay` render
  leaves grouped as `highlight_*` (missing z2d AA/hairline + RenderState metadata). Everything else
  FULL, incl. `draw`/`upload`/`overlay_update`/`kitty_requires_update`/`prep_image`, all of `Image`'s
  state machine + `Image::upload`, and Overlay's `Color`/`init`/`deinit`/`pending_image`/`reset`/
  `apply_features`.
- **Compile boundary:** `image.rs`/`overlay.rs` (both `#[cfg(target_os="macos")]`, crate root)
  compile against the LANDED metal leaves + `crate::shaders::{Image,BgImage}` + `crate::size` +
  rustty-vt + rustty-z2d, with draw/upload taking `&Metal`/`&Pipeline`/`&RenderPass` **as params** —
  no `GenericRenderer` named. Gate: `cargo build -p rustty-renderer` (macOS) + `check_acyclic.sh` +
  two smoke `#[test]`s (empty-map lifecycle; overlay surface round-trip); the binding gate is the
  Chunk-4 offscreen golden. 0 reference `test{}` in either file → golden-gated (record in DEFERRALS).
