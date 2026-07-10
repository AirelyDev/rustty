# WIRE1 — un-defer the `list_colors` + `list_keybinds` interactive `prettyPrint` paths

TUI-CLI wiring 1. Fill the two R5 `unimplemented!()` `prettyPrint` seams in
`crates/rustty-cli/src/{list_colors,list_keybinds}.rs` from their preserved
verbatim Zig bodies, using the now-landed **`rustty-vaxis`** core (Vaxis / Window
/ Tty / ctlseqs / Style / Color / prettyPrint).

- Reference (read-only oracle): `ghostty/src/cli/{list_colors,list_keybinds}.zig`.
- `--plain` / non-tty paths already work and **stay untouched**.
- DISPOSITION: **ready** — no vaxis API is missing; every call the reference
  `prettyPrint` makes has a landed Rust analogue (mapped 1:1 below).

---

## 0. Dependency + gate deltas

- **Cargo**: add exactly `rustty-vaxis = { workspace = true }` to
  `crates/rustty-cli/Cargo.toml` `[dependencies]` (task-mandated: "+ nothing
  else"). `rustty-cli` is a top consumer; `rustty-vaxis` is a low/mid vendored
  crate with **no** edge back to cli/config/apprt → `cli → vaxis` is a downward
  edge, **acyclic** (`check_acyclic.sh`, R4).
- One cli-local addition to `crates/rustty-cli/src/ghostty.rs`: a
  `From<rustty_vaxis::tty::TtyError> for Error` (new `Error::Tty(TtyError)`
  variant). `vaxis::Tty::init` / `get_winsize` return `TtyError`, which the
  reference `try`s into `prettyPrint`'s inferred `!u8`; `Error` already has
  `From<io::Error>` for the `resize`/`pretty_print` (`io::Result`) calls. This is
  cli-crate-internal, adds no dependency.
- Gates to re-run green: build / test / clippy `-D` / fmt / `check_acyclic.sh` /
  `check_test_counts.py` / `check_comment_coverage.py` (scoped-G3b — the verbatim
  comment blocks are REPLACED by the live port, whose own line/comment fidelity
  must hold) / Miri (the render seam is pure over a `Vec<u8>` writer → Miri-able).

---

## 1. The landed `rustty-vaxis` surface (verified) — reference → Rust map

| Reference (`vaxis.*`) | Rust analogue | Notes |
|---|---|---|
| `vaxis.Tty.init(&buf)` (`buf: [1024]u8`) | `vaxis::Tty::init(vec![0u8; 1024])? ` → `Result<_, TtyError>` | `Tty = PosixTty` in a normal (non-test) build of vaxis. |
| `tty.deinit()` | `tty.deinit()` (consumes `self`) | |
| `tty.writer()` | `tty.writer() -> &mut TtyWriter` | `TtyWriter: std::io::Write` (vaxis.rs `impl std::io::Write for TtyWriter`). |
| `tty.fd` | `tty.fd() -> c_int` | |
| `vaxis.Tty.getWinsize(tty.fd)` | `vaxis::Tty::get_winsize(tty.fd())? ` → `Result<Winsize, TtyError>` | |
| `vaxis.init(alloc, .{})` | `vaxis::Vaxis::init(vaxis::vaxis::Options::default())` | `Options` derives `Default` (`.{}`); `alloc` dropped. |
| `vx.caps.unicode = .unicode` | `vx.caps.unicode = vaxis::gwidth::Method::Unicode` | `caps: Capabilities { unicode: Method, .. }`. |
| `vaxis.ctlseqs.unicode_set` / `_reset` | `vaxis::ctlseqs::unicode_set` / `unicode_reset` (`&[u8]`) | |
| `vx.resize(alloc, w, winsize)` | `vx.resize(w, winsize)?` → `io::Result<()>` | `W: Write`. |
| `vx.window()` | `vx.window() -> Window<'a>` | `Window` is `Copy`, holds `*mut Screen` (raw ptr, not a live borrow — §5). |
| `vx.prettyPrint(w)` | `vx.pretty_print(w)?` → `io::Result<()>` | |
| `vx.deinit(alloc, w)` | `vx.deinit(w)` | swallows reset errors (Zig `catch {}`). |
| `win.width` | `win.width: u16` | |
| `win.clear()` | `win.clear()` | |
| `win.gwidth(bytes)` | `win.gwidth(&[u8]) -> u16` | |
| `win.printSegment(seg, opts)` | `win.print_segment(seg, opts) -> PrintResult` | |
| `win.print(&[seg,..], opts)` | `win.print(&[Segment], opts) -> PrintResult` | |
| `vaxis.Window.PrintResult{col,row,overflow}` | `vaxis::window::PrintResult { col: u16, row: u16, overflow: bool }` | |
| `.{ .col_offset = c }` | `PrintOptions { col_offset: c, ..Default::default() }` | |
| `vaxis.Style{ .fg=.., .bg=.., .bold, .dim }` | `vaxis::cell::Style { fg, bg, bold, dim, ..Default::default() }` | |
| `.{ .rgb = .{r,g,b} }` | `Color::Rgb([r, g, b])` | |
| `.{ .index = n }` | `Color::Index(n)` | |
| `.{ .text = t }` (segment) | `Segment { text: t, style: Style::default(), link: Hyperlink::default() }` | `Segment{ text:&[u8], style, link }`. |
| `vaxis.Winsize` | `vaxis::Winsize { rows, cols, x_pixel, y_pixel }` | |

**Byte-slice fidelity (R1)**: every `.text` is `&[u8]` (`name.as_bytes()`,
`b" = "`, `b"  "`, the formatted `#rrggbb` / action / key bytes) — never a lossy
`String`. `Style`/`Color`/`Segment` mirror the Zig field literals field-for-field.

---

## 2. `list_colors.rs` fill

Replace the `unimplemented!()` + verbatim comment block in `pretty_print`. The
render is factored into **two** functions so a `Vec<u8>` smoke test can exercise
the styled-Window build without a real tty (§4):

### 2a. `pretty_print(keys: &[&str]) -> Result<u8, Error>` (outer — device path)

Ports the tty/vx lifecycle + the three Zig `defer`s (LIFO):

```
let mut tty = vaxis::Tty::init(vec![0u8; 1024])?;   // var buf:[1024]u8; Tty.init(&buf)
let mut vx  = vaxis::Vaxis::init(Options::default());
vx.caps.unicode = Method::Unicode;                  // enable mode 2027
tty.writer().write_all(ctlseqs::unicode_set)?;
let winsize = winsize_for(&tty)?;                   // §2c
let res = render_colors(tty.writer(), &mut vx, winsize, keys);   // §2b (testable)
// defers, LIFO — fire on BOTH Ok and Err (res captured, not `?`-ed yet):
let _ = tty.writer().write_all(ctlseqs::unicode_reset);   // defer unicode_reset (LIFO-first)
vx.deinit(tty.writer());                                  // defer vx.deinit
tty.deinit();                                             // defer tty.deinit (LIFO-last)
res?;
Ok(0)
```

PORT NOTE to record inline: the three reference `defer`s are emitted after the
body in LIFO order (`unicode_reset` → `vx.deinit` → `tty.deinit`) and fire on
every exit. Minor documented divergence: if `unicode_set` itself fails the
reference has not yet registered the `unicode_reset` defer; here `res` short-
circuits before the loop and cleanup still runs (harmless — the process exits).

### 2b. `render_colors<W: Write>(w, vx, winsize, keys) -> io::Result<()>` (inner — testable)

Everything from `vx.resize` onward, verbatim in structure:

```
vx.resize(w, winsize)?;
let win = vx.window();
let mut max_name_len = 0usize;
for name in keys { max_name_len = max_name_len.max(name.len()); }
let column_size = max_name_len + 15;                       // usize
let columns = (win.width as usize + 2) / column_size;     // @divFloor
let step = keys.len() / columns + 1;
for i in 0..step {
    win.clear();
    let mut result = PrintResult { col: 0, row: 0, overflow: false };
    for j in 0..columns {
        let k = i + step * j;
        if k >= keys.len() { continue; }
        let name = keys[k];
        let rgb = x11_color::map.get(name).unwrap();
        let style1 = Style { fg: Color::Rgb([rgb.r, rgb.g, rgb.b]), ..Default::default() };
        let style2 = Style { fg: Color::Rgb([rgb.r, rgb.g, rgb.b]),
                             bg: Color::Rgb([rgb.r, rgb.g, rgb.b]), ..Default::default() };
        result = win.print_segment(seg(name.as_bytes()), po(result.col));   // name
        for _ in 0..(max_name_len - name.len()) {
            result = win.print_segment(seg(b" "), po(result.col));          // pad
        }
        result = win.print_segment(seg(b" = "), po(result.col));
        let hex = format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b);       // allocPrint
        result = win.print_segment(styled(hex.as_bytes(), style1), po(result.col));
        result = win.print_segment(seg(b" "), po(result.col));
        result = win.print_segment(styled(b"  ", style2), po(result.col));  // colored block
        if j + 1 < columns {
            result = win.print_segment(seg(b"  "), po(result.col));         // gutter
        }
    }
    while result.col != 0 {                                                  // clear rest of line
        result = win.print_segment(seg(b" "), po(result.col));
    }
    vx.pretty_print(w)?;                                                     // output row
}
Ok(())
```

`seg(t)` = `Segment{text:t, style:Style::default(), link:Hyperlink::default()}`;
`styled(t,s)` = same with `style:s`; `po(c)` = `PrintOptions{col_offset:c,
..Default::default()}`. `hex` must be bound in-scope so the `&[u8]` borrow lives
across the `print_segment` (mirrors the reference `allocPrint` lifetime).

Borrow note: `let win = vx.window()` returns a `Copy` `Window` holding a raw
`*mut Screen`; it does **not** keep `&mut vx` alive, so the interleaved
`vx.pretty_print(w)` at loop end is sound — this is exactly the established §5
alias pattern the C4a render tests already exercise.

`x11_color::map.get`/`.keys()` and the `rgb.{r,g,b}: u8` fields are the same
surface the live `--plain` path uses today.

### 2c. `winsize_for` (the `builtin.os.tag` switch)

```
fn winsize_for(tty: &vaxis::Tty) -> Result<Winsize, Error> {
    #[cfg(windows)]  { Ok(Winsize{rows:24, cols:120, x_pixel:1024, y_pixel:768}) }
    #[cfg(not(windows))] { Ok(vaxis::Tty::get_winsize(tty.fd())?) }
}
```

Faithful to the `.windows => {24,120,1024,768}` / `else => getWinsize(tty.fd)`
switch. (Could inline; a helper keeps the `#[cfg]` clean and is shared verbatim
by both actions.)

---

## 3. `list_keybinds.rs` fill

The larger fill: port `TriggerNode` + `ChordBinding` + `iterateBindings` + the
`prettyPrint` body. Same outer/inner split (`pretty_print` device wrapper →
`render_keybinds<W: Write>(w, vx, winsize, keybinds)` testable seam); the tty
lifecycle/defers are identical to §2a (the reference binds `writer` once at top —
same three defers, `writer.flush()` before `return 0` is subsumed by
`vx.pretty_print`'s own flush + our explicit final `Ok(())`).

### 3a. `TriggerNode` / `ChordBinding` — the SinglyLinkedList port

The reference uses an **intrusive `std.SinglyLinkedList` of `TriggerNode`** with
`@fieldParentPtr` + arena `alloc.create`. The observable semantics are: an
ordered sequence of `Trigger`s supporting **forward iteration**, **`len()`**, and
**`prepend`** (leader chords). Port `TriggerNode`/`SinglyLinkedList` as a plain
owned **`Vec<Trigger>`** (`prepend` = `insert(0, ..)`, `.first`-walk = `.iter()`,
`.len()` = `.len()`, `node.next != null` = `idx + 1 < len`). This is the faithful,
`unsafe`-free port of a local (non-`pub`) cli helper — the intrusive list +
`@fieldParentPtr` are an arena-allocation vehicle, not observable behavior. Record
this as an explicit PORT NOTE (mirrors the placeholder-leaf doctrine: preserve
semantics, not the allocation mechanism).

```
struct ChordBinding<'a> {
    table_name: Option<&'a [u8]>,   // ?[]const u8  (borrows a tables map key)
    triggers:   Vec<Trigger>,       // std.SinglyLinkedList<TriggerNode>
    actions:    Vec<Action>,        // []const Binding.Action (owned; Action: Clone)
}
```

Types resolve as: `Trigger` / `TriggerKey` / `Mods` / `Action` / `Value` /
`Set` from `rustty_input::binding`; `Key` (physical) from
`rustty_vt::input::key` (`use rustty_vt::input::key::Key as KeyKey`).

### 3b. `ChordBinding::less_than` → `Ordering` comparator

Port `lessThan(void, lhs, rhs) bool` as a `cmp(&ChordBinding, &ChordBinding) ->
Ordering` for `std.mem.sort` (→ Rust **stable** `sort_by`, matching Zig
`std.mem.sort`). Each reference `return <bool>` becomes `Less`/`Greater`; the
comparator only needs to return `Equal` on the (unreachable) fall-through.

Faithful field access:
- `lhs.table_name != null` → `.table_name.is_some()`; `!lhs_has_table` first
  (defaults before tables).
- `std.mem.order(u8, a, b)` → `a.cmp(b)` on `&[u8]` (lexicographic — same).
- `triggers.len()` → `.len()`; keep the two `assert(len != 0)` as `debug_assert!`.
- modifier count: `mods.super/ctrl/shift/alt` bools (`mods.super_` in Rust) summed.
- `mods.int()` → `Mods::int(self) -> u8` (landed).
- key sort key: `.physical => @intFromEnum(k)` → `k as c_int`; `.unicode =>
  @intCast(c)` → `c as c_int`; `.catch_all => maxInt(c_int)` → `c_int::MAX`.
  (The `TriggerKey::Physical(p) => p as c_int` cast already exists in binding.rs
  — the `Key` enum discriminants are ported faithfully, so this matches
  `@intFromEnum`.) Preserve the final `unreachable` as `unreachable!()`.

### 3c. `iterate_bindings` — recursive chord flattener

```
fn iterate_bindings<'a>(iter: impl Iterator<Item=(&'a Trigger, &'a Value)>,
                        win: Window) -> (Vec<ChordBinding<'a>>, u16)
```

`win` is `Copy` → pass by value (reference `win: *const vaxis.Window`, used only
for `gwidth`). Per entry `(t, value)` (from `set.iter_bindings()` — the landed
`impl Iterator<Item=(&Trigger,&Value)>` mirroring `bindings.iterator()` yielding
`key_ptr.*`/`value_ptr.*`):

- **width**: build a `Vec<u8>` (the `std.Io.Writer.Allocating` analogue) — push
  `b"super + "` / `b"ctrl  + "` / `b"alt   + "` / `b"shift + "` per active mod
  (note the reference's exact space padding), then the key: `.physical =>
  Key::tag_name(k).as_bytes()` (`{t}` = `@tagName`), `.unicode =>` UTF-8 of the
  codepoint (`{u}` = `char::from_u32(c as u32).unwrap().encode_utf8(..)`),
  `.catch_all => b"catch_all"`. `width = win.gwidth(&buf)`.
- **`Value::Leader(box_set)`**: recurse `iterate_bindings(box_set.iter_bindings(),
  win)`; for each sub-binding `insert(0, *t)` (prepend current trigger);
  `widest_chord = max(widest_chord, width + max_width + 5)` (`'  >  '` padding);
  extend `bindings`.
- **`Value::Leaf(leaf)`**: `bindings.push(ChordBinding{ triggers: vec![*t],
  actions: vec![leaf.action.clone()], table_name: None })`; `widest_chord =
  max(.., width)`.
- **`Value::LeafChained(leaf)`**: `actions: leaf.actions.clone()` (owned
  `Vec<Action>`), `triggers: vec![*t]`; `widest_chord = max(.., width)`.

Returns `(bindings, widest_chord)` (Zig `toOwnedSlice` → owned `Vec`).

### 3d. `render_keybinds` body

```
vx.resize(w, winsize)?;
let win = vx.window();
let (default_bindings, mut widest_chord) =
    iterate_bindings(keybinds.set.iter_bindings(), win);
let mut bindings_list: Vec<ChordBinding> = default_bindings;
let mut widest_table_prefix: u16 = 0;
for (table_name, set) in keybinds.tables.iter() {          // tables.iterator()
    let (mut table_bindings, table_width) = iterate_bindings(set.iter_bindings(), win);
    for b in &mut table_bindings { b.table_name = Some(table_name.as_slice()); }
    widest_chord = widest_chord.max(table_width);
    widest_table_prefix = widest_table_prefix
        .max(win.gwidth(table_name) + win.gwidth(b"/"));
    bindings_list.extend(table_bindings);
}
bindings_list.sort_by(ChordBinding::cmp);                  // std.mem.sort (stable)

let super_style = Style { fg: Color::Index(1), ..Default::default() };
let ctrl_style  = Style { fg: Color::Index(2), ..Default::default() };
let alt_style   = Style { fg: Color::Index(3), ..Default::default() };
let shift_style = Style { fg: Color::Index(4), ..Default::default() };
let table_style = Style { fg: Color::Index(8), ..Default::default() };

for bind in &bindings_list {
    win.clear();
    let mut result = PrintResult { col:0, row:0, overflow:false };
    if let Some(name) = bind.table_name {
        result = win.print_segment(styled(name, table_style), po(result.col));
        result = win.print_segment(styled(b"/", table_style), po(result.col));
    }
    let n = bind.triggers.len();
    for (idx, trigger) in bind.triggers.iter().enumerate() {
        if trigger.mods.super_ {
            result = win.print_segment(styled(b"super", super_style), po(result.col));
            result = win.print_segment(seg(b" + "), po(result.col));
        }
        // ctrl "ctrl " / alt "alt  " / shift "shift" — same shape, note padding
        let key: Vec<u8> = match trigger.key {                 // allocPrint {t}/{u}
            TriggerKey::Physical(k) => Key::tag_name(k).as_bytes().to_vec(),
            TriggerKey::Unicode(c)  => utf8(c),                // char::encode_utf8
            TriggerKey::CatchAll    => b"catch_all".to_vec(),
        };
        result = win.print_segment(seg(&key), po(result.col));
        if idx + 1 < n {                                       // node.next != null
            result = win.print_segment(
                styled(b"  >  ", Style{ bold:true, fg:Color::Index(6), ..Default::default() }),
                po(result.col));
        }
    }
    let mut action_col: u16 = widest_table_prefix + widest_chord + 3;
    for (i, act) in bind.actions.iter().enumerate() {
        if i > 0 {
            let r = win.print_segment(
                styled(b", ", Style{ dim:true, ..Default::default() }), po(action_col));
            action_col = r.col;
        }
        let mut aw = AllocatingWriter::init();                 // {f} of Action
        act.format(&mut aw).unwrap();                          // Infallible
        let action = aw.written();                             // &[u8]
        if let Some(pos) = action.iter().position(|&b| b == b':') {
            let r = win.print(&[
                seg(&action[..pos]),
                styled(&action[pos..pos+1], Style{ dim:true, ..Default::default() }),
                styled(&action[pos+1..],    Style{ fg:Color::Index(5), ..Default::default() }),
            ], po(action_col));
            action_col = r.col;
        } else {
            let r = win.print_segment(seg(action), po(action_col));
            action_col = r.col;
        }
    }
    vx.pretty_print(w)?;
}
Ok(())
```

`keybinds` type is `rustty_config::value_types::Keybinds` (already the seam's
current signature). `keybinds.set` : `rustty_input::binding::Set` →
`iter_bindings()`. `keybinds.tables` :
`AutoArrayHashMapUnmanaged<Vec<u8>, Set>` — needs an insertion-ordered
`iter() -> (&Vec<u8>, &Set)` accessor (the `RepeatableStringMap` map type; verify
it exposes `.iter()`/`.entries` — if only `entries` is public, iterate that). The
`AllocatingWriter` (`rustty_cfg_fmt::AllocatingWriter`, already a cli dep) is the
`std.Io.Writer.Allocating` analogue for `Action::format`'s `{f}`; its `Error =
Infallible`, so `.unwrap()`.

`utf8(c: u21)` = `char::from_u32(c as u32).unwrap().encode_utf8(&mut [0u8;4])`
into an owned `Vec<u8>` (the `{u}` UTF-8 encode of the codepoint).

RESOLVED: `keybinds.tables.iter() -> impl Iterator<Item=(&Vec<u8>, &Set)>`
(insertion-ordered) is landed (`rustty_zigstd::array_hash_map:119`) and already
used by `Keybinds::format_entry_docs` (`value_types.rs:4749`). Nothing missing.

---

## 4. Test strategy

Mirror the vaxis C4a render smoke tests (`vaxis.rs::tests::render_emits_*`):
build a `Vaxis::init(Options::default())`, drive rendering into a `Vec<u8>`,
assert on emitted bytes.

- **Interactive device path** (`pretty_print` outer): opens a real
  `vaxis::Tty` (`= PosixTty` in a normal build of rustty-cli's dep graph — the
  `Tty = TestTty` alias is gated on *rustty-vaxis's own* `#[cfg(test)]`, which
  does **not** apply to rustty-cli's tests). `Tty::init` needs `/dev/tty`, absent
  in the sandbox → a single `#[test] #[ignore]` device smoke per action
  (`list_colors_pretty_print_smoke` / `list_keybinds_pretty_print_smoke`) that
  runs only on a real terminal.

- **Render seam** (`render_colors` / `render_keybinds`): fully testable — takes
  `&mut impl Write` + `&mut Vaxis` + a synthetic `Winsize`, no tty. Tests:
  - `render_colors`: pass a small `&["black","white"]`, a
    `Vaxis::init(Options::default())` with `caps.unicode = Method::Unicode`, and
    `Winsize{rows:24,cols:120,..}` into a `Vec<u8>`; assert the emitted bytes
    **contain** each color name (`b"black"`) and its `#rrggbb` hex, and end after
    a `vx.pretty_print` epilogue (`ctlseqs::sync_reset` tail, as C4a asserts).
  - `render_keybinds`: build a `Config::default_config()` (or a tiny hand-built
    `Keybinds` with one leaf + one leader chord + one table) and assert the
    output contains an expected action tag and, for a chord, the `  >  ` spacer /
    for a `foo:bar` action the argument split. Also unit-test `ChordBinding::cmp`
    ordering (defaults-before-tables, longer-chord-first, mod-count) and
    `iterate_bindings` chord flattening (leader prepend) directly on
    `Vec<ChordBinding>` — pure, no Window needed beyond `gwidth` (a real
    `win` from a resized `Vaxis` over a synthetic winsize).

  These render-seam tests are pure over a `Vec<u8>` → run under **Miri**.

- `check_test_counts.py`: the two files currently declare **0 tests**; the fill
  adds a small, documented count (device-ignored smokes + render/cmp/flatten
  unit tests). Update the expected counts accordingly.

---

## 5. Fidelity checklist (R1)

- [ ] Every reference line/comment/`test{}` of both `prettyPrint` bodies +
      `ChordBinding`/`TriggerNode`/`iterateBindings` ported; the preserved
      verbatim comment blocks REPLACED by the live port with per-line PORT NOTEs
      where the mapping is non-obvious (defers→LIFO cleanup, SinglyLinkedList→Vec,
      `{t}`/`{u}`/`{f}` formats, `@intFromEnum`→`as c_int`).
- [ ] All segment text is `&[u8]` (never lossy `String`); `Style`/`Color`/
      `Segment` literals field-for-field.
- [ ] `--plain` / non-tty paths unchanged.
- [ ] `From<TtyError> for Error` added cli-locally; no dep beyond `rustty-vaxis`.
- [ ] Gates: build / test / clippy `-D` / fmt / acyclic / test-counts /
      comment-coverage / Miri.

---

## 6. Disposition

**ready** — no missing vaxis API, no open verification items. Every reference
`prettyPrint` call maps 1:1 to a landed Rust analogue (§1); the keybind helper
types + iterators all exist (`Set::iter_bindings`, `keybinds.tables.iter`,
`Key::tag_name`, `Mods::int`, `Action::format` + `AllocatingWriter`). Implement
per §2–§3, test per §4.
