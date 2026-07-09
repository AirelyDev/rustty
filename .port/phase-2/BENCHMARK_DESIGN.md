# BENCHMARK_DESIGN — faithful port of `src/benchmark/` → `rustty-bench`

Reference oracle (READ-ONLY): `/Users/gael/projects/ghostty/src/benchmark/*.zig`
(Zig 0.15.2). Target crate: `crates/rustty-bench` (currently scaffold: `lib.rs`
doc-only, `Cargo.toml` deps = rustty-vt/simd/unicode).

Disposition: **READY**. Every benchmarked subsystem API is already ported and
reachable from `rustty-bench` under the acyclic graph (rustty-bench sits above
vt/unicode/simd/args/global/lib — all earlier in the workspace topo order). No
genuine blocker.

---

## 0. Source inventory (verbatim from the oracle)

| Zig file | lines | test{} | role |
|---|---|---|---|
| `Benchmark.zig` | 178 | 1 (`test Benchmark`) | the runner: `ptr+VTable` fn-ptr vtable, `run(RunMode)`, `RunResult`, `Error`, macOS signpost |
| `cli.zig` | 100 | 0 | `Action` enum (7 benches), comptime `Action.Struct`, `main`, `Args` union, `mainAction`/`mainActionImpl` |
| `CApi.zig` | 34 | 0 | `export fn ghostty_benchmark_cli(action, args) bool` |
| `main.zig` | 13 | 1 (`refAllDecls`) | pub re-export hub + `test { refAllDecls }` |
| `options.zig` | 20 | 0 | `dataFile(?[]const u8) !?std.fs.File` (stdin `-`, else openFile) |
| `CodepointWidth.zig` | 207 | 1 | modes noop/wcwidth/simd/table |
| `GraphemeBreak.zig` | 152 | 1 | modes noop/table |
| `IsSymbol.zig` | 151 | 1 | modes uucode/table |
| `OscParser.zig` | 118 | 1 | length-prefixed OSC record replay |
| `ScreenClone.zig` | 193 | **0** | modes noop/clone/render |
| `TerminalParser.zig` | 114 | 1 | raw `Parser.next` per byte |
| `TerminalStream.zig` | 161 | 1 | `Stream(*Handler)` → `terminal.print` |

**Test contract = 8 Zig `test{}` blocks** (Benchmark + 6 benchmark decltests —
ScreenClone has none — + main's `refAllDecls`). See §10.

> NOTE on the task brief: this pinned oracle (`Benchmark.zig` = 178 lines) uses
> **only** `std.time.Instant`. There is **no** `getrusage`/`perf_event`/warmup
> path in any of these files (grep-confirmed: the only timing calls are the two
> `std.time.Instant.now()` in `Benchmark.zig`). The brief's "rusage/perf-counter
> / warmup" description does not apply to this revision; the `#[cfg]`-gated
> non-portable arm is the **macOS `os_signpost`** Instruments integration (§5),
> not rusage. Nothing else is platform-gated except `wcwidth` (extern libc,
> skipped on Windows) — faithfully preserved.

---

## 1. Module map (Zig file → Rust module)

All under `crates/rustty-bench/src/`. `main.zig`'s re-export hub becomes `lib.rs`.

| Zig | Rust module | notes |
|---|---|---|
| `main.zig` | `lib.rs` | `pub mod` each; `refAllDecls` → dropped (Zig reflection idiom; the `cargo build --all-targets` + workspace test run is the Rust equivalent). Document the drop in the module count contract. |
| `Benchmark.zig` | `benchmark.rs` | `Benchmark` struct, `VTable`, `RunMode`, `RunResult`, `Error`, `init`/`run` |
| `cli.zig` | `cli.rs` | `Action` enum + `BenchmarkAction` trait (comptime replacement, §6), `main`, `Args`, `main_action`/`main_action_impl` |
| `CApi.zig` | `capi.rs` | `extern "C" fn rustty_benchmark_cli` (§11) |
| `options.zig` | `options.rs` | `data_file()` → `DataFile` reader enum (§8) |
| `CodepointWidth.zig` | `codepoint_width.rs` | |
| `GraphemeBreak.zig` | `grapheme_break.rs` | |
| `IsSymbol.zig` | `is_symbol.rs` | |
| `OscParser.zig` | `osc_parser.rs` | |
| `ScreenClone.zig` | `screen_clone.rs` | |
| `TerminalParser.zig` | `terminal_parser.rs` | |
| `TerminalStream.zig` | `terminal_stream.rs` | |

The Rust file names are snake_case per repo convention; the *type* names keep the
reference PascalCase (`CodepointWidth`, `TerminalStream`, …) verbatim (R1/R2).

---

## 2. Cargo.toml dependencies

Current scaffold deps: `rustty-vt`, `rustty-simd`, `rustty-unicode`. **Add**:

- `rustty-args` — `cli.args.parse`, `cli.args.argsIterator`, `cli.action.detectArgs`
  (the `Options` parsing + argv iterator + action auto-detect). *(rustty-args is
  crate #15, below rustty-bench — acyclic.)*
- `rustty-global` — `state.alloc` for the C-ABI entrypoint (`CApi`/`cli.main`).
- `rustty-lib` — `allocator::ZigAllocatorRef` / `GlobalZigAllocator` for the osc
  `Parser::init(Option<ZigAllocatorRef>)` seam, and the `allocator_api2` `Global`
  alias that `Terminal`/`Screen` take. *(Confirm whether `Global` is re-exported
  via rustty-vt; if so this dep may be implicit. rustty-vt already re-exports the
  allocator types it needs.)*
- `libc` (already vendored elsewhere) — only for the `wcwidth` extern (§7.1). Gate
  the whole `wcwidth` mode behind `#[cfg(not(windows))]` faithfully.

`check_acyclic.sh` must stay green: all five deps precede rustty-bench in the
workspace `members` topo order. No new edge inverts the graph.

---

## 3. Timing / perf model

`Benchmark.zig` lines 67/74:
```zig
const start = std.time.Instant.now() catch return error.BenchmarkFailed;
...
const now = std.time.Instant.now() catch return error.BenchmarkFailed;
const exit = switch (mode) { .once => true, .duration => |ns| now.since(start) >= ns };
if (exit) { result.duration = now.since(start); return result; }
```

Rust mapping:
- `std.time.Instant.now()` → `std::time::Instant::now()`. **Infallibility note:**
  Zig's `Instant.now()` returns an error union (can fail on exotic platforms),
  hence the `catch return error.BenchmarkFailed`. `std::time::Instant::now()` is
  infallible, so the error arm is unreachable in Rust — document it as a
  deliberate faithful simplification (keep the `Error::BenchmarkFailed` variant
  for the step-fn path, which *can* fail).
- `now.since(start)` (u64 ns) → `now.duration_since(start).as_nanos() as u64`
  (or `saturating_duration_since` — `Instant::now()` is monotonic so plain
  `duration_since` is fine; `as_nanos()` is `u128`, cast to `u64` to match the
  `RunResult.duration: u64` field exactly).
- `RunMode` = `union(enum){ once, duration: u64 }` → Rust `enum RunMode { Once,
  Duration(u64) }`. `.once => true`, `.duration => |ns| elapsed >= ns`.
- `RunResult { iterations: u32 = 0, duration: u64 = 0 }` → `#[derive(Default)]
  struct RunResult { iterations: u32, duration: u64 }`.

`std.mem.doNotOptimizeAway(x)` (appears in every step fn) → `std::hint::black_box(x)`.
This is the load-bearing "keep the work" primitive — must be preserved at every
call site (CodepointWidth ×3, GraphemeBreak, IsSymbol ×2, OscParser, ScreenClone
×3; TerminalParser discards via `_ = actions`).

---

## 4. The runner — `Benchmark` (raw-ptr fn-ptr vtable)

The reference is a type-erased `ptr: *anyopaque` + a `VTable` of
`*const fn(ptr: *anyopaque) Error!void` function pointers, with each impl casting
back via `@ptrCast(@alignCast(ptr))`. **Faithful modeling = a raw-pointer
trampoline**, NOT a `dyn Trait` object — this preserves the exact `*anyopaque` +
per-callee-cast structure and stays Miri-clean (every cast round-trips the *same*
`*mut Self` provenance the impl handed in).

```rust
pub struct Benchmark {
    ptr: *mut c_void,
    vtable: VTable,
}

pub struct VTable {
    // Zig: stepFn: *const fn(*anyopaque) Error!void
    pub step_fn: unsafe fn(*mut c_void) -> Result<(), Error>,
    // Zig: setupFn: ?*const fn(*anyopaque) Error!void = null
    pub setup_fn: Option<unsafe fn(*mut c_void) -> Result<(), Error>>,
    // Zig: teardownFn: ?*const fn(*anyopaque) void = null
    pub teardown_fn: Option<unsafe fn(*mut c_void)>,
}

pub enum RunMode { Once, Duration(u64) }
#[derive(Default)] pub struct RunResult { pub iterations: u32, pub duration: u64 }
#[derive(Debug)] pub enum Error { BenchmarkFailed }
```

- `init(pointer, vtable)`: Zig asserts the arg is a single-item pointer to a
  struct (`@typeInfo` asserts). In Rust those invariants are enforced by the type
  system, so `init` takes `ptr: *mut c_void` (each caller passes `self as *mut _
  as *mut c_void`). Keep the three asserts as a doc comment noting they are
  compile-time-guaranteed in Rust.
- `run(self, mode)`: line-for-line — call `setup_fn` if `Some` (fail-fast via
  `?`), `defer teardown` → a drop-guard or an explicit call before every return
  (a small `TeardownGuard` holding `ptr` + `Option<teardown_fn>` is the faithful
  `defer` — runs on all exit paths incl. the error return). Then the signpost
  block (§5), the `Instant` loop (§3), `result.iterations += 1`, exit check.
- Every impl's `benchmark(&mut self) -> Benchmark` builds
  `Benchmark::init(self as *mut Self as *mut c_void, VTable { step_fn: step, .. })`
  where `step`/`setup`/`teardown` are `unsafe fn(*mut c_void)` that do
  `let self = &mut *(ptr as *mut Self);` (the faithful `@ptrCast(@alignCast)`).

**Miri**: all pointer casts are `*mut Self ↔ *mut c_void` round-trips with intact
provenance → clean. The vtable holds plain `fn` pointers (not closures), matching
Zig's `*const fn`. No `dyn`, no vtable synthesis divergence.

---

## 5. macOS signpost — the `#[cfg]`-gated non-portable arm

`Benchmark.zig` lines 44–65 wrap the timed loop in an Instruments
`os_signpost` interval (`macos.os.signpost.intervalBegin/End`, `macos.os.Log`),
guarded by `builtin.target.os.tag.isDarwin()` with a `void` else-branch and a
`defer` that ends the interval + releases the log.

rustty has **no `os_signpost` binding** today. Port faithfully as a
**DEFERRED SEAM**:
- `#[cfg(target_os = "macos")]` arm: a no-op today, with the entire reference
  body preserved **verbatim in comments** (R1 — no comment/logic lost). The
  `build_config.bundle_id` and `macos.os.signpost.*` calls are named in the
  comment for the future binding.
- `#[cfg(not(target_os = "macos"))]`: the `void` branch → nothing (matches Zig's
  empty else).
- The `defer intervalEnd + log.release()` → represented by the same drop-guard
  pattern used for teardown, but empty on macOS-today.
- Record in `DEFERRALS.md`: "benchmark: macOS `os_signpost` Instruments interval
  (Benchmark.zig 44–65) — deferred to the macOS binding tier; verbatim reference
  preserved, no-op body."

This is the ONE platform-gated seam in the runner.

---

## 6. CLI dispatch — comptime `Action.Struct` → the `BenchmarkAction` trait

`cli.zig` uses `inline else => |comptime_action|` + `Action.Struct(comptime_action)`
to select the benchmark type at comptime, then `mainActionImpl(BenchmarkImpl, ...)`
which relies on each impl exposing `Options`, `create`, `destroy`, `benchmark`.

Rust has no comptime type-switch. Faithful modeling: a **`BenchmarkAction` trait**
capturing the four decls the comptime code requires, plus a `match` that
monomorphizes `main_action_impl::<Impl>` per arm.

```rust
pub enum Action { CodepointWidth, GraphemeBreak, ScreenClone, TerminalParser,
                  TerminalStream, IsSymbol, OscParser }   // #[strum]/from_name for detect

pub trait BenchmarkAction: Sized {
    type Options: Default;                    // + optional deinit
    fn create(alloc: Allocator, opts: Self::Options) -> Result<Box<Self>, ...>;
    fn destroy(self: Box<Self>, alloc: Allocator);
    fn benchmark(&mut self) -> Benchmark;
}

pub enum Args<'a> { Cli, String(&'a [u8]) }   // union(enum){ cli, string: []const u8 }

pub fn main_action(alloc, action: Action, args: Args) -> Result<(), ...> {
    match action {
        Action::CodepointWidth => main_action_impl::<CodepointWidth>(alloc, args),
        Action::TerminalStream => main_action_impl::<TerminalStream>(alloc, args),
        ... // one arm per Action, mirroring Action.Struct's switch
    }
}

fn main_action_impl<B: BenchmarkAction>(alloc, args: Args) -> Result<(), ...> {
    let mut opts = B::Options::default();
    // args.cli   → cli::args::args_iterator()? then cli::args::parse(&mut opts, &mut iter)
    // args.string → std.process.ArgIteratorGeneral equivalent (rustty-args string iter)
    //               then cli::args::parse(&mut opts, &mut iter)
    let impl_ = B::create(alloc, opts)?;      // Box<B>
    let b = impl_.benchmark();
    let _ = b.run(RunMode::Once)?;
    // defer impl_.destroy(alloc)  — drop-guard / explicit
}
```

- `cli.action.detectArgs(Action, alloc)` (in `cli.main`) → the rustty-args
  action auto-detect (`detect_args`) producing `Option<Action>`; `orelse return
  error.NoAction`. `Action` needs a `stringToEnum`-equivalent `from_name` (the
  `@"codepoint-width"`-style kebab names are the CLI tokens — preserve exactly).
- `cli.args.parse(Options, alloc, &opts, &iter)` → `rustty_args::args::parse(&mut
  opts, &mut iter)` (rustty signature drops the comptime `T`/`alloc` into
  generics/`&mut`). Each benchmark's `Options` struct derives the args-parse
  surface (field names incl. `@"terminal-rows"` → the kebab CLI key `terminal-rows`).
- `.string` arm: Zig's `std.process.ArgIteratorGeneral(.{}).init(alloc, str)`
  splits a single string into argv. rustty-args exposes the string/slice iterator
  variant (`ArgsIterator` over a line/slice splitter) — use it for the C-ABI path.
- `opts.deinit()` (`@hasDecl` guard) → `Options::Drop` where present (only impls
  whose Options own heap; most are `Copy`-ish plain structs → no-op).

---

## 7. Per-benchmark subsystem-call mapping (against REAL rustty signatures)

Shared shape per benchmark: `struct { opts, data_f: Option<DataFile>, ... }`,
`create`/`destroy`/`benchmark`/`setup`/`teardown` + one or more `step*` fns. The
`setup` opens the data file (`options::data_file`), `teardown` closes it.

### 7.1 CodepointWidth (`codepoint_width.rs`)
Modes: `noop | wcwidth | simd | table`. Reads UTF-8 bytes via `UTF8Decoder`,
per-codepoint:
- decode: `rustty_vt::utf8_decoder::UTF8Decoder::next(&mut self, byte: u8) ->
  (Option<u32>, bool)` — the `(cp_, consumed)` tuple, `assert(consumed)` preserved.
- `wcwidth`: `extern "C" { fn wcwidth(c: u32) -> c_int; }` behind
  `#[cfg(not(windows))]`; the Windows arm logs the warn + returns (verbatim).
  `black_box(wcwidth(cp))`.
- `simd`: `rustty_simd::codepoint_width::codepointWidth(cp: u32) -> i8`
  → `black_box(codepointWidth(cp))`. **Confirmed ported** (`codepoint_width.rs:13`).
- `table`: the terminal.zig trick `if cp <= 0xFF { 1 } else { table.get(cp).width }`
  using `rustty_unicode::table` (`pub use props_table::table`, a `lut::get(cp)
  -> Elem` with a `.width` field). `black_box(...)`.

### 7.2 GraphemeBreak (`grapheme_break.rs`)
Modes: `noop | table` (default `table`).
- `noop`: decode-only loop (`_ = d.next(c)`).
- `table`: `rustty_unicode::graphemeBreak(cp1, cp2, &mut state)` where `state:
  rustty_unicode::grapheme::BreakState = BreakState::Default`. **Confirmed**:
  `grapheme::grapheme_break(cp1: u32, cp2: u32, state: &mut BreakState) -> bool`,
  re-exported as `graphemeBreak` (`lib.rs:25`), and `BreakState` is the faithful
  port of `uucode.grapheme.BreakState` (`grapheme.rs:12`). `black_box(...)`; `cp1
  = cp2` carry preserved.

### 7.3 IsSymbol (`is_symbol.rs`)
Modes: `uucode | table` (default `uucode`).
- The reference `.uucode` mode calls the LIVE `uucode.get(.is_symbol, cp)`. rustty
  has **no live uucode dep** (R3) — it embeds uucode-derived tables. The faithful
  reproduction is the reference `get`-closure rule: `if cp > MAX_CODE_POINT {
  false } else { symbols_table::table().get(cp) }`, written **inline in the
  benchmark** over already-`pub` APIs — `rustty_unicode::props_uucode::MAX_CODE_POINT`
  (`= 0x10FFFF`, `props_uucode.rs:7`) and `symbols_table::table()`. (rustty's
  own `is_symbol` reference impl at `symbols_uucode.rs:62` lives inside a
  `#[cfg(test)]` mod — do NOT depend on it; it is `= table().get(cp)` with the
  same guard, so replicating the two-line guard in the benchmark is exact and
  needs no visibility change.)
- `.table`: `rustty_unicode::symbols_table::table().get(cp)` (the Ghostty
  table-based approach, no guard — matches the reference's direct
  `symbols_table.get(cp)`). `table()` is `pub const fn table() ->
  TablesRef<'static, bool>` (`symbols_table.rs:5`); `.get(cp) -> bool`.
- Both `black_box(...)`. **No rustty-unicode surface change required** — both
  `MAX_CODE_POINT` and `symbols_table::table()` are already public.

### 7.4 OscParser (`osc_parser.rs`)
- `parser: rustty_vt::osc::Parser<'a>` — `Parser::init(alloc:
  Option<ZigAllocatorRef>)` (`osc/mod.rs:635`), `deinit` (659), `reset` (664),
  `next(&mut self, c: u8)` (769), `end(&mut self, Option<u8>) -> Option<&mut
  Command>` (1004). All present.
- `create` builds `Parser::init(Some(alloc_ref))`; `destroy` calls `parser.deinit()`.
- `setup` calls `parser.reset()`.
- `step`: length-prefixed record loop — read `usize` LE length then that many
  bytes into `osc_buf`, replay each byte via `parser.next(c)`, then
  `black_box(parser.end(Some(0x07 /* BEL */)))`, then `parser.reset()`. The
  `@call(.always_inline, Parser.next, ...)` → a plain `parser.next(c)` (the
  always-inline hint has no faithful Rust knob; `#[inline]` on `next` is the
  nearest, but `next` is defined in rustty-vt — leave as a call, note the drop).
  `std.ascii.control_code.bel` = `0x07`.
- The record framing (`fill(@bitSizeOf(usize)/8)` + `takeInt(usize,.little)` +
  `readSliceAll`) → §8's exact-read helpers on the `DataFile` reader.

### 7.5 ScreenClone (`screen_clone.rs`) — **0 tests**
Modes: `noop | clone | render` (default `clone`). Terminal-backed.
- `terminal: rustty_vt::terminal::Terminal`, `Terminal::init(alloc: Allocator,
  Options{ rows, cols })` (`terminal.rs:532`), `deinit(alloc)` (594),
  `full_reset` (the `fullReset`), `vt_stream(&mut self) -> Stream` (627).
- `setup`: `full_reset`, then a `vt_stream()` writes the SGR + `rows-1 ×
  "hello\r\n"` + `"hello"` priming (`s.nextSlice` → `stream.next_slice(&[u8])`,
  `s.deinit()`), then replays the data file through a second `vt_stream()`.
- `stepNoop`: `for 0..1000 { let s: Screen = ptr::read(self.terminal.screens.active);
  black_box(&s); mem::forget(s); }`. **Fidelity/Miri detail**: Zig does
  `self.terminal.screens.active.*` — a shallow by-value struct copy that is
  NEVER deinit'd (aliases the same backing pages). In Rust `Screen` owns heap and
  is not `Copy`; the faithful equivalent is `core::ptr::read` (bitwise shallow
  copy) + `mem::forget` so the aliasing copy is not dropped (no double-free). Wrap
  in `black_box`. `screens.active` is `*mut Screen` (`screen_set.rs:50`).
- `stepClone`: `for 0..1000 { let copy = (*active).clone(alloc, Point::viewport
  default, None)?; black_box(&copy); mem::forget(copy); }`. **Confirmed** sig:
  `Screen::clone(&self, alloc: Allocator, top: point::Point, bot:
  Option<point::Point>) -> Result<Screen, OutOfMemory>` (`screen.rs:828`). The
  `.{ .viewport = .{} }` = `point::Point::Viewport(<default>)`; `null` → `None`.
  Purposeful leak (`mem::forget`) — the reference explicitly does NOT free.
  `s.alloc` via `(*active).alloc`.
- `stepRender`: `rustty_vt::render::RenderState::empty()` (`render.rs:256`),
  `state.update(alloc, &mut self.terminal)` (`render.rs:300`), then `for 0..1000
  { state.screen = ScreenType::Alternate; state.update(alloc, &mut t)?;
  black_box(&state); }`. RenderState is reachable via the `render` module (the
  vt-root `pub const RenderState` re-export is commented-out today — use
  `rustty_vt::render::RenderState`; non-blocking).

### 7.6 TerminalParser (`terminal_parser.rs`)
- `p: rustty_vt::parser::Parser` — `Parser::init()` (`parser.rs:342`),
  `next(&mut self, c: u8) -> [Option<Action>; 3]` (`parser.rs:377`).
- `step` reads 4096-byte chunks; `parseAll(&mut p, &buf[..n])` is a `noinline`
  helper (fidelity comment about function alignment preserved) →
  `#[inline(never)] fn parse_all(p: &mut Parser, data: &[u8])` looping `let
  actions = p.next(c); let _ = actions;`.

### 7.7 TerminalStream (`terminal_stream.rs`)
- `Stream(*Handler)` → `rustty_vt::stream::Stream<'a, Handler>` (the generic
  `Stream<H: Handler>`, `stream.rs:575`), built with `Stream::init(handler)`
  (599) or `init_alloc`. The reference `Handler` struct with `vt(comptime action,
  value)` that only acts on `.print` → a Rust `struct Handler { t: *mut Terminal }`
  impl'ing the **`Handler` trait** (`stream.rs:228`), overriding **only** `fn
  print(&mut self, cp: u32)` (`stream.rs:245`) → `unsafe { (*self.t).print(cp) }`
  with the warn-on-err; all other trait methods keep their default no-op bodies
  (exactly the reference `else => {}`).
- **Self-referential struct**: the reference stores `handler: .{ .t = &ptr.terminal }`
  and `stream: .init(&ptr.handler)` — the handler borrows the terminal and the
  stream borrows the handler, all inside the same heap `TerminalStream`. In Rust
  this is a self-referential struct; model `Handler.t` as `*mut Terminal` (raw
  ptr, faithful to Zig's `*Terminal`) and hold the `Stream` over the boxed
  handler. `Terminal::print(&mut self, c: u32) -> Result<(), OutOfMemory>`
  (`terminal.rs:687`) confirmed.
- `step`: `self.stream.next_slice(&buf[..n])` per chunk.

---

## 8. IO / reader model (`options.rs`)

`options.dataFile(?[]const u8) !?std.fs.File`: `null` → `None`; `"-"` → stdin;
else `std.fs.cwd().openFile(path, .{})`. Rust:

```rust
pub enum DataFile { Stdin(std::io::Stdin), File(std::fs::File) }
pub fn data_file(path: Option<&[u8]>) -> std::io::Result<Option<DataFile>> { ... }
```
- `"-"` → `DataFile::Stdin(std::io::stdin())`; else `File::open(path)`. The
  `errdefer file.close()` is Rust's automatic drop. Path is bytes → `OsStr`
  from bytes (`OsStrExt`, faithful to Zig `[]const u8` paths; do NOT lossily
  `String` per the byte-slice fidelity rule).

The Zig 0.15 reader interface (`f.reader(&read_buf)` → `f_reader.interface`,
`readSliceShort`, `fill`, `takeInt`, `readSliceAll`) → `std::io::BufReader` +
`Read`:
- `readSliceShort(&buf) -> n` (short only at EOF) → a small helper looping
  `Read::read` to fill `buf` until full or EOF, returning bytes read (`n == 0` ⇒
  EOF). This matches the reference's "n==0 ⇒ break" contract.
- OscParser's `fill(8)` + `takeInt(usize,.little)` + `readSliceAll(buf[..len])` →
  `read_exact(&mut [u8; 8])` → `u64::from_le_bytes` (usize = 8 on the 64-bit
  targets; `@bitSizeOf(usize)/8`), then `read_exact(&mut osc_buf[..len])`; map
  `EndOfStream`→`return Ok(())`, `ReadFailed`→`Err(BenchmarkFailed)` exactly.
- The `[4096]u8 align(std.atomic.cache_line)` buffers → a cache-line-aligned
  newtype `#[repr(align(64))] struct CacheLineBuf([u8; 4096])` (64 is the
  `std.atomic.cache_line` value on the port's targets; note it — the alignment is
  a perf-fidelity detail, not correctness). Non-aligned `[4096]u8` buffers stay
  plain arrays.

---

## 9. Test contract (§0 → Rust)

8 Zig `test{}` blocks. Rust mapping:
- `test Benchmark` → `#[test] fn benchmark()` in `benchmark.rs`. Ports the
  `Simple` inline struct (setup/step counters), `b.run(.once)`, the 4
  `expectEqual`/`expect(duration > 0)` asserts. Keep the FreeBSD/Windows
  `SkipZigTest` guard → `#[cfg]`/early-return (the port targets macOS+Linux; a
  documented skip-stub keeps the count).
- `test CodepointWidth / GraphemeBreak / IsSymbol / OscParser / TerminalParser /
  TerminalStream` (6) → one `#[test]` each: `create(alloc, Default)` →
  `benchmark()` → `run(RunMode::Once)`. These are the **6 benchmark decltests**.
- `ScreenClone` — **no test** (faithfully: no `#[test]`).
- `main.zig test { refAllDecls }` → **dropped** (Zig reflection idiom with no Rust
  analogue; `cargo build --all-targets` covers "all decls compile"). Document the
  drop in the module's count note so `check_test_counts.py` accounting is explicit
  (rust_count = zig_count − 1 for the `lib.rs`/`main.zig` module, which the tool
  allows only with a documented deferral — record it).

Net: **7 `#[test]` functions** (Benchmark + 6 benchmarks). Rust may add
rustty-only helper tests (allowed: `rust >= zig`).

---

## 10. C-ABI export (`capi.rs`) — R2 name + deferred alias

Reference: `export fn ghostty_benchmark_cli(action_name: [*:0]const u8, args:
[*:0]const u8) bool` — resolves the action name via `stringToEnum`, warns +
`false` on unknown, else `cli.mainAction(state.alloc, action, .{ .string = args })`,
warns + `false` on error, else `true`.

Port (per R2, matching the vt-c `#[no_mangle] pub extern "C" fn rustty_*`
precedent):
```rust
#[no_mangle]
pub extern "C" fn rustty_benchmark_cli(
    action_name: *const c_char,
    args: *const c_char,
) -> bool { ... }
```
- `[*:0]const u8` → `*const c_char`; `std.mem.sliceTo(_, 0)` → `CStr::from_ptr`
  → bytes (keep as `&[u8]`, not lossy `String`).
- `stringToEnum(Action, name)` → `Action::from_name(bytes)`; `None` → warn +
  `false`.
- alloc = `rustty_global::STATE.get().alloc` (the `state.alloc` seam).
- `cli::main_action(alloc, action, Args::String(args_bytes))`; `Err` → warn +
  `false`; `Ok` → `true`.
- `log.scoped(.benchmark)` → the crate's `log`/`tracing` scoped logger.

**Deferred alias** (record in `DEFERRALS.md`): the `ghostty_benchmark_cli` C name
is provided later by sub-project #2's alias layer (the C-ABI compat shim), NOT in
rustty-bench. rustty-bench exports only `rustty_benchmark_cli`.

---

## 11. Miri posture (R6)

- `rustty-bench` core logic is **pure Rust** → **Miri is primary**. The raw-ptr
  vtable (§4), the `ptr::read` + `mem::forget` in ScreenClone::stepNoop, and the
  self-referential `*mut Terminal` handler (§7.7) are the three unsafe spots —
  all provenance-preserving (round-trip the exact ptr the owner handed in) →
  Miri-clean. Add a Miri harness note for stepNoop (the shallow-copy-then-forget
  is the sharp edge; assert no drop/no double-free under Miri).
- The `extern "C"` `rustty_benchmark_cli` and the `wcwidth` extern are
  **Miri-excluded** (FFI/C-ABI) → covered by the ASan lane instead. Record both
  in `MIRI_EXCEPTIONS.md`.
- The benchmark `#[test]`s call real file IO only when `data` is set; the decltests
  pass `data: None` → the step fns early-return (`orelse return`), so the tests are
  pure and Miri-runnable (no real fd).

---

## 12. Deferrals summary (→ DEFERRALS.md)

1. macOS `os_signpost` Instruments interval (Benchmark.zig 44–65) — verbatim
   comment, no-op body, `#[cfg(target_os="macos")]`; awaits the macOS binding tier.
2. `ghostty_benchmark_cli` C alias — provided by sub-project #2's alias layer;
   rustty-bench ships `rustty_benchmark_cli` only.
3. `main.zig`'s `refAllDecls` test — dropped (Zig reflection idiom); count note
   recorded for `check_test_counts.py`.
4. `@call(.always_inline, Parser.next, …)` inline hint — no faithful Rust knob;
   plain call (nearest is `#[inline]` on the vt-side `next`).
5. (Resolved — no action.) IsSymbol `.uucode` mode: the out-of-range guard is
   replicated inline over already-`pub` `props_uucode::MAX_CODE_POINT` +
   `symbols_table::table()` (§7.3). No rustty-unicode visibility change needed.

## 13. Disposition

**READY.** All seven benchmarks' subsystem calls resolve to ported, reachable,
already-public rustty APIs (signatures cited above). The runner, CLI dispatch,
options, and C-ABI export all have faithful, Miri-clean Rust models. No pre-req
surface changes in any dependency crate. Proceed to implementation.
