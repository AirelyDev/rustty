# VAXIS Chunk 1a — Core value types — DESIGN

**Crate:** NEW `crates/rustty-vaxis` (named `rustty-*` per R4, the vendored-dep-port
pattern like `rustty-z2d`). Ports the VENDORED dep `libvaxis` 0.5.1
(rockorager, hash `BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS`), the interactive
CLI's TUI dependency.

**Source (read-only):**
`/Users/gael/.cache/zig/p/vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS/src/`

**Chunk 1a scope — the CORE VALUE TYPES only, port faithfully:**
`Cell.zig` (221 LOC, 1 test), `Key.zig` (458 LOC, 5 test), `Mouse.zig` (50 LOC),
`GraphemeCache.zig` (20 LOC), and the `Winsize` struct (`main.zig:38`).
Plus the `Image` STRUCT SHAPE (Cell embeds `?Image.Placement`) with zigimg-dependent
method bodies as R5 seams. ~6 `test{}` total (Cell 1 + Key 5).

**DEFERRED this chunk (do NOT port — recorded in DEFERRALS.md):**
`gwidth.zig` (216 LOC, 11 test) + `unicode.zig` (64 LOC). BLOCKED on a
**uucode-0.1.0 width-table decision** — vaxis pins uucode 0.1.0-vanilla and reads
`east_asian_width` / `general_category` / `is_emoji_presentation` (absent from
rustty-unicode's Ghostty-stripped 0.2.0 data). Human decision (Option A extend
rustty-unicode vs Option B vendor uucode-0.1.0 tables) pending. Full analysis lives
in **DEFERRALS.md → "rustty-vaxis Chunk 1b (gwidth/unicode) — uucode-0.1.0 blocker"**.
Any Chunk-1a Cell/Key method that would need grapheme width becomes an R5
`unimplemented!()` seam this chunk (none of the value-type methods actually do —
see §2).

Later chunks add Window/Screen/Tty/Parser/vxfw and the deferred gwidth/unicode.

---

## DISPOSITION: **READY** (for the value-types subset)

The value types are self-contained and **need no unicode data**: `Cell` (+ Style /
Color / Segment / Scale / Character / Hyperlink), `Key` (+ Modifiers / KittyFlags /
all constants / matchers), `Mouse`, `GraphemeCache`, `Winsize`, and the `Image`
struct-shape all compile and pass their tests with **zero external deps**. The only
uucode-blocked surface (`gwidth`/`unicode`) is explicitly OUT of this chunk (§ above,
DEFERRALS.md). Nothing in Chunk 1a is blocked. Proceed.

---

## 1. Per-file mapping (Zig → rustty module)

Target module layout under `crates/rustty-vaxis/src/`:

| Zig source | rustty module | Status | Notes |
|---|---|---|---|
| `Cell.zig` (221 LOC, 1 test) | `cell.rs` | READY | value struct + nested types |
| `Key.zig` (458 LOC, 5 test) | `key.rs` | READY | needs no unicode data |
| `Mouse.zig` (50 LOC) | `mouse.rs` | READY | pure enums/struct |
| `GraphemeCache.zig` (20 LOC) | `grapheme_cache.rs` | READY | 8 KiB ring buffer |
| `main.zig:38` `Winsize` | `lib.rs` (or `winsize.rs`) | READY | 4×u16 |
| `Image.zig` struct-shape | `image.rs` | READY (R5 seams) | shape only; zigimg bodies seamed |
| `gwidth.zig` (216 LOC, 11 test) | `gwidth.rs` | **DEFERRED → 1b** | needs EAW/GC/emoji_presentation |
| `unicode.zig` (64 LOC) | `unicode.rs` | **DEFERRED → 1b** | needs uucode-0.1.0 grapheme iter |

Chunk-1a test total = **Cell 1 (`rgbFromSpec`) + Key 5 = 6** `test{}` — pins
`check_test_counts.py`. (`gwidth`'s 11 tests land with Chunk 1b.)

---

## 2. `cell.rs` (Cell.zig) — READY

Top-level struct fields → a Rust `Cell` struct with `Default`:
`char: Character`, `style: Style`, `link: Hyperlink`, `image: Option<image::Placement>`,
`default: bool`, `wrapped: bool`, `scale: Scale`. Preserve the two doc-comments
(`wrapped`, and `Character.width`).

Nested types, each faithful:

- **`Segment { text: &[u8]/Vec<u8>, style: Style, link: Hyperlink }`** — the `text`
  byte-slice → `Vec<u8>`/`&[u8]`, **NEVER lossy `String`**
  (zig-byteslice-to-rust-rawbytes; these are grapheme/UTF-8 bytes). Style/link default.
- **`Character { grapheme: &[u8] (default b" "), width: u8 (default 1) }`** — grapheme
  is a byte-slice; keep the two-line doc-comment on `width`. For a `Default` impl the
  `grapheme` default `b" "` needs a borrowed-or-owned representation; use `Cow<'a,[u8]>`
  or `&'static [u8]` default (see §7 lifetime note).
- **`CursorShape`** enum (7: default, block_blink, block, underline_blink, underline,
  beam_blink, beam).
- **`Hyperlink { uri: &[u8] (default b""), params: &[u8] (default b"") }`** — byte-slices;
  preserve the `params` doc-comment (`ie "id=app-1234"`).
- **`Scale`** — Zig `packed struct(u13)`:
  `scale:u3=1, numerator:u4=1, denominator:u4=1, vertical_alignment: enum(u2){top=0,bottom=1,center=2}=top`.
  LSB-first bit order → `scale` bits 0-2, `numerator` bits 3-6, `denominator` bits 7-10,
  `vertical_alignment` bits 11-12. Default bitcast value = `1 | (1<<3) | (1<<7) | (0<<11)
  = 137`. `eql` = `@bitCast` both to `u13` and compare. **Port `eql` via an explicit
  `to_u13()` bit-pack** reproducing this layout, then `==` (§6). Preserve both
  "spec allows up to 15, but we limit to 7" comments and the alignment enum comment.
- **`Style`** — `fg/bg/ul: Color (default .default)`, `ul_style: Underline (default .off)`,
  7 SGR bools (`bold, dim, italic, blink, reverse, invisible, strikethrough`, all
  default false). Nested `Underline` enum (off, single, double, curly, dotted, dashed).
  **`eql`** builds an `SGRBits` `packed struct` of the 7 bools (LSB-first: bold=bit0 …
  strikethrough=bit6, i.e. a `u7`), compares the bitcast equality **and** `Color::eql`
  on fg/bg/ul **and** `ul_style ==`. Port `eql` reproducing the SGRBits pack + the
  4-way `and` chain assertion-for-assertion.
- **`Color`** — `union(enum){ default, index: u8, rgb: [3]u8 }` → Rust
  `enum Color { Default, Index(u8), Rgb([u8;3]) }`. Nested:
  - `Kind` = `union(enum){ fg, bg, cursor, index: u8 }` → `enum Kind { Fg, Bg, Cursor, Index(u8) }`.
  - `Report { kind: Kind, value: [u8;3] }` (+ its doc-comment).
  - `Scheme` enum (dark, light).
  - **Methods (all pure, port assertion-for-assertion):**
    - `eql(a,b)` — the manual `switch`-based union compare (default↔default, index↔index
      value-eq, rgb↔rgb elementwise; else false). Do NOT `derive(PartialEq)` as a
      substitute — the hand-written body is what Style.eql calls; but a `derive` here IS
      bit-identical to the manual switch, so `derive(PartialEq)` is acceptable **iff**
      documented as equivalent (keep the manual body faithful anyway for R1).
    - `rgb_from_uint(val: u24)` — mask/shift `r=val>>16, g=val>>8, b=val` truncated to u8
      (preserve the binary-literal masks + comments). Rust has no `u24`; take `u32`
      constrained to 24 bits, or a newtype — keep the mask semantics exact.
    - `rgb_from_spec(spec: &[u8]) -> Result<Color, _>` — XParseColor `rgb:rrrr/gggg/bbbb`:
      `splitScalar(':')`, require prefix `"rgb"`, `splitScalar('/')` three 4-char groups,
      `parseUnsigned(u8, group[2..], 16)` (the HIGH two hex digits). Error variants →
      a `ColorSpecError` enum (`InvalidColorSpec` + parse-int). Preserve the doc-comment.
      **Ports `test "rgbFromSpec"`** (`rgb:aaaa/bbbb/cccc` → 0xAA/0xBB/0xCC).

No Cell/Style/Color method needs Window/Screen/gwidth in Chunk 1a — **Cell is pure
fields + pure methods here**, so the only R5 seam inside `cell.rs`'s neighbourhood is
the `image::Placement` field TYPE (defined in `image.rs`, §3).

---

## 3. `image.rs` (Image.zig) — struct-shape + R5 zigimg/Window seams

`Cell.image: ?Image.Placement`. `Image.zig` imports external `zigimg` and `Window`
(neither used by the CLI's value-type layer). Port the STRUCT SHAPES faithfully; the
methods that touch `Window`/geometry are R5 `unimplemented!()` seams with the verbatim
Zig body preserved as a leading block comment (placeholder-leaf-doctrine — inhabited
placeholder leaves, never exercised by Chunk-1a).

Shapes to port (all plain value fields — no zigimg needed for the SHAPE):
- `Source` = `union(enum){ path: []const u8, mem: []const u8 }` → `enum { Path(Vec<u8>), Mem(Vec<u8>) }`.
- `TransmitFormat` enum (rgb, rgba, png).
- `TransmitMedium` enum (file, temp_file, shared_mem).
- `Placement { img_id: u32, options: DrawOptions }`  ← the field Cell needs.
- `CellSize { rows: u16, cols: u16 }`.
- `DrawOptions` — faithful nested shape, all with defaults + their doc-comments:
  - `pixel_offset: Option<{ x: u16, y: u16 }>` (default None),
  - `z_index: Option<i32>` (default None),
  - `clip_region: Option<{ x: Option<u16>, y: Option<u16>, width: Option<u16>, height: Option<u16> }>`,
  - `scale: enum { none, fill, fit, contain } (default none)`,
  - `size: Option<{ rows: Option<u16>, cols: Option<u16> }>`.
    (The anonymous Zig structs → named helper structs, e.g. `PixelOffset`, `ClipRegion`,
    `ImageSize`, or inline structs; keep field defaults.)
- `Image { id: u32, width: u16, height: u16 }` (+ the pixel-dimension doc-comments) and
  the module const `transmit_opener` (verbatim byte-string).

R5 seams (verbatim body in a comment, `unimplemented!()` leaf — need `Window`/`Screen`,
absent until a later chunk):
- `Image::draw(self, win: Window, opts: DrawOptions) !void` — the fill/fit/contain
  geometry + `win.writeCell`. NOT reachable by Chunk-1a.
- `Image::cellSize(self, win: Window) !CellSize` — `divCeil` pixel-per-cell math.

`transmit_opener` and the `TransmitFormat`/`Medium` enums are shape-only (their consumer
`transmit`/`transmitImage` lives in `Vaxis.zig`, a later chunk).

---

## 4. `key.rs` (Key.zig) — READY

- **`Modifiers`** — `packed struct(u8)`, LSB-first:
  `shift(0), alt(1), ctrl(2), super(3), hyper(4), meta(5), caps_lock(6), num_lock(7)`,
  all default false. `eql` = `@bitCast` both to `u8`, compare. The matchers use
  `std.meta.eql(mods_a, mods_b)` over this packed layout — which for a packed struct is
  the **u8 bitcast equality**. Port `Modifiers::eql` via an explicit `to_u8()` bit-pack
  (bold this bit order — it is load-bearing for the matchers, §6). `std.meta.eql` sites
  in matchers call the SAME u8-equality (a `derive(PartialEq)` on the field-struct is
  bit-equivalent since all fields are bool, but keep the explicit `to_u8` path to make
  the bitcast semantics obvious and R1-faithful).
- **`KittyFlags`** — `packed struct(u5)`, LSB-first, non-trivial defaults:
  `disambiguate(0)=true, report_events(1)=false, report_alternate_keys(2)=true,
  report_all_as_ctl_seqs(3)=true, report_text(4)=true`. Default bitcast `= 0b11101 = 29`.
  (Shape-only in 1a; consumed by tty/Parser later. No methods.)
- **`Key` struct fields** (with doc-comments):
  - `codepoint: u21` → Rust `u32` (holds the u21 range **incl. `multicodepoint =
    1_114_113`** which exceeds `char::MAX` 0x10FFFF, so `char` is WRONG — keep `u32`).
  - `text: Option<&[u8]>/Option<Vec<u8>>` — key text is grapheme **bytes**, NOT `String`
    (zig-byteslice-to-rust-rawbytes). Preserve the ring-buffer-lifetime doc-comment.
  - `shifted_codepoint: Option<u32>` (Zig `?u21`),
  - `base_layout_codepoint: Option<u32>` (Zig `?u21`),
  - `mods: Modifiers (default .{})`.
- **Matchers (all pure — port assertion-for-assertion):**
  - `matches(cp, mods)` = rule1 `matchExact` ∨ rule2 `matchText` ∨ rule3
    `matchShiftedCodepoint` (preserve the 3-rule doc-comment block).
  - `matches_any(cps: &[u32], mods)` — loop.
  - `match_shortcut(cp, mods)` — `base_layout_codepoint == Some(cp) && meta.eql(self.mods, mods)`.
  - `match_shifted_codepoint(cp, mods)` — requires `shifted_codepoint`, `self.mods.shift`;
    clears shift/caps_lock/num_lock on a COPY of `self.mods`, then `cp == shifted &&
    meta.eql(self_mods, mods)`. **Note the Zig asymmetry:** it clears caps/num on
    `tgt_mods` too but then compares `self_mods` against the ORIGINAL `mods` (not
    `tgt_mods`) — reproduce this exactly (it is a real quirk, not a bug to "fix").
  - `match_text(cp, mods)` — clears num_lock/shift/caps_lock on a self-copy; if `cp<128 &&
    (mods.shift || mods.caps_lock)` uppercases via `std.ascii.toUpper` (Rust:
    `(c as u8).to_ascii_uppercase()`, ASCII-only — keep the `<128` guard + the TODO
    comment about zg case_data); `utf8Encode` into a `[u8;4]` (Rust
    `char::from_u32(_cp).encode_utf8(&mut buf)`, `catch return false` → on invalid
    codepoint return false); compare `self.text == buf[0..n] && meta.eql(self_mods, arg_mods)`.
  - `match_exact(cp, mods)` — clears caps_lock/num_lock on both copies; `codepoint == cp &&
    meta.eql(self_mods, tgt_mods)`.
  - `is_modifier()` — the 12-way `left_/right_ × shift/alt/super/hyper/control/meta` OR.
- **113 key-constant `pub const NAME: u21`** — the 5 ascii specials (tab=0x09, enter=0x0D,
  escape=0x1B, space=0x20, backspace=0x7F), `multicodepoint = 1_114_112 + 1`, and the
  PUA-mapped kitty range 57348..57454 (insert..iso_level_5_shift). Zig quoted idents
  `@"f16"`/`@"f32"` → plain Rust `F16`/`F32` consts (values 57379 / 57395). Keep the
  section comments ("a few special keys…", "multicodepoint…", "kitty encodes these…").
  Emit as `pub const NAME: u32 = …`.
- **`name_map`** — Zig `StaticStringMap(u21)` of 117 entries (5 common-name punctuation
  `plus/minus/colon/semicolon/comma` mapping to ascii `'+' '-' ':' ';' ','` + the special
  keys). Port as a **plain const `&[(&[u8], u32)]` (or `&[(&str,u32)]`) slice + a linear
  `get`** — no external `phf` dep (R4 acyclic, no new deps). Ports `test "name_map"`
  (`name_map.get("insert") == insert`; note Zig returns `?u21`, so `get` → `Option<u32>`).
- **5 tests:** `matches 'a'`, `matches 'shift+a'`, `matches 'shift+tab'`, `matches 'shift+;'`,
  `name_map` — port each assertion (`text = "a"` etc. → `Some(b"a")`).

---

## 5. `mouse.rs` (Mouse.zig) — READY

`Mouse = @This()` → a `Mouse` struct with fields
`col: u16, row: u16, xoffset: u16 (=0), yoffset: u16 (=0), button: Button, mods: Modifiers,
type: Type` (`type` is reserved in Rust → field name `kind` or `r#type`; prefer `kind`
with a PORT-NOTE, or `r#type` for verbatim fidelity — choose `r#type` to keep the name).
Nested:
- `Shape` enum (9: default, text, pointer, help, progress, wait, `ew-resize`, `ns-resize`,
  cell). Kebab idents `@"ew-resize"`/`@"ns-resize"` → `EwResize`/`NsResize` (PORT-NOTE).
- `Button` `enum(u8)` with **explicit discriminants** — `left=0, middle=1, right=2, none=3,
  wheel_up=64, wheel_down=65, wheel_right=66, wheel_left=67, button_8=128, button_9=129,
  button_10=130, button_11=131` → `#[repr(u8)]` + explicit values (the gaps matter — SGR
  mouse decode maps raw bytes to these).
- `Modifiers` `packed struct(u3)` — `shift(0), alt(1), ctrl(2)`, default false (distinct
  from Key.Modifiers's u8 — 3 bits only).
- `Type` enum (press, release, motion, drag).
No tests. Preserve the "A mouse event" doc-comment.

---

## 6. `grapheme_cache.rs` (GraphemeCache.zig) — READY

`GraphemeCache { buf: [u8; 8192] (Zig `[1024*8]u8 = undefined`), idx: usize (=0) }`.
`put(&mut self, bytes: &[u8]) -> &mut [u8]` ring-buffer semantics: reset `idx=0` if
`idx + bytes.len > buf.len`, then `defer idx += bytes.len` (Rust: compute the slice range,
copy, bump `idx` at the end), `memcpy` (`copy_from_slice`), return `&mut self.buf[start..end]`.
Faithful, pure, Miri-clean. No tests. Preserve the three comments. Note: Zig `buf =
undefined` → Rust must initialize (`[0u8; 8192]`) — a PORT-NOTE divergence (Zig's
uninitialized read is only ever of bytes it just wrote; `[0u8;8192]` is behaviorally
identical and Miri-safe).

---

## 7. Packed-struct fidelity + Rust representation

Zig `packed struct` fields are **LSB-first**; `@bitCast` to the backing integer is the
canonical equality used by `std.meta.eql`. For each, port a struct-of-fields plus an
explicit `to_<uN>()` bit-pack helper and implement `eql`/`PartialEq` via that integer —
this is the load-bearing bit that makes Key `matches`/`matchExact` and Style/Scale `eql`
faithful:

| type | backing | field → bit | default bitcast |
|---|---|---|---|
| `Key::Modifiers` | u8 | shift0 alt1 ctrl2 super3 hyper4 meta5 caps6 num7 | 0 |
| `Key::KittyFlags` | u5 | disamb0 events1 alt_keys2 all_ctl3 text4 | 29 (0b11101) |
| `Mouse::Modifiers` | u3 | shift0 alt1 ctrl2 | 0 |
| `Cell::Scale` | u13 | scale0-2 num3-6 den7-10 valign11-12 | 137 |
| `Style` SGRBits (in `eql`) | u7 | bold0 dim1 italic2 blink3 reverse4 invisible5 strike6 | — |

`Color` / `Color::Kind` are Zig **tagged unions** → Rust `enum`s (data-carrying). Their
`eql` is a hand-written tag+payload compare — reproduce faithfully (a `derive(PartialEq)`
is bit-equivalent for these but keep the manual body for R1; document equivalence).

**Lifetimes / owned-vs-borrowed (PORT-NOTE, resolve in impl):** Zig's byte-slice fields
(`Segment.text`, `Character.grapheme`, `Hyperlink.uri/params`, `Key.text`) are borrowed
`[]const u8` with caller-managed lifetimes (the `GraphemeCache` ring buffer backs
`Key.text`). In Rust either (a) thread a lifetime `Cell<'a>` / `Key<'a>` with `&'a [u8]`
(most faithful, zero-copy — mirrors the ring-buffer borrow), or (b) `Cow<'a,[u8]>` to
allow a `'static` default (`b" "`, `b""`). Recommend **`&'a [u8]` + explicit `Default`
using `&'static [u8]` defaults** to stay zero-copy and faithful to Zig's borrow model;
`Cow` only if a later chunk needs owned storage. This is the single largest structural
decision — pin it at the top of `cell.rs`/`key.rs` with a PORT-NOTE.

---

## 8. Crate deps (Cargo.toml)

```
[dependencies]
# (none required for Chunk 1a — the value types are self-contained)
```

- **R4 acyclic:** Chunk 1a value types (cell/key/mouse/grapheme_cache/winsize/image-shape)
  need **NO external dep**. `check_acyclic.sh` trivially OK.
- The `rustty-unicode` edge is only needed by the DEFERRED `gwidth.rs`/`unicode.rs`
  (Chunk 1b) — do NOT add it yet (dead dep). When 1b lands, `rustty-vaxis →
  rustty-unicode` is a clean DAG edge (rustty-unicode is a leaf).
- Do **NOT** dep the heavy font/vt/renderer crates (task constraint), nor a `uucode`
  cargo crate (R3 — data must be embedded/derived).

---

## 9. Rules / gates

- **R1** — every line/comment/`test{}` ported assertion-for-assertion (incl. the
  `match_shifted_codepoint` self-vs-tgt asymmetry, the `divCeil` seams, section comments).
- **Byte-slices** — `Key.text`, `Character.grapheme`, `Segment.text`, `Hyperlink.uri/params`,
  `Image::Source` → `&[u8]`/`Vec<u8>`, **NEVER lossy `String`** (zig-byteslice-to-rust-rawbytes).
- **R4** — crate `rustty-vaxis`, acyclic (no deps in 1a).
- **R5** — `Image::draw`/`cellSize` (need Window) = `unimplemented!()` seams + verbatim
  body comment (placeholder-leaf-doctrine); never change a value/shape to silence a lint.
- **R6** — PURE crate ⇒ **Miri primary** (no FFI). Every Chunk-1a type is Miri-able
  (GraphemeCache ring buffer especially).
- **Packed-struct fidelity** — §7: reproduce Zig LSB-first bit layout + bitcast `eql`
  exactly (the matchers rely on `std.meta.eql` over packed structs).
- **Gates:** build / test / `clippy -D` / fmt / `check_acyclic.sh` /
  `check_test_counts.py` (**6** tests: Cell 1 + Key 5) / `check_comment_coverage.py`
  + Miri. (`check_*` live in `.port/tools/gates/`.)

---

## 10. Bottom line

Chunk 1a is **READY** and dep-free. Port `cell`, `key`, `mouse`, `grapheme_cache`,
`Winsize`, and the `Image` struct-shape (with R5 zigimg/Window seams on `draw`/`cellSize`)
now. Pin the five packed-struct bit layouts (§7) so `Key::matches`/`matchExact` and
`Style`/`Scale`/`Color` `eql` are byte-faithful. Decide the borrowed-vs-owned byte-slice
representation up front (§7). `gwidth.rs` + `unicode.rs` are **DEFERRED to Chunk 1b** on
the uucode-0.1.0 blocker (DEFERRALS.md) — do not fake widths; they are out of this chunk.
