# Parity audit — src/cli

Audited 27 oracle units across 5 shards (all of `src/cli`, `src/cli/ssh-cache`, plus two `src/config` units swept in by shard 71), reading 9,162 of 9,162 in-scope oracle lines — 100% coverage, no partial units.
39 candidate findings were verified by two independent lenses: **30 CONFIRMED**, **4 DISPUTED** (one lens refuted), **5 REFUTED**, 0 UNVERIFIED / over-cap.
By corrected severity across the 34 surviving (confirmed + disputed) findings: **0 high, 13 medium, 21 low**. The originally-high `+version` finding was corrected to medium by the semantic lens (diagnostic output only).
The single most important gap is **S70-F1**: `rustty +version` omits the entire GTK diagnostic block (kernel version, desktop env, GTK/libadwaita build+runtime, libX11/libwayland) on the _default_ Linux/FreeBSD build, even though every ingredient (gtk_version.rs, adw_version.rs, `rustty_os::getKernelInfo`, `desktopEnvironment`, x11/wayland features) already exists — the deferral comment is stale.
Runner-up: **S67-F1**, the `, valid values are: …` enum suffix is never emitted anywhere in the workspace, degrading every enum config diagnostic in the CLI, `+validate-config`, and the C API.

## Coverage

| Oracle unit (path:lines)                                               | rustty counterpart(s)                                                                                                               | Status  | Note                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| ---------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/cli/args.zig` [1,1656]                                            | `crates/rustty-args/src/args.rs`, `src/args/tests.rs`, `tests/os_args_seam.rs`                                                      | partial | Comptime reflection (parse/parseIntoField/parseStruct/parseTaggedUnion/parseAutoStruct/parsePackedStruct) threaded through `Target`/`AutoStruct`/`PackedStruct` traits + `parse_field_*` helpers; all 36 reference tests have named counterparts. Deltas: formatValues suffix never emitted, LineIterator over-long-line handling, `_` separators in int/float, CommaSplitter errors collapsed, several parseIntoField tests exercise test-local re-implementations. |
| `src/cli/boo.zig` [1,240]                                              | `crates/rustty-cli/src/boo.rs`                                                                                                      | ported  | Options, OS gate, raw-DEFLATE frame decompression split on `0x01`, widget/event handler, draw (100x41 minimum), updateFrame span state machine, framerate 1000/30, outline fg index 4 all match. `vxfw.App.init` env-map argument dropped (vaxis crate is another shard).                                                                                                                                                                                            |
| `src/cli/crash_report.zig` [1,98]                                      | `crates/rustty-cli/src/crash_report.rs`                                                                                             | ported  | defaultDir → iterator → reports; empty+tty prints `No crash reports! 👻`; descending mtime sort; `now` vs rounded `{f} ago`; `{name} ({since})`; return 0. Stable `sort_by` only affects equal-mtime ordering.                                                                                                                                                                                                                                                       |
| `src/cli/diagnostics.zig` [1,197]                                      | `crates/rustty-args/src/diagnostics.rs`                                                                                             | ported  | Diagnostic.format prefixes, clone, Location{none,cli,file}, LocationKey, from_iter via LocationProvider, DiagnosticList{clone, append + precompute, empty, items, containsLocation} all match. `precompute_enabled` uses `cfg!(test)` (see refuted S67-F10).                                                                                                                                                                                                         |
| `src/cli/edit_config.zig` [1,162]                                      | `crates/rustty-cli/src/edit_config.rs`                                                                                              | ported  | Options/help, Config::load side effect, preferred_default_file_path, Windows arm, NoEditorConfigured + OSC8 path → 1, `/bin/sh -c exec`, exec-failure message → 1. Product name substituted per R2. Minor exec/message deltas below.                                                                                                                                                                                                                                 |
| `src/cli/explain_config.zig` [1,152]                                   | `crates/rustty-cli/src/explain_config.rs`                                                                                           | ported  | Manual arg loop, keybind>option>positional precedence, usage → stderr → 1, explainOption/explainKeybind lookup order, pager init vs default, `Unknown: '<name>'.` → 1, `explain` test with all 6 assertions.                                                                                                                                                                                                                                                         |
| `src/cli/ghostty.zig`                                                  | `crates/rustty-cli/src/rustty.rs`, `help_strings.rs`, `build.rs`, `test_util.rs`                                                    | ported  | Action enum (19 variants, same order), detectSpecialCase verbatim, run() help_error catch, runMain dispatch to all 19 modules, `file()` returns `.rs` (deliberate), `options()` reified as `option_specs`. All 4 tests ported with identical assertions.                                                                                                                                                                                                             |
| `src/cli/help.zig`                                                     | `crates/rustty-cli/src/help.rs`                                                                                                     | ported  | `Options{help:bool}` without a help decl, argsIterator+parse, header byte-equivalent modulo R2 rename (incl. the reproduced Ghostty.app/ghostty.app inconsistency), `  +<tag>` per action in enum order, trailer, flush, return 0.                                                                                                                                                                                                                                   |
| `src/cli/list_actions.zig`                                             | `crates/rustty-cli/src/list_actions.rs`                                                                                             | ported  | `Options{docs}`, help decl → ActionHelpRequested, `helpgen_actions::generate(stdout, Plaintext, opts.docs, RealHelpStrings)`, flush, return 0.                                                                                                                                                                                                                                                                                                                       |
| `src/cli/list_colors.zig`                                              | `crates/rustty-cli/src/list_colors.rs`, `pretty_arena.rs`, `tui.rs`                                                                 | ported  | `Options{plain}`, case-insensitive key sort, tty/plain gate, `name = #rrggbb` lines, prettyPrint column layout verbatim. Two deviations recorded as findings (missing cleanup on early error; added flush on plain path).                                                                                                                                                                                                                                            |
| `src/cli/list_fonts.zig`                                               | `crates/rustty-cli/src/list_fonts.rs`                                                                                               | ported  | `Options{family,style,bold,italic}`, run/runArgs split, `Discover == void` arm → wasm32, discover(monospace = family==null), failure → continue, getOrPut first-seen ordering, conditional sorts, output shape, flush, return 0. Two `log.err` calls dropped (finding).                                                                                                                                                                                              |
| `src/cli/list_keybinds.zig`                                            | `crates/rustty-cli/src/list_keybinds.rs`, `pretty_arena.rs`, `tui.rs`                                                               | ported  | `Options{default,docs,plain}`, tty gate, formatEntryDocs plain path, ChordBinding ordering comparator verbatim, prettyPrint styles/columns verbatim, iterateBindings width strings and leader recursion verbatim. TriggerNode intrusive list → `Vec<Trigger>` (same semantics).                                                                                                                                                                                      |
| `src/cli/list_themes.zig`                                              | `crates/rustty-cli/src/list_themes.rs`, `lorem_ipsum.txt`, `crates/rustty-zf/src/lib.rs`                                            | ported  | Full Options/ColorScheme/ThemeListElement/run/Preview surface as free fns over `DrawState`; `lorem_ipsum.txt` byte-identical. Divergences: search-box rendering bypasses `TextInput.drawWithStyle`, save-pane lines 12/14 reworded, error pane `{err:?}`, stderr flushed on early return, OOB-index guards added. Oracle has no tests; port adds 11.                                                                                                                 |
| `src/cli/new_tab.zig`                                                  | `crates/rustty-cli/src/new_tab.rs`                                                                                                  | ported  | Options incl. `_working_directory_seen`/`_arguments`/`_diagnostics`, parseManuallyHook split into hook + record_replay_arg + gather_arguments, checkArg, run/runArgs incl. cwd injection at index 0, `GHOSTTY_SURFACE_ID` fallback, performIpc, IPCFailed → 1. Only cosmetic error-text differences.                                                                                                                                                                 |
| `src/cli/new_window.zig`                                               | `crates/rustty-cli/src/new_window.rs`                                                                                               | ported  | Same shape as new_tab minus surface-id; class-only diagnostic filter; cwd injection; `performIpc .new_window`; unsupported-platform message. Only cosmetic error-text differences.                                                                                                                                                                                                                                                                                   |
| `src/cli/show_config.zig`                                              | `crates/rustty-cli/src/show_config.rs`                                                                                              | ported  | Options defaults (`changes-only` = true), help hook, Config.default vs Config.load, FileFormatter{changed, docs}, Pager::init vs default, format+flush, return 0. No divergences found.                                                                                                                                                                                                                                                                              |
| `src/cli/show_face.zig` [1,240]                                        | `crates/rustty-cli/src/show_face.rs`                                                                                                | ported  | Options + run/runArgs, `Discover == void` arm → wasm32 cfg, both diagnostic filters, SharedGridSet init/ref/deref, `--cp` then `--string` UTF-8 iteration, lookup arms. 0 reference tests. One low gap: freetype-load-flags not threaded.                                                                                                                                                                                                                            |
| `src/cli/ssh.zig` [1,667]                                              | `crates/rustty-cli/src/ssh.rs`, `ssh_cache/disk_cache.rs`, `crates/rustty-terminfo/src/entry.rs`, `crates/rustty-os/src/file.rs`    | ported  | usage text, Options + ConsumeRest hook, run (exit 2 on diagnostics), runInner session block, verbosePrint/warnPrint/Joined/checkExit/resolveDestination/parseDestination/installRemoteTerminfo/childExec all present. All 9 reference tests ported. Gap: `--flag=` reset-to-default not reproduced.                                                                                                                                                                  |
| `src/cli/ssh_cache.zig` [1,558]                                        | `crates/rustty-cli/src/ssh_cache.rs`, `ssh_cache/disk_cache.rs`, `ssh_cache/entry.rs`                                               | ported  | Options + positional/`--host=` lifting with deprecation warning, exit codes 1/2 arms, listEntries padding + ISO timestamp + relative age, matchesQuery, formatTimestamp clamp + civil-date math, relativeAge. All 6 reference tests ported. Gap: `--clear=` empty-value reset.                                                                                                                                                                                       |
| `src/cli/toggle_quick_terminal.zig` [1,63]                             | `crates/rustty-cli/src/toggle_quick_terminal.rs`, `crates/rustty-apprt/src/none.rs`, `gtk/app.rs`                                   | ported  | `Options{class}` + help hooks; run → `performIpc(.detect, .toggle_quick_terminal)`; IPCFailed → 1; else unsupported message + 1. Neither side parses Options in run(). 0 tests.                                                                                                                                                                                                                                                                                      |
| `src/cli/tui.zig` [1,6]                                                | `crates/rustty-cli/src/tui.rs`                                                                                                      | ported  | `can_pretty_print` → `CAN_PRETTY_PRINT = !cfg!(ios                                                                                                                                                                                                                                                                                                                                                                                                                   | tvos | watchos)`, consumed by list_colors.rs:134 and list_keybinds.rs:178 exactly as the reference. |
| `src/cli/validate_config.zig` [1,83]                                   | `crates/rustty-cli/src/validate_config.rs`                                                                                          | ported  | `Options{config-file}` with empty-value reset + help; run (parse, writer, runInner, flush); runInner (canonicalize + loadFile + loadRecursiveFiles else Config.load; finalize; print diagnostics → 1 else 0). 0 tests.                                                                                                                                                                                                                                               |
| `src/cli/version.zig` [1,80]                                           | `crates/rustty-cli/src/version.rs`, `crates/rustty-build-config/src/lib.rs`, `rustty-apprt/src/gtk/{gtk_version.rs,adw_version.rs}` | partial | OSC-8 tty hyperlink, header, Version block, Build Config block ported. The whole `app_runtime == .gtk` block is commented out although all its ingredients exist and `gtk` is a default feature on Linux/FreeBSD (S70-F1).                                                                                                                                                                                                                                           |
| `src/cli/ssh-cache/DiskCache.zig` [1,807]                              | `crates/rustty-cli/src/ssh_cache/disk_cache.rs`, `crates/rustty-os/src/hostname.rs`                                                 | ported  | All pub/private fns, MAX_CACHE_SIZE, the error set and all 14 test blocks have counterparts with matching assertions, except the two FailingAllocator tests (collapsed) and the host-validator deltas below. `program` → `rustty_identity::APP_NAME` (R2).                                                                                                                                                                                                           |
| `src/cli/ssh-cache/Entry.zig` [1,101]                                  | `crates/rustty-cli/src/ssh_cache/entry.rs`                                                                                          | ported  | Fields, parse (trim set, tokenize on `\|`, default terminfo token → `rustty_identity::TERMINFO_PRIMARY_NAME`), format, all 3 tests with identical assertions. Only delta: `_` digit separators in the timestamp.                                                                                                                                                                                                                                                     |
| `src/config/CApi.zig` [1,280] _(swept in by shard 71)_                 | `crates/rustty-config/src/c_api.rs`, `rustty-args/src/diagnostics.rs`, `rustty-apprt-c/src/lib.rs`                                  | ported  | All 13 exports 1:1 in signature and behaviour under the `rustty_config_*` prefix; config_trigger_, Diagnostic extern struct, String projection and all 8 tests present. The `ghostty_config_*` forwarder layer was deliberately deleted by the rebrand (see refuted S71-F6).                                                                                                                                                                                         |
| `src/config/ClipboardCodepointMap.zig` [1,44] _(swept in by shard 71)_ | `crates/rustty-config/src/clipboard_codepoint_map.rs`                                                                               | ported  | Entry/Replacement re-exports, `Vec<Entry>` list (perf-only), deinit, deep clone, add with the `range[0] <= range[1]` assert. Oracle has no tests.                                                                                                                                                                                                                                                                                                                    |

**Partial units:** there were **no** `units_partial` entries — every unit was read end to end. Two _counterparts_ are marked `partial` (`src/cli/args.zig`, `src/cli/version.zig`); those are structural-completeness ratings, not coverage gaps, and their deltas are captured as findings below.

## Findings

### High

None. (S70-F1 was filed as high and corrected to medium by the semantic lens: the divergence is diagnostic output only, with no behavioural consequence.)

### Medium

#### S67-F1 — formatValues enum suffix (`, valid values are: ...`) is never emitted: `Target::format_values` default is empty and no implementor overrides it

- **Status:** ✅ CONFIRMED (both lenses, high confidence)
- **Kind:** missing-branch
- **Oracle:** `src/cli/args.zig:240-286` (suffix built at 267-289)
- **rustty:** `crates/rustty-args/src/args.rs:292-294`, `762-800`

The reference's `formatValueRequired`/`formatInvalidValue` append `, valid values are: a, b, c` for any enum-typed field, so `--window-decoration=blah` yields `invalid value "blah", valid values are: none, auto, client, server`. rustty routes this through `Target::format_values`, whose default returns `Vec::new()`. `grep -rn 'fn format_values' crates` finds only that default; the production `impl Target for Config` (`crates/rustty-config/src/config_struct.rs:9525`) does not override it, so every enum diagnostic loses the suffix (user-visible in the CLI, `+validate-config`, and the C-API precomputed messages).

**Evidence:** `args.zig:268-286` iterates enum fields and prints `, valid values are: ` + names; `args.rs:292-294` `fn format_values(&self, _key: &[u8]) -> Vec<u8> { Vec::new() }`. `grep -rn "valid values are" crates` returns zero hits workspace-wide; the PORT NOTE at `args.rs:804-810` claims each destination implements it.

**Suggested fix:** Implement `format_values` in Config's `Target` impl (and other diagnostic-tracking Targets) to enumerate the enum field's variant names for `key`.

#### S67-F2 — LineIterator: a line longer than `MAX_LINE_SIZE-2` (4094) bytes is silently truncated and parsing continues, whereas the reference warns and stops iteration

- **Status:** ✅ CONFIRMED (both lenses, high confidence)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/args.zig:1437-1447` (writer at 1435, MAX_LINE_SIZE at 1397)
- **rustty:** `crates/rustty-args/src/args.rs:1790-1806`

The reference streams the line into a `.fixed(self.entry[2..])` writer; on overflow `Writer.fixedDrain` returns `error.WriteFailed`, which `streamDelimiterEnding ... catch |e| { log.warn("cannot read from \"{s}\": {}"); return null; }` turns into end-of-iteration: the remainder of the config file is **not** parsed and a warning is logged. rustty computes `let n = content.len().min(Self::MAX_LINE_SIZE - 2)` and copies only the first 4094 bytes, then continues with the next line, so an over-long line becomes a truncated (possibly valid) key/value and all subsequent lines are still applied. No warning is logged.

**Evidence:** `args.zig:1441-1444` vs `args.rs:1798` `let n = content.len().min(Self::MAX_LINE_SIZE - 2); self.entry[2..2 + n].copy_from_slice(&content[..n]);` followed by `continue`/`break cur_len`.

**Suggested fix:** When `content.len() > MAX_LINE_SIZE - 2`, log the `cannot read from "{filepath}": WriteFailed` warning and return `None` (and keep returning `None`), matching the fixed-writer overflow path.

#### S67-F3 — Integer fields reject `_` digit separators that `std.fmt.parseInt(T, v, 0)` accepts

- **Status:** ✅ CONFIRMED (both lenses, high confidence)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/args.zig:418-435`
- **rustty:** `crates/rustty-args/src/args.rs:1239-1274`

The reference parses integer fields with `std.fmt.parseInt(Int, value, 0)`, which ignores `_` between digits (rejecting only a leading/trailing underscore; Zig's own test asserts `parseUnsigned(u16,"65_535") == 65535`). rustty's `parse_int_auto` strips the sign and `0x`/`0o`/`0b` prefix and hands the rest to `from_str_radix`, which rejects any `_` — so `--scrollback-limit-bytes=10_000_000` (or any `u8..isize` field) parses in ghostty but produces an `invalid value` diagnostic in rustty. `parse_field_int` is production code used by `config_struct.rs:9717/9727`, `value_types.rs:82` and `binding.rs:1739+`.

**Evidence:** `std/fmt.zig:439-442` `if (buf_start[0] == '_' or buf_start[buf_start.len - 1] == '_') return error.InvalidCharacter; for (buf_start) |c| { if (c == '_') continue; ...`; `args.rs:1273` `T::from_str_radix_auto(&combined, radix)` with no underscore filtering.

**Suggested fix:** In `parse_int_auto`, reject leading/trailing `_` on the digit string, strip interior `_`, then call `from_str_radix`.

#### S67-F4 — Float fields reject `_` separators and hexadecimal floats that `std.fmt.parseFloat` accepts

- **Status:** ✅ CONFIRMED (both lenses, high confidence)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/args.zig:436-442`
- **rustty:** `crates/rustty-args/src/args.rs:1187-1191`

The reference uses `std.fmt.parseFloat(Float, value)`, which accepts underscore digit separators (`parse.zig:143`, `:278 validUnderscores`) and hexadecimal float literals (`parse_float.zig:10/41 convertHex`, with `hex.f32`/`hex.f64` tests). rustty's `parse_field_float` calls `str::parse::<f32/f64>()`, which rejects both (`1_000.5`, `0x1.8p3`). Any `f32`/`f64` field (font-size, mouse-scroll-multiplier bare fallback, …) therefore accepts a strictly smaller input set. Used in production by `config_struct.rs:9737` and `rustty-input/src/binding.rs:1727-1735`.

**Evidence:** `parse.zig:278 pub fn validUnderscores(...)`, `parse_float.zig:41 if (n.hex)`; `args.rs:1189-1190` `let s = std::str::from_utf8(v)...; s.parse::<T>()`.

**Suggested fix:** Add a `std.fmt.parseFloat`-compatible front end: validate/strip underscores per `validUnderscores` and handle the `0x` hex-float mantissa/exponent before delegating to `f64::from_str`.

#### S67-F6 — Six `parseIntoField` tests exercise test-local re-implementations of the generic rules rather than library code

- **Status:** ✅ CONFIRMED (both lenses, high confidence)
- **Kind:** weakened-assertion
- **Oracle:** `src/cli/args.zig:893-908, 719-737, 910-931, 1121-1140, 1142-1187`
- **rustty:** `crates/rustty-args/src/args/tests.rs:626-649, 241-300, 651-690, 1008-1040, 1042-1118`

Because `parseIntoField` became the per-Target trait method, the reference's generic rules — skip `_`-prefixed fields, empty value resets to the declared default, empty value calls `Field.init`, optional child-type handling and 1/2/3-arg `parseCLI` dispatch — are re-implemented inside each test's own `parse_into_field`. In _ignore underscore-prefixed fields_ the test impl is literally `Err(Error::InvalidField)` unconditionally (`tests.rs:634-642`), so its assertions pass regardless of any library behaviour. Assertion counts match, but none of these assertions constrain `rustty-args`; the corresponding production behaviour lives only in Config's hand-written `parse_into_field` (other shard).

**Evidence:** `tests.rs:634-642` `fn parse_into_field(...) { Err(Error::InvalidField) }` vs `args.zig:893-908` which calls the library `parseIntoField` and expects it to skip `_a`; `tests.rs:262-283` hard-codes `self.a = 42 // declared default` inside the test struct.

**Suggested fix:** Provide library-level helpers for the generic rules (e.g. `parse_field_with_default(value, default, parser)` / `parse_optional_field` and an underscore-key guard used by every Target) and make these tests drive those helpers.

#### S68-F1 — `list_colors` pretty_print: early `?` returns skip `tty.deinit`/`vx.deinit` (terminal left in raw mode)

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** missing-error-path
- **Oracle:** `src/cli/list_colors.zig:82-104`
- **rustty:** `crates/rustty-cli/src/list_colors.rs:206-224`

The oracle registers `defer tty.deinit()` and `defer vx.deinit(...)` immediately after creating them, so a failure of `try tty.writer().writeAll(vaxis.ctlseqs.unicode_set)` (line 90) or `try tty.getWinsize()` (line 103) still restores the termios state. The Rust port materialises those defers manually **after** `render_colors`, but the two fallible statements before it use `?` and return early, bypassing `vx.deinit(tty.writer())` and `tty.deinit()`. `rustty_vaxis::tty::PosixTty` has no `Drop` impl (the only `impl Drop` in rustty-vaxis is `VxfwArena`, `vxfw/mod.rs:841`), and `PosixTty::init` calls `make_raw` (`tty.rs:258`), so on those error paths the user's terminal stays in raw mode and `/dev/tty` is left open.

**Evidence:** `list_colors.rs:206` `tty.writer().write_all(ctlseqs::unicode_set)?;` and `:209` `let winsize = winsize_for(&tty)?;` precede the cleanup at `:220-224`; `tty.rs:288-297 pub fn deinit(self)` is the only termios restore.

**Suggested fix:** Capture the results of the unicode_set write and `winsize_for` into a `Result` (as is already done for `render_colors`) and run the three cleanup steps unconditionally before propagating; or give `PosixTty` a `Drop` that restores termios.

#### S68-F2 — `list_keybinds` pretty_print: early `?` returns skip `tty.deinit`/`vx.deinit` (terminal left in raw mode)

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** missing-error-path
- **Oracle:** `src/cli/list_keybinds.zig:226-251`
- **rustty:** `crates/rustty-cli/src/list_keybinds.rs:440-472` (verifier-corrected: `477-494`)

Same defect as in list_colors: the oracle's `defer tty.deinit()` / `defer vx.deinit()` fire when `try writer.writeAll(unicode_set)` (line 236) or `try tty.getWinsize()` (line 249) fail; the Rust port `?`s those two statements before its manual LIFO cleanup block, so a failure there returns without restoring the termios state set by `PosixTty::init`'s `make_raw`.

**Evidence:** `list_keybinds.rs:453` `tty.writer().write_all(ctlseqs::unicode_set)?;`, `:456` `let winsize = winsize_for(&tty)?;`, cleanup only at `:465-470`.

**Suggested fix:** Same as list_colors — fold the pre-render fallible steps into the captured `res` so the cleanup block runs on every exit.

#### S69-F1 — Search box does not use `TextInput.drawWithStyle`: no cursor, no horizontal scroll/ellipsis

- **Status:** ✅ CONFIRMED (both lenses, high confidence)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/list_themes.zig:823-836` (call at 835)
- **rustty:** `crates/rustty-cli/src/list_themes.rs:1345-1370` (verifier-corrected: `1358-1387`)

The reference renders the search widget via `self.text_input.drawWithStyle(child, self.ui_standard())`, which draws the buffer with `draw_offset`-based horizontal scrolling, an ellipsis when clipped, and calls `showCursor` at the cursor column. The port prints the concatenated `first_half ++ second_half` bytes with `child.print_segment(..., propt(0,0))` instead (documented as a "forced divergence"). Observable in the interactive picker: no cursor in the search input; a query longer than the box width clips instead of scrolling with the ellipsis marker; cursor-movement keys inside the search box have no visible effect. `rustty_vaxis::widgets::text_input::TextInput::draw_with_style` exists (`text_input.rs:289`, with the scroll loop at 305-319 and the `Op::Ellipsis`/cursor bookkeeping) but is never called.

**Evidence:** `list_themes.zig:835` vs `list_themes.rs:1360-1369`; `grep` for `draw_with_style` across `crates/` finds only `text_input.rs:273` and `:289` — no call site.

**Suggested fix:** Restructure lifetimes so `text_input` is `'a`-lived (hold it outside `DrawState` next to tty/vx, or give `DrawState` a `&'a mut TextInput` slot for the frame) and call `draw_with_style(child, ui_standard(scheme))` as the reference does.

#### S70-F1 — `+version` omits the entire GTK block (kernel, desktop env, GTK/libadwaita versions, libX11/libwayland) on the GTK build

- **Status:** ✅ CONFIRMED (both lenses, high confidence) — filed **high**, corrected to **medium** (diagnostic output only)
- **Kind:** missing-branch
- **Oracle:** `src/cli/version.zig:49-75`
- **rustty:** `crates/rustty-cli/src/version.rs:186-223` (comment only; no live GTK block anywhere in the workspace)

The reference prints, when `build_config.app_runtime == .gtk`: `  - kernel version: {s}` (Linux only, via `internal_os.getKernelInfo` with the `Kernel information unavailable` fallback), `  - desktop env   : {t}`, `  - GTK version   :` + `    build         :` + `    runtime       :`, `  - libadwaita    : enabled` + build/runtime lines, and `  - libX11`/`  - libwayland` `enabled|disabled`. rustty's `version.rs` carries this block only as a verbatim comment ("compiled out … ports with the GTK apprt (Phase-3)"). That deferral is **stale**: `rustty-apprt/src/gtk/gtk_version.rs` (BASELINE at :47, `runtime_version` at :103) and `adw_version.rs` exist, `rustty-os` has `getKernelInfo` (`kernel_info.rs:12`) and `desktopEnvironment` (`desktop.rs:147`), rustty-apprt exposes `gtk`/`x11`/`wayland` features, and `crates/rustty/Cargo.toml:38` makes `default = ["gtk", "wayland", "x11"]`, wiring `rustty-cli/gtk`. On the default Linux/FreeBSD build `rustty +version` stops after the libxev line and never prints the ten GTK diagnostic lines users paste into bug reports.

**Evidence:** `version.zig:49-75` `if (comptime build_config.app_runtime == .gtk) { ... }` vs the commented-out `version.rs:189-223`; the only workspace occurrence of `"GTK version   :"` is inside that comment.

**Suggested fix:** Add a `#[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]` block (guarded at runtime on `build_config::app_runtime() == Runtime::Gtk`) printing the kernel line (Linux only, with the exact fallback string), the `desktop env` tag from `rustty_os::desktopEnvironment(&environ_map)`, GTK/libadwaita build (BASELINE) and runtime versions as `{major}.{minor}.{micro}`, and `libX11`/`libwayland` enabled/disabled from `cfg!(feature = "x11")`/`cfg!(feature = "wayland")`.

#### S70-F2 — `+ssh` does not implement the `--flag=` (set-but-empty) reset-to-default rule for its five flags

- **Status:** ✅ CONFIRMED (both lenses, high confidence)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/ssh.zig:30-48` + the generic rule at `src/cli/args.zig:326-338`
- **rustty:** `crates/rustty-cli/src/ssh.rs:127-159` (guard absent at `132-159`)

The generic reference parser resets a field to its declared default when the value is present but empty. rustty-args delegates that step to each Target's `parse_into_field` (the driver at `args.rs:492` passes `Some(b"")` through unchanged), and `show_face.rs` / `validate_config.rs` / `ssh_cache.rs` (add/remove/prune) reproduce it, but `ssh.rs`'s Target does not for any field. Consequences: `--ssh=` yields `ssh = ""` so `Command::new("")` fails and the action prints `Error: failed to run : ...` and exits 1 (reference: resets to `"ssh"` and runs normally); `--verbose=` / `--terminfo=` / `--cache=` / `--forward-env=` feed `parse_bool(b"")` → `InvalidValue` → a diagnostic, so `run()` prints ``Error: unknown flag `--verbose`.`` plus usage and exits 2 (reference: silently resets the bool to its default and proceeds).

**Evidence:** `args.zig:326-338` applied to `ssh.zig:35-48` defaults (`forward-env`=true, `terminfo`=true, `cache`=true, `ssh`="ssh", `verbose`=false); `ssh.rs:134-158` dispatches all five arms straight to `parse_field_bool`/`parse_field_string`; `args.rs:1141-1143` returns `Err(Error::InvalidValue)` for empty.

**Suggested fix:** In `ssh.rs::parse_into_field`, before delegating, check `if let Some(v) = value { if v.is_empty() { <field> = <default>; return Ok(()); } }` for each of the five flags (true/true/true/`b"ssh"`/false), as `show_face.rs` does.

#### S70-F3 — `+ssh-cache --clear=` (empty value) errors instead of resetting to false

- **Status:** ✅ CONFIRMED (both lenses, high confidence)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/ssh_cache.zig:13` + `src/cli/args.zig:326-338`
- **rustty:** `crates/rustty-cli/src/ssh_cache.rs:60-63`

Reference: `--clear=` triggers the empty-value branch and resets `clear` to its default `false`, so the command proceeds to the default listing (exit 0 with the cache contents). rustty: the `clear` arm calls `parse_field_bool(Some(b""))` → `InvalidValue`, which `run()` maps to `Error: invalid flag value.` and exit 2. The sibling `add`/`remove`/`prune` arms in the same file (`ssh_cache.rs:65-97`) **do** implement the reset, so only `clear` diverges.

**Evidence:** `ssh_cache.rs:60-63` `b"clear" => { self.clear = args::parse_field_bool(value)?; Ok(()) }` vs the guarded `add` arm; `ssh_cache.rs:243-247` maps `InvalidValue` to exit 2.

**Suggested fix:** Add the same empty-value guard to the `clear` arm: `if let Some(v) = value { if v.is_empty() { self.clear = false; return Ok(()); } }`.

#### S71-F1 — `isValidHost` total-length limit: oracle accepts up to 255 bytes, rustty rejects above 253

- **Status:** ✅ CONFIRMED (both lenses, high confidence)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/ssh-cache/DiskCache.zig:432-449`
- **rustty:** `crates/rustty-cli/src/ssh_cache/disk_cache.rs:665-681`

At this pin `isValidHost` calls `std.Io.net.HostName.validate`, whose limit is `max_len = 255` (`Io/net/HostName.zig:20,35`; its own test accepts a 255-byte name). rustty's `is_valid_host` calls `rustty_os::hostname::isValid`, which has `if end > 253 { return false; }` (`crates/rustty-os/src/hostname.rs:73-75`, a port of the now-deleted ghostty `os/hostname.zig isValid`). A 254- or 255-byte hostname (or `user@<that host>`) is a valid cache key in ghostty (add/remove/contains proceed) but returns `error.InvalidCacheKey` in rustty. All other label rules are equivalent, and `isValidCacheKey` imposes no other length cap, so the path is reachable.

**Evidence:** `DiskCache.zig:436` vs `disk_cache.rs:669` → `hostname.rs:73`.

**Suggested fix:** Port `HostName.validate`'s limit (255) for the DiskCache validator — bump `rustty_os::hostname::isValid` to 255 (nothing else pins 253 at this pin) or add a `validate`-faithful helper in rustty-os and call it from `is_valid_host`.

#### S71-F2 — `Entry.parse` timestamp: Zig `parseInt` accepts `_` digit separators, Rust `parse` rejects them

- **Status:** ✅ CONFIRMED (both lenses; presence lens suggested low, semantic kept medium — recorded at **medium**)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/ssh-cache/Entry.zig:21-27`
- **rustty:** `crates/rustty-cli/src/ssh_cache/entry.rs:153-161`

`std.fmt.parseInt(i64, s, 10)` skips `_` between digits (`fmt.zig:438-442`), rejecting only a leading/trailing underscore. rustty's `parse_int_i64` uses `str::parse::<i64>()`, which rejects any `_`. A cache line such as `host|1_700_000_000|xterm-rustty` yields a valid entry (timestamp 1700000000) in ghostty but is logged as an invalid timestamp and dropped in rustty, so `contains()`/`list()`/`prune()` disagree on such files. Sign handling is otherwise equivalent.

**Evidence:** `fmt.zig:440-443` vs `entry.rs:157` `Ok(text) => text.parse::<i64>()`; the comment at `entry.rs:153-161` claims it "matches std.fmt.parseInt" but has no underscore handling.

**Suggested fix:** In `parse_int_i64`, reject a leading/trailing `_` and strip interior `_` before calling `parse` (or hand-roll the base-10 accumulate loop mirroring `parseIntWithSign`, including overflow → error).

### Low

#### S67-F5 — `parse_auto_struct` collapses CommaSplitter errors into `InvalidValue`; the reference propagates them as distinct errors

- **Status:** ✅ CONFIRMED (both lenses, medium confidence) — severity corrected **medium → low** (message text only)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/args.zig:540-542`
- **rustty:** `crates/rustty-args/src/args.rs:942`

The reference's `loop: while (try iter.next()) |entry|` propagates `CommaSplitter.Error {UnclosedQuote, UnfinishedEscape, IllegalEscape}` unchanged. Callers that switch on `error.InvalidValue` (e.g. `Config.zig:9282-9290` MouseScrollMultiplier) therefore do **not** take the InvalidValue fallback for a malformed quote, and `parse`'s diagnostic arm renders them via the `else => "unknown error {}"` arm. rustty maps every splitter error to `Error::InvalidValue`, so callers see InvalidValue and diagnostics read `invalid value "..."`. Both verifiers noted the only observable consequence is the rendered message: every splitter-error input contains a quote/backslash that the bare-float fallback also rejects, so the value is rejected either way.

**Evidence:** `args.zig:542` `loop: while (try iter.next()) |entry| {`; `CommaSplitter.zig:28`; `args.rs:942` `while let Some(entry) = iter.next().map_err(|_| Error::InvalidValue)? {`.

**Suggested fix:** Widen `ParseError` with the three CommaSplitter variants (or a `Splitter(comma_splitter::Error)` variant) and propagate them; render them through the `unknown error {err}` diagnostic arm in `parse`/`parse_self`.

#### S67-F8 — `parse: compatibility renamed` does not exercise the library `compatibility_renamed` helper

- **Status:** ⚠️ DISPUTED (presence lens refuted; semantic lens confirmed) — severity low
- **Kind:** weakened-assertion
- **Oracle:** `src/cli/args.zig:866-891`
- **rustty:** `crates/rustty-args/src/args/tests.rs:560-624` (test at 597-620; helper at `args.rs:742-766`)

The reference test registers `compatibilityRenamed(@This(), "a")` and thereby tests the helper. The rustty test builds its own inline closure `matches!(dst.parse_into_field(b"a", value), Ok(()))` (`tests.rs:603-609`) instead of calling `compatibility_renamed(b"a")` (`args.rs:742`), so the shipped helper (used by Config for background-blur-radius / adw-toolbar-style / scrollback-limit) has no unit test in this crate.

**Evidence:** `tests.rs:597-612` comment "the handler for \"old\" is dispatched here by name and forwards to parse_into_field" vs `args.zig:875` `.{ "old", compatibilityRenamed(@This(), "a") }`.

**Suggested fix:** Return `Some(compatibility_renamed(b"a"))` from the test's `compatibility()` so the library closure is what runs.

**Refuting lens — presence (medium confidence), verbatim:**

> Assertion count and content match the oracle exactly: args.zig:886-890 asserts `_arena != null`, `data.a`, `data.b`; tests.rs:617-620 asserts `data.arena_set`, `data.a`, `data.b`. Nothing is fewer or looser, so it is not a weakened-assertion. The inline closure at tests.rs:605-609 (`matches!(dst.parse_into_field(b"a", value), Ok(()))`) is byte-for-byte the semantics of compatibility_renamed's body (args.rs:749-765: forward to parse_into_field(to, value); false on Err, true on Ok) and the deviation is documented at tests.rs:589-594. The shipped helper is exercised in production by config_struct.rs:9554/9560/9567. This is a coverage-nuance nit about which of two behaviourally identical closures runs, not a functional-parity difference.

#### S67-F9 — Three log messages are dropped (compat-handler info, renamed-field warn, LineIterator read-failure warn)

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/args.zig:144-147, 222-225, 1442`
- **rustty:** `crates/rustty-args/src/args.rs:509-510, 752, 1790`

The reference emits `log.info("compatibility handler for {s} handled error, you may be using a deprecated field: {}")` when a compat handler succeeds, `log.warn("error parsing renamed field {s}: {}")` when `compatibilityRenamed` fails, and `log.warn("cannot read from \"{s}\": {}")` on a LineIterator read/overflow failure. rustty keeps all three only as comments; nothing is logged. The presence lens notes this is an instance of a workspace-wide port convention (no logging facade is wired into these crates; cf. `list_fonts.rs:31-34`), better tracked once globally than per shard.

**Evidence:** `args.rs:509` and `args.rs:752` are commented-out log lines; `LineIterator::next` (`args.rs:1770-1845`) has no logging at all.

**Suggested fix:** Emit the messages through the crate's logging facade (as other rustty crates do for `std.log.scoped`).

#### S67-F11 — Editor is exec'd via `std::process::Command::exec` instead of a raw `execve`, changing the inherited signal state

- **Status:** ✅ CONFIRMED (both lenses, medium confidence)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/edit_config.zig:129-136` (execve at 135)
- **rustty:** `crates/rustty-cli/src/edit_config.rs:234-243` (verifier-corrected: `236-239` / `262-273`)

The reference calls `execve("/bin/sh", argv, std.c.environ)` directly, so the editor inherits the process's signal disposition (the Zig runtime sets SIGPIPE to `SIG_IGN`). Rust's `CommandExt::exec` goes through std's child setup, which resets SIGPIPE to `SIG_DFL` before `execvp`. The editor therefore starts with a different SIGPIPE disposition than under ghostty. **Correction from the presence lens:** std explicitly does _not_ reset the signal mask ("Inherit the signal mask from the parent rather than resetting it", `unix.rs:352-353`), so only the SIGPIPE disposition differs. PATH lookup is irrelevant since the program is the absolute `/bin/sh`.

**Evidence:** `edit_config.zig:135` `std.posix.system.execve(argv[0].?, &argv, envp)`; `edit_config.rs:236-239` `std::process::Command::new("/bin/sh").arg("-c").arg(...).exec()`; `library/std/src/sys/process/unix/unix.rs:224-239, 350-370`.

**Suggested fix:** Call `execve` directly (via the crate's existing raw-syscall shim) or use `Command` with the signal-reset behaviour disabled.

#### S67-F12 — exec-failure message: command bytes printed lossily and errno name falls back to a number outside the transcribed execve table

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/edit_config.zig:138-156`
- **rustty:** `crates/rustty-cli/src/edit_config.rs:253-266, 283-306, 379-382` (verifier-corrected: `296-312`, `340-369`, `431-433`)

The reference prints `Command: {s}` with the raw command bytes and `(E{t})` with `@tagName` for whatever errno `execve` returned. rustty prints the command through `String::from_utf8_lossy` (non-UTF-8 bytes become U+FFFD) and maps errno to a name only for the 17 transcribed `execve(2)` codes, printing the bare number otherwise — reachable on macOS where `execve` can return `EBADARCH`/`EBADEXEC`/`ESHLIBVERS`/`ENOTSUP`, giving e.g. `E86` where ghostty prints `EBADARCH`. The same function deliberately uses byte-exact `write_all` for the path lines, so the lossy command line is an inconsistency.

**Evidence:** `edit_config.rs:254` `command = show(command_bytes)` (lossy, `show` at `:431-433`); `edit_config.rs:300-305` `.map_or_else(|| code.to_string(), ...)`.

**Suggested fix:** Write the command bytes with `write_all` like the path lines, and extend the errno table (or derive names from the platform's full `E` enum).

#### S68-F3 — `list_colors` plain path flushes stdout; oracle never flushes its buffered writer

- **Status:** ✅ CONFIRMED (both lenses, medium confidence) — deliberate oracle-bug fix
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/list_colors.zig:58-73`
- **rustty:** `crates/rustty-cli/src/list_colors.rs:148-166`

At the pin, the oracle's non-tty/`--plain` branch writes all `name = #rrggbb` lines into a 4096-byte `File.Writer` buffer and returns 0 without `flush()` (`grep -n flush src/cli/list_colors.zig` returns nothing, while `list_keybinds.zig:85` and `list_fonts.zig:168` do flush), so up to 4 KiB of trailing output is dropped by the oracle. The port adds an explicit `writer.flush()?` (PORT NOTE at `:144-147`) and thus prints the complete list. This is a deliberate fix of an oracle bug rather than a fidelity regression, but it IS an observable output difference; recorded so the parent can decide how to classify it.

**Evidence:** `list_colors.zig:59-71` + `:73 return 0;` with no flush; `list_colors.rs:166 writer.flush()?;`.

**Suggested fix:** None required (keep the flush); optionally note it in DEFERRALS/PORT NOTES as an intentional oracle-bug deviation.

#### S68-F4 — `list_fonts` drops the two `log.err` messages on familyName/name failure

- **Status:** ✅ CONFIRMED (both lenses, high confidence)
- **Kind:** other
- **Oracle:** `src/cli/list_fonts.zig:120-131`
- **rustty:** `crates/rustty-cli/src/list_fonts.rs:311-331`

The oracle logs `failed to get font family name: {err}` / `failed to get font name: {err}` via `std.log.scoped(.list_fonts)` before `continue`-ing; the port keeps the `continue` but emits nothing (the log calls are comments). Control flow is identical; only the stderr diagnostic is missing. Other cli modules (`ssh.rs:997`) do emit `[warn] [scope] msg` to stderr, so the crate has a precedent for keeping these, and there is no documented rustty-cli log-drop policy (the "§10.16 log.* dropped" note exists only in rustty-vaxis).

**Evidence:** `list_fonts.rs:313-316` and `:327-330` `Err(_err) => { // log.err(...) (no-op) continue; }`.

**Suggested fix:** `eprintln!("[error] [list_fonts] failed to get font family name: {err:?}")` (and the name variant) before `continue`, matching the `ssh.rs` convention.

#### S68-F5 — help-text write/flush failure warning lacks the `std.log` level/scope prefix

- **Status:** ✅ CONFIRMED (both lenses, high confidence)
- **Kind:** other
- **Oracle:** `src/cli/ghostty.zig:133-141`
- **rustty:** `crates/rustty-cli/src/rustty.rs:372-381`

The oracle reports the failure through `std.log.warn(...)`, which ghostty's `logFn` renders with a `warning:`-style level prefix; the port uses bare `eprintln!("failed to write help text: ...")`. Exit code (1) and ordering are identical; only the stderr text format differs. Cosmetic.

**Evidence:** `rustty.rs:374` / `:380` vs `ghostty.zig:135` `std.log.warn("failed to write help text: {}\n", ...)`.

**Suggested fix:** Use the crate's `[warn] [...]` stderr convention (`ssh.rs`) or a shared log helper.

#### S69-F2 — Save-pane instruction lines 12 and 14 reworded beyond the Ghostty→Rustty token rename

- **Status:** ✅ CONFIRMED (both lenses, high confidence)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/list_themes.zig:866-880` (lines 877, 879)
- **rustty:** `crates/rustty-cli/src/list_themes.rs:1393-1416` (verifier-corrected: `1431-1435`)

Reference line 12 reads `For more details on configuration and themes, visit the Ghostty documentation:` and line 14 `https://ghostty.org/docs/config/reference`. The port prints `For more details on configuration and themes, visit the config reference:` and `https://rustty.org/docs/config/reference`. The first is a wording change not covered by the per-token rename (every other line in the same array applies the rename faithfully); the second is the sanctioned rename applied to a URL (yielding a non-existent host).

**Evidence:** `list_themes.zig:877/879` vs `list_themes.rs:1411-1415`.

**Suggested fix:** Restore `visit the Rustty documentation:` (token rename only) and decide explicitly on the docs URL.

#### S69-F3 — Theme-load error pane prints Debug repr instead of the reference `{}` error name

- **Status:** ✅ CONFIRMED (both lenses, high confidence)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/list_themes.zig:952-966` (line 953)
- **rustty:** `crates/rustty-cli/src/list_themes.rs:1508-1516` (line 1510)

When `loadFile` fails, the reference prints `std.fmt.allocPrint(alloc, "{}", .{err})` which yields `error.FileNotFound` etc. The port prints `format!("{err:?}")` (Debug of the port's `LoadError`), so the third error-pane line differs textually — and, since centring uses `text_len`, the layout shifts too.

**Evidence:** `list_themes.zig:953` vs `list_themes.rs:1510` `let text = arena.dupe(format!("{err:?}").as_bytes());`.

**Suggested fix:** Implement `Display` for `LoadError` producing `error.<Name>` (or map to the Zig error-name string) and use it here.

#### S69-F4 — Port flushes stderr; reference never flushes its buffered stderr writer in `run()`

- **Status:** ✅ CONFIRMED (both lenses) — deliberate oracle-bug fix
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/list_themes.zig:120-127, 171-174`
- **rustty:** `crates/rustty-cli/src/list_themes.rs:281-299, 356-364`

The reference writes the "Could not find the Ghostty resources directory…" warning and the "No themes found, check to make sure…" message into a 4096-byte buffered stderr writer and never calls `stderr.flush()` on any path (the only flushes in the file are for stdout), so those messages are dropped at process exit. The port explicitly flushes (`stderr.flush()?` before `return Ok(1)`, and BufWriter flush-on-drop for the warning), so the messages appear. Almost certainly a reference bug, but an observable stderr difference under bug-for-bug parity.

**Evidence:** `list_themes.zig:171-174` with no flush anywhere in `run` (only `try stdout.flush();`) vs `list_themes.rs:357-364`.

**Suggested fix:** Keep the port behaviour (it is the intended one) but record the deliberate divergence in the PORT NOTE; or, for strict parity, drop the flush.

#### S69-F5 — `Error parsing args` message formats the error differently (Debug/Display vs Zig `{}` error name)

- **Status:** ✅ CONFIRMED (both lenses, high confidence)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/new_tab.zig:208-214` (and `new_window.zig:194-200`)
- **rustty:** `crates/rustty-cli/src/new_tab.rs:425-441` (and `new_window.rs:360-376`)

Reference prints `"Error parsing args: {}\n"` with a Zig error value, rendering as `error.InvalidCharacter` / `error.FileNotFound`. The port prints `{err:?}` for `args::Error` (e.g. `InvalidField`) and, for checkArg failures surfaced via `gather_arguments`, `CheckArgError`'s Display which is `{e:?}` for ParseInt/ExpandHome and `io::Error` Display (e.g. `No such file or directory (os error 2)`) for realpath failures. Same exit code and position; only the stderr text differs.

**Evidence:** `new_tab.zig:212` vs `new_tab.rs:432/439` with `CheckArgError::fmt` at `new_tab.rs:74-84`.

**Suggested fix:** Format as `error.<ZigName>` for each variant (map io::Error kinds to the Zig `RealPathFileError` names) to match the reference text.

#### S69-F7 — Port adds empty-filter guards where the reference indexes out of bounds (Debug panic / ReleaseFast UB)

- **Status:** ⚠️ DISPUTED (presence lens refuted; semantic lens confirmed) — severity low, no change recommended
- **Kind:** other
- **Oracle:** `src/cli/list_themes.zig:484-495, 839-863, 407-425`
- **rustty:** `crates/rustty-cli/src/list_themes.rs:900-916, 1381-1391, 2318-2325` (verifier-corrected: `960-976`, `1388-1412`, `2316-2327`)

Three places: (1) `c`/`C` clipboard copy in normal mode indexes `filtered.items[current]` unguarded in the reference; the port skips the copy when `filtered` is empty. (2) The `.save` draw reads `themes[filtered.items[current]]` unguarded; the port renders an empty `theme = ` line instead. (3) `updateFiltered` reads an `undefined` `selected` slice when the prior filtered list was empty; the port models it as `None` → `(0,0)`. In Zig Debug these are bounds/undefined panics (a TUI crash), in ReleaseFast undefined behaviour; the port continues. Recorded because the reference behaviour is not well-defined.

**Evidence:** `list_themes.zig:484-489` vs `list_themes.rs:903-909` (`if !st.filtered.is_empty() { ... }`); `list_themes.zig:839` vs `list_themes.rs:1383-1391`.

**Suggested fix:** No change required; keep the PORT NOTEs (issue 4) documenting the UB resolution.

**Refuting lens — presence (high confidence), verbatim:**

> Read both sides. Oracle ghostty/src/cli/list_themes.zig:485-493 indexes self.themes[self.filtered.items[self.current]] unguarded, :839 does the same in the .save branch, and :407-425 reads an `undefined` `selected` when the prior filtered list was empty. All three oracle paths are a Debug bounds/undefined panic or ReleaseFast UB — there is no well-defined observable behaviour to diverge from, so there is nothing to be non-parity with. The port implements exactly the documented resolution and is present at rustty crates/rustty-cli/src/list_themes.rs:961-976 (`PORT NOTE (§1 divergence) ... (issue 4) guard the `filtered[current]` index on an empty filter` around both the 'c' and shift-'c' copies) and :2316-2327 (`fn reselect` with `PORT NOTE: the reference reads an `undefined` `selected`... the port models "no prior selection" as`None` → (0, 0)`), plus the same issue-4 note pattern at :1396-1400 for the wrapping `end/G` case. The finding's own text says 'No change required' and 'the reference behaviour is not well-defined'; that is a perf/robustness note, not an observable functional difference. Also note the cited rustty line numbers were stale (the clipboard guard is at 960-976, not 900-916).

#### S70-F4 — `+version` Build Config line formats differ from the reference `{}` enum rendering

- **Status:** ✅ CONFIRMED (both lenses, medium confidence)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/version.zig:43-46`
- **rustty:** `crates/rustty-cli/src/version.rs:159-186`

The reference prints `  - build mode    : {}` / `  - app runtime   : {}` / `  - font engine   : {}` with Zig's default enum formatting (`.<tag>`: `.Debug`, `.gtk`, `.coretext`, `.fontconfig_freetype`). rustty prints `build_config::mode_string()` (`Debug`/`ReleaseFast`, no leading dot; ReleaseSafe/ReleaseSmall cannot be reported) and `{:?}` Debug of the Rust enums (`Gtk`/`None`, `Coretext`/`FontconfigFreetype`). The toolchain line is also relabelled `  - Rust version  :` instead of `  - Zig version   :` (an unavoidable toolchain fork, not itself a defect). Cosmetic, but any script that greps `+version` output sees different tokens.

**Evidence:** `version.zig:43-46` vs `version.rs:162-184`; `rustty-build-config/src/lib.rs:123-129 mode_string`; `apprt.rs:14-23 Runtime { None, Gtk }`; `font.rs:10-40 Backend`.

**Suggested fix:** Emit `.{snake_tag}` for runtime/font backend (add a `tag()` accessor returning the reference `@tagName` strings) and `.Debug`/`.ReleaseFast` for build mode; keep the Rust-version relabel only if the fork is an accepted R2 decision.

#### S70-F5 — `+show-face` drops the user's `freetype-load-flags` on freetype targets when building the font DerivedConfig

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/show_face.zig:175-178`
- **rustty:** `crates/rustty-cli/src/show_face.rs:701-707` (verifier-corrected: `713-718`)

The reference `SharedGridSet.DerivedConfig.init(alloc, config)` copies `config.@"freetype-load-flags"` whenever `FreetypeLoadFlags != void` (Linux/BSD fontconfig_freetype). rustty's cli-local bridge hardcodes `freetype_load_flags: FREETYPE_LOAD_FLAGS_DEFAULT` on every target, so on Linux the configured hinting/autohint/monochrome/light flags are not threaded into the grid key/face options used for the lookup. rustty-core already has the exact conversion (`freetype_load_flags_from_config`, `surface.rs:1223-1233`) and threads it in its own copy of this bridge (`surface.rs:1202`), so the two supposedly identical bridges diverge. Observable impact is limited (the face chosen for a codepoint does not depend on load flags), hence low — it survives chiefly as a stale PORT-NOTE deferral in production code.

**Evidence:** `show_face.zig:175` → `SharedGridSet.zig DerivedConfig.init` (echoed at `shared_grid_set.rs:820`); `show_face.rs:701-707` with the comment "the config value flows on freetype targets when that DI lands"; `face/mod.rs:158-159`.

**Suggested fix:** Replace the constant with a cfg-split helper mirroring rustty-core's `derived_freetype_load_flags` (call `rustty_core::surface::freetype_load_flags_from_config(&config.freetype_load_flags)` on non-macOS/non-wasm targets).

#### S70-F6 — Install-failure warning text differs: reference prints the error tag `InstallFailed`, port prints ad-hoc sentences

- **Status:** ✅ CONFIRMED (both lenses, high confidence)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/ssh.zig:281-283, 530-545`
- **rustty:** `crates/rustty-cli/src/ssh.rs:525-531, 903-923` (verifier-corrected: `528-531`, `900-926`)

The reference's `installRemoteTerminfo` returns `error.InstallFailed` for spawn failure, wait failure and non-zero exit alike, and the caller prints `Warning: failed to install terminfo: InstallFailed` (`{t}`). rustty returns three distinct `io::Error::other(...)` values whose Display is `terminfo install spawn failed` / `terminfo install wait failed` / `terminfo install failed`, so the warning line reads differently in each case. Similarly `show_face.rs` prints `{err:?}` (Rust Debug, no `error.` prefix) where the reference `{}` on an error prints `error.<Name>`.

**Evidence:** `ssh.zig:282` `warnPrint(stderr, "failed to install terminfo: {t}", .{err})` with `ssh.zig:532/543/545 return error.InstallFailed`; `ssh.rs:526` + `:907/:917/:922`; `show_face.zig:112,145,164,184,194,226,233` vs `show_face.rs:257,318,415,447,466,533,552`.

**Suggested fix:** Return a single `InstallFailed`-named error (Display = `InstallFailed`) from `install_remote_terminfo`, and format reference `{}`-printed errors as `error.<Name>` in `show_face.rs`.

#### S70-F7 — `toggle_quick_terminal` `Options.class` lacks the empty-value reset (not reachable at runtime)

- **Status:** ✅ CONFIRMED (both lenses, medium confidence)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/toggle_quick_terminal.zig:9` + `src/cli/args.zig:326-338`
- **rustty:** `crates/rustty-cli/src/toggle_quick_terminal.rs:57-60` (context `48-66`)

`class: ?[:0]const u8 = null` would be reset to null by the reference parser on `--class=`; the rustty Target sets `Some(vec![])`. Neither the reference `run` nor the port ever calls the arg parser for this action (both verified), so this is unobservable today, but the Target is a public parse surface that diverges from the args.zig rule — and the sibling `validate_config.rs:47-53` shows the port's own correct pattern.

**Evidence:** `toggle_quick_terminal.rs:57-60` `b"class" => { self.class = Some(rustty_args::args::parse_field_string(value)?); Ok(()) }`; `toggle_quick_terminal.zig:41-63` and `toggle_quick_terminal.rs:98-145` both never parse.

**Suggested fix:** Add the `if v.is_empty() { self.class = None; return Ok(()); }` guard, matching `validate_config.rs:49-55`. (The `parse_field_sentinel_string` half of the fix is cosmetic — both helpers return `Vec<u8>`.)

#### S71-F3 — IPv6 fallback accepts embedded-IPv4 forms the oracle rejects

- **Status:** ✅ CONFIRMED (both lenses, high confidence)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/ssh-cache/DiskCache.zig:442-448`
- **rustty:** `crates/rustty-cli/src/ssh_cache/disk_cache.rs:676-681`

Oracle `isValidHost` falls back to `std.Io.net.IpAddress.parseIp6` → `Ip6Address.Unresolved.parse`, which only recognises a dotted quad after the literal `::ffff:` prefix (`net.zig:499-509`); a `.` anywhere else is `invalid_byte`, so `::1.2.3.4` and `64:ff9b::1.2.3.4` are **not** valid hosts. rustty uses `std::net::Ipv6Addr::from_str`, which accepts an embedded IPv4 in the last 32 bits of any address, so `is_valid_cache_key("::1.2.3.4")` / `"user@64:ff9b::1.2.3.4"` return true in rustty but `error.InvalidCacheKey` in ghostty. Scope ids (`%`), >4 hex digits, double `::` and empty input agree.

**Evidence:** `net.zig:499-509` + the digit state's `else => return .{ .invalid_byte = text_i }` vs `disk_cache.rs:679 s.parse::<std::net::Ipv6Addr>()`.

**Suggested fix:** Pre-check in `is_valid_host`: if the text contains `.`, accept it as IPv6 only when it starts (case-insensitively) with `::ffff:` and the remainder parses as IPv4; otherwise return false.

#### S71-F4 — `add()` opens an existing cache file via create_new + open + chmod instead of a plain `O_CREAT` open

- **Status:** ✅ CONFIRMED (both lenses, medium confidence)
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/ssh-cache/DiskCache.zig:72-92`
- **rustty:** `crates/rustty-cli/src/ssh_cache/disk_cache.rs:200-208`

Oracle `add()` calls `createFileAbsolute` with `.read = true, .truncate = false` and the default `.exclusive = false` (`Io/Dir.zig:586-594`), so an existing file is simply opened read-write and the `error.PathAlreadyExists` arm (with `fixupPermissions`) is unreachable; the existing file's mode is never chmod'ed in `add` (the final 0600 comes from the atomic replacement). rustty deliberately uses `create_new` (O_EXCL) and on `AlreadyExists` re-opens and calls `fixup_permissions`. Observable differences: (a) an extra `chmod(0600)` side effect on a pre-existing non-0600 file before the rewrite; (b) if that chmod fails (e.g. a group-writable file owned by another uid: open succeeds, fchmod → EPERM) rustty's `add` returns `Error::Io` while ghostty's succeeds; (c) a create_new→open TOCTOU window that can surface `NotFound` where the oracle would create the file.

**Evidence:** `DiskCache.zig:72-76` vs `disk_cache.rs:200-208` `create_new_secure(...) ... Err(e) if e.kind() == AlreadyExists => { open_read_write; fixup_permissions(&existing_file)?; }`.

**Suggested fix:** Open with `OpenOptions::new().read(true).write(true).create(true).mode(0o600)` (no create_new, no fixup) to match the oracle's effective behaviour.

#### S71-F5 — FailingAllocator tests collapsed to a single success assertion

- **Status:** ⚠️ DISPUTED (semantic lens refuted; presence lens confirmed) — severity low, non-actionable
- **Kind:** weakened-assertion
- **Oracle:** `src/cli/ssh-cache/DiskCache.zig:640-717`
- **rustty:** `crates/rustty-cli/src/ssh_cache/disk_cache.rs:1174-1261`

`test "disk cache reads survive allocation failure"` and `test "disk cache add survives allocation failure"` loop over `fail_index`, asserting `expectEqual(error.OutOfMemory, err)` on every induced failure and (via `testing.allocator`) no leak/double-free, until a run with no induced failure. rustty runs one iteration and asserts only `list_result.is_ok()` / `cache.add(...).is_ok()`. Rust's global allocator is infallible so the OOM arm cannot be reproduced; recorded per the audit rule that fewer assertions is a weakened-assertion finding, with the inherent-limitation caveat.

**Evidence:** `DiskCache.zig:667-682` and `706-716` vs `disk_cache.rs:1222` `assert!(list_result.is_ok())` and `:1260` `assert!(cache.add(...).is_ok())`, with PORT NOTEs at `:1215-1219` / `:1255-1259`.

**Suggested fix:** None required functionally; if desired, run these two tests under Miri (leak check) to preserve the no-leak intent, and keep the PORT NOTE explaining the collapse.

**Refuting lens — semantic (medium confidence), verbatim:**

> Read oracle DiskCache.zig:646-716 and rustty disk_cache.rs:1174-1261. The only assertion the port drops is `expectEqual(error.OutOfMemory, err)` on the induced-failure arm, plus testing.allocator's leak check. Rust's global allocator is infallible (abort on OOM), so that arm is unreachable in the port — the difference is confined to code that cannot exist. The oracle's terminal assertion (a run completing with no induced failure) is preserved verbatim as `assert!(list_result.is_ok())` / `assert!(cache.add(...).is_ok())`, the fixture (duplicate-key populated cache, deleted-file-per-iteration) is reproduced, and the collapse is documented in an in-place PORT NOTE with Miri as the leak/UB backstop. Neither test asserted entry contents on either side, so nothing else is looser.

#### S71-F7 — Log sink/format differs: `std.log.warn` replaced by `eprintln!` with an ad-hoc prefix

- **Status:** ⚠️ DISPUTED (presence lens refuted; semantic lens confirmed at low) — severity low
- **Kind:** behavioral-difference
- **Oracle:** `src/cli/ssh-cache/Entry.zig:22-25`
- **rustty:** `crates/rustty-cli/src/ssh_cache/entry.rs:57-70, 163-168`

`Entry.parse`'s invalid-timestamp warning goes through `std.log.warn` (scoped default, filtered by log level, formatted `{s}` raw bytes) but rustty prints `[warn] Invalid timestamp in cache entry: <lossy utf8> err=<ParseIntError Display>` unconditionally to stderr. Control flow and return value (`None`) are identical; only the stderr text/routing differs (no scoped `std.log` level gating).

> **Note on the original description:** the finding also claimed the same defect for every `log.err(...)` in `CApi.zig` → `c_api.rs`. **Both lenses independently found those citations unsupported for this shard** — neither `src/cli/ssh-cache/CApi.zig` nor `crates/rustty-cli/src/ssh_cache/c_api.rs` exists — so that half carries no weight and is not part of the surviving finding.

**Evidence:** `Entry.zig:22` `std.log.warn("Invalid timestamp in cache entry: {s} err={}", ...)` vs `entry.rs:57-70` + `:165-168` `eprintln!("[warn] {msg}")`.

**Suggested fix:** Route through the workspace's scoped log facility (if one exists for cli/config) with the same message text and error-name formatting.

**Refuting lens — presence (high confidence), verbatim:**

> The warn path IS present and faithful: entry.rs:55-70 mirrors Entry.zig:21-28 (parse_int_i64 failure -> warn with the same message text 'Invalid timestamp in cache entry: <str> err=<err>' -> return None), routed through the workspace's documented log_warn helper at entry.rs:163-168 which writes to stderr, matching std.log.warn's default sink (comment there explicitly cites the convention shared with ssh.rs:996). The only delta is the '[warn] ' prefix and Rust's ParseIntError Display vs Zig's error name — pure log text, and the ground rules' presence/relocation lens is satisfied. Additionally, the finding's second half is not substantiable in this area: neither /Users/gael/projects/ghostty/src/cli/ssh-cache/CApi.zig nor crates/rustty-cli/src/ssh_cache/c_api.rs exists (ls of both dirs shows only Entry.zig/DiskCache.zig and entry.rs/disk_cache.rs), so the cited log.err/{err:?} evidence is fabricated for shard 71.

### Unverified (over the verification cap)

None — every candidate finding in this area was verified by both lenses.

## Performance-only differences (not findings)

- `boo.zig` `frame_list.initCapacity(gpa, 235)` → rustty collects `split(0x01)` directly; identical frame list.
- `boo.rs` adds a `MAX_DECOMPRESSED` (8 MiB) inflate cap; the embedded asset is ~1.39 MB so it is never hit.
- `crash_report.rs` uses stable `sort_by` where the reference uses unstable `std.mem.sort`; only equal-mtime ordering could differ.
- `LineIterator` in rustty holds the whole source slice instead of a buffered `std.Io.Reader`; the three buffered/primed/refill-at-boundary tests reduce to the same-output checks (the over-long-line difference is reported as S67-F2).
- `parse_auto_struct` folds the reference's `default_: ?T` inheritance parameter into `AutoStruct::apply_default` on a pre-populated result; expressively equivalent.
- `Error::ActionHelpRequested` is an added variant carrying the help sentinel that the reference's inferred error set picks up from `Options.help`; `parse`/`parse_self` propagate it identically.
- `list_keybinds`: the oracle's intrusive `std.SinglyLinkedList` of arena-allocated `TriggerNode`s (with `@fieldParentPtr`) is ported as an owned `Vec<Trigger>` (prepend = `insert(0)`, `next != null` = `idx+1 < len`); the sort comparator returns `Ordering` for `sort_by` (stable, like `std.mem.sort`). Same observable order.
- `list_colors`/`list_keybinds` prettyPrint: the reference `ArenaAllocator` + `std.fmt.allocPrint` for transient `#rrggbb`/key/action strings becomes a small `pretty_arena::Arena::dupe` bump arena; the vaxis env-map argument is dropped because rustty-vaxis reads the same env vars internally.
- `list_fonts`: `StringHashMap(ArrayListUnmanaged)` + arena dupes → `HashMap<Vec<u8>, Vec<Vec<u8>>>` with owned Vecs; first-seen order and the conditional sorts preserved.
- All actions: fixed-size Zig `File.Writer` stack buffers (1024/2048/4096) → `BufWriter` on locked stdout/stderr; flush points preserved (plus the `list_colors` plain-path flush, recorded as S68-F3).
- `list_themes.rs update_filtered`: the reference's Wyhash digest of the filtered index list is replaced by an equal-`Vec<usize>` comparison; identical observable short-circuit (modulo hash collisions).
- `list_themes.rs update_filtered` precomputes `color_pass` in one pass with a single accumulating Config (same load order and inherited-background semantics); identical results.
- `list_themes.rs run_preview` uses a single session Arena for per-frame temporaries instead of the reference's per-iteration `ArenaAllocator` (memory-only; screen `Cell` borrows require `'a`).
- `list_themes.rs` sorts with `sort_unstable_by(order_ignore_case)` rather than a `lessThan` predicate; both unstable, same key, same ordering.
- `new_tab.rs`/`new_window.rs`: the `parseManuallyHook` body is split into hook + `record_replay_arg` + `gather_arguments`; since the hook consumes every remaining arg and halts, the observable sequence and error surfacing are identical.
- rustty-vaxis `Mouse.row` is `u16` (libvaxis 0.5.1), so the reference's `@max(0, window + mouse.row)` clamp for negative rows (libvaxis 0.6) is a provable no-op in the port.
- `ssh.rs`/`ssh_cache.rs` drop the reference `ArenaAllocator` and own every `Vec<u8>` directly (RAII drop replaces `defer arena.deinit`).
- `ssh_cache.rs list_entries` collects HashMap values into a Vec and sorts with `sort_by` on hostname bytes; `Entry` values are cloned into `matches` rather than shallow-borrowed.
- `ssh.rs log_debug` is a no-op and `log_warn` writes `[warn] [ssh] ...` to stderr — the tree-wide rustty logging convention (also in rustty-os `edit.rs`/`systemd.rs`/`open.rs`), not a per-file divergence; reference debug logs are filtered out at release log level anyway.
- `show_face.rs` realises the reference `defer font_grid_set.deref(key)` by computing the result in a closure then deref-ing; same ordering of side effects.
- `ssh_cache.rs format_timestamp` returns an owned `Vec<u8>` instead of formatting into a caller-provided `[20]u8`; same bytes.
- `disk_cache.rs write_cache_file` calls `tmp_file.sync_all()` (fsync) before rename; the oracle's `File.Atomic.replace` does not fsync. Durability-only.
- `disk_cache.rs read_entries` reads through `file.try_clone()` + `take(MAX+1)` then checks `len > MAX` (equivalent to the oracle's `allocRemaining(.limited)` StreamTooLong path; both fail, both classified `isFailure = true`).
- `disk_cache.rs` stores the hostname twice (HashMap key + `Entry.hostname`) instead of the oracle's single shared allocation; pointer-identity asserts become `debug_assert` content-equality.
- `clipboard_codepoint_map.rs` uses `Vec<Entry>` (AoS) instead of `std.MultiArrayList` (SoA); linear-search semantics identical.
- Atomic temp-file naming: rustty uses `rustty_os::file::randomBasename` + `.tmp` in the cache dir vs Zig's random base64 name; the temp is removed on failure in both, so the directory-listing test still sees only `cache`.

## Refuted candidates

Both verification lenses refuted each of these; they are recorded so a reader can see what was checked and cleared.

- **S67-F7 — "parseIntoField: sentinel string" replaces the sentinel-byte assertion with an unrelated "no NUL in value" assertion.** The Zig `data.a[data.a.len] == 0` check asserts a `[:0]` _representation_ detail with no Rust counterpart; the substantive value assertion (`b"42"`) is identical, the deviation is documented in place, and where a NUL terminator IS observable (the C-ABI message pointer) rustty stores and tests it (`c_api.rs:238-251`, `diagnostics.rs:406-431`).
- **S67-F10 — `precompute_enabled` uses `cfg!(test)`, which is false when rustty-args is a dependency of another crate's tests.** The oracle's `.exe/.wasm_module => builtin.is_test` arm exists only to keep the comptime-eliminated `Precompute` type analysed; in Rust it is always compiled and is exercised by rustty-args' own tests. The shipped `.lib => true` arm is reproduced and covered end-to-end by the rustty-apprt-c C-ABI KAT; no downstream test reads precompute under the Exe artifact.
- **S68-F6 — `Action::file()` returns `cli/<name>.rs` instead of `cli/<name>.zig`.** Sanctioned naming/extension transform: the port's action impls genuinely are `.rs` files, the only consumers are build-time helpgen tooling and the `lib.rs:124` assertion (updated in lockstep), and no runtime behaviour depends on the extension.
- **S69-F6 — zf pinned at 0.10.3 (`to_lower`) while the oracle pin uses zf 0.11.0 (`case_sensitive=false`).** Both lenses extracted and diffed the two dependency tarballs: `rank` differs only by renames and a semantics-preserving `indexOf` refactor; `case_sensitive == !to_lower` so the same boolean is passed; the only behavioural addition (`deduplicateMatches`, plus a highlight sort) is on the `highlight` path, which rustty-cli never calls. The needle is pre-lowercased on both sides, so the IndexIterator delta is unobservable.
- **S71-F6 — No `ghostty_config_*` aliases: the config C ABI is exported only as `rustty_config_*`.** A locked project decision recorded at `crates/rustty-config/Cargo.toml:54-59` (the forwarder layer was deleted by rebrand Task 1); all 13 exports are 1:1 in signature and behaviour, every consumer (`include/rustty-embed.h:1141-1149`, `include/rustty-apprt.h:20`, `macos/`) uses the rebranded name, and there is no dangling use-site. Also outside this shard's area (`src/config/CApi.zig`).
