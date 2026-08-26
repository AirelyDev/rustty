# WS5 — lib-vt C-API Expansion (upstream BL-5 series) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the entire upstream "BL-5" lib-vt C-API expansion series (10 oracle commits) into `rustty-vt` / `rustty-unicode` / `rustty-vt-c` + the `include/rustty/vt/` header family, closing every BL-5 row in `.port/phase-2/MAIN_PARITY.md`.

**Architecture:** Each API family lands as (engine port in `rustty-vt`/`rustty-unicode`) → (C-ABI projection in `rustty-vt-c` module) → (`#[no_mangle] extern "C" fn rustty_*` wrapper in `crates/rustty-vt-c/src/lib.rs`) → (hand-maintained header in `include/rustty/vt/` + cbindgen regen of `include/rustty.h`) → (gates). The C API mostly wraps engine capability that already exists; the genuinely NEW engine work is: color parse/palette-entry/x11-entries expansion, the color-scheme report encoder, OSC/Kitty color-query reporting, unicode width APIs, `ScrollViewport::Row`, and the protocol-neutral clipboard-write effect.

**Tech Stack:** Rust 1.93.1 (pinned), cbindgen via `cargo run -p xtask -- header-gen`, clang header probes (`.port/tools/gates/check_rustty_vt_headers.sh`), oracle git repo at `/Users/gael/projects/ghostty`.

## Global Constraints (inherited from 2026-07-23-rustty-completion-ROADMAP.md)

- R1 full fidelity vs the ghostty oracle at `../ghostty`; the oracle is reference-only — no build-time dependency on it (R3).
- R2 product identity: rustty naming with per-token casing preservation (Ghostty→Rustty, ghostty→rustty, GHOSTTY_→RUSTTY_). NOTE on the roadmap's "`ghostty_*` C-ABI aliases stay" line: the `ghostty_*` alias-forwarder layer was **deleted** at commit `e4cc52d` ("abi: delete ghostty_* alias layer; identity-ghostty flips strings only") — there are zero `extern "C" fn ghostty_*` symbols anywhere in the workspace today, and `rustty-vt-c` never had any. New WS5 exports therefore get **`rustty_*` names only**, exactly like the existing 169 vt-c exports. Do NOT add ghostty_* forwarders.
- R4/R5: acyclic crate graph; gates (`check_acyclic.sh`, A4 ABI gate, Miri lanes) are invariant — never weakened to pass.
- DG5: C-ABI panic policy is ABORT — no unwind across the ABI. All new `extern "C"` wrappers follow the existing lib.rs pattern (no `catch_unwind`; internal code must not panic on valid caller contracts).
- Toolchain: pinned Rust 1.93.1 (`rust-toolchain.toml`). LTO stays banned.
- Ledger discipline: strike BL-5 entries in `.port/phase-2/MAIN_PARITY.md` in the same PR.
- Commit style: `subsystem: summary (chunk-ref)` — use `(WS5 T<n>)` as the chunk-ref. Branch per WS; merge to main only with both platforms green.
- Zig byte-slices → `Vec<u8>`/`&[u8]`, never lossy `String` (memory: zig-byteslice-to-rust-rawbytes).
- Raw-pointer discipline for C-ABI args (memory: rust-mut-args-strongly-protected): out-params are `*mut T`, never `&mut T` re-read across calls.
- When porting oracle doc prose into headers, strip any snark/gag editorializing (memory: strip-ghostty-gag-prose); the BL-5 headers are dry, so this is mostly a non-issue — rename mechanically.

## Oracle Access (READ THIS FIRST)

The local ghostty checkout `/Users/gael/projects/ghostty` has its working tree pinned at `4749c4e` (the port pin), which is **OLDER** than the BL-5 commits. The BL-5 commits live on `origin/main` of that repo (already fetched — verified). Therefore:

- Read a commit's diff: `git -C /Users/gael/projects/ghostty show <hash>`
- Read a touched file's CURRENT state: `git -C /Users/gael/projects/ghostty show origin/main:<path>` — do NOT read the working-tree file (it's the old pin).
- Scope discipline: `origin/main` also contains NON-BL-5 work (the BL-1 compression series, generation stamps — `ghostty_terminal_compress`, `render_state_begin_update`, etc.). Port ONLY the hunks belonging to the BL-5 commits listed below. When a BL-5 commit was later amended by another BL-5 commit (clipboard: `0a410f18e` superseded by `634ef7198`; rgb-by-pointer `20a1bfa5f` fixes `2970e9a2a`), port the FINAL combined state directly and record the supersession in a PORT NOTE.

The 10 BL-5 oracle commits (from `.port/phase-2/MAIN_PARITY.md` rows):

| hash        | subject                                          | family                       |
| ----------- | ------------------------------------------------ | ---------------------------- |
| `20a1bfa5f` | fix: pass RGB color inputs by pointer            | A (RGB-by-pointer)           |
| `2970e9a2a` | lib-vt: many more color utility APIs             | B (color utilities)          |
| `f00e90694` | lib-vt: add color scheme report encoder          | C (color-scheme report)      |
| `14c829883` | terminal: report OSC color queries in lib-vt     | D (OSC/Kitty queries)        |
| `fc5a72772` | lib-vt: add unicode codepoint width API          | E (unicode width)            |
| `65e61282a` | lib-vt: add unicode grapheme width API           | E (unicode width)            |
| `3a2e28329` | lib-vt: add scroll-to-row viewport scrolling     | F (scroll-to-row)            |
| `0a410f18e` | terminal: clipboard_set effect for OSC 52 writes | G (clipboard, superseded)    |
| `634ef7198` | terminal: make clipboard writes protocol neutral | G (clipboard, final)         |
| `f245cdc66` | lib-vt: expose selection gesture to Zig          | H (already covered — verify) |

**Export gap (verified by comm-diff of `origin/main:src/lib_vt.zig` exports vs the 169 live `rustty_*` exports):** 13 new exports —
`rustty_color_{parse,parse_x11,parse_palette_entry,palette_default,palette_generate,luminance,perceived_luminance,contrast,x11_names,x11_name_count}`, `rustty_color_scheme_report_encode`, `rustty_unicode_{codepoint_width,grapheme_width}` — plus one signature change (`rustty_color_rgb_get` takes `*const` RGB now) and one new terminal option (`clipboard_write = 26`). The BL-1 exports (`terminal_compress`, `render_state_begin/end_update`, …) are WS7 — do NOT add them here.

## File Structure

- `crates/rustty-vt/src/color.rs` — modify: `RGB::parse` trim + bare-hex forms, `PaletteEntry`, `parse_palette_entry`, `encode_rgb8/16`
- `crates/rustty-vt/src/x11_color.rs` — modify: add ordered `Entry`/`ENTRIES` table (rgb.txt order, names preserved)
- `crates/rustty-vt/src/device_status.rs` — modify: `encode_color_scheme_report` + `MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE`
- `crates/rustty-vt/src/terminal.rs` — modify: `color_for_xterm`, `color_for_kitty`, `ScrollViewport::Row`
- `crates/rustty-vt/src/kitty/color.rs` — modify: `Kind::has_terminal_query_color`
- `crates/rustty-vt/src/osc/mod.rs` — modify: color_operation reset leak fix; `ColorOperation` carries `terminator` to the handler (already parsed — verify)
- `crates/rustty-vt/src/stream_terminal.rs` — modify: clipboard_write effect, OSC/Kitty color-query reporting, color-scheme encoder delegation
- `crates/rustty-vt/src/clipboard.rs` — create: `Location`/`Content`/`Write`/`WriteResult`
- `crates/rustty-vt/src/lib.rs` — modify: `pub mod clipboard;`
- `crates/rustty-unicode/src/lib.rs` — modify: `codepoint_width`
- `crates/rustty-unicode/src/grapheme.rs` — modify: `GraphemeWidthEffect`, `grapheme_width_effect`, `GraphemeWidth`, `grapheme_width`
- `crates/rustty-config/src/...` (color/palette parse sites) — modify: delegate to the shared `rustty-vt` helpers (the `Config.zig` −80 hunk of `2970e9a2a`)
- `crates/rustty-vt-c/src/color.rs` — modify: full color-utility C projection
- `crates/rustty-vt-c/src/color_scheme.rs` — create
- `crates/rustty-vt-c/src/unicode.rs` — create
- `crates/rustty-vt-c/src/terminal.rs` — modify: `ClipboardContent`/`ClipboardWrite`, `ClipboardWriteFn`, `Option_::ClipboardWrite`, scroll `Row` tag
- `crates/rustty-vt-c/src/types.rs` — modify: register `PaletteMask`, `X11Entry`, `ClipboardContent`, `ClipboardWrite`
- `crates/rustty-vt-c/src/lib.rs` — modify: 13 new exports + rgb_get signature
- `include/rustty/vt/color.h` — modify; `include/rustty/vt/color_scheme.h`, `include/rustty/vt/unicode.h` — create; `include/rustty/vt/terminal.h`, `include/rustty/vt/sgr.h`, `include/rustty/vt.h` — modify
- `include/rustty.h` — regenerated (never hand-edited)
- `example/c-vt-color-scheme/` — create; `example/c-vt-effects/` — modify
- `.port/phase-2/MAIN_PARITY.md`, `.port/audits/GHOSTTY_LIFETIMES.tsv` — modify

Gate/tooling facts discovered during research (so you don't re-research):

- **Header/ABI gate for vt-c** is `.port/tools/gates/check_rustty_vt_headers.sh`: (a) `clang -fsyntax-only` compiles `#include <rustty/vt.h>`; (b) every `rustty_*` token in the curated headers that also appears in cbindgen's `include/rustty.h` must be a real export of BOTH the release dylib and the wasm32 build; (c) `node .port/tools/check-vt-struct-layouts.mjs` cross-checks every header-declared struct against `rustty_type_json()` (so new C structs MUST be registered in `crates/rustty-vt-c/src/types.rs`).
- **A4** (`.port/tools/gates/check_embed_h_abi.sh`) gates `include/rustty-embed.h` (the embedding umbrella) — WS5 does not touch that header and adds **no by-value aggregate returns** (the RGB fix makes the only candidate pointer-passed), so A4 must simply stay green; run it in the finale to prove no drift. No new `@embed-abi` freezes are required. `.port/audits/CABI_SYMBOLS.tsv` maps only the 90 embed-header symbols — vt-c exports are NOT in it; do not touch it.
- Header regen: `cargo run -p xtask -- header-gen` (cbindgen per `crates/rustty-vt-c/cbindgen.toml` → `include/rustty.h`). Hand-maintained per-module headers live in `include/rustty/vt/` (sorted: macros, forward decls, types, functions).
- vt-c test conventions: per-module `#[cfg(test)] mod tests` with `crate::test_util::with_test_allocator` for allocator-taking APIs; oracle tests ported assertion-for-assertion (G2).
- LIFETIMES ledger: `.port/audits/GHOSTTY_LIFETIMES.tsv` — prospective rows required for new raw-pointer-holding structs (memory: adopt-lifetimes-tsv).

---

### Task 1: Branch + oracle verification

**Files:** none (setup)

- [ ] **Step 1: Create the branch**

```bash
cd /Users/gael/projects/rustty && git checkout main && git pull && git checkout -b ws5-libvt-capi
```

- [ ] **Step 2: Verify the oracle has all 10 commits on origin/main**

```bash
cd /Users/gael/projects/ghostty
for h in 20a1bfa5f 2970e9a2a f00e90694 14c829883 fc5a72772 65e61282a 3a2e28329 0a410f18e 634ef7198 f245cdc66; do
  git merge-base --is-ancestor $h origin/main && echo "$h OK" || echo "$h MISSING"
done
```

Expected: 10 × `OK`. If any `MISSING`: `git -C /Users/gael/projects/ghostty fetch origin` and re-check. Do NOT move the pinned working tree.

- [ ] **Step 3: Baseline sanity build**

```bash
cd /Users/gael/projects/rustty && cargo build -p rustty-vt-c && cargo test -p rustty-vt-c --lib 2>&1 | tail -3
```

Expected: green (this is the pre-WS5 baseline).

---

### Task 2: Family A — RGB-by-pointer ABI fix (`20a1bfa5f`)

`rustty_color_rgb_get` currently takes `RgbC` **by value**; upstream changed it (and all RGB inputs) to `const*` for wasm/FFI friendliness. Only `rgb_get` exists in rustty today; all NEW color functions (Task 5) are authored pointer-style from the start.

**Files:**

- Modify: `crates/rustty-vt-c/src/color.rs` (fn `rgb_get`, ~line 70)
- Modify: `crates/rustty-vt-c/src/lib.rs` (wrapper `rustty_color_rgb_get`, ~line 119)
- Modify: `include/rustty/vt/color.h`
- Test: `crates/rustty-vt-c/src/color.rs` (`mod tests`)

**Interfaces:**

- Produces: `pub unsafe fn rgb_get(c: *const RgbC, r: *mut u8, g: *mut u8, b: *mut u8)` (module), `rustty_color_rgb_get(color: *const RgbC, r: *mut u8, g: *mut u8, b: *mut u8)` (export).

- [ ] **Step 1: Read the oracle commit**

```bash
git -C /Users/gael/projects/ghostty show 20a1bfa5f
```

- [ ] **Step 2: Write the failing test** (append to — or create — `mod tests` in `crates/rustty-vt-c/src/color.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // test coverage for the by-pointer form (oracle 20a1bfa5f)
    #[test]
    fn rgb_get_by_pointer() {
        let c = RgbC { r: 1, g: 2, b: 3 };
        let (mut r, mut g, mut b) = (0u8, 0u8, 0u8);
        unsafe { rgb_get(&c as *const RgbC, &mut r, &mut g, &mut b) };
        assert_eq!((r, g, b), (1, 2, 3));
    }
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p rustty-vt-c --lib color::tests::rgb_get_by_pointer`
Expected: COMPILE FAIL (`rgb_get` takes `RgbC` by value).

- [ ] **Step 4: Change the module fn** in `crates/rustty-vt-c/src/color.rs` (keep the existing PORT NOTE block, update the transcribed Zig):

```rust
/// PORT NOTE: `pub fn rgb_get(c: *const color.RGB.C, r: *u8, g: *u8, b: *u8)
/// callconv(lib.calling_conv) void` (oracle 20a1bfa5f: RGB inputs pass by
/// pointer for wasm/FFI friendliness).
///
/// # Safety
/// `c` must be a valid readable `RgbC` pointer; `r`, `g`, and `b` must be
/// valid, writable pointers to `u8`.
pub unsafe fn rgb_get(c: *const RgbC, r: *mut u8, g: *mut u8, b: *mut u8) {
    // SAFETY: pointer validity is the caller's contract.
    unsafe {
        // r.* = c.r;  g.* = c.g;  b.* = c.b;
        *r = (*c).r;
        *g = (*c).g;
        *b = (*c).b;
    }
}
```

- [ ] **Step 5: Change the lib.rs wrapper** (`rustty_color_rgb_get`):

```rust
/// C: rustty_color_rgb_get
///
/// # Safety
/// `color` must be a valid readable `RgbC`; `r`/`g`/`b` valid writable `u8`.
#[no_mangle]
pub unsafe extern "C" fn rustty_color_rgb_get(
    color: *const color::RgbC,
    r: *mut u8,
    g: *mut u8,
    b: *mut u8,
) {
    // SAFETY: caller contract (see # Safety).
    unsafe { color::rgb_get(color, r, g, b) }
}
```

- [ ] **Step 6: Update `include/rustty/vt/color.h`** — change the declaration + doc param line, mirroring the oracle header hunk (renamed):

```c
RUSTTY_API void rustty_color_rgb_get(const RusttyColorRgb* color,
                           uint8_t* r,
                           uint8_t* g,
                           uint8_t* b);
```

(and `@param color Pointer to the RGB color value` in its Doxygen block).

- [ ] **Step 7: Run tests + regen header**

```bash
cargo test -p rustty-vt-c --lib color::
cargo run -p xtask -- header-gen
git diff --stat include/rustty.h
```

Expected: tests PASS; `include/rustty.h` shows the pointer signature.

- [ ] **Step 8: Grep for in-tree callers** of `rustty_color_rgb_get` / `rgb_get` (wasm glue, examples) and fix them to pass a pointer:

```bash
grep -rn "color_rgb_get" --include="*.rs" --include="*.c" --include="*.mjs" crates example .port/tools | grep -v target
```

Expected: update any call site found (e.g. `example/` C files, `.port/tools/wasm-*.mjs` glue); rebuild each.

- [ ] **Step 9: Commit**

```bash
git add -A && git commit -m "vt-c: pass RGB color inputs by pointer — rustty_color_rgb_get ABI fix (WS5 T2)"
```

---

### Task 3: Family B engine — color parse expansion in rustty-vt (`2970e9a2a` engine half)

Port the `src/terminal/color.zig` and `src/terminal/x11_color.zig` hunks: `RGB::parse` gains whitespace trim + bare `rgb`/`rrggbb` hex forms + X11-name doc; new `PaletteEntry` + `parse_palette_entry`; `x11_color` gains an ordered `Entry`/`ENTRIES` table (rgb.txt file order, null-terminated-capable names).

**Files:**

- Modify: `crates/rustty-vt/src/color.rs`
- Modify: `crates/rustty-vt/src/x11_color.rs`
- Test: same files (`mod tests`)

**Interfaces:**

- Produces: `color::PaletteEntry { pub index: u8, pub color: RGB }`; `color::parse_palette_entry(value: &[u8]) -> Result<PaletteEntry, ParsePaletteEntryError>` with `pub enum ParsePaletteEntryError { InvalidFormat, Overflow }`; `x11_color::Entry { pub name: &'static str, pub color: RGB }` and `x11_color::entries() -> &'static [Entry]` (rgb.txt order). `RGB::parse` semantics change: trims ASCII space/tab, accepts bare 3/6-digit hex AFTER the X11-name lookup fails.

- [ ] **Step 1: Read the oracle hunks**

```bash
git -C /Users/gael/projects/ghostty show 2970e9a2a -- src/terminal/color.zig src/terminal/x11_color.zig
```

- [ ] **Step 2: Write the failing tests** — port the oracle's new test assertions into `crates/rustty-vt/src/color.rs` `mod tests` (assertion-for-assertion; keep the Zig lines as `//` comments per house style):

```rust
// test "parsePaletteEntry" (oracle 2970e9a2a)
#[test]
fn parse_palette_entry_cases() {
    let e = parse_palette_entry(b"0=#AABBCC").unwrap();
    assert_eq!(e.index, 0);
    assert_eq!(e.color, RGB { r: 170, g: 187, b: 204 });
    let e = parse_palette_entry(b"0b1=#014589").unwrap();
    assert_eq!((e.index, e.color), (1, RGB { r: 1, g: 69, b: 137 }));
    let e = parse_palette_entry(b"0o7=#234567").unwrap();
    assert_eq!((e.index, e.color), (7, RGB { r: 35, g: 69, b: 103 }));
    let e = parse_palette_entry(b"0xF=#ABCDEF").unwrap();
    assert_eq!((e.index, e.color), (15, RGB { r: 171, g: 205, b: 239 }));
    let e = parse_palette_entry(b"0 =  #AABBCC").unwrap();
    assert_eq!((e.index, e.color), (0, RGB { r: 170, g: 187, b: 204 }));
    let e = parse_palette_entry(b" 1= #DDEEFF    ").unwrap();
    assert_eq!((e.index, e.color), (1, RGB { r: 221, g: 238, b: 255 }));
    let e = parse_palette_entry(b"  2  =  #123456 ").unwrap();
    assert_eq!((e.index, e.color), (2, RGB { r: 18, g: 52, b: 86 }));
    let e = parse_palette_entry(b"1=black").unwrap();
    assert_eq!((e.index, e.color), (1, RGB { r: 0, g: 0, b: 0 }));

    assert_eq!(parse_palette_entry(b" "), Err(ParsePaletteEntryError::InvalidFormat));
    assert_eq!(parse_palette_entry(b"a"), Err(ParsePaletteEntryError::InvalidFormat));
    assert_eq!(parse_palette_entry(b"256=#AABBCC"), Err(ParsePaletteEntryError::Overflow));
    assert_eq!(parse_palette_entry(b"1=notacolor"), Err(ParsePaletteEntryError::InvalidFormat));
}

// new "RGB.parse" assertions (oracle 2970e9a2a)
#[test]
fn rgb_parse_bare_hex_and_trim() {
    assert_eq!(RGB { r: 10, g: 11, b: 12 }, RGB::parse(b"0A0B0C").unwrap());
    assert_eq!(RGB { r: 255, g: 255, b: 255 }, RGB::parse(b"FFFFFF").unwrap());
    assert_eq!(RGB { r: 255, g: 255, b: 255 }, RGB::parse(b"FFF").unwrap());
    assert_eq!(RGB { r: 51, g: 68, b: 85 }, RGB::parse(b"#345").unwrap());
    assert_eq!(RGB { r: 170, g: 187, b: 204 }, RGB::parse(b" #AABBCC   ").unwrap());
}
```

NOTE: adjust the argument type of `RGB::parse` in the test to whatever it is today (`&str` or `&[u8]`) — check `crates/rustty-vt/src/color.rs:855`. If it is `&str`, keep `&str` (that is the pre-existing port decision) and use string literals.

And in `crates/rustty-vt/src/x11_color.rs` `mod tests`:

```rust
// entries table: rgb.txt order, matches the map (oracle 2970e9a2a)
#[test]
fn x11_entries_order_and_map_parity() {
    let entries = entries();
    assert!(entries.len() > 700);
    for e in entries {
        assert_eq!(map.get(e.name), Some(e.color));
    }
    // rgb.txt starts with "snow"
    assert_eq!(entries[0].name, "snow");
    assert_eq!(entries[0].color, RGB { r: 255, g: 250, b: 250 });
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p rustty-vt --lib color::tests::parse_palette_entry_cases x11_color::`
Expected: COMPILE FAIL (`parse_palette_entry`, `entries` not defined).

- [ ] **Step 4: Implement in `crates/rustty-vt/src/color.rs`** — faithful port of the oracle hunk:

```rust
/// A parsed palette entry from Rustty's config "N=COLOR" syntax.
// pub const PaletteEntry = struct { index: u8, color: RGB };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteEntry {
    pub index: u8,
    pub color: RGB,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParsePaletteEntryError {
    InvalidFormat,
    Overflow,
}

/// Parse a palette entry in Rustty config syntax: "N=COLOR" where N is
/// a palette index 0-255 (decimal, or 0x/0o/0b-prefixed per Zig's
/// parseInt base-0 rules) and COLOR is anything RGB::parse accepts.
/// Whitespace (spaces/tabs) around N and COLOR is ignored.
// pub fn parsePaletteEntry(value: []const u8) error{ InvalidFormat, Overflow }!PaletteEntry
pub fn parse_palette_entry(value: &[u8]) -> Result<PaletteEntry, ParsePaletteEntryError> {
    // const eql_idx = std.mem.indexOfScalar(u8, value, '=') orelse return error.InvalidFormat;
    let eql_idx = value
        .iter()
        .position(|&b| b == b'=')
        .ok_or(ParsePaletteEntryError::InvalidFormat)?;
    // const index = std.fmt.parseInt(u8, std.mem.trim(u8, value[0..eql_idx], " \t"), 0) catch ...
    let index_str = trim_space_tab(&value[..eql_idx]);
    let index = parse_int_u8_base0(index_str)?;
    // const rgb = try RGB.parse(value[eql_idx + 1 ..]);
    let rgb = RGB::parse(/* value[eql_idx + 1..] as the existing parse input type */)
        .map_err(|_| ParsePaletteEntryError::InvalidFormat)?;
    Ok(PaletteEntry { index, color: rgb })
}

/// Zig `std.fmt.parseInt(u8, s, 0)`: base-0 = auto-detect 0x/0o/0b prefix,
/// else decimal. `Overflow` (value > 255) is distinguished from
/// `InvalidCharacter` (mapped to InvalidFormat), matching the oracle.
fn parse_int_u8_base0(s: &[u8]) -> Result<u8, ParsePaletteEntryError> {
    let (digits, radix): (&[u8], u32) = if s.len() > 2 && s[0] == b'0' {
        match s[1] {
            b'x' | b'X' => (&s[2..], 16),
            b'o' | b'O' => (&s[2..], 8),
            b'b' | b'B' => (&s[2..], 2),
            _ => (s, 10),
        }
    } else {
        (s, 10)
    };
    if digits.is_empty() {
        return Err(ParsePaletteEntryError::InvalidFormat);
    }
    let mut acc: u32 = 0;
    for &b in digits {
        // Zig parseInt allows '_' separators; preserve that.
        if b == b'_' {
            continue;
        }
        let d = (b as char)
            .to_digit(radix)
            .ok_or(ParsePaletteEntryError::InvalidFormat)?;
        acc = acc * radix + d;
        if acc > 255 {
            return Err(ParsePaletteEntryError::Overflow);
        }
    }
    Ok(acc as u8)
}

fn trim_space_tab(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&b| b != b' ' && b != b'\t').unwrap_or(s.len());
    let end = s.iter().rposition(|&b| b != b' ' && b != b'\t').map_or(start, |i| i + 1);
    &s[start..end]
}
```

Then apply the `RGB::parse` hunk in place (the existing rustty `parse` mirrors the OLD Zig body line-by-line — apply the diff mechanically):

1. First line: `let input = trim of value over " \t"`; use `input` everywhere `value` was used.
2. After the X11-map lookup fails, add the bare-hex arm before the `rgb:`-prefix check:

```rust
// switch (input.len) { 3 => ..., 6 => ..., else => {} }
match input.len() {
    3 => {
        return Ok(RGB {
            r: RGB::from_hex(&input[0..1])?,
            g: RGB::from_hex(&input[1..2])?,
            b: RGB::from_hex(&input[2..3])?,
        })
    }
    6 => {
        return Ok(RGB {
            r: RGB::from_hex(&input[0..2])?,
            g: RGB::from_hex(&input[2..4])?,
            b: RGB::from_hex(&input[4..6])?,
        })
    }
    _ => {}
}
```

(Adapt slicing to the existing `parse` input type — `&str` slices if the port uses `&str`. `from_hex` = whatever the existing helper is named; check before writing.) Update the doc comment to the oracle's new wording (forms with/without `#`, X11 names, "Leading and trailing spaces and tabs are ignored.").

3. The old parse trimmed only `" "` for the X11 lookup; the new code trims `" \t"` once at the top — make sure the old inner trim is removed (oracle deleted it).

- [ ] **Step 5: Implement `x11_color::entries`** in `crates/rustty-vt/src/x11_color.rs`. The existing port builds `color_map()` lazily from the embedded `rgb.txt` data at runtime (`OnceLock`). Follow the same shape:

```rust
/// A single X11 color entry.
// pub const Entry = struct { name: [:0]const u8, color: RGB };
// PORT NOTE: the reference name is null-terminated for the C API. In rustty the
// static C table (with its own null-terminated copies) is built in
// rustty-vt-c::color; here `name` is the plain rgb.txt name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: &'static str,
    pub color: RGB,
}

/// All X11 colors in rgb.txt file order.
///
/// This is kept separate from `map` because the C API needs stable file order.
// pub const entries: []const Entry = entriesArray();
pub fn entries() -> &'static [Entry] {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(build_entries)
}

fn build_entries() -> Vec<Entry> {
    // Same rgb.txt line parse the map builder uses (columns 0..3, 4..7, 8..11
    // are r/g/b, name from column 13 to trailing-whitespace-trimmed end) — in
    // FILE ORDER, no case folding, duplicates kept.
    ...
}
```

Implement `build_entries` by refactoring the line-parsing loop out of the existing `build_color_map()` (`crates/rustty-vt/src/x11_color.rs:97`) into a shared iterator so the two stay in lockstep (DRY; the oracle shares the same comptime parse). Keep `map`'s case-insensitive behavior untouched.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p rustty-vt --lib color:: x11_color::`
Expected: PASS (including all pre-existing color tests — the `parse` semantics change must not break them; the oracle's updated `RGB.parse` test block shows the expected final assertions — port any changed ones).

- [ ] **Step 7: Commit**

```bash
git add crates/rustty-vt/src/color.rs crates/rustty-vt/src/x11_color.rs
git commit -m "vt: color parse trim+bare-hex, parse_palette_entry, ordered x11 entries (WS5 T3)"
```

---

### Task 4: Family B — rustty-config delegation (`2970e9a2a` Config.zig hunk)

Upstream moved config color/palette parsing onto the shared terminal helpers (−80 lines in `Config.zig`) so CLI/config and the C ABI stay in lockstep.

**Files:**

- Modify: `crates/rustty-config/src/` — the `Color`/`Palette` config-value parse impls (find with `grep -rn "fn parse" crates/rustty-config/src | grep -i -E "color|palette"` and by reading the oracle hunk `git -C /Users/gael/projects/ghostty show 2970e9a2a -- src/config/Config.zig`)

**Interfaces:**

- Consumes: `rustty_vt::color::{RGB, parse_palette_entry, ParsePaletteEntryError}` from Task 3.

- [ ] **Step 1: Read the oracle hunk** and locate the rustty analogs (the config `Color.parseCLI`/`Palette.parseCLI` ports).
- [ ] **Step 2: Write a failing test** in the config crate asserting the NEW accepted forms flow through config parsing (bare hex + trimmed input + `0x10=#282c34` palette syntax), e.g.:

```rust
#[test]
fn color_config_accepts_bare_hex_and_prefixed_palette_index() {
    // exact API: mirror the neighboring existing parseCLI tests in the file
    // you are editing; assert "0A0B0C" parses to (10,11,12) and
    // "0x10=#282c34" sets palette index 16.
}
```

- [ ] **Step 3: Run to verify it fails**, then **replace the config-local parsing bodies with delegation** to `rustty_vt::color::RGB::parse` / `rustty_vt::color::parse_palette_entry` exactly as the oracle diff does (delete the now-dead local helpers the diff deletes).
- [ ] **Step 4: Run** `cargo test -p rustty-config` — PASS (all pre-existing config color tests must still pass; the oracle updated some expectations in the same commit — port those test changes too).
- [ ] **Step 5: Commit**

```bash
git add crates/rustty-config crates/rustty-vt
git commit -m "config: delegate color/palette parsing to shared vt color helpers (WS5 T4)"
```

---

### Task 5: Family B — vt-c color module expansion (`2970e9a2a` + `20a1bfa5f` C half)

Port the full `origin/main:src/terminal/c/color.zig` (already read during research — 10 functions + 2 C types + a static X11 table + ~25 tests).

**Files:**

- Modify: `crates/rustty-vt-c/src/color.rs`
- Test: same file, `mod tests`

**Interfaces:**

- Produces (module fns, all `pub unsafe fn`, wrapped in Task 6):
  - `pub struct PaletteMask { pub bits: [u64; 4] }` (`#[repr(C)]`) with `fn to_zig(&self) -> color::PaletteMask`
  - `pub struct X11Entry { pub name: *const c_char, pub color: RgbC }` (`#[repr(C)]`; `name` null ⇒ end of table)
  - `parse_x11(name: *const u8, len: usize, out: *mut RgbC) -> Result`
  - `parse(value: *const u8, len: usize, out: *mut RgbC) -> Result`
  - `palette_default(out: *mut PaletteC)`
  - `palette_generate(base: *const PaletteC, skip: *const PaletteMask, bg: *const RgbC, fg: *const RgbC, harmonious: bool, out: *mut PaletteC)`
  - `x11_names() -> *const X11Entry`, `x11_name_count() -> usize`
  - `parse_palette_entry(value: *const u8, len: usize, out_index: *mut u8, out_rgb: *mut RgbC) -> Result`
  - `luminance(c: *const RgbC) -> f64`, `perceived_luminance(c: *const RgbC) -> f64`, `contrast(a: *const RgbC, b: *const RgbC) -> f64`

- [ ] **Step 1: Read the oracle file**

```bash
git -C /Users/gael/projects/ghostty show origin/main:src/terminal/c/color.zig
```

- [ ] **Step 2: Write the failing tests** — port the oracle test block. Representative complete translations (port ALL oracle tests in the file, assertion-for-assertion; the helpers below make the rest mechanical):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rustty_vt::color::{self, RGB};

    fn expect_rgb(expected: RGB, actual: RgbC) {
        assert_eq!(expected, actual);
    }

    fn generate_palette(
        base: Option<&PaletteC>,
        skip: Option<&PaletteMask>,
        bg: RGB,
        fg: RGB,
        harmonious: bool,
    ) -> PaletteC {
        let mut out: PaletteC = [RGB { r: 0, g: 0, b: 0 }; 256];
        unsafe {
            palette_generate(
                base.map_or(std::ptr::null(), |b| b as *const PaletteC),
                skip.map_or(std::ptr::null(), |s| s as *const PaletteMask),
                &bg as *const RgbC,
                &fg as *const RgbC,
                harmonious,
                &mut out,
            )
        };
        out
    }

    fn set_mask_bit(mask: &mut PaletteMask, idx: usize) {
        mask.bits[idx >> 6] |= 1u64 << (idx & 63);
    }

    // test "color: parse_x11 valid"
    #[test]
    fn parse_x11_valid() {
        let cases: &[(&[u8], RGB)] = &[
            (b"white", RGB { r: 255, g: 255, b: 255 }),
            (b"medium spring green", RGB { r: 0, g: 250, b: 154 }),
            (b"ForestGreen", RGB { r: 34, g: 139, b: 34 }),
            (b"FoReStGReen", RGB { r: 34, g: 139, b: 34 }),
            (b" Forest Green ", RGB { r: 34, g: 139, b: 34 }),
            (b"\tblack\t", RGB { r: 0, g: 0, b: 0 }),
        ];
        for (input, expected) in cases {
            let mut out = RGB { r: 0, g: 0, b: 0 };
            assert_eq!(Result::Success, unsafe {
                parse_x11(input.as_ptr(), input.len(), &mut out)
            });
            expect_rgb(*expected, out);
        }
    }

    // test "color: parse_x11 invalid"
    #[test]
    fn parse_x11_invalid() {
        for case in [&b"nosuchcolor"[..], b"", b"   ", b"#ffffff"] {
            let mut out = RGB { r: 0, g: 0, b: 0 };
            assert_eq!(Result::InvalidValue, unsafe {
                parse_x11(case.as_ptr(), case.len(), &mut out)
            });
        }
        let mut out = RGB { r: 0, g: 0, b: 0 };
        assert_eq!(Result::InvalidValue, unsafe {
            parse_x11(std::ptr::null(), 0, &mut out)
        });
    }

    // test "color: parse valid" / "color: parse invalid" — same shape, cases:
    //   valid: black/#AABBCC/0A0B0C/FFF/#345/rgb:1/2/3/"  black "/" #AABBCC   "
    //   invalid: ""/notacolor/#12345 + null ptr
    // test "color: parse_palette_entry valid" — cases from the oracle:
    //   ("0=#AABBCC",0,(170,187,204)) ("0xF=#ABCDEF",15,(171,205,239))
    //   ("0b1=#014589",1,(1,69,137)) ("0o7=#234567",7,(35,69,103))
    //   (" 1= #DDEEFF    ",1,(221,238,255)) ("1=black",1,(0,0,0))
    // test "color: parse_palette_entry invalid": "256=#AABBCC","a","","1=notacolor",null
    // test "color: palette_default" — spot values: [0]=(0x1D,0x1F,0x21),
    //   [16]=(0,0,0), [231]=(255,255,255), [232]=(8,8,8), [255]=(238,238,238)
    // test "color: palette_generate base16 preserved" / "cube corners" /
    //   "light theme harmonious=false" / "grayscale monotonic" / "skip mask" /
    //   "dark harmonious no-op" / "light harmonious ramp" /
    //   "NULL base uses default" / "NULL skip" / "matches generate256Color" /
    //   "in-place" — all direct translations via generate_palette/set_mask_bit.
    // test "color: luminance" / "perceived_luminance" / "contrast" — use
    //   (v - expected).abs() < 0.000001 for expectApproxEqAbs.

    // test "color: x11_names"
    #[test]
    fn x11_names_walk() {
        let names = x11_names();
        let count = x11_name_count();
        assert_eq!(rustty_vt::x11_color::entries().len(), count);
        assert!(count > 700);
        let mut walked = 0usize;
        unsafe {
            while !(*names.add(walked)).name.is_null() {
                let name = std::ffi::CStr::from_ptr((*names.add(walked)).name)
                    .to_str()
                    .unwrap();
                let expected = rustty_vt::x11_color::map.get(name).unwrap();
                expect_rgb(expected, (*names.add(walked)).color);
                walked += 1;
            }
            assert_eq!(count, walked);
            assert!((*names.add(count)).name.is_null());
        }
    }

    // test "color: x11_names static"
    #[test]
    fn x11_names_static() {
        assert_eq!(x11_names() as usize, x11_names() as usize);
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p rustty-vt-c --lib color::`
Expected: COMPILE FAIL (functions/types not defined).

- [ ] **Step 4: Implement** in `crates/rustty-vt-c/src/color.rs` (below the existing `rgb_get`; keep the Zig-transcription comment style):

```rust
use std::ffi::c_char;
use std::sync::OnceLock;

use crate::result::Result;
use rustty_vt::x11_color;

/// C: GhosttyColorPaletteMask -> RusttyColorPaletteMask (R2)
// pub const PaletteMask = extern struct { bits: [4]u64, ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteMask {
    pub bits: [u64; 4],
}

impl PaletteMask {
    /// Convert to the Zig PaletteMask (std.StaticBitSet(256)) analog.
    pub fn to_zig(&self) -> rustty_vt::color::PaletteMask {
        let mut result = rustty_vt::color::PaletteMask::init_empty();
        for i in 0..256usize {
            if ((self.bits[i >> 6] >> (i & 63)) & 1) != 0 {
                result.set(i);
            }
        }
        result
    }
}

/// C: GhosttyColorX11Entry -> RusttyColorX11Entry (R2)
// pub const X11Entry = extern struct { name: ?[*:0]const u8, color: color.RGB.C };
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct X11Entry {
    /// Null-terminated color name; null marks the end of the table.
    pub name: *const c_char,
    pub color: RgbC,
}

// SAFETY: the table is immutable static data (pointers into leaked
// NUL-terminated strings); sharing across threads is sound.
unsafe impl Sync for X11Entry {}

// PORT NOTE: the reference builds `x11_entries` at COMPTIME
// (`[x11_color.entries.len + 1]X11Entry`, null-name terminated). Rust has no
// comptime rgb.txt parse; the same table is built once at runtime and leaked
// (name storage lives for the process lifetime, satisfying the header's
// "static table" contract and the oracle's pointer-stability test).
fn x11_entries() -> &'static [X11Entry] {
    static ENTRIES: OnceLock<Vec<X11Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        let src = x11_color::entries();
        let mut result = Vec::with_capacity(src.len() + 1);
        for e in src {
            let name: &'static std::ffi::CStr = Box::leak(
                std::ffi::CString::new(e.name).unwrap().into_boxed_c_str(),
            );
            result.push(X11Entry { name: name.as_ptr(), color: e.color });
        }
        result.push(X11Entry {
            name: std::ptr::null(),
            color: RGB { r: 0, g: 0, b: 0 },
        });
        result
    })
}

/// # Safety
/// `name_`, if non-null, must be readable for `len` bytes; `out` writable.
pub unsafe fn parse_x11(name_: *const u8, len: usize, out: *mut RgbC) -> Result {
    // const name = (name_ orelse return .invalid_value)[0..len];
    if name_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: caller contract.
    let name = unsafe { std::slice::from_raw_parts(name_, len) };
    // const trimmed = std.mem.trim(u8, name, " \t");
    let trimmed = trim_space_tab(name);
    // const rgb = x11_color.map.get(trimmed) orelse return .invalid_value;
    let Ok(trimmed) = std::str::from_utf8(trimmed) else {
        return Result::InvalidValue;
    };
    let Some(rgb) = x11_color::map.get(trimmed) else {
        return Result::InvalidValue;
    };
    // SAFETY: caller contract.
    unsafe { *out = rgb };
    Result::Success
}

/// # Safety
/// `value_`, if non-null, readable for `len` bytes; `out` writable.
pub unsafe fn parse(value_: *const u8, len: usize, out: *mut RgbC) -> Result {
    if value_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: caller contract.
    let value = unsafe { std::slice::from_raw_parts(value_, len) };
    // const rgb = color.RGB.parse(value) catch return .invalid_value;
    match RGB::parse(/* value, via the crate's parse input type */) {
        Ok(rgb) => {
            // SAFETY: caller contract.
            unsafe { *out = rgb };
            Result::Success
        }
        Err(_) => Result::InvalidValue,
    }
}

/// # Safety
/// `out` must be a valid writable `PaletteC` pointer.
pub unsafe fn palette_default(out: *mut PaletteC) {
    // out.* = color.paletteCval(&color.default);
    // SAFETY: caller contract (RGB == RGB.C here, plain copy).
    unsafe { *out = rustty_vt::color::DEFAULT };
}

/// Generate a 256-color palette. The output may alias the base input.
/// # Safety
/// `bg`/`fg`/`out` valid; `base_`/`skip_` valid or null.
pub unsafe fn palette_generate(
    base_: *const PaletteC,
    skip_: *const PaletteMask,
    bg: *const RgbC,
    fg: *const RgbC,
    harmonious: bool,
    out: *mut PaletteC,
) {
    // SAFETY: caller contract. `base` is COPIED before `out` is written, so
    // out==base aliasing is sound (the oracle documents in-place use).
    unsafe {
        let base: rustty_vt::color::Palette =
            if base_.is_null() { rustty_vt::color::DEFAULT } else { *base_ };
        let skip = if skip_.is_null() {
            rustty_vt::color::PaletteMask::init_empty()
        } else {
            (*skip_).to_zig()
        };
        let result =
            rustty_vt::color::generate_256_color(base, skip, *bg, *fg, harmonious);
        *out = result;
    }
}

pub fn x11_names() -> *const X11Entry {
    x11_entries().as_ptr()
}

pub fn x11_name_count() -> usize {
    x11_color::entries().len()
}

/// # Safety
/// `value_` readable for `len` (or null); `out_index`/`out_rgb` writable.
pub unsafe fn parse_palette_entry(
    value_: *const u8,
    len: usize,
    out_index: *mut u8,
    out_rgb: *mut RgbC,
) -> Result {
    if value_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: caller contract.
    let value = unsafe { std::slice::from_raw_parts(value_, len) };
    // const entry = color.parsePaletteEntry(value) catch return .invalid_value;
    match rustty_vt::color::parse_palette_entry(value) {
        Ok(entry) => {
            // SAFETY: caller contract.
            unsafe {
                *out_index = entry.index;
                *out_rgb = entry.color;
            }
            Result::Success
        }
        Err(_) => Result::InvalidValue,
    }
}

/// # Safety
/// `c` must be a valid readable `RgbC` pointer.
pub unsafe fn luminance(c: *const RgbC) -> f64 {
    // SAFETY: caller contract.
    unsafe { (*c).luminance() }
}

/// # Safety
/// `c` must be a valid readable `RgbC` pointer.
pub unsafe fn perceived_luminance(c: *const RgbC) -> f64 {
    // SAFETY: caller contract.
    unsafe { (*c).perceived_luminance() }
}

/// # Safety
/// `a`/`b` must be valid readable `RgbC` pointers.
pub unsafe fn contrast(a: *const RgbC, b: *const RgbC) -> f64 {
    // SAFETY: caller contract.
    unsafe { (*a).contrast(*b) }
}

fn trim_space_tab(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&b| b != b' ' && b != b'\t').unwrap_or(s.len());
    let end = s.iter().rposition(|&b| b != b' ' && b != b'\t').map_or(start, |i| i + 1);
    &s[start..end]
}
```

Adjust minor names to what actually exists in `rustty-vt` (`DEFAULT` static, `generate_256_color` signature `(Palette, PaletteMask, RGB, RGB, bool) -> Palette` — verified at `crates/rustty-vt/src/color.rs:199`; `PaletteMask::init_empty()`/`set()` — verified; `map.get(&str)` — verified at `x11_color.rs:42`).

- [ ] **Step 5: Run the module tests**

Run: `cargo test -p rustty-vt-c --lib color::`
Expected: PASS (all ported oracle tests).

- [ ] **Step 6: Commit**

```bash
git add crates/rustty-vt-c/src/color.rs
git commit -m "vt-c: color utility module — parse/parse_x11/palette/luminance/contrast/x11 table (WS5 T5)"
```

---

### Task 6: Family B — color exports + types registry + headers + gate

**Files:**

- Modify: `crates/rustty-vt-c/src/lib.rs` (10 new wrappers)
- Modify: `crates/rustty-vt-c/src/types.rs` (register `PaletteMask`, `X11Entry`)
- Modify: `include/rustty/vt/color.h`, `include/rustty/vt/sgr.h`
- Regen: `include/rustty.h`

**Interfaces:**

- Produces the 10 exported symbols `rustty_color_parse`, `rustty_color_parse_x11`, `rustty_color_parse_palette_entry`, `rustty_color_palette_default`, `rustty_color_palette_generate`, `rustty_color_luminance`, `rustty_color_perceived_luminance`, `rustty_color_contrast`, `rustty_color_x11_names`, `rustty_color_x11_name_count`.

- [ ] **Step 1: Add the wrappers to lib.rs** (next to the existing `rustty_color_rgb_get`), each 1:1 with the oracle `lib_vt.zig` `@export` line, `Result`-returning ones through `.to_c_int()`:

```rust
// pub const color_parse = color.parse;  (ghostty_color_parse)
/// C: rustty_color_parse
/// # Safety
/// `value`, if non-null, readable for `len` bytes; `out` valid writable.
#[no_mangle]
pub unsafe extern "C" fn rustty_color_parse(
    value: *const u8,
    len: usize,
    out: *mut color::RgbC,
) -> c_int {
    // SAFETY: caller contract.
    unsafe { color::parse(value, len, out) }.to_c_int()
}

/// C: rustty_color_parse_x11
/// # Safety
/// `name`, if non-null, readable for `len` bytes; `out` valid writable.
#[no_mangle]
pub unsafe extern "C" fn rustty_color_parse_x11(
    name: *const u8,
    len: usize,
    out: *mut color::RgbC,
) -> c_int {
    // SAFETY: caller contract.
    unsafe { color::parse_x11(name, len, out) }.to_c_int()
}

/// C: rustty_color_parse_palette_entry
/// # Safety
/// `value`, if non-null, readable for `len`; `out_index`/`out_rgb` writable.
#[no_mangle]
pub unsafe extern "C" fn rustty_color_parse_palette_entry(
    value: *const u8,
    len: usize,
    out_index: *mut u8,
    out_rgb: *mut color::RgbC,
) -> c_int {
    // SAFETY: caller contract.
    unsafe { color::parse_palette_entry(value, len, out_index, out_rgb) }.to_c_int()
}

/// C: rustty_color_palette_default
/// # Safety
/// `out` must point to writable storage for 256 RGB values.
#[no_mangle]
pub unsafe extern "C" fn rustty_color_palette_default(out: *mut color::PaletteC) {
    // SAFETY: caller contract.
    unsafe { color::palette_default(out) }
}

/// C: rustty_color_palette_generate
/// # Safety
/// `bg`/`fg`/`out` valid; `base`/`skip` valid or null; out may alias base.
#[no_mangle]
pub unsafe extern "C" fn rustty_color_palette_generate(
    base: *const color::PaletteC,
    skip: *const color::PaletteMask,
    bg: *const color::RgbC,
    fg: *const color::RgbC,
    harmonious: bool,
    out: *mut color::PaletteC,
) {
    // SAFETY: caller contract.
    unsafe { color::palette_generate(base, skip, bg, fg, harmonious, out) }
}

/// C: rustty_color_luminance
/// # Safety
/// `color` must be a valid readable RGB pointer.
#[no_mangle]
pub unsafe extern "C" fn rustty_color_luminance(color_: *const color::RgbC) -> f64 {
    // SAFETY: caller contract.
    unsafe { color::luminance(color_) }
}

/// C: rustty_color_perceived_luminance
/// # Safety
/// `color` must be a valid readable RGB pointer.
#[no_mangle]
pub unsafe extern "C" fn rustty_color_perceived_luminance(
    color_: *const color::RgbC,
) -> f64 {
    // SAFETY: caller contract.
    unsafe { color::perceived_luminance(color_) }
}

/// C: rustty_color_contrast
/// # Safety
/// `a`/`b` must be valid readable RGB pointers.
#[no_mangle]
pub unsafe extern "C" fn rustty_color_contrast(
    a: *const color::RgbC,
    b: *const color::RgbC,
) -> f64 {
    // SAFETY: caller contract.
    unsafe { color::contrast(a, b) }
}

/// C: rustty_color_x11_names
#[no_mangle]
pub extern "C" fn rustty_color_x11_names() -> *const color::X11Entry {
    color::x11_names()
}

/// C: rustty_color_x11_name_count
#[no_mangle]
pub extern "C" fn rustty_color_x11_name_count() -> usize {
    color::x11_name_count()
}
```

- [ ] **Step 2: Register the two new structs in `types.rs`** exactly as the oracle does (`2970e9a2a` types.zig hunk): add map rows `"RusttyColorPaletteMask" -> StructInfo of color::PaletteMask` and `"RusttyColorX11Entry" -> StructInfo of color::X11Entry`, following the existing row pattern in `crates/rustty-vt-c/src/types.rs` (alphabetical position after `RusttyColorRgb`'s neighbors, same as the oracle's ordering: PaletteMask before ColorRgb's row? — mirror the oracle: `GhosttyColorPaletteMask` between `GhosttyCodepoints` and `GhosttyColorRgb`, `GhosttyColorX11Entry` right after `GhosttyColorRgb`). Extend the `json parses`-analog test with `contains` asserts for both names.

- [ ] **Step 3: Update the headers.** Port `origin/main:include/ghostty/vt/color.h` (427-line expansion) and the `sgr.h` 7-line hunk of `2970e9a2a` into `include/rustty/vt/color.h` / `include/rustty/vt/sgr.h` with mechanical R2 renaming (`Ghostty→Rustty`, `ghostty_→rustty_`, `GHOSTTY_→RUSTTY_`, `libghostty→librustty`, ghostty.org→rustty.sh):

```bash
git -C /Users/gael/projects/ghostty show origin/main:include/ghostty/vt/color.h > /tmp/oracle_color.h
git -C /Users/gael/projects/ghostty show 2970e9a2a -- include/ghostty/vt/sgr.h
```

Keep the existing rustty-local content (the `RusttyColorRgb` typedef already present) merged with the new sections; preserve the header's macro/forward-decl/type/function ordering rule from `crates/rustty-vt-c/AGENTS.md`. The oracle `color.h` at origin/main also reflects the `20a1bfa5f` pointer signatures — copy those verbatim (renamed).

- [ ] **Step 4: Regen + gates**

```bash
cargo run -p xtask -- header-gen
cargo build -p rustty-vt-c --release
cargo build -p rustty-vt-c --target wasm32-unknown-unknown --release
bash .port/tools/gates/check_rustty_vt_headers.sh
```

Expected: `VT-HEADERS: GREEN` (headers compile; all `rustty_color_*` symbols exported natively AND in wasm; struct layouts match `rustty_type_json`). If (b) flags a wasm-missing symbol, confirm the export is in `lib.rs` (cbindgen + wasm both key off `#[no_mangle]` there).

- [ ] **Step 5: Run the full crate tests**

Run: `cargo test -p rustty-vt-c`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "vt-c: export the 10 color utility APIs + type registry + color.h/sgr.h (WS5 T6)"
```

---

### Task 7: Family C engine — color-scheme report encoder (`f00e90694` engine half)

**Files:**

- Modify: `crates/rustty-vt/src/device_status.rs`
- Modify: `crates/rustty-vt/src/stream_terminal.rs` (~line 806: replace the hand-inlined `b"\x1B[?997;1n"` arm with the encoder)
- Modify: `crates/rustty-termio/src/termio_impl.rs` (~line 1662: same delegation, per the commit's `Termio.zig` hunk)
- Test: `crates/rustty-vt/src/device_status.rs` `mod tests`

**Interfaces:**

- Produces: `device_status::encode_color_scheme_report(out: &mut Vec<u8>, scheme: ColorScheme)` — writer-style append (match the crate's existing encode_* conventions: check `modes::report_encode` / `focus::encode` in rustty-vt for the established writer analog and mirror it) and `device_status::MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE: usize = 9`.

- [ ] **Step 1: Read the oracle**

```bash
git -C /Users/gael/projects/ghostty show f00e90694
git -C /Users/gael/projects/ghostty show origin/main:src/terminal/device_status.zig
```

- [ ] **Step 2: Failing tests** in `crates/rustty-vt/src/device_status.rs`:

```rust
// test "encode color scheme report dark" / "light" (oracle f00e90694)
#[test]
fn encode_color_scheme_report_dark_light() {
    assert_eq!(9, MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE);
    let mut buf = Vec::new();
    encode_color_scheme_report(&mut buf, ColorScheme::Dark);
    assert_eq!(b"\x1B[?997;1n", &buf[..]);
    buf.clear();
    encode_color_scheme_report(&mut buf, ColorScheme::Light);
    assert_eq!(b"\x1B[?997;2n", &buf[..]);
}
```

(Adapt the writer type to the crate's existing encoder convention discovered in Step 1's grep — if the house style is `fn(&mut impl std::io::Write)` or a fixed-buffer writer, use that; the assertions stay identical.)

- [ ] **Step 3: Run to fail**, then implement:

```rust
/// Maximum number of bytes that `encode_color_scheme_report` will write.
// pub const max_color_scheme_report_encode_size = ... (comptime max over fields)
// PORT NOTE: the reference computes this at comptime by dry-running the encoder
// over every enum field; both arms write 9 bytes, frozen here with a test
// asserting the value matches a dry run.
pub const MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE: usize = 9;

/// Encode a color scheme report response for CSI ? 996 n queries.
pub fn encode_color_scheme_report(writer: &mut Vec<u8>, scheme: ColorScheme) {
    writer.extend_from_slice(match scheme {
        // .dark => "\x1B[?997;1n",
        ColorScheme::Dark => b"\x1B[?997;1n",
        // .light => "\x1B[?997;2n",
        ColorScheme::Light => b"\x1B[?997;2n",
    });
}
```

- [ ] **Step 4: Delegate the two existing hand-encoded sites** (`stream_terminal.rs:810-819`, `termio_impl.rs:1662-1667`) to the new encoder, exactly as the oracle commit's `stream_terminal.zig`/`Termio.zig` hunks do. Run `cargo test -p rustty-vt -p rustty-termio` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt crates/rustty-termio
git commit -m "vt: color scheme report encoder + delegate stream/termio DSR 996 arms (WS5 T7)"
```

---

### Task 8: Family C — vt-c color_scheme module + export + header + example

**Files:**

- Create: `crates/rustty-vt-c/src/color_scheme.rs`
- Modify: `crates/rustty-vt-c/src/lib.rs` (`pub mod color_scheme;` + wrapper + `ref_all_decls`)
- Create: `include/rustty/vt/color_scheme.h`
- Modify: `include/rustty/vt.h` (add `#include <rustty/vt/color_scheme.h>` and the `@ref color_scheme` group line + `@example c-vt-color-scheme/src/main.c` block)
- Create: `example/c-vt-color-scheme/` (README.md, build.sh or the build files matching sibling rustty examples — copy the shape of `example/c-vt-effects/`, NOT the oracle's zig build files)

**Interfaces:**

- Produces: `pub unsafe fn report_encode(scheme: ColorScheme, out: *mut u8, out_len: usize, out_written: *mut usize) -> Result` and export `rustty_color_scheme_report_encode(scheme: c_int-compatible RusttyColorScheme, buf: *mut c_char, buf_len: usize, out_written: *mut usize) -> c_int`.

- [ ] **Step 1: Read the oracle module**

```bash
git -C /Users/gael/projects/ghostty show origin/main:src/terminal/c/color_scheme.zig
git -C /Users/gael/projects/ghostty show origin/main:include/ghostty/vt/color_scheme.h
git -C /Users/gael/projects/ghostty show origin/main:example/c-vt-color-scheme/src/main.c
```

- [ ] **Step 2: Failing tests** (new file `crates/rustty-vt-c/src/color_scheme.rs`, tests ported 1:1 — the oracle has 5):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::Result;
    use rustty_vt::device_status::{ColorScheme, MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE};

    // test "encode color scheme report dark"
    #[test]
    fn dark() {
        let mut buf = [0u8; MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE];
        let mut written = 0usize;
        let result = unsafe {
            report_encode(ColorScheme::Dark, buf.as_mut_ptr(), buf.len(), &mut written)
        };
        assert_eq!(Result::Success, result);
        assert_eq!(b"\x1B[?997;1n", &buf[..written]);
    }

    // test "encode color scheme report light"
    #[test]
    fn light() {
        let mut buf = [0u8; MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE];
        let mut written = 0usize;
        let result = unsafe {
            report_encode(ColorScheme::Light, buf.as_mut_ptr(), buf.len(), &mut written)
        };
        assert_eq!(Result::Success, result);
        assert_eq!(b"\x1B[?997;2n", &buf[..written]);
    }

    // test "encode color scheme report with null buffer"
    #[test]
    fn null_buffer() {
        let mut written = 0usize;
        let result =
            unsafe { report_encode(ColorScheme::Dark, std::ptr::null_mut(), 0, &mut written) };
        assert_eq!(Result::OutOfSpace, result);
        assert_eq!(9, written);
    }

    // test "encode color scheme report with insufficient buffer"
    #[test]
    fn insufficient_buffer() {
        let mut buf = [0u8; 3];
        let mut written = 0usize;
        let result = unsafe {
            report_encode(ColorScheme::Light, buf.as_mut_ptr(), buf.len(), &mut written)
        };
        assert_eq!(Result::OutOfSpace, result);
        assert_eq!(9, written);
    }

    // test "encode color scheme report with exact buffer"
    #[test]
    fn exact_buffer() {
        let mut buf = [0u8; 9];
        let mut written = 0usize;
        let result = unsafe {
            report_encode(ColorScheme::Dark, buf.as_mut_ptr(), buf.len(), &mut written)
        };
        assert_eq!(Result::Success, result);
        assert_eq!(9, written);
    }
}
```

(`Result::OutOfSpace` exists — it is the established `out_of_space` projection used by `focus`/`modes` encoders; verify the exact variant name in `crates/rustty-vt-c/src/result.rs` before writing.)

- [ ] **Step 3: Run to fail**, then implement the module:

```rust
//! rustty-vt-c color_scheme
//!
//! PORTS: ghostty src/terminal/c/color_scheme.zig (BL-5 f00e90694)

use crate::result::Result;
use rustty_vt::device_status::{self, ColorScheme};

// pub fn report_encode(scheme, out_, out_len, out_written) callconv(...) Result
/// # Safety
/// `out_`, if non-null, must be writable for `out_len` bytes; `out_written`
/// must be a valid writable pointer.
pub unsafe fn report_encode(
    scheme: ColorScheme,
    out_: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> Result {
    // PORT NOTE: the reference encodes into a fixed writer and, on
    // error.WriteFailed, re-runs against a discarding writer to compute the
    // required size. rustty encodes into a Vec (encoder is infallible) and
    // compares lengths — byte-identical outputs and identical Result mapping.
    let mut buf = Vec::with_capacity(device_status::MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE);
    device_status::encode_color_scheme_report(&mut buf, scheme);
    if out_.is_null() || out_len < buf.len() {
        // SAFETY: out_written validity is the caller's contract.
        unsafe { *out_written = buf.len() };
        return Result::OutOfSpace;
    }
    // SAFETY: caller contract (out_ writable for out_len >= buf.len()).
    unsafe {
        std::ptr::copy_nonoverlapping(buf.as_ptr(), out_, buf.len());
        *out_written = buf.len();
    }
    Result::Success
}
```

lib.rs additions: `pub mod color_scheme;`, plus the wrapper (the C `scheme` arrives as the `RusttyColorScheme` c_int enum — guard the range on the boundary like `rustty_build_info` does):

```rust
// pub const color_scheme_report_encode = color_scheme.report_encode;
//   (@export name ghostty_color_scheme_report_encode)
/// C: rustty_color_scheme_report_encode
/// # Safety
/// `buf`, if non-null, writable for `buf_len`; `out_written` valid writable.
#[no_mangle]
pub unsafe extern "C" fn rustty_color_scheme_report_encode(
    scheme: c_int,
    buf: *mut u8,
    buf_len: usize,
    out_written: *mut usize,
) -> c_int {
    use rustty_vt::device_status::ColorScheme;
    // Integer-range guard on the raw c_int crossing the C boundary
    // (RusttyColorScheme: LIGHT=0, DARK=1 — see include/rustty/vt/device.h).
    let scheme = match scheme {
        0 => ColorScheme::Light,
        1 => ColorScheme::Dark,
        _ => return result::Result::InvalidValue.to_c_int(),
    };
    // SAFETY: caller contract.
    unsafe { color_scheme::report_encode(scheme, buf, buf_len, out_written) }.to_c_int()
}
```

**Verify** the `RusttyColorScheme` numeric values in `include/rustty/vt/device.h:78` (light/dark order) before hardcoding 0/1 — they must match the enum the existing `ColorSchemeFn` callback already uses.

- [ ] **Step 4: Header + umbrella + example.** Create `include/rustty/vt/color_scheme.h` as the R2-renamed oracle header (content read in Step 1 — 73 lines; includes become `<rustty/vt/types.h>` and `<rustty/vt/device.h>`). Add the include + group + example refs to `include/rustty/vt.h`. Create `example/c-vt-color-scheme/` by translating the oracle example's `src/main.c` (rename `ghostty/vt.h`→`rustty/vt.h`, `ghostty_`→`rustty_`, `GHOSTTY_`→`RUSTTY_`) and copying the `README.md`+build-script shape from `example/c-vt-effects/` (see `example/AGENTS.md`).

- [ ] **Step 5: Run everything**

```bash
cargo test -p rustty-vt-c --lib color_scheme::
cargo run -p xtask -- header-gen
cargo build -p rustty-vt-c --release && cargo build -p rustty-vt-c --target wasm32-unknown-unknown --release
bash .port/tools/gates/check_rustty_vt_headers.sh
bash example/c-vt-color-scheme/build.sh && ./example/c-vt-color-scheme/<binary as sibling examples name it>
```

Expected: tests PASS; gate GREEN; example prints the encoded report (oracle example output: the escape sequence for the dark scheme — `\x1B[?997;1n`). PORT NOTE: `.port/tools/cvt-parity.sh c-vt-color-scheme` will report BLOCKED (the pinned oracle worktree predates this example) — record that in the commit message rather than moving the pin.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "vt-c: color scheme report encoder — rustty_color_scheme_report_encode + header + example (WS5 T8)"
```

---

### Task 9: Family D engine — OSC/Kitty color-query reporting (`14c829883`)

**Files:**

- Modify: `crates/rustty-vt/src/color.rs` (`encode_rgb8`, `encode_rgb16`)
- Modify: `crates/rustty-vt/src/terminal.rs` (`color_for_xterm`, `color_for_kitty`)
- Modify: `crates/rustty-vt/src/kitty/color.rs` (`Kind::has_terminal_query_color`)
- Modify: `crates/rustty-vt/src/osc/mod.rs` (reset releases `color_operation.requests`)
- Modify: `crates/rustty-vt/src/stream_terminal.rs` (`color_operation` arm: query reporting; `kitty_color_operation`: query reporting)
- Test: each modified file's `mod tests`

**Interfaces:**

- Produces: `RGB::encode_rgb8(&self, out: &mut Vec<u8>)`, `RGB::encode_rgb16(&self, out: &mut Vec<u8>)`; `Terminal::color_for_xterm(&self, target: osc::parsers::color::Target) -> Option<RGB>`; `Terminal::color_for_kitty(&self, key: kitty::color::Kind) -> Option<RGB>`; `kitty::color::Kind::has_terminal_query_color(&self) -> bool`.

- [ ] **Step 1: Read the full oracle commit** (`git -C /Users/gael/projects/ghostty show 14c829883`) — the hunks were captured during plan research: Terminal helpers, RGB encoders, kitty `hasTerminalQueryColor`, the osc.zig reset fix, and the ~189-line stream_terminal reporting rework (including `colorOperation` gaining `terminator` and dropping the unused `op` param).

- [ ] **Step 2: Failing engine tests** (translate the oracle's new tests):

In `color.rs`:

```rust
// test "RGB: encode"
#[test]
fn rgb_encode() {
    let rgb = RGB { r: 0x01, g: 0x23, b: 0xff };
    let mut buf = Vec::new();
    rgb.encode_rgb8(&mut buf);
    assert_eq!(b"rgb:01/23/ff", &buf[..]);
    buf.clear();
    rgb.encode_rgb16(&mut buf);
    assert_eq!(b"rgb:0101/2323/ffff", &buf[..]);
}
```

In `kitty/color.rs`:

```rust
#[test]
fn kind_has_terminal_query_color() {
    assert!(Kind::Palette(42).has_terminal_query_color());
    assert!(Kind::Special(Special::Foreground).has_terminal_query_color());
    assert!(!Kind::Special(Special::SelectionBackground).has_terminal_query_color());
}
```

(Adjust variant names to the existing rustty `Kind`/special enum — read `crates/rustty-vt/src/kitty/color.rs` first.)

In `stream_terminal.rs`, port the commit's new stream tests (OSC 4/10/11/12 query → write_pty reply with 16-bit rgb + preserved terminator; kitty OSC 21 query reporting; cursor falls back to foreground). The oracle test names/behaviors to port (find them in the commit diff): every `test "..."` added to `stream_terminal.zig` by `14c829883`, assertion-for-assertion, using the crate's existing stream-test scaffolding (there are existing write_pty-capturing tests in the file — mirror their setup).

- [ ] **Step 3: Run to fail**, then implement, hunk by hunk:

`color.rs`:

```rust
// pub fn encodeRgb8(self: RGB, writer: *std.Io.Writer) !void
pub fn encode_rgb8(&self, out: &mut Vec<u8>) {
    // try writer.print("rgb:{x:0>2}/{x:0>2}/{x:0>2}", .{self.r, self.g, self.b});
    use std::io::Write as _;
    let _ = write!(out, "rgb:{:02x}/{:02x}/{:02x}", self.r, self.g, self.b);
}

// pub fn encodeRgb16(self: RGB, writer: *std.Io.Writer) !void
pub fn encode_rgb16(&self, out: &mut Vec<u8>) {
    // 8->16 bit expansion: v * 257 (0xFF -> 0xFFFF)
    use std::io::Write as _;
    let _ = write!(
        out,
        "rgb:{:04x}/{:04x}/{:04x}",
        u16::from(self.r) * 257,
        u16::from(self.g) * 257,
        u16::from(self.b) * 257,
    );
}
```

`terminal.rs` — direct translation of `colorForXterm`/`colorForKitty` (the Zig bodies are in the research capture above; `self.colors.palette.current[i]`, `foreground/background.get()`, cursor falls back to foreground for xterm, NO fallback for kitty). Place them next to the existing `Colors` impl (~line 201).

`kitty/color.rs` — `has_terminal_query_color` per the Zig switch.

`osc/mod.rs` — in the parser reset path, add the `color_operation` arm releasing `requests` alongside the existing `kitty_color_protocol` arm (find the reset `match` mirroring `osc.zig:405`).

`stream_terminal.rs` — rework `color_operation` handling:

1. The arm passes `&value.requests` + `value.terminator` (drop `op`).
2. In the query loop: `if self.effects.write_pty.is_none() { continue; }` → `let c = self.terminal.color_for_xterm(target) else continue;` → append `write_xterm_color_report`.
3. Add the private helper:

```rust
// fn writeXtermColorReport(writer, target, c, terminator) !void
fn write_xterm_color_report(
    out: &mut Vec<u8>,
    target: &osc_color::Target,
    c: RGB,
    terminator: osc::Terminator,
) {
    use std::io::Write as _;
    match target {
        // .palette => |i| "\x1b]4;{d};" ++ rgb16 ++ terminator
        osc_color::Target::Palette(i) => {
            let _ = write!(out, "\x1b]4;{};", i);
            c.encode_rgb16(out);
            out.extend_from_slice(terminator.string());
        }
        osc_color::Target::Dynamic(dynamic) => match dynamic {
            // foreground(10)/background(11)/cursor(12): "\x1b]{d};" ++ rgb16 ++ term
            d @ (Dynamic::Foreground | Dynamic::Background | Dynamic::Cursor) => {
                let _ = write!(out, "\x1b]{};", *d as u32);
                c.encode_rgb16(out);
                out.extend_from_slice(terminator.string());
            }
            _ => {}
        },
        osc_color::Target::Special(_) => {}
    }
}
```

4. `kitty_color_operation`: build the response buffer, and for `.query` items report terminal-backed keys via `color_for_kitty` (empty value when unset) — port the oracle's loop verbatim (read the full hunk: `git -C /Users/gael/projects/ghostty show 14c829883 -- src/terminal/stream_terminal.zig`).
5. The batched response is sent once via the existing `write_pty` path when non-empty (the Zig `Writer.Allocating` + `toOwnedSliceSentinel` becomes a `Vec<u8>` handed to `self.write_pty(&resp)`).

Adapt names to the file's existing ports: `osc_color` = `crate::osc::parsers::color`, `Terminator` = whatever `osc::Terminator` is called (verify `terminator.string()` analog; grep `Terminator` in `crates/rustty-vt/src/osc/`).

- [ ] **Step 4: Run** `cargo test -p rustty-vt` → PASS (including the new stream tests).
- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt
git commit -m "vt: report OSC 4/10/11/12 + kitty OSC 21 color queries via write_pty (WS5 T9)"
```

---

### Task 10: Family D — vt-c C-level query KAT

`14c829883` added ~40 lines of C API tests (`src/terminal/c/terminal.zig`) proving the query replies flow through a C `write_pty` callback. No new export is involved — this is coverage.

**Files:**

- Modify: `crates/rustty-vt-c/src/terminal.rs` (`mod tests`)

- [ ] **Step 1: Read the oracle tests**: `git -C /Users/gael/projects/ghostty show 14c829883 -- src/terminal/c/terminal.zig`
- [ ] **Step 2: Port them** into `crates/rustty-vt-c/src/terminal.rs` `mod tests`, following the file's existing C-callback test pattern (static capture struct + `extern "C" fn` callback + `set(t, Option_::WritePty, ...)` + `vt_write` of the OSC query + assert the captured reply bytes, e.g. `\x1b]4;1;rgb:cccc/6666/6666\x1b\\` for the default palette entry 1 — take exact expected bytes from the oracle test).
- [ ] **Step 3: Run** `cargo test -p rustty-vt-c --lib terminal::` → PASS.
- [ ] **Step 4: Commit**

```bash
git add crates/rustty-vt-c/src/terminal.rs
git commit -m "vt-c: C-level KATs for OSC/kitty color query reporting (WS5 T10)"
```

---

### Task 11: Family E — unicode codepoint width (`fc5a72772`)

**Files:**

- Modify: `crates/rustty-unicode/src/lib.rs`
- Test: same file

**Interfaces:**

- Produces: `rustty_unicode::codepoint_width(cp: u32) -> u8` (0/1/2; total over u32 — values > 0x10FFFF handled by the CALLER in vt-c per the oracle split: the module fn takes a valid `u21`-range value). Mirror the oracle exactly: engine fn `codepoint_width(cp: u32) -> u8` asserting/assuming `cp <= 0x10FFFF` is NOT what the oracle does — Zig's is `fn codepointWidth(cp: u21) u2 { return table.get(cp).width; }` with the `> 0x10FFFF → 1` guard living in `c/unicode.zig`. Keep that split: engine fn takes `u32` but documents the ≤ 0x10FFFF contract and indexes the table (the existing `table.get` analog in `rustty-unicode` — grep `props_table`/`lut` for the getter the print path uses).

- [ ] **Step 1: Read the oracle**: `git -C /Users/gael/projects/ghostty show fc5a72772`
- [ ] **Step 2: Failing tests** (port `test "codepointWidth"` — the full assertion list is in the commit):

```rust
// test "codepointWidth" (oracle fc5a72772)
#[test]
fn codepoint_width_table() {
    // Narrow (width 1)
    assert_eq!(1, codepoint_width('a' as u32));
    assert_eq!(1, codepoint_width(' ' as u32));
    assert_eq!(1, codepoint_width(0x10FFFF));
    // C0/C1 controls (width 0)
    assert_eq!(0, codepoint_width(0x00));
    // Combining mark (width 0)
    assert_eq!(0, codepoint_width(0x0301));
    // Wide (width 2)
    assert_eq!(2, codepoint_width(0x4E00));
    assert_eq!(2, codepoint_width(0x1F600));
    // ...port every remaining assertion from the oracle test verbatim...
}
```

- [ ] **Step 3: Implement** (doc comment ported from the commit, gag-free):

```rust
/// Returns the terminal display width of a codepoint in terminal
/// grid cells: 0, 1, or 2. Same width table the terminal print path uses.
/// Caller must pass cp <= 0x10FFFF (the C wrapper guards).
// pub fn codepointWidth(cp: u21) u2 { return table.get(cp).width; }
pub fn codepoint_width(cp: u32) -> u8 {
    props::table_get(cp).width
}
```

(`props::table_get` = the actual table getter used by the grapheme/props code in `crates/rustty-unicode` — locate it first: `grep -rn "fn get" crates/rustty-unicode/src/props_table.rs crates/rustty-unicode/src/lut.rs`.)

- [ ] **Step 4: Run** `cargo test -p rustty-unicode` → PASS. **Commit**

```bash
git add crates/rustty-unicode
git commit -m "unicode: codepoint_width — the print-path width table as a public API (WS5 T11)"
```

---

### Task 12: Family E — grapheme width kernel + print-path sharing (`65e61282a`)

**Files:**

- Modify: `crates/rustty-unicode/src/grapheme.rs` (kernel + measure fn)
- Modify: `crates/rustty-vt/src/terminal.rs` (print path uses the shared kernel)
- Test: both files

**Interfaces:**

- Produces: `pub enum GraphemeWidthEffect { Ignore, NoChange, Wide, Narrow }`; `pub fn grapheme_width_effect(prev: u32, cp: u32) -> GraphemeWidthEffect` (inline kernel); `pub struct GraphemeWidth { pub len: usize, pub width: u8 }`; `pub fn grapheme_width(cps: &[u32]) -> GraphemeWidth` (u32 input, invalid-codepoint guards per the oracle's `check_invalid` branch — rustty's `grapheme_break` already takes `u32`).

- [ ] **Step 1: Read the oracle**: `git -C /Users/gael/projects/ghostty show 65e61282a` (kernel, measure fn, Terminal.zig print refactor, and the width-parity test helper are captured in this plan's research; the full `grapheme.zig` test block must be read from the commit).
- [ ] **Step 2: Failing tests** in `grapheme.rs` — port every `test` the commit adds to `grapheme.zig` (185-line hunk; includes emoji-sequence, VS15/VS16, invalid-u32, empty-slice cases), e.g.:

```rust
#[test]
fn grapheme_width_emoji_zwj_sequence() {
    // 👨‍👩‍👧 = width 2, consumes all 5 codepoints
    let cps = [0x1F468, 0x200D, 0x1F469, 0x200D, 0x1F467];
    let r = grapheme_width(&cps);
    assert_eq!(r.len, 5);
    assert_eq!(r.width, 2);
}

#[test]
fn grapheme_width_invalid_u32() {
    let r = grapheme_width(&[0x110000, 0x0301]);
    assert_eq!((r.len, r.width), (1, 1));
    let r = grapheme_width(&['a' as u32, 0x110000]);
    assert_eq!((r.len, r.width), (1, 1));
}

#[test]
fn grapheme_width_empty() {
    let r = grapheme_width(&[]);
    assert_eq!((r.len, r.width), (0, 0));
}
```

- [ ] **Step 3: Implement the kernel + measure fn** (translate the Zig captured in research):

```rust
/// Width change requested by a codepoint that continues a grapheme cluster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphemeWidthEffect {
    /// Do not append the codepoint; leave break state as before seeing it.
    Ignore,
    /// Append the codepoint; cluster width unchanged.
    NoChange,
    /// Cluster becomes two terminal cells.
    Wide,
    /// Cluster becomes one terminal cell.
    Narrow,
}

/// Result of measuring the first grapheme cluster in a codepoint slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphemeWidth {
    pub len: usize,
    pub width: u8,
}

/// Shared width-decision kernel for the streaming printer and grapheme_width.
/// Assumes grapheme_break already said there is no break between prev and cp.
#[inline]
pub fn grapheme_width_effect(prev: u32, cp: u32) -> GraphemeWidthEffect {
    // VS16 wide / VS15 narrow, only for valid emoji variation bases.
    if cp == 0xFE0F || cp == 0xFE0E {
        let prev_props = table_get(prev);
        if !prev_props.emoji_vs_base {
            return GraphemeWidthEffect::Ignore;
        }
        return match cp {
            0xFE0F => GraphemeWidthEffect::Wide,
            0xFE0E => GraphemeWidthEffect::Narrow,
            _ => unreachable!(),
        };
    }
    if !table_get(cp).width_zero_in_grapheme {
        return GraphemeWidthEffect::Wide;
    }
    GraphemeWidthEffect::NoChange
}

#[inline]
fn invalid_codepoint(cp: u32) -> bool {
    cp > 0x10FFFF
}

/// Measures the first grapheme cluster using the same segmentation and width
/// rules as Terminal::print with mode 2027. Not a streaming API.
pub fn grapheme_width(cps: &[u32]) -> GraphemeWidth {
    // PORT NOTE: the reference is generic over u21/u32 with comptime-dead
    // invalid checks for u21. rustty callers are all u32; the checks are live.
    if cps.is_empty() {
        return GraphemeWidth { len: 0, width: 0 };
    }
    if invalid_codepoint(cps[0]) {
        return GraphemeWidth { len: 1, width: 1 };
    }

    let mut len: usize = 1;
    let mut width = table_get(cps[0]).width;
    let mut prev: u32 = cps[0];
    let mut state = BreakState::default();

    while len < cps.len() {
        if invalid_codepoint(cps[len]) {
            break;
        }
        let cp = cps[len];
        let saved_state = state;
        if grapheme_break(prev, cp, &mut state) {
            break;
        }
        match grapheme_width_effect(prev, cp) {
            GraphemeWidthEffect::Ignore => {
                // Do not append; restore break state and prev.
                state = saved_state;
            }
            GraphemeWidthEffect::NoChange => prev = cp,
            GraphemeWidthEffect::Wide => {
                width = 2;
                prev = cp;
            }
            GraphemeWidthEffect::Narrow => {
                width = 1;
                prev = cp;
            }
        }
        len += 1;
    }

    GraphemeWidth { len, width }
}
```

IMPORTANT fidelity check: the loop's exact `.ignore` semantics (whether len advances past an ignored VS — it DOES: the codepoint is consumed but not appended to the cluster state) must match the oracle's loop body — read the full `graphemeWidth` while-loop in the commit before finalizing (the research capture cuts off mid-loop). `table_get`/`BreakState`/`grapheme_break` = the file's existing items (`grapheme_break(cp1: u32, cp2: u32, state: &mut BreakState) -> bool` verified at `grapheme.rs:45`).

- [ ] **Step 4: Refactor the rustty-vt print path** — in `crates/rustty-vt/src/terminal.rs`'s `print`, find the mode-2027 no-break branch containing the inline VS16/VS15 + `width_zero_in_grapheme` logic (search `0xFE0F`), and replace it with a `match rustty_unicode::grapheme::grapheme_width_effect(previous_codepoint, c)` whose `Ignore` arm `return`s and whose `Wide`/`Narrow`/`NoChange` arms keep the existing bodies — exactly the Terminal.zig hunk in the research capture. Also port the commit's `expectGraphemeWidthParity` test helper + its invocations into the terminal tests (printer cursor advance == buffered `grapheme_width` measurement).
- [ ] **Step 5: Run** `cargo test -p rustty-unicode -p rustty-vt` → PASS (the full vt suite guards against print-path regressions).
- [ ] **Step 6: Commit**

```bash
git add crates/rustty-unicode crates/rustty-vt
git commit -m "unicode+vt: grapheme width kernel shared by print path + grapheme_width API (WS5 T12)"
```

---

### Task 13: Family E — vt-c unicode module + exports + header

**Files:**

- Create: `crates/rustty-vt-c/src/unicode.rs`
- Modify: `crates/rustty-vt-c/src/lib.rs` (`pub mod unicode;` + 2 wrappers + ref_all_decls)
- Modify: `crates/rustty-vt-c/Cargo.toml` (add `rustty-unicode = { workspace = true }` if not already a transitive-direct dep — check first; R4 acyclic is safe: rustty-unicode is below vt)
- Create: `include/rustty/vt/unicode.h`
- Modify: `include/rustty/vt.h`

**Interfaces:**

- Produces exports: `rustty_unicode_codepoint_width(cp: u32) -> u8`, `rustty_unicode_grapheme_width(cps: *const u32, len: usize, width: *mut u8) -> usize`.

- [ ] **Step 1: Failing tests** — port `origin/main:src/terminal/c/unicode.zig`'s 8 tests (full source captured in research) into the new module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // test "codepoint_width narrow" / "wide" / "zero" / "out of range"
    #[test]
    fn codepoint_width_cases() {
        assert_eq!(1, codepoint_width('a' as u32));
        assert_eq!(2, codepoint_width(0x4E00));
        assert_eq!(0, codepoint_width(0x0301));
        assert_eq!(1, codepoint_width(0x110000));
        assert_eq!(1, codepoint_width(u32::MAX));
    }

    // test "grapheme_width empty"
    #[test]
    fn grapheme_width_empty() {
        let mut width: u8 = 42;
        assert_eq!(0, unsafe { grapheme_width(std::ptr::null(), 0, &mut width) });
        assert_eq!(0, width);
    }

    // test "grapheme_width null width"
    #[test]
    fn grapheme_width_null_width() {
        let cps: [u32; 2] = [0x2764, 0xFE0F];
        assert_eq!(2, unsafe {
            grapheme_width(cps.as_ptr(), cps.len(), std::ptr::null_mut())
        });
    }

    // test "grapheme_width out of range"
    #[test]
    fn grapheme_width_out_of_range() {
        let mut width: u8 = 0;
        let invalid_first: [u32; 2] = [0x110000, 0x0301];
        assert_eq!(1, unsafe {
            grapheme_width(invalid_first.as_ptr(), invalid_first.len(), &mut width)
        });
        assert_eq!(1, width);
        let invalid_second: [u32; 2] = ['a' as u32, 0x110000];
        assert_eq!(1, unsafe {
            grapheme_width(invalid_second.as_ptr(), invalid_second.len(), &mut width)
        });
        assert_eq!(1, width);
    }

    // test "grapheme_width emoji sequence"
    #[test]
    fn grapheme_width_emoji_sequence() {
        let mut width: u8 = 0;
        let cps: [u32; 5] = [0x1F468, 0x200D, 0x1F469, 0x200D, 0x1F467];
        assert_eq!(cps.len(), unsafe {
            grapheme_width(cps.as_ptr(), cps.len(), &mut width)
        });
        assert_eq!(2, width);
    }
}
```

- [ ] **Step 2: Run to fail**, then implement:

```rust
//! rustty-vt-c unicode
//!
//! PORTS: ghostty src/terminal/c/unicode.zig (BL-5 fc5a72772 + 65e61282a)

// pub fn codepoint_width(cp: u32) callconv(lib.calling_conv) u8
pub fn codepoint_width(cp: u32) -> u8 {
    // if (cp > 0x10FFFF) return 1;
    if cp > 0x10FFFF {
        return 1;
    }
    rustty_unicode::codepoint_width(cp)
}

// pub fn grapheme_width(cps: ?[*]const u32, len: usize, width: ?*u8) ... usize
/// # Safety
/// `cps`, if non-null, must be readable for `len` u32s; `width` writable or null.
pub unsafe fn grapheme_width(cps: *const u32, len: usize, width: *mut u8) -> usize {
    if len == 0 {
        if !width.is_null() {
            // SAFETY: caller contract.
            unsafe { *width = 0 };
        }
        return 0;
    }
    if cps.is_null() {
        if !width.is_null() {
            // SAFETY: caller contract.
            unsafe { *width = 0 };
        }
        return 0;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(cps, len) };
    let result = rustty_unicode::grapheme::grapheme_width(slice);
    if !width.is_null() {
        // SAFETY: caller contract.
        unsafe { *width = result.width };
    }
    result.len
}
```

lib.rs wrappers:

```rust
// pub const unicode_codepoint_width = unicode.codepoint_width;
//   (@export name ghostty_unicode_codepoint_width)
/// C: rustty_unicode_codepoint_width
#[no_mangle]
pub extern "C" fn rustty_unicode_codepoint_width(cp: u32) -> u8 {
    unicode::codepoint_width(cp)
}

// pub const unicode_grapheme_width = unicode.grapheme_width;
/// C: rustty_unicode_grapheme_width
/// # Safety
/// `cps`, if non-null, readable for `len` u32s; `width` writable or null.
#[no_mangle]
pub unsafe extern "C" fn rustty_unicode_grapheme_width(
    cps: *const u32,
    len: usize,
    width: *mut u8,
) -> usize {
    // SAFETY: caller contract.
    unsafe { unicode::grapheme_width(cps, len, width) }
}
```

If `rustty-vt` re-exports the unicode crate (the oracle's `lib_vt.zig` exposes `vt.unicode.codepointWidth/graphemeWidth`), add the matching re-export block to `crates/rustty-vt/src/lib.rs`:

```rust
// lib_vt.zig: pub const unicode = struct { codepointWidth, graphemeWidth, ... }
pub mod unicode {
    pub use rustty_unicode::codepoint_width;
    pub use rustty_unicode::grapheme::{grapheme_width, GraphemeWidth, GraphemeWidthEffect};
}
```

(only if it doesn't conflict with an existing `unicode` item in rustty-vt — check first; if rustty-vt already depends on and re-exports rustty-unicode under another path, keep that path and skip this).

- [ ] **Step 3: Create `include/rustty/vt/unicode.h`** — R2-renamed `origin/main:include/ghostty/vt/unicode.h` (170 lines, read it in full), and add `#include <rustty/vt/unicode.h>` + the `@ref unicode` group row to `include/rustty/vt.h`.
- [ ] **Step 4: Run + gate**

```bash
cargo test -p rustty-vt-c --lib unicode::
cargo run -p xtask -- header-gen
cargo build -p rustty-vt-c --release && cargo build -p rustty-vt-c --target wasm32-unknown-unknown --release
bash .port/tools/gates/check_rustty_vt_headers.sh
```

Expected: PASS / GREEN.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "vt-c: unicode codepoint/grapheme width exports + unicode.h (WS5 T13)"
```

---

### Task 14: Family F engine — ScrollViewport::Row (`3a2e28329` engine half)

**Files:**

- Modify: `crates/rustty-vt/src/terminal.rs` (`ScrollViewport` at ~line 318 + `scroll_viewport`)
- Modify: `crates/rustty-vt/src/pagelist/` scrollbar doc comment (the PageList.zig doc hunk)
- Test: `crates/rustty-vt/src/terminal.rs`

**Interfaces:**

- Produces: `ScrollViewport::Row(usize)` variant; `Terminal::scroll_viewport` maps it to the PageList `Scroll::Row` (already existing at `pagelist/scroll.rs:91` per MAIN_PARITY).

- [ ] **Step 1: Read the oracle hunks** (captured in research: Terminal.zig adds `row: usize` + Tag entry + `.row => |row| .{ .row = row }` mapping; PageList.zig is doc-only).
- [ ] **Step 2: Failing engine test** — add a terminal-level test mirroring the C test semantics (scrollback of 4 rows in a 5x2 term; `scroll_viewport(Row(0))` shows "1\n2"; `Row(1)` shows "2\n3"; `Row(9999)` clamps to active "3\n4"; alt screen stays active):

```rust
#[test]
fn scroll_viewport_row() {
    // build a 5x2 terminal with max_scrollback 10_000 using the file's
    // existing test constructor; print "1\r\n2\r\n3\r\n4"
    // then:
    t.scroll_viewport(ScrollViewport::Row(0));
    assert_eq!(b"1\n2", plain_string(&mut t).as_slice());
    t.scroll_viewport(ScrollViewport::Row(1));
    assert_eq!(b"2\n3", plain_string(&mut t).as_slice());
    t.scroll_viewport(ScrollViewport::Row(9999));
    assert_eq!(b"3\n4", plain_string(&mut t).as_slice());
}
```

(Use the file's existing scroll_viewport test as the scaffold template — there is one for Top/Bottom/Delta.)

- [ ] **Step 3: Implement**: add the variant + doc comment + the `Row(row) => Scroll::Row(row)` arm (match the existing arms' exact target-enum names in the rustty pagelist scroll enum — `delta` maps to `DeltaRow` today; `Row` maps to the pre-existing `Row` variant). Apply the PageList scrollbar doc-comment replacement ("amortized O(1) ... cached") to the rustty scrollbar fn's comment.
- [ ] **Step 4: Run** `cargo test -p rustty-vt --lib terminal::` → PASS. **Commit**

```bash
git add crates/rustty-vt
git commit -m "vt: ScrollViewport::Row absolute-row viewport scrolling (WS5 T14)"
```

---

### Task 15: Family F — vt-c scroll row tag + header

**Files:**

- Modify: `crates/rustty-vt-c/src/terminal.rs` (C ScrollViewport tag enum + value union + mapping in `scroll_viewport`)
- Modify: `include/rustty/vt/terminal.h` (RUSTTY_SCROLL_VIEWPORT_ROW + `size_t row;` union member + SCROLLBAR doc correction)
- Test: `crates/rustty-vt-c/src/terminal.rs`

- [ ] **Step 1: Read** the C tag/union projection currently in `crates/rustty-vt-c/src/terminal.rs` (search `ScrollViewport`) — it mirrors Zig's `lib.TaggedUnion` as tag + `#[repr(C)]` union. Add the `Row = 3` tag and `row: usize` union member; extend the `scroll_viewport` mapping with `Row => ZigScrollViewport::Row(value.row)`.
- [ ] **Step 2: Failing tests** — port `test "scroll_viewport row"` and `test "scroll_viewport row alt screen"` (full Zig source captured in research) into `mod tests`, using `with_test_allocator` + the existing `new_t`/`plain_string` helpers + `get(t, Option_::ViewportActive/Scrollbar/ActiveScreen, ...)` keys (`ViewportActive = 32`, `Scrollbar = 9` — already exist in the getter Key enum). Assert the same scrollbar total/offset/len (4/1/2 then offset 2) and alt-screen clamping.
- [ ] **Step 3: Implement + run**: `cargo test -p rustty-vt-c --lib terminal::` → PASS.
- [ ] **Step 4: Header**: apply the `3a2e28329` terminal.h hunk (renamed): the `GHOSTTY_SCROLL_VIEWPORT_ROW` enum entry with its full doc block, the `size_t row;` union member, and the corrected `GHOSTTY_TERMINAL_DATA_SCROLLBAR` doc paragraph. Then regen + gate:

```bash
cargo run -p xtask -- header-gen
cargo build -p rustty-vt-c --release && cargo build -p rustty-vt-c --target wasm32-unknown-unknown --release
bash .port/tools/gates/check_rustty_vt_headers.sh
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "vt-c: RUSTTY_SCROLL_VIEWPORT_ROW absolute-row scrolling tag (WS5 T15)"
```

---

### Task 16: Family G engine — protocol-neutral clipboard writes (`0a410f18e` + `634ef7198`, FINAL state)

Port directly to the final protocol-neutral shape (`clipboard_write` effect with decoded MIME contents); record a PORT NOTE that the intermediate `clipboard_set(kind, base64)` form (`0a410f18e`) is subsumed.

**Files:**

- Create: `crates/rustty-vt/src/clipboard.rs`
- Modify: `crates/rustty-vt/src/lib.rs` (`pub mod clipboard;`)
- Modify: `crates/rustty-vt/src/stream_terminal.rs` (Effects field + `clipboard_contents` arm + decode)
- Test: `crates/rustty-vt/src/stream_terminal.rs`

**Interfaces:**

- Produces: `clipboard::Location` (`#[repr(i32)]`: Standard=0, Selection=1, Primary=2), `clipboard::Content<'a> { pub mime: &'a [u8], pub data: &'a [u8] }`, `clipboard::Write<'a> { pub location: Location, pub contents: &'a [Content<'a>] }`, `clipboard::WriteResult` (`#[repr(i32)]`: Success=0, Denied=1, Unsupported=2, Busy=3, InvalidData=4, IoError=5); `Effects.clipboard_write: Option<fn(&mut Handler<'a>, clipboard::Write) -> clipboard::WriteResult>`.

- [ ] **Step 1: Read both commits** and the final files:

```bash
git -C /Users/gael/projects/ghostty show 634ef7198
git -C /Users/gael/projects/ghostty show origin/main:src/terminal/clipboard.zig
```

- [ ] **Step 2: Create `crates/rustty-vt/src/clipboard.rs`** (direct translation of the 36-line file captured in research):

```rust
//! rustty-vt clipboard
//!
//! PORTS: ghostty src/terminal/clipboard.zig (BL-5 634ef7198)

/// The clipboard destination for a write.
// pub const Location = enum(c_int) { standard = 0, selection = 1, primary = 2, _ };
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    Standard = 0,
    Selection = 1,
    Primary = 2,
}

/// A single representation of clipboard data. Borrowed; binary-safe.
// pub const Content = struct { mime: []const u8, data: []const u8 };
#[derive(Clone, Copy, Debug)]
pub struct Content<'a> {
    pub mime: &'a [u8],
    pub data: &'a [u8],
}

/// One atomic clipboard write. Contents borrowed for the callback duration.
/// An empty contents slice clears the destination.
// pub const Write = struct { location: Location, contents: []const Content };
#[derive(Clone, Copy, Debug)]
pub struct Write<'a> {
    pub location: Location,
    pub contents: &'a [Content<'a>],
}

/// The result of a clipboard write.
// pub const WriteResult = enum(c_int) { success..io_error, _ };
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteResult {
    Success = 0,
    Denied = 1,
    Unsupported = 2,
    Busy = 3,
    InvalidData = 4,
    IoError = 5,
}
```

- [ ] **Step 3: Failing stream tests** — port `test "clipboard_write effect callback"` (the full final test from `634ef7198`, ~130 lines: null callback ignores; `\x1B]52;c;aGVsbG8=\x1B\\` forwards location=Standard, one content `mime=text/plain`, `data=hello`; BEL termination + `p` → Primary; `?` read requests never forwarded; empty payload → clear with empty contents; result plumbing) into `stream_terminal.rs` `mod tests` using the file's existing effect-callback test scaffolding.
- [ ] **Step 4: Run to fail**, then implement in `stream_terminal.rs`:
  1. `use crate::clipboard;` + `Effects` field `pub clipboard_write: Option<fn(&mut Handler<'a>, clipboard::Write) -> clipboard::WriteResult>` with the ported doc comment; add `clipboard_write: None` to `READONLY`.
  2. Route the OSC arm: `.clipboard_contents => self.clipboard_contents(kind, data)` (moving it out of the no-effect list — mirror the diff).
  3. The handler (translate the final Zig captured in research):

```rust
// fn clipboardContents(self: *Handler, kind: u8, data: []const u8) !void
fn clipboard_contents(&mut self, kind: u8, data: &[u8]) {
    let Some(func) = self.effects.clipboard_write else { return };

    // Read requests are deliberately not forwarded; see the effect docs.
    if data.len() == 1 && data[0] == b'?' {
        return;
    }

    let location = match kind {
        b's' => clipboard::Location::Selection,
        b'p' => clipboard::Location::Primary,
        _ => clipboard::Location::Standard,
    };

    // OSC 52 uses an empty payload to clear the selected clipboard.
    if data.is_empty() {
        let _ = func(self, clipboard::Write { location, contents: &[] });
        return;
    }

    // const decoder = std.base64.standard.Decoder; decode into a temp alloc.
    // PORT NOTE: rustty's std.base64 analog is rustty-simd's scalar decoder
    // (standard alphabet, padding REQUIRED — verify the decoder config matches
    // std.base64.standard before wiring; if only the no-pad variant exists,
    // add the padded variant to rustty-simd/base64_scalar with a KAT).
    let Some(decoded) = decode_base64_standard(data) else {
        // Zig: try decoder.decode -> error propagates out of the OSC dispatch
        // (invalid base64 aborts the effect silently at the stream level).
        return;
    };
    let contents = [clipboard::Content { mime: b"text/plain", data: &decoded }];
    let _ = func(self, clipboard::Write { location, contents: &contents });
}
```

FIDELITY CHECK before finalizing: the Zig `clipboardContents` is `!void` and the caller `try`s it — determine what the rustty OSC-dispatch arm does with handler errors today (the existing arms are infallible or log) and match the least-lossy behavior; document the decision in a PORT NOTE. The decode error path in Zig propagates `error.InvalidCharacter` up `nextSlice` — check how rustty's `next_slice` surface handles handler errors (it likely doesn't — the stream tests in the oracle only exercise VALID base64, so a silent-return PORT NOTE divergence is acceptable ONLY if flagged; prefer matching whatever the existing kitty/OSC error plumbing does). 4. **Borrow-check note:** `func(self, ...)` passes `&mut Handler` while `contents` borrows the locally-decoded Vec — this is fine (the Vec is a local, not a field). Do NOT stash raw pointers; keep everything as reborrowed slices (SB-clean by construction — no Miri-hostile aliasing introduced; memory: shaper-hook-sb-clean-interleave does not apply because no long-lived alias exists).

- [ ] **Step 5: Run** `cargo test -p rustty-vt --lib stream_terminal::` → PASS.
- [ ] **Step 6: Commit**

```bash
git add crates/rustty-vt
git commit -m "vt: protocol-neutral clipboard_write effect for OSC 52 (WS5 T16)"
```

---

### Task 17: Family G — vt-c clipboard C surface + Option 26 + trampoline

**Files:**

- Modify: `crates/rustty-vt-c/src/terminal.rs`
- Modify: `crates/rustty-vt-c/src/types.rs` (register `ClipboardContent`, `ClipboardWrite` + json-parses asserts)
- Modify: `include/rustty/vt/terminal.h` (the `634ef7198` + `0a410f18e` header hunks, renamed: `RusttyClipboardLocation`, `RusttyClipboardContent`, `RusttyClipboardWrite`, `RusttyClipboardWriteResult`, `RusttyTerminalClipboardWriteFn`, `RUSTTY_TERMINAL_OPT_CLIPBOARD_WRITE` row + option doc)
- Modify: `example/c-vt-effects/src/main.c` + `README.md` (the `effects-clipboard-write` snippet — oracle hunk in `634ef7198`)
- Modify: `.port/audits/GHOSTTY_LIFETIMES.tsv` (prospective rows)
- Test: `crates/rustty-vt-c/src/terminal.rs`

**Interfaces:**

- Produces (C surface, mirroring the Zig captured in research):

```rust
/// C: RusttyClipboardContent
// pub const ClipboardContent = extern struct { mime: lib.String, data: lib.String };
#[repr(C)]
pub struct ClipboardContent {
    pub mime: LibString,
    pub data: LibString,
}

/// C: RusttyClipboardWrite (sized struct pattern)
// pub const ClipboardWrite = extern struct { size, location, contents, contents_len };
#[repr(C)]
pub struct ClipboardWrite {
    pub size: usize,
    pub location: clipboard::Location,
    pub contents: *const ClipboardContent,
    pub contents_len: usize,
}

pub type ClipboardWriteFn =
    extern "C" fn(Terminal, *mut c_void, *const ClipboardWrite) -> clipboard::WriteResult;
```

(`LibString` = the existing `lib.String` projection already used by `EnquiryFn`/`XtversionFn` — same type.)

- [ ] **Step 1: Failing test** — port `test "set clipboard_write callback"` from `origin/main:src/terminal/c/terminal.zig` (read the full ~90-line test: static capture struct with `next_result`, asserts location/mime/data captured, result round-trip, `set(t, .clipboard_write, ...)`), into `mod tests` following the established `extern "C"` callback test pattern.
- [ ] **Step 2: Run to fail**, then implement:
  1. Add the two structs + fn type above (place next to the other Effects fn types, ~line 151).
  2. `Option_::ClipboardWrite = 26` + the `set_typed` arm `wrapper.effects.clipboard_write = value` (mirror the neighboring callback arms' null-handling).
  3. Add `clipboard_write` field to the vt-c `Effects` struct (the wrapper-side C callbacks struct at `terminal.rs:~100`).
  4. The trampoline — follow the existing trampoline pattern (container_of recovery via summed field offsets, `terminal.rs:238` comment block; memory: vtc-effects-trampoline-seam-d3):

```rust
// fn clipboardWriteTrampoline(handler: *Handler, write: clipboard.Write) WriteResult
pub(crate) fn clipboard_write_trampoline(
    handler: &mut Handler<'static>,
    write: rustty_vt::clipboard::Write<'_>,
) -> rustty_vt::clipboard::WriteResult {
    use rustty_vt::clipboard::WriteResult;
    // SAFETY: container_of recovery — same offset arithmetic as the 9
    // existing trampolines (Handler -> Stream -> TerminalWrapper).
    let wrapper: *mut TerminalWrapper = unsafe { wrapper_from_handler(handler) };
    // const func = wrapper.effects.clipboard_write orelse return .unsupported;
    let Some(func) = (unsafe { (*wrapper).effects.clipboard_write }) else {
        return WriteResult::Unsupported;
    };

    // Stack-first contents conversion (alloc only past 4 entries), exactly
    // mirroring the Zig (gpa alloc fallback -> .io_error on failure).
    let mut stack_contents: [ClipboardContent; 4] = /* zeroed init */;
    let heap_contents: Vec<ClipboardContent>;
    let contents: &[ClipboardContent] = if write.contents.len() <= 4 {
        for (dst, content) in stack_contents.iter_mut().zip(write.contents) {
            *dst = ClipboardContent {
                mime: LibString { ptr: content.mime.as_ptr(), len: content.mime.len() },
                data: LibString { ptr: content.data.as_ptr(), len: content.data.len() },
            };
        }
        &stack_contents[..write.contents.len()]
    } else {
        heap_contents = write.contents.iter().map(|content| ClipboardContent {
            mime: LibString { ptr: content.mime.as_ptr(), len: content.mime.len() },
            data: LibString { ptr: content.data.as_ptr(), len: content.data.len() },
        }).collect();
        &heap_contents
    };

    let request = ClipboardWrite {
        size: std::mem::size_of::<ClipboardWrite>(),
        location: write.location,
        contents: if contents.is_empty() { std::ptr::null() } else { contents.as_ptr() },
        contents_len: contents.len(),
    };
    func(wrapper as Terminal, unsafe { (*wrapper).effects.userdata }, &request)
}
```

(Adapt: `wrapper_from_handler` = the file's existing recovery helper name; `LibString` field names; `Terminal` cast form — copy exactly what `bell_trampoline`/`write_pty_trampoline` do. Zig's gpa-alloc fallback becomes a plain `Vec` — PORT NOTE: the Zig `catch return .io_error` OOM arm has no Rust analog because `Vec` aborts on OOM, consistent with DG5 abort policy.) 5. Install `clipboard_write: Some(Effects::clipboard_write_trampoline)` in the always-installed effects block where the other 9 trampolines are wired. 6. Register both structs in `types.rs` (rows `RusttyClipboardContent`, `RusttyClipboardWrite` in the oracle's positions) and extend the json-parses test with the `contains` + field asserts from the `634ef7198` types.zig hunk (size/location/contents/contents_len; mime/data).

- [ ] **Step 3: Run** `cargo test -p rustty-vt-c --lib` → PASS (new test + all existing terminal tests).
- [ ] **Step 4: Headers + example.** Apply the terminal.h hunks from `634ef7198` (types + option row + option doc + `@snippet` reference) with R2 renaming; update `example/c-vt-effects/src/main.c`/README with the renamed oracle snippet. Regen + gate + example build:

```bash
cargo run -p xtask -- header-gen
cargo build -p rustty-vt-c --release && cargo build -p rustty-vt-c --target wasm32-unknown-unknown --release
bash .port/tools/gates/check_rustty_vt_headers.sh
bash example/c-vt-effects/build.sh
.port/tools/cvt-parity.sh c-vt-effects || echo "BLOCKED expected if oracle pin predates the example change — record in commit"
```

- [ ] **Step 5: LIFETIMES ledger** — append prospective rows to `.port/audits/GHOSTTY_LIFETIMES.tsv` for the new borrowed-pointer C structs, following the file's existing row format (read its header lines first): `ClipboardWrite.contents` (borrowed, callback-duration), `ClipboardContent.mime/.data` (borrowed, callback-duration), `TerminalWrapper.effects.clipboard_write` (fn ptr, wrapper lifetime).
- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "vt-c: clipboard_write option + trampoline + C types + terminal.h + effects example (WS5 T17)"
```

---

### Task 18: Family H — selection-gesture export (`f245cdc66`) verification

The commit is a 1-line `lib_vt.zig` module re-export; the vt-c `selection_gesture` C surface is already fully ported (`rustty_selection_gesture_*` live). Verify and close the row without code.

- [ ] **Step 1: Verify no export gap**

```bash
git -C /Users/gael/projects/ghostty show f245cdc66
git -C /Users/gael/projects/ghostty show origin/main:src/lib_vt.zig | grep -oE 'ghostty_[a-z0-9_]+' | sort -u | sed 's/ghostty_/rustty_/' > /tmp/oracle_syms.txt
grep -oE 'fn rustty_[a-z0-9_]+' /Users/gael/projects/rustty/crates/rustty-vt-c/src/lib.rs | sed 's/fn //' | sort -u > /tmp/live_syms.txt
comm -23 /tmp/oracle_syms.txt /tmp/live_syms.txt
```

Expected after Tasks 2–17: the ONLY remaining lines are the BL-1/WS7 symbols (`rustty_terminal_compress`, `rustty_terminal_compression_activity`, `rustty_render_state_begin_update`, `rustty_render_state_end_update`). If any WS5-family symbol appears, go back and fix. Also check the Zig-module half: if `lib_vt.zig`'s non-export module surface (`pub const selectionGesture = ...`) has a rustty-vt `pub use` analog convention, confirm `rustty-vt`'s lib.rs already re-exports `selection_gesture` (it does — it's a ported module); note the row closes with "already covered" evidence in Task 20's MAIN_PARITY edit.

---

### Task 19: Full gate sweep

- [ ] **Step 1: Workspace tests**

```bash
cargo test -p rustty-unicode -p rustty-vt -p rustty-vt-c -p rustty-config -p rustty-termio 2>&1 | tail -20
```

Expected: all PASS.

- [ ] **Step 2: Header + ABI gates**

```bash
cargo run -p xtask -- header-gen && git diff --exit-code include/rustty.h || echo "regen drift — commit it"
bash .port/tools/gates/check_rustty_vt_headers.sh          # VT-HEADERS: GREEN
bash .port/tools/gates/check_embed_h_abi.sh                # A4: PASS (must be unaffected)
bash .port/tools/gates/check_acyclic.sh                    # R4: PASS
bash .port/tools/gates/check_symbol_manifest.sh || true    # A1 embed manifest: unchanged state (PROGRESS gate)
```

Expected: vt-headers GREEN; A4 PASS with the same coverage ledger as before (no new by-value returns); acyclic PASS. A1 reports the same pre-existing state (WS5 adds nothing to the embed header).

- [ ] **Step 3: Targeted Miri on the new stream seams** (clipboard_write borrows + color-query buffers) — run the vt stream tests under the repo's Miri runner:

```bash
bash .port/tools/miri-run.sh -p rustty-vt stream_terminal:: 2>&1 | tail -5
```

(Adapt to `miri-run.sh`'s actual CLI — read its header first. Pre-existing vt-search Miri RED (memory: search-screen-sb-ub-finding) is out of scope; only the stream_terminal lane must be clean.)

- [ ] **Step 4: Commit any regen/gate fixups**

```bash
git add -A && git commit -m "vt-c: WS5 gate sweep fixups — header regen + ledgers (WS5 T19)" || echo "nothing to commit"
```

---

### Task 20: Both-platform build gate + ledger updates

- [ ] **Step 1: macOS build**

```bash
cd /Users/gael/projects/rustty && cargo build --workspace 2>&1 | tail -3
```

Expected: green.

- [ ] **Step 2: Linux (VM) build** — push the branch and build on the VM:

```bash
git push -u origin ws5-libvt-capi
ssh rvm 'cd ~/projects/rustty && git fetch && git checkout ws5-libvt-capi && cargo build --workspace' 2>&1 | tail -3
```

Expected: green. Fix any non-darwin breakage on this branch before proceeding (standing lesson: xev `Loop::init` / `COMPLETION_SIZE` class).

- [ ] **Step 3: MAIN_PARITY.md** — edit `.port/phase-2/MAIN_PARITY.md`: flip the 10 BL-5 rows (lines listed in the research header: 68, 146, 147, 169, 172, 174, 177, 178, 179, 182 at time of writing — re-locate by hash) from `missing BL-5` to `ported (WS5, 2026-07-…)` with one-line evidence each (e.g. `rustty_color_parse @ vt-c lib.rs`, `clipboard.rs + clipboard_write effect`, `ScrollViewport::Row @ terminal.rs`). Update the BL-5 summary bullet (line ~53) to note the series is closed by WS5, and remove/adjust its stale evidence claims (`ScrollViewport` now HAS a `Row` variant; `clipboard_set` grep now hits). Check `.port/phase-2/DEFERRALS.md` / `VERIFICATION_DEBT.md` for any BL-5-adjacent rows and strike them in the same commit.
- [ ] **Step 4: Commit**

```bash
git add .port/phase-2
git commit -m "parity: mark BL-5 lib-vt C-API series ported (WS5 T20)"
```

---

### Task 21: Merge

- [ ] **Step 1:** REQUIRED SUB-SKILL: `superpowers:finishing-a-development-branch`. Merge `ws5-libvt-capi` → `main` only with BOTH platform builds green (re-run Task 20 Steps 1–2 on the merge result), then delete the branch (local + origin + the VM checkout back to main).
- [ ] **Step 2:** Update the roadmap checklist: tick `WS5 lib-vt C-API` in `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`'s plan-authoring checklist (and note completion).

---

## Self-Review Notes (done at authoring time)

1. **Spec coverage:** all 7 roadmap-named families have task groups (A=T2, B=T3–T6, C=T7–T8, D=T9–T10, E=T11–T13, F=T14–T15, G=T16–T17) + the H verification row (T18); the 13-export gap enumerated in the research header is exactly covered by T6 (10) + T8 (1) + T13 (2); gates/ledgers/platforms = T19–T21.
2. **A4 clarification vs roadmap wording:** the roadmap says "A4 gate freeze"; research shows A4 gates `rustty-embed.h` by-value returns only — WS5 adds none (the RGB fix is the opposite direction), so the correct action is "run A4, prove unchanged" (T19), with the vt-family analog gate (`check_rustty_vt_headers.sh` + `check-vt-struct-layouts.mjs` + `types.rs` registration) carrying the real freeze work (T6/T15/T17).
3. **Known judgment points left explicitly flagged in-task** (not placeholders — each has a bounded decision procedure): `RGB::parse` input type (`&str` vs `&[u8]`, T3/T5), base64 decoder padding parity (T16), handler-error plumbing for invalid base64 (T16), `RusttyColorScheme` discriminants (T8), rustty-vt `unicode` re-export path (T13).
