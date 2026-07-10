# VAXIS Chunk 4a — `Vaxis.zig` (the main object + the render diff-and-emit engine) — DESIGN

**Crate:** existing `crates/rustty-vaxis` (chunks 1a/1b/2/3a/3b landed). This chunk adds
ONE new module — `vaxis.rs` — by **replacing** the Chunk-3b INHABITED `Vaxis` placeholder
(currently the whole of `vaxis.rs`) with the REAL `Vaxis`, preserving its field/method
surface so `loop_.rs`'s `handle_event_generic` + `winsize_callback` still compile.

**Source (read-only):**
`/Users/gael/.cache/zig/p/vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS/src/Vaxis.zig`
— **1411 LOC, 0 `test{}`**.

**Deps consumed (all intra-crate, already landed — confirmed):**
`cell` (`Cell<'a>`/`Style`/`Color`/`Hyperlink<'a>`/`Character<'a>`/`Scale`/`CursorShape`),
`key` (`f3`, `KittyFlags`), `mouse` (`Mouse`/`Shape`), `screen` (`Screen<'a>`),
`internal_screen` (`InternalScreen`/`InternalCell` — `eql`/`write_cell`/`skip`/`skipped`),
`window` (`Window<'a>` + its fields), `tty` (`TtyWriter`), `ctlseqs` (all the byte
templates), `gwidth` (`gwidth(&[u8], Method) -> u16`, `Method`), `unicode`, `image`
(`Placement`/`DrawOptions`/`Source`/`TransmitFormat`/`TransmitMedium`/`Image`),
`lib::Winsize`. **`zigimg`** (for the two `Image`-decode transmit paths) is **DEFERRED**
(R5 seams).

> **DISPOSITION: ready.** No genuine blocker. The single non-trivial structural decision
> (the `Vaxis<'a>` lifetime and its mechanical threading through `Loop`) is spelled out in
> §2/§3 and is sound + faithful. `placeholder_fill_ok = true`.

---

## 0. The five load-bearing decisions (read this first)

1. **`Vaxis<'a>` gains a lifetime** because it OWNS `Screen<'a>` (whose cells borrow their
   grapheme bytes `&'a [u8]`, landed in Chunk 2). This forces `Loop` to become
   `Loop<'a, T>` and `handle_event_generic`/`winsize_callback`/`SendLoop` to thread `'a`.
   These are **mechanical generics edits to `loop_.rs`** — the router **body** and the
   **field/method surface it names are unchanged** (§3). `placeholder_fill_ok = true`.
2. **`*IoWriter` → `&mut W: std::io::Write` (a generic type param)**, not the concrete
   `&mut TtyWriter`. Zig's `std.io.Writer` is a generic writer; the faithful Rust image is
   `W: std::io::Write`. This makes the render engine **pure over a generic sink** →
   Miri-able (R6) AND directly testable against a `Vec<u8>` (the added smoke test, §12).
   The `loop_.rs` `device_status_report((*tty).writer())` call still compiles (infers
   `W = TtyWriter`).
3. **`tty.print(template, args)` → inline byte emission** (§5). Zig applies the
   `ctlseqs.*` `{d}`/`{s}`/`{c}`/`{x:0>2}` templates at runtime via `std.fmt`. Rust cannot
   feed a `&[u8]` const to `write!`, so the render engine emits inline: `write_all` the
   const sequences; `write!` the numeric slots; `write_all` the raw byte-slice (`{s}`)
   slots (graphemes/URIs are non-UTF-8 bytes — must NOT go through `{}`). Every emit
   carries a comment naming the `ctlseqs` template it mirrors; the smoke test locks the
   exact bytes so any drift is caught.
4. **The render diff-and-emit engine is the fidelity heart** (§6) — ported
   **branch-for-branch**. The two Zig `defer`s and the multiple `continue` exits map to
   explicit statements at each exit point (§6.4); the non-`Copy` `InternalCell` forces a
   small borrow-capture accommodation (§6.5). No behavior changes.
5. **The `zigimg` R5 seams are exactly two functions** — `transmitImage` and `loadImage`
   (they decode/convert an image via `zigimg.Image`). Everything else in the image path
   (`render`'s kitty-graphics emission, `transmitLocalImagePath`, `transmitPreEncodedImage`,
   `freeImage`) is **portable and ported live** (§11). No non-image render logic is dropped.

---

## 1. Vaxis.zig → vaxis.rs — the item map (all 1411 LOC)

| Zig item (line) | Rust | Notes |
|---|---|---|
| `Capabilities` (32) | `pub struct Capabilities` | already exists in placeholder — real one is the SAME 9 fields (§3) |
| `Options` (44) | `pub struct Options { kitty_keyboard_flags: KittyFlags, system_clipboard: bool }` | `system_clipboard_allocator: ?Allocator` (§44–49) → **`system_clipboard: bool`** (default `false`); the allocator's *presence* is what gated `requestSystemClipboard` (`== null → NoClipboardAllocator`), so a `bool` faithfully preserves that guard (§10) while paste/clipboard own their own `Vec` |
| fields 52–103 | `Vaxis<'a>` struct fields | §2 |
| `init` (106) | `Vaxis::init() -> Vaxis<'a>` | alloc dropped; `screen = Screen::default()` (0×0), `screen_last = InternalScreen::init(0,0)` |
| `deinit` (118) | `deinit(&mut self, tty: &mut W)` | `alloc?` dropped → `Drop` of `Vec`s; calls `reset_state` (errors swallowed) |
| `resetState` (128) | `reset_state<W>(&mut self, &mut W) -> io::Result<()>` | §9 |
| `resize` (187) | `resize<W>(&mut self, winsize, &mut W) -> io::Result<()>` | `alloc` dropped; `log.debug` dropped |
| `window` (217) | `window(&mut self) -> Window<'a>` | returns root Window over `&mut self.screen` |
| `enterAltScreen` (231) | `enter_alt_screen<W>` | §9 |
| `exitAltScreen` (238) | `exit_alt_screen<W>` | §9 |
| `queryTerminal` (250) | `query_terminal<W>(&mut self, &mut W, timeout_ns)` | §8; futex `timedWait` → §8.1 |
| `queryTerminalSend` (261) | `query_terminal_send<W>` | §8 |
| `enableDetectedFeatures` (312) | `enable_detected_features<W>` | §8.2; `builtin.os.tag` switch → `cfg`; `getenv` → `std::env::var_os` |
| `queueRefresh` (353) | `queue_refresh(&mut self)` | `self.refresh = true` |
| **`render` (358)** | **`render<W>(&mut self, &mut W) -> io::Result<()>`** | **§6 — the heart** |
| `enableKittyKeyboard` (783) | `enable_kitty_keyboard<W>` | §8.3; needs `KittyFlags` → u5 bits |
| `notify` (791) | `notify<W>` | §10 |
| `setTitle` (801) | `set_title<W>` | §10 (list_themes) |
| `setBracketedPaste` (809) | `set_bracketed_paste<W>` | §10 |
| `setMouseShape` (820) | `set_mouse_shape(&mut self, Shape)` | §10 (list_themes) — no tty |
| `setMouseMode` (825) | `set_mouse_mode<W>` | §10 (list_themes); `log.debug` dropped |
| `translateMouse` (844) | `translate_mouse(&self, Mouse) -> Mouse` | §10 — **the placeholder leaf, now filled** |
| `transmitLocalImagePath` (866) | `transmit_local_image_path<W>` | **portable** (no zigimg); §11 |
| `transmitPreEncodedImage` (925) | `transmit_pre_encoded_image<W>` | **portable**; §11 |
| `transmitImage` (983) | `transmit_image` | **R5 seam** (`zigimg.Image`); §11 |
| `loadImage` (1017) | `load_image` | **R5 seam** (`zigimg.Image`); §11 |
| `freeImage` (1035) | `free_image<W>` | **portable**; §11 |
| `copyToSystemClipboard` (1043) | `copy_to_system_clipboard<W>` | §10 (list_themes); needs base64 **encode** (§5.4) |
| `requestSystemClipboard` (1057) | `request_system_clipboard<W>` | §10 |
| `setTerminalForegroundColor` (1067) | `set_terminal_foreground_color<W>` | §10 |
| `setTerminalBackgroundColor` (1074) | `set_terminal_background_color<W>` | §10 |
| `setTerminalCursorColor` (1081) | `set_terminal_cursor_color<W>` | §10 |
| `queryColor` (1090) | `query_color<W>` | §10 |
| `subscribeToColorSchemeUpdates` (1105) | `subscribe_to_color_scheme_updates<W>` | §10 (list_themes) |
| `deviceStatusReport` (1112) | `device_status_report<W>(&self, &mut W)` | §8 — **the placeholder leaf, now filled** |
| **`prettyPrint` (1121)** | **`pretty_print<W>(&mut self, &mut W) -> io::Result<()>`** | §7 — render's simpler mirror |
| `setTerminalWorkingDirectory` (1396) | `set_terminal_working_directory<W>` | §10; `std.Uri` → §10.4 |
| `wake_query_futex` (placeholder leaf) | `wake_query_futex(&self)` | §8.1 — fills the leaf |

Module-level Zig imports (`atomic`, `base64Encoder`, `zigimg`, `IoWriter`, the type
aliases, `log`) map per §5/§8/§11; `log = std.log.scoped(.vaxis)` and every `log.*` call is
**dropped** (crate precedent — see `internal_screen.rs`/`tty.rs` PORT-NOTEs).

---

## 2. The `Vaxis<'a>` struct

```rust
pub struct Vaxis<'a> {
    pub screen: Screen<'a>,            // the screen we write to
    pub screen_last: InternalScreen,   // last drawn (diff back buffer)
    pub caps: Capabilities,            // = .{}
    pub opts: Options,                 // = .{} — Options { kitty_keyboard_flags: KittyFlags, system_clipboard: bool = false } (§1, §10)
    pub refresh: bool,                 // redraw-all next render (default false)
    pub query_futex: AtomicU32,        // atomic.Value(u32).init(0)
    pub queries_done: AtomicBool,      // atomic.Value(bool).init(true)  ← NOTE default TRUE
    pub next_img_id: u32,              // = 1
    pub sgr: Sgr,                      // enum { Standard, Legacy } = Standard
    pub enable_workarounds: bool,      // = true
    pub state: State,                  // the nested state struct (below)
}

#[derive(Clone, Copy)] pub enum Sgr { Standard, Legacy }

pub struct State {                     // Zig anonymous `state: struct {...}`
    pub alt_screen: bool,
    pub kitty_keyboard: bool,
    pub bracketed_paste: bool,
    pub mouse: bool,
    pub pixel_mouse: bool,
    pub color_scheme_updates: bool,
    pub in_band_resize: bool,
    pub changed_default_fg: bool,
    pub changed_default_bg: bool,
    pub changed_cursor_color: bool,
    pub cursor: CursorPos,             // Zig nested `cursor: struct { row, col }`
}
pub struct CursorPos { pub row: u16, pub col: u16 }
```

**Why `'a`:** `Screen<'a>` (Chunk 2) is `Vec<Cell<'a>>` where `Cell.char.grapheme: &'a [u8]`
— the cells borrow their grapheme bytes from caller memory (segment text). `Vaxis` owns the
`Screen`, so it inherits `'a`. This is the faithful image of Zig's `screen: Screen` +
`Cell.grapheme: []const u8` (caller-kept). `Default`/`init` produce a `Vaxis<'a>` with an
empty (0-length, no-borrow) buffer, so `'a` is free at construction.

**Atomics (from the placeholder PORT-NOTE, preserved):** Zig uses `.unordered`
loads/stores; the Rust port uses `AtomicU32` (`query_futex`) + `AtomicBool`
(`queries_done`), both accessed `Relaxed` (weakest, closest to `.unordered`). **`queries_done`
defaults to `true`** (Zig `init(true)`) — the placeholder's `Default` set it to `false`
(harmless there since only `store` is used, but the real `Vaxis::init`/`Default` must set
`true` to match `queryTerminalSend`'s `store(false)`→`queryTerminal`'s `store(true)` cycle).

**`Capabilities`** — identical to the placeholder's 9 fields (`kitty_keyboard`,
`kitty_graphics`, `rgb`, `unicode: Method`, `sgr_pixels`, `color_scheme_updates`,
`explicit_width`, `scaled_text`, `multi_cursor`), all `bool` except `unicode`, all
default-`false`/`Wcwidth`. **Reused as-is** (moved into vaxis.rs; the placeholder’s
`Capabilities` IS the real one). Same for `State` (superset of the placeholder’s
one-field `State { in_band_resize }`).

---

## 3. Placeholder replacement — surface preservation (the `placeholder_fill_ok` proof)

`loop_.rs` (`handle_event_generic` + `winsize_callback` + `stop`) names exactly this Vaxis
surface. Every member is preserved by the real `Vaxis<'a>`:

| loop_ names | placeholder had | real `Vaxis<'a>` has | preserved? |
|---|---|---|---|
| `vx.caps.explicit_width` (bool `= true`) | ✔ | ✔ | ✅ |
| `vx.caps.unicode` (`= Method::Unicode`) | ✔ | ✔ | ✅ |
| `vx.caps.scaled_text`/`kitty_keyboard`/`kitty_graphics`/`rgb`/`sgr_pixels`/`color_scheme_updates`/`multi_cursor` | ✔ | ✔ | ✅ |
| `vx.screen.width_method` (`= Method::Unicode`) | fake `Screen{width_method}` | **real** `Screen<'a>` (HAS `width_method`) | ✅ |
| `vx.state.in_band_resize` (`= true`) | fake `State{in_band_resize}` | **real** `State` (HAS `in_band_resize`) | ✅ |
| `vx.queries_done.load/store(Relaxed)` | `AtomicBool` | `AtomicBool` | ✅ |
| `vx.query_futex` (via `wake_query_futex`) | `AtomicU32` | `AtomicU32` | ✅ |
| `vx.opts` | `Options{}` | `Options{kitty_keyboard_flags, system_clipboard}` (superset) | ✅ |
| `vx.translate_mouse(&self, Mouse)->Mouse` | `unimplemented!()` leaf | **filled** (§10, real body) | ✅ |
| `vx.device_status_report(&self, &mut W)->Result` | `unimplemented!()` leaf | **filled** (§8) | ✅ |
| `vx.wake_query_futex(&self)` | `unimplemented!()` leaf | **filled** (§8.1) | ✅ |

**The field/method surface + the router body are unchanged → `placeholder_fill_ok = true`.**

**The one mechanical edit to `loop_.rs`** (generics threading, NOT a logic change): because
`Vaxis` becomes `Vaxis<'a>`, the raw-pointer types that name it must thread `'a`:

- `pub struct Loop<T>` → `pub struct Loop<'a, T>` with field `vaxis: *mut Vaxis<'a>`.
- `SendLoop<T>(*mut Loop<T>)` → `SendLoop<'a, T>(*mut Loop<'a, T>)` (+ the `unsafe impl Send`).
- `impl<T: LoopEvent> Loop<T>` → `impl<'a, T: LoopEvent> Loop<'a, T>`; `Loop::new(tty, vaxis: *mut Vaxis<'a>)`.
- `winsize_callback` / `tty_run` / `handle_event_generic` gain `<'a, …>` and use
  `*mut Vaxis<'a>` / `*mut Loop<'a, T>`. **Their bodies are byte-identical.**
- `lib.rs` `pub use loop_::Loop` is unchanged (re-exports the generic item).
- The `#[ignore]`d `loop_` test constructs `Vaxis::default()` → `Vaxis<'a>` (inferred);
  `Loop::<TestEvent>::new(&mut tty, &mut vx)` infers `'a`. Compiles unchanged in body.

> This is the established discipline: the C3B design already contemplated threading a
> lifetime through `Loop<T>` (the `'buf` note), and `cargo build --workspace` is the
> backstop (intrusive-ptr-and-placeholder-fill). The threading is purely at the
> type-parameter level; no method body in `loop_.rs` changes.

**None of the loop_-named methods depend on `'a`** (they touch caps/state/screen.width_method/
queries_done/query_futex — never the borrowed graphemes), so the threading is a pure
annotation exercise with no borrow-checker friction.

---

## 4. The generic-writer mapping (`*IoWriter` → `&mut W: std::io::Write`)

Zig `Vaxis` methods take `tty: *IoWriter` (`std.io.Writer`, a generic buffered writer) and
call `writeAll`/`writeByte`/`print`/`flush`. The faithful Rust image is a **generic type
parameter** `W: std::io::Write`:

| Zig | Rust |
|---|---|
| `tty.writeAll(bytes)` | `w.write_all(bytes)?` |
| `tty.writeByte(b)` | `w.write_all(&[b])?` |
| `tty.print(tmpl, args)` | inline emission — §5 |
| `tty.flush()` | `w.flush()?` |
| `... catch {}` | `let _ = w.write_all(...);` (error swallowed) |

Every Vaxis method that writes becomes `fn foo<W: std::io::Write>(…, tty: &mut W, …) -> std::io::Result<()>`.
Benefits: (a) `TtyWriter` (native) and `Vec<u8>` (Miri + the §12 smoke test) both satisfy
`W`; (b) render is **pure over the sink** → Miri-clean (R6); (c) `loop_.rs`'s
`device_status_report((*tty).writer())` (passing `&mut TtyWriter`) still compiles (infers
`W = TtyWriter`). `Vec<u8>`'s `Write::flush` is a no-op — the `flush()?` calls are inert
under test, real under `TtyWriter`.

**Error model:** Zig methods are `!void`. Rust returns `std::io::Result<()>` and uses `?`.
`TtyWriter::write` is infallible (it just extends its `Vec`; only `flush` hits `libc::write`),
so under normal operation the `?`s never early-return — but the signature is faithful to
Zig's `!void`.

---

## 5. The `ctlseqs` format-template application seam

Zig `ctlseqs.*` are `&[u8]` templates with `std.fmt` verbs; `tty.print(tmpl, args)` applies
them at runtime. Rust can't feed a `&[u8]` const to `write!`. **Decision: inline emission**,
one construct per verb, each site commented with the `ctlseqs` name it mirrors. (This is the
"format-application seam" the `ctlseqs.rs` PORT-NOTE deferred to "a later chunk" — this
chunk.) The non-parametrized consts are emitted verbatim via `w.write_all(ctlseqs::X)?`.

### 5.1 Verb → Rust mapping

| Zig verb | example template | Rust emission |
|---|---|---|
| `{d}` (decimal) | `cup = "\x1b[{d};{d}H"` | `write!(w, "\x1b[{};{}H", row+1, col+1)?` |
| `{c}` (char byte) | `t={c}` (medium_char `'f'`/`'t'`/`'s'`) | `write!(w, "{}", medium_char as char)?` |
| `{x:0>2}` (hex, w2, zero-pad) | `osc10_set rgb:{x:0>2}…` | `write!(w, "…{:02x}…", …)?` |
| `{s}` (**byte slice**) | `osc8 = "\x1b]8;{s};{s}\x1b\\"` | **split + `write_all`** the raw bytes (below) |

**`{s}` is byte-slice interpolation and MUST use `write_all`** — graphemes/URIs are
non-UTF-8 bytes, so `{}`/`{s:?}` is wrong (lossy + escaped). Split the template at the `{s}`
and `write_all` the const pieces around the byte slice. E.g. `osc8`:

```rust
// ctlseqs::osc8 = b"\x1b]8;{s};{s}\x1b\\"  (params, uri)
w.write_all(b"\x1b]8;")?; w.write_all(ps)?; w.write_all(b";")?; w.write_all(uri)?; w.write_all(b"\x1b\\")?;
```

Likewise `osc2_set_title` (`\x1b]2;{s}\x1b\\`), `osc22_mouse_shape`, `explicit_width`
(`\x1b]66;w={d};{s}\x1b\\` — mixed: `write!` the `{d}` piece, `write_all` the grapheme),
`scaled_text*`, `osc8`, `osc52_clipboard_copy`, and the kitty transmit templates. The
`scaled_text` grapheme and `cell.char.grapheme` are byte slices → `write_all`.

> **Fidelity guard against drift:** the `ctlseqs::X` const stays the single source of the
> exact bytes for the const sequences; for the parametrized ones the inline `write!`/
> `write_all` reproduces the template, and the §12 smoke test asserts the **exact emitted
> bytes** for a known state, so any divergence from the Zig template is caught mechanically.

### 5.2 `@tagName` (mouse shape / `osc22`)

`ctlseqs.osc22_mouse_shape` = `"\x1b]22;{s}\x1b\\"` with `@tagName(self.screen.mouse_shape)`.
`Shape` (mouse.rs) is a Rust enum; add a small `fn shape_tag_name(Shape) -> &'static [u8]`
returning Zig's exact tag bytes (`default`/`text`/`pointer`/`help`/`progress`/`wait`/
`ew-resize`/`ns-resize`/`cell` — **note the kebab** `ew-resize`/`ns-resize`, matching Zig's
`@"ew-resize"`). Emit `w.write_all(b"\x1b]22;")?; w.write_all(tag)?; w.write_all(b"\x1b\\")?;`.

### 5.3 `@intFromEnum` (cursor shape)

`cursor_shape = "\x1b[{d} q"` with `@intFromEnum(self.screen.cursor_shape)` and
`@intFromEnum(Cell.CursorShape.default)` (= 0). `CursorShape` (cell.rs) is a fieldless enum
in declaration order `Default=0,BlockBlink=1,…Beam=6`; add `fn cursor_shape_int(CursorShape)
-> u8` (an `as u8` on a `#[repr(u8)]`, or a match) — reproducing `@intFromEnum` exactly.
Same for `scale.vertical_alignment` (`@intFromEnum` — `VerticalAlignment` already carries
explicit discriminants `Top=0,Bottom=1,Center=2`).

### 5.4 base64 **encode** (clipboard + image transmit)

`copyToSystemClipboard` (list_themes-critical), `transmitLocalImagePath`,
`transmitPreEncodedImage` use `std.base64.standard.Encoder` (`calcSize` + `encode`).
`parser.rs` already ports the matching **decoder** (`base64_standard_decode`). Add a
module-local `fn base64_standard_encode(&[u8]) -> Vec<u8>` — a faithful port of
`std.base64.standard.Encoder` (standard alphabet `A–Za–z0–9+/`, `=` padding). ~25 LOC, no
external dep (keeps the R4 leaf; only `libc` is a dep). `calcSize(n) = ((n + 2) / 3) * 4`.

---

## 6. `render` — the diff-and-emit engine (THE FIDELITY HEART) — §358–781

Signature: `pub fn render<W: std::io::Write>(&mut self, tty: &mut W) -> std::io::Result<()>`.
Single-threaded `&mut self`: `screen` and `screen_last` are separate owned fields, so all
reads/writes are plain field access — **no raw-pointer gymnastics** (unlike loop_).

### 6.1 Prologue (§358–404) — ported verbatim

1. `defer self.refresh = false;` → §6.6 (a reset that runs on ALL exits).
2. Two `assert`s: `screen.buf.len == width*height` and `== screen_last.buf.len` →
   `assert!(…)` (or `debug_assert!` — keep `assert!` to match Zig's always-on `assert`).
3. `writeAll(sync_set)`; **`errdefer writeAll(sync_reset)`** → §6.7.
4. `writeAll(hide_cursor)`; then alt-screen branch: `if alt_screen { write_all(home) } else
   { write_all(b"\r"); for _ in 0..state.cursor.row { write_all(ri) } }`.
5. `writeAll(sgr_reset)`.
6. Locals: `reposition=false`, `row=0`, `col=0`, `cursor: Style = default`,
   `link: Hyperlink = default`, `cursor_pos_row=0`, `cursor_pos_col=0` (the nested
   `cursor_pos` anon struct → two `u16` locals).
7. `if caps.kitty_graphics { write_all(kitty_graphics_clear) }`.
8. **Reset skip flags:** `for last_cell in &mut self.screen_last.buf { last_cell.skip = false; }`.

### 6.2 The main `while i < screen.buf.len` loop (§405–732) — the diff

Per cell (branch-for-branch):

- `let cell = self.screen.buf[i];` — `Cell` is `Copy`, so this copies (no borrow held on
  `screen`; `cell.char.grapheme` borrows `'a`, fine).
- **width `w`** (§408–414): `if cell.char.width != 0 { cell.char.width as u16 } else {
  max(1, gwidth(cell.char.grapheme, caps.unicode)) }`.
- **wrap check** (§426–432): `if col >= screen.width { row += 1; col = 0; if !cell.wrapped
  { reposition = true } }`.
- **the skip/same fast-path** (§435–449): capture from `&screen_last.buf[i]` (non-`Copy`,
  §6.5): `let last_skip = ...; let last_skipped = ...; let last_eql = last.eql(&cell);`.
  Then `if (!self.refresh && last_eql && !last_skipped && cell.image.is_none()) || last_skip
  { reposition = true; if link.uri.len() > 0 { write_all(osc8_clear)? } ; <ADVANCE>;
  continue; }` — **the ADVANCE (§6.4) runs before continue**, the cursor/link update does NOT.
- `self.screen_last.buf[i].skipped = false;` (§450).
- **back-buffer store** (§455–456) — **"Set this cell in the last frame":**
  `self.screen_last.write_cell(col, row, &cell);`. This sits **between** the §450
  `skipped = false` write and the §459 scaled-skip marking, and runs on the non-skip paths
  only (it is past the §441–448 skip-`continue`). **This is the diff's back-buffer update
  that makes the NEXT render's suppression work** — dropping it silently corrupts every
  subsequent frame (each cell would re-emit forever because `screen_last` never learns the
  new content). `InternalScreen::write_cell` takes `&Cell`, so pass `&cell` (Zig passes
  `cell` by value; the borrow is faithful — `write_cell` copies into the internal cell).
  It is currently guarded only by the §12 second-render suppression assertion — named here
  in the spec so it is not lost.
- **scaled-text skip marking** (§459–474): `if caps.scaled_text && cell.scale.scale > 1 {
  assert!(cell.char.width > 0); … set screen_last.buf[skipped_i].skip = true … }`.
- **reposition** (§477–497): if `reposition`, reset it + `link = default`; alt-screen →
  `write!(cup, row+1, col+1)`; else the relative-move logic (`cursor_pos_row == row` →
  `cuf(n)` if `n=col-cursor_pos_col > 0`; else emit `n=row-cursor_pos_row` `\n`s, a `\r`,
  and `cuf(col)` if `col>0`).
- **image emission** (§499–530) — **PORTABLE, ported live**: from `cell.image: Option<Placement>`,
  `write!(kitty_graphics_preamble, img.img_id)` then the optional `pixel_offset`
  (`,X={d},Y={d}`), `clip_region` (`,x=`/`,y=`/`,w=`/`,h=`), `size` (`,r=`/`,c=`), `z_index`
  (`,z={d}`), then `write_all(kitty_graphics_closing)`. (No `zigimg` here — the `Placement`
  is a plain value; §11.)
- **SGR minimization** (§536–676) — emit **only on change vs the tracked `cursor` Style**.
  Branch-for-branch, all preserved:
  - fg (§536): `if !Color::eql(cursor.fg, cell.style.fg) { match cell.style.fg { Default →
    fg_reset; Index(idx) → 0..=7 fg_base(idx) / 8..=15 fg_bright(idx-8) / else sgr:{standard
    fg_indexed | legacy fg_indexed_legacy}(idx); Rgb → sgr:{standard fg_rgb | legacy
    fg_rgb_legacy} } }`. **The `sgr` field selects standard/legacy** at every indexed/rgb site.
  - bg (§560): mirror with `bg_*`.
  - ul color (§584): `Default→ul_reset; Index→sgr{ul_indexed|ul_indexed_legacy}; Rgb→ if
    enable_workarounds { ul_rgb_conpty } else sgr{ul_rgb|ul_rgb_legacy}`.
  - ul_style (§604): `if cursor.ul_style != cell.style.ul_style` → the 6-arm match
    (`off/single/double/curly/dotted/dashed` → the const).
  - bold (§616): on change → `bold_set`/`bold_dim_reset`; **and if `cell.style.dim` also
    emit `dim_set`** (the bold/dim reset interaction — preserve).
  - dim (§627): on change → `dim_set`/`bold_dim_reset`; **and if `cell.style.bold` also emit
    `bold_set`**.
  - italic/blink/reverse/invisible/strikethrough (§638–676): each `if changed` → set/reset.
- **url / OSC-8** (§679–687): `if link.uri != cell.link.uri { let mut ps = cell.link.params;
  if cell.link.uri.is_empty() { ps = b""; } emit osc8(ps, cell.link.uri) }` (§5.1 split).
- **scaled text** (§690–722): `if caps.scaled_text && !cell.scale.eql(default) { match
  cell.scale.denominator { 0 → unreachable!(); 1 → scaled_text(scale, w, grapheme); else →
  scaled_text_with_fractions(scale, w, num, den, @intFromEnum(v_align), grapheme) };
  cursor_pos_col = col + w*scale; cursor_pos_row = row; <ADVANCE + cursor/link update>;
  continue; }` — **both defers run before this continue** (§6.4).
- **glyph emit** (§724–731): `if caps.explicit_width && w > 1 { emit explicit_width(w,
  grapheme) } else { write_all(cell.char.grapheme) }; cursor_pos_col = col + w;
  cursor_pos_row = row;` then loop-end ADVANCE + cursor/link update.

### 6.3 Epilogue (§733–781) — ported verbatim

- **cursor show/position** (§733–763): `if screen.cursor_vis { alt → cup(cursor_row+1,
  cursor_col+1); else the relative `\r` + `\n`×(cursor_row-cursor_pos_row) OR
  `ri`×(cursor_pos_row-cursor_row) + `cuf(cursor_col)` if `>0`; then set
  `state.cursor.{row,col}`; write_all(show_cursor) } else { set state.cursor from
  cursor_pos }`.
- **mouse shape** (§764–770): `if screen.mouse_shape != screen_last.mouse_shape { emit
  osc22_mouse_shape(@tagName); screen_last.mouse_shape = screen.mouse_shape }` (§5.2).
- **cursor shape** (§771–777): `if screen.cursor_shape != screen_last.cursor_shape { emit
  cursor_shape(@intFromEnum); screen_last.cursor_shape = screen.cursor_shape }` (§5.3).
- `write_all(sync_reset)?; w.flush()?;`.

### 6.4 The `defer` translation (the most error-prone point)

Zig has **two** `defer`s in the loop body:
- **ADVANCE** (§415–425, declared FIRST → runs LAST on every exit incl. every `continue`):
  `assert!(w > 0); for j in (i+1)..(i+w) { if j >= screen_last.buf.len { break } screen_last.buf[j].skipped = true } col += w; i += w;`
- **cursor/link update** (§451–454, declared AFTER the skip-continue → runs on the
  scale-continue and normal-end **but NOT the skip-continue**): `cursor = cell.style; link =
  cell.link;`.

Rust has no `defer`. **Map:** place the ADVANCE explicitly at all THREE exits (skip-continue,
scale-continue, loop-end); place the cursor/link update at the scale-continue and loop-end
ONLY. Since ADVANCE reads `w` + mutates `i`/`col` and the update reads `cell` (independent),
declaration order is immaterial. Recommended shape — a labeled block so `continue` becomes
`break 'body` and the ADVANCE runs exactly once after it:

```rust
while i < self.screen.buf.len() {
    let cell = self.screen.buf[i];
    let w = /* … */;
    // wrap check … (mutates row/col/reposition)
    'body: {
        // skip fast-path: … if skip { reposition=true; osc8_clear?; break 'body; }  // ← ADVANCE only
        // (past here the cursor/link update MUST run) …
        self.screen_last.buf[i].skipped = false;                 // §450
        self.screen_last.write_cell(col, row, &cell);            // §455–456 back-buffer store (pass &cell)
        // scaled continue: … break 'body;  // after setting cursor/link update inline
        // normal glyph emit …
        // cursor/link update (cursor = cell.style; link = cell.link) runs on the two non-skip exits
    }
    // ADVANCE (runs on every path out of 'body):
    debug_assert!(w > 0);
    let mut j = i + 1; while j < i + (w as usize) { if j >= self.screen_last.buf.len() { break } self.screen_last.buf[j].skipped = true; j += 1; }
    col += w; i += w as usize;
}
```

The cursor/link update, being skipped on the skip-path, is placed inside `'body` after the
skip check (so it's reached on the scale-continue and normal-end paths). **The design leaves
the exact factoring to the porter but pins the invariant:** ADVANCE on every iteration
exit; cursor/link update on non-skip exits only. The adversary must re-check this against
§415–454.

### 6.5 Non-`Copy` `InternalCell` borrow accommodation

Zig `const last = self.screen_last.buf[i]` is a **copy**; `InternalCell` (owns three `Vec`s)
is **not `Copy`** in Rust. The fast-path reads `last.eql(cell)`, `last.skipped`, `last.skip`
and THEN (later, §450) writes `self.screen_last.buf[i].skipped = false`. Capture the three
needed values into `bool` locals **before** any `&mut self.screen_last` write, releasing the
`&self.screen_last.buf[i]` borrow first:

```rust
let (last_eql, last_skipped, last_skip) = {
    let last = &self.screen_last.buf[i];
    (last.eql(&cell), last.skipped, last.skip)
};
```

Then all subsequent `screen_last` mutations are conflict-free. Value-identical to Zig.

### 6.6 `defer self.refresh = false`

Must run on ALL exits (incl. the `?` early-returns and the two `assert`s? — no, `assert`
panics; Zig `assert` also aborts, so no defer runs then; match by placing the asserts before
any state). For the `Ok`/`Err` exits use an inner helper returning `Result` and reset after,
**or** a tiny drop guard. Recommended: structure `render` as `let r = self.render_inner(tty);
self.refresh = false; r` — the reset runs on both `Ok` and `Err` (faithful to `defer`), and
avoids a guard type. `render_inner` holds the `errdefer` (§6.7).

### 6.7 `errdefer writeAll(sync_reset)`

Zig `errdefer tty.writeAll(ctlseqs.sync_reset) catch {}` (§368) runs `sync_reset` ONLY on an
error return between §368 and the normal §779 `sync_reset`. Rust: in `render_inner`, wrap the
body so that on the first `Err` we emit `sync_reset` (swallowing its own error) and propagate:

```rust
fn render_inner<W: Write>(&mut self, w: &mut W) -> io::Result<()> {
    w.write_all(ctlseqs::sync_set)?;
    let res = (|| -> io::Result<()> { /* §374 … §779 incl. the final sync_reset + flush */ })();
    if res.is_err() { let _ = w.write_all(ctlseqs::sync_reset); }  // errdefer
    res
}
```

(The normal path's §779 `sync_reset` + §780 `flush` live inside the inner closure; the
`errdefer` only fires on the error path — matching Zig, where a successful run emits
`sync_reset` once via §779 and the errdefer is disarmed.)

---

## 7. `pretty_print` — §1121–1393 (render's simpler mirror)

`pub fn pretty_print<W: Write>(&mut self, tty: &mut W) -> io::Result<()>`. Same diff-shape as
render but **stateless** (no `screen_last` diff, no refresh, no sync/futex):

- `if self.state.alt_screen { return Err(NotInPrimaryScreen) }` — map Zig
  `error.NotInPrimaryScreen` to an `io::Error` (`io::ErrorKind::Other`, message
  `"NotInPrimaryScreen"`) OR a small crate error enum; since the signature is `io::Result`,
  use `io::Error::other("NotInPrimaryScreen")`. (list_themes/boo never call `pretty_print`
  in alt-screen, so the branch is untested but ported.)
- `hide_cursor`; `sync_set`; **two `defer`s** — and their **emission ORDER is pinned**.
  Zig defers run **LIFO** at scope exit, and both fire **AFTER** the normal body's final
  `\r\n` write (§1391) and `flush` (§1392). Declaration order is `sync_reset` (§1126,
  declared FIRST) then `sgr_reset` (§1128, declared LAST), so at scope exit the byte stream
  is: `… \r\n` (§1391) → `flush` (§1392) → **`sgr_reset`** (§1128, LIFO-first) →
  **`sync_reset`** (§1126, LIFO-last). **Emit them in exactly this order** (`sgr_reset`
  before `sync_reset`, both after the `\r\n`+`flush`); a swapped order or placing them before
  the flush diverges from the byte stream. Both are `catch {}`/best-effort → `let _ = …`.
  The **leading** `sgr_reset` at §1127 (declared between the two defers) is a normal
  `try writeAll` and emits **before** the loop. Use the inner-closure pattern (as §6.6/§6.7)
  so the two trailing emissions run on all exits, in LIFO order, after the closure returns.
- **Loop index type (§1140):** Zig `prettyPrint` uses `var i: u16` (§1140), whereas `render`
  uses `var i: usize` (§405). Porting `i`/`cursor_pos`/`col`/`row` here — reproduce `u16`
  for strict fidelity (matching §1140 and the `w: u16` advance), **OR** carry a PORT-NOTE
  that `usize` is a benign divergence only observable for screens with >65535 cells (`i += w`
  overflow behavior); `u16` is the faithful choice and is recommended since the surrounding
  arithmetic (`w`, `col`, `row`, `cursor_pos.{row,col}`) is already `u16`.
- The loop (§1141–1390): same width/wrap/reposition/image/SGR/OSC-8 branches as render, but
  the "skip" test is replaced by **`if cell.default { reposition = true; continue }`**
  (§1168) — the ONLY structural difference besides no `screen_last`/scale/skip/explicit-width
  branches. The ADVANCE defer (§1150–1160, marks `screen_last.buf[j].skipped` — yes,
  pretty_print still touches `screen_last.skipped`) + the cursor/link update defer (§1172)
  translate exactly as §6.4 (cursor/link update after the `cell.default` continue check).
- Ends `write_all(b"\r\n")?; flush()?;`.

The SGR block (§1233–1386) is **identical** to render's (§536–687) — the porter should share
a helper `emit_style_diff<W>(w, cursor: &mut Style, link: &mut Hyperlink, cell, sgr,
enable_workarounds)` to avoid duplicating ~150 lines twice AND to keep the two byte-identical
(a fidelity win — one source for the SGR bytes). This is a **refactor of a shared inner
routine**, not a behavior change; the adversary must confirm render and pretty_print emit the
identical SGR bytes for identical input.

---

## 8. Capability queries — `queryTerminal`/`queryTerminalSend`/`enableDetectedFeatures`/`deviceStatusReport`

### 8.1 `query_terminal` (§250) + the futex

```
query_terminal_send(tty)?;
Futex.timedWait(&query_futex, 0, timeout_ns) catch {}
queries_done.store(true, .unordered→Relaxed);
enable_detected_features(tty)?;
```

**Repeated-call fidelity — reset `query_futex` to 0 before each wait (§13-risk-2 PORT-NOTE).**
Zig's `std.Thread.Futex.wake` **never mutates the word** — `wake_query_futex` only wakes
waiters; the word stays 0. So `timedWait(&query_futex, 0, …)` always waits fresh on
value==0, and a *second* `queryTerminal` call blocks again from scratch. Our poll-with-
deadline (§8.1 below) instead **stores 1** into `query_futex` (via `wake_query_futex`, §8.4)
and loops `while load()==0`; once that store-1 has happened, the word stays non-zero and
**every subsequent poll returns immediately forever after** — a repeated-call infidelity.
**Fix:** reset `query_futex` to 0 before each wait — either in `query_terminal_send`
(`self.query_futex.store(0, Relaxed);` alongside its `queries_done.store(false)`) or
immediately before the poll loop in `query_terminal`. This re-arms the wait so a second
`queryTerminal` blocks again, matching Zig's fresh-wait-on-0 semantics. **Not reached by
list_themes/boo** (each calls `queryTerminal` once), but a real fidelity fix; carry it as a
PORT-NOTE at the reset site.

Rust has no `std::thread::Futex`. **`std.Thread.Futex.timedWait(&word, expect, timeout_ns)`**
blocks until the word changes from `expect` (0) or the timeout elapses; the DA1 handler calls
`Futex.wake`. Port with **`std::sync::atomic` + a `Condvar`-free timed park** is heavy; the
faithful-and-simple image: since `query_futex` is an `AtomicU32`, use a **spin/park with a
deadline** — `let deadline = Instant::now() + Duration::from_nanos(timeout_ns); while
self.query_futex.load(Relaxed) == 0 && Instant::now() < deadline { std::thread::yield_now(); }`
— OR (preferred, no busy-spin) the `atomic-wait` shape via `std::thread::park_timeout` is not
keyed to the atomic. **Recommendation:** a bounded timed wait loop keyed on the atomic
(`query_futex`), documented as the faithful image of `Futex.timedWait` (Rust std has no
stable futex API). `wake_query_futex` (§8.4) stores a non-zero value into `query_futex` so
the loop observes the change. This is a divergence in *mechanism* (poll-with-deadline vs
kernel futex) but **identical in observable behavior** (block until DA1 or timeout). Note it
as a PORT-NOTE + a §13 risk (the on-device Loop test exercises it; the smoke test does not).

`Futex.timedWait` **error is swallowed** (`catch {}` — timeout is normal).

### 8.2 `query_terminal_send` (§261) + `enable_detected_features` (§312)

`query_terminal_send`: `queries_done.store(false, Relaxed);` then the big `writeAll(...)` of
`++`-concatenated consts → **sequential `w.write_all(ctlseqs::X)?`** in the exact order
(§279–304): `decrqm_sgr_pixels, decrqm_unicode, decrqm_color_scheme, in_band_resize_set,
home, explicit_width_query, cursor_position_request, home, scaled_text_query,
multi_cursor_query, cursor_position_request, xtversion, csi_u_query, kitty_graphics_query,
primary_device_attrs`; then `flush()?`. The commented-out `COLORTERM`/`XTGETTCAP`/`decrqm_*`
TODOs (§264–278) are **preserved as comments** (R1).

`enable_detected_features`: `builtin.os.tag` switch → `#[cfg(windows)] { self.sgr = Legacy; }`
`#[cfg(not(windows))] { … }`. The else-arm env reads: `std.posix.getenv(NAME)` →
`std::env::var_os(NAME).is_some()` for the presence checks (`TERMUX_VERSION`, `VHS_RECORD`,
`VAXIS_FORCE_LEGACY_SGR`, `VAXIS_FORCE_WCWIDTH`, `VAXIS_FORCE_UNICODE`) and
`std::env::var("TERM_PROGRAM")` `== "vscode"` for the value compare. Effects preserved
(`sgr = Legacy`; `caps.unicode = Wcwidth`/`Unicode`; `caps.kitty_keyboard = false`). Then:
`if caps.kitty_keyboard { enable_kitty_keyboard(tty, opts.kitty_keyboard_flags)? }`; `if
caps.unicode == Unicode && !caps.explicit_width { write_all(unicode_set)? }`; `flush()?`.
`VHS_RECORD` sets three fields (§322–326) — preserve all three.

### 8.3 `enable_kitty_keyboard` (§783)

`const flag_int: u5 = @bitCast(flags);` → add `KittyFlags::bits() -> u8` (LSB-first pack:
`disamb0 events1 alt_keys2 all_ctl3 text4`, per key.rs’s documented layout; the existing
`Modifiers::to_u8` is the precedent). Emit `csi_u_push` (`"\x1b[>{d}u"` — `write!(w,
"\x1b[>{}u", flag_int)?`), `flush()?`, `state.kitty_keyboard = true`.

### 8.4 `device_status_report` (§1112) + `wake_query_futex`

`device_status_report(&self, tty)`: `write_all(device_status_report)?; flush()?;` — fills the
placeholder leaf; the loop_ `stop` call site (`(*vx).device_status_report((*tty).writer())`)
compiles (infers `W=TtyWriter`). `wake_query_futex(&self)`: the inlined
`std.Thread.Futex.wake(&vx.query_futex, 10)` — store a non-zero value into `query_futex`
(`self.query_futex.store(1, Relaxed)`) to release the §8.1 wait; documented as the faithful
image of `Futex.wake` (no std futex).

---

## 9. Lifecycle — `init`/`deinit`/`reset_state`/`resize`/alt-screen/`window`/`queue_refresh`

- **`init`** (§106): `alloc` dropped. `Vaxis { opts, screen: Screen::default(), screen_last:
  InternalScreen::init(0, 0), ..Vaxis::default() }` — `queries_done = true`, `next_img_id =
  1`, `sgr = Standard`, `enable_workarounds = true`, `refresh = false`, all `state`/`caps`
  false. (`init(alloc, 0, 0)` → `InternalScreen::init(0,0)`.) Returns `Vaxis<'a>`.
- **`deinit`** (§118): `deinit<W>(&mut self, tty: &mut W)`: `let _ = self.reset_state(tty);`
  (Zig `catch {}`); `alloc?` → `Vec` `Drop` (the explicit `screen.deinit`/`screen_last.deinit`
  become no-op shims / `Drop`). Keep the optional-alloc semantics as a doc note (Rust always
  frees on drop).
- **`reset_state`** (§128): ported line-for-line. `show_cursor`, `sgr_reset` (both
  `catch {}` → `let _ =`); `if screen.cursor_shape != Default { print(cursor_shape,
  @intFromEnum(CursorShape::Default)=0) catch {} }`; kitty_keyboard pop; mouse off;
  bracketed_paste off; alt-screen teardown (`home`+`erase_below_cursor`+`exit_alt_screen`)
  else (`\r` + `ri`×cursor.row + `erase_below_cursor`); color_scheme/in_band_resize/
  changed_default_fg/bg/cursor_color resets (each guarded by its `state.*` flag, then clears
  it); `flush()?`. **Teardown order preserved exactly** (§129–180).
- **`resize`** (§187): `log.debug` dropped; `alloc` dropped. `screen.deinit()`; `screen =
  Screen::init(winsize)`; `screen.width_method = caps.unicode`; `screen_last.deinit()`;
  `screen_last = InternalScreen::init(winsize.cols, winsize.rows)`; alt→`home` else
  (`ri`×cursor.row + `\r`); `state.cursor.{row,col} = 0`; `write_all(sgr_reset)?;
  write_all(erase_below_cursor)?` (the Zig `sgr_reset ++ erase_below_cursor` → two writes);
  `flush()?`.
- **`window`** (§217): `window(&mut self) -> Window<'a>` — `Window { x_off:0, y_off:0,
  parent_x_off:0, parent_y_off:0, width: self.screen.width, height: self.screen.height,
  screen: &mut self.screen as *mut Screen<'a> }` (Window holds `*mut Screen<'a>`, §Chunk-2).
- **`enter_alt_screen`** (§231): `smcup`+`flush`+`state.alt_screen=true`.
  **`exit_alt_screen`** (§238): `rmcup`+`flush`+`state.alt_screen=false`.
- **`queue_refresh`** (§353): `self.refresh = true`.

---

## 10. The setters/queries (all portable, list_themes-critical marked ★)

- **`set_mouse_shape`** ★ (§820): `self.screen.mouse_shape = shape;` (no tty).
- **`set_mouse_mode`** ★ (§825): `if enable { state.mouse=true; if caps.sgr_pixels {
  state.pixel_mouse=true; write_all(mouse_set_pixels) } else { write_all(mouse_set) } } else
  { write_all(mouse_reset) }`; `flush`. `log.debug` dropped.
- **`set_title`** ★ (§801): emit `osc2_set_title` (`{s}` split), `flush`.
- **`copy_to_system_clipboard`** ★ (§1043): `encode_allocator` dropped; base64-encode
  `text` (§5.4) into a local `Vec`; emit `osc52_clipboard_copy(b64)` (`{s}` split); `flush`.
- **`subscribe_to_color_scheme_updates`** ★ (§1105): `write_all(color_scheme_request)?;
  write_all(color_scheme_set)?; flush?; state.color_scheme_updates = true`.
- **`translate_mouse`** (§844) — **fills the placeholder leaf**: `translate_mouse(&self, mouse)
  -> Mouse` (Zig takes `self` by value; `&self` is faithful — it only reads). `if
  screen.width==0 || screen.height==0 { return mouse }`; `if state.pixel_mouse { assert!(
  mouse.xoffset==0); assert!(mouse.yoffset==0); … the xcell/ycell/xextra/yextra math …; if
  xcell==0||ycell==0 { return mouse }; result.{col,row,xoffset,yoffset} = … }`. Ported
  arithmetic-for-arithmetic (integer `/` and `%` on `u16`).
- **`set_bracketed_paste`** (§809): `bp_set`/`bp_reset`; `flush`; `state.bracketed_paste=enable`.
- **`notify`** (§791): `if let Some(t)=title { osc777_notify(t, body) } else {
  osc9_notify(body) }`; `flush`.
- **`request_system_clipboard`** (§1057): `if opts.system_clipboard_allocator == null →
  error.NoClipboardAllocator`. **Options dropped the allocator** → this guard is vestigial;
  port as `request_system_clipboard<W>(&self, tty)` that always emits (or returns an error if
  a `bool has_clipboard` opt is retained). **Decision:** use the `system_clipboard: bool`
  field now defined on `Options` (§1/§2, default false) to preserve the guard faithfully →
  `if !opts.system_clipboard {
  return Err(NoClipboardAllocator) }` then emit `osc52_clipboard_request`; `flush`. (Not used
  by list_themes; the guard is preserved for fidelity.)
- **`set_terminal_foreground/background/cursor_color`** (§1067/1074/1081): `osc10/11/12_set`
  with the `{x:0>2}` sextet `(rgb[0],rgb[0],rgb[1],rgb[1],rgb[2],rgb[2])` → `write!(w,
  "…{:02x}{:02x}/…", …)`; `flush`; set the `state.changed_*` flag.
- **`query_color`** (§1090): `match kind { Fg→osc10_query; Bg→osc11_query; Cursor→osc12_query;
  Index(idx)→osc4_query(idx) }`; `flush`.
- **`free_image`** (§1035) — **portable**: `tty.print("\x1b_Ga=d,d=I,i={d};\x1b\\", id)
  catch |err| log.err → return; flush catch {}` → `let _ = write!(w, "\x1b_Ga=d,d=I,i={};\x1b\\",
  id); let _ = w.flush();` (errors swallowed; `log.err` dropped).
- **`set_terminal_working_directory`** (§1396): `if path.is_empty() || path[0] != b'/' {
  return Err(InvalidAbsolutePath) }`; hostname via `std::env::var_os("HOSTNAME")` (windows→
  `null`) or `"localhost"`; **`std.Uri` (§10.4)**.

### 10.4 `std.Uri` (set_terminal_working_directory)

Zig builds `std.Uri{ scheme:"file", host:.{.raw=hostname}, path:.{.raw=path} }` and formats
with `{ .scheme=true, .authority=true, .path=true }` into `osc7` (`"\x1b]7;{f}\x1b\\"`). The
`{f}` is the Uri formatter. **This is used by neither list_themes nor boo.** Port faithfully
as a small inline URI emit — `file://<hostname><path>` (raw components, no percent-encoding,
matching `.raw` which bypasses encoding) — OR mark the URI-formatting a **narrow R5 leaf**
if the exact `std.Uri.fmt` percent-encoding of `.raw` is uncertain. **Recommendation:** port
inline (`file://` + hostname + path via `write_all` of the raw bytes) with a PORT-NOTE that
`.raw` components are emitted verbatim (Zig `.raw` = no re-encoding); this is faithful and
avoids an R5 seam for a small function. The adversary should confirm `.raw` emits verbatim.

---

## 11. The `zigimg`/Image seams — exactly TWO R5 functions

**Portable, ported LIVE (no zigimg):**
- `render`'s kitty-graphics emission block (§499–530) — operates on `Placement` (a plain
  value); ported live (§6.2). **No non-image render logic is dropped.**
- `transmit_local_image_path` (§866): base64-encodes a filesystem `payload` (§5.4) and emits
  `\x1b_Gf=24/32,s=,v=,i=,t={c};{s}\x1b\\` (rgb/rgba) or `\x1b_Gf=100,i=,t={c};{s}\x1b\\`
  (png). No zigimg. `alloc` → local `Vec`. Returns `Image { id, width, height }`. Error
  `PathTooLong` (`size >= 4096`) / `NoGraphicsCapability` → a small `ImageError` enum or
  `io::Error`.
- `transmit_pre_encoded_image` (§925): emits pre-b64 bytes, chunking at 4096 (`m=1`/`m=0`).
  No zigimg. Ported live.
- `free_image` (§1035): §10.

**R5 seams (`zigimg.Image` — the DEFERRED dep):**
- `transmit_image` (§983): takes `img: *zigimg.Image`, does `img.writeToMemory(.png)` /
  `img.convert(.rgb24/.rgba32)` + `img.rawBytes()` + arena + base64, then
  `transmit_pre_encoded_image`. → **`unimplemented!("transmit_image — zigimg R5 seam")`**
  with the **verbatim Zig body preserved as a leading block comment** (placeholder-leaf-
  doctrine). Signature retained (takes a `&mut ImagePlaceholder` or is `#[cfg(feature=
  "zigimg")]`-gated — recommend a plain `unimplemented!()` + comment, no feature flag, matching
  `image.rs::draw`/`cell_size`).
- `load_image` (§1017): `zigimg.Image.fromFilePath`/`fromMemory` + `transmit_image`. →
  **R5 seam**, verbatim body as comment.

`list_themes`/`boo` use **no images**, so the two seams are never reached by the deliverables.
`Image.TransmitMedium`/`TransmitFormat`/`Image`/`Placement`/`DrawOptions` all already exist
(image.rs, Chunk 1a). `TRANSMIT_OPENER` const (image.rs) is unused by these paths (Zig inlines
the templates in `transmit*`).

---

## 12. The ADDED render-to-`Vec<u8>` smoke test (Vaxis.zig has 0 `test{}`)

R1 fidelity here is by faithful translation + the adversary's re-check + **this smoke test**
(the render engine's only executable pin). Because `render<W: Write>` is generic, a `Vec<u8>`
is a valid sink — the test asserts the **exact emitted escape bytes** for a known screen.

**Test `render_emits_minimal_escapes` (primary screen, one cell changed):**

```rust
#[test]
fn render_emits_minimal_escapes() {
    // 2x1 primary-screen Vaxis: build screen + screen_last same size (render asserts equal len)
    let mut vx = Vaxis::init();               // 0x0
    // hand-size to 2x1 without a tty (resize writes to a tty; instead set fields directly):
    vx.screen = Screen::init(Winsize{rows:1, cols:2, x_pixel:0, y_pixel:0});
    vx.screen_last = InternalScreen::init(2, 1);
    // write a known cell 'A' (default=false) at (0,0); (1,0) stays default space
    vx.screen.write_cell(0, 0, Cell{ char: Character{ grapheme: b"A", width: 1 },
                                     default: false, ..Default::default() });
    let mut out: Vec<u8> = Vec::new();
    vx.render(&mut out).unwrap();

    // Prologue (primary screen, cursor.row=0): sync_set, hide_cursor, "\r", sgr_reset
    assert!(out.starts_with(b"\x1b[?2026h\x1b[?25l\r\x1b[m"));
    // the changed cell emits 'A' (grapheme, write_all — raw byte)
    assert!(find(&out, b"A"));
    // epilogue: sync_reset + (flush no-op on Vec)
    assert!(out.ends_with(b"\x1b[?2026l"));
    // 2nd render with NO change emits NO 'A' (diff suppresses it) — proves the diff works
    let mut out2 = Vec::new();
    vx.render(&mut out2).unwrap();
    assert!(!find(&out2, b"A"));
}
```

This is **meaningful** (asserts real emitted escapes, exercises the diff's suppression on the
second render — not a trivial "runs without panic"), and **Miri-clean** (pure over `Vec<u8>`,
no FFI). It pins: the prologue order, the primary-screen `\r` path, the changed-cell glyph
emit, the `sync_set`/`sync_reset` framing, AND the core diff invariant (unchanged cell →
nothing emitted). Optional second test `render_emits_sgr_on_color_change` sets a cell fg to
`Index(1)` and asserts `\x1b[31m` (`fg_base`) appears — pinning the SGR-minimization path.

**Counts toward `check_test_counts`** (Vaxis.zig has 0 `test{}`, so the added tests are
"extra" Rust tests, allowed by the crate's non-parity-contract status — Cargo.toml notes
libvaxis `test{}` are NOT part of the src/ parity test contract; the added smoke tests are
the fidelity substitute the workflow mandates).

---

## 13. Gates / R4 / R6 / risks

- **R4 acyclic:** vaxis.rs consumes only intra-crate modules already depended on by loop_/
  window/screen; no new crate edge (only `libc`, already present). `check_acyclic.sh` green.
- **R6 (Miri):** `render`/`pretty_print`/the setters over a `Vec<u8>` sink are pure → run
  under Miri. The tty-native path (`TtyWriter::flush` → `libc::write`) is NOT Miri (native/
  ASan), same as the existing tty.rs exception (MIRI_EXCEPTIONS.md). The §12 smoke test is
  Miri-target.
- **`check_comment_coverage`:** every Zig comment ported verbatim (the TODOs at §264–278, the
  `HACK`/explicit-width/scaled-text explanatory comments, the OSC-8/skip comments). The two
  R5 image seams carry the verbatim Zig body as a block comment.
- **`check_test_counts`:** the added render smoke tests (see §12).
- **clippy `-D` / fmt:** the generic `<W: Write>` bounds, the labeled-block `'body`, the
  inner-closure `errdefer`/`refresh` guards are clippy-clean; run `cargo fmt`.

**Risks (for the adversary):**
1. **The `defer` translation (§6.4)** — the single highest-risk point; ADVANCE must run on
   every exit, cursor/link update on non-skip exits only. A mis-placement silently corrupts
   the diff (no test in Zig). Mitigated by the §12 second-render suppression assertion + the
   adversary re-check.
2. **The futex (§8.1)** — poll-with-deadline vs kernel futex: observably equivalent (block
   until DA1 or timeout) but a mechanism divergence; only the on-device Loop test exercises it.
   **Plus the repeated-call trap:** because our `wake_query_futex` *stores 1* (Zig's
   `Futex.wake` never mutates the word), the word must be **reset to 0 before each wait**
   (§8.1 PORT-NOTE) or a second `queryTerminal` returns instantly instead of blocking. Not
   hit by list_themes/boo (single query) but fixed for fidelity.
3. **SGR minimization sharing (§7)** — render and pretty_print MUST emit identical SGR bytes;
   the shared `emit_style_diff` helper is a refactor the adversary must confirm is byte-exact
   vs both §536–687 and §1233–1386.
4. **`{s}` byte-slice emission (§5.1)** — must be `write_all`, never `{}` (lossy). Grep the
   port for any `write!("{}", grapheme/uri/params/title/b64)` — all must be `write_all`.
5. **`Vaxis<'a>` threading into loop_ (§3)** — mechanical, but the porter must not
   accidentally alter a `handle_event_generic` body line; `cargo build --workspace` is the
   backstop.

**DISPOSITION: ready.**
