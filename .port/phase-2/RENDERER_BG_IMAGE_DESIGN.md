# Renderer Background-Image Pipeline — Design

Milestone: fill the DEFERRED `rustty-renderer` background-image pipeline (the last
`prep_background_image` seam + the deferred `Path` config type). GPU R-d, the
config/resize surface, and the Kitty image path are DONE + committed; this
milestone activates the full-screen background-image draw.

Status of the two gating pre-checks (verified FIRST, per the ARCHITECT brief):

- **`decode_and_path_feasible = feasible`.** The wuffs-sys PNG/JPEG *decoder*
  entrypoints are real external-linkage symbols already present in the generated
  bindings; only a small, well-understood set of `static inline` base-runtime
  helpers is missing and is trivially exposable (§2.1). The acyclic `bg_image`
  `Path` resolves cleanly to a renderer-local enum (§1). Neither blocker fires.

Design ONLY. This document touches NO `crates/`. It specifies the implementation
milestone that follows.

---

## 0. What is already done vs. what this milestone adds

Confirmed by reading the landed code (`crates/rustty-renderer/src/{generic,image,config}.rs`,
`crates/rustty-config/src/{path,renderer_config}.rs`, `crates/rustty-util/src/file_type.rs`,
`crates-sys/wuffs-sys/{src/lib.rs,build.rs,wrapper.h}`):

| Piece | State | Evidence |
|---|---|---|
| `Renderer.bg_image: Option<image::Image>` field | LANDED | generic.rs:1059 |
| `Renderer.bg_image_buffer: BgImage` + `_modified` | LANDED | generic.rs:1068/1071/1274 |
| `update_bg_image_buffer()` (maps config → `BgImage`/`BgImageInfo`) | LANDED + unit-tested | generic.rs:4052; test `update_bg_image_buffer_maps_config` (5947) |
| `upload_background_image()` (unload/replace/pending → GPU `upload`) | **FULLY IMPLEMENTED** (not a seam) | generic.rs:4487–4507 |
| `draw_frame` bg-image render step (full-screen textured quad, 3-vert, `bg_image` pipeline) | **WIRED** | generic.rs:4976–4988 |
| `change_config` bg_image_changed / bg_image_config_changed dispatch | LANDED (call to `prep_background_image` is the only live seam) | generic.rs:1830–1873 |
| `image::Image` (`Pending`/`Ready`/`Replace`, `mark_for_replace`, `is_pending`, `is_unloading`, `upload`, `deinit`) + `PixelFormat::Rgba` + align-1 raw alloc/free | LANDED (Kitty milestone; REUSABLE) | image.rs:1182,1220,1300,1321,1660,1697 |
| `BgImageGpu` / `BgImage` vertex / `gpu_abi` | LANDED (Chunk 4) | generic.rs:731,739; used at 4875 |
| `FileType::detect` / `FileType::guessFromExtension` | **ALREADY PORTED** (renderer deps rustty-util) | rustty-util/src/file_type.rs:120,146,166 |
| `DerivedConfig.bg_image` field | DEFERRED — typed `Option<Path>` where `Path` is the **inhabited unit-struct placeholder** | generic.rs:122,187,327 |
| `RendererConfig::bg_image()` accessor | **ABSENT** (trait has `background_image_{opacity,repeat,position,fit}` but no path) | config.rs:44–100 |
| `prep_background_image()` | **R5 seam** (`unimplemented!()`, verbatim body preserved) | generic.rs:1904–1957 |
| Safe wuffs `png::decode` / `jpeg::decode` wrapper | **ABSENT** (wuffs-sys is the raw FFI leaf; no parity wrapper exists yet) | wuffs-sys/src/lib.rs docline "safe wrappers live in the parity crates" |

**Net milestone scope is small and sharply bounded:** (a) resolve the renderer-local
`Path` + add `RendererConfig::bg_image` + un-defer `DerivedConfig.bg_image`; (b) add
a safe wuffs decode wrapper (+ the wuffs-sys static-inline shim it needs); (c)
port `prep_background_image` verbatim over `FileType` + the wuffs wrapper, feeding
the ALREADY-wired upload/render. Upload and render need NO code — they are
activated by `bg_image` becoming non-`None`.

---

## 1. The acyclic `bg_image` Path (R4/G7)

### 1.1 Constraint

`rustty-config` DEPENDS ON `rustty-renderer` (the `Message::ChangeConfig`
back-edge — `rustty-config/Cargo.toml:48`). Therefore the renderer may **never**
name a `rustty_config` type. The reference reads `config.@"background-image"`
(a `configpkg.Path`) directly; here the read must go through the renderer-defined
`RendererConfig` accessor trait (`crate::config`), impl'd by `Config` IN
rustty-config (orphan-rule OK; adds no crate edge). This is the exact pattern the
landed `background_image_{fit,position,repeat,opacity}` accessors already use.

### 1.2 The renderer-local `Path` type

Reference config type (`ghostty/src/config/path.zig:18`):
```zig
pub const Path = union(enum) {
    optional: [:0]const u8,   // no error if the file does not exist
    required: [:0]const u8,   // the file is required to exist
    pub fn len(self) usize ...
    pub fn equal(self, other) bool { return std.meta.eql(self, other); }
    pub fn clone(self, arena) !Path ...   // dupeZ each arm
};
```
The renderer only ever consumes the *path bytes* (`prepBackgroundImage`:
`switch (p) { .required, .optional => |slice| slice }` — both arms identical);
the optional/required distinction is carried purely for fidelity + `equal()`
(the change-detect in `change_config`). Because the reference type is
null-terminated (`[:0]const u8`) and the byte payload is opened as an absolute
file path, **`CString` is the faithful carrier** (byte-faithful, null-terminated,
never lossy `String` — MEMORY zig-byteslice-to-rust-rawbytes, paths especially).

Replace the placeholder unit struct (generic.rs:115–122) with:

```rust
/// The renderer-local mirror of the reference `configpkg.Path`
/// (`union(enum) { required: [:0]const u8, optional: [:0]const u8 }`).
///
/// R4/G7: the renderer may NOT name `rustty_config::path::Path`, so `DerivedConfig`
/// carries this renderer-nameable enum instead; `Config` maps its own `Path` into
/// it via `RendererConfig::bg_image` (config-side, acyclic). Byte-faithful: the
/// null-terminated path bytes -> `CString`, never a lossy `String`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Path {
    /// No error if the file does not exist.
    Optional(CString),
    /// The file is required to exist.
    Required(CString),
}

impl Path {
    /// Reference `Path.len` — the byte length of either arm's slice.
    pub fn len(&self) -> usize {
        match self {
            Path::Optional(p) | Path::Required(p) => p.as_bytes().len(),
        }
    }
    /// Reference `Path.equal` (`std.meta.eql`) — variant + bytes equal.
    pub fn equal(&self, other: &Path) -> bool { self == other }
    /// The raw path bytes both `prepBackgroundImage` arms consume
    /// (`switch (p) { .required, .optional => |slice| slice }`).
    pub fn slice(&self) -> &CStr {
        match self {
            Path::Optional(p) | Path::Required(p) => p.as_c_str(),
        }
    }
}
```
Notes:
- **No `Default`.** The placeholder derived `Default` only because it was a
  ZST field-filler; the real enum has no meaningful default and the field type is
  `Option<Path>` (default `None`), so nothing needs `Default`. Grep confirms no
  `DerivedConfig::default()`/`Path::default()` call sites (the two `#[cfg(test)]`
  constructors build `bg_image: None` literally — generic.rs:5263,5394,5535).
- `Clone` replaces the reference arena `clone` (the `CString` owns its bytes;
  `DerivedConfig::init`'s reference `try bg.clone(alloc)` collapses to `.clone()`).
  Document in-body that the arena `dupeZ` is subsumed by owned `CString`.
- `RepeatablePath` (the sibling `custom_shaders` placeholder, generic.rs:124–132)
  stays a placeholder — custom shaders are a separate deferred feature, out of
  scope here. Keep its unit struct + `Default`.

### 1.3 `RendererConfig::bg_image` + the config-side impl

Add to the trait (`crates/rustty-renderer/src/config.rs`, alongside the other
`background_image_*` accessors):
```rust
    // background-image: config `?Path` -> the renderer-local `Path` (§1.2). The
    // optional/required arm + the null-terminated bytes are preserved; `None`
    // means no background image configured.
    fn bg_image(&self) -> Option<crate::generic::Path>;
```
(**ADVERSARY F2 — import correction.** `crates/rustty-renderer/src/config.rs`
imports `crate::generic::{Color, TerminalColor}` at line 27 — extend it to
`{Color, Path, TerminalColor}`. Its `std::ffi` import (line 18) is `use
std::ffi::CString;` only — it does **not** import `CStr`. Since `Path::slice()`
names `&CStr`, change that line to `use std::ffi::{CStr, CString};` wherever the
trait/impl surface names `&CStr`. The earlier draft's claim that "`CStr` is already
imported there" is false and is corrected here.)

Impl in `crates/rustty-config/src/renderer_config.rs` (this file already
`use std::ffi::CString;` and imports renderer types):
```rust
    fn bg_image(&self) -> Option<rustty_renderer::generic::Path> {
        use rustty_renderer::generic::Path as RPath;
        // config `self.background_image` is `Option<crate::path::Path>`
        // (`enum { Optional(Vec<u8>), Required(Vec<u8>) }`, path.rs:34). Map arm
        // for arm; the parsed bytes carry no interior NUL (parse dupeZ'd them),
        // so `CString::new` cannot fail on well-formed config — but map the
        // `Result` defensively (an interior NUL => treat as no bg image, matching
        // the reference's "path won't open" degradation).
        match self.background_image.as_ref()? {
            crate::path::Path::Optional(b) => CString::new(b.clone()).ok().map(RPath::Optional),
            crate::path::Path::Required(b) => CString::new(b.clone()).ok().map(RPath::Required),
        }
    }
```
> **Verify at impl time** the exact field name/type of the config
> `background-image` field (`config_struct.rs` — the `@"background-image": ?Path`
> port). The `Key::BackgroundImage` mapping exists (config_struct.rs:355); confirm
> the struct field is `Option<crate::path::Path>` and adjust the accessor path if
> the port named it differently.

**ADVERSARY F3 — every `RendererConfig` impl must add `bg_image()`.** `bg_image` is
a **required** trait method (preferred over a defaulted one, for fidelity), so it
forces *every* impl to define it — not only the config-side `Config`
(renderer_config.rs, above) but also the renderer's **test `MockConfig`** (the
`struct MockConfig` in `generic.rs`'s `derived_config_tests` module — it already
impls `RendererConfig` alongside `background_image_opacity`/`_repeat`). Add:
```rust
    fn bg_image(&self) -> Option<crate::generic::Path> {
        // T2/T3 need a real path; default the base mock to None and let the
        // per-test config override to Some(Path::Required(CString::new(..))).
        None
    }
```
Grep for any OTHER `impl RendererConfig` before landing (a missing method is a hard
compile error, caught by `cargo build --workspace`); at the time of writing the two
impls are `Config` (rustty-config) and `MockConfig` (renderer test).

### 1.4 Un-defer `DerivedConfig.bg_image`

The field is already `pub bg_image: Option<Path>` (generic.rs:187); once `Path`
becomes the real enum it needs no field-type change. Only the `init<C>` extraction
changes, from the deferred stub (generic.rs:327):
```rust
    // DEFERRED: let bg_image: Option<Path> = None;
    let bg_image = config.bg_image();   // reference: try config.@"background-image" clone
```
Update the surrounding PORT NOTES (generic.rs:115–130,157,291–294,322–327) to
record that `bg_image` is no longer a placeholder (only `custom_shaders` remains).
The `#[cfg(test)]` `DerivedConfig` literals that set `bg_image: None` stay valid.

### 1.5 Acyclicity

New edges: `renderer` gains a dependency on the wuffs safe wrapper (§2) —
`renderer -> rustty-wuffs -> wuffs-sys` (or `renderer -> wuffs-sys` for the
in-crate alternative). **No `renderer -> config` edge** is introduced: the config
read stays behind `RendererConfig` and the impl lives in config. `rustty-wuffs`
is a leaf-ish parity crate (deps: `wuffs-sys` + an allocator shim + `rustty-util`
only if it re-uses `FileType`; it need not). Run `.port/tools/gates/check_acyclic.sh`
(binding G7 = the real Cargo graph) after adding the dependency — expected GREEN
(the new edges are strictly downward toward the `-sys` leaf).

---

## 2. Decode: wuffs-sys usability + the safe wrapper

### 2.1 wuffs-sys feasibility (VERIFIED)

The reference decode path is `wuffs.png.decode` / `wuffs.jpeg.decode`
(`ghostty/pkg/wuffs/src/{png,jpeg}.zig`). Both drive this C surface:

| Symbol | Kind | In generated `bindings.rs`? |
|---|---|---|
| `sizeof__wuffs_png__decoder`, `..._jpeg_...` | fn (accessor) | **yes** |
| `wuffs_png__decoder__initialize` / `..._jpeg_...` | `WUFFS_BASE__MAYBE_STATIC` (external linkage) | **yes** |
| `wuffs_png__decoder__decode_image_config` / `..._jpeg_...` | external | **yes** |
| `wuffs_png__decoder__decode_frame` / `..._jpeg_...` | external | **yes** |
| `wuffs_png__decoder__workbuf_len` / `..._jpeg_...` | external | **yes** |
| `wuffs_base__io_buffer` / `..__image_config` / `..__pixel_buffer` / `..__slice_u8` / `..__color_u32_argb_premul` types | struct | **yes** |
| `WUFFS_BASE__PIXEL_FORMAT__RGBA_NONPREMUL`, `WUFFS_VERSION` | const | **yes** |
| `wuffs_base__pixel_config__{width,height,set}` | **`static inline`** | **no** |
| `wuffs_base__pixel_buffer__set_from_slice` | **`static inline`** | **no** |
| `wuffs_base__make_slice_u8` | **`static inline`** | **no** |
| `wuffs_base__status__message` | **`static inline`** | **no** |
| `WUFFS_BASE__PIXEL_BLEND__SRC` | `#define ((wuffs_base__pixel_blend)0)` | **no** (function-style macro over a cast) |

(Verified by grepping the built `target/**/wuffs-sys-*/out/bindings.rs` and the
vendored `wuffs-v0.4.c`: the decoder entrypoints are `WUFFS_BASE__MAYBE_STATIC`,
which the build leaves at *external* linkage — `WUFFS_CONFIG__STATIC_FUNCTIONS` is
not defined — hence they bind; the base-runtime helpers are unconditionally
`static inline`, which bindgen never emits as linkable FFI.)

**This is NOT a stub or absent API** — the full decoder is present and linkable.
The gap is exactly the class the existing wuffs-sys KAT already works around by
hand: `zeroed_hasher` mirrors `alloc.alloc(sizeof__...)`, `slice_u8` **constructs
`wuffs_base__slice_u8` by hand** (== `make_slice_u8`), and `status_is_ok` **reads
`status.repr` directly** (== the `static inline` `is_ok`). We extend that same
proven technique to the ~5 helpers the decode path needs.

**Recommended exposure (a wuffs-sys additive change — the `-sys` leaf, no
reference-fidelity surface):** add a tiny C shim compiled into the same TU that
wraps each needed `static inline` with an external-linkage function, and allowlist
them in bindgen. Concretely, in `crates-sys/wuffs-sys`:
- `shim.c` (new; compiled by `build.rs` after the main TU) `#include`s
  `wuffs-v0.4.c` *without* `WUFFS_IMPLEMENTATION` (declarations) and defines:
  ```c
  uint32_t rustty_wuffs_pixel_config__width (const wuffs_base__pixel_config* c){ return wuffs_base__pixel_config__width(c); }
  uint32_t rustty_wuffs_pixel_config__height(const wuffs_base__pixel_config* c){ return wuffs_base__pixel_config__height(c); }
  void     rustty_wuffs_pixel_config__set(wuffs_base__pixel_config* c, uint32_t pf, uint32_t ps, uint32_t w, uint32_t h){ wuffs_base__pixel_config__set(c,pf,ps,w,h); }
  wuffs_base__status rustty_wuffs_pixel_buffer__set_from_slice(wuffs_base__pixel_buffer* pb, const wuffs_base__pixel_config* pc, wuffs_base__slice_u8 s){ return wuffs_base__pixel_buffer__set_from_slice(pb,pc,s); }
  const char* rustty_wuffs_status__message(const wuffs_base__status* z){ return wuffs_base__status__message(z); }
  ```
  (`make_slice_u8` and `WUFFS_BASE__PIXEL_BLEND__SRC` need no shim — construct the
  2-field `wuffs_base__slice_u8 { ptr, len }` by hand exactly as the KAT does, and
  the blend enum value is the literal `0`.)
- **`shim.h` (new; the load-bearing bindgen step — ADVERSARY F1).** bindgen emits a
  binding ONLY for a symbol it *parses*; today `wrapper.h` `#include`s
  `wuffs-v0.4.c` and nothing else (verified — its sole non-comment line is
  `#include "wuffs-v0.4.c"`), so it never sees the `rustty_wuffs_*` symbols and
  **allowlisting `rustty_wuffs_.*` alone yields zero bindings.** Add a `shim.h`
  declaring the extern prototypes for the five `rustty_wuffs_*` functions above and
  `#include "shim.h"` from `wrapper.h` (or paste the prototypes into `wrapper.h`
  directly). THEN the bindgen allowlist below emits them. `shim.c` provides the
  definitions; `shim.h`/`wrapper.h` provide the declarations bindgen parses.
- The existing bindgen allowlist matches `wuffs_.*` / `sizeof__wuffs_.*`; extend the
  **function** allowlist with `rustty_wuffs_.*` (only effective once `shim.h` is
  parsed per the bullet above).
- **Alternative** (no shim.c): enable bindgen `wrap_static_fns` (bindgen 0.72,
  already the pinned version), which auto-generates the extern wrappers; `build.rs`
  then compiles the emitted `extern.c` into the archive. Slightly more magical;
  the explicit shim is preferred for determinism + a readable FFI surface.

Either way: **additive to the `-sys` crate, no reference code, no cyclic risk.**
This is a required sub-step of the milestone and MUST land before §2.2 compiles.

### 2.2 The safe decode wrapper — placement decision

The reference lives in `pkg/wuffs` (a standalone package: `main.zig` +
`png.zig`/`jpeg.zig`/`error.zig`/`swizzle.zig`), each with its own `test`s. wuffs-sys's
own lib.rs states "Downstream safe wrappers live in the parity crates; this crate
is the raw FFI leaf." Two placements:

- **(A) New parity crate `crates/rustty-wuffs`** — the faithful mirror of the
  `pkg/wuffs` package boundary. Ports `main.zig -> lib.rs` (`ImageData`,
  `maximum_image_size`), `error.zig` (`Error`, `check`), `png.zig`, `jpeg.zig`
  (+ their `png_decode_000000/FFFFFF/too big` `test`s — R1 requires every `test{}`).
  Renderer deps it. **Bonus fidelity win:** rustty-font's `glyf_rasterize_png_test`
  currently substitutes a pure-Rust `png` dev-dep for `wuffs.png.decode` (a
  documented R1 stand-in); a real `rustty-wuffs` is the eventual home that closes
  that gap too.

  > **ADVERSARY F7 — no pre-existing font→wuffs edge (premise correction).** Verified
  > against `crates/rustty-font/Cargo.toml:48-52`: `wuffs-sys` is **NOT** a dependency
  > of `rustty-font` — the comment there explicitly says wuffs-sys "is NOT pulled in
  > here" and font uses a pure-Rust `png` dev-dep for its KAT. wuffs-sys today has
  > exactly one consumer: its own KAT. So NO impl step in this milestone may assume a
  > `rustty-font -> wuffs-sys` edge already exists; this milestone (via `rustty-wuffs`)
  > is what would *later* let font migrate off its stand-in — it does not depend on
  > that having happened. The framing in this section is the accurate one.
- **(B) A renderer-local module** (`renderer/src/wuffs.rs`, renderer deps
  wuffs-sys directly, decode inlined into/next to `prep_background_image`).
  Lighter (no new crate), but *misplaces* `pkg/wuffs` code into the renderer,
  contradicts the wuffs-sys layering note, and can't be shared with font.

**Recommendation: (A).** It is the faithful package→crate mapping, it isolates the
`unsafe` FFI + the KAT-style tests behind a safe seam, and it is reusable. The
extra cost (a Cargo.toml + porting 4 short files) is small and R1-aligned. If the
milestone owner wants to minimize new surface, (B) is acceptable as a documented
compromise, but this design proceeds on (A).

`swizzle.zig` is NOT on the bg-image decode path (`decode_frame` +
`set_from_slice` with `RGBA_NONPREMUL` performs the format conversion internally),
so port it as an R5 `unimplemented!()` seam sibling in `rustty-wuffs` (verbatim
body) — or omit until a caller needs it, noted in DEFERRALS.

### 2.3 `rustty-wuffs` signatures (port of `pkg/wuffs/src/{main,error,png,jpeg}.zig`)

```rust
// lib.rs  (main.zig)
/// 4 GiB — the reference `image-storage-limit`-derived cap.
pub const MAXIMUM_IMAGE_SIZE: usize = 4 * 1024 * 1024 * 1024;

/// Reference `ImageData { width: u32, height: u32, data: []u8 }`. `data` is the
/// decoded RGBA_NONPREMUL buffer, OWNED by the passed allocator (align 1) so the
/// caller can transfer `data.as_ptr()` into the renderer's `image::Pending.data`
/// and later free it with the SAME allocator/layout (§3.2).
pub struct ImageData<A: Allocator> {
    pub width: u32,
    pub height: u32,
    pub data: RawBuf<A>,   // owns { ptr: NonNull<u8>, len, alloc }; align-1 Layout
}

// error.zig
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error { OutOfMemory, WuffsError, Overflow }   // Allocator.Error || {WuffsError, Overflow}

// png.zig  /  jpeg.zig  — identical bodies bar the `png`/`jpeg` decoder symbols.
pub fn decode<A: Allocator + Copy>(alloc: A, data: &[u8]) -> Result<ImageData<A>, Error>;
```
Body maps `png.zig` line-for-line (R1), all FFI `unsafe`-blocked with the KAT's
SAFETY notes:
1. `alloc` `sizeof__wuffs_png__decoder()` bytes (u64-aligned backing, KAT precedent) → `initialize`.
2. Build `wuffs_base__io_buffer` over `data` (`{ data:{ptr,len}, meta:{ wi:len, ri:0, pos:0, closed:true } }`) — construct the struct literally (matches the reference `.{ ... }`).
3. `decode_image_config` → read `width`/`height` via the `rustty_wuffs_pixel_config__width/height` shims.
4. `rustty_wuffs_pixel_config__set(&pixcfg, RGBA_NONPREMUL, SUBSAMPLING__NONE, w, h)`.
5. `size = w*h*sizeof(color_u32_argb_premul)` with **checked** mul (reference `std.math.mul`); `> MAXIMUM_IMAGE_SIZE → Err(Overflow)`.
6. `destination = alloc.allocate(Layout(size, 1))` (align 1 — matches the renderer's free layout).
7. `work_buffer = alloc.allocate(workbuf_len().max_incl)`; `work_slice = make_slice_u8(ptr,len)` (hand-built struct).
8. `rustty_wuffs_pixel_buffer__set_from_slice(&pb, &pixcfg, make_slice_u8(dest.ptr, dest.len))`.
9. `decode_frame(dec, &pb, &io, PIXEL_BLEND__SRC(=0), work_slice, null)`.
10. return `ImageData { width, height, data: destination }` (transfer ownership; `errdefer`-free on the earlier error arms).
`check(status)` reads `status.repr.is_null()` (KAT `status_is_ok`) and, on error,
logs via the `rustty_wuffs_status__message` shim + returns `WuffsError`.
Port the three `test`s (`decode_000000`/`decode_FFFFFF`/`too big`) — but do NOT
naively copy the reference fixtures (ADVERSARY F5):
- **The `1x1#*` fixtures are NOT present in the reference checkout at all.** Verified:
  `ghostty/pkg/wuffs/src/` contains only `too_big.png`/`too_big.jpg`; `png.zig`/`jpeg.zig`
  `@embedFile("1x1#000000.png")` etc. reference files that do not exist on disk here
  (upstream generates/omits them). So "copy the reference fixtures" is impossible for
  the 1×1 cases. Generate a 1×1 solid PNG/JPEG at build time via the pure-Rust `png`
  encoder (a dev-dep, the same stand-in `rustty-font` already uses) or embed a small
  crafted byte literal, and assert `width==1,height==1,data==[c,c,c,255]`.
- **The `too_big` fixtures are heavy, not "tiny".** Verified: `too_big.jpg` is 12.5 MB
  and `too_big.png` is 1.0 MB — checking those into the parity repo is a real cost. The
  `Overflow` guard triggers on `width*height*4 > MAXIMUM_IMAGE_SIZE` where `width`/`height`
  come from `decode_image_config` (the header parse), **not** on file size (verified in
  `png.zig`: `decode_image_config` → `pixel_config__{width,height}` → checked `mul` →
  `if (size > maximum_image_size) return error.Overflow`, all before any full-frame
  decode). So prefer a **minimal** PNG/JPEG whose *declared* IHDR/SOF dimensions overflow
  (e.g. width=height=65536 ⇒ 65536·65536·4 = 16 GiB > 4 GiB) — a few hundred bytes,
  no multi-MB blob. Consciously accepting the upstream fixture weight is the fallback,
  not the default.

---

## 3. `prep_background_image` — the renderer port

### 3.1 Signature + control flow (verbatim map of generic.zig:1747)

Replace the `unimplemented!()` seam (generic.rs:1951) with the real body; the
verbatim reference comment block already above it (generic.rs:1904–1942) becomes
the port's line-anchored guide. `Error` return type is unchanged (already
`Result<(), Error>`; the `change_config` call site at 1864 already does `?`-shaped
handling).

```rust
fn prep_background_image(&mut self) -> Result<(), Error> {
    // if (self.config.bg_image) |p| load_background: { ... } else { unload }
    let Some(p) = self.config.bg_image.as_ref() else {
        // No path: mark any existing bg image for unload (reference `else` arm).
        if let Some(img) = self.bg_image.as_mut() { img.mark_for_unload(); }
        return Ok(());
    };
    // const path = switch (p) { .required, .optional => |slice| slice };
    //
    // ADVERSARY F6 — borrow discipline: `p`/`path` borrow `self.config` immutably,
    // but the load block later mutates `self.bg_image` (`as_mut()` /
    // `mark_for_replace(self.alloc, ..)`). To keep the borrow checker happy WITHOUT
    // relying on NLL subtleties, copy the path bytes into an OWNED buffer here and
    // drop the `&self.config` borrow before touching `self.bg_image`:
    let path: CString = p.slice().to_owned();   // owned; `self.config` no longer borrowed
    // (Purely mechanical; no fidelity impact — the reference's `path` is a plain
    // slice into config memory, but Rust's aliasing rules require the early copy.)

    // 'load_background: { ... break :load_background; }  — a labeled block whose
    // early exits (open-fail / read-fail / unknown-type) fall through to "no
    // change". Port as an inner closure returning Option<Pending>, or a
    // `'load: { ... break 'load None }`-style helper; on `None` just return Ok.
    let pending: Option<image::Pending> = /* see §3.2 */;

    match pending {
        Some(pend) => {
            // if (self.bg_image) |*img| img.markForReplace(alloc, image) else self.bg_image = image;
            let image = image::Image::Pending(pend);
            if let Some(img) = self.bg_image.as_mut() {
                img.mark_for_replace(self.alloc, image);
            } else {
                self.bg_image = Some(image);
            }
        }
        None => { /* break :load_background — leave bg_image unchanged */ }
    }
    Ok(())
}
```

### 3.2 The load block (open → read → detect → decode → Pending)

```rust
// var file = std.fs.openFileAbsolute(path, .{}) catch { log.warn(...); break; };
// const contents = file.readToEndAlloc(alloc, maxInt(u32)) catch { log.warn(...); break; };
let contents: Vec<u8> = match std::fs::read(OsStr::from_bytes(path.to_bytes())) {
    Ok(b) if b.len() <= u32::MAX as usize => b,          // reference cap: maxInt(u32) = 4 GiB
    Ok(_) | Err(_) => { log::warn!(...); return None; }, // open-or-read failure -> break :load_background
};

// const file_type = switch (FileType.detect(contents)) {
//     .unknown => FileType.guessFromExtension(std.fs.path.extension(path)),
//     else => |t| t };
let file_type = match FileType::detect(&contents) {
    FileType::Unknown => FileType::guessFromExtension(path_extension(path.to_bytes())),
    t => t,
};

// const image_data = switch (file_type) { .png/.jpeg => decode, .unknown/else => warn+break };
let decoded = match file_type {
    FileType::Png  => rustty_wuffs::png::decode(self.alloc, &contents),
    FileType::Jpeg => rustty_wuffs::jpeg::decode(self.alloc, &contents),
    FileType::Unknown => { log::warn!("Cannot determine file type ..."); return None; }
    f => { log::warn!("Unsupported file type {f:?} ..."); return None; }
};
let image_data = match decoded {
    Ok(d) => d,
    Err(e) => return Err(e.into()),   // reference `try` — decode error propagates out of prepBackgroundImage
};

// const image = .{ .pending = .{ width, height, .pixel_format = .rgba, .data = data.ptr } };
// Transfer OWNERSHIP of the decoded buffer to Pending (do NOT free it here):
let (w, h, buf) = image_data.into_raw();   // NonNull<u8> ptr + len, allocator-owned, align-1
Some(image::Pending {
    width: w, height: h,
    pixel_format: image::PixelFormat::Rgba,
    data: buf.as_ptr(),                    // *mut u8 — freed later by Image::deinit / free_pending_data
})
```

**Ownership / allocator invariant (the load-bearing detail).** `image::Pending`'s
buffer is freed by `image.rs::free_pending_data` with `Layout::from_size_align(
p.len(), 1)` over the renderer's `Global` (image.rs:1697). `rustty-wuffs::decode`
therefore MUST allocate `data` over the **same `Global`, align 1**, and
`prep_background_image` transfers the raw pointer WITHOUT freeing (mirrors the
reference, where `image_data.data.ptr` is moved into `.pending` and the
`defer alloc.free(contents)` frees only `contents`, never `destination`). `into_raw`
must *forget* the `RawBuf` so its `Drop` does not double-free. `p.len()` recomputes
as `w*h*4` (Rgba bpp) — **assert this equals the decoded `size`** so the later free
layout matches exactly (an invariant test, §4).

**`path_extension` — a REQUIRED sub-task, not just a note (ADVERSARY F4).** Verified:
NO port of `std.fs.path.extension` exists anywhere in `rustty-util` (grep for
`extension` returns only `FileType::guessFromExtension`, which *consumes* the
dot-inclusive bytes and does not compute them). `guessFromExtension`
(file_type.rs:166) compares dot-**inclusive** (`ext.as_bytes()` for `.exts` like
`".png"`/`".jpg"`), so `std::path::Path::extension` — which *strips* the dot and
mishandles leading-dot files (`.png` as a bare filename) — is **unfaithful** and
must NOT be used. This milestone MUST land a small byte helper that reproduces
`std.fs.path.extension` exactly: return the substring from the last `.` in the
final path component (after the last `/`), or `""` when there is no `.` in that
component (and, per zigstd, when the `.` is the component's first byte — a leading
dot is not an extension). Place it in `rustty-util` next to `FileType`, or inline
in the renderer; either way it is a concrete deliverable, tracked in §7 land order.

### 3.3 Error surface

`rustty_wuffs::Error {OutOfMemory, WuffsError, Overflow}` must map into the
renderer's `Error` (generic.rs:774 — has `OutOfMemory` + `MetalFailed`). The
reference `prepBackgroundImage` returns `!void` whose error set includes the wuffs
`Error` (via `try`). Add wuffs arms to the renderer `Error` (e.g. a `BgImageDecode`
variant, or reuse `OutOfMemory` for OOM + a new `ImageDecode` for `WuffsError`/`Overflow`)
+ a `From<rustty_wuffs::Error>`; keep it faithful to "decode error propagates".
Note the open/read failures do NOT propagate (they `break :load_background` = a
warn + no-op), only the *decode* error propagates — preserve that asymmetry.

### 3.4 upload + render — NO new code

Once `bg_image` is `Some(Image::Pending(..))`, the already-landed machinery runs:
`change_config` (after `prep_background_image`) → next `draw_frame` calls
`upload_background_image` (4487, pending → `bg.upload` → `Image::Ready(texture)`)
→ the bg-image render step (4976) draws the full-screen textured quad
(`pipeline.bg_image`, `bg_image_buffer.raw()`, `vertex_count: 3`) BEFORE the
cells. `update_bg_image_buffer` (already called on `bg_image_config_changed`)
supplies opacity/position/fit/repeat via `BgImageGpu::from`. This design adds
nothing here — the milestone's value is that these paths are finally *reachable*.

---

## 4. Offscreen test plan (macOS/Metal-gated, Metal-skip)

The G3b comment-coverage gate SKIPS rustty-renderer, so tests + the adversary are
the ONLY R1 backstop — make the tests load-bearing.

**T1 — `rustty-wuffs` unit (host, no Metal).** Port the reference `png_decode_000000`,
`png_decode_FFFFFF`, `png: too big`, and the `jpeg` trio: `decode` a 1×1 fixture,
assert `width==1, height==1, data==[r,g,b,255]`, and `too_big → Err(Overflow)`.
This proves the wuffs-sys shim + the safe wrapper end-to-end WITHOUT Metal. Per
§2.3/F5, GENERATE the 1×1 fixtures (pure-Rust `png` encoder dev-dep or a crafted
byte literal — the reference `1x1#*` files are absent from the checkout) and drive
the Overflow case from a minimal image with overflowing *declared* dimensions, NOT
a checked-in 12.5 MB `too_big.jpg`.

**T2 — Path/config mapping unit (host).** Via `MockConfig` (renderer's test config;
the renderer canNOT dep rustty-config): a mock returning
`bg_image() -> Some(Path::Required(CString))` flows through `DerivedConfig::init`
into `DerivedConfig.bg_image == Some(Path::Required(..))`; assert bytes + variant
round-trip (byte-faithful). Also assert `Path::equal` semantics feed
`bg_image_changed` correctly (change None→Some, Some→different, Some→same).

**T3 — end-to-end golden (macOS-gated, Metal-skip).** Mirror the existing
`golden_renders_cursor_fg_bg` harness (generic.rs:29, the Chunk-4 IOSurface
readback):
1. Write a tiny **real PNG** to a temp file (`tempfile`), e.g. a 2×2 solid
   `#3060C0` (distinct from the bg_color) — either `include_bytes!` a checked-in
   fixture or generate via the pure-Rust `png` encoder (dev-dep) to avoid a
   fixture. Point `MockConfig::bg_image()` at its path (`Path::Required`).
2. `Renderer::init` (or `change_config`) → triggers `prep_background_image`
   (decode) → `draw_frame(sync=true)` → `upload_background_image` (GPU texture) +
   the bg render step.
3. Assert: (a) `renderer.bg_image` is `Some(Image::Ready(_))` (texture created);
   (b) read back the IOSurface and assert the sampled bg pixels equal the decoded
   image color (fit/position per `BgImageInfo`), **distinct from the plain
   `bg_color`** — proving the bg-image step drew, not the bg-color fallback step.
4. A negative control: `bg_image() == None` → `bg_image` stays `None`, readback
   shows the plain bg_color (guards against false positives).
   Gate `#[cfg(target_os = "macos")]` + the established Metal-skip (skip when no
   Metal device — same pattern as the Chunk-3b/4 tests).

**T4 — layout invariant (host).** Assert `Pending::len()` (`w*h*4`) equals the
`rustty-wuffs` decoded `size` for the T1 fixtures, so the later
`free_pending_data` align-1 layout provably matches the alloc (guards §3.2 UB).

**No-regression:** the existing renderer tests (incl. `update_bg_image_buffer_maps_config`
and the cursor golden) + the workspace suite must stay green; `cargo build
--workspace` is the backstop for the placeholder→enum + trait-method surface change
(the two `#[cfg(test)]` `DerivedConfig` literals build `bg_image: None`, unaffected).

---

## 5. Dead-code & no-regression boundary

- The `Path` enum, `RendererConfig::bg_image`, and the `DerivedConfig.bg_image`
  extraction become *reachable* (init reads them; `change_config` change-detects
  via `Path::equal`; `prep_background_image` consumes them) — no `#[allow(dead_code)]`
  needed, and the placeholder's `Default` derive is dropped.
- `upload_background_image` + the draw step were dead-but-compiled (bg_image was
  always `None`); this milestone exercises them — remove any "unreachable while
  bg_image==None" PORT NOTES that no longer hold (generic.rs:1860,4972 area).
- `RepeatablePath`/`custom_shaders` stay placeholders (separate deferred feature) —
  do NOT touch; keep their seams + notes.
- wuffs-sys change is purely additive (shim + allowlist); its KAT stays green and
  a new decode KAT (T1) is added. `check_acyclic.sh` GREEN (§1.5).

---

## 6. Open risks

- **R-1 (`std.fs.path.extension` byte port) — CONFIRMED REQUIRED, now scheduled
  (see §3.2 + §7).** No such helper exists in `rustty-util` today (grep empty), and
  `guessFromExtension` needs the dot-inclusive extension bytes; `std::path::Path::
  extension` is NOT faithful. This is a concrete sub-task that MUST land, not a
  wait-and-see note. Low effort. (Only bites files whose magic bytes are unknown
  but whose extension is `.png`/`.jpg` — otherwise `FileType::detect` already
  wins — but it must still be correct.)
- **R-2 (allocator/layout consistency).** `rustty-wuffs::decode` MUST alloc `data`
  over `Global`, align 1, and `prep` MUST transfer-without-free; a mismatch =
  double-free or a bad free `Layout`. Enforced by §3.2 + test T4. This is the one
  genuine UB surface; keep the `RawBuf::into_raw` (forget) discipline explicit.
- **R-3 (wuffs-sys shim linkage).** The static-inline shim adds a second C TU;
  confirm `links = "wuffs"` single-linkage still holds (compile the shim into the
  same `cc` archive, not a separate `links`). The `wrap_static_fns` alternative
  sidesteps a hand shim if preferred.
- **R-4 (new crate vs. module).** §2.2 recommends the `rustty-wuffs` crate; if the
  milestone owner rejects a new crate, fall back to a renderer-local `wuffs`
  module (renderer deps wuffs-sys directly) — same signatures, worse layering.
- **R-5 (config field name).** §1.3 assumes `Config.background_image:
  Option<crate::path::Path>`; verify the exact field ident in `config_struct.rs`
  before writing the accessor.
- **R-6 (Metal golden flakiness / P3 colorspace).** The bg-image shader also draws
  the bg color and may apply display-P3 / linear-blending conversions; pick a bg
  image color whose post-conversion readback is still unambiguously distinct from
  `bg_color`, or assert with a tolerance (mirror the Chunk-4 golden's tolerance).

---

## 7. Compile boundary (build/land order)

1. wuffs-sys: add the static-inline shim + bindgen allowlist; `cargo test -p wuffs-sys` (KAT green).
2. `rustty-wuffs`: new crate; port `main/error/png/jpeg`; `cargo test -p rustty-wuffs` (T1 green).
3. `path_extension` byte helper (`std.fs.path.extension` port — §3.2/R-1) in `rustty-util` next to `FileType` (or inline in the renderer); unit-test the leading-dot + no-dot + final-component cases. `cargo test -p rustty-util`.
4. renderer: `Path` enum (replace placeholder) + `RendererConfig::bg_image` (required method; also add to the test `MockConfig` — F3) + config.rs imports (`Path`, `CStr` — F2) + `DerivedConfig.bg_image` extraction + `prep_background_image` body (own-the-path-bytes-early borrow discipline — F6) + wuffs `Error` map + `rustty-wuffs` dep. `cargo build -p rustty-renderer`.
5. rustty-config: impl `RendererConfig::bg_image`. `cargo build -p rustty-config`.
6. `check_acyclic.sh` (G7) + `cargo build --workspace` + `cargo test -p rustty-renderer` (T2/T3/T4) + full suite.

Everything downstream of the placeholder→enum change is `cargo build --workspace`-guarded.
