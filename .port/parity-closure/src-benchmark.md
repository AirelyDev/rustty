# Parity closure checklist — src/benchmark

7 open finding(s): 0 high, 5 medium, 2 low. Full evidence per row is in [`docs/parity-audit/src-benchmark.md`](../../docs/parity-audit/src-benchmark.md).

A row is CLOSED only when its fix has a test that fails without it (or a written, reviewed justification for why no test is possible).


## Medium (5)

- [x] **S62-F1** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** Args::String splits on whitespace instead of std.process.ArgIteratorGeneral (no quote/backslash handling)
  - **Oracle:** `src/benchmark/cli.zig:99-106` → **Rustty:** `crates/rustty-bench/src/cli.rs:303-319`
  - **Audit's suggested fix:** Move the `ArgIteratorGeneral` port from rustty-config/command.rs into a leaf crate rustty-bench can depend on (rustty-args or rustty-zigstd) and use it for `Args::String` in both rustty-bench and rustty-synthetic.
- [x] **S62-F2** · CONFIRMED · `LOG` · missing-branch
  - **What:** cli::main does not call global.init(.{ .tool = minimal }) — no process init (logging, GHOSTTY_LOG, rlimits, locale, onig/glslang, resources dir, signals)
  - **Oracle:** `src/benchmark/cli.zig:54-60` → **Rustty:** `crates/rustty-bench/src/cli.rs:201-208`
  - **Audit's suggested fix:** Call the rustty-global process-init path (a `tool` variant that skips app-action detection but performs the rest of `init`) from `cli::main`, or document each skipped side effect as an accepted divergence.
- [x] **S62-F3** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** report mode prints to stdout instead of stderr (std.debug.print)
  - **Oracle:** `src/benchmark/ScrollbackCompression.zig:257-270` → **Rustty:** `crates/rustty-bench/src/scrollback_compression.rs:460-467`
  - **Audit's suggested fix:** Use `eprintln!` for every `std.debug.print` port.
- [x] **S62-F4** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** report/roundtrip output printed to stdout instead of stderr
  - **Oracle:** `src/benchmark/TerminalFormatter.zig:330-355, 405-408, 415-423` → **Rustty:** `crates/rustty-bench/src/terminal_formatter.rs:622-647, 725-731, 746-751`
  - **Audit's suggested fix:** Replace `println!` with `eprintln!` at the four sites.
- [x] **S62-F5** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** snapshot report lines printed to stdout instead of stderr
  - **Oracle:** `src/benchmark/TerminalSnapshot.zig:298-314` → **Rustty:** `crates/rustty-bench/src/terminal_snapshot.rs:517-535`
  - **Audit's suggested fix:** Use `eprintln!`.

## Low (2)

- [x] **S62-F6** · CONFIRMED · `INDIV` · missing-error-path
  - **What:** read_slice_short treats EINTR (io::ErrorKind::Interrupted) as a read failure
  - **Oracle:** `src/benchmark/TerminalParser.zig:87-90` → **Rustty:** `crates/rustty-bench/src/options.rs:104-114`
  - **Audit's suggested fix:** In `read_slice_short`, `continue` on `e.kind() == ErrorKind::Interrupted`.
- [x] **S62-F7** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** roundtrip mismatch diagnostic escapes bytes with escape_ascii instead of std.zig.fmtString
  - **Oracle:** `src/benchmark/TerminalFormatter.zig:415-423` → **Rustty:** `crates/rustty-bench/src/terminal_formatter.rs:746-751`
  - **Audit's suggested fix:** Use the rustty-zigstd string-escape shim (or hand-roll the `std.zig.stringEscape` rules) instead of `escape_ascii`.
