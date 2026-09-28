# Parity audit — src/benchmark

Audited all 9 oracle units under `src/benchmark` (Ghostty pin `20abdb50a`) against `crates/rustty-bench`: 1971 lines read of 1971 lines in scope (100%; no unit left partially read). Counterparts: 8 units **ported**, 1 unit (`cli.zig`) **partial**; no unit missing, replaced, or not-applicable.
Findings: **7 CONFIRMED**, 0 DISPUTED, 0 REFUTED, 0 UNVERIFIED — by corrected severity: 0 high, 5 medium, 2 low.
The single most important gap is **S62-F2**: `cli::main` never performs the process init the reference does (`global.init(.{ .tool = minimal })`), so the port skips `GHOSTTY_LOG` logging config, startup info lines on stderr, rlimit raising, locale/`syncEnviron`, oniguruma/glslang init and resources-dir/i18n resolution. Closely behind is **S62-F1** (the C-API `Args::String` path tokenizes on whitespace and loses quote/backslash handling). The remaining four confirmed mediums are all one class: `std.debug.print` (stderr) ported as `println!` (stdout) across the four report/roundtrip sites.

## Coverage

| Oracle unit                               | Rustty counterpart(s)                                                     | Status  | Note                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| ----------------------------------------- | ------------------------------------------------------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `src/benchmark/main.zig`                  | `crates/rustty-bench/src/lib.rs`                                          | ported  | Re-export hub → `pub mod` per file (lib.rs:22-60); `test { refAllDecls }` → `tests::ref_all_decls` (lib.rs:100-139) touching every decl incl. options/osc_parser. Also carries the shared `terminal_options()` default reproduction (lib.rs:69-89), verified against `Terminal.zig:266-308` defaults (max_scrollback_bytes=10_000, cursor block/blink=false, 320MB kitty limit, `.direct`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `src/benchmark/options.zig`               | `crates/rustty-bench/src/options.rs`                                      | ported  | `dataFile` → `data_file` (options.rs:42-77): `None`→`Ok(None)`, `"-"`→stdin, else open by byte-exact `OsStr` path. Extra helpers `CacheLineBuf` and `read_slice_short` (port of the `std.Io` `readSliceShort` contract) live here; see the low EINTR finding S62-F6.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `src/benchmark/cli.zig`                   | `crates/rustty-bench/src/cli.rs`, `crates/rustty-bench/src/main_bench.rs` | partial | `Action` enum (14 tags, cli.rs:27-79 + round-trip test), `Args` union, `mainAction` match-monomorphized via a `BenchmarkAction` trait, `mainActionImpl` order (parse → create → benchmark → run(.once) → destroy on all paths) all ported. Two gaps: `main` omits `global.init(.{ .tool = minimal })` (S62-F2) and `Args::String` tokenizes on whitespace instead of `std.process.ArgIteratorGeneral` (S62-F1).                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `src/benchmark/TerminalParser.zig`        | `crates/rustty-bench/src/terminal_parser.rs`                              | ported  | `Options{data}`, create/destroy/benchmark vtable (step+setup+teardown), setup assert+open+warn, teardown close, step 4096-byte loop with EOF break, `noinline parseAll` → `#[inline(never)] parse_all`, `test TerminalParser` → `terminal_parser` with same `run(.once)`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `src/benchmark/TerminalStream.zig`        | `crates/rustty-bench/src/terminal_stream.rs`                              | ported  | Options defaults (80/120, continuation-enabled=false, continuation-max-bytes=1MiB, data=None), create wires `continuation_max_bytes = if enabled Some(..) else None` and the full `stream_terminal::Handler`, destroy order stream.deinit → terminal.deinit, setup fullReset+open, step 64KiB unbuffered loop, `test TerminalStream` (plain + continuation-enabled runs) → `terminal_stream`. One additional rustty-only test.                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `src/benchmark/ScrollbackCompression.zig` | `crates/rustty-bench/src/scrollback_compression.rs`                       | ported  | Options (mode=compress, 80/120, max-scrollback=10_000_000, data), `Mode` enum 5 tags + names, create passes `max_scrollback_bytes`, benchmark step switch + setup only (no teardown), setup fullReset/loadCorpus/warn/restore-precompress, loadCorpus 64KiB chunks, `pages()` → primary screen, step_noop/compress/incremental/restore/report, `visitColdPages` break-at-active-node, `percentage()`, all 3 tests with the same assertions (compressed_pages>0, visit>=compressed, recompress equal; drain vs full `MemoryStats` equal). Report output goes to stdout instead of stderr (S62-F3).                                                                                                                                                                                                                                                                              |
| `src/benchmark/TerminalFormatter.zig`     | `crates/rustty-bench/src/terminal_formatter.rs`                           | ported  | Options defaults (format/vt/screen/unwrap=false/pin-map=false/loops=25/24x80), `Mode`/`Emit`/`Region` enums + `Emit.format()`, create/destroy (terminal, output, pins), `setup_impl` (unlimited scrollback, 4096-byte corpus feed), teardown (deinit terminal, clear output/pins retaining capacity), `formatter()` region → selection (`getBottomRight` None → None), step_noop/format/report/roundtrip incl. Wyhash out/pin hashes, page/row counting via `Node::page`, first-diff offset with saturating −32/+32 window, `RoundtripMismatch`. All 4 tests ported with the same runs (roundtrip 4x8; 3x3 emit/region matrix loops=1; pin-map). Formatter `Options::plain/vt/html` verified equal to `formatter.zig:85-116` defaults. Report/roundtrip prints go to stdout instead of stderr (S62-F4), and the mismatch diagnostic uses a different escape alphabet (S62-F7). |
| `src/benchmark/TerminalResize.zig`        | `crates/rustty-bench/src/terminal_resize.rs`                              | ported  | Options defaults (cols/1/80x120/None/None/10_000/50_000_000/None), `Mode` 5 tags incl. `cols-no-reflow`, create with scrollback-bytes, benchmark step switch (`cols`\|`cols-no-reflow` → `step_cols`) setup-only, setup fullReset + asserts + fill + replayData + wraparound off for `cols-no-reflow`, deterministic fill (Xoshiro256++ seed 0xB3, i%64 SGR toggles, i%16==15 blank, len=min+uintLessThan(max−min), wide char every 8th line at j%16==8, space at j%8==7, intRangeAtMost a..z), replayData 4096-byte loop, resizeTerminal with cell_size_px 10x20, loop counts 50_000_000 / 25 / 1_500 / 2_500_000 / 25 × loops, targetCols/Rows `max(1, dim/2)`. `test TerminalResize` (cols, 10x20, fill 50) → `terminal_resize`.                                                                                                                                            |
| `src/benchmark/TerminalSnapshot.zig`      | `crates/rustty-bench/src/terminal_snapshot.rs`                            | ported  | Options defaults (encode/1/24x80/None), `Mode` 5 tags incl. `decode-ready`, create/destroy, `setup_impl` (unlimited scrollback, corpus feed, shrink + encode with continuation=`.ground`), teardown, step_noop/encode/decode (max_continuation_bytes 1MiB, deinit inside loop)/decode_ready (`Decoder::ready`)/report (envelope ENCODED_LEN offset, LE u16 tag + u32 len, unknown-tag warn+fail, per-tag totals in declaration order 1..7 verified against `record.zig:34-60`, framing = records×`Header.len` + envelope len). All 3 tests ported (default; decode 4x8; decode-ready 4x8). Report prints to stdout instead of stderr (S62-F5).                                                                                                                                                                                                                                 |

### Partially-read units

None. Every unit in scope was read in full (`units_partial` was empty).

## Findings

### High

None.

### Medium

#### S62-F1 — Args::String splits on whitespace instead of `std.process.ArgIteratorGeneral` (no quote/backslash handling)

- **Status:** CONFIRMED (both verifier lenses failed to refute)
- **Kind:** behavioral-difference
- **Severity:** medium (corrected: medium)
- **Oracle:** `src/benchmark/cli.zig:99-106`
- **Rustty:** `crates/rustty-bench/src/cli.rs:303-319`

**Description.** The `.string` arm of `mainActionImpl` (reached via the `ghostty_benchmark_cli`/`rustty_benchmark_cli` C API, `capi.rs:62`) parses the single args string with `std.process.ArgIteratorGeneral(.{})`, which honours double/single quotes and backslash escapes. The port tokenizes on ASCII whitespace only and admits in its PORT NOTE that quote/escape handling is not implemented. A C caller passing `--data="/path with space/x.vt"` gets a path containing the literal quote characters (open fails → `BenchmarkFailed`) in rustty, while ghostty opens the file; `--data=/a\ b` likewise diverges. A faithful `ArgIteratorGeneral` port already exists in the workspace at `crates/rustty-config/src/command.rs:402-620` (handles `"`, `'`, `\`), but rustty-bench does not depend on it.

**Evidence.** `cli.zig:100-103` `std.process.Args.IteratorGeneral(.{}).init(alloc, str)` vs `cli.rs:312-316` `str.split(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r')).filter(|t| !t.is_empty())` + `slice_iterator(&tokens)`. Same shortcut duplicated in `crates/rustty-synthetic/src/cli.rs:397-413`. The presence lens additionally confirmed `crates/rustty-bench/Cargo.toml` has no `rustty-config` dependency, so the relocation defence fails.

**Suggested fix.** Move the `ArgIteratorGeneral` port from `rustty-config/command.rs` into a leaf crate rustty-bench can depend on (`rustty-args` or `rustty-zigstd`) and use it for `Args::String` in both rustty-bench and rustty-synthetic.

#### S62-F2 — `cli::main` does not call `global.init(.{ .tool = minimal })` — no process init (logging, GHOSTTY_LOG, rlimits, locale, onig/glslang, resources dir, signals)

- **Status:** CONFIRMED (both verifier lenses failed to refute)
- **Kind:** missing-branch
- **Severity:** medium (corrected: medium)
- **Oracle:** `src/benchmark/cli.zig:54-60`
- **Rustty:** `crates/rustty-bench/src/cli.rs:201-208`

**Description.** The reference benchmark entry point first runs `try global.init(.{ .tool = minimal })` (`global.zig:55-232`): it sets up the global allocator/io, tmp dir, parses `GHOSTTY_LOG` into the logging config, installs signal handlers, emits `std.log.info` startup lines (`ghostty version=…`, `build optimize=…`, `renderer=…`, `libxev default backend=…`) to stderr (`Logging.stderr` defaults true for the exe), raises resource limits, ensures the process locale, initialises oniguruma/glslang, resolves the resources dir and i18n, and fails the process on any of those errors. The port's `main` goes straight to action detection. rustty-global's PORT NOTE (`crates/rustty-global/src/lib.rs:420-429`) declares the `tool` arm "N/A", but the justification only covers the skipped app-action detection, not the remaining observable init work; rustty-bench has no rustty-global dependency at all (`Cargo.toml`).

**Evidence.** `cli.zig:55` `try global.init(.{ .tool = minimal });` has no counterpart in `cli.rs:201-208` (`let alloc = Allocator; let action_ = detect_args()?; … main_action(alloc, action, Args::Cli)`). `grep -rn rustty_global crates/rustty-bench` → 0 hits (only a comment at `capi.rs:58`). Semantic lens pinned the concrete side effects: `global.zig:132-136` (action=null for `.tool`, so `logging.stderr` stays true), `:166-178` (seven `std.log.info` startup lines), `:180` rlimits `.init()`, `:204` `ensureLocale` + `syncEnviron`, `:208-211` glslang/oniguruma init, `:215` resourcesDir/i18n. `.port/phase-2/DEFERRALS.md` contains no deferral covering benchmark init.

**Suggested fix.** Call the rustty-global process-init path (a `tool` variant that skips app-action detection but performs the rest of `init`) from `cli::main`, or document each skipped side effect as an accepted divergence.

#### S62-F3 — `report` mode prints to stdout instead of stderr (`std.debug.print`)

- **Status:** CONFIRMED (both verifier lenses failed to refute)
- **Kind:** behavioral-difference
- **Severity:** medium (corrected: medium)
- **Oracle:** `src/benchmark/ScrollbackCompression.zig:257-270`
- **Rustty:** `crates/rustty-bench/src/scrollback_compression.rs:460-467`

**Description.** `stepReport` uses `std.debug.print`, which writes to stderr. The port uses `println!`, writing to stdout. Scripts that capture the report from stderr (or that redirect stdout, e.g. hyperfine's default capture of stdout) see different behaviour.

**Evidence.** `ScrollbackCompression.zig:257` `std.debug.print("scrollback-compression compressed={d} …` vs `scrollback_compression.rs:460` `println!("scrollback-compression compressed={} …`. No shadowing `println!` macro exists in `crates/rustty-bench/src`, and no PORT NOTE justifies the stream change; the same file uses `eprintln!` at `:295` for `log.warn`, so the divergence is specific to the `debug.print` report line.

**Suggested fix.** Use `eprintln!` for every `std.debug.print` port.

#### S62-F4 — report/roundtrip output printed to stdout instead of stderr

- **Status:** CONFIRMED (both verifier lenses failed to refute)
- **Kind:** behavioral-difference
- **Severity:** medium (corrected: medium)
- **Oracle:** `src/benchmark/TerminalFormatter.zig:330-355, 405-408, 415-423`
- **Rustty:** `crates/rustty-bench/src/terminal_formatter.rs:622-647, 725-731, 746-751`

**Description.** All three `std.debug.print` sites (report line, pins line, roundtrip summary and mismatch diagnostics) are stderr in the reference; the port uses `println!` (stdout) for each.

**Evidence.** `TerminalFormatter.zig:330` `std.debug.print("terminal-formatter emit={s} …` → `terminal_formatter.rs:622` `println!(…)`; `:351` → `:642`; `:405` → `:725`; `:415` → `:746`. Both lenses verified all four sites; no compensating redirection or PORT NOTE was found.

**Suggested fix.** Replace `println!` with `eprintln!` at the four sites.

#### S62-F5 — snapshot report lines printed to stdout instead of stderr

- **Status:** CONFIRMED (both verifier lenses failed to refute; both corrected the rustty line cite)
- **Kind:** behavioral-difference
- **Severity:** medium (corrected: medium)
- **Oracle:** `src/benchmark/TerminalSnapshot.zig:298-314`
- **Rustty:** as reported `crates/rustty-bench/src/terminal_snapshot.rs:517-535`; **corrected by both lenses to `terminal_snapshot.rs:562-585`, with the `println!` calls at `:567` (per-tag line) and `:579` (total line)** — lines 517-535 are the record-walk loop.

**Description.** The per-tag `terminal-snapshot tag=…` lines and the `terminal-snapshot total …` line are `std.debug.print` (stderr) in the reference and `println!` (stdout) in the port.

**Evidence.** `TerminalSnapshot.zig:300` and `:306` `std.debug.print(…)` vs the port's `println!(…)` at `terminal_snapshot.rs:567` and `:579`.

**Suggested fix.** Use `eprintln!`.

### Low

#### S62-F6 — `read_slice_short` treats EINTR (`io::ErrorKind::Interrupted`) as a read failure

- **Status:** CONFIRMED (both verifier lenses failed to refute; both medium confidence)
- **Kind:** missing-error-path
- **Severity:** low (corrected: low)
- **Oracle:** `src/benchmark/TerminalParser.zig:87-90`
- **Rustty:** `crates/rustty-bench/src/options.rs:104-114`

**Description.** The port-only `read_slice_short` helper (used by every benchmark's read loop in place of `std.Io.Reader.readSliceShort`) propagates any `Read::read` error, including `ErrorKind::Interrupted`, which each caller turns into `error.BenchmarkFailed`. Zig's std file reader retries on EINTR internally (`std.posix.read` loops on `.INTR`), so an interrupted `read(2)` on a data file / stdin never fails the benchmark in the reference but does in rustty.

**Evidence.** `options.rs:107` `let got = r.read(&mut buf[n..])?;` (no `Interrupted` retry) feeding `terminal_parser.rs:132-138`, `terminal_stream.rs:294-300`, `terminal_resize.rs:264-271`, `terminal_formatter.rs:423-429`, `terminal_snapshot.rs:331-337`, `scrollback_compression.rs:340-343`. Oracle call site `TerminalParser.zig:87-90` `r.readSliceShort(&buf) catch { … return error.BenchmarkFailed; }`. A grep of rustty-bench found no `Interrupted` handling anywhere. Confidence is medium because the Zig-side EINTR retry is asserted from std behaviour rather than read locally.

**Suggested fix.** In `read_slice_short`, `continue` when `e.kind() == ErrorKind::Interrupted`.

#### S62-F7 — roundtrip mismatch diagnostic escapes bytes with `escape_ascii` instead of `std.zig.fmtString`

- **Status:** CONFIRMED (both verifier lenses failed to refute)
- **Kind:** behavioral-difference
- **Severity:** low (corrected: low)
- **Oracle:** `src/benchmark/TerminalFormatter.zig:415-423`
- **Rustty:** `crates/rustty-bench/src/terminal_formatter.rs:746-751` (presence lens refines to `:743-751`, including the PORT NOTE at `:743-745`)

**Description.** The mismatch context windows are rendered with `std.zig.fmtString` (Zig string-literal escaping: `\n`, `\r`, `\t`, `\\`, `\"`, `\xNN` for other non-printables; `'` left as-is) in the reference and with `<[u8]>::escape_ascii` in the port (which additionally escapes `'` as `\'`). Diagnostics-only text differs for inputs containing single quotes; both sides still return `RoundtripMismatch`, so there is no control-flow or return-value effect.

**Evidence.** `TerminalFormatter.zig:420-421` `std.zig.fmtString(first[i -| 32..@min(first.len, i + 32)])` vs `terminal_formatter.rs:749` `first[i.saturating_sub(32)..core::cmp::min(first.len(), i + 32)].escape_ascii()`. A workspace-wide grep found a faithful `std.zig.stringEscape` port at `crates/rustty-args/src/args.rs:1453-1474` (`zig_string_escape`, tested at `crates/rustty-args/src/args/tests.rs:1556+`, used by `rustty-config/src/value_types.rs:6135` and `rustty-apprt/src/embedded.rs:1255`) — it is simply not used at this call site, so this is a deliberate substitution rather than a relocation.

**Suggested fix.** Use the existing zig string-escape shim (`rustty_args::args::zig_string_escape`) instead of `escape_ascii`.

### Unverified (over the verification cap)

None — all 7 findings were verified by both lenses.

## Performance-only differences (not findings)

- `options.rs:21,73-76` wraps files in a 256 KiB `BufReader` whereas the reference uses a 4096-byte (or 64 KiB / unbuffered) `std.Io.File.Reader`; identical bytes delivered, only syscall count differs.
- `scrollback_compression.rs:338` allocates the 64 KiB corpus dispatch buffer on the heap (`vec![0u8; 64*1024]`) instead of the stack; same chunking.
- `lib.rs:152-183` `do_not_optimize_away` is a register-constraint `asm` sink for integer sinks; pointer-valued sinks (`&self.terminal`, `written()`) use `core::hint::black_box`, matching Zig's `.pointer` memory-operand arm.
- `terminal_stream.rs:40-41,178-192` backs the OSC parser with a process-static `GlobalZigAllocator` and constructs the `Stream` twice (null handler then patched pointer) to obtain a stable self-referential terminal pointer; no observable difference.
- `terminal_resize.rs:449-451` hoists the `self.terminal.cols` read above the `vt_stream()` borrow; the stream cannot resize, so the value is identical.

## Refuted candidates

None. No candidate finding in this area was refuted by both verifier lenses, and none was refuted by one lens (no DISPUTED entries either) — all 7 raised findings survived verification as CONFIRMED.
