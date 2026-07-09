# rustty-synthetic — faithful-port design

Port target: `ghostty/src/synthetic/` (+ `cli/`) → crate `crates/rustty-synthetic`.
Reference is a read-only oracle (R3). Crate is PURE Rust ⇒ **Miri is the primary gate** (R6):
`cargo +nightly miri test -p rustty-synthetic`. All standard gates also apply
(build / test / clippy -D / fmt / check_acyclic.sh / check_test_counts.py /
check_comment_coverage.py).

Disposition: **ready**. No blocker. One dependency wrinkle (CLI action-detection /
string arg-iterator) resolved by a small local faithful port — detailed in §6.

---

## 1. Scope & source inventory

| Zig file (LOC, test{}) | rustty module |
|---|---|
| `Generator.zig` (42, 0) | `src/generator.rs` |
| `Bytes.zig` (119, 1) | `src/bytes.rs` |
| `Osc.zig` (366, 3) | `src/osc.rs` |
| `Utf8.zig` (229, 3) | `src/utf8.rs` |
| `cli.zig` (115, 1) | `src/cli.rs` |
| `main.zig` (25, 1) | `src/lib.rs` (the package root / `main.zig` equivalent) |
| `cli/Ascii.zig` (68, 1) | `src/cli/ascii.rs` |
| `cli/Osc.zig` (82, 1) | `src/cli/osc.rs` |
| `cli/Utf8.zig` (111, 1) | `src/cli/utf8.rs` |
| **+ new** `src/rand.rs` | ported `std.Random` (see §2) — the ONE piece with no upstream Zig-file counterpart in synthetic/; it is the std facility the package leans on |

**Phase-0 test contract:** 9 `test{}` blocks across the synthetic module
(`Bytes×1, Osc×3, Utf8×3, cli.zig×1, cli/Ascii×1, cli/Osc×1, cli/Utf8×1, main.zig×1`
= actually 12 counting each; the "~9" in the brief refers to the substantive generator
tests). Port **every** `test{}` assertion-for-assertion; `check_test_counts.py` must be
`>=` for the synthetic module. Additionally we add the Zig-std `Xoshiro256` `"sequence"` /
`fill` tests and `SplitMix64` behaviour into `src/rand.rs` as bonus fidelity anchors
(these RAISE the count, which the `>=` gate permits, and pin the PRNG bit-for-bit).

---

## 2. PRNG — the crux (bit-faithful port required)

`std.Random.DefaultPrng` in Zig **0.15.2** (ghostty's pinned toolchain,
`build.zig.zon: .minimum_zig_version = "0.15.2"`) is **Xoshiro256++**, seeded via
**SplitMix64**. Every generator is driven by `std.Random`'s helper methods. Because R1
requires bit-faithfulness of derived/comptime artifacts and the OSC "valid/invalid"
tests depend on the *exact* branch selection the RNG drives, we port the **exact
algorithm**, NOT the `rand` crate.

Source of truth (verified, Zig 0.15.2 stdlib):
`…/lib/zig/std/Random/Xoshiro256.zig`, `…/std/Random/SplitMix64.zig`, `…/std/Random.zig`.

### 2a. SplitMix64 (`src/rand.rs`)
```
s: u64
next(): s +%= 0x9e3779b97f4a7c15;
        z = s;  z = (z ^ (z>>30)) *% 0xbf58476d1ce4e5b9;
                z = (z ^ (z>>27)) *% 0x94d049bb133111eb;
        return z ^ (z>>31);
```
Rust: all ops on `u64` with `wrapping_add`/`wrapping_mul`/shift. `+%`→`wrapping_add`,
`*%`→`wrapping_mul`.

### 2b. Xoshiro256++ (`Xoshiro256`)
```
s: [u64;4]
init(seed): gen = SplitMix64::init(seed); s[0..4] = gen.next() ×4
next(): r = rotl(s[0]+%s[3], 23) +% s[0];
        t = s[1] << 17;
        s[2]^=s[0]; s[3]^=s[1]; s[1]^=s[2]; s[0]^=s[3];
        s[2]^=t; s[3] = rotl(s[3],45);
        return r
fill(buf): 8-byte LE chunks from next(); tail loop truncates a fresh next() ("cuts the stream")
```
`rotl(u64,x,k)` = `x.rotate_left(k)`. `jump()` (2^128 skip, uses u256) is present in the
Zig file — port it verbatim for completeness even though synthetic never calls it
(refAllDecls-style coverage). u256 in Rust: model as `[u64;4]`/manual or a small
128-bit-pair; simplest faithful approach is a fixed `u256` helper (two `u128`s) since it
is only bit-twiddled through a constant table.

### 2c. `std.Random` helper methods (port exactly — these gate every generator)
The generators call: `bytes`, `boolean`, `int(T)`, `uintLessThan(T)`, `intRangeAtMost(T)`,
`intRangeAtMostBiased(T)`, `float(f64)`, `weightedIndex(f64, &proportions)`. Port the
supporting chain verbatim from `std/Random.zig`:

- `bytes(buf)` → `fill(buf)`.
- `int(T)`: fill `ceil(bits/8)` bytes, `readInt(ByteAligned, .little)`, truncate to T.
  **LE byte order is load-bearing.**
- `boolean()` = `int(u1) != 0`.
- `limitRangeBiased(T, x, less_than)` = `(mulWide(x,less_than) >> bits)`.
- `uintLessThan(T, n)`: Lemire with rejection (the pcg "with extra tweak"): `mulWide`,
  low-half rejection against `t = -%n` reduced twice then `%`. **Port the loop exactly** —
  the number of `int()` draws consumed on rejection changes the whole downstream stream.
- `uintLessThanBiased` / `uintAtMostBiased` / `uintAtMost` — as written.
- `intRangeAtMost(T,lo,hi)` = `lo + uintAtMost(hi-lo)` (unsigned branch; synthetic only
  uses unsigned `usize`/`u8`/`u3`/`u21`). `intRangeAtMostBiased` = `lo + uintAtMostBiased`.
- `intRangeLessThan` (unsigned branch) — used indirectly by `weightedIndex` only for int
  proportions; synthetic's `weightedIndex` is always `f64`, so the float branch is what
  matters, but port both for the method's completeness.
- `float(f64)`: the mantissa+biased-exponent construction with the `@clz` leading-zero
  loop. Port the f64 arm exactly (`rand = int(u64)`, `rand_lz` loop, `mantissa =
  rand & 0xFFFFFFFFFFFFF`, `exponent = (1022 - rand_lz) << 52`, `@bitCast`). The rare
  extra-draw loop must match to keep the stream aligned. f32 arm can be ported too or
  omitted (synthetic never uses f32) — porting it is cheap and safest for `>=`.
- `weightedIndex(f64, props)`: `sum = Σprops`; `point = min(float(f64)*sum,
  sum - floatEps(f64))`; accumulate until `point < acc`, return index. `floatEps(f64)` =
  `f64::EPSILON` (2^-52). This is what selects the OSC ValidKind / InvalidKind and the
  Utf8 length — **must be exact**.

**T-generic note:** Zig's `int(T)`/`uintLessThan(T)` are comptime-generic over integer
width. Rust: implement over the concrete widths actually used (`u1,u3,u8,u21,u64,usize`)
via a small trait or macro-generated impls; `mulWide(T)` = widen to 2×T (`u128` for
`u64/usize`, `u16` for `u8`, etc.). `usize` is 64-bit on the target — treat as `u64`
LE, matching Zig's `usize`.

### 2d. `std.Random` as a shared handle
Zig passes `std.Random` (a `{ptr, fillFn}` vtable) by value; a `*Xoshiro256`'s
`.random()` yields a handle that **mutates the shared prng state** on every draw. Several
call sites (`Osc.bytes()`, `Bytes.newAlphabet/atMost` copies) **copy the `rand` handle by
value but share the underlying generator** — draws through any copy advance the one prng.

Rust modelling: `Rng` is a thin `Copy` handle wrapping a `*mut Xoshiro256` (raw pointer),
mirroring Zig's by-value vtable that aliases one generator. All the `Bytes`/`Osc` structs
hold `rand: Rng` and freely copy it (`newAlphabet`, `atMost`, `bytes()` all construct new
`Bytes`/`Osc` values carrying the same `Rng`). Because the crate is Miri-gated, the raw
alias must obey Stacked/Tree Borrows: the prng is owned by the test/`run` frame (`prng`
local), and `Rng` only ever calls `&mut self`-style mutation **through its own raw ptr**
with narrow access — no `&mut Xoshiro256` is held across a draw. This is the same
raw-alias discipline recorded in memory (`shaper-hook-sb-clean-interleave`,
`rust-mut-args-strongly-protected`): route mutation via the alias's own pointer, prove on
Miri, don't hand-wave. A non-`unsafe` alternative — `Rng(Rc<RefCell<Xoshiro256>>)` or
`&Cell<Xoshiro256>` — is acceptable if the raw-ptr Miri story proves fiddly; it changes no
observable bytes. **Recommendation:** start with `&Cell<Xoshiro256>` /
`&RefCell<Xoshiro256>` (Copy shared handle, zero unsafe, trivially Miri-clean) unless a
borrow conflict forces the raw-ptr form. Bit output is identical either way.

---

## 3. Comptime / byte-faithfulness concerns

1. **`Bytes.generateAlphabet(comptime func)`** builds a `[]const u8` at comptime by
   scanning `0..256`. Rust: a `const fn generate_alphabet(f: fn(u8)->bool) -> ([u8;256],
   usize)` producing a fixed 256-buffer + length, or `const`-evaluated `&[u8]` arrays.
   The **exact set and order** of bytes must match (ascending 0..255). All derived
   alphabets are `pub const` arrays:
   - `Osc.kv_alphabet` (printable minus ESC/BEL/`;`/`=`), `osc_alphabet` (all minus
     ESC/BEL), `ascii_alphabet` (`isPrint`), `alphabetic_alphabet` (`isAlphabetic`),
     `alphanumeric_alphabet` (`isAlphanumeric`); `cli/Ascii.ascii` (`isPrint` minus space).
   - `std.ascii.isPrint` = `0x20..=0x7E`; `isAlphabetic` = `A-Za-z`; `isAlphanumeric` =
     `A-Za-z0-9`. `control_code.esc`=0x1B, `bel`=0x07. Reproduce these predicates exactly
     (do NOT reuse Rust `char::is_ascii_*` blindly — verify boundaries; `isPrint` in Zig
     excludes DEL 0x7F and everything ≥0x80).
2. **`osc`/`st` constants** — `std.fmt.comptimePrint("{c}]", .{esc})` = bytes `1B 5D`
   (`ESC ]`); `st` = `07` (BEL). In Rust: `const OSC: &[u8] = b"\x1b]"; const ST: &[u8] =
   b"\x07";`. `osc.len==2`, `st.len==1` feed the `max_len - (osc.len+st.len)` arithmetic.
3. **`EnumArray(Kind, f64)`** (`p_valid_kind`/`p_invalid_kind`/`p_length`): a dense
   f64 array indexed by enum in **declaration order**, with `.initFill(1.0)`, `.set(k,v)`,
   `.values` (the raw slice handed to `weightedIndex`), and `Indexer.keyForIndex(idx)`
   (index→enum). Follow the existing rustty convention (rustty-vt `screen.rs` uses a
   "bespoke struct instead of std.EnumArray"): model as `[f64; N]` plus an enum with
   `from_index(usize)`/`to_index()` in the Zig field order. `ValidKind` has 13 variants
   (order: change_window_title … iterm2_notification), `InvalidKind` 2, `Utf8Len` 4.
   `keyForIndex` maps the `weightedIndex` result back to the variant — order-sensitive.
4. **`{f}` custom-format hooks.** `Bytes.format` and `writer.print("…{f}…", .{bytes})`
   invoke Zig's `format` method, which *writes random bytes* as a side effect (consuming
   prng draws). This is NOT `Display` (fallible with `Writer.Error`, and stateful). Model
   as an explicit method: everywhere Zig does `writer.print("0;{f}", .{self.bytes().
   atMost(n)})`, port to: write literal `"0;"`, then call `bytes.write(writer)` (which is
   what `format` delegates to). Keep the write ORDER identical (literal prefix first, then
   the generated bytes) so prng draws land in the same sequence.
5. **`intRangeAtMostBiased(usize, min_len, max_len)`** with `max_len =
   std.math.maxInt(usize)` default: fine (unsigned biased path). The `atMost`/`atLeast`
   builders clamp with `@min` — port with `usize::min` / `.min()`.

---

## 4. Per-file port notes

### generator.rs (`Generator.zig`)
Zig `Generator` is a `{ptr, nextFn}` vtable with `init(pointer, comptime nextFn)` and
`next(writer, max_size)`; `Error = error{WriteFailed}`. Rust: model as a trait
`Generator { fn next(&mut self, w, max_len) -> Result<(), WriteFailed> }` OR a
`{*mut (), fn}` struct mirroring the vtable. Since `Bytes`/`Osc`/`Utf8` each expose
`.generator()` returning a `Generator` used polymorphically in tests
(`v.generator(); gen.next(&writer, len)`), a small enum or a `&mut dyn Generator` /
boxed-fn vtable both work. **Recommendation:** a `Generator<'a>` struct holding a raw
`ptr` + `next_fn`, faithfully reproducing the Zig vtable (the `init` asserts about pointer
kind become no-ops / debug_asserts). No test{} of its own.

### bytes.rs (`Bytes.zig`)
Fields `rand, min_len=1, max_len=maxInt(usize), alphabet: Option<&[u8]>`. Methods
`generateAlphabet`, `generator`, `newAlphabet`, `atLeast`, `atMost`, `next`, `format`,
`write`. `write` asserts `min_len>=1`, `max_len>=min_len`; draws `len =
intRangeAtMostBiased(min,max)`; fills in 8-byte chunks via `rand.bytes(data)`; if alphabet,
`byte = alphabet[byte % alphabet.len]`; returns `len`. **1 test** `"bytes"`: DefaultPrng
seed 0, 256-buf, min=max=256, `gen.next`; assert `buffered().len == 256`.

### osc.rs (`Osc.zig`)
`ValidKind`(13), `InvalidKind`(2), fields `rand, p_valid=1.0, p_valid_kind, p_invalid_kind`.
`next` writes `ESC ]` + `nextUnwrapped(max_len - 3)` + `BEL`, asserts `max_len>=3`.
`chooseValidity`: `if rand.float(f64) > p_valid → invalid else valid`. `nextUnwrapped`
picks kind via `weightedIndex(&values)` + `keyForIndex`. Port all 13 valid-kind arms and
2 invalid-kind arms **byte-for-byte**, including every `max_len` guard, every
`rand.boolean()` branch, `writer.print("133;{d}", …)` integer formatting, the
`report_pwd`/`hyperlink` `intRangeAtMost(2,5)` loops, `conemu_progress`
`intRangeAtMost(u3,0,4)`/`intRangeAtMost(u8,0,100)`, and the `end_of_input` path that
routes generated ASCII through **`urlPercentEncode`**. `bytes()` helper returns a `Bytes`
with `osc_alphabet`. **3 tests**: `"OSC generator"` (seed `0xC0FFEEEEEEEEEEEE`, 50 iters,
`result.len>0`); `"OSC generator valid"` (`p_valid=1.0`, parse `seq[2..len-1]` through
`rustty_vt::osc::Parser`, expect `end(None).is_some()`); `"OSC generator invalid"`
(`p_valid=0.0`, expect `end(None).is_none()`).

### utf8.rs (`Utf8.zig`)
`Utf8Len`(one=1..four=4), `InvalidSequence{len,bytes:[4]}`, `invalid_sequences` (9 fixed
byte patterns — copy verbatim), fields `rand, min_len=1, max_len=maxInt, p_length,
ascii_printable_only=false, invalid_rate=0`. `next` draws `len =
min(intRangeAtMostBiased(min,max), max_len)`, loops filling via `writeInvalid`(prob
`invalid_rate`, pick a fitting invalid seq via `uintLessThan(valid_len)`) then
`writeValid` (choose `utf8Len` via `weightedIndex` shrinking to fit `rem`, draw a
codepoint in the length's range, `utf8Encode`, retry on surrogate-half). **Needs unicode
helpers** `utf8Encode`, `utf8CodepointSequenceLength`, `utf8ValidateSlice` (see §6).
**3 tests**: `"utf8"` (len 256, valid); `"utf8 printable ascii only"` (weights one=1 rest
0, all printable); `"utf8 malformed output"` (`invalid_rate=1.0`, `!valid`).

### cli.rs (`cli.zig`)
`Action{ascii,osc,utf8}` with `Struct(action)` → the impl module; `main()` (binary
entry, uses `detectArgs` — see §6), `Args{cli, string}`, `mainAction`, `mainActionImpl`
(parse Options via rustty-args `parse`, build DefaultPrng from `nanoTimestamp` seed,
`Impl.create/run/destroy`, flush). **1 test**: comptime-ref all actions ⇒ a Rust test
that references each `cli::{ascii,osc,utf8}` impl type (`_ = …`) — faithful to
`inline for` over the enum fields.

### cli/ascii.rs, cli/osc.rs, cli/utf8.rs
Each: `Options` struct (rustty-args-parseable — `#[derive]`/trait matching the args
framework), `create/destroy/run`. `run` loops `gen.write`/`gen.next` into the writer until
`WriteFailed`/`BrokenPipe`. **1 test each** (`Ascii`/`Osc`/`Utf8`): `create` with default/
seeded opts, run into a fixed buffer. cli/Utf8 additionally asserts `buffered().len ==
1024`. cli/Utf8 `create` validates `invalid-rate ∈ [0,1]` and weight sum > 0 →
`error.InvalidValue`; cli/Utf8 `run` optionally reseeds from `opts.seed`.

### lib.rs (`main.zig`)
Package root: doc comment (the "synthetic package" blurb) + `pub use` of
`cli, Generator, Bytes, Utf8, Osc`. **1 test**: `refAllDecls` → a Rust test touching each
public item. Flip `PORT STATUS: stub` → `done` once filled.

---

## 5. Dependencies that already exist in rustty (reuse, don't re-port)

- **`urlPercentEncode`** → `rustty_os::string_encoding::urlPercentEncode(writer, data)`
  is ported (verified signature `<Wr: Writer>(&mut Wr, &[u8]) -> Result<(), Wr::Error>`).
  Used by `Osc.end_of_input`. rustty-synthetic already deps `rustty-os`. ✅
- **OSC parser** → `rustty_vt::osc::Parser::init(Option<ZigAllocatorRef>)`, `.next(u8)`,
  `.end(Option<u8>) -> Option<&mut Command>` is ported. Used by the two OSC
  valid/invalid tests. rustty-synthetic already deps `rustty-vt`. ✅ (tests pass `None`
  for the allocator, matching Zig `.init(null)`.)

## 6. Dependency wrinkles (resolved — not blockers)

1. **`std.Random` itself is not yet ported anywhere below synthetic.** ⇒ new local
   module `src/rand.rs` (§2). This is the single largest chunk of net-new code and the
   one place where "port the exact algorithm" is mandatory. Self-contained, pure, Miri-
   friendly. Not a blocker.
2. **Unicode helpers** (`utf8Encode`, `utf8CodepointSequenceLength`, `utf8ValidateSlice`)
   used by `Utf8.zig` + its tests. rustty has utf8 machinery in `rustty-unicode` /
   `rustty-vt`, but **synthetic's allowed deps are {vt, os, args}** and `rustty-vt` is a
   dep, so prefer a helper reachable there; if not cleanly exposed, port these three tiny
   std.unicode functions locally into `src/utf8.rs` (they are ~30 LOC total and fully
   specified by the Unicode encoding rules — `utf8Encode` writes 1–4 bytes with the
   standard lead/continuation masks; `utf8CodepointSequenceLength` is a range check;
   `utf8ValidateSlice` a DFA/length walk). Faithful and self-contained. Not a blocker.
3. **`cli.action.detectArgs` + `std.process.ArgIteratorGeneral`** (used only by
   `cli.zig::main()` and the `Args.string` branch). These live in ghostty's
   `src/cli/action.zig` / std.process, which in rustty were ported into **rustty-cli**
   (`crates/rustty-cli/src/action.rs`) and only appear as **test helpers** in
   rustty-args — neither is reachable from synthetic without a cycle (rustty-cli is
   above synthetic). Resolution, in priority order:
   - `mainAction` / `mainActionImpl` — the substantive glue — need only
     `rustty_args::args::parse` (ported ✅) + an arg iterator. The `.cli` branch uses
     `rustty_os::args::iterator()` (ported ✅, `rustty-os` is a dep).
   - `detectArgs`/`detectIter` (generic `+action` detection, ~50 LOC, `stringToEnum` +
     an args iterator) and a minimal `ArgIteratorGeneral` (shell-ish string splitter,
     used by `Args.string`) are **ported locally** into `src/cli.rs` as small faithful
     reproductions. This mirrors ghostty's own layering (synthetic is a *separate
     package* importing the root cli); the acyclic Rust split just forces the helper to
     live locally. Alternatively, `main()`/`.string` may be marked a thin
     binary-entry seam per the `placeholder-leaf-doctrine` if the local port is deemed
     out of scope — but a faithful local port is preferred and cheap. **Not a blocker**;
     none of the 9 substantive tests exercise `detectArgs` or the `.string` branch (the
     cli/* tests call `create`/`run` directly).
4. **`Options` ↔ rustty-args parse.** The three `Options` structs use Zig field names
   with dashes (`"p-valid"`, `"weight-one"`, `"invalid-rate"`, `"ascii-printable-only"`).
   Port using whatever field-name mapping the rustty-args `parse` framework already uses
   for dashed CLI keys elsewhere (follow rustty-cli/rustty-config precedent). Enum-typed
   option (`Osc.Options.style {streaming, parser}`) parses via the args enum path.

---

## 7. Acyclicity (R4)

`rustty-synthetic → { rustty-vt, rustty-os }` (+ **add `rustty-args`** to Cargo.toml for
`args::parse` / arg iteration; it sits below vt & os, so the edge is acyclic — confirm
with `check_acyclic.sh`). No edge to rustty-cli, rustty-unicode-as-such, or the ghostty
tree (R3). All net-new helpers (`rand.rs`, local utf8/detectArgs) stay inside the crate.

---

## 8. Fidelity checklist (R1)

- [ ] Xoshiro256++ + SplitMix64 + every `std.Random` method used, ported bit-exact
      (LE byte order, Lemire rejection loop, float `@clz` loop, weightedIndex float path).
- [ ] All 6 comptime alphabets + `osc`/`st` constants byte-identical.
- [ ] 13-variant `ValidKind` / 2 `InvalidKind` / 4 `Utf8Len` in declaration order;
      EnumArray `keyForIndex` order-faithful.
- [ ] 9 `invalid_sequences` byte patterns copied verbatim.
- [ ] `{f}` format side-effects modelled as explicit `write` in the SAME order.
- [ ] Every `max_len` guard / `break :label` early-out reproduced.
- [ ] All test{} assertions ported assertion-for-assertion; +Xoshiro/SplitMix std tests
      added as bonus anchors (raises count, `>=` gate OK).
- [ ] byte-slices → `&[u8]`/`Vec<u8>`/`[u8;N]`, never lossy `String`.
- [ ] Miri clean (the shared-`Rng`-handle alias discipline of §2d proven on Miri).
- [ ] `PORT STATUS: done` in lib.rs.
</content>
</invoke>
