# SYNTHETIC_CLI_PARITY — faithful-port design for the `rustty-synthetic` CLI seam

Scope: fill the 4 `unimplemented!()` stubs in `crates/rustty-synthetic/src/cli.rs`
(`main`, `main_action_impl_{ascii,osc,utf8}`), wire the per-action `Options` to
`rustty_args::args::Target`, add the stdout writer + time seed, and add a
`[[bin]]` so `cargo run -p rustty-synthetic` works.

Reference oracle (READ-ONLY):
- `ghostty/src/synthetic/cli.zig` — `main` / `mainAction` / `mainActionImpl`.
- `ghostty/src/synthetic/cli/{Ascii,Osc,Utf8}.zig` — the `Options` structs.
- `ghostty/src/main_gen.zig` — the 3-line binary entry.

Exemplar to mirror (identical problem already solved): `crates/rustty-bench/src/cli.rs`
(local `detect_args`/`main`, the `Args::Cli`/`Args::String` parse branches) and
`crates/rustty-bench/src/codepoint_width.rs` (`impl rustty_args::args::Target for Options`).

R1: preserve every reference comment verbatim as a `//`/PORT NOTE; keep the
existing `tests::ref_all_actions`. Nothing here changes the already-real generator
logic in `cli/{ascii,osc,utf8}.rs` — only the CLI glue above them.

---

## 0. Cargo.toml changes (`crates/rustty-synthetic/Cargo.toml`)

Add the args dependency (member 15, strictly earlier than synthetic #29 → acyclic, R4)
and a binary target mirroring `main_gen.zig`:

```toml
[dependencies]
rustty-vt = { workspace = true }
rustty-os = { workspace = true }
# cli.args.parse / cli.args.argsIterator (Options parsing + argv iterator).
# rustty-args is member 15, strictly-earlier than rustty-synthetic (29) — acyclic (R4).
rustty-args = { workspace = true }

# main_gen.zig: `pub const main = synthetic.cli.main;`
[[bin]]
name = "rustty-synthetic"
path = "src/main_gen.rs"
```

Note: `rustty-bench` carries NO `[[bin]]` (its `cli::main` is reached only through
the `CApi` C-ABI). `rustty-synthetic` DOES get one because `main_gen.zig` is a real
standalone generator binary and the task requires `cargo run -p rustty-synthetic`.

---

## 1. Reference-Options → CLI-key map + `Target::parse_into_field` per action

The kebab key is the Zig field name verbatim (`@"weight-one"` → `b"weight-one"`),
exactly as `Mode::from_name` preserves `@"codepoint-width"` in the bench exemplar.
Each `Options` already derives/impls `Default`; we only ADD `impl
rustty_args::args::Target for Options` (and, for osc, a `Style::from_name`). The
trait method is `fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>)
-> Result<(), rustty_args::args::ParseError>`; unknown key → `Err(Error::InvalidField)`.

### ascii (`cli/ascii.rs`) — `Options = struct {}` (EMPTY)

No fields. `Target` rejects every key:

```rust
impl rustty_args::args::Target for Options {
    fn parse_into_field(&mut self, _key: &[u8], _value: Option<&[u8]>)
        -> Result<(), rustty_args::args::ParseError> {
        // Options is empty — no field matches; `parse` maps this to "unknown field".
        Err(rustty_args::args::Error::InvalidField)
    }
}
```

### osc (`cli/osc.rs`)

| CLI key       | Rust field | Rust type    | helper                                          | default          |
|---------------|------------|--------------|-------------------------------------------------|------------------|
| `p-valid`     | `p_valid`  | `f64`        | `parse_field_float::<f64>(value)?`              | `0.5`            |
| `style`       | `style`    | `Style` enum | `parse_field_enum(value, Style::from_name)?`    | `Style::Streaming` |

Add `Style::from_name` (mirrors `Mode::from_name`; tokens are the inline-enum tag
names `streaming`/`parser`):

```rust
impl Style {
    /// `std.meta.stringToEnum(@TypeOf(style), name)` — the inline-enum tag names.
    fn from_name(name: &[u8]) -> Option<Style> {
        match name {
            b"streaming" => Some(Style::Streaming),
            b"parser" => Some(Style::Parser),
            _ => None,
        }
    }
}

impl rustty_args::args::Target for Options {
    fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>)
        -> Result<(), rustty_args::args::ParseError> {
        match key {
            // @"p-valid": f64 = 0.5
            b"p-valid" => { self.p_valid = rustty_args::args::parse_field_float::<f64>(value)?; Ok(()) }
            // style: enum { streaming, parser } = .streaming
            b"style" => { self.style = rustty_args::args::parse_field_enum(value, Style::from_name)?; Ok(()) }
            _ => Err(rustty_args::args::Error::InvalidField),
        }
    }
}
```

### utf8 (`cli/utf8.rs`)

| CLI key                 | Rust field             | Rust type    | helper                                       | default |
|-------------------------|------------------------|--------------|----------------------------------------------|---------|
| `seed`                  | `seed`                 | `Option<u64>`| `Some(parse_field_int::<u64>(value)?)`       | `None`  |
| `weight-one`            | `weight_one`           | `f64`        | `parse_field_float::<f64>(value)?`           | `1.0`   |
| `weight-two`            | `weight_two`           | `f64`        | `parse_field_float::<f64>(value)?`           | `1.0`   |
| `weight-three`          | `weight_three`         | `f64`        | `parse_field_float::<f64>(value)?`           | `1.0`   |
| `weight-four`           | `weight_four`          | `f64`        | `parse_field_float::<f64>(value)?`           | `1.0`   |
| `ascii-printable-only`  | `ascii_printable_only` | `bool`       | `parse_field_bool(value)?`                   | `false` |
| `invalid-rate`          | `invalid_rate`         | `f64`        | `parse_field_float::<f64>(value)?`           | `0.0`   |

The `seed: ?u64` optional mirrors `data: ?[]const u8` in the bench exemplar
(`self.data = Some(parse_field_string(value)?)`): wrap the parsed value in `Some`.

```rust
impl rustty_args::args::Target for Options {
    fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>)
        -> Result<(), rustty_args::args::ParseError> {
        match key {
            // seed: ?u64 = null
            b"seed" => { self.seed = Some(rustty_args::args::parse_field_int::<u64>(value)?); Ok(()) }
            // @"weight-one": f64 = 1.0
            b"weight-one" => { self.weight_one = rustty_args::args::parse_field_float::<f64>(value)?; Ok(()) }
            b"weight-two" => { self.weight_two = rustty_args::args::parse_field_float::<f64>(value)?; Ok(()) }
            b"weight-three" => { self.weight_three = rustty_args::args::parse_field_float::<f64>(value)?; Ok(()) }
            b"weight-four" => { self.weight_four = rustty_args::args::parse_field_float::<f64>(value)?; Ok(()) }
            // @"ascii-printable-only": bool = false
            b"ascii-printable-only" => { self.ascii_printable_only = rustty_args::args::parse_field_bool(value)?; Ok(()) }
            // @"invalid-rate": f64 = 0.0
            b"invalid-rate" => { self.invalid_rate = rustty_args::args::parse_field_float::<f64>(value)?; Ok(()) }
            _ => Err(rustty_args::args::Error::InvalidField),
        }
    }
}
```

(`parse_field_int::<u64>` is supported — `impl_int_from_str_radix!(u8,u16,u32,u64,usize,…)`
in `rustty-args/src/args.rs`. `ParseError` is `rustty_args::args::Error`, so the
`?`-propagated `Error::InvalidValue`/`ValueRequired` and the returned
`Error::InvalidField` share one type.)

---

## 2. The stdout Writer

Reference (`cli.zig` lines 92-95, 102-105):

```zig
// Our output always goes to stdout.
var buffer: [2048]u8 = undefined;
var stdout_writer = std.fs.File.stdout().writer(&buffer);
const writer = &stdout_writer.interface;
...
// Always flush
writer.flush() catch |err| switch (err) { error.WriteFailed => return };
```

Model this as a **buffered stdout newtype** implementing
`rustty_os::string_encoding::Writer` with `type Error = WriteFailed`. The trait
(`crates/rustty-os/src/string_encoding.rs:19`) requires exactly two methods — both
MUST be provided:

```rust
pub trait Writer {
    type Error;
    fn writeByte(&mut self, byte: u8) -> Result<(), Self::Error>;
    fn writeAll(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
}
```

Design (place in `cli.rs`, or a small `cli/stdout.rs` module):

```rust
use rustty_os::string_encoding::{WriteFailed, Writer};
use std::io::Write;

/// PORT NOTE: `std.fs.File.stdout().writer(&buffer)` with a 2048-byte buffer.
/// A buffered writer over stdout: accumulates up to 2048 bytes, auto-flushing to
/// the fd when full (this is the buffered `std.Io.Writer` behaviour — the buffer
/// filling is NOT `WriteFailed`; only a failing underlying `write` is). A broken
/// pipe / write error surfaces as `WriteFailed`, which every action's `run` loop
/// treats as end-of-generation (`return Ok`).
struct StdoutWriter {
    // var buffer: [2048]u8 = undefined;
    buf: [u8; 2048],
    len: usize,
}

impl StdoutWriter {
    fn new() -> StdoutWriter { StdoutWriter { buf: [0u8; 2048], len: 0 } }

    /// Flush the accumulated buffer to the fd. A write error => `WriteFailed`.
    fn flush(&mut self) -> Result<(), WriteFailed> {
        if self.len == 0 { return Ok(()); }
        let mut out = std::io::stdout().lock();
        out.write_all(&self.buf[..self.len]).map_err(|_| WriteFailed)?;
        self.len = 0;
        Ok(())
    }
}

impl Writer for StdoutWriter {
    type Error = WriteFailed;
    fn writeByte(&mut self, byte: u8) -> Result<(), WriteFailed> {
        self.writeAll(&[byte])
    }
    fn writeAll(&mut self, bytes: &[u8]) -> Result<(), WriteFailed> {
        let mut rest = bytes;
        while !rest.is_empty() {
            if self.len == self.buf.len() { self.flush()?; }        // buffer full → auto-flush
            let n = core::cmp::min(self.buf.len() - self.len, rest.len());
            self.buf[self.len..self.len + n].copy_from_slice(&rest[..n]);
            self.len += n;
            rest = &rest[n..];
        }
        Ok(())
    }
}
```

Rationale: the emitted byte stream on stdout is identical regardless of buffer
size; the 2048-byte batching only sets the flush granularity (perf fidelity) and
the point at which a broken pipe is observed. `WriteFailed` from the auto-flush is
propagated up through the generator's `writeAll`, and each `run` loop already
converts it to `Ok(())` (see `cli/{ascii,osc,utf8}.rs`). This is distinct from the
inner per-1024-byte `FixedWriter` that Osc/Utf8 use internally — that one fails on
overflow; this stdout writer never "overflows", it flushes.

---

## 3. The time seed

Reference (`cli.zig` 84-90):

```zig
// TODO: Make this a command line option.
const seed: u64 = @truncate(@as(u128, @bitCast(std.time.nanoTimestamp())));
var prng = std.Random.DefaultPrng.init(seed);
const rand = prng.random();
```

`std.time.nanoTimestamp()` is nanoseconds since the Unix epoch as `i128`;
`@bitCast` to `u128` then `@truncate` keeps the low 64 bits. Faithful Rust:

```rust
use std::time::{SystemTime, UNIX_EPOCH};
// @truncate(@as(u128, @bitCast(std.time.nanoTimestamp())))
let nanos: u128 = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_nanos())          // u128 nanoseconds since the epoch
    .unwrap_or(0);
let seed: u64 = nanos as u64;        // @truncate → low 64 bits
let prng = crate::rand::DefaultPrng::init(seed);
let rand = prng.random();
```

Mapping notes:
- `Duration::as_nanos()` returns `u128` = the same magnitude as Zig's `i128` for
  any post-epoch instant (the only reachable case), and `as u64` performs the
  identical low-64-bit `@truncate`.
- Pre-epoch (`duration_since` errors): Zig's `@bitCast` of a negative `i128` would
  differ, but no real clock reads before 1970; we map the error to `0` (documented,
  astronomically unreachable — same posture as the RNG's "unlikely loop" notes).
- `DefaultPrng`/`Rng` are the already-ported `crate::rand` types (Xoshiro256++).

For utf8, this time-seeded `rand` is still constructed and passed to `run`; when
`opts.seed` is `Some`, `Utf8::run` internally re-seeds a fresh `DefaultPrng` and
ignores the passed `rand` (already implemented in `cli/utf8.rs`) — faithful to the
Zig `Utf8.run`.

---

## 4. `main_action_impl_*` glue per action

`mainActionImpl(comptime Impl, alloc, args)` is monomorphised in Zig via `inline
else`; Rust names the three functions explicitly (as the bench crate does). Each is
structurally identical; only the concrete `Options`/impl type and the `create`
error mapping differ. Skeleton (shown for utf8; ascii/osc drop the seed-comment
nuance but are otherwise identical):

```rust
fn main_action_impl_utf8(args: Args) -> Result<(), Error> {
    // First, parse our CLI options.
    // const Options = Impl.Options; var opts: Options = .{};
    let mut opts = utf8::Options::default();
    // defer if (@hasDecl(Options, "deinit")) opts.deinit();
    //   (Options own no heap → automatic drop; no explicit deinit — see bench §6.)
    match args {
        // .cli => { var iter = try cli.args.argsIterator(alloc); defer iter.deinit();
        //           try cli.args.parse(Options, alloc, &opts, &iter); }
        Args::Cli => {
            let mut iter = rustty_args::args::args_iterator().unwrap_or_else(|e| match e {});
            let result = rustty_args::args::parse(&mut opts, &mut iter);
            iter.deinit();
            result?;
        }
        // .string => |str| { var iter = try ArgIteratorGeneral.init(alloc, str); ...
        //                    try cli.args.parse(Options, alloc, &opts, &iter); }
        Args::String(str) => {
            // ArgIteratorGeneral splits one string into argv; rustty-args exposes the
            // pre-split SliceIterator, so tokenize on ASCII whitespace (mirrors bench §6).
            let tokens: Vec<&[u8]> = str
                .split(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
                .filter(|t| !t.is_empty())
                .collect();
            let mut iter = rustty_args::args::slice_iterator(&tokens);
            rustty_args::args::parse(&mut opts, &mut iter)?;
        }
    }

    // TODO: Make this a command line option.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let seed: u64 = nanos as u64;
    let prng = crate::rand::DefaultPrng::init(seed);
    let rand = prng.random();

    // Our output always goes to stdout.
    let mut writer = StdoutWriter::new();

    // Create our implementation
    // const impl = try Impl.create(alloc, opts); defer impl.destroy(alloc);
    let impl_ = utf8::Utf8::create(opts).map_err(|_| Error::InvalidValue)?;
    // try impl.run(writer, rand);
    //   (destroy runs on every path, matching Zig's `defer impl.destroy`.)
    let run_result = impl_.run(&mut writer, rand);
    impl_.destroy();
    run_result.map_err(|_| Error::WriteFailed)?;   // run() already swallows WriteFailed → Ok

    // Always flush — writer.flush() catch |err| switch (err) { error.WriteFailed => return };
    let _ = writer.flush();
    Ok(())
}
```

Per-action `create` error mapping (the ONLY difference between the three):
- **ascii**: `Ascii::create(opts) -> Result<Ascii, WriteFailed>` (never fails) →
  `.map_err(|_| Error::WriteFailed)?`. `Options::default()` is `Options {}`.
- **osc**: `Osc::create(opts) -> Result<Osc, WriteFailed>` (never fails) →
  `.map_err(|_| Error::WriteFailed)?`.
- **utf8**: `Utf8::create(opts) -> Result<Utf8, InvalidValue>` (validates
  `invalid-rate`/weights) → `.map_err(|_| Error::InvalidValue)?`.

`run` for every action returns `Result<(), WriteFailed>` and, in practice, always
`Ok` (the loops convert `WriteFailed`/`BrokenPipe` to `Ok`); the `?` keeps the Zig
`try impl.run(...)` shape. `destroy(self)` consumes by value (no `alloc` arg).

### The `Error` enum (Zig's inferred `!void`)

Mirror the bench `Error` enum, trimmed to synthetic's error sources:

```rust
#[derive(Debug)]
pub enum Error {
    NoAction,                                   // return error.NoAction;
    InvalidAction,                              // stringToEnum(...) orelse InvalidAction
    MultipleActions,                            // two +actions
    Parse(rustty_args::args::ParseError),       // try cli.args.parse(...)
    InvalidValue,                               // try Utf8.create(...) → error.InvalidValue
    WriteFailed,                                // create/run WriteFailed (unreachable in practice)
}
impl From<rustty_args::args::ParseError> for Error {
    fn from(e: rustty_args::args::ParseError) -> Error { Error::Parse(e) }
}
```

(`?` on `rustty_args::args::parse(...)` uses the `From<ParseError>`.) Replace the
current `Result<(), ()>` signatures and the `#[allow(clippy::result_unit_err)]`
attributes on `main`/`main_action`/`main_action_impl_*` with `Result<(), Error>`.

---

## 5. `main()` / `detect_args` local replica

Mirror `crates/rustty-bench/src/cli.rs` exactly — `cli.action.detectArgs` lives in
`rustty-cli` (a heavy crate synthetic does not depend on → a cycle), so replicate
its scan locally over `std::env::args_os`. The synthetic `Action` declares no
special-case, so the special-case arm is elided (same as bench).

Add `Action::from_name` (the kebab tags are the enum names):

```rust
impl Action {
    /// `std.meta.stringToEnum(Action, name)`.
    pub fn from_name(name: &[u8]) -> Option<Action> {
        match name {
            b"ascii" => Some(Action::Ascii),
            b"osc" => Some(Action::Osc),
            b"utf8" => Some(Action::Utf8),
            _ => None,
        }
    }
}

// pub fn main() !void {
//     const alloc = std.heap.c_allocator;
//     const action_ = try cli.action.detectArgs(Action, alloc);
//     const action = action_ orelse return error.NoAction;
//     try mainAction(alloc, action, .cli);
// }
pub fn main() -> Result<(), Error> {
    let action_ = detect_args()?;
    let action = action_.ok_or(Error::NoAction)?;
    main_action(action, Args::Cli)
}

/// Local replica of `cli.action.detectArgs(Action, alloc)` (see bench cli.rs).
fn detect_args() -> Result<Option<Action>, Error> {
    let mut pending: Option<Action> = None;
    for arg in std::env::args_os() {
        #[cfg(unix)]
        let bytes: Vec<u8> = { use std::os::unix::ffi::OsStrExt; arg.as_os_str().as_bytes().to_vec() };
        #[cfg(not(unix))]
        let bytes: Vec<u8> = arg.to_string_lossy().into_owned().into_bytes();
        // Commands must start with "+"
        if bytes.is_empty() || bytes[0] != b'+' { continue; }
        if pending.is_some() { return Err(Error::MultipleActions); }
        pending = Some(Action::from_name(&bytes[1..]).ok_or(Error::InvalidAction)?);
    }
    Ok(pending)
}
```

`main_action` keeps its existing three-arm `match` but now returns `Result<(),
Error>` and calls the (now real) `main_action_impl_{ascii,osc,utf8}(args)`. The
`Args` enum (`Cli` / `String(&[u8])`) is already defined; the `.string` branch is
handled inside each `main_action_impl` (§4).

Note there is no `alloc` parameter in the Rust `main`/`main_action` (the Rust
`create` signatures dropped the allocator, unlike Zig's `create(alloc, opts)`).

---

## 6. The `[[bin]]` wrapper — `crates/rustty-synthetic/src/main_gen.rs`

`main_gen.zig` is 3 lines: `const synthetic = @import("synthetic/main.zig"); pub
const main = synthetic.cli.main;`. Faithful Rust binary entry:

```rust
//! PORT NOTE: `src/main_gen.zig` — `pub const main = synthetic.cli.main;`.
//! The binary entry for the synthetic generator CLI.

// const synthetic = @import("synthetic/main.zig");
// pub const main = synthetic.cli.main;
fn main() -> Result<(), rustty_synthetic::cli::Error> {
    rustty_synthetic::cli::main()
}
```

Returning `Result<(), Error>` (with `Error: Debug`) lets a non-`Ok` outcome
(`NoAction`, `InvalidAction`, a parse error) exit non-zero with a diagnostic —
matching Zig `main() !void` propagating the error to the runtime. `[[bin]] name =
"rustty-synthetic", path = "src/main_gen.rs"` (§0) makes `cargo run -p
rustty-synthetic -- +utf8 --seed=1` work; the lib crate stays intact for the
`#[cfg(test)]` surface.

---

## 7. R1 / test posture

- Every reference comment/line in `cli.zig` (the `// First, parse our CLI options.`
  / `// TODO: Make this a command line option.` / `// Our output always goes to
  stdout.` / `// Create our implementation` / `// Always flush` markers, and each
  `Options` field doc-comment) is preserved as `//`/PORT NOTE next to its port.
- The `Args` / `Action` enums and their doc-comments already exist — unchanged.
- Keep `tests::ref_all_actions` verbatim (it ports `test { inline for … _ = Impl; }`).
- The per-action `#[cfg(test)] mod tests` (`ascii` / `osc` / `utf8`) and the
  crate-root `ref_all_decls` stay untouched — the real generator logic is unchanged.
- Gate: `cargo build --workspace` + `cargo test -p rustty-synthetic` (the existing
  action/RNG tests, unaffected). The new glue is exercised structurally; the
  binary-entry `main`/`detect_args` remain untested-by-unit (same as bench),
  now real rather than `unimplemented!()`.

## No blockers

All referenced APIs exist and are acyclic: `rustty_args::args::{parse,
args_iterator, slice_iterator, parse_field_float, parse_field_int, parse_field_bool,
parse_field_enum, Target, Error/ParseError}` (member 15); `internal_os::args::InitError`
is the empty enum `{}` (→ `unwrap_or_else(|e| match e {})`); `rustty_os::string_encoding::{Writer,
WriteFailed}`; `crate::rand::{DefaultPrng, Rng}`. Disposition: ready.
