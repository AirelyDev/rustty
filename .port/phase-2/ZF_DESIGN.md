# ZF_DESIGN — faithful port of vendored `zf` 0.10.3 → `rustty-zf`

Status: **ready** (no blocker). Design phase only; no code written yet.

## 0. What this is and why

Ghostty's `cli/list_themes.zig` fuzzy-sorts the theme picker with the vendored
dependency **`zf`** (natecraddock/zf 0.10.3): `zf.rank(name, tokens, .{})` returns
`?f64`, and the picker sorts themes by that rank (lower = better) and drops
non-matches (`null`). `zf` is a *dependency package* (like `z2d` / `wuffs`), NOT
ghostty `src/`, so:

- Source of truth (read-only oracle):
  `/Users/gael/.cache/zig/p/zf-0.10.3-OIRy8RuJAACKA3Lohoumrt85nRbHwbpMcUaLES8vxDnh/src/zf/{zf,filter,clib}.zig`
  (zf.zig 152 LOC / 3 `test{}`, filter.zig 460 / 4, clib.zig 182 / 2).
- Target: a **new** crate `crates/rustty-zf` — pure-Rust leaf, **no `rustty-*`
  deps** (R4 acyclic), the vendored-dep-port pattern of `rustty-z2d` / `rustty-wuffs`.
- Because the base is the zig-cache dep dir (not ghostty `src/`), the crate is
  **outside** the 3234-test src/ contract; it is scoped-mapped into the count gate
  under its own GH base (like `rustty-wuffs` = `pkg/wuffs/src`, `rustty-xev` = the
  pinned libxev cache dir).

### Fidelity contract (R1)
The **rank algorithm must be bit-faithful**: `list_themes` sorts on the exact `f64`
and relies on the exact match / no-match (`?f64 == null`) decision. Every
line / comment / `test{}` is ported assertion-for-assertion. Zig byte-slices
(`[]const u8`) → `&[u8]` / `Vec<u8>`, **never lossy `String`** (zf matches over raw
bytes; `std.ascii.toLower` is ASCII-only). The 8 named `test{}` (esp. filter.zig's
`rankToken` and zf.zig's `highlight library interface` with pinned index arrays)
are the fidelity pins.

## 1. File → module map

| Zig ref | Rust module | Contents |
|---|---|---|
| `zf.zig` | `src/zf.rs` | `RankOptions`, `RankTokenOptions`, `rank`, `rankToken`, `highlight`, `highlightToken`; the 2 named library tests. Public façade over `filter`. |
| `filter.zig` | `src/filter.rs` | `indexOf`, `IndexIterator`, `hasSeparator`, `PathIterator`, `segmentLen`, `rankToken`, `FixedArrayList`, `highlightToken`, `isStartOfWord`, `ScanResult`, `scanToEnd`; the 4 named tests (`path iterator`, `segmentLen`, `rankToken`, `FixedArrayList`). The core scorer. |
| `clib.zig` | `src/clib.rs` | The C ABI: `rustty_zf_{rank,rank_token,highlight,highlight_token}` (0.10.3; `_needle` since S-10b, see the note under the export table) `#[no_mangle] pub extern "C"`; the 2 named C tests. |
| (crate root) | `src/lib.rs` | Crate doc; `mod filter; mod clib; pub mod zf;` (or flatten — see §7); the anonymous `test{}` refAllDecls → a `ref_all_decls` compile-touch `#[test]`. |
| `std.fs.path` (used by zf.zig + clib.zig) | inline in `src/filter.rs` | `SEP: u8 = b'/'` and `basename()` — a faithful inline of `std.fs.path.sep` / `std.fs.path.basenamePosix` (pure leaf ⇒ cannot use std shim). See §3. |

Module privacy mirrors Zig: `zf.zig`'s `pub fn`s are the crate's public API;
`filter.zig` is `pub` within zf but the crate can keep `mod filter` crate-private
and re-export only the `zf::*` surface (list_themes only needs `rank` / `highlight`).

## 2. The scoring algorithm — pin every operation (bit-faithful)

The scorer is `filter.scanToEnd` (a Smith-Waterman-ish greedy gap/bonus walk).
**All arithmetic is `f64` and every value that enters an add is an exact integer in
`f64` EXCEPT the two coverage divisions — so op order must be preserved verbatim.**

### 2.1 `scanToEnd(str, token, start_index, offset, matched_indices?, case_sensitive, strict_path) -> ?ScanResult`
```
rank: f64 = 1
last_index = start_index
last_sequential = false
if start_index > 0 and !isStartOfWord(str[start_index-1]): rank += 2.0   // not on word boundary
for c in token:
    index = indexOf(str, last_index+1, c, case_sensitive)   // haystack lowered on the fly, NOT c
    if index is null: return null
    idx = index
    if strict_path and hasSeparator(str[last_index ..= idx]): return null   // match spanned a '/'
    matched_indices?.append(idx + offset)
    if idx == last_index + 1:
        if !last_sequential: last_sequential = true; rank += 1.0   // sequential: only first char scores
    else:
        if !isStartOfWord(str[idx-1]): rank += 2.0                 // gap not landing on word boundary
        last_sequential = false
        rank += (idx - last_index) as f64                          // gap penalty = distance
    last_index = idx
return ScanResult{ rank, index: last_index + 1 }
```
- `isStartOfWord(byte)` = `byte ∈ { SEP('/'), '_', '-', '.', ' ' }`.
- `ScanResult { rank: f64, index: usize }`.
- The `str[last_index .. idx+1]` slice in the strict-path guard is
  `&str[last_index ..= idx]` (Zig `[a..b]` is half-open with `b = idx+1`).
- **`indexOf` lowercases ONLY the haystack byte** (`std.ascii.toLower(slice[i]) == value`),
  never the token byte `value`. This is why `RankOptions.to_lower` "does not convert
  the tokens" and why `case_sensitive = !to_lower`: the caller is expected to hand a
  lowercase token. Preserve exactly — do **not** lower `value`.

### 2.2 `rankToken(str, filenameOrNull, token, case_sensitive, strict_path) -> ?f64`
Three mutually-exclusive branches (faithful):

**(a) strict_path** — token contains a `/`. Iterate token *segments* via
`PathIterator`; for each segment, `IndexIterator` over `str` (starting at `start`,
carried across segments) for the segment's first byte; on the first `scanToEnd`
success of the segment tail:
```
path_segment_len = segmentLen(str, start_index)
coverage = 1.0 - (segment.len as f64 / path_segment_len as f64)   // DIVISION — preserve order
rank = coverage * scan.rank
best_rank = (best_rank == null) ? rank : best_rank + rank         // SUM across segments
start = scan.index; break
```
If any segment finds no match → `return null`. Returns summed `best_rank`.

**(b) filename** (non-strict, `filenameOrNull = Some(filename)`) — `IndexIterator`
over `filename`; keep the **minimum** `scan.rank` (`best_rank = min`). If matched:
```
best_rank /= 2.0                                                   // filename priority
if token.len == filename.len: best_rank /= 2.0
else: coverage = 1.0 - (token.len as f64 / filename.len as f64); best_rank *= coverage
return best_rank
```
If no filename match → fall through to (c). (Note the `break` on the first
`scanToEnd` failure inside the loop — faithful; the loop stops at the first index
whose tail fails, it does not scan all indices.)

**(c) full string** — `IndexIterator` over `str`, keep min `scan.rank`. Return
`best_rank` (may be `null`).

- `IndexIterator` loops in (b)/(c) `break` on the first `scanToEnd` that returns
  null (not `continue`) — this is a faithful quirk, keep it.
- Case sensitivity: `IndexIterator` seeds on `token[0]` with the same
  `case_sensitive`; `scanToEnd` scans `token[1..]`.

### 2.3 `zf.rank(str, tokens, opts) -> ?f64` (zf.rs)
```
filename = opts.plain ? None : Some(basename(str))
sum: f64 = 0
for token in tokens:
    strict_path = !opts.plain && hasSeparator(token)
    match filter::rank_token(str, filename, token, !opts.to_lower, strict_path):
        Some(r) => sum += r
        None    => return None
return Some(sum)
```
`RankOptions { to_lower: bool = true, plain: bool = false }` (Rust: no field
defaults; provide `Default` = `{ to_lower: true, plain: false }` and a builder-free
`RankOptions::default()`; the many `.{}` call sites map to `RankOptions::default()`).
`RankTokenOptions { to_lower: bool = false, strict_path: bool = false, filename: Option<&[u8]> = None }`
→ `Default = { to_lower:false, strict_path:false, filename:None }`.
Note `rankToken` (zf.rs) passes `!opts.to_lower` as `case_sensitive` with default
`to_lower=false` ⇒ default case_sensitive=**true** (differs from `rank`'s default
`to_lower=true`); pin both.

### 2.4 `highlight` / `highlightToken`
Mirror rankToken's three branches but thread a `FixedArrayList<usize>` of matched
indices; on each improving `best_rank` copy `matched → best_matched`. `scanToEnd`
appends `idx + offset` when `matched_indices` is `Some`. The filename branch
computes the trailing-slash offset:
```
offset = str.len - filename.len - (if str[str.len-1] == SEP {1} else {0})
```
Return the filled prefix slice. Highlights are **not** sorted (see zf.zig's
lines 86-89 musing comment — port it verbatim).

## 3. `std.fs.path` inline (pure-leaf, no std shim)

`zf.zig` / `clib.zig` call `std.fs.path.basename`; `filter.zig` uses
`std.fs.path.sep`. Port target is macOS/Linux ⇒ `sep = '/'` (`sep_posix`), and
`basename` = `basenamePosix`. Inline faithfully in `filter.rs`:
```
pub const SEP: u8 = b'/';
// std.fs.path.basenamePosix
pub fn basename(path: &[u8]) -> &[u8] {
    if path.is_empty() { return &[]; }
    let mut end = path.len() - 1;
    while path[end] == SEP { if end == 0 { return &[]; } end -= 1; }   // all-slashes → ""
    let mut start = end; end += 1;
    while path[start] != SEP { if start == 0 { return &path[0..end]; } start -= 1; }
    &path[start+1 .. end]
}
```
This is the exact semantics the tests need: `basename("a/path/to/file") = "file"`,
`basename("s/") = "s"`, `basename("/this/is/path/not/a/file/") = "file"`,
`basename("abcdefg") = "abcdefg"`. (Windows `sep='\\'` is out of scope; note it as a
faithful simplification — zf itself uses the native `sep`, and the theme picker only
ever sees POSIX-style resource paths on the supported targets.)

## 4. Zig-ism → Rust mapping

| Zig | Rust |
|---|---|
| `[]const u8` | `&[u8]` (bytes, never `String`) |
| `[]const []const u8` (tokens) | `&[&[u8]]` |
| `?f64` / `?usize` | `Option<f64>` / `Option<usize>` |
| `?[]const u8` (filenameOrNull) | `Option<&[u8]>` |
| `std.ascii.toLower(b)` | ASCII-only: `if b'A'..=b'Z' { b + 32 } else { b }` (inline; do NOT use `char::to_ascii_lowercase` on non-ASCII paths — bytes only) |
| `indexOf(comptime T, …, comptime case_sensitive: bool)` | only `T=u8` is instantiated ⇒ monomorphize to `fn index_of(slice: &[u8], start: usize, value: u8, case_sensitive: bool) -> Option<usize>` with a **runtime** `if case_sensitive` (behavior identical to the two comptime instantiations) |
| `IndexIterator` / `PathIterator` structs w/ `next()` | plain structs `impl Iterator`-free `fn next(&mut self) -> Option<…>` (keep the explicit `next`, not `impl Iterator`, to mirror control flow 1:1 — the `while (it.next()) |x|` loops map to `while let Some(x) = it.next()`) |
| `FixedArrayList(comptime T)` | `struct FixedArrayList<'a> { buffer: &'a mut [usize], len: usize }`; `append` is a **silent no-op when full** (`if len >= buffer.len { return; }`) — the small-buffer overflow tests depend on this; `slice(&self) -> &[usize] = &buffer[0..len]`; `clear` sets len=0. Only `usize` is used ⇒ no generic needed. |
| `matched_indices: ?*FixedArrayList(usize)` | `Option<&mut FixedArrayList<'_>>` |
| `@as(f64, @floatFromInt(x))` (usize→f64) | `x as f64` (exact for in-range) |
| `std.mem.span([*:0]const u8)` | `CStr::from_ptr(p).to_bytes()` (null-terminated → `&[u8]`) — clib only |
| `export fn` | `#[no_mangle] pub extern "C" fn rustty_zf_*` |
| `test "name" {}` | `#[test] fn name() {}` (assertions ported one-for-one) |
| anonymous `test { _ = @import(...) }` | a `ref_all_decls` compile-touch test (skipped by the count gate; keeps rust ≥ zig) |

### Borrow-checker note (highlightToken return)
`highlight_token<'a>(…, matches: &'a mut [usize]) -> &'a [usize]`: `best_matched`
holds `&mut matches`; after building, return `&matches[0..len]` (reborrow as shared
once the `FixedArrayList` is out of scope). The local `matched` list backs a
stack `[usize; 1024]` (Zig `var buf: [1024]usize`). Two independent lists coexist —
no aliasing.

## 5. C ABI (clib.rs) — R2 export names

Zig `export fn {rank,rankToken,highlight,highlightToken}` → Rust
`#[no_mangle] pub extern "C" fn`:

| Zig export | Rust symbol (R2) |
|---|---|
| `rank` | `rustty_zf_rank` |
| `rankToken` | `rustty_zf_rank_token` |
| `highlight` | `rustty_zf_highlight` |
| `highlightToken` | `rustty_zf_highlight_token` |

**Superseded (parity tail S-10b, zf 0.10.3 -> 0.11.0):** upstream renamed `rankToken` /
`highlightToken` to `rankNeedle` / `highlightNeedle`, and the exports are now
`rustty_zf_rank_needle` / `rustty_zf_highlight_needle` (`NEW-FINDINGS.md` N240). The names
in this document are the 0.10.3 design as written.

- Signatures faithful: `str: *const c_char` (`[*:0]const u8`),
  `tokens: *const *const c_char`, `num_tokens: usize`, `case_sensitive: bool`,
  `plain: bool`, `matches: *mut usize`, `matches_len: usize`. Return `f64`
  (`-1.0` on no-match — the C sentinel replacing `null`) or `usize` (# matches).
- Inside: rebuild `&[u8]` via `CStr::from_ptr(...).to_bytes()`; walk the token
  pointer array with `std::slice::from_raw_parts(tokens, num_tokens)`; `matches`
  slice via `from_raw_parts_mut(matches, matches_len)`. `filename = plain ? None :
  Some(basename(string))`. Note clib's `strict_path = hasSeparator(token)` has **no
  `!plain` guard** (unlike zf.rs's `rank`) — faithful.
- **The `ghostty_zf_*` / `zf_*` alias exports are DEFERRED to item #2** (a later
  chunk) — this crate ships only the `rustty_zf_*` names.
- Crate-type: start `["rlib"]` (list_themes consumes the **Rust** `zf::rank` API,
  not the C ABI). The `extern "C"` fns still compile and are unit-tested in-crate.
  Add `["staticlib","cdylib"]` + a cbindgen header only when the C alias (#2)
  actually needs a linkable library. (Recorded as a follow-up, not this chunk.)

## 6. Tests — the 8 named `test{}` (+1 anon) = fidelity pins

Port each `test "…"` → one `#[test]`, assertion-for-assertion:
- `zf.rs`: `rank_library_interface` (zf.zig:64), `highlight_library_interface`
  (zf.zig:120 — pins exact index arrays incl. disconnected/overflow highlights).
- `filter.rs`: `path_iterator` (79), `segment_len` (130), `rank_token` (215 —
  plain/filename/strict-path match & no-match pins), `fixed_array_list` (293 —
  the silent-drop-when-full pin).
- `clib.rs`: `rank_exported_c_library_interface` (44),
  `highlight_exported_c_library_interface` (144). Use `c"…"` (`CStr`) literals /
  `[*const c_char; N]` arrays built from `CStr::as_ptr()` for the null-terminated
  inputs; a small `test_highlight` helper mirrors clib.zig's `testHighlight`.
- `lib.rs`: `ref_all_decls` compile-touch for the anonymous `test{}`.

`expectEqualSlices(usize, …)` → `assert_eq!(slice, &[…][..])`;
`expect(x == null)` → `assert!(x.is_none())`; `expect(x != null)` →
`assert!(x.is_some())`; `expectEqual(null, …)` → `assert_eq!(…, None)`.

Count-gate expectation: `zig_count` skips the anonymous `test{}` ⇒ zig named = 8
(zf 2 + filter 4 + clib 2). Rust ships 8 `#[test]` + the `ref_all_decls` touch ⇒
rust ≥ zig. **New MAP entry** in `.port/tools/gates/check_test_counts.py`:
```
GHZF = "/Users/gael/.cache/zig/p/zf-0.10.3-OIRy8RuJAACKA3Lohoumrt85nRbHwbpMcUaLES8vxDnh/src/zf"
RSZF = "/Users/gael/projects/rustty/crates/rustty-zf/src"
"zf": ([f"{GHZF}/zf.zig", f"{GHZF}/filter.zig", f"{GHZF}/clib.zig"], [f"{RSZF}/*.rs"], 0),
```

## 7. Cargo / crate skeleton

`crates/rustty-zf/Cargo.toml`: `name = "rustty-zf"`, workspace-inherited
version/edition/lints, `[dependencies]` **empty** (pure leaf, R4), `[dev-dependencies]`
**empty** (the C tests use only `std::ffi::CStr`/`c_char` from libcore/std). Add the
crate to the workspace `members` list. `[lib] crate-type = ["rlib"]` (see §5).
Header doc comment mirrors the z2d/wuffs identity banner (vendored-dep, MIT/zf
license, external-oracle-only, no ghostty-tree dep).

Module layout — flat is faithful and simplest:
```
src/lib.rs      // crate doc + `mod filter; mod clib; pub mod zf;` + ref_all_decls
src/zf.rs       // RankOptions/RankTokenOptions/rank/rankToken/highlight/highlightToken
src/filter.rs   // scorer + SEP/basename + FixedArrayList + iterators
src/clib.rs     // rustty_zf_* extern "C"
```
(Keeping `zf.rs` as the public module means the API is `rustty_zf::zf::rank`; if a
flatter `rustty_zf::rank` is preferred, `pub use zf::*` in lib.rs — cosmetic,
decide at implementation. The Zig import graph is `zf → {filter, clib}`, `clib →
filter`, so `mod` order is filter (leaf), then clib, then zf.)

## 8. Gates (all green criteria for the crate)

- **build / test / clippy `-D warnings` / fmt** — standard.
- **check_acyclic.sh** — trivially green (no `rustty-*` dep edges).
- **check_test_counts.py** — new `"zf"` MAP entry (§6); rust 8 ≥ zig 8.
- **check_comment_coverage.py (G3b)** — port **every** distinctive reference
  comment (the algorithm-explaining `//` lines in `scanToEnd`/`rankToken`, the
  doc-comments on the options structs, zf.zig's lines 86-89 highlight-sorting
  musing) retargeted ghostty→rustty. This is the crate's densest comment payload;
  budget for it.
- **Miri (R6 primary)** — pure Rust ⇒ Miri is primary. The `clib.rs` `extern "C"`
  fns are **callable from Rust under Miri** (Miri only bars FFI *across* the
  language boundary; calling a `pub extern "C" fn` from a Rust `#[test]` with
  valid `CStr`-backed pointers is fully interpreted). So the clib tests run under
  Miri as-is; the raw-pointer null-terminated walk + `from_raw_parts` are
  Miri-clean given the CStr-backed literals. **No MIRI_EXCEPTIONS entry needed.**
  (If a future real cross-language C consumer appears via a `cdylib`, that link
  path — not this crate's tests — would be the ASan-only surface.)

## 9. Downstream (out of scope for this chunk)

`cli/list_themes.zig` (→ `rustty-cli`) calls `zf.rank`; wiring that consumer and the
`ghostty_zf_*`/`zf_*` C aliases are **item #2**, not this crate. This design covers
only `crates/rustty-zf` itself.

## 10. Risks / faithful simplifications (all low)

- **`sep = '/'` (POSIX-only)** — Windows `'\\'` separator dropped; matches the
  supported targets and zf's own native-`sep` behavior. Documented, not a
  correctness risk for the theme picker.
- **`case_sensitive` runtime vs Zig comptime** — a runtime `bool` branch replaces
  the two `indexOf` comptime instantiations; behavior identical, zero fidelity gap.
- **`f64` bit-exactness** — the only inexact ops are the three `coverage` divisions
  (`1.0 - a/b`); Rust and Zig both use IEEE-754 round-to-nearest default, so
  preserving op order (spelled out in §2) yields identical bits. The 9 pinned
  match/no-match tests are `is_none`/`is_some` (decision), and the sort in
  list_themes is a total order over these `f64`s — the port reproduces both.
- **No genuine blocker.** Disposition: **ready**.
