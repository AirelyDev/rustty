# Renderer Kitty Image Pixel-Path Design (R-d fill)

Fills the 5 deferred R5 seams in `crates/rustty-renderer/src/image.rs`, porting the
Kitty-graphics image PIXEL PATH faithfully to the ported vt-kitty + metal-Texture
APIs. Reference (read-only): `/Users/gael/projects/ghostty/src/renderer/Image.zig`
(the verbatim Zig bodies are already preserved as leading block comments at each
seam). Design ONLY — this milestone touches no `crates/`.

- `vt_kitty_api_reachable = REACHABLE` (every symbol the seams need exists and is
  `pub`/accessible from the renderer, which already deps `rustty-vt`; the exact
  access map is §2).
- Scope: the 5 seams (`kitty_update`, `prep_kitty_placement`,
  `prep_kitty_virtual_placement`, `prep_kitty_image`, `Image::convert`) plus the
  ONE support-type change §3 requires (inhabiting `WuffsError`).
- Compile boundary: `rustty-renderer` only. No new crate deps. `Image::convert`'s
  wuffs swizzle is re-homed as a pure-Rust byte loop inside `image.rs` (wuffs has
  no Rust home; §3).

---

## 0. Verification result — vt-kitty API is REACHABLE

Every dependency named in the task brief was confirmed present, `pub`, and
reachable from `rustty-renderer` (which already `use`s `rustty_vt::...`, see
`image.rs:38-41`). Concretely:

| Needed (Zig)                                      | Rust symbol (verified)                                                                 | Location |
|---------------------------------------------------|----------------------------------------------------------------------------------------|----------|
| `t.screens.active.kitty_images`                   | `unsafe { &(*t.screens.active).kitty_images }` → `ImageStorage`                         | `screen.rs:132`, `graphics_storage.rs:54` |
| `storage.dirty`                                   | `ImageStorage.dirty: bool` (pub)                                                        | `graphics_storage.rs:59` |
| `storage.imageById(id)`                           | `ImageStorage::image_by_id(&self, u32) -> Option<Image>`                                | `graphics_storage.rs:293` |
| `storage.placements` (iterate)                    | `ImageStorage.placements: PlacementMap` (pub); `.iter()`/`.keys()`+`.get()`             | `graphics_storage.rs:78` |
| `Placement` (loc/offsets/source/cols/rows/z)      | `graphics_storage::Placement` (all fields pub) + `pixel_size`/`grid_size`/`rect`        | `graphics_storage.rs:875,945,1020,1050` |
| `Placement.location` (`.pin`/`.virtual`)          | `graphics_storage::Location::{Pin(*mut Pin), Virtual}`                                  | `graphics_storage.rs:898` |
| `p.rect(image, t)`                                | `Placement::rect(&self, &Image, &Terminal) -> Option<Rect>` (method)                   | `graphics_storage.rs:1050` |
| `Rect { top_left, bottom_right }` (the TYPE)      | `graphics_image::Rect { top_left: Pin, bottom_right: Pin }` (both fields pub)          | `graphics_image.rs:774` (imported into `graphics_storage` via `use crate::kitty::graphics_image::{…Rect}`, `graphics_storage.rs:28`) |
| `p.pixelSize(image, t)`                           | `Placement::pixel_size(&self, &Image, &Terminal) -> PixelSize`                          | `graphics_storage.rs:945` |
| `pages.getTopLeft(.viewport)`                     | `PageList::get_top_left(&self, point::Tag) -> Pin`                                      | `pagelist/mod.rs:1645` |
| `pages.getBottomRight(.viewport)`                 | `PageList::get_bottom_right(&self, point::Tag) -> Option<Pin>`                          | `pagelist/access.rs:504` |
| `pages.pointFromPin(.screen, pin)`                | `PageList::point_from_pin(&self, point::Tag, Pin) -> Option<point::Point>`              | `pagelist/pin.rs:121` |
| `.screen.y` on the returned point                 | `point::Point::Screen(Coordinate{ y, .. })`                                             | `point.rs:81,142` |
| `unicode.placementIterator(top, bot)`             | `kitty::graphics_unicode::placement_iterator(Pin, Option<Pin>) -> PlacementIterator`   | `graphics_unicode.rs:34` |
| virtual `p.renderPlacement(...)`                  | `graphics_unicode::Placement::render_placement(&self, &ImageStorage, &Image, u32, u32) -> Result<RenderPlacement, Error>` | `graphics_unicode.rs:173` |
| `RenderPlacement` fields                          | `graphics_render::Placement { top_left, offset_x/y, source_x/y/width/height, dest_width/height }` | `graphics_render.rs:17` |
| `image.{id,width,height,format,data,transmit_time}` | `graphics_image::Image` (all pub); `data: Vec<u8>`, `transmit_time: Option<Instant>`  | `graphics_image.rs:720` |
| `image.format` variants                           | `command::Format::{Gray,GrayAlpha,Rgb,Rgba,Png}`                                        | `graphics_command.rs:564` |
| `t.{width_px,height_px,cols,rows}`                | `Terminal` fields (pub, used by `pixel_size`)                                           | `terminal.rs:376-381` |
| GPU `Texture` + `imageTextureOptions`             | `metal::texture::Texture::init`, `Metal::image_texture_options`                        | `metal/texture.rs:61`, `metal/metal_impl.rs:477` |

The renderer type-aliases the vt types already: `KittyImage` (`graphics_image::Image`),
`KittyStoragePlacement` (`graphics_storage::Placement`), `KittyUnicodePlacement`
(`graphics_unicode::Placement`) — `image.rs:38-40`. The seam signatures already
name these. Nothing is missing; we proceed to design.

**One fidelity nuance (open risk R-1):** `image_by_id` returns an OWNED `Option<Image>`
(`self.images.get(&id).cloned()`, `graphics_storage.rs:294`) — it CLONES the
`Vec<u8>` pixel data. Zig's `?Image` is a value copy that *shares* the data pointer.
Functionally identical for our read-only consumers (`prep_kitty_image` only reads
`image.data` to `dupe` it into the renderer's own buffer), but it costs one extra
heap copy of the source image per dirty frame. Faithful enough; noted, not blocking.

---

## 1. The 5 seams — concrete Rust signature blocks

The signatures are ALREADY in `image.rs` (currently `unimplemented!()`). Filling =
replacing each body; NO signature change to the four Kitty methods. `Image::convert`
keeps its signature but its error type `WuffsError` must be inhabited (§3.3).

### 1.1 `kitty_update` (image.rs ~:459) — the terminal walk

```rust
pub fn kitty_update<A: Allocator + Copy>(
    &mut self,
    alloc: A,
    t: &Terminal,
    cell_size: CellSize,
) {
    // `const storage = &t.screens.active.kitty_images;`
    // SAFETY (memory-model, §5): `t.screens.active` is the faithful raw `*mut
    // Screen` field. We read `kitty_images` through the raw ptr (NOT a `&mut`
    // derived from `&Terminal`), exactly as `kitty_requires_update` (image.rs:329)
    // already does. Bind the shared view in a SCOPE so it is dropped before the
    // `dirty = false` write below (Stacked-Borrows clean: no shared ref live
    // across the write through the same raw ptr).
    let screen: *mut Screen = t.screens.active;

    // We always clear our previous placements ... rebuild from scratch.
    self.kitty_placements.clear();          // clearRetainingCapacity
    self.kitty_virtual = false;

    // Mark now-unreferenced images for unload (only Kitty ids; overlay skipped).
    {
        let storage: &ImageStorage = unsafe { &(*screen).kitty_images };
        let stale: Vec<u32> = self
            .images
            .keys()
            .filter_map(|id| match id {
                Id::Kitty(kid) if storage.image_by_id(*kid).is_none() => Some(*kid),
                _ => None,
            })
            .collect();
        for kid in stale {
            self.images.get_mut(&Id::Kitty(kid)).unwrap().image.mark_for_unload();
        }
    }

    // Viewport corners in screen points → containment/offset math.
    let (top, bot, top_y, bot_y) = {
        let pages = unsafe { &(*screen).pages };
        let top = pages.get_top_left(point::Tag::Viewport);
        let bot = pages.get_bottom_right(point::Tag::Viewport).unwrap();
        let top_y = pages.point_from_pin(point::Tag::Screen, top).unwrap().coord().y;
        let bot_y = pages.point_from_pin(point::Tag::Screen, bot).unwrap().coord().y;
        (top, bot, top_y, bot_y)
    };

    // Walk placements; prep each non-virtual one. Virtual → set flag + continue.
    {
        let storage: &ImageStorage = unsafe { &(*screen).kitty_images };
        for (key, p) in storage.placements.iter() {
            match p.location {
                Location::Pin(_) => {}
                Location::Virtual => { self.kitty_virtual = true; continue; }
            }
            let image = match storage.image_by_id(key.image_id) {
                Some(img) => img,
                None => { /* log.warn missing image */ continue; }
            };
            // errors: log + continue (place the others even if one fails).
            let _ = self.prep_kitty_placement(alloc, t, top_y, bot_y, &image, p);
        }
    }

    // Virtual placements → scan placeholders in the viewport.
    if self.kitty_virtual {
        let mut v_it = graphics_unicode::placement_iterator(top, Some(bot));
        while let Some(virtual_p) = v_it.next() {
            let _ = self.prep_kitty_virtual_placement(alloc, t, &virtual_p, cell_size);
        }
    }

    // Sort by z (tie-break by Id::z_less_than).
    self.kitty_placements.sort_by(|lhs, rhs| {
        if lhs.z < rhs.z
            || (lhs.z == rhs.z && Id::z_less_than(lhs.image_id, rhs.image_id))
        { core::cmp::Ordering::Less } else { core::cmp::Ordering::Greater }
        // NOTE: mirrors Zig `sortUnstable` with a strict `lessThan`; use
        // `sort_unstable_by` for parity with `std.mem.sortUnstable`.
    });

    // bg_end / text_end split (values sorted by z).
    let bg_limit: i32 = i32::MIN / 2;
    let mut bg_end: Option<u32> = None;
    let mut text_end: Option<u32> = None;
    for (i, p) in self.kitty_placements.iter().enumerate() {
        if bg_end.is_none() && p.z >= bg_limit { bg_end = Some(i as u32); }
        if text_end.is_none() && p.z >= 0 { text_end = Some(i as u32); }
    }
    self.kitty_bg_end = bg_end.unwrap_or(self.kitty_placements.len() as u32);
    self.kitty_text_end = text_end.unwrap_or(self.kitty_placements.len() as u32);

    // `defer storage.dirty = false;` — LAST, through the raw ptr, no live borrow.
    unsafe { (*screen).kitty_images.dirty = false; }
}
```

Notes on divergences forced by Rust:
- Zig's `defer storage.dirty = false` runs on every early return; `kitty_update`
  has none, so a plain final write is faithful. Because it must run through a `&mut`
  path while the loop needs `&` reads of the same `storage`, we do NOT hold a
  long-lived `storage` binding — each read block re-borrows shared through the raw
  ptr and drops it, then the final write goes through the raw ptr. (memory model §5.)
- `sortUnstable` → `sort_unstable_by`. The comparator returns `Less`/`Greater`
  only (never `Equal`); for an UNSTABLE sort that is acceptable and matches Zig's
  boolean `lessThan` (equal elements may reorder either way — Zig's is unstable too).
- `it.next()` on `PlacementIterator` is an explicit method (the vt port keeps the
  Zig iterator shape); `while let Some(..) = v_it.next()`.
- `.coord()` is `point::Point::coordinate()` (point.rs:89) returning the inner
  `Coordinate` whose `.y` is the screen row — faithful to Zig `.?.screen.y`.

### 1.2 `prep_kitty_placement` (image.rs ~:532)

```rust
fn prep_kitty_placement<A: Allocator + Copy>(
    &mut self,
    alloc: A,
    t: &Terminal,
    top_y: u32,
    bot_y: u32,
    image: &KittyImage,
    p: &KittyStoragePlacement,
) -> Result<(), PrepImageError> {
    // Get the rect; virtual/no-rect → skip.
    let rect = match p.rect(image, t) { Some(r) => r, None => return Ok(()) };

    let pages = unsafe { &(*t.screens.active).pages };
    let img_top_y = pages.point_from_pin(point::Tag::Screen, rect.top_left).unwrap().coord().y;
    let img_bot_y = pages.point_from_pin(point::Tag::Screen, rect.bottom_right).unwrap().coord().y;

    // Cull to viewport.
    if img_top_y > bot_y { return Ok(()); }
    if img_bot_y < top_y { return Ok(()); }

    // Ensure on/queued to GPU.
    self.prep_kitty_image(alloc, image)?;

    // Dest size (accounts for rows/cols/aspect).
    let dest_size = p.pixel_size(image, t);

    // Source rectangle (clamped into the image).
    let source_x = image.width.min(p.source_x);
    let source_y = image.height.min(p.source_y);
    let source_width = if p.source_width > 0 { (image.width - source_x).min(p.source_width) } else { image.width };
    let source_height = if p.source_height > 0 { (image.height - source_y).min(p.source_height) } else { image.height };

    // Viewport-relative Y.
    let y_pos: i32 = img_top_y as i32 - top_y as i32;

    if dest_size.width > 0 && dest_size.height > 0 {
        self.kitty_placements.push(Placement {
            image_id: Id::Kitty(image.id),
            x: rect.top_left.x as i32,
            y: y_pos,
            z: p.z,
            width: dest_size.width,
            height: dest_size.height,
            cell_offset_x: p.x_offset,
            cell_offset_y: p.y_offset,
            source_x, source_y, source_width, source_height,
        });
    }
    Ok(())
}
```

`rect.top_left`/`rect.bottom_right` are `Pin` values (`Rect { top_left: Pin, bottom_right: Pin }`
is defined at `graphics_image.rs:774`, NOT `graphics_storage.rs` — the latter only re-imports
it via `use crate::kitty::graphics_image::{…Rect}` at `graphics_storage.rs:28`); `rect.top_left.x`
is the `CellCountInt` column. `push` never
fails (Zig `append(alloc,...)` OOM → in Rust `Vec::push` aborts on OOM; the reference
`catch` in `kitty_update` swallows the error — faithful behavior is "best effort").

### 1.3 `prep_kitty_virtual_placement` (image.rs ~:603)

```rust
fn prep_kitty_virtual_placement<A: Allocator + Copy>(
    &mut self,
    alloc: A,
    t: &Terminal,
    p: &KittyUnicodePlacement,
    cell_size: CellSize,
) -> Result<(), PrepImageError> {
    let storage = unsafe { &(*t.screens.active).kitty_images };
    let image = match storage.image_by_id(p.image_id) {
        Some(img) => img,
        None => return Ok(()),          // log.warn missing image
    };

    let rp = match p.render_placement(storage, &image, cell_size.width, cell_size.height) {
        Ok(rp) => rp,
        Err(_) => return Ok(()),        // log.warn error rendering
    };
    if rp.dest_width == 0 || rp.dest_height == 0 { return Ok(()); }

    // Virtual placements always take one row and are in-viewport → point_from_pin
    // is Some; Zig `unreachable` on None.
    let viewport = unsafe { &(*t.screens.active).pages }
        .point_from_pin(point::Tag::Viewport, rp.top_left)
        .expect("virtual placement in viewport");
    let vp_y = viewport.coord().y;   // Zig `viewport.viewport.y`

    self.prep_kitty_image(alloc, &image)?;
    self.kitty_placements.push(Placement {
        image_id: Id::Kitty(image.id),
        x: rp.top_left.x as i32,
        y: vp_y as i32,
        z: -1,
        width: rp.dest_width,
        height: rp.dest_height,
        cell_offset_x: rp.offset_x,
        cell_offset_y: rp.offset_y,
        source_x: rp.source_x,
        source_y: rp.source_y,
        source_width: rp.source_width,
        source_height: rp.source_height,
    });
    Ok(())
}
```

Memory-model note: `storage` here is a shared borrow of `(*t.screens.active).kitty_images`
that is passed BY SHARED REF into `render_placement(storage, ...)`. `render_placement`
takes `&ImageStorage` (graphics_unicode.rs:173) — read-only. `self.prep_kitty_image`
mutates `self` (a distinct object from `t`/`storage`), so no aliasing conflict.

### 1.4 `prep_kitty_image` (image.rs ~:731)

```rust
fn prep_kitty_image<A: Allocator + Copy>(
    &mut self,
    alloc: A,
    image: &KittyImage,
) -> Result<(), PrepImageError> {
    self.prep_image(
        alloc,
        Id::Kitty(image.id),
        image.transmit_time.expect("transmitted image has a time"),
        Pending {
            width: image.width,
            height: image.height,
            pixel_format: match image.format {
                Format::Gray => PixelFormat::Gray,
                Format::GrayAlpha => PixelFormat::GrayAlpha,
                Format::Rgb => PixelFormat::Rgb,
                Format::Rgba => PixelFormat::Rgba,
                Format::Png => unreachable!(), // should be decoded by now
            },
            // Zig `@constCast(image.data.ptr)` — safe: prep_image only READS and
            // dupes it into the renderer's own buffer. `image` is our owned clone
            // (§0 R-1) so the ptr is valid for the call. Cast away const:
            data: image.data.as_ptr() as *mut u8,
        },
    )
}
```

`image.transmit_time` is `Option<Instant>` in the port (graphics_image.rs:731) vs
Zig `Instant` (non-optional). A transmitted+placed image always has a time set; we
`expect`. Faithful; if it can ever be `None` for a placed image that is a vt bug, and
`expect` surfaces it (open risk R-2, low). `prep_image` is already FULL (image.rs:616);
it dupes `data_slice()` via `dupe_bytes` into an owned `Pending` and calls
`prep_for_upload` → `convert` (the last seam, §3).

### 1.5 `Image::convert` (image.rs ~:1058) — the swizzle, re-homed pure-Rust

See §3. Signature keeps `-> Result<(), WuffsError>` but `WuffsError` becomes inhabited.

---

## 2. vt-kitty access map (the raw-ptr / borrow model)

The renderer reads terminal state through `t: &Terminal` whose `screens.active`
field is a raw `*mut Screen` (the faithful intrusive-ptr port — memory
`rust-mut-args-strongly-protected` / `intrusive-ptr-and-placeholder-fill`). Every
access below goes THROUGH that raw pointer, never through a `&mut` derived from
`&Terminal`, so Stacked Borrows is satisfied:

| Access | Mutability | Rust expression |
|--------|-----------|-----------------|
| `kitty_images` (read placements/images/dirty) | shared | `unsafe { &(*t.screens.active).kitty_images }` |
| `kitty_images.dirty = false` (kitty_update tail) | write | `unsafe { (*t.screens.active).kitty_images.dirty = false }` |
| `pages` (viewport pins, pointFromPin) | shared | `unsafe { &(*t.screens.active).pages }` |
| geometry `t.width_px/height_px/cols/rows` (inside `pixel_size`/`grid_size`) | shared | passed `&Terminal` into vt methods |

**The one write is the crux (memory-model §5).** `kitty_update` reads `storage`
(shared) in loops, then writes `dirty = false` at the very end. We must not hold any
`&ImageStorage`/`&PageList` across that write. The design scopes every shared borrow
into a block that ends before the final `dirty = false`. This is the exact pattern
`kitty_requires_update` uses for its read (image.rs:324-331), extended by one write.
`self` (the renderer `State`) is a wholly separate object from `*t`, so mutating
`self.images`/`self.kitty_placements` while holding a shared `&(*t...)` never aliases.

The `t: &Terminal` params on all four Kitty methods are READ-only aliases; NO `&mut
Terminal`/`&mut Page`/`&mut Row` is ever taken here, so the raw-alias-crossed-by-&mut
SB-UB pattern the memory note warns about does not arise. (If the adversary wants a
Miri proof it belongs on `.port/phase-2/mirihar-chunk3b/` — but no new unsafe pattern
is introduced beyond the already-shipped `kitty_requires_update` read.)

---

## 3. `Image::convert` + the GPU upload map

### 3.1 The already-landed upload machinery (nothing to design)

`Image::upload` (image.rs:1078) is ALREADY FULLY wired above the `convert` seam:
`prep_for_upload` → `convert` → `Texture::init(api.image_texture_options(Rgba,
true), w, h, Some(data_slice()))` → `map_in_place(self, Ready(texture))`. `State::upload`
(image.rs:134) already walks the map and calls `img.upload(alloc, api)` for every
pending image, unloading the unloading. So once `convert` produces RGBA bytes, the
whole "convert → texture → Ready → draw" path runs unchanged. The ONLY thing the
seam must produce is: pending pixel data in RGBA (premultiplied) format.

`Texture::init` (metal/texture.rs:61) asserts `data.len() == width*height*bpp` with
`bpp = 4` for `Rgba`. So `convert` MUST leave `p.width*p.height*4` bytes. That is the
correctness contract of §3.2.

### 3.2 The swizzle — pure-Rust re-home of `wuffs.swizzle`

wuffs (`ghostty/pkg/wuffs/src/swizzle.zig`) converts each source format to
`RGBA_PREMUL` via `wuffs_base__pixel_swizzler`. wuffs has NO Rust home (confirmed:
it is a vendored C lib under `pkg/wuffs`; no rustty crate binds it). Decision:
**re-home the swizzle as a pure-Rust per-pixel loop inside `image.rs`**, reproducing
wuffs's RGBA_PREMUL output byte-for-byte for the 5 reachable source formats. This
keeps the port's pure-Rust trajectory (no new C dep) and is trivially verifiable.

The wuffs semantics, made explicit (dst = RGBA_PREMUL, 4 bpp):

| src format (Zig) | src bpp | per-pixel dst (R,G,B,A) | premul note |
|------------------|---------|-------------------------|-------------|
| `gray` (Y)               | 1 | `(Y, Y, Y, 255)`        | opaque; premul no-op |
| `gray_alpha` (YA_PREMUL) | 2 | `(Y, Y, Y, A)`          | source treated as ALREADY premultiplied → no multiply |
| `rgb`                    | 3 | `(R, G, B, 255)`        | opaque; premul no-op |
| `bgr`                    | 3 | `(R, G, B, 255)` from `(B,G,R)` | channel swap |
| `bgra` (BGRA_PREMUL)     | 4 | `(R, G, B, A)` from `(B,G,R,A)` | already premul → channel swap only |
| `rgba`                   | 4 | early `return` (no-op)  | already RGBA |

Only `gray`, `gray_alpha`, `rgb`, `bgr`, `bgra` reach the swizzle body (the `rgba`
arm returns before it; Zig's `.rgba => unreachable` inside the switch). Kitty images
transmit as `gray`/`gray_alpha`/`rgb`/`rgba` (command::Format), so in practice only
`gray`/`gray_alpha`/`rgb` swizzle for Kitty; `bgr`/`bgra` come from the overlay/z2d
path. All 5 are specified for completeness/fidelity.

**Byte-fidelity certainty tiers (adversary fix §3.2).** The pure-Rust re-home is
UNVERIFIED against a captured `wuffs` `RGBA_PREMUL` output vector, so rank the arms by
how provable they are:

- **Provably exact (no verification debt):** `gray → (Y,Y,Y,255)`,
  `gray_alpha(YA_PREMUL) → (Y,Y,Y,A)`, `rgb → (R,G,B,255)`, `rgba → noop`. These are
  straight channel-copy/append where src and dst share channel naming, and Metal's
  `.rgba` texture reads memory in R,G,B,A order (proven by shipping ghostty). Kitty
  only ever transmits these four formats.
- **Assumes `RGBA_PREMUL == memory-order R,G,B,A` for the swap:** the `bgr` and `bgra`
  arms. These are overlay-only / low-exposure (Kitty never transmits them, and the
  overlay path is already RGBA), so the risk is contained, but the byte order of the
  swap is an assumption, not a proof.

Verification plan: keep the §4 integration test on **opaque RGB** (unambiguous:
rgb→rgba appends 255), and add a FOLLOW-UP that diffs a `gray_alpha` sample against a
captured `wuffs` reference vector before trusting the premul/swap arms in anger. Do
not treat the `bgr`/`bgra` arms as verified until that reference diff exists.

Proposed body:

```rust
fn convert<A: Allocator>(&mut self, alloc: A) -> Result<(), WuffsError> {
    let p = self.get_pending_pointer().unwrap();      // &mut Pending (already ported)
    // We always upload RGBA; already-RGBA is a no-op.
    if p.pixel_format == PixelFormat::Rgba { return Ok(()); }

    let src = p.data_slice();                          // &[u8], len = w*h*src_bpp
    let src_bpp = p.pixel_format.bpp();
    let px = src.len() / src_bpp;                      // pixel count
    // Overflow guard mirrors wuffs `error{Overflow}` on `src.len * 4 / src_bpp`.
    let out_len = px.checked_mul(4).ok_or(WuffsError::Overflow)?;

    // Allocate the RGBA buffer with the SAME allocator model as `dupe_bytes`
    // (align 1) so `free_data`/`Image::deinit` can release it identically.
    let dst_ptr = alloc_bytes(alloc, out_len).map_err(|_| WuffsError::OutOfMemory)?;
    let dst = unsafe { core::slice::from_raw_parts_mut(dst_ptr, out_len) };

    match p.pixel_format {
        PixelFormat::Gray =>
            for i in 0..px { let y = src[i]; let o = i*4;
                dst[o]=y; dst[o+1]=y; dst[o+2]=y; dst[o+3]=255; },
        PixelFormat::GrayAlpha =>
            for i in 0..px { let y = src[i*2]; let a = src[i*2+1]; let o = i*4;
                dst[o]=y; dst[o+1]=y; dst[o+2]=y; dst[o+3]=a; },
        PixelFormat::Rgb =>
            for i in 0..px { let s=i*3; let o=i*4;
                dst[o]=src[s]; dst[o+1]=src[s+1]; dst[o+2]=src[s+2]; dst[o+3]=255; },
        PixelFormat::Bgr =>
            for i in 0..px { let s=i*3; let o=i*4;
                dst[o]=src[s+2]; dst[o+1]=src[s+1]; dst[o+2]=src[s]; dst[o+3]=255; },
        PixelFormat::Bgra =>
            for i in 0..px { let s=i*4; let o=i*4;
                dst[o]=src[s+2]; dst[o+1]=src[s+1]; dst[o+2]=src[s]; dst[o+3]=src[s+3]; },
        PixelFormat::Rgba => unreachable!(),           // early-returned above
    }

    // `alloc.free(data); p.data = rgba.ptr; p.pixel_format = .rgba;`
    free_raw(alloc, p.data, src.len());                // free old buffer (align 1)
    p.data = dst_ptr;
    p.pixel_format = PixelFormat::Rgba;
    Ok(())
}
```

`alloc_bytes`/`free_raw` are the same `Layout::from_size_align(n,1)` +
`allocate`/`deallocate` primitives already factored as `dupe_bytes`/`free_data`
(image.rs:1208/1227) — `convert` needs a "raw alloc without copy" and a "free by
ptr+len"; either reuse `free_data`(via a temp `Pending`) or add two tiny sibling
helpers. Prefer factoring `dupe_bytes` into `alloc_bytes`+copy so `convert` and
`dupe_bytes` share one allocation primitive (no duplicated `Layout` logic).

**Layout parity + borrow ordering (adversary fix §3.2).** Both allocations MUST use
`align = 1`, byte-identical to `dupe_bytes`, so `free_data`/`Image::deinit` release
them the same way: the old buffer is `Layout(w*h*src_bpp, 1)` (= `src.len()`), the new
RGBA buffer is `Layout(w*h*4, 1)` (= `out_len`). Borrow ordering is load-bearing for
NLL: `src = p.data_slice()` reborrows `&*p`, and `p.data = dst_ptr` writes through `p`.
`src`'s LAST use is `src.len()` inside the old-buffer free, so the free-old-buffer and
the `p.data`/`p.pixel_format` reassignment MUST be the final statements, in that order
— `free_raw(alloc, p.data, src.len())` (last read of `src`), THEN `p.data = dst_ptr;
p.pixel_format = Rgba;` (writes to `p`, `src` now dead). This mirrors Zig's
`alloc.free(data); p.data = rgba.ptr; p.pixel_format = .rgba;` tail exactly and keeps
the shared reborrow from overlapping the mutation.

### 3.3 Inhabiting `WuffsError` (the ONE support-type change)

`WuffsError` is currently uninhabited (`pub enum WuffsError {}`, image.rs:1177, doc'd
as "the `wuffs.Error` set") — correct while the seam is dead. The reference type is
`wuffs.Error = Allocator.Error || error{ WuffsError, Overflow }` = exactly THREE
statuses: **`OutOfMemory`, `WuffsError` (the swizzler-`prepare` status failure),
`Overflow`** (`pkg/wuffs/src/error.zig`, verified). Reachability differs from the
type's nominal set:

- In the reference `swizzle.zig`, the ONLY failures the swizzle path yields are
  `OutOfMemory` (from `alloc.alloc`) and the status `WuffsError` (from
  `pixel_swizzler__prepare`). `Overflow` is NEVER produced by swizzle — it comes from
  the wuffs PNG *decode* path, and rides along only because `convert` shares the whole
  `wuffs.Error` union type.
- In the pure-Rust re-home there is no `pixel_swizzler__prepare`, so the status
  `WuffsError` variant is genuinely unreachable; the hand-coded loop's only truly
  reachable failure is allocation (`OutOfMemory`). The `checked_mul(4)` is a defensive
  guard on `px*4` that in practice cannot trip for any real image buffer (the source
  `Vec` could never hold `> usize::MAX/4` pixels); Zig's swizzle does the equivalent
  `src.len * dst_size / src_size` as a plain (unchecked) multiply.

**Decision (adversary fix §3.3).** Mirror the reference type faithfully — keep the
enum named to match Zig's `wuffs.Error` and carry its full nominal set:

```rust
pub enum WuffsError { OutOfMemory, WuffsError, Overflow }
```

The `WuffsError` (status) variant is present for R1 type-fidelity to
`wuffs.Error` but is unreachable in the pure-Rust convert (no swizzler-`prepare`
call); `Overflow` is likewise carried for type-fidelity and doubles as the defensive
`checked_mul` guard. If a leaner surface is preferred over nominal type-fidelity, the
equally-acceptable alternative is to trim to the single genuinely-reachable variant
`pub enum WuffsError { OutOfMemory }` and drop the `checked_mul` guard (making the
convert loop match Zig's unchecked multiply). Either is faithful; the full-set form is
chosen here because the enum is documented as "the `wuffs.Error` set". What the design
must NOT do is the earlier `{ OutOfMemory, Overflow }` — that is neither the nominal
type set (it drops the source's `WuffsError` variant) nor the minimal reachable set.

**Blast radius (verified small):** `WuffsError` appears only at image.rs:1058
(`convert`), :1066 (`prep_for_upload`, propagates), and :1183 (`UploadError::Wuffs(WuffsError)`).
No exhaustive `match` on `WuffsError` exists anywhere (it was uninhabited, so none
could). `UploadError::Wuffs` is matched only where `UploadError` is consumed —
`State::upload` (image.rs:159) discards it (`is_err()`), so inhabiting the payload
breaks no existing match. This is a strictly additive change to the seam's own type.

---

## 4. The offscreen APC-driven test

Add ONE macOS/Metal-gated integration test (Metal-skip pattern like the existing
`image_empty_lifecycle`, image.rs:1247, and the Chunk-3c golden). It drives a real
`Terminal` via the ported vt stream, runs `update_frame` (which calls `kitty_update`
under the `kitty_requires_update` guard, generic.rs:3767) then `draw_frame`, and
asserts the image reached the GPU.

Template (models `stream_terminal.rs::kitty_graphics_via_apc`, :3431, which already
uploads a 1x2 RGB image via `\x1b_Ga=t,...;////////\x1b\\`):

```rust
#[test]
fn kitty_image_pixel_path() {
    let alloc = Global;
    let Ok(metal) = Metal::init_offscreen(AlphaBlending::Native, W, H) else {
        eprintln!("no Metal device; skipping kitty_image_pixel_path"); return;
    };
    // Build a Terminal, feed a transmit+PLACE APC (a=T places at the cursor):
    //   a=T,f=24 (rgb), s=<w>,v=<h>, c=<cols>,r=<rows>, i=1 ; <base64 rgb> ESC \
    let mut t = Terminal::init(alloc, opts(cols, rows)).unwrap();
    { let mut s = t.vt_stream();
      s.next_slice(b"\x1b_Ga=T,f=24,s=2,v=2,c=1,r=1,i=1;<base64 of 2x2 rgb>\x1b\\"); }

    // Sanity: the image + a placement now live in storage, dirty is set.
    let storage = unsafe { &(*t.screens.active).kitty_images };
    assert!(storage.image_by_id(1).is_some());
    assert!(!storage.placements.is_empty());
    assert!(storage.dirty);

    // Drive the renderer. update_frame → kitty_requires_update==true → kitty_update.
    let mut r = /* Renderer over `metal`, wired like the Chunk-3c golden */;
    r.update_frame(&state_for(&t), true).unwrap();

    // ASSERT the pixel path ran (State-level, deterministic; no readback needed):
    assert!(!r.images.kitty_placements.is_empty());          // a placement computed
    let entry = r.images.images.get(&Id::Kitty(1)).unwrap();
    assert!(entry.image.has_texture() || entry.image.is_pending()); // convert+prep ran
    r.draw_frame(/* offscreen */).unwrap();                  // Ready → drawn
    assert!(r.images.images.get(&Id::Kitty(1)).unwrap().image.has_texture());
    // storage.dirty cleared by kitty_update's tail:
    assert!(!unsafe { &(*t.screens.active).kitty_images }.dirty);

    r.deinit(); t.deinit(alloc);
}
```

Assertion strategy (strongest-that-is-deterministic): assert the STATE the path
produces — (a) `kitty_placements` non-empty (the terminal-walk + containment math
ran), (b) an `Id::Kitty(1)` image entry with a texture after `draw_frame`'s upload
(the `convert`+`prep_image`+`Texture::init` path ran), (c) `storage.dirty` cleared
(the `kitty_update` tail write ran). A full IOSurface pixel readback of the placed
image is a stronger check but couples to exact placement geometry + blending; keep it
as a follow-up if the golden harness already exposes readback (the Chunk-4 golden
does). The State assertions are sufficient to prove all 5 seams executed. Base64 the
raw RGB bytes of a tiny known image (e.g. 2x2 solid) so `convert` (rgb→rgba) has a
verifiable expansion; optionally assert the pending `PixelFormat` is `Rgba` after
`prep_for_upload` for a direct swizzle check.

`Image.zig` carries 0 `test{}` blocks (golden-gated), so this is a rustty-only
integration test — same status as the two existing smoke tests. Place it beside them
in `image.rs` `mod tests` (or a `generic.rs` renderer-level test if a full `Renderer`
is easier to stand up there — the Chunk-3c golden at generic.rs:5608 is the wiring
precedent for `update_frame`/`draw_frame`).

---

## 5. Dead-code + no-regression

- **The 44 existing tests + the golden must still pass unchanged.** The golden
  renders NO Kitty images and has a `None` overlay, so `kitty_requires_update`
  returns false (empty storage, `dirty=false`, no virtual) → `kitty_update` is never
  entered → the filled bodies are not exercised by the golden. The fill is purely
  additive to a guarded slow path. Verify: `cargo test -p rustty-renderer` +
  the golden gate still green.
- **`#[allow(dead_code)]` removal.** FOUR stale allows must come off once the seams
  call for real (adversary fix §5). Three are the Kitty methods —
  `prep_kitty_placement` (image.rs:531), `prep_kitty_virtual_placement` (:602),
  `prep_kitty_image` (:730) — "called only from the kitty_update seam (a comment)".
  The FOURTH, easy to miss, is **`get_pending_pointer` (:1159)**: it too carries
  `#[allow(dead_code)]` ("used only by the `convert` seam … which is a comment") and
  becomes genuinely reached the moment `convert`'s body calls it. REMOVE all four.
  `kitty_update` itself is already called (generic.rs:3771), so it needs no allow.
  `Image::convert` is reached via `prep_for_upload`; it is `#[allow]`-free.
- **The guard is now exercisable.** Filling `kitty_update` means the
  `kitty_requires_update` branch (generic.rs:3767) can now do real work; the new test
  §4 is what exercises it. No other call site changes.
- **`get_pending_pointer`** must exist and return `&mut Pending` for `convert`
  (Zig `getPendingPointer`). It is listed as FULL in the module doc (image.rs:12);
  confirm its signature returns a mutable pending view before wiring `convert`
  (it is the only mutable-pending accessor `convert` needs).
- **No new crate deps, no cross-crate edits.** Everything lands in
  `rustty-renderer/src/image.rs` (+ possibly one test in `generic.rs`). rustty-vt is
  consumed read-only through APIs that already exist. R4 (acyclic) preserved: the
  renderer→vt edge is pre-existing.

---

## 6. Open risks (ranked)

1. **wuffs byte-fidelity (R-3, medium).** The pure-Rust swizzle must match wuffs
   `RGBA_PREMUL` output exactly. The subtle cases are the *premultiplied* source
   formats (`gray_alpha`=YA_PREMUL, `bgra`=BGRA_PREMUL): wuffs treats the source as
   ALREADY premultiplied, so the conversion is a straight channel copy/expand with NO
   alpha multiply. If wuffs instead re-premultiplies (it does not, for PREMUL→PREMUL),
   our output would differ. Mitigation: the §4 test uses opaque RGB (unambiguous:
   rgb→rgba appends 255), and a follow-up can diff a `gray_alpha` sample against a
   captured wuffs reference vector. Kitty only transmits gray/gray_alpha/rgb/rgba, so
   `bgr`/`bgra` (overlay-only, and the overlay is already RGBA) are low-exposure.
2. **`image_by_id` clones (R-1, low).** Extra `Vec<u8>` copy per dirty frame vs Zig's
   shared pointer (§0). Pure perf; correctness-neutral. Could add a borrowing
   `image_by_id_ref` to vt later, but that is a vt change out of this milestone's
   scope — do NOT do it here.
3. **`transmit_time: Option` `expect` (R-2, low).** A placed image with `None`
   transmit_time would panic in `prep_kitty_image`. Believed impossible (transmit sets
   it); if the adversary finds a placement-without-transmit path, replace `expect`
   with an early `return Ok(())` + log.
4. **Sort stability (R-4, low).** `sort_unstable_by` with a `Less`/`Greater`-only
   comparator (never `Equal`) is fine for an unstable sort and matches Zig
   `sortUnstable`; if a strict-weak-ordering debug assert ever fires, switch the
   comparator to return `Equal` on `z==z && !zLessThan && !zGreaterThan`. Not
   expected.
5. **Memory model (R-5, low but load-bearing).** The single `dirty = false` write
   through `*mut Screen` while shared borrows of the same screen were used earlier in
   the fn. The design scopes every shared borrow to end before the write (§2). This is
   the same raw-ptr discipline `kitty_requires_update` already ships; no NEW unsafe
   pattern. If the adversary wants Miri coverage, the harness lives at
   `.port/phase-2/mirihar-chunk3b/`.
