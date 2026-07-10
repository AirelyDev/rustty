# VAXIS Chunk 3a — the ANSI INPUT PARSER (the pure half of the backend) — DESIGN

**Crate:** existing `crates/rustty-vaxis` (Chunks 1a value types, 1b uucode/gwidth/
unicode, 2 rendering — all landed, `lib.rs` PORT STATUS `done`). This chunk adds the
**input-parsing layer** as three new modules — `event.rs`, `ctlseqs.rs`, `parser.rs` —
wired into `lib.rs`, plus three tiny **derive/helper additions** to already-landed
value types (`key.rs`, `mouse.rs`, `cell.rs`) that the parser + its tests require.

**Source (read-only):**
`/Users/gael/.cache/zig/p/vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS/src/`
— `Parser.zig` (1285 LOC, **34 `test{}`**), `ctlseqs.zig` (151 LOC, 0 test),
`event.zig` (30 LOC, 0 test).

**Scope (R1 faithful):** the ground/escape/csi/osc/dcs/sos/pm/apc/ss2/ss3 state machine;
the Kitty-keyboard `u21`-codepoint + modifier decode; SGR + legacy xterm mouse
button/coord parsing; bracketed paste (`paste_start`/`paste_end`) + OSC-52 clipboard
paste; the legacy-key / tilde-key / SS3 tables; XTWINOPS in-band resize; DECRPM / DSR /
primary-DA / kitty-multi-cursor capability probes; color-report (OSC 4/10/11/12) +
color-scheme (DSR 997) reports; the grapheme-text path via the c1b `uucode`
utf8/grapheme iterators; the `ctlseqs` control-sequence constants; the `Event` union.

---

## DISPOSITION: **READY**

Every dependency is present and intra-crate (`cell`, `key`, `mouse`, `uucode`,
`unicode`, `Winsize`). The whole parser is **pure** — it maps `&[u8] → (Option<Event>,
n)` with no `Tty`/`Loop`/`queue`/`Vaxis` coupling. **There are ZERO chunk-3b R5 seams
inside these three files** (the parser is precisely "the pure half"; the tty/Loop reader
that *calls* `parse` in a loop is chunk 3b and out of scope). The only structural
decisions are (a) the **event/text byte-slice ownership + lifetime threading** (§2, the
KEY DECISION), (b) dropping the Zig **allocator param + error-union** where Rust `Vec`
is infallible (§2.4), and (c) a handful of **std-shim ports** (`splitScalar`→`[u8]::split`,
`indexOfScalarPos`, `parseUnsigned`, `utf8Encode`, standard-base64 decode — §5). All are
resolvable with the crate's established discipline. Nothing is blocked. Proceed.

---

## 1. Per-file mapping (Zig → rustty module)

| Zig source | rustty module | new public items | lifetime |
|---|---|---|---|
| `event.zig` | `event.rs` | `Event<'a>` (+ `pub use` of `Key`/`Mouse`/`Color`/`Winsize`) | `'a` (KeyPress/KeyRelease borrow text) |
| `ctlseqs.zig` | `ctlseqs.rs` | ~60 `pub const … : &[u8]` | `'static` |
| `Parser.zig` | `parser.rs` | `Parser`, `ParseResult<'a>`, `ParseError`; free `parse` + arm fns | see §2 |

`lib.rs`: add `pub mod event; pub mod ctlseqs; pub mod parser;` and extend the CHUNK
banner / PORT STATUS to note the input-parsing layer landed.

**Touch-ups to landed value types (§4):** add `PartialEq` to `Event`'s payload types so
the 34 tests can `assert_eq!` whole events (Zig `testing.expectEqual`/`expectEqualDeep`
structural compare): `PartialEq` on `key::Key`/`key::Modifiers`, `mouse::Mouse`,
`cell::Report`/`cell::Kind`; and `key::Modifiers::from_u8` (the inverse of the existing
`to_u8`, for the `@bitCast(u8)→Modifiers` decode); and `mouse::Button::from_u8` (for
`@enumFromInt`). All are additive, non-invasive.

---

## 2. THE KEY DECISION — event payload ownership + the `parse` lifetime

The load-bearing fidelity question: **where does each event's bytes live, and how does
that thread through Rust lifetimes?** Three distinct ownership classes, exactly mirroring
Zig:

### 2.1 `Key.text` borrows — TWO possible backings

`Key.text: ?[]const u8` (grapheme bytes) points at one of two buffers depending on the arm:

- **Ground path** (`parseGround`): `key.text = input[0..n]` — borrows the **input buffer**.
- **CSI paths** (legacy `{ABCDEFHPQS}` / tilde `~` / kitty `u`, field3 + the disambiguate
  block): `key.text = text_buf[0..total]` where `text_buf = &self.buf` — borrows the
  **Parser's internal `buf: [128]u8`** (the "internal ring buffer to temporarily store
  text" documented on `Key.text`).

So a returned `KeyPress`/`KeyRelease` event borrows **either `input` or `self.buf`**.

### 2.2 `Paste` is OWNED

`event.zig`: `paste: []const u8, // osc 52 paste, caller must free` — allocated in
`parseOsc` via `paste_allocator.?.alloc` (base64-decoded). This is an **owned** allocation.
→ Rust **`Paste(Vec<u8>)`** (owns the decoded bytes; drops itself — no "caller must free").

### 2.3 Everything else is plain data

`Mouse`, `Winsize`, `ColorReport(Report)`, `ColorScheme(Scheme)`, `FocusIn/Out`,
`Paste{Start,End}`, `MouseLeave`, and the eight `Cap*` — all `Copy`/owned, no borrow.

### 2.4 The `parse` signature (chosen)

```rust
pub struct ParseResult<'a> {          // Zig `Result { event: ?Event, n: usize }`
    pub event: Option<Event<'a>>,     //   renamed `Result`→`ParseResult` to avoid
    pub n: usize,                     //   shadowing std::result::Result (PORT-NOTE)
}

pub fn parse<'a>(&'a mut self, input: &'a [u8]) -> Result<ParseResult<'a>, ParseError>
```

**Why a single `'a` on `&'a mut self` AND `input`:** the returned `Key.text` may borrow
`self.buf` (CSI) *or* `input` (ground). Unifying both to one `'a` makes the result borrow
whichever, and Rust picks the shorter of the two actual lifetimes at the call site — both
the parser and `input` outlive the result in every test. Holding the result keeps `self`
mutably borrowed, so you cannot re-`parse` until the previous event is dropped. This is the
**faithful-and-safe** reading of Zig's ring-buffer caveat (Zig lets a second `parse`
silently clobber `buf` under a still-live prior slice; Rust forbids it — a strictly sound
tightening, the same pattern as `internal_screen::read_cell` returning a `Cell<'_>` tied to
`&self`).

**Drop the `paste_allocator` param + the error-union's OOM arm** (crate precedent —
`Screen::init`/`InternalScreen::init` dropped their allocator; `Vec` self-allocates,
infallible). PORT-NOTE prominently. The tests lose the `alloc` arg and the `defer
alloc.free(text)` (the `Vec` drops itself).

**Arm-fn signatures** (Zig `inline fn`; `!Result` only where it can error):

| Zig | Rust |
|---|---|
| `parseGround(input) !Result` | `fn parse_ground(input: &[u8]) -> Result<ParseResult<'_>, ParseError>` |
| `parseSs3(input) Result` | `fn parse_ss3(input: &[u8]) -> ParseResult<'static>` |
| `parseApc(input) Result` | `fn parse_apc(input: &[u8]) -> ParseResult<'static>` |
| `skipUntilST(input) Result` | `fn skip_until_st(input: &[u8]) -> ParseResult<'static>` |
| `parseOsc(input, alloc) !Result` | `fn parse_osc(input: &[u8]) -> Result<ParseResult<'static>, ParseError>` (Paste owned) |
| `parseCsi(input, text_buf) Result` | `fn parse_csi<'b>(input: &[u8], text_buf: &'b mut [u8]) -> ParseResult<'b>` |
| `parseMouse(input, full_input) Result` | `fn parse_mouse(input: &[u8], full_input: &[u8]) -> ParseResult<'static>` |

`parse_csi`'s only output-borrow is `Key.text` into `text_buf`; `input` is an independent
(shorter-OK) lifetime. Returning `&'b [u8]` derived from the `&'b mut [u8]` is a standard
mutable-reborrow-then-share (`fn f<'b>(x: &'b mut [u8]) -> &'b [u8] { x[0]=1; &x[..1] }`
compiles). `parse` calls `parse_csi(input, &mut self.buf)` → both `'b` and the ground
`input` borrow unify to `'a`. Top-level dispatch: `return parse_osc(input)` /
`parse_ground(input)` (already `Result`); `return Ok(parse_ss3(input))` /
`Ok(skip_until_st(input))` / `Ok(parse_apc(input))` / `Ok(parse_csi(input, &mut self.buf))`
(Zig coerces the non-error `Result` into the `!Result`).

### 2.5 `ParseError`

Zig `parse` error set = `error.InvalidUTF8` (parseGround) ∪ `Color.rgbFromSpec` errors
(`cell::ColorSpecError`) ∪ base64 `Decoder` errors ∪ `Allocator.Error` (dropped). →

```rust
pub enum ParseError {
    InvalidUtf8,                 // parseGround
    Color(cell::ColorSpecError), // rgbFromSpec (OSC 4/10/11/12)
    Base64,                      // std.base64.standard.Decoder failure (OSC 52)
}
```

**No test exercises `Err`** (all 34 assert `Ok`), so `ParseError` need only compile +
be faithful. `#[derive(Debug, Clone, PartialEq, Eq)]`.

---

## 3. The `Event` union (event.rs)

`event.zig` re-exports `Key`/`Mouse`/`Color`/`Winsize` then declares the union. Port:

```rust
pub use crate::cell::Color;        // Zig `pub const Color = @import("Cell.zig").Color;`
pub use crate::key::Key;
pub use crate::mouse::Mouse;
pub use crate::Winsize;

/// The events that Vaxis emits internally
#[derive(Clone, Debug, PartialEq)]   // NOT Copy (Paste owns a Vec)
pub enum Event<'a> {
    KeyPress(Key<'a>),
    KeyRelease(Key<'a>),
    Mouse(Mouse),
    MouseLeave,
    FocusIn,
    FocusOut,
    PasteStart,            // bracketed paste start
    PasteEnd,              // bracketed paste end
    Paste(Vec<u8>),        // osc 52 paste, owned (was `[]const u8`, "caller must free")
    ColorReport(cell::Report), // osc 4, 10, 11, 12 response
    ColorScheme(cell::Scheme),
    Winsize(Winsize),
    // these are delivered as discovered terminal capabilities
    CapKittyKeyboard,
    CapKittyGraphics,
    CapRgb,
    CapSgrPixels,
    CapUnicode,
    CapDa1,
    CapColorSchemeUpdates,
    CapMultiCursor,
}
```

Preserve the `bracketed paste start`/`end`, `osc 52 paste, caller must free`, `osc 4, 10,
11, 12 response`, and `these are delivered as discovered terminal capabilities` comments.
`cap_rgb` has no producer in the parser (present in the union; kept for parity — it is set
elsewhere in vaxis). Variant naming: Zig snake `key_press` → Rust `KeyPress` (enum-variant
CamelCase); note the mapping in a module comment.

---

## 4. Additive touch-ups to landed value types (§1)

All required by the parser body or the event-equality tests; each is a minimal,
non-invasive addition. **PORT-NOTE each** at its site.

1. **`key::Modifiers::from_u8(bits: u8) -> Modifiers`** — inverse of the existing `to_u8`
   (LSB-first: shift0 alt1 ctrl2 super3 hyper4 meta5 caps6 num7). Implements Zig
   `@bitCast(u8) -> Modifiers` used at three CSI sites: `key.mods = @bitCast(modifier_mask
   -| 1)` (`modifier_mask.saturating_sub(1)`).
2. **`#[derive(PartialEq, Eq)]` on `key::Modifiers`** — field-by-field bool compare ≡ the
   existing bitcast `eql`; needed to derive `PartialEq` on `Key`.
3. **`#[derive(PartialEq)]` on `key::Key`** — compares `codepoint`/`shifted`/`base_layout`/
   `mods` by value and `text: Option<&[u8]>` by **byte-slice equality** (what
   `expectEqualDeep` does; for single-byte literals it also matches Zig's `expectEqual`).
   PORT-NOTE: Rust byte-equality is the robust faithful semantic (Zig's `expectEqual` on a
   slice is pointer-identity that happens to pass via literal interning).
4. **`#[derive(PartialEq)]` on `mouse::Mouse`** and **`cell::Report`/`cell::Kind`** — for
   the mouse tests (32/33) and to let `Event` derive `PartialEq`. (`mouse::Button`/`Type`/
   `Modifiers`, `cell::Scheme`, `Winsize` already derive it.)
5. **`mouse::Button::from_u8(v: u8) -> Button`** — the `@enumFromInt(button_mask &
   mouse_bits.buttons)` decode. Match the 12 discriminants (0,1,2,3,64,65,66,67,128..131);
   `_ => unreachable!()` — **faithful to Zig's checked `@enumFromInt` illegal-behavior**
   (a masked value with both bits 6+7, i.e. 192..195, is an invalid button; Zig safe-mode
   panics there, ReleaseFast is UB). PORT-NOTE: provably not hit by the corpus (SGR button
   masks never set both wheel bits); the panic mirrors Zig safe-mode, not a new behavior.

---

## 5. std-shim ports (parser-local, §"the pure primitives")

The parser leans on a cluster of `std` functions. Map each faithfully; keep them
**private to `parser.rs`** (self-contained; `cell.rs`'s `split_scalar`/`parse_unsigned_u8`
are private there — do not couple).

- **`std.mem.splitScalar(u8, buf, delim)`** → **Rust std `<[u8]>::split(|&b| b == delim)`**
  directly (no port needed). Semantics match exactly: N delimiters → N+1 pieces incl.
  trailing empty; `.next()` never-`None`-first. Zig `iter.first()` (XTWINOPS `t` arm) ≡ the
  first `.next()`. Used for the `;`-field split and the `:`-param split throughout CSI.
- **`std.mem.indexOfScalarPos(u8, s, start, needle)`** → `fn index_of_scalar_pos(s, start,
  needle) -> Option<usize>` = `s[start..].iter().position(|&b| b==needle).map(|o| start+o)`.
- **`std.fmt.parseUnsigned(T, buf, 10)`** → a faithful base-10 digit-only parser (rejects
  non-digit, rejects overflow → `None`; **no `+`/`-` sign** unlike Rust's `str::parse`).
  Recommend one generic `fn parse_unsigned<T>(buf, base) -> Option<T>` (macro- or
  trait-backed for u8/u16/u32) mirroring `cell.rs`'s `parse_unsigned_u8` pattern
  (`checked_mul`/`checked_add`). **`u21` → `u32` bounded to `0x1F_FFFF`** (Zig
  `parseUnsigned(u21,…)` overflows past 2097151 → `None`).
- **`parseParam(comptime T, buf, default) ?T`** (the parser's own helper) → `fn
  parse_param_u8/u16/u21(buf, default: Option<T>) -> Option<T>`: `if buf.is_empty() {
  default } else { parse_unsigned(buf, 10) }`. Preserve the doc `Parse a param buffer,
  returning a default value if the param was empty`.
- **`std.unicode.utf8Encode(cp: u21, buf) !usize`** → `fn utf8_encode(cp: u32, buf: &mut
  [u8]) -> Option<usize>`: `char::from_u32(cp)?.encode_utf8(buf).len()` (`from_u32` returns
  `None` for surrogates + `>0x10FFFF`, exactly Zig's error). The `catch return null_event`
  sites → `?`/`orelse null_event`; the disambiguate-block `catch unreachable` → `.unwrap()`
  (upper is ASCII, always valid). Same encode pattern already used in `key.rs:215`.
- **`std.unicode.utf8CodepointSequenceLength(cp) !usize`** (ground path) → `fn
  utf8_codepoint_sequence_length_checked(cp: u32) -> Option<usize>` returning `None` for
  `cp>0x10FFFF` (the **erroring** variant — distinct from `unicode.rs`'s private `catch 1`
  fallback version; the parser needs `catch return error.InvalidUTF8`).
- **`std.base64.standard.Decoder`** (OSC 52) → `fn base64_standard_decode(payload: &[u8])
  -> Result<Vec<u8>, ParseError>`: a ~40-line faithful port of the standard alphabet
  (`A–Za–z0–9+/`, `=` pad), `calcSizeForSlice` (4→3 groups, minus padding) + `decode`
  (invalid char/length → `Err(ParseError::Base64)`). Pins test 12:
  `"b3NjNTIgcGFzdGU="` (16 chars, 1 pad) → 11 bytes `"osc52 paste"`.
- **`std.ascii.isPrint` / `toUpper`** (disambiguate block) → `matches!(b, 0x20..=0x7E)`
  (isPrint **includes space** 0x20 — load-bearing for test 34 shift+space) and
  `b.to_ascii_uppercase()`.
- **`std.mem.eql(u8, a, b)`** → `a == b`. **`std.mem.eql(u8, event_type_buf, "3")`**
  (release detection) → `event_type_buf == b"3"`.
- **`log.warn`/`log.debug`** (ss3-unhandled, decoded-paste) → **dropped** (crate precedent),
  PORT-NOTE. `const log = std.log.scoped(.vaxis_parser)` dropped.
- **`std.debug.assert`** → `debug_assert!` (parse `input.len>0`; parseGround escape-arm
  `input.len==1`; the `c`/`n` `sequence.len>=` asserts).

---

## 6. The parser state machine (parser.rs) — arm-by-arm fidelity

**Struct + consts.** `Parser { buf: [u8; 128] }` (Zig `buf: [128]u8 = undefined`; manual
`impl Default` → `[0u8; 128]`, PORT-NOTE `undefined`→zeroed, never read before written).
Preserve the `a buffer to temporarily store text in…encode text-as-codepoints` comment.
`mouse_bits` → a private const group (`const MOTION: u8 = 0b0010_0000; BUTTONS: u8 =
0b1100_0011; SHIFT: u8 = 0b0000_0100; ALT: u8 = 0b0000_1000; CTRL: u8 = 0b0001_0000;
LEAVE: u16 = 0b1_0000_0000;`). The `State` enum (ground/escape/csi/osc/dcs/sos/pm/apc/
ss2/ss3) is declared in Zig but **only used as documentation** (the dispatch is a direct
`input[1]` switch, no stored state) — port it as a `#[allow(dead_code)] enum State {…}`
with its `// the state of the parser` comment, for parity.

**`parse`** (§2.4 signature). `debug_assert!(!input.is_empty())`. Preserve the doc-block
(`Parse the first event…If a completion event is not present…If an unknown event is
found…`). Gate `if input[0]==0x1b && input.len>1` then `match input[1]`:
`0x4F=>parse_ss3` · `0x50=>skip_until_st // DCS` · `0x58=>skip_until_st // SOS` ·
`0x5B=>parse_csi(input, &mut self.buf) // CSI` · `0x5D=>parse_osc(input) // OSC` ·
`0x5E=>skip_until_st // PM` · `0x5F=>parse_apc // APC` · `else` → the `alt + <char>`
key (`Key { codepoint: input[1] as u32, mods: Modifiers{alt:true, ..default}, .. }`,
`n=2`) — preserve `// Anything else is an "alt + <char>" keypress`. Else → `parse_ground`.

**`parse_ground`** — `debug_assert!(!input.is_empty())`; preserve the `ground state
generates keypresses…map these to lowercase ascii…` comment. The `match b`:
- `0x00 => @ + ctrl`; `0x08 => backspace`; `0x09 => tab`; `0x0A => j + ctrl`; `0x0D =>
  enter`; `0x01..=0x07 | 0x0B..=0x0C | 0x0E..=0x1A => (b+0x60) + ctrl`; `0x1B => escape`
  (with `debug_assert!(input.len()==1) // parseGround expects len == 1 with 0x1b`);
  `0x7F => backspace`.
- `else` = the **grapheme block** (§ ground-grapheme, below).
Return `ParseResult { event: Some(KeyPress(key)), n }`. Note the outer `n` mutates only in
the grapheme block; ASCII arms keep `n=1`.

**Ground-grapheme block** (the c1b `uucode` path — fidelity core):
```
let mut it = uucode::utf8::Iterator::init(input);
let first_cp = it.next().ok_or(ParseError::InvalidUtf8)?;   // Some(0xFFFD) on reject; None only if empty
let mut n = utf8_codepoint_sequence_length_checked(first_cp).ok_or(ParseError::InvalidUtf8)?;
let mut code = first_cp;
let mut g = uucode::grapheme::Iterator::init(uucode::utf8::Iterator::init(input));
let (mut grapheme_len, mut cp_count) = (0usize, 0usize);
while let Some(res) = g.next() {
    cp_count += 1;
    if res.is_break { grapheme_len = g.i; break; }   // first grapheme boundary; g.i is the byte offset
}
if grapheme_len > 0 { n = grapheme_len; if cp_count > 1 { code = key::multicodepoint; } }
key = Key { codepoint: code, text: Some(&input[0..n]), .. };
```
Preserve `// return null if we don't have a valid codepoint`, `// Check if we have a
multi-codepoint grapheme`, `// Found the first grapheme boundary`. Uses the **raw uucode
iterators** (accessing `.i`/`.is_break`), NOT the `unicode.rs` Old-API wrapper. This drives
tests 19–27 (single cp, emoji, ZWJ astronaut, flag, combining mark, skin-tone, VS, keycap).

**`parse_ss3`** — `input.len<3` → `{None,0}`; `match input[2]`: `0x1B => {None, 2}`;
`A/B/C/D => up/down/right/left`; `E => kp_begin`; `F => end`; `H => home`;
`P/Q/R/S => f1/f2/f3/f4`; `else` → drop `log.warn("unhandled ss3…")`, `{None, 3}`.
Return `{Some(KeyPress(key)), 3}`. (Tests 7-alt-keys.)

**`parse_apc`** — `input.len<3`→`{None,0}`; `end = index_of_scalar_pos(input, 2, 0x1b)`
else `{None,0}`; `sequence = &input[0..end+1+1]`; `match input[2]`: `'G' =>
{Some(CapKittyGraphics), seq.len}`; `else => {None, seq.len}`.

**`skip_until_st`** — preserve `/// Skips sequences until we see an ST (String Terminator,
ESC \)`. `input.len<3`→`{None,0}`; `end=index_of_scalar_pos(input,2,0x1b)` else `{None,0}`;
`if input.len < end+1+1 { {None,0} }`; `seq=&input[0..end+1+1]`; `{None, seq.len}`.

**`parse_osc`** — preserve `/// Parses an OSC sequence` + the `end is the index of the
terminating byte(s)…` comment. `input.len<3`→`{None,0}`. Compute `end`/`bel_terminated`:
try `skip_until_st(input)`; if its `.n>0` use it, else find BEL `index_of_scalar_pos(input,
2, 0x07)` else `{None,0}` and set `bel_terminated=true`, `end=bel+1`. `sequence=&input[0..
end]`; `null_event = {None, seq.len}`. `semicolon_idx = index_of_scalar_pos(input,2,';')`
else null_event; `ps = parse_unsigned::<u8>(&input[2..semicolon_idx], 10)` else null_event.
`match ps`:
- `4` → second `;` → `ps_idx = parse_unsigned::<u8>` → `color_spec` slice (bel: `..seq.len-1`
  else `..seq.len-2`) → `Color::rgb_from_spec(color_spec).map_err(ParseError::Color)?` →
  `ColorReport(Report{ kind: Kind::Index(ps_idx), value: color.rgb })`. **Extract
  `color.rgb`**: `rgb_from_spec` returns `Color::Rgb([u8;3])` — pattern-match to the array
  (Zig `color.rgb`).
- `10|11|12` → `color_spec` (same bel branch) → `rgb_from_spec` → `ColorReport(Report{
  kind: 10=>Fg,11=>Bg,12=>Cursor, value })`.
- `52` → `if input[semicolon_idx+1] != b'c' { null_event }`; `payload` = `&input[
  semicolon_idx+3 .. seq.len-1|-2]`; `base64_standard_decode(payload)?` → `Paste(Vec<u8>)`;
  drop `log.debug("decoded paste…")`. **(Test 12.)**
- `else` → null_event.
Every branch `n = seq.len`.

**`parse_csi`** — preserve `// We start iterating at index 2 to get past the '['`.
`input.len<3`→`{None,0}`. Find the final byte: iterate `input[2..]` with running index,
`0x40..=0xFF => break sequence=&input[0..i+1]`, else continue; the `for…else` fall-through →
`{None, 0}`. `null_event = {None, seq.len}`; `final = seq[seq.len-1]`. `match final`:

- **`A|B|C|D|E|F|H|P|Q|R|S`** — legacy keys. Preserve the `// Legacy keys` / `CSI
  {ABCDEFHPQS}` / `CSI 1 ; modifier:event_type {…}` comments. `<[u8]>::split(&seq[2..
  seq.len-1], b';')`, **skip field 1** (`_ = fields.next()`). `codepoint` per `final` (up/
  down/right/left/kp_begin/end/home/f1..f4; `else => null_event`). **field2** (`modifier_mask:
  event_type`): `split(field_buf, b':')`; `modifier_mask = parse_param_u8(mod_buf, Some(1))`
  else null_event; `key.mods = Modifiers::from_u8(modifier_mask.saturating_sub(1))`; if a
  `:` param, `is_release = (event_type_buf == b"3")`. **field3** (`text_as_codepoint[:…]`):
  split on `:`; loop cps → `parse_param_u21` → `utf8_encode` into `text_buf[total..]`;
  `key.text = Some(&text_buf[0..total])`. Event = `is_release ? KeyRelease : KeyPress`.
- **`~`** — tilde keys. Preserve `// Legacy keys` / `CSI number ~` / `…; modifier ~` /
  `…; modifier:event_type ; text_as_codepoint ~`. field1 `number = parse_param_u16`;
  `match number`: 2=insert 3=delete 5=page_up 6=page_down 7=home 8=end 11..24=f1..f12
  (11→f1,12→f2,13→f3,14→f4,15→f5,17→f6,18→f7,19→f8,20→f9,21→f10,23→f11,24→f12),
  **`200 => return {PasteStart, seq.len}`**, **`201 => {PasteEnd, seq.len}`**,
  `57427 => kp_begin`, `else => null_event`. Then the SAME field2 (modifier/is_release) +
  field3 (text) as legacy. **(Tests 9 insert, 10 paste_start, 11 paste_end.)**
- **`I` => `{FocusIn, seq.len}`** · **`O` => `{FocusOut, seq.len}`** (tests 13/14).
- **`M|m` => `parse_mouse(sequence, input)`** (tests 32/33).
- **`c`** — Primary DA. `debug_assert!(seq.len>=4) // ESC [ ? c == 4 bytes`; `input[2]=='?'
  => {CapDa1, seq.len}` else null_event. Preserve `// Primary DA (CSI ? Pm c)`. (Test 30.)
- **`n`** — DSR. `debug_assert!(seq.len>=3)`; `match seq[2]`: `'?'` → `delim =
  index_of_scalar_pos(input,3,';')` else null; `ps=parse_unsigned::<u16>(&input[3..delim])`;
  `match ps { 997 => match seq[delim+1] { '1'=>ColorScheme(Scheme::Dark) '2'=>
  ColorScheme(Scheme::Light) _=>null }, _=>null }`; `else=>null`. Preserve the `// Device
  Status Report` / `CSI Ps n` / `CSI ? Ps n` / `Color scheme update (CSI 997 ; Ps n)` +
  the contour-terminal URL. **(Test 31, incl. the `\x1b[0n`→null sub-case.)**
- **`t`** — XTWINOPS. `split(&seq[2..seq.len-1], b';')`; `ps = iter.next().unwrap()` (Zig
  `.first()`); `if ps == b"48"` → in-band resize: `height_char=next?`, `width_char=next?`,
  `height_pix=next.unwrap_or(b"0")`, `width_pix=next.unwrap_or(b"0")`; build `Winsize {
  rows: parse::<u16>(height_char)?, cols: parse::<u16>(width_char)?, x_pixel:
  parse::<u16>(width_pix)?, y_pixel: parse::<u16>(height_pix)? }` → `{Winsize(…), seq.len}`;
  else null. Preserve `// XTWINOPS` / `// in band window resize` / `CSI 48 ; height ; width
  ; height_pix ; width_pix t`. **Note the x/y-pixel swap** (`x_pixel←width_pix`,
  `y_pixel←height_pix`) — faithful.
- **`u`** — Kitty keyboard. Preserve the 3-line `CSI unicode-key-code:…` doc + `Not all
  fields…Only unicode-key-code is mandatory`. `if seq.len>2 && seq[2]==b'?' => {
  CapKittyKeyboard, seq.len}`. Else build `key = Key { codepoint: 0 /* Zig undefined,
  overwritten in field1 */, text:None, shifted:None, base_layout:None, mods:default }`.
  **field1** (`unicode-key-code:shifted:base_layout`): split `:`; `key.codepoint =
  parse_param_u21(cp_buf, None)` else null; optional `shifted_codepoint = parse_param_u21`;
  optional `base_layout_codepoint = parse_param_u21`. **field2/field3** = same as legacy.
  Then the **disambiguate block** (preserve `// We check if we have *only* shift, no text…
  shift + space can produce CSI 32 ; 2 u`): `mod_test = Modifiers{ shift:true, caps_lock:
  key.mods.caps_lock, num_lock: key.mods.num_lock, ..default }`; `if key.text.is_none() &&
  Modifiers::eql(key.mods, mod_test) && key.codepoint <= u8::MAX as u32 && (0x20..=0x7E)
  .contains(&(key.codepoint as u8))` → `upper = (key.codepoint as u8).to_ascii_uppercase();
  n = utf8_encode(upper as u32, &mut text_buf).unwrap(); key.text = Some(&text_buf[0..n]);
  key.shifted_codepoint = Some(upper as u32)`. Preserve `// Encode the codepoint as upper`.
  Event = `is_release ? KeyRelease : KeyPress`. **(Tests 15/16/17/18 kitty, 34 disambiguate.)**
  ⚠ Borrow note: the disambiguate `text_buf[0..n]` and field3 `text_buf[0..total]` both
  borrow `text_buf` — mutually exclusive arms, single `&'b` return. Fine.
- **`y`** — DECRPM. Preserve `// DECRPM (CSI ? Ps ; Pm $ y)`. `delim=index_of_scalar_pos(
  input,3,';')`; `ps=parse::<u16>(&input[3..delim])`; `pm=parse::<u8>(&input[delim+1..
  seq.len-2])`; `match ps { 1016 => match pm {0|4=>null, _=>CapSgrPixels}, 2027 => {0|4=>
  null,_=>CapUnicode}, 2031 => {0|4=>null,_=>CapColorSchemeUpdates}, _=>null }`. Preserve
  the `Mouse Pixel reporting` / `Unicode Core` / `Color scheme reportnig` (sic) comments +
  both contour URLs. **(Test 29, incl. `…;0$y`→null.)**
- **`q`** — kitty multi-cursor. Preserve `// kitty multi cursor cap (CSI > 1;2;… TRAILER)
  (TRAILER is " q")` + `// check for any digits…`. `second_final = seq[seq.len-2]`; `if
  second_final != b' ' { null }`; scan `seq[0..seq.len-2]`: `b'0'..=b'9' => return {
  CapMultiCursor, seq.len}`, else continue; fall-through → null. **(Test 28, incl. `\x1b[>
  q`→null.)**
- **`else` => null_event**.

**`parse_mouse`** — preserve `/// Parse a mouse event`. `null_event = {None, input.len}`.
Two forms:
- **xterm** (`input.len==3 && input[2]==b'M' && full_input.len>=6`): `xterm=true`;
  `button_mask = full_input[3]-32`, `px=full_input[4]-32`, `py=full_input[5]-32` (as u16).
- **SGR** (`input.len>=4 && input[2]==b'<'`): `xterm=false`; `delim1=index_of_scalar_pos(
  input,3,';')`; `button_mask=parse_param_u16(&input[3..delim1],None)`; `delim2=…(delim1+1,
  ';')`; `px=parse_param_u16(&input[delim1+1..delim2], Some(1))`; `py=parse_param_u16(
  &input[delim2+1..input.len-1], Some(1))`.
- else → null_event.
Then: `if button_mask & LEAVE > 0 { return {MouseLeave, if xterm {6} else {input.len}} }`.
`button = Button::from_u8((button_mask & BUTTONS) as u8)`; `motion = button_mask&MOTION>0`;
`shift/alt/ctrl` from their bits. `Mouse { col: px.saturating_sub(1), row: py.saturating_sub(1),
xoffset: 0, yoffset: 0` (**Zig field defaults — set explicitly, Mouse has no `Default`**),
`button, mods: mouse::Modifiers{shift,alt,ctrl}, r#type:` — the `type` blk: `motion &&
button!=None => Drag`; `motion && button==None => Motion`; `xterm => (button==None ?
Release : Press)`; `input[input.len-1]==b'm' => Release`; else `Press`. `n = if xterm {6}
else {input.len}`. **(Test 32: SGR `<35;1;1m` → col0 row0 button none type Motion; Test
33: xterm `M\x20\x21\x21` → button left type Press.)**

---

## 7. `ctlseqs.rs` — the control-sequence constants

~60 `pub const` string literals → **`pub const NAME: &[u8] = b"…";`** (byte-slices, never
`String` — they are raw control bytes; task R1). Keep the exact Zig snake_case identifiers.
`\x1b` → `\x1b`, ST `\x1b\\` preserved. **The format-template constants** (`{d}`, `{s}`,
`{f}`, `{x:0>2}` — e.g. `cup = "\x1b[{d};{d}H"`, `osc10_set`, `explicit_width`,
`scaled_text{,_with_fractions}`, `fg_rgb`, …) are **preserved verbatim as literal bytes**
(the `{…}` are Zig `std.fmt` placeholders the *later* tty/renderer chunks fill; that
formatting is out of 3a scope). PORT-NOTE: 3a lands them as byte constants for fidelity;
the format-application seam is a later chunk. Preserve every section comment (`// Queries`,
the mouse-set block incl. the zellij issue URL, `// in-band window size reports`, `// sync`,
`// unicode`, `// text sizing`, `// bracketed paste`, `// color scheme updates`, `// Key
encoding`, `// Cursor`, `// Erase`, `// alt screen`, `// sgr reset all`, `// colors`,
`// Underlines` incl. the `ul_off` NOTE, `// Attributes`, `// OSC sequences`, `// Kitty
graphics`, `// Color control sequences` incl. the per-const `// fg`/`// bg`/`// cursor
color`/`// color index {d}`/`// this resets _all_ color indexes`/`// reset … to terminal
default` trailers). The named ones the memory index calls out (`unicode_set`/`unicode_reset`
= `b"\x1b[?2027h"`/`b"\x1b[?2027l"`) must be byte-exact.

---

## 8. The 34 `test{}` — assertion-for-assertion (the fidelity contract)

Each Zig `test` → a `#[test]` in `#[cfg(test)] mod tests` in `parser.rs`, exact input
bytes → exact event. The `alloc` arg is dropped (§2.4); `expectEqual`/`expectEqualDeep`/
`expectEqualStrings` → `assert_eq!` (on `Event`/fields via the §4 `PartialEq` derives).
A tiny local ctor eases `Key`/`Segment`-free construction. **Tests using free arm-fns**
(`parseCsi(input,&buf)`) call `parse_csi(input, &mut buf)` directly (same module).

| # | Zig test | input | asserts (n; event) |
|---|---|---|---|
| 1 | single xterm keypress | `"a"` | 1; KeyPress{cp:'a', text:b"a"} |
| 2 | …backspace | `"\x08"` | 1; KeyPress{cp:backspace} |
| 3 | …with more buffer | `"ab"` | 1; text=b"a" (expectEqualStrings + Deep) |
| 4 | xterm escape keypress | `"\x1b"` | 1; KeyPress{cp:escape} |
| 5 | xterm ctrl+a | `"\x01"` | 1; KeyPress{cp:'a', ctrl} |
| 6 | xterm alt+a | `"\x1ba"` | 2; KeyPress{cp:'a', alt} |
| 7 | xterm key up | `"\x1b[A"` **and** `"\x1bOA"` | 3; KeyPress{cp:up} (both sub-blocks) |
| 8 | xterm shift+up | `"\x1b[1;2A"` | 6; KeyPress{cp:up, shift} |
| 9 | xterm insert | `"\x1b[2~"` | 4; KeyPress{cp:insert} |
| 10 | paste_start | `"\x1b[200~"` | 6; PasteStart |
| 11 | paste_end | `"\x1b[201~"` | 6; PasteEnd |
| 12 | osc52 paste | `"\x1b]52;c;b3NjNTIgcGFzdGU=\x1b\\"` | 25; Paste == b"osc52 paste" |
| 13 | focus_in | `"\x1b[I"` | 3; FocusIn |
| 14 | focus_out | `"\x1b[O"` | 3; FocusOut |
| 15 | kitty shift+a no-text | `"\x1b[97:65;2u"` | 10; KeyPress{cp:'a', shifted:'A', shift, text:b"A"} (Deep) |
| 16 | kitty alt+shift+a no-text | `"\x1b[97:65;4u"` | 10; KeyPress{cp:'a', shifted:'A', shift+alt} |
| 17 | kitty a no-text | `"\x1b[97u"` | 5; KeyPress{cp:'a'} |
| 18 | kitty release | `"\x1b[97;1:3u"` | 9; **KeyRelease**{cp:'a'} |
| 19 | single codepoint | `"🙂"` | 4; KeyPress{cp:0x1F642, text:input} |
| 20 | single cp + more | `"🙂a"` | 4; text=b"🙂" (Deep) |
| 21 | multi-cp grapheme | `"👩‍🚀"` | input.len; KeyPress{cp:multicodepoint, text:input} |
| 22 | multi-cp grapheme + more | `"👩‍🚀abc"` | text.len; text=b"👩‍🚀", cp:multicodepoint |
| 23 | flag emoji | `"🇺🇸"` | input.len; {multicodepoint, input} |
| 24 | combining mark | `"a\u{0301}"` | input.len; {multicodepoint, input} |
| 25 | skin tone emoji | `"👋🏿"` | input.len; {multicodepoint, input} |
| 26 | text variation selector | `"❤︎"` | input.len; {multicodepoint, input} |
| 27 | keycap sequence | `"1️⃣"` | input.len; {multicodepoint, input} |
| 28 | csi kitty multi cursor | `"\x1b[>1;2;3;29;30;40;100;101 q"` **and** `"\x1b[> q"` | len; CapMultiCursor / **null** |
| 29 | csi decrpm | `"\x1b[?1016;1$y"` **and** `"\x1b[?1016;0$y"` | len; CapSgrPixels / **null** |
| 30 | csi primary da | `"\x1b[?c"` | len; CapDa1 |
| 31 | csi dsr | `"\x1b[?997;1n"`, `"\x1b[?997;2n"`, `"\x1b[0n"` | len; ColorScheme(Dark)/Light/**null** |
| 32 | csi mouse (SGR) | `"\x1b[<35;1;1m"` | len; Mouse{col0,row0,none,Motion,{}} |
| 33 | csi xterm mouse | `"\x1b[M\x20\x21\x21"` | len; Mouse{col0,row0,left,Press,{}} |
| 34 | disambiguate shift+space | `"\x1b[32;2u"` | 7; KeyPress{cp:' ', shifted:' ', shift, text:b" "} (Deep) |

**Total = 34** (7 and 28/29 have multiple sub-blocks in one `test`; 31 has three). The
grapheme tests (19–27) are the real c1b-uucode fidelity check — spot-verify 21 (ZWJ),
24 (combining), 23 (flag/RI-pair) land `multicodepoint` with `n==input.len`.

---

## 9. Deps / acyclic (R4) + gates

**No new crate deps.** `parser.rs` consumes `crate::{cell, key, mouse, uucode, Winsize}` +
`crate::event::Event`; `event.rs` consumes `crate::{cell, key, mouse, Winsize}`; `ctlseqs.rs`
is standalone. `rustty-vaxis` stays a dep-free leaf → `check_acyclic.sh` trivially green.

- **R1** — every line/comment/`test{}` faithful: the state-machine arms, the n-byte counts,
  the `-|`/saturating subs, the `undefined`→0/zeroed placeholders, the DSR/DECRPM URLs, the
  `reportnig` typo, the ctlseqs section comments + format templates, all 34 test asserts.
- **Byte-slices** — `Key.text`, `Paste`, all `ctlseqs` consts, event text → `&[u8]`/
  `Vec<u8>`, **never lossy `String`**.
- **R5** — **none new** (the parser is pure; the tty/Loop that drives it is chunk 3b).
- **R6** — PURE ⇒ **Miri primary**. No `unsafe` in the parser (the `&mut self`/`text_buf`
  lifetimes are safe references; the raw-ptr discipline of chunk 2 is not needed here). Run
  `cargo +nightly miri test -p rustty-vaxis` — it exercises all 34 (esp. the uucode
  grapheme table lookups + the base64 decode).
- **`check_test_counts.py` MAP update (REQUIRED):** add **`Parser.zig`** (and `event.zig`/
  `ctlseqs.zig`, 0-test, for completeness) to the `"vaxis"` zig list (line ~197). The rust
  glob `{RSVAXIS}/*.rs` auto-picks the 34 new `parser.rs` tests; adding `Parser.zig` (34)
  keeps `rust >= zig`.
- **`check_comment_coverage.py --crate rustty-vaxis`** — preserve every distinctive
  (≥5-token) comment enumerated in §6/§7 (the `parse` doc-block, the ground-state
  lowercase-map note, the field-format `modifier_mask:event_type` / `text_as_codepoint`
  comments, the disambiguate `shift + space can produce CSI 32 ; 2 u` note, the XTWINOPS /
  DECRPM / DSR / kitty-multi-cursor comments + URLs, the OSC terminating-byte note, the
  ctlseqs section headers + zellij/contour URLs).
- **Gates:** `cargo build --workspace` · `cargo test -p rustty-vaxis` · `cargo clippy -p
  rustty-vaxis --all-targets -- -D warnings` · `cargo fmt --check` ·
  `bash .port/tools/gates/check_acyclic.sh` · `python3 .port/tools/gates/check_test_counts.py`
  · `python3 .port/tools/gates/check_comment_coverage.py --crate rustty-vaxis` ·
  `cargo +nightly miri test -p rustty-vaxis`.

---

## 10. Fidelity quirks to preserve (port EXACTLY — each is real Zig behavior)

1. **`Result`→`ParseResult` rename** (avoids shadowing `std::result::Result`).
2. **`&'a mut self` + `input: &'a [u8]` unified lifetime** (§2.4) — the safe reading of the
   ring-buffer caveat; holding the result blocks re-parse.
3. **allocator param + OOM error-union dropped**; **`paste: Vec<u8>` owned** (no "caller
   must free").
4. **`log.warn`/`log.debug` dropped** (crate precedent).
5. **`Parser.buf = undefined`** → `[0u8; 128]`; **kitty `key.codepoint = undefined`** → `0`
   placeholder (overwritten in field1 before any read).
6. **`Mouse.xoffset/yoffset` Zig field-defaults `= 0`** set explicitly (Mouse has no
   `Default`).
7. **`Button::from_u8` `_ => unreachable!()`** — mirrors Zig checked `@enumFromInt`; not hit
   by the corpus.
8. **isPrint includes space** (`0x20..=0x7E`) — load-bearing for test 34.
9. **XTWINOPS x/y-pixel swap** (`x_pixel←width_pix`, `y_pixel←height_pix`) — faithful.
10. **`parseUnsigned` digit-only, no sign** (unlike Rust `str::parse`); **`u21`→`u32`
    bounded `0x1F_FFFF`**.
11. **`splitScalar`→ std `<[u8]>::split`** (semantics verified identical, incl. `.first()`
    ≡ first `.next()` and trailing-empty).
12. **the `State` enum is documentation-only** (dispatch is a direct `input[1]` switch) —
    kept `#[allow(dead_code)]` for parity.
13. **`match_shifted_codepoint`-style dead stores are NOT in the parser** — but the tilde
    arm's `57427 => kp_begin` and the legacy/tilde/kitty three-field shape must match byte
    counts exactly (n = seq.len for CSI; 2/3 for ss3; sequence-relative for osc/apc/skip).
14. **base64 `calcSizeForSlice` + `decode`** faithful to `std.base64.standard.Decoder`
    (padding-aware; test 12 = 11 bytes).

---

## 11. Bottom line

Chunk 3a is **READY** and dep-free. Port `event.zig`→`event.rs` (`Event<'a>`, `Paste`
owned `Vec<u8>`, key variants borrow text; `PartialEq` for the tests), `ctlseqs.zig`→
`ctlseqs.rs` (~60 `&[u8]` byte constants, format templates verbatim), and `Parser.zig`→
`parser.rs` (the full ground/escape/csi/osc/dcs/sos/pm/apc/ss3 machine; the KEY DECISION
§2 — `parse<'a>(&'a mut self, input: &'a [u8]) -> Result<ParseResult<'a>, ParseError>` with
`Key.text` borrowing input-or-`self.buf`, allocator/OOM dropped, paste owned; the std-shim
ports §5; the additive `from_u8`/`PartialEq` value-type touch-ups §4; the SGR/xterm mouse,
Kitty-keyboard u21 decode, bracketed paste, OSC-52 base64, and the c1b-uucode grapheme
path). Port all **34 `test{}`** assertion-for-assertion, update the `check_test_counts` MAP
(add `Parser.zig`), and keep Miri primary (pure crate, **no `unsafe`**). **Zero new R5
seams** — the parser is the pure half; the tty/Loop driver is chunk 3b. Nothing blocked.
