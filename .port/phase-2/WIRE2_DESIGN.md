# WIRE2 — un-defer the `list_themes` interactive fuzzy-finder preview picker

TUI-CLI wiring 2 (the finale). Fill the R5 `unimplemented!()` `preview` seam +
the preserved-verbatim `Preview`/`Event`/`color`/`lorem_ipsum`/`toUri`/
`resolveAutoThemePath`/`writeAutoThemeFile` comment block at the tail of
`crates/rustty-cli/src/list_themes.rs`, from those preserved bodies, using the
now-landed **`rustty-vaxis`** (Vaxis / Window / Tty / **Loop** / Style / Color /
Key / Mouse / `widgets::TextInput`) and **`rustty-zf`** (`zf::rank`).

- Reference (read-only oracle): `ghostty/src/cli/list_themes.zig` (Event union +
  `Preview` struct + `init`/`deinit`/`run`/`update`/`updateFiltered`/`up`/`down`/
  `draw`/`drawPreview`/`saveSelectedTheme` + the `ui_*` color helpers + `color` +
  `lorem_ipsum` + `preview` + `resolveAutoThemePath`/`writeAutoThemeFile` +
  `ThemeListElement.toUri`).
- The `--plain` / non-tty enumeration path (`run`) already works and **stays
  untouched**.
- DISPOSITION: **ready** — every vaxis/zf call the reference makes has a landed
  Rust analogue (mapped 1:1 in §1). The Loop-holds-raw-self aliasing is the SAME
  *queue-synchronized* discipline C3b established, but the tty/vx now live as
  **fields of a boxed `Preview`** (not separate stack `var`s), so a whole-`&mut
  Preview` reborrow would assert uniqueness over the very bytes the read thread's
  raw ptrs alias. The design therefore (a) NEVER forms a whole-`&mut Preview`
  while the read thread is live — `update`/`draw`/the run-loop body operate over
  **disjoint field borrows** via free fns (§3/§4/§5), and (b) adds a **fresh
  mirihar interleave test** for this struct-field shape rather than asserting C3b
  covers it (§3, §9).

---

## 0. Dependency + asset + gate deltas

- **Cargo** (`crates/rustty-cli/Cargo.toml` `[dependencies]`): add exactly
  `rustty-zf = { workspace = true }` (task-mandated). `rustty-vaxis` was already
  added in WIRE1. Both are low pure-leaf vendored-dep-port crates with **no** edge
  back to cli/config/apprt → `cli → {vaxis, zf}` are downward edges, **acyclic**
  (`check_acyclic.sh`, R4).
- **Asset (R3)**: copy `ghostty/src/cli/lorem_ipsum.txt` (3035 bytes) into
  `crates/rustty-cli/src/lorem_ipsum.txt` and bind it with
  `const LOREM_IPSUM: &[u8] = include_bytes!("lorem_ipsum.txt");` (the reference's
  `@embedFile` → `include_bytes!`, kept as `&[u8]` — never a lossy `String`, since
  `drawPreview` `splitAny(u8, lorem_ipsum, " \n")` matches over raw bytes).
- **No new `Error` variants / `From` impls needed.** `ghostty.rs::Error` already
  has `From<io::Error>` (WIRE1), `From<TtyError>` (WIRE1), and
  `From<config_struct::LoadError>`. The three fallible surfaces `preview` reaches
  — `Tty::init`/`get_winsize` (`TtyError`), `Vaxis::*` writer calls (`io::Result`),
  `Config::load_file` (`LoadError`) — all already convert.
- Gates to re-run green: build / test / clippy `-D` / fmt / `check_acyclic.sh` /
  `check_test_counts.py` (0 → the small new count, §9) / `check_comment_coverage.py`
  (scoped-G3b — the tail verbatim block is REPLACED by the live port whose own
  line/comment fidelity must hold) / **Miri** (the factored `update_filtered`
  ranking kernel + the `draw` render-to-`Vec<u8>` smoke are pure → Miri-able; the
  interactive `run`/Loop path is `#[ignore]`-on-device, not under Miri) / **a NEW
  `mirihar` interleave harness** (`.port/phase-2/mirihar-wire2`) that reproduces
  the struct-field aliasing shape (§3) — a boxed 2-field stand-in for
  `Preview{tty,vx}`, a `Loop::new(&mut .tty, &mut .vx)`-style pair of raw ptrs, a
  spawned reader that touches them, and a main loop that drives the disjoint-field
  free fns — run under `cargo +nightly miri test` to PROVE no `&mut *self` reborrow
  invalidates the live raw ptrs (Stacked/Tree Borrows). This is NOT the C3b
  harness; it is authored fresh for the field-layout shape the cli introduces.

---

## 1. Landed `rustty-vaxis` + `rustty-zf` surface (verified) — reference → Rust map

| Reference | Rust analogue | Notes |
|---|---|---|
| `vaxis.Tty.init(&buf)` | `rustty_vaxis::Tty::init(vec![0u8; 4096])?` → `Result<_, TtyError>` | `Tty = PosixTty` in a normal (non-test) cli build. Reference `buf:[4096]u8`. |
| `tty.deinit()` | `tty.deinit()` (consumes `self`) | |
| `tty.writer()` | `tty.writer() -> &mut TtyWriter` (`: io::Write`) | `&mut self`. |
| `vaxis.Tty.getWinsize(tty.fd)` | `rustty_vaxis::Tty::get_winsize(tty.fd())?` | (`run` does NOT call this — the Loop posts a `winsize` event; see §4.) |
| `vaxis.init(alloc, .{})` | `rustty_vaxis::Vaxis::init(vaxis::vaxis::Options::default())` | `alloc` dropped; `Options: Default`. |
| `vaxis.Loop(Event){ .tty=&self.tty, .vaxis=&self.vx }` | `rustty_vaxis::Loop::<'_, Event>::new(tty_ptr, vx_ptr)` | `new(tty: *mut Tty, vaxis: *mut Vaxis)` — raw ptrs into the pinned `Preview` (§3). |
| `loop.init()` / `loop.start()` | `loop_.init()?` / `loop_.start()?` | spawns the read thread. |
| `loop.pollEvent()` | `loop_.poll_event()` | blocking wait for ≥1 event. |
| `loop.tryEvent()` | `loop_.try_event() -> Option<Event>` | drain. |
| `vx.enterAltScreen(w)` | `vx.enter_alt_screen(w)?` | |
| `vx.setTitle(w, "…")` | `vx.set_title(w, b"\xf0\x9f\x91\xbb Ghostty Theme Preview \xf0\x9f\x91\xbb")?` | title is `&[u8]`; keep the two 👻 as UTF-8 bytes. |
| `vx.queryTerminal(w, 1*ns_per_s)` | `vx.query_terminal(w, 1_000_000_000)?` | `std.time.ns_per_s` = 1e9. |
| `vx.setMouseMode(w, true)` | `vx.set_mouse_mode(w, true)?` | |
| `vx.caps.color_scheme_updates` | `vx.caps.color_scheme_updates: bool` | |
| `vx.subscribeToColorSchemeUpdates(w)` | `vx.subscribe_to_color_scheme_updates(w)?` | |
| `vx.render(w)` | `vx.render(w)?` | |
| `vx.resize(alloc, w, ws)` | `vx.resize(w, ws)?` | `alloc` dropped. |
| `vx.window()` | `vx.window() -> Window<'a>` | `Copy`, holds `*mut Screen` (§5 alias pattern — not a live `&mut vx`). |
| `vx.setMouseShape(.default)` | `vx.set_mouse_shape(mouse::Shape::Default)` | `&mut self`. |
| `vx.copyToSystemClipboard(w, txt, alloc)` | `vx.copy_to_system_clipboard(w, txt)?` | **Divergence**: Rust sig is `(&self, tty, text: &[u8])` — NO `alloc` param (base64 into an owned `Vec`). Record inline. |
| `vx.deinit(alloc, w)` | `vx.deinit(w)` | swallows reset errs. |
| `win.child(.{x_off,y_off,width,height,border})` | `win.child(ChildOptions{ x_off, y_off, width: Some(w), height: Some(h), border })` | `x_off/y_off: i32`; `width/height: Option<u16>`. |
| `.border=.{.where=.all,.style=s}` | `BorderOptions{ r#where: Where::All, style: s, glyphs: Glyphs::SingleRounded }` | default glyphs = rounded (matches reference default). |
| `win.width` / `win.height` | `win.width: u16` / `win.height: u16` | |
| `win.x_off` | `win.x_off: i32` | (used as `theme_list.x_off + theme_list.width`). |
| `win.clear()` / `win.fill(cell)` | `win.clear()` / `win.fill(Cell{ style, ..Default::default() })` | `.{ .style = s }` → `Cell{ style: s, ..Default::default() }`. |
| `win.hideCursor()` | `win.hide_cursor()` | |
| `win.hasMouse(mouse)` | `Window::has_mouse(win, Some(mouse)) -> Option<Mouse>` | associated fn; pass `Some(mouse)`. |
| `win.printSegment(seg, opts)` | `win.print_segment(seg, opts) -> PrintResult` | |
| `win.print(&[seg,..], opts)` | `win.print(&[Segment], opts) -> PrintResult` | |
| `.{ .row_offset=r, .col_offset=c }` | `PrintOptions{ row_offset: r, col_offset: c, ..Default::default() }` | defaults `wrap: Grapheme, commit: true`. |
| `.{ …, .wrap = .none }` | `PrintOptions{ …, wrap: Wrap::None, ..Default::default() }` | the theme-path row. |
| `vaxis.Style{ .fg,.bg,.bold,.italic,.ul_style }` | `cell::Style{ fg, bg, bold, italic, ul_style, ..Default::default() }` | |
| `.ul_style = .single/.double/.dashed/.curly/.dotted` | `Underline::Single/Double/Dashed/Curly/Dotted` | |
| `.{ .rgb = .{r,g,b} }` | `Color::Rgb([r,g,b])` | |
| `Segment .{ .text, .style, .link }` | `Segment{ text: &[u8], style, link }` | |
| `.link = .{ .uri = try theme.toUri(alloc) }` | `Hyperlink{ uri: &uri_bytes, params: b"" }` | `uri_bytes: Vec<u8>` bound in-scope so the `&[u8]` borrow outlives the `print_segment` (mirrors the `allocPrint` lifetime). |
| `text_input.buf.realLength()` | `self.text_input.buf.real_length()` | `buf: Buffer` is `pub`. |
| `text_input.buf.firstHalf()/secondHalf()` | `.first_half()` / `.second_half()` (`&[u8]`) | |
| `text_input.buf.clearRetainingCapacity()` | `self.text_input.buf.clear_retaining_capacity()` | (normal-mode `^X`/`^/`). |
| `text_input.clearRetainingCapacity()` | `self.text_input.clear_retaining_capacity()` | (search-mode `^X`/`^/`). |
| `text_input.update(.{ .key_press = key })` | `self.text_input.update(text_input::Event::KeyPress(key))` | widget-local `Event`; `key` is a borrowed `Key<'_>` (§5b). |
| `text_input.drawWithStyle(child, s)` | `self.text_input.draw_with_style(child, s)` | `&mut self.text_input`, `win: Window`. |
| `key.matches(cp, .{ .ctrl=true })` | `key.matches(cp, Modifiers{ ctrl: true, ..Default::default() })` | `cp: u32`. |
| `key.matchesAny(&.{a,b,..}, .{})` | `key.matches_any(&[a, b, ..], Modifiers::default())` | |
| `vaxis.Key.escape/f1/enter/kp_enter/home/kp_home/end/kp_end/down/kp_down/kp_add/up/kp_up/kp_subtract/page_down/page_up/kp_page_up` | `rustty_vaxis::key::{escape,f1,enter,kp_enter,home,kp_home,end,kp_end,down,kp_down,kp_add,up,kp_up,kp_subtract,page_down,page_up,kp_page_up}` (`u32`) | all present. |
| `mouse.button == .wheel_up/.wheel_down/.left` | `mouse.button == Button::WheelUp/WheelDown/Left` | |
| `mouse.type == .release` | `mouse.r#type == Type::Release` | |
| `mouse.row` | `mouse.row: u16` | |
| `zf.rank(theme.theme, tokens.items, .{ .to_lower=true, .plain=true })` | `zf::rank(&theme.theme, &tokens, RankOptions{ to_lower: true, plain: true }) -> Option<f64>` | `tokens: Vec<&[u8]>`; byte-faithful. |
| `Config.default(alloc)` | `Config::default_config()` | |
| `config.loadFile(arena, path)` | `config.load_file(&theme.path)?` (or `match`) | `path: &[u8]`; err → `LoadError`. |
| `config.background/foreground` | `config.background: value_types::Color{ r,g,b }` | direct `.r/.g/.b`. |
| `config.@"cursor-text"/@"cursor-color"/@"selection-*"` | `config.cursor_text/cursor_color/selection_foreground/selection_background: Option<TerminalColor>` | see fidelity note ↓. |
| `config.palette.value[i].r` | `config.palette.value[i].r` (`value: [RGB;256]`, direct index) | |
| `config._diagnostics.items()` | `config._diagnostics.items() -> &[Diagnostic]` | |

**`cursor-text`/`cursor-color`/`selection-*` fidelity note** (fill-time, not a
blocker): the reference reads `cursor_text.color.r`. The Rust port's
`TerminalColor` is `enum { Color(Color), CellForeground, CellBackground }`. Map
`TerminalColor::Color(c) => Color::Rgb([c.r, c.g, c.b])`; the `cell-foreground`/
`cell-background` arms have no rgb — resolve them faithfully to the `else` fallback
the reference uses for a *missing* value (`cursor_fg → bg`, `cursor_bg → fg`,
`selection_fg → bg`, `selection_bg → fg`), i.e. treat "cell-*" as "no explicit
rgb". Record this as a PORT NOTE at each of the four `if let Some(..)` sites.

---

## 2. `Event` + `LoopEvent`

The reference `Event = union(enum){ key_press: vaxis.Key, mouse: vaxis.Mouse,
color_scheme: vaxis.Color.Scheme, winsize: vaxis.Winsize }`. The vaxis `Loop<T>`
requires `T: LoopEvent` and **queues owned** key text across the thread boundary
(the `LoopEvent::key_press` constructor takes a `KeyOwned`, §6 of loop_.rs). So:

```rust
enum Event {
    KeyPress(key::KeyOwned),   // key_press: vaxis.Key  (owned across the queue)
    Mouse(mouse::Mouse),       // mouse
    ColorScheme(cell::Scheme), // color_scheme: vaxis.Color.Scheme
    Winsize(Winsize),          // winsize
}

impl LoopEvent for Event {
    fn key_press(k: key::KeyOwned) -> Option<Self> { Some(Event::KeyPress(k)) }
    fn mouse(m: mouse::Mouse)      -> Option<Self> { Some(Event::Mouse(m)) }
    fn color_scheme(s: cell::Scheme) -> Option<Self> { Some(Event::ColorScheme(s)) }
    fn winsize(ws: Winsize)        -> Option<Self> { Some(Event::Winsize(ws)) }
    // all other constructors default to None ≡ Zig's `@hasField` false (the four
    // absent variants — focus/paste/color_report/… — are never posted for this T)
}
```

PORT NOTE to record: Zig's `union(enum)` field-presence gate (`@hasField`) is
modeled by which `LoopEvent` constructors return `Some`; the four variants above
mirror the reference `Event` exactly. `KeyPress` carries `KeyOwned` (not a
borrowed `Key<'a>`) because the queued `T` must be `Send + 'static`.

---

## 3. `Preview` struct + heap-pinning + the Loop-holds-raw-self aliasing (C3b reuse)

```rust
#[derive(Clone, Copy, PartialEq)]
enum Mode { Normal, Help, Search, Save }

// The non-tty/non-vx field subset — the ONLY state `update`/`draw` touch. Split
// out so the run loop can form a `&mut DrawState` borrow DISJOINT from `&mut vx`
// / `&mut tty` and thereby never reborrow a whole `&mut Preview` while the read
// thread's raw ptrs into `tty`/`vx` are live (the §3 aliasing discipline).
struct DrawState<'t> {
    should_quit: bool,
    mouse: Option<mouse::Mouse>,
    themes: &'t mut [ThemeListElement],                // []ThemeListElement (rank is written back)
    filtered: Vec<usize>,                              // std.ArrayList(usize)
    current: usize,
    window: usize,
    hex: bool,
    mode: Mode,
    color_scheme: cell::Scheme,                        // = .light
    text_input: widgets::text_input::TextInput,
    theme_filter: ColorScheme,
}

struct Preview<'t> {
    // allocator: dropped (Rust owns each Vec)          // allocator: std.mem.Allocator
    tty: Tty,                                           // vaxis.Tty
    vx: Vaxis<'t>,                                      // vaxis.Vaxis
    draw_state: DrawState<'t>,                          // all remaining reference fields
}
```

The reference `Preview` is one flat struct; the Rust port groups its fields into
`{ tty, vx, draw_state }` **purely** to make the disjoint borrows expressible — no
field is added or dropped, and `should_quit`/`mode`/etc. map 1:1. Field accesses
in the ported bodies read `self.draw_state.<f>` where the reference reads
`self.<f>` (a mechanical rename, recorded once as a PORT NOTE).

`themes` is `&'t mut [ThemeListElement]` because `updateFiltered`/`drawPreview`
write `theme.rank` (`for (self.themes) |*theme|`). The reference `preview` entry
takes `themes: []ThemeListElement` (a mutable view); `run` must therefore pass
`&mut themes[..]` (drop the current `preview(themes: &[...])` immutable sig — a
signature-only widening, no behavior change; the `--plain` path keeps its own
immutable `&themes` iteration).

**Heap-pinning + aliasing (the corrected discipline).** The reference
`Preview.init` does `allocator.create(Preview)` (heap) and `run` builds
`vaxis.Loop(Event){ .tty = &self.tty, .vaxis = &self.vx }` — the Loop holds raw
pointers into the pinned `Preview`'s `tty`/`vx` fields while the main thread
drives the event loop. Zig has no borrow stack, so the reference's `self.update`/
`self.draw`/`self.vx.render` (`*Preview` receiver) are fine there. In Rust they
are **not** automatically fine: reborrowing `&mut *self` (a whole `&mut Preview`)
asserts uniqueness over ALL of `self`'s bytes, *including* the `tty`/`vx` bytes the
read thread's raw ptrs alias, and under Stacked/Tree Borrows that reborrow pops
those child raw ptrs off the borrow stack → later `(*tty)`/`(*vaxis)` derefs on the
read thread are UB. **`Box` pinning buys address stability, NOT borrow-stack
validity** — the two are distinct and must not be conflated. C3b's precedent does
**not** transfer verbatim: there, `tty` and `vx` were *separate stack `var`s*, so
no single borrow ever spanned both, and no whole-object reborrow existed to
invalidate them. Here they are fields of one boxed `Preview`, so the fill MUST
avoid ever forming a whole-`&mut Preview` while the read thread is live. Port
faithfully with the disjoint-split discipline:

- `Preview::init(...) -> Box<Preview>` — `Box` is the `allocator.create` heap pin;
  it keeps `tty`/`vx` at stable addresses for the Loop's raw ptrs (necessary but
  not sufficient — see below).
- `run(self: &mut Preview)` builds the Loop with
  `Loop::<Event>::new(&mut self.tty as *mut Tty, &mut self.vx as *mut Vaxis)`,
  `init`/`start`s it, then runs the event loop **without ever reborrowing `&mut
  *self`**. Every per-iteration call is expressed as a call on a **disjoint field
  borrow**, mirroring the split §7 already adopts for `draw`:
    - `update` becomes a free fn `update(vx: &mut Vaxis, tty: &mut Tty, st: &mut
      DrawState, event) -> Result<(), Error>` (it needs `vx`+`tty` only for the
      `.winsize => vx.resize(tty.writer(), ws)` and the clipboard arms; all other
      arms touch only `DrawState`). The run loop calls it as `update(&mut
      self.vx, &mut self.tty, &mut self.draw_state, event)?` — three disjoint
      field borrows, **never** a `&mut self`.
    - `draw` becomes the free fn of §7 `draw(vx: &mut Vaxis, st: &mut DrawState)`,
      called `draw(&mut self.vx, &mut self.draw_state)?` — again disjoint from
      `self.tty` (draw never names tty) and from the `loop_` raw ptrs.
    - the writer calls (`self.vx.render(self.tty.writer())`, `enter_alt_screen`,
      …) already borrow only the disjoint `vx`+`tty` fields (§4), never whole
      `self`.
  The read thread projects `(*tty).read(&self)` (shared read of the fd) and the
  narrow `(*vaxis).caps` writes — the vaxis Loop's own established cross-thread
  contract (unchanged from C3b). Because the main thread forms only per-field
  `&mut self.vx`/`&mut self.tty`/`&mut self.draw_state` borrows — and crucially
  `&mut self.tty`/`&mut self.vx` are the SAME field paths the raw ptrs were cast
  from, so re-deriving them does not pop a *parent* — **no whole-`Preview`
  reborrow ever sits above the raw ptrs in the borrow stack**.
- `DrawState` (introduced here, reused by §7): the non-`tty`/non-`vx` subset of
  `Preview`'s fields (`should_quit, mouse, themes, filtered, current, window, hex,
  mode, color_scheme, text_input, theme_filter`). `Preview` = `{ tty, vx,
  draw_state: DrawState }`. This lets `update`/`draw` name a `&mut DrawState`
  disjoint from `&mut vx`/`&mut tty`. (`should_quit` lives in `DrawState`; the run
  loop reads `self.draw_state.should_quit` between drains.)
- **A NEW mirihar is required** (correcting the prior "no new mirihar" claim): the
  field-layout aliasing shape is *not* what C3b proved (separate stack vars), so
  §0's `mirihar-wire2` harness must reproduce it — boxed 2-field struct, raw ptrs
  cast from the two fields, a spawned reader touching them, and a main loop
  driving the disjoint-field free fns — and pass `cargo +nightly miri test` to
  prove no reborrow invalidates the live raw ptrs. Record a PORT NOTE citing
  `shaper-hook-sb-clean-interleave` (PROVE on an isolated Miri harness, don't
  hand-reason SB): no `&mut *self` is taken while the read thread is live; only
  disjoint `&mut self.<field>` borrows are; `tty.read` is `&self`; the
  cross-thread `vx.caps` write is the vaxis Loop's contract, not introduced here.

`deinit` (reference `Preview.deinit`): `filtered`/`text_input` drop on their own
(the `.deinit(allocator)` calls collapse); port the order
`vx.deinit(tty.writer())` then `tty.deinit()` (`Box` frees on drop = the
`allocator.destroy(self)`). Model as an explicit `deinit(self: Box<Preview>)` that
runs `self.vx.deinit(self.tty.writer())` then `self.tty.deinit()` — this consumes
the whole `Box` and runs only AFTER `run` returns (the read thread is joined/dead
by then via the Loop's own teardown), so forming whole-`self` access here is sound.

---

## 4. `init` / `run`

`Preview::init` (ports the struct-literal + the trailing `updateFiltered`):

```rust
fn init(themes: &mut [ThemeListElement], theme_filter: ColorScheme) -> Result<Box<Preview>, Error> {
    let themes_len = themes.len();
    let mut me = Box::new(Preview {
        tty: Tty::init(vec![0u8; 4096])?,               // .init(buf)  (buf:[4096]u8)
        vx: Vaxis::init(vaxis::vaxis::Options::default()),
        draw_state: DrawState {
            should_quit: false,
            mouse: None,
            themes,
            filtered: Vec::with_capacity(themes_len),    // initCapacity(themes.len)
            current: 0, window: 0, hex: false,
            mode: Mode::Normal,
            color_scheme: cell::Scheme::Light,           // = .light
            text_input: TextInput::init(),
            theme_filter,
        },
    });
    update_filtered(&mut me.draw_state)?;               // try self.updateFiltered() — pre-loop, no read thread yet
    Ok(me)
}
```

`update_filtered` is a free fn over `&mut DrawState` (§6) — it touches only
DrawState fields (themes/filtered/current/window/text_input/theme_filter), never
tty/vx, so it fits the split cleanly. Called here before the read thread exists
(whole-`me` access is sound pre-`run`).

`run` (the event loop) — faithful to the reference body:

```rust
fn run(&mut self) -> Result<(), Error> {
    let Preview { tty, vx, draw_state } = self;         // split ONCE into disjoint field borrows
    let mut loop_ = Loop::<Event>::new(tty as *mut _, vx as *mut _);
    loop_.init()?;
    loop_.start()?;
    vx.enter_alt_screen(tty.writer())?;
    vx.set_title(tty.writer(), b"\xf0\x9f\x91\xbb Ghostty Theme Preview \xf0\x9f\x91\xbb")?;
    vx.query_terminal(tty.writer(), 1_000_000_000)?;
    vx.set_mouse_mode(tty.writer(), true)?;
    if vx.caps.color_scheme_updates {
        vx.subscribe_to_color_scheme_updates(tty.writer())?;
    }
    while !draw_state.should_quit {
        loop_.poll_event();
        while let Some(event) = loop_.try_event() {
            update(vx, tty, draw_state, event)?;         // free fn, disjoint borrows; NO &mut *self
        }
        draw(vx, draw_state)?;                            // free fn, disjoint from tty + loop_ raw ptrs
        vx.render(tty.writer())?;
        tty.writer().flush()?;
    }
    Ok(())
}
```

The `let Preview { tty, vx, draw_state } = self;` destructure forms the three
disjoint `&mut` field borrows ONCE; `Loop::new` casts `tty`/`vx` to raw ptrs and
thereafter every loop-body call reuses those same field borrows — **no whole-`&mut
*self` reborrow is ever created above the raw ptrs** (the §3 discipline). The
`&mut self.tty`/`&mut self.vx` the raw ptrs derive from and the loop body's
`vx`/`tty` uses are the identical field paths, so re-use does not pop a parent tag.

PORT NOTE: the per-iteration arena `alloc` the reference threads is gone — every
`alloc`-threaded value (`toUri`, save/error `allocPrint`, clipboard base64) is an
owned `Vec<u8>` freed at scope end (crate convention). The reference's
`should_quit`/other field reads become `draw_state.<f>` (the §3 rename).

Borrow disjointness: `vx.<writer-call>(tty.writer())` borrows the `vx` and `tty`
field bindings from the single destructure as **disjoint** paths → sound. `loop_`
holds raw ptrs (not borrows) cast from those same `tty`/`vx` bindings, so it does
not conflict with the loop-body field uses (§3/§5). Crucially, no call in the loop
body reborrows the whole `Preview` — the `update`/`draw` free fns take the
disjoint bindings directly (§3).

---

## 5. `update` (event dispatch) + `up`/`down`

`up`/`down` are pure integer helpers over `&mut DrawState` — port verbatim
(`self.current -|= count` → `st.current = st.current.saturating_sub(count)`; the
`-1` in `down` guarded by the `len == 0` early return, so `filtered.len() - 1` is
safe THERE). They read/write only `current`/`filtered`, so they take `&mut
DrawState` (not `&mut Preview`).

`update` is the free fn `update(vx: &mut Vaxis, tty: &mut Tty, st: &mut
DrawState, event: Event) -> Result<(), Error>` (§3 — disjoint field borrows, never
`&mut Preview`). `vx`/`tty` are needed only by the `.winsize` resize and the
clipboard arms; every other arm mutates only `st`:

```rust
match event {
    Event::KeyPress(owned) => {
        let key = borrow_key(&owned);                    // §5b: KeyOwned → Key<'_>
        if key.matches('c' as u32, mods(ctrl)) { st.should_quit = true; }
        match st.mode {
            Mode::Normal  => { /* the full normal-mode arm ↓ (over st, + vx/tty for clipboard) */ }
            Mode::Help    => { … }
            Mode::Search  => { … }   // labeled-block `break :search` → early `return Ok(())` / restructure
            Mode::Save    => { … }
        }
    }
    Event::ColorScheme(cs) => st.color_scheme = cs,
    Event::Mouse(m)        => st.mouse = Some(m),
    Event::Winsize(ws)     => vx.resize(tty.writer(), ws)?,
}
Ok(())
```

**Every normal-mode key arm ported (R1 — a dropped arm silently breaks nav):**

| Reference match | Rust |
|---|---|
| `matchesAny(&{'q', escape}, {})` → quit | `key.matches_any(&['q' as u32, key::escape], m())` |
| `matchesAny(&{'?', f1}, {})` → `.help` | → `self.mode = Mode::Help` |
| `matches('h', {ctrl})` → `.help` | |
| `matches('/', {})` → `.search` | |
| `matchesAny(&{enter, kp_enter}, {})` → `.save` | |
| `matchesAny(&{'x','/'}, {ctrl})` → `text_input.buf.clear_retaining_capacity(); update_filtered()?` | note: `buf.clear…`, not `text_input.clear…` |
| `matchesAny(&{home, kp_home}, {})` → `current = 0` | `st.current = 0` |
| `matchesAny(&{end, kp_end}, {})` → `current = filtered.len() - 1` | `st.current = st.filtered.len().wrapping_sub(1)` — **empty-filter parity (issue 4)**, see note ↓ |
| `matchesAny(&{'j','+', down, kp_down, kp_add}, {})` → `down(1)` | |
| `matchesAny(&{page_down, kp_down}, {})` → `down(20)` | |
| `matchesAny(&{'k','-', up, kp_up, kp_subtract}, {})` → `up(1)` | |
| `matchesAny(&{page_up, kp_page_up}, {})` → `up(20)` | |
| `matchesAny(&{'h','x'}, {})` → `hex = true` | |
| `matches('d', {})` → `hex = false` | |
| `matches('c', {})` → `vx.copy_to_system_clipboard(tty.writer(), &st.themes[st.filtered[st.current]].theme)?` **else if** `matches('c', {shift})` → `…&…path` | the `if/else if` chain (theme name vs path); NO `alloc` arg (§1 divergence). Indexes `filtered[current]` — see empty-filter note ↓ |
| `matches('f', {})` → cycle `theme_filter` (all→dark→light→all) + `update_filtered(st)?` | |

**PORT NOTE — empty-filter index/underflow parity (issue 4, R1 parity target).**
Three reference sites index or underflow `filtered` with NO `len==0` guard, all
reachable in the degenerate "filter everything out" state (type a query matching
nothing, or cycle `f` to a filter no theme passes):
1. the normal-mode **`end`** arm `current = filtered.len() - 1`;
2. the normal-mode **`c`/`C` clipboard** arms `themes[filtered[current]]`;
3. the **`.save`** draw arm (§7) `themes[filtered[current]]`.
In Zig **ReleaseFast** these wrap / index-UB (unchecked); a literal Rust `- 1` /
`[]` **panics in debug**, a *different observable*. Parity decision, applied at
each site:
- **`end`**: use `filtered.len().wrapping_sub(1)` — faithfully mirrors ReleaseFast's
  `usize` wrap to `SIZE_MAX`. This is harmless: `draw`'s `filtered.len()==0` branch
  resets `current = 0` *before* any indexing, so the wrapped value is never used to
  index (matching the reference, where the same reset masks the wrap).
- **`c`/`C` clipboard** and the **`.save` draw** index: **guard** with an
  `if !filtered.is_empty()` (clipboard: skip the copy; save-draw: skip the theme
  line / the whole `themes[filtered[current]]` read). Rationale: the reference's
  unchecked index is a *latent bug* that in a real Zig **debug** build panics too;
  the port cannot reproduce ReleaseFast's UB soundly, so it guards to keep the
  degenerate state non-panicking. Documented divergence: in the (undefined,
  unobservable) ReleaseFast-UB case the port does nothing instead. `saveSelectedTheme`
  ALREADY has the reference's own `filtered.len()==0` early-return (§8) — that site
  needs no new guard.

PORT NOTE (order-sensitivity): the reference runs these as **independent `if`s**
(not `else if`, except the `'c'` clipboard pair), so e.g. `page_down` also matches
`down`'s `kp_down` set. Preserve the exact sequence of independent `if`s — do NOT
collapse into a `match`, or the overlapping `kp_down`/`page_down` semantics change.
The clipboard `if 'c' … else if 'c'+shift …` is the ONE `else if`.

Help / Search / Save arms: port their `if`s verbatim (all over `st`). The
`.search` block uses a Zig labeled `break :search` (early-out on escape/enter and
on `^X`/`^/`); model as an inner helper returning early, or a `'search: { … }`
restructure — the two early-outs are `return Ok(())` after setting mode / clearing.
The `.search` fall-through does `st.text_input.update(text_input::Event::KeyPress(key));
then update_filtered(st)?`. Save arm: `matches('w', {})` → `save_selected_theme(st)`.

### 5b. `borrow_key` — `KeyOwned` → `Key<'_>`

The queued `Event::KeyPress` owns a `KeyOwned` (no `matches`); the dispatch needs a
borrowed `Key<'_>` (which has `matches`/`matches_any`) and `text_input.update`
takes a `Key<'a>`. All `Key` fields are `pub`, so build the view in-cli (no vaxis
change):

```rust
fn borrow_key(k: &key::KeyOwned) -> key::Key<'_> {
    key::Key { codepoint: k.codepoint, text: k.text.as_deref(),
               shifted_codepoint: k.shifted_codepoint,
               base_layout_codepoint: k.base_layout_codepoint, mods: k.mods }
}
```

---

## 6. `updateFiltered` — the PURE zf-ranking kernel (unit-testable)

The reference `updateFiltered`: (a) snapshot `selected`/`relative`; (b) hash the
old `filtered` (Wyhash digest); (c) rebuild `filtered` — tokenize the lowercased
search string on spaces and, per theme, `loadFile` + `shouldIncludeTheme` gate,
then `zf.rank(theme, tokens, {to_lower,plain})` (append `i` iff `rank != null`),
or (empty search) append every color-passing theme; (d) hash the new `filtered`;
(e) if unchanged, return; else reclamp `current`/`window` to keep `selected`
visible (with the `SMALL_LIST_THRESHOLD` reset). Note: `filtered` is built in
**theme order** (themes are pre-sorted alphabetically in `run`); the rank only
decides **inclusion** (`null` drops), it does NOT reorder.

**Impurity** lives only in the `loadFile` + `shouldIncludeTheme` color gate.
Factor the ranking into a PURE kernel:

```rust
// PURE. Two branches keyed on `has_query` (= text_input.buf.real_length() > 0,
// NOT token-count — see the all-spaces note ↓):
//  has_query == true : tokens ← lowercased(buffer) split on ' ' (tokenizeScalar
//    skips empty runs, so an ALL-SPACES buffer yields ZERO tokens but STILL takes
//    this branch); per theme keep i iff color_pass[i] && zf::rank(name, &tokens,
//    {to_lower:true, plain:true}).is_some() — zf::rank IS CALLED even when tokens
//    is empty; write ranks_out[i] = that Option.
//  has_query == false: per theme keep i iff color_pass[i]; ranks_out[i] = None.
// Byte-faithful (buffer/name are &[u8], never String).
fn rank_filtered(
    theme_names: &[&[u8]],
    buffer: &[u8],      // raw first_half ++ second_half (pre-lowercase)
    has_query: bool,    // = text_input.buf.real_length() > 0
    color_pass: &[bool],
    ranks_out: &mut [Option<f64>],
) -> Vec<usize>
```

**PORT NOTE — all-spaces search edge (issue 5).** The reference branches on
`self.text_input.buf.realLength() > 0`, which counts spaces, so a query of only
spaces has `real_length > 0` and enters the ranking branch, where
`tokenizeScalar(u8, string, ' ')` yields **zero tokens** yet the code still calls
`zf.rank(theme, tokens.items /* empty */, opts)`. This is observably distinct from
the `else` (empty-query) branch, which sets `rank = null` and includes every
color-passing theme unconditionally. The kernel MUST therefore key its branch on
`has_query` (real_length), **not** on `tokens.is_empty()` — routing an all-spaces
query through `zf::rank(name, &[], opts)` exactly as the reference does. (`&[]`
tokens vs the empty-query branch may differ in what `zf::rank` returns/includes;
the port defers that entirely to `zf::rank`, matching the reference call.)

`updateFiltered` then: computes `color_pass[i]` via one **shared, accumulating**
`theme_config` (see the next PORT NOTE) + `should_include_theme(self.theme_filter,
&theme_config)` (impure, reuses the already-ported `should_include_theme`),
computes `has_query = self.text_input.buf.real_length() > 0` and the `buffer =
first_half ++ second_half`, calls `rank_filtered`, writes back `theme.rank =
ranks_out[i]`, and applies the digest/reselect logic. The lowercasing
(`std.ascii.allocLowerString`) and the space tokenize (`tokenizeScalar`, which
skips empty tokens) stay inside the kernel over `&[u8]` (never `String`).

**PORT NOTE — single accumulating `Config` (issue 2, FIDELITY).** Both the
reference `run` enumeration path (`src/cli/list_themes.zig:182`) AND
`updateFiltered` (`:343`) create **one** `Config.default` and call `loadFile`
repeatedly *on that same config* across the whole theme loop — layered
accumulation, **no per-theme reset**. A theme file that omits `background`
therefore inherits the *previous* theme's `background`, which changes
`shouldIncludeTheme`'s luminance verdict (and thus which themes the color gate
admits). The `color_pass` computation in `updateFiltered` MUST reproduce this:
construct **one** `let mut theme_config = Config::default_config();` **outside** the
theme loop and call `theme_config.load_file(&t.path)?` inside it (exactly as the
already-landed Rust `run` at `crates/rustty-cli/src/list_themes.rs:386-395` does) —
do NOT create a fresh `Config` per theme, or background-less themes silently
diverge from the reference. Because this accumulation is *impure and order-
dependent*, it stays in `updateFiltered` (outside the pure `rank_filtered` kernel);
the kernel only receives the resulting `color_pass: &[bool]`.

The digest short-circuit + reselect is a second pure helper:

```rust
// PURE integer/byte math: given the rebuilt filtered, the pre-rebuild selected
// name + relative offset, returns the new (current, window) (or (0,0)).
fn reselect(filtered: &[usize], names: &[&[u8]], selected: Option<&[u8]>,
            relative: usize) -> (usize, usize)
```

`old_digest == new_digest` early-return: port the Wyhash-over-`filtered` (feed
`std.mem.asBytes(&item)` → `item.to_ne_bytes()` per `usize`); use any equal-hash
predicate (the reference only needs "did the filtered set change"; a direct
`old_filtered == new_filtered` `Vec<usize>` compare is behavior-equivalent and
avoids importing Wyhash — record this as a documented simplification PORT NOTE,
OR port Wyhash faithfully if a Wyhash is already available in-tree). Either is
byte-observably identical (the digest is only ever compared for equality).

Both kernels are **pure** (`&[&[u8]]` + integers) → Miri-clean, no tty/config →
the primary unit tests (§9).

---

## 7. `draw` / `drawPreview` + `color` + `lorem_ipsum` — PURE render-to-`Vec<u8>` smoke

`draw`/`drawPreview` touch only `self.vx` (via `window()`) + the drawing state
(`filtered`/`current`/`window`/`hex`/`mode`/`color_scheme`/`theme_filter`/
`text_input`/`mouse`) — **never `self.tty`**. This is BOTH the testability seam
(the draw smoke needs a `Vaxis` + state but no `Tty`) AND the §3 aliasing seam:
`draw`/`draw_preview` are free fns `draw(vx: &mut Vaxis, st: &mut DrawState) ->
Result<(), Error>` / `draw_preview(vx: &mut Vaxis, st: &mut DrawState, win, x_off)`
— they take a `&mut DrawState` disjoint from `&mut Vaxis` (and from `&mut Tty` +
the `loop_` raw ptrs), so the run loop drives them without a whole-`&mut Preview`.
Everywhere the reference reads `self.<f>` (a DrawState field) the port reads
`st.<f>`; `win` comes from `vx.window()`.

Port `draw` verbatim in structure:
- `let win = vx.window(); win.clear(); vx.set_mouse_shape(Shape::Default);`
- `theme_list = win.child(ChildOptions{ x_off:0, y_off:0, width:Some(32), height:Some(win.height), border:Default })`.
- the mouse block (`st.mouse.take()` → `wheel_up`/`wheel_down` → `up(st,1)`/`down(st,1)`;
  `Window::has_mouse(theme_list, Some(mouse))` → left-release selection +
  `highlight = Some(mouse.row)`).
- the `filtered.len()==0` reset else window-scroll clamp
  (`start`/`end = window + theme_list.height - 1`, the three `if`s). `theme_list.height`
  is `u16`; keep the `usize`/`u16` casts faithful (`row: u16` in the loop).
- the per-row loop: `style` = selected/highlighted/normal; the `❯ ` prefix,
  `theme.theme` with `.link = toUri`, the selected right-pad + ` ❮` tail. All
  glyphs (`❯`, `❮`, `█`, box-drawing) stay UTF-8 `&[u8]` literals.
- `draw_preview(vx, st, win, theme_list.x_off + theme_list.width as i32)?`.
- the `mode` switch: `Normal` → `win.hide_cursor()`; `Help` → the 60×22 bordered
  child + the 19-row `key_help` table (each `.{keys, help}` pair → a `(&[u8],
  &[u8])` array, the `—` separator col); `Search` → the bordered child +
  `st.text_input.draw_with_style(child, ui_standard())`; `Save` → the 92×17 child +
  the 14-line `save_instructions` (`allocPrint "theme = {s}"` → a `Vec<u8>` bound
  in-scope). **Empty-filter guard (issue 4):** the reference save arm reads
  `themes[filtered[current]]` with no guard; wrap the `themes[filtered[current]]`
  read (and the `theme = {s}` line it feeds) in `if !st.filtered.is_empty()` per
  the §5 empty-filter PORT NOTE — an empty `filtered` in save mode would otherwise
  panic on the index (Rust debug) where ReleaseFast is UB.

`draw_preview(vx, st, win, x_off) -> Result<(), Error>` (free fn; `win = vx.window()`
passed from `draw`):
- `st.filtered.len()>0` branch: `Config::default_config()` + `config.load_file(&theme.path)`;
  the `catch |err|` error path → the centered "Unable to open …"/path/`{err}` pane.
  **FORMATTER NOTE (issue 3, VERIFIED):** the Rust `LoadError`
  (`crates/rustty-config/src/config_struct.rs:5192`) is `#[derive(Debug)]` with **NO
  `Display` impl** — so the reference's `try std.fmt.allocPrint(alloc, "{}", .{err})`
  (which on a Zig error prints the bare error name, e.g. `FileNotFound`) has no
  `Display` analogue; the port formats the err via `format!("{err:?}")` (Debug) into
  a `Vec<u8>`. For the unit-variant `FileNotFound` this renders identical text; the
  `Io(..)`/`Parse(..)` variants render Rust Debug text that DIFFERS from Zig's bare
  name. **This is a documented COSMETIC divergence** (the error pane's exact wording
  only) — the draw smoke asserts only the stable `b"Unable to open"` + theme-name
  prefix, never the err line. On success: the fg/bg/cursor/selection colors (§1
  fidelity note), the 12 `standard*` styles, the 4-row title/path child, the
  optional `_diagnostics` child. **DIAGNOSTICS NOTE (issue 3, VERIFIED):**
  `config._diagnostics` is a `rustty_args::diagnostics::DiagnosticList` exposing
  `items() -> &[Diagnostic]` (`crates/rustty-args/src/diagnostics.rs:322`) and each
  `Diagnostic` has `format(&self, writer: &mut Vec<u8>)`
  (`diagnostics.rs:53`) — a byte-faithful port of the reference
  `diag.format(&buf.writer)`. So the reference `for (…items()) |diag| { try
  diag.format(&buf.writer); … }` maps 1:1: call `diag.format(&mut buf)` into a reused
  `Vec<u8>`, print `buf`, clear. (The exact diagnostic wording is whatever
  `Diagnostic::format` emits — a landed, tested formatter, not assumed.) Then the
  6-row 16-color swatch grid (`color(config, i)` = `Color::Rgb([palette.value[i].r/g/b])`;
  hex/decimal index via `st.hex`), the 24-row `ziggzagg.zig` `bat` sample (the long
  `child.print(&[Segment,..], …)` blocks — ported segment-for-segment), and the
  `lorem_ipsum` fill (`splitAny(LOREM_IPSUM, b" \n")` word iterator with reset;
  the 9 special-word styles). `else` branch: the full-window "No theme found!"
  centered pane.

`color(config, palette)` → `fn color(config: &Config, palette: usize) ->
cell::Color { Color::Rgb([config.palette.value[palette].r, .g, .b]) }`.

**Draw factoring (test AND aliasing)**: `draw`/`draw_preview` are **free
functions** `(vx: &mut Vaxis, st: &mut DrawState, …)` — NOT `&mut Preview` methods
(the §3 discipline requires the run loop to call them on disjoint field borrows,
never forming a whole `&mut Preview` while the read thread lives). `DrawState` is
the concrete embedded struct of §3 (`Preview = { tty, vx, draw_state: DrawState }`),
not merely a view. The smoke test builds a `Vaxis` + `DrawState` directly (no
`Tty`) and calls `draw(&mut vx, &mut st)` — see §9. The invariant, load-bearing for
BOTH the test seam and the aliasing proof: `draw`/`draw_preview` never name a
`Tty` and never take `&mut Preview`.

---

## 8. `toUri` / `saveSelectedTheme` / `resolveAutoThemePath` / `writeAutoThemeFile`

- `ThemeListElement::to_uri(&self) -> Vec<u8>` — the reference builds a
  `std.Uri{ scheme:"file", host:.{.raw=""}, path:.{.raw=self.path} }` and
  `writeToStream(.{scheme,authority,path})`. Port as a faithful `file://` URI
  builder over `&[u8]` (scheme `b"file:"` + authority `b"//"` + percent-encoded
  path per RFC 3986, matching `std.Uri.writeToStream`'s path-escaping). Returns an
  owned `Vec<u8>` (the `toOwnedSlice`). Bind the result in-scope at each call site
  so the `Hyperlink.uri: &[u8]` borrow is valid across `print_segment`. (Fill-time:
  match `std.Uri`'s exact escape set; the theme paths are ASCII-safe in practice
  but port the escaper faithfully for R1.)
- `resolve_auto_theme_path() -> Result<Vec<u8>, Error>`: `preferred_default_file_path()`
  (landed, returns `String`) → `dirname` → `join(base, "auto", "theme.ghostty")`
  (reuse the file's existing `join` helper; `dirname == None` → the reference
  `error.BadPathName`, map to an `Error` arm / io error).
- `write_auto_theme_file(theme_name: &[u8]) -> Result<(), Error>`: `create_dir_all`
  the dirname, `File::create` (truncate), write `b"theme = " + theme_name + b"\n"`.
  Byte-faithful (`theme_name` stays `&[u8]`).
- `save_selected_theme(st: &mut DrawState)` (free fn over DrawState, called from
  the `update` Save arm — §5): `st.filtered.is_empty()` → return **first** (the
  reference's own `if (self.filtered.items.len == 0) return;` guard, so the
  subsequent `filtered[current]` index is safe — this is site (3)-adjacent but
  ALREADY guarded upstream, no issue-4 change needed here); else
  `write_auto_theme_file(&st.themes[st.filtered[st.current]].theme)` with the
  reference's `catch { return; }` (swallow the error — a `let _ = …;`).

These are straightforward `std.fs` ports; move them out of the verbatim tail into
live code alongside the enumeration `run`.

---

## 9. Test strategy

- **Interactive `run`/Loop path** — `#[ignore]`-on-device. A single
  `#[test] #[ignore]` (`preview_run_smoke`) that would `Tty::init` (needs
  `/dev/tty`, absent in the sandbox; `Tty = PosixTty` in cli's dep graph — the
  `TestTty` alias is gated on *rustty-vaxis's own* `#[cfg(test)]`, not cli's). Runs
  only on a real terminal; not under Miri (spawns a thread + FFI).

- **`rank_filtered` (PURE, §6)** — the primary unit tests, Miri-clean:
  - inclusion: given theme names `[b"Solarized Dark", b"Gruvbox", b"Nord"]`, an
    all-`true` `color_pass`, `has_query=true`, and buffer `b"grv"` → asserts
    `filtered == [1]` (only Gruvbox fuzzy-matches); `has_query=false` → all three;
    buffer `b"zzz"` → empty.
  - **all-spaces edge (issue 5)**: buffer `b"   "` with `has_query=true` yields
    ZERO tokens but MUST still route through `zf::rank(name, &[], opts)` — assert
    its result matches an explicit `zf::rank(name, &[], {to_lower,plain})` call,
    NOT the `has_query=false` (include-all, rank=None) branch. This pins that the
    branch keys on `has_query` (real_length), not on `tokens.is_empty()`.
  - rank ordering: a filter where two names match asserts the written
    `ranks_out` reflect `zf::rank`'s relative values (better/contiguous match ⇒
    lower `f64`), pinning that WIRE2 consumes zf's exact ranks.
  - color gate: `color_pass = [true,false,true]` drops the masked index.
  - `reselect`: pins `(current, window)` for the `SMALL_LIST_THRESHOLD` reset and
    the `selected`-preservation walk (pure integer/byte math).

- **NEW `mirihar-wire2` interleave harness (§0/§3, aliasing proof)** — the
  correction to the "no new mirihar" claim. A standalone `#![cfg(miri)]`-style
  harness reproducing the struct-field shape: a boxed 2-field `{ a, b }` stand-in
  for `Preview{tty,vx}` plus a `DrawState`-like third field; cast `&mut box.a`/
  `&mut box.b` to raw ptrs (as `Loop::new` does); spawn a reader thread that
  `(*a)`/`(*b)`-derefs under a sync boundary; and a main loop that drives free fns
  over the disjoint `(&mut a, &mut b, &mut draw_state)` split — asserting under
  `cargo +nightly miri test` (Stacked AND Tree Borrows) that NO reborrow pops the
  live raw ptrs. Authored fresh (NOT reused from `mirihar-chunk3b`, whose stack-var
  shape does not cover this).

- **`draw` smoke (render-to-`Vec<u8>`, §7)** — Miri-clean, no `Tty`:
  build `let mut vx = Vaxis::init(Options::default()); vx.resize(&mut sink,
  Winsize{rows:40,cols:120,..})?;` then a `DrawState` (built directly, no `Preview`/
  `Tty`) and call `draw(&mut vx, &mut st)?`: (a) `filtered` empty → drives the "No
  theme found!" pane (needs no config), assert the rendered `Vec<u8>` **contains**
  `b"No theme found!"`; and/or (b) one `ThemeListElement` with a **bogus path** →
  `draw_preview` `load_file` `Err` → the "Unable to open" pane, assert it contains
  `b"Unable to open"` + the theme name (NOT the `{err:?}` line — that is the issue-3
  cosmetic-divergence text, deliberately unasserted). `render(&mut Vec<u8>)` accepts
  any `impl Write`. (Optionally a `Mode::Help` variant asserting a `key_help` string
  like `b"Toggle help window."`.) These exercise the child-window layout +
  `print_segment` path without a real theme file or tty. (An empty-`filtered` +
  `Mode::Save` variant also confirms the issue-4 guard renders without panicking.)

- `check_test_counts.py`: the file currently declares **0 tests**; update the
  expected count to the new total (the ignored device smoke + the `rank_filtered`/
  all-spaces/`reselect`/`draw`/save-guard unit tests — a documented small number).
  The `mirihar-wire2` harness lives under `.port/phase-2/` (its own crate), not in
  the cli test count.

---

## 10. Fidelity checklist (R1)

- [ ] Every reference line/comment of `Event` + `Preview` (`init`/`deinit`/`run`/
      `update`/`up`/`down`/`updateFiltered`/`draw`/`drawPreview`/`saveSelectedTheme`
      + the `ui_*` helpers) + `color` + `lorem_ipsum` + `preview` + `toUri` +
      `resolveAutoThemePath`/`writeAutoThemeFile` ported; the tail verbatim comment
      block REPLACED by the live port with per-line PORT NOTEs where non-obvious
      (owned-`KeyOwned` queue, `borrow_key`, clipboard-no-`alloc`, `TerminalColor`
      cell-* mapping, arena→owned-`Vec`, Wyhash→set-compare, `std.Uri`→`file://`,
      the `Preview{tty,vx,draw_state}` split + `self.<f>`→`st.<f>` rename).
- [ ] All text/paths/theme-names/`lorem_ipsum`/URIs are `&[u8]`/`Vec<u8>` — never a
      lossy `String`; `Style`/`Color`/`Segment`/`Hyperlink` literals field-for-field.
- [ ] EVERY `update` key arm present (the independent-`if` order preserved; the one
      `'c'`/`'c'+shift` `else if`); mouse (wheel/left-release/highlight) faithful.
- [ ] `updateFiltered` inclusion (`zf::rank` null-drop) + color gate + reselect +
      digest short-circuit faithful; `filtered` in theme order (no re-sort). Color
      gate uses ONE accumulating `Config` (issue 2). All-spaces query routes through
      `zf::rank(&[])` via `has_query`/real_length, not token-count (issue 5).
- [ ] Loop-holds-raw-self aliasing uses the **disjoint-field-split** discipline
      (`Preview{tty,vx,draw_state}`; `run` destructures ONCE; `update`/`draw` are
      free fns over disjoint borrows; **NO** `&mut *self` while the read thread
      lives; `tty.read` `&self`) — and is PROVEN by the **NEW `mirihar-wire2`**
      harness for this struct-field shape (issue 1; C3b's stack-var proof does not
      cover it).
- [ ] Empty-filter degenerate state (issue 4): `end` uses `wrapping_sub(1)`;
      `c`/`C` clipboard + `.save` draw index guarded by `!filtered.is_empty()`;
      `save_selected_theme` keeps the reference's own len==0 early-return — no bare
      `- 1`/index that panics in Rust debug where ReleaseFast wraps/UBs.
- [ ] `LoadError` err-pane text (issue 3): NO `Display` exists → `{err:?}` (Debug),
      a documented cosmetic divergence; `Diagnostic::format(&mut Vec<u8>)` +
      `DiagnosticList::items()` VERIFIED landed (rustty-args), ported 1:1.
- [ ] `--plain` / non-tty enumeration path unchanged (only `preview`'s sig widens
      to `&mut [ThemeListElement]`).
- [ ] Deps: `+rustty-zf` only; `lorem_ipsum.txt` copied. Gates: build / test /
      clippy `-D` / fmt / acyclic / test-counts / comment-coverage / Miri / the
      NEW `mirihar-wire2` interleave harness.

---

## 11. Disposition

**ready** — no missing vaxis/zf API and no open verification blocker. Every
reference call maps 1:1 to a landed Rust analogue (§1): `Loop::{new,init,start,
poll_event,try_event}`, `Vaxis::{init,window,render,resize,enter_alt_screen,
set_title,query_terminal,set_mouse_mode,set_mouse_shape,subscribe_to_color_scheme_updates,
copy_to_system_clipboard,deinit}`, `Tty::{init,writer,get_winsize,deinit}`,
`Window::{child,clear,fill,hide_cursor,has_mouse,print,print_segment}`,
`TextInput::{init,update,draw_with_style,clear_retaining_capacity, buf.*}`,
`Key::{matches,matches_any}` + the key/mouse constants, and
`zf::rank(&[u8], &[&[u8]], RankOptions{to_lower,plain})`; and the two formatter
deps (issue 3) are VERIFIED landed: `Diagnostic::format(&mut Vec<u8>)` +
`DiagnosticList::items()` (rustty-args), while `LoadError` has no `Display`
(`{err:?}` Debug, a documented cosmetic divergence). The fill-time fidelity notes
(`TerminalColor` cell-* arms, `std.Uri`→`file://` escaper, Wyhash→set-compare,
`LoadError` Debug text) are documented mappings, not blockers.

The one **substantive correction** from adversary review (issue 1): the Loop
aliasing is NOT a verbatim C3b reuse — the tty/vx-as-struct-fields shape requires
the disjoint-field-split discipline (§3: `Preview{tty,vx,draw_state}`, free-fn
`update`/`draw`, never `&mut *self` while the read thread lives) AND a **fresh
`mirihar-wire2` interleave proof** (§0/§9). This is a build-time-verifiable
obligation (the harness must pass `cargo +nightly miri test`), not an open design
blocker: the split is standard disjoint-borrow Rust and the raw-ptr/queue contract
is unchanged from C3b — only the *proof surface* is new. Issues 2 (single
accumulating `Config`), 4 (empty-filter `wrapping_sub`/guards), and 5 (all-spaces
`has_query` routing) are pinned PORT NOTES in §5/§6/§7. Implement per §2–§8; test
per §9.
