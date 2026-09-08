# Parity audit — src/config

Audited 8 shards covering the whole of `src/config` (all 6 parts of `Config.zig` plus `ErrorList.zig`, `RepeatableStringMap.zig`, `Wasm.zig`, `c_get.zig`, `command.zig`, `conditional.zig`, `edit.zig`, `file_load.zig`, `formatter.zig`, `formatter_file.zig`, `io.zig`, `key.zig`, `limit.zig`, `path.zig`, `string.zig`, `theme.zig`, `url.zig`) and, as spillover in shard 79, `src/crash/{dir,main,sentry,sentry_envelope}.zig`. **15,822 oracle lines read, equal to the full line count in scope** (no unit was left partially read).

33 findings: **31 CONFIRMED, 2 DISPUTED, 0 REFUTED, 0 UNVERIFIED.** By corrected severity: **0 high, 7 medium, 26 low.** The single most important gap is `S74-F1`: `loadDefaultFiles` swallows XDG/app-support path-resolution errors with `.unwrap_or_default()` and then feeds the resulting empty path into `load_file`'s `assert!(is_absolute)`, turning a recoverable oracle error into a release-mode process abort. Two sibling error-swallow findings (`S74-F3` cwd realpath, `S78-F2`/`S75-F1` Zig-vs-Rust number-literal grammar) and one ratified upstream deviation (`S79-F1` inverted macOS `XDG_CACHE_HOME` check) make up the rest of the medium tier; everything else is diagnostic/log text, dropped fixed-buffer `OutOfMemory` bounds, or stale port notes.

## Coverage

| Oracle unit (path:lines) | Rustty counterpart(s) | Status | Note |
| --- | --- | --- | --- |
| `src/config/Config.zig:1-61` | `crates/rustty-config/src/config_struct.rs:1-103` | ported | Imports become `use`; `terminal.CursorStyle` → `rustty_vt::cursor::Style`; `WindowPaddingBalance` re-exported from `rustty-render-types`; the `unistd.h` cImport has no in-range use-site and is documented as replaced. |
| `src/config/Config.zig:63-114` (`compatibility` map) | `config_struct.rs:9536-9635` | ported | All 10 keys with identical targets; handler bodies at 6304-6480. |
| `src/config/Config.zig:116-1594` (99 fields) | `config_struct.rs:1024-2537`, `:4966-5100`, `value_types.rs:55-59,106,1287-1300,1337-1339,1416-1450,3078-3085,6182-6195`, `rustty-render-types/src/lib.rs:175-186` | ported | Field order and every default checked value-for-value; integer widths match; docs diff clean modulo the two locked identity rebrands. |
| `src/config/Config.zig:1595-1879` (`keybind` doc head) | `config_struct.rs:2538-2886` | ported | Doc text identical after token rename. |
| `src/config/Config.zig:1880-3758` (98 fields) | `config_struct.rs:2540-4765`, `:4966-5227`, `value_types.rs:64,103-110,241,619-622,1322-1378,3307-3315,6311-6315`, `rustty-input/src/key_mods.rs:121` | ported | All 98 fields in order with Rust-idiom types; every default compared one-by-one. `gtk-quick-terminal-namespace` differs only by the mandated identity rename. Three doc casing drifts reported (S73-F1). |
| `src/config/Config.zig:3759-4036` (last fields + `_`-prefixed) | `config_struct.rs:4740-4964`, `:5226-5245` | ported | All 14 fields and defaults present; `_arena` modeled as a bool. |
| `src/config/Config.zig:4038-4173` (deinit/load/loadFile/…/writeConfigTemplate) | `config_struct.rs:5807-6039`, `:5466-5502`, `:1013-1018`, `:11669-11733` | ported | Same branches/logs; BOM test ported assertion-for-assertion; template byte-identical modulo identity. |
| `src/config/Config.zig:4173-4470` (loadDefaultFiles/loadCliArgs/loadRecursiveFiles/arenaAlloc) | `config_struct.rs:6041-6170`, `:6551-6733`, `:6726-6906` | partial | Ordering matches; see S74-F1..F4. `arenaAlloc` not applicable. |
| `src/config/Config.zig:4470-4562` (changeConditionalState/expandPaths/expandHome) | `config_struct.rs:6939-7049`, `:5579-5632` | ported | Both conditional keys iterated; the exact oracle path-field set enumerated. |
| `src/config/Config.zig:4579-4863` (loadTheme/finalize) | `config_struct.rs:7051-7557` | partial | All clamps/branches incl. `is_test` gates; log-text omissions (S74-F5), Windows `home()` error path (S74-F6), wasm gate (S74-F7). |
| `src/config/Config.zig:4865-5108` (hook/compat/clone/changed/ChangeIterator) | `config_struct.rs:6195-6300`, `:6304-6549`, `:7559-7670`, `:7947-8200`, `:5756-5798`, `:5514-5574`, `rustty-args/src/args.rs:404-421,633-657` | ported | `-e` drain split hook/driver preserving replay-step order and the missing-command diagnostic; `clone_value_into` covers every Key. |
| `src/config/Config.zig:5110-5435` (Replay.Step/Iterator) | `config_struct.rs:5251-5321`, `:5643-5745` | ported | Self-driven iterator taking `&mut Config` per `next` (aliasing-safe); identical step semantics. |
| `src/config/Config.zig:5437-5637` (small enums, WorkingDirectory, Color) | `value_types.rs:373-447,1922-2050,2058-2125,8049-8095`, `rustty-render-types` | ported | `repr(C)` enums with identical variant order; both WorkingDirectory tests ported. |
| `src/config/Config.zig:5638-5697` (Color equal/format + 4 tests) | `value_types.rs:2127-2166`, `:7426-7560` | ported | Same lowercase `#rrggbb` output; all assertions present. |
| `src/config/Config.zig:5699-5763` (TerminalColor) | `value_types.rs:2174-2242`, `:7563-7613` | ported | Identical branches; `InvalidValue` for `a` asserted. |
| `src/config/Config.zig:5765-5817` (BoldColor) | `value_types.rs:2244-2305`, `:7616-7656` | ported | Identical. |
| `src/config/Config.zig:5819-5972` (ColorList) | `value_types.rs:2671-2823`, `:7659-7724`, `include/rustty-embed.h:555` | ported | Reset-on-parse, 64-colour cap, empty→`ValueRequired`; tests carry same assertions. |
| `src/config/Config.zig:5974-6132` (Palette) | `value_types.rs:2826-2947`, `:7727-7829`, `include/rustty-embed.h:566` | ported | Overflow/InvalidFormat mapping preserved; mask set only on success; all 5 tests. |
| `src/config/Config.zig:6134-6270` (RepeatableString) | `value_types.rs:2955-3065`, `:7832-7908` | ported | Empty clears; `overwrite_next` semantics identical. |
| `src/config/Config.zig:6272-6396` (SelectionWordChars) | `value_types.rs:3067-3187`, `:7911-7975` | ported | Null codepoint first; 4096-byte formatEntry stop reproduced. |
| `src/config/Config.zig:6398-6543` (RepeatableFontVariation) | `value_types.rs:3189-3306`, `:7978-8047` | partial | Structure/tests ported; float parser narrower than `std.fmt.parseFloat` (S75-F1) and NaN spelling (S75-F3). |
| `src/config/Config.zig:6545-6559` (keyEventIsBinding) | `config_struct.rs:6906-6918` | ported | Release→false; press/repeat→`get_event(..).is_some()`. |
| `src/config/Config.zig:6561-6600` (Keybinds fields) | `value_types.rs:3308-3358` | ported | Insertion-ordered `tables`; `chain_target` Root/Table. |
| `src/config/Config.zig:6577-7340` (Keybinds.init) | `value_types.rs:3383-4776` | ported | Every put/putFlags compared in order incl. the goto_tab digit loop. |
| `src/config/Config.zig:7342-7443` (Keybinds.parseCLI) | `value_types.rs:4778-4884` | partial | Control flow/errors identical; three `std.log` lines dropped (S75-F2). |
| `src/config/Config.zig:7445-7462` (Keybinds.clone) | `value_types.rs:4886-4914` | ported | Deep-clones set and every table. |
| `src/config/Config.zig:7464-7545` (Keybinds.equal/equalSet) | `value_types.rs:4916-5064` | ported | Count check, recursive leader compare, chained flags/len/zip compare. |
| `src/config/Config.zig:7547-7613` (formatEntryDocs/formatEntry) | `value_types.rs:5066-5232`, `rustty-cli/src/list_keybinds.rs:186-192` | partial | Root/table formatting identical incl. the 1024-byte bound; `docs=true` branch differs (S76-F1). |
| `src/config/Config.zig:7615-8163` (36 Keybinds tests) | `value_types.rs:9653-10327` | ported | All 36 present with identical assertions (+1 extra bound test). |
| `src/config/Config.zig:8165-8253` (RepeatableCodepointMap) | `value_types.rs:5232-5415` | partial | parse/clone identical; `equal` by content (S76-F5); formatEntry lacks the 1024 bound (S76-F3). |
| `src/config/Config.zig:8255-8303` (UnicodeRangeParser) | `value_types.rs:5620-5866` | ported | Hoisted to module scope; parse/advance/eof identical incl. `parseInt(u21,16)` rules. |
| `src/config/Config.zig:8305-8385` (4 tests) | `value_types.rs:8354-8470` | ported | Same assertions (+ extra clone_equal test). |
| `src/config/Config.zig:8387-8581` (RepeatableClipboardCodepointMap + 5 tests) | `value_types.rs:5417-5618`, `:8166-8350` | ported | 1024-byte bound reproduced; all 5 tests. |
| `src/config/Config.zig:8583-8698` (FontStyle + 4 tests) | `value_types.rs:2307-2400`, `:8105-8160` | ported | Identical. |
| `src/config/Config.zig:8700-8710` (FontSyntheticStyle, FontShapingBreak) | `value_types.rs:1281-1300,1459-1503`, `rustty-render-types/src/lib.rs:176-186` | ported | Field sets and all-true defaults match. |
| `src/config/Config.zig:8712-8762` (RepeatableLink) | `value_types.rs:5868-5943` | partial | clone/equal/formatEntry identical; `parseCLI` error tag differs (S76-F2). |
| `src/config/Config.zig:8764-8829` (CopyOnSelect/RightClick/MiddleClick/ShellIntegration) | `value_types.rs:456-515`, `:988-1023` | ported | Variant sets, order and names identical. |
| `src/config/Config.zig:8831-8843` (ShellIntegrationFeatures, SplitPreserveZoom) | `value_types.rs:1313-1340`, `:1506-1554` | ported | Fields and defaults match. |
| `src/config/Config.zig:8845-9126` (RepeatableCommand + 7 tests) | `value_types.rs:5945-6175`, `:9413-9640`, `rustty-input/src/command.rs:196-238` | partial | Behaviour identical and all 7 tests present; formatEntry lacks the 4096 bound (S76-F4); two `log.info` lines commented out. |
| `src/config/Config.zig:9128-9202` (OSC/Window/Mac enums) | `value_types.rs:517-611,1025-1113`, `rustty-render-types/src/lib.rs:126-137`, `config_struct.rs:865,9228-9232,9345-9349` | ported | All variants/names match; `WindowTheme.ghostty` → `Rustty`/`"rustty"` per the transform. |
| `src/config/Config.zig:9204-9245` (Gtk* enums, getGObjectType) | `value_types.rs:613-651,1114-1134`, `rustty-apprt/src/gtk/class/window.rs:70-73,135-141` | replaced | Enums ported incl. `DEFAULT=Detect` and `repr(C)`; the GObject enum registration is replaced by carrying the `repr(C)` discriminant as an i32 across the titlebar-style property. |
| `src/config/Config.zig:9247-9269` (AppNotifications/BellFeatures/MouseShiftCapture) | `value_types.rs:1343-1381,1557-1600,653-661` | ported | Defaults and variant order match. |
| `src/config/Config.zig:9271-9364` (MouseScrollMultiplier + 2 tests) | `value_types.rs:6177-6310`, `:8517-8610` | partial | Defaults 1/3, fallback, clone/equal/format and both tests match; float grammar narrower (S76-F6). |
| `src/config/Config.zig:9366-9448` (ClipboardAccess … QuickTerminalLayer) | `value_types.rs:663-700,1135-1163,694-747` | ported | Variants and string names identical. |
| `src/config/Config.zig:9449-9740` (QuickTerminalSize + 2 tests) | `value_types.rs:6311-6765`, `:8616-8895` | ported | All branches/defaults (400/800 fallbacks, landscape/portrait rule); both tests ported. |
| `src/config/Config.zig:9742-9803` (QuickTerminal* / Grapheme / AlphaBlending / BackgroundImage*) | `value_types.rs:750-812`, `rustty-render-types/src/lib.rs:65-88,141-172` | ported | R4-hoisted to render-types and re-exported; `is_linear` arms identical. |
| `src/config/Config.zig:9804-9838` (FreetypeLoadFlags, LinuxCgroup, AsyncBackend, AutoUpdate) | `value_types.rs:814-837,1383-1409,1601-1630` | ported | Defaults and hyphenated packed-struct field names match. |
| `src/config/Config.zig:9840-9951` (BackgroundBlur + test) | `rustty-render-types/src/lib.rs:199-225`, `value_types.rs:2413-2545`, `:8907-8985` | ported | parse order preserved; cval 0/20/radius/-1/-2; identical assertions. |
| `src/config/Config.zig:9953-10017` (WindowDecoration + test) | `value_types.rs:839-914`, `:8992-9040` | ported | `getGObjectType` has no Rust registration but no oracle GTK use-site references it. |
| `src/config/Config.zig:10019-10164` (Theme + test) | `value_types.rs:6766-6931`, `:9048-9135` | partial | Parse/finalize/clone identical incl. the Windows drive-letter rule; formatEntry drops the 4096 bound (S77-F1). |
| `src/config/Config.zig:10166-10316` (Duration) | `value_types.rs:6933-7215` | ported | Unit table order/factors identical (incl. µs and us), saturating arithmetic, c_uint saturation. |
| `src/config/Config.zig:10318-10410` (WindowPadding + test) | `value_types.rs:7243-7325`, `:9283-9330` | ported | Trim set, comma split, `InvalidValue`; identical assertions. |
| `src/config/Config.zig:10412-10437` (Scrollbar/ScrollToBottom/NotifyOnCommandFinish*) | `value_types.rs:916-929,1411-1447,1632-1668` | ported | Defaults match. |
| `src/config/Config.zig:10439-10535` (duration/format/entryFormatter tests) | `value_types.rs:9138-9280` | ported | All assertions incl. per-unit loops and overflow saturation. |
| `src/config/Config.zig:10537-10589` (TestIterator + 3 hook/-e tests) | `config_struct.rs:11821-11985` | ported | Tests inline the driver drain and assert the same end-state. |
| `src/config/Config.zig:10591-10635, 10777-10790` (clone/changed tests) | `config_struct.rs:11583-11660` | partial | Assertions identical but the fixture is `Config::default()` rather than `Config.default(alloc)` (S77-F2). |
| `src/config/Config.zig:10637-11115` (conditional/theme tests) | `config_struct.rs:11987-12480` | ported | TempDir + `testdata/theme_*` fixtures and all colour expectations present. |
| `src/config/Config.zig:11117-11271` (limits + compatibility tests) | `config_struct.rs:12481-12815` | ported | Defaults, reset-on-empty, compat renames all asserted (+ rustty-only round-trip tests). |
| `src/config/ErrorList.zig:1-23` | `crates/rustty-config/src/error_list.rs:1-52` | ported | `Error{message}`, list, `empty()`, `add()`. |
| `src/config/RepeatableStringMap.zig:1-198` | `crates/rustty-config/src/repeatable_string_map.rs:1-418` | ported | Insertion order, `=` required, trim, empty-value removal, 256-byte bound; all 4 tests (+1 boundary). |
| `src/config/Wasm.zig:1-54` | `crates/rustty-config/src/wasm.rs:1-114` | ported | Four unprefixed C exports behind the `wasm` feature; error arms log at `.config` scope. |
| `src/config/c_get.zig:1-240` | `crates/rustty-config/src/c_get.rs:1-738` | ported | Comptime type-switch rendered as an exhaustive 209-arm match (== 209 non-underscore oracle fields, script-verified); all 6 tests (+1). |
| `src/config/command.zig` | `crates/rustty-config/src/command.rs` | partial | Union, parseCLI, `ArgIteratorGeneral` port, clone/deinit/formatEntry; all 9 named tests + `test{}`. Direct `formatEntry` drops the 4096 bound (S78-F1). |
| `src/config/conditional.zig` | `crates/rustty-config/src/conditional.rs` | ported | State/match/Key/Conditional/Op/clone; the one test ported with all 3 assertions. |
| `src/config/edit.zig` | `crates/rustty-config/src/edit.rs` | ported | `openPath`/`configPath`/`configPathCandidates` branch-for-branch; local `dirname` is a simplified `std.fs.path.dirname` (S78-F8, disputed). |
| `src/config/file_load.zig` | `crates/rustty-config/src/file_load.rs` | ported | All six path helpers + `open` with identical fallback ordering; subdir literals retargeted through `rustty_identity`. Two `log.warn` calls dropped (S78-F6). |
| `src/config/formatter.zig` | `crates/rustty-cfg-fmt/src/formatter.rs` | ported | Comptime dispatch rendered as the `FormatEntry` trait plus per-arm helpers; all 8 tests (+1 NaN test). |
| `src/config/formatter_file.zig` | `crates/rustty-config/src/formatter_file.rs` | ported | Default-config diff via `changed`, ordered non-`_` iteration, `# ` doc lines from generated `config_help`; both tests. |
| `src/config/io.zig` | `crates/rustty-config/src/io.rs` | ported | `ReadableIO`/`RepeatableReadableIO` parse/clone/equal/formatEntry with the 4096 bound; all 3 tests + `test{}`. |
| `src/config/key.zig` | `crates/rustty-config/src/config_struct.rs` | ported | `Key` as an explicit enum; `Value(comptime key)` → `value_type(key)` returning a `TypeId`; the `Value` test ported with both assertions. |
| `src/config/limit.zig` | `crates/rustty-config/src/value_types.rs:43-185`, tests `:7346-7405` | ported | `Limit<T, const DEFAULT>` default/parse/format/optional/clone; all 3 tests. `parseInt` base-0 `_` semantics differ (S78-F2). |
| `src/config/path.zig` | `crates/rustty-config/src/path.rs` | ported | `Path`/`RepeatablePath` full surface incl. the `max_path_bytes+1` bound and std path helper ports; all 8 tests. Divergences S78-F3/F4/F7. |
| `src/config/string.zig` | `crates/rustty-config/src/string.rs` | ported | Faithful inlined `parseEscapeSequence`/`utf8Encode` and codepoint iterator; all 10 tests (+1 regression). |
| `src/config/theme.zig` | `crates/rustty-config/src/theme.rs` | ported | Location/LocationIterator/open/openAbsolute; every diagnostic string matches modulo error/kind rendering (S78-F5). Reference has no tests. |
| `src/config/url.zig` | `crates/rustty-config/src/url.rs`, `rustty-vt/src/string_map.rs` (byte-identical copy) | ported | All 13 fragments and 3-branch assembly byte-identical; the 80-pair regex test ported and exercised by the hard `check_oniguruma_lane.sh` CI gate. |
| `src/crash/dir.zig:1-66` | `crates/rustty-crash/src/dir.rs` | ported | `defaultDir`, iterator (open failure → empty), report skipping/stat semantics; 0 tests either side. |
| `src/crash/main.zig:1-21` | `crates/rustty-crash/src/lib.rs` | ported | All 8 re-exports plus a `ref_all_decls` analogue. |
| `src/crash/sentry.zig:1-373` | `crates/rustty-crash/src/sentry.rs` (+ `build_options.rs`, `sentry_pkg/*`, `rustty-core/src/surface.rs:13902`, `rustty-apprt/src/lib.rs:322`) | ported | init/initThread/deinit/beforeSend/Transport all present with identical branch structure; deviations S79-F1, S79-F2, S79-F5. |
| `src/crash/sentry_envelope.zig:1-597` | `crates/rustty-crash/src/sentry_envelope.rs` | ported | parse/serialize/Item/Attachment with the 1 MB limits and every error tag; all 8 tests. JSON edge cases S79-F3/F4. |

**Units partially read: none.** Every unit listed above was read in full; the `partial` rows in the Status column mean the port diverges in a named way (each linked to a finding), not that the audit stopped short.

## Findings

### High severity

None.

### Medium severity

#### S74-F1 — loadDefaultFiles swallows XDG/app-support path resolution errors and then panics on an empty path

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** missing-error-path
- **Oracle:** `src/config/Config.zig:4172-4175, 4191-4194`
- **Rustty:** `crates/rustty-config/src/config_struct.rs:6045-6048, 6083-6089, 5918`

**Description.** The oracle does `try file_load.legacyDefaultXdgPath(alloc)` / `try file_load.defaultXdgPath(alloc)` (and on macOS `try legacyDefaultAppSupportPath` / `try preferredAppSupportPath`), so a failure (e.g. `DirError.NoHomeDir` when neither `XDG_CONFIG_HOME` nor `HOME` resolve) propagates out of `Config.load()` as an error. The port replaces each `try` with `.unwrap_or_default()`, yielding an empty path, then calls `self.load_optional_file(&[])` → `load_file`, whose first statement is `assert!(Path::new(...).is_absolute())`, which panics (`assert!` is active in release). Observable: a recoverable load error in the oracle becomes a process abort in the port.

**Evidence.** Oracle: `const legacy_xdg_path = try file_load.legacyDefaultXdgPath(alloc);` (4172) and `const xdg_path = try file_load.defaultXdgPath(alloc);` (4174). Port: `let legacy_xdg_path = crate::file_load::legacy_default_xdg_path().unwrap_or_default();` (6045), `let xdg_path = crate::file_load::default_xdg_path().unwrap_or_default();` (6048); `load_file` at 5918: `assert!(std::path::Path::new(os_str_from_bytes(path)).is_absolute());`. `rustty_os::xdg::dir` returns `Err(DirError::NoHomeDir)` (`xdg.rs:157-170`), so the `Err` arm is reachable.

**Suggested fix.** Map the `FileLoadError` into `LoadError` and use `?` for all four path lookups, matching the oracle's `try`.

#### S74-F2 — xdg-terminal-exec argv is read via `std::env::args()` (panics on non-UTF-8 argv, lossy `&str`)

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/config/Config.zig:4265-4285`
- **Rustty:** `crates/rustty-config/src/config_struct.rs:6580-6612`

**Description.** The oracle passes `global.args().vector` (raw `[*:0]u8` argv) to `internal_os.xdg.parseTerminalExec` and copies each arg with `std.mem.sliceTo(arg_raw, 0)` as bytes. The port builds `Vec<String>` via `std::env::args()`, which panics if any argv element is not valid Unicode, and converts to `&str`. A Linux/FreeBSD user invoking `xdg-terminal-exec` with a non-UTF-8 argument (a filename, for example) gets a panic instead of that byte string as `initial-command`. This also violates the workspace byte-slice rule.

**Evidence.** Oracle 4265: `if (internal_os.xdg.parseTerminalExec(global.args().vector)) |args| {` … 4276-4277: `const arg = std.mem.sliceTo(arg_raw, 0); const copy = try arena_alloc.dupeZ(u8, arg);`. Port 6581-6583: `let argv: Vec<String> = std::env::args().collect(); let argv_refs: Vec<&str> = ...; if let Some(args) = rustty_os::xdg::parseTerminalExec(&argv_refs)`; 6597: `let copy = arg_raw.as_bytes().to_vec();`. `parseTerminalExec` signature (`xdg.rs:175`) takes `&[&str]`.

**Suggested fix.** Use `std::env::args_os()` and `OsStrExt::as_bytes`, and make `parseTerminalExec` take `&[&[u8]]`.

#### S74-F3 — loadCliArgs ignores cwd realpath failure and expands CLI paths against an empty base

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** missing-error-path
- **Oracle:** `src/config/Config.zig:4331-4336`
- **Rustty:** `crates/rustty-config/src/config_struct.rs:6712-6717`

**Description.** The oracle does `try self.expandPaths(buf[0..try std.Io.Dir.cwd().realPathFile(...)])`, so if the current directory cannot be resolved (deleted cwd, EACCES) config loading fails with an error. The port uses `.ok().and_then(...).unwrap_or_default()` and calls `expand_paths(b"")`, which records an `Expand("")` replay step and expands relative `config-file`/`custom-shader`/… entries against an empty base, leaving them relative; `load_recursive_files` then hits `assert!(is_absolute)` (6779) for any relative `config-file` given on the CLI.

**Evidence.** Oracle 4333-4336: `try self.expandPaths(buf[0..try std.Io.Dir.cwd().realPathFile(global.io(), ".", &buf)]);`. Port 6712-6717: `let cwd = std::env::current_dir().ok().and_then(|p| p.canonicalize().ok()).map(|p| path_to_bytes(&p)).unwrap_or_default(); self.expand_paths(&cwd)?;`.

**Suggested fix.** Propagate the `io::Error` as `LoadError::Io` instead of defaulting.

#### S75-F1 — RepeatableFontVariation.parseCLI rejects float syntaxes `std.fmt.parseFloat` accepts (digit separators, hex floats)

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/config/Config.zig:6437-6447`
- **Rustty:** `crates/rustty-config/src/value_types.rs:3229-3233, 6296-6302`

**Description.** The reference parses the axis value with `std.fmt.parseFloat(f64, value)`, which accepts digit-separator underscores such as `1_000` and hex float literals such as `0x1p8`. The port's `parse_float_f64` is `core::str::from_utf8(input).ok().and_then(|s| s.parse::<f64>().ok())`; Rust's `f64::from_str` rejects both forms. So `font-variation = wght=1_000` (or `wght=0x1p8`) loads in Ghostty but yields `error.InvalidValue` (config diagnostic, value dropped) in rustty.

**Evidence.** `Config.zig:6445` `.value = std.fmt.parseFloat(f64, value) catch return error.InvalidValue` vs `value_types.rs:6297-6302`. Zig std `parse_float/parse.zig:143` (`validUnderscores`) and `:220-227` (hex-float branch, `exp_char_lower = 'p'`). The same helper also backs `value_types.rs:6217/6272-6273`.

**Suggested fix.** Make `parse_float_f64` a faithful port of `std.fmt.parseFloat`: strip well-placed `_` separators before parsing and add a hex-float path, or port `parse_float` from Zig std into `rustty-zigstd`.

#### S78-F1 — Command.direct formatEntry drops the `[4096]u8` fixed-buffer OutOfMemory bound

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** missing-error-path
- **Oracle:** `src/config/command.zig:182-196`
- **Rustty:** `crates/rustty-config/src/command.rs:283-295`

**Description.** The reference formats a direct command into a 4096-byte fixed writer and every `writeAll`/`writeByte` that overflows returns `error.OutOfMemory`. The port builds an unbounded `Vec` and always succeeds, so a direct command whose `direct:` + args + separating spaces exceed 4096 bytes formats successfully in rustty but fails with `OutOfMemory` in Ghostty. The sibling ports (`io.rs:260-262`, `path.rs:424-426`) explicitly enforce their fixed-buffer bounds; `command.rs` does not.

**Evidence.** `command.zig:183-189` `var buf: [4096]u8 = undefined; var writer: std.Io.Writer = .fixed(&buf); writer.writeAll("direct:") catch return error.OutOfMemory; …` vs `command.rs:287-292` `let mut buf: Vec<u8> = Vec::new(); buf.extend_from_slice(b"direct:"); for arg in v { buf.extend_from_slice(arg); buf.push(b' '); }` with no length check.

**Suggested fix.** Return `Err(FormatError::OutOfMemory)` once `7 + sum(arg.len() + 1)` exceeds 4096 (mirror `io.rs:260`) and add a boundary test.

#### S78-F2 — Limit.parseCLI rejects `_` digit separators that `std.fmt.parseInt(T, input, 0)` accepts

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/config/limit.zig:21-31`
- **Rustty:** `crates/rustty-config/src/value_types.rs:114-131`

**Description.** The reference parses the limit with `std.fmt.parseInt(T, input, 0)`. Zig's `parseWithSign` skips `_` characters between digits (only a leading or trailing `_` is `InvalidCharacter`), so `scrollback-limit-bytes = 50_000_000` or `clipboard-write-limit-bytes = 64_000_000` are valid in Ghostty. rustty's `Limit::parse_cli` delegates to `LimitInt::parse_limit` → `rustty_args parse_field_int` → `parse_int_auto`, which strips a sign and a `0x`/`0o`/`0b` prefix and then calls `<T>::from_str_radix`, which rejects any `_` as `InvalidDigit` → `Error::InvalidValue`. Also `-0` for an unsigned `T` parses to 0 in Zig but is rejected by `from_str_radix`.

**Evidence.** `limit.zig:28-29` `.value = std.fmt.parseInt(T, input, 0) catch return error.InvalidValue` vs `value_types.rs:129` → `:80-81 parse_field_int::<$t>(Some(input))` → `crates/rustty-args/src/args.rs:1239-1274 parse_int_auto` (no `_` handling) → `args.rs:1220-1226 <$t>::from_str_radix(s, radix)`.

**Suggested fix.** In `parse_int_auto`, reproduce `parseWithSign`: reject leading/trailing `_`, strip interior `_` before `from_str_radix`, and accept `-0` for unsigned types. Add a `Limit` test for `1_000` and `0x1_0`.

#### S79-F1 — macOS cacheDir XDG_CACHE_HOME condition is deliberately inverted relative to the oracle at pin

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/crash/sentry.zig:188-208`
- **Rustty:** `crates/rustty-crash/src/sentry.rs:444-492` (respect_xdg at 470-478)

**Description.** Oracle `cacheDir`: on macOS, if `XDG_CACHE_HOME` is present **and** non-empty it returns `internal_os.macos.cacheDir(alloc, "sentry")` (NSCachesDirectory); otherwise it falls through to `internal_os.xdg.cache(..., "ghostty/sentry")`. rustty does the opposite: `respect_xdg = XDG_CACHE_HOME set && non-empty`; if `!respect_xdg` it returns `macos::cacheDir("sentry")`, else `xdg::cache("rustty/sentry")`. So with `XDG_CACHE_HOME=/x` the oracle stores the Sentry database under `~/Library/Caches/sentry` while rustty stores it under `/x/rustty/sentry`, and vice versa when unset. This is a ratified, documented deviation (`.port/phase-4/UPSTREAM_BUGS.md` P4-U1: the oracle's `15c50c1db` inverted the check contrary to its own comment), but it IS an observable functional difference from the oracle at pin `20abdb50a` and is reported per the 100 %-parity bar.

**Evidence.** `sentry.zig:192-200` `if (comptime builtin.os.tag == .macos) macos: { const xdg_cache_home = environ_map.get("XDG_CACHE_HOME") orelse break :macos; if (xdg_cache_home.len > 0) { return try internal_os.macos.cacheDir(alloc, "sentry"); } }` vs `sentry.rs:461-472` `let respect_xdg = environ_map.get(&b"XDG_CACHE_HOME"[..]).is_some_and(|v| !v.is_empty()); if !respect_xdg { return internal_os::macos::cacheDir("sentry")... }`.

**Suggested fix.** Decide explicitly: either match the oracle as written (swap the branch) or keep the ratified fix and record the deviation in the parity exception ledger so audits do not re-flag it; if kept, monitor the upstream fix so the pin can be re-aligned.

### Low severity

#### S72-F1 — Stale module-header PORT STATUS claims the per-field doc comments are NOT ported and the G3b gate is RED

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** other
- **Oracle:** `src/config/Config.zig:116-1879`
- **Rustty:** `crates/rustty-config/src/config_struct.rs:19-26` (and a second stale note at `:1022-1023`)

**Description.** The module header says the per-field Pandoc Markdown `///` doc comments "are NOT yet ported, so the whole-`Config.zig` G3b comment-coverage gate remains RED until that sub-batch lands." A line-by-line diff of the `///` doc lines for every field in this range (oracle 116-1879 vs `config_struct.rs:1024-2886`, after the Ghostty→Rustty token rename) shows the docs ARE fully ported (only the two locked identity rebrands differ). The header is a stale deferral note that misreports a gap. Cosmetic only; no behavioural difference. The presence lens additionally identified a second stale instance at `config_struct.rs:1023` ("per-field Pandoc doc comments are ported in the fill sub-batch").

**Evidence.** `config_struct.rs:19-26` (deferral text) vs the extracted doc-line diff: 1578 oracle lines vs 1646 rustty lines, the only diffs being 5 leading `language` doc lines, `GHOSTTY_RESOURCES_DIR`→`RUSTTY_RESOURCES_DIR`, `com.mitchellh.rustty`→`io.airely.rustty`, and the keybind doc continuing past the shard boundary. The file carries 3808 `///` lines.

**Suggested fix.** Update the header's PORT STATUS paragraph to state the field docs are ported and drop the "G3b RED" claim (or point at the actual gate record); fix the in-struct note at `:1023` too.

#### S73-F1 — Three doc-comment casing drifts vs the per-token Ghostty→Rustty transform (user-visible help text)

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** other
- **Oracle:** `src/config/Config.zig:3493, 3564, 3707`
- **Rustty:** `crates/rustty-config/src/config_struct.rs:4472, 4543, 4687`

**Description.** The field doc comments (which feed helpgen / `+show-config --docs`) diverge from the oracle text after the mandated rename in three places: (1) oracle `Ghostty > Secure Keyboard Entry` is rendered lowercase as `rustty > Secure Keyboard Entry`; (2) oracle default custom icon path `~/.config/ghostty/Ghostty.icns` is rendered as `~/.config/rustty/rustty.icns`, whereas the actual macOS code in `macos/Sources/RusttyKit/Rustty.Config.swift:406` uses `~/.config/rustty/Rustty.icns` (so the doc disagrees with the real default); (3) oracle `each new ghostty process` (lowercase, the binary) is rendered as `each new Rustty process`. All other ~1680 doc lines in the range match exactly after the transform. Verification confirmed these strings really are emitted: `crates/rustty-config/build.rs:1-40` generates the `config_help` table from these `///` blocks, consumed by `+show-config --docs`.

**Evidence.** `Config.zig:3493` `` /// `Ghostty > Secure Keyboard Entry` menu item. `` vs `config_struct.rs:4472`; `Config.zig:3564` `` /// Defaults to `~/.config/ghostty/Ghostty.icns` `` vs `config_struct.rs:4543`; `Config.zig:3707` `` /// If `false`, each new ghostty process will launch a separate application. `` vs `config_struct.rs:4687`. Cross-check: `macos/Sources/Features/Secure Input/SecureInputOverlay.swift:62` writes `Rustty > Secure Keyboard Entry`.

**Suggested fix.** Change `:4472` to `Rustty > Secure Keyboard Entry`, `:4543` to `~/.config/rustty/Rustty.icns`, `:4687` to `each new rustty process`; regenerate help strings.

#### S74-F4 — config-file diagnostics render Rust `io::Error` / a reduced kind set instead of Zig error and `File.Kind` tag names

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/config/Config.zig:4413-4426, 4432-4448`
- **Rustty:** `crates/rustty-config/src/config_struct.rs:6811-6818, 6836-6848, 5424-5434`

**Description.** User-visible diagnostic text differs: the oracle formats the open error as the Zig error name (`error opening config-file /p: error.AccessDenied`) whereas the port formats `std::io::Error` Display (`… Permission denied (os error 13)`). Likewise `config-file {s}: not reading because file type is {s}` uses `@tagName(stat.kind)` (directory, named_pipe, character_device, block_device, sym_link, unknown_file, …) while the port's `file_kind_name` only yields `directory`, `sym_link` or `unknown` (a FIFO reports `unknown` instead of `named_pipe`). These strings appear in the config-error window / `+show-config` output. Control flow is identical on both sides.

**Evidence.** Oracle 4415-4416 `"error opening config-file {s}: {}", .{ path, err }`; 4437-4440 `"config-file {s}: not reading because file type is {s}", .{ path, @tagName(kind) }`. Port 6812-6817 `format!("error opening config-file {}: {}", …)`; `fn file_kind_name` at 5424-5434 maps `is_dir`/`is_symlink`/else.

**Suggested fix.** Map `io::ErrorKind` to the Zig error names and use the unix `FileTypeExt` to name fifo/char/block devices.

#### S74-F5 — finalize log lines drop their formatted values

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/config/Config.zig:4680, 4844-4849`
- **Rustty:** `crates/rustty-config/src/config_struct.rs:7290, 7514-7518`

**Description.** `log.info("shell src=config value={}", .{cmd})` is ported as `log_info("shell src=config")` without the command value, and the `quit-after-last-window-closed-delay … short value ({f})` warning omits the formatted duration. Log-only text differences.

**Evidence.** Oracle 4680 `log.info("shell src=config value={}", .{cmd})`; port 7289-7290 `if let Some(_cmd) = &self.command { log_info("shell src=config".to_string()); }`. Oracle 4846-4848 `"…very short value ({f}), which might cause problems"`; port 7515-7517 fixed string.

**Suggested fix.** Include the `Command`'s Display and the `Duration` formatter output in the messages.

#### S74-F6 — Windows arm: `internal_os.home` error is swallowed into `.inherit` instead of propagating

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** missing-error-path
- **Oracle:** `src/config/Config.zig:4722-4730`
- **Rustty:** `crates/rustty-config/src/config_struct.rs:7367-7376`

**Description.** The oracle does `if (try internal_os.home(global.io(), &environ_map, &buf)) |home|` so an error from `home()` fails finalize; the port's `if let Ok(Some(home)) = rustty_os::homedir::home(...)` treats `Err` the same as `None` and sets `wd = Inherit`. Windows-only (not a supported target), hence low.

**Evidence.** Oracle 4725 vs port 7369-7376 as quoted above.

**Suggested fix.** Match on the `Result` and return `LoadError` on `Err`.

#### S74-F7 — wasm: SHELL-env/flatpak lookup in finalize is not gated off like the oracle's isWasm guard

- **Status:** ⚠️ DISPUTED (semantic lens refuted; presence lens confirmed)
- **Kind:** missing-branch
- **Oracle:** `src/config/Config.zig:4671-4674` (presence lens corrects to `:4737-4739`)
- **Rustty:** `crates/rustty-config/src/config_struct.rs:7276-7278, 7315-7332` (presence lens corrects to `:7284-7286`)

**Description.** The oracle wraps the entire `command:` block in `if (!isWasm and !is_test)`, so on wasm neither the flatpak check nor the SHELL env lookup nor passwd runs. The port only gates the passwd arm with `#[cfg(not(target_family = "wasm"))]`; the `isFlatpak()`/`SHELL` branch still runs on wasm and can set `self.command = Shell(SHELL)` there. wasm-only, hence low.

**Evidence.** Oracle 4671-4674 `if ((comptime !builtin.target.cpu.arch.isWasm()) and (comptime !builtin.is_test)) { … }`. Port 7276-7278 `#[cfg(not(test))] { … }` with only the passwd block under `#[cfg(not(any(target_os = "windows", target_family = "wasm")))]` (7358).

**Refuting lens — SEMANTIC (verbatim).** "Read both regions. Oracle ghostty/src/config/Config.zig:4732-4735 gates the whole `command:` block with `!isWasm and !is_test`; rustty crates/rustty-config/src/config_struct.rs:7276-7278 gates it only with `#[cfg(not(test))]`, with the passwd arm additionally `#[cfg(not(any(target_os=\"windows\", target_family=\"wasm\")))]` (7383-7386, documented by the WS14 T6 PORT NOTE). But the residual wasm path is a provable no-op: rustty-os/src/flatpak.rs:28-40 makes `isFlatpak()` a compile-time `false` off Linux, so control reaches config_struct.rs:7310-7313 `if !probable_cli { break 'shell_env; }`, and `probable_cli_environment()` (config_struct.rs:5514-5568) on wasm returns false — the captured environ block (rustty-global/src/environ.rs lazy capture of `std::env::vars_os`) is empty on wasm and `std::env::args_os().len()` is 0 there — so it breaks out before the `SHELL` lookup at 7326. `self.command` therefore stays `None` and `wd` is untouched (both the Windows and passwd arms are cfg'd out on wasm), exactly matching the oracle's skipped block. No input/state on the wasm target yields a different value, error, or side effect; the difference is confined to code that cannot take effect."

**Counter-note (presence lens, not refuted).** On a wasm build whose captured environ carries `TERM_PROGRAM`/args plus `SHELL`, the port would set `self.command = Shell(SHELL)` where the oracle sets nothing; no alternative gate exists anywhere in the workspace. Confidence medium.

**Suggested fix.** Add `#[cfg(not(target_family = "wasm"))]` to the outer block.

#### S75-F2 — Keybinds.parseCLI drops the three `std.log` messages

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/config/Config.zig:7355, 7365, 7397` (semantic lens corrects to `:7369, 7379, 7418`)
- **Rustty:** `crates/rustty-config/src/value_types.rs:4798-4815, 4850-4856`

**Description.** The reference logs `config has 'keybind =', using default keybinds`, `config has 'keybind = clear', all keybinds cleared` and `config has 'keybind = {s}/', table cleared` (stderr, scope `config`). The port keeps them only as commented-out lines; `value_types.rs` contains no log call at all, even though `config_struct.rs:5405-5415` has `log_info`/`log_warn` helpers reproducing the reference stderr gate.

**Evidence.** The three `log.info`/`log.debug` calls in the oracle vs `value_types.rs:4799`, `:4809`, `:4851` comments; `grep -n log_info value_types.rs` → no matches.

**Suggested fix.** Expose the `config_struct.rs` `log_info` helper (plus a `log_debug` variant) to `value_types.rs` and emit the three messages at the same points.

#### S75-F3 — RepeatableFontVariation.formatEntry prints NaN as `NaN` instead of Zig's `nan`

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/config/Config.zig:6474-6481`
- **Rustty:** `crates/rustty-config/src/value_types.rs:3283-3293, 6304-6308`

**Description.** Both parsers accept `nan`. On formatting, the reference's `{d}` float formatter emits `nan` (Zig `std/fmt/float.zig:91-101 copySpecialStr`), while the port's `fmt_float` uses `format!("{v}")`, which emits `NaN`. `font-variation = wght=nan` followed by `+show-config` therefore prints `wght=nan` in Ghostty and `wght=NaN` in rustty. Finite values and inf agree.

**Evidence.** `Config.zig:6477` `std.fmt.bufPrint(&buf, "{s}={d}", …)` vs `value_types.rs:6306-6308` `fn fmt_float(v: f64) -> String { format!("{v}") }`.

**Suggested fix.** Special-case `v.is_nan()` → `"nan"` (and confirm inf spelling) in `fmt_float`.

#### S76-F6 — MouseScrollMultiplier float parsing uses Rust `str::parse::<f64>` instead of `std.fmt.parseFloat`

- **Status:** ✅ CONFIRMED (both lenses; presence lens corrected severity medium→low)
- **Kind:** behavioral-difference
- **Oracle:** `src/config/Config.zig:9296-9309` (lenses correct to `:9284-9298`)
- **Rustty:** `crates/rustty-config/src/value_types.rs:6208-6240, 6264-6284, 6297-6304`

**Description.** Both the bare fallback and the `precision:`/`discrete:` fields go through `std.fmt.parseFloat`, which accepts hex floats (`0x1p1`, `0x1.8p0`) and `_` digit separators (`1_000.5`). rustty's `parse_float_f64` delegates to `str::parse::<f64>()`, which rejects both, so `mouse-scroll-multiplier = 0x1p1` or `precision:1_0` is accepted by the oracle (2.0 / 10.0) but produces an `InvalidValue` diagnostic in rustty. The presence lens downgraded this to low as an extremely narrow input-syntax edge case; the same helper also backs `RepeatableFontVariation` (see S75-F1).

**Evidence.** Oracle `std.fmt.parseFloat(f64, input) catch return error.InvalidValue`; zig 0.16 `std/fmt/parse_float.zig:41` (`convertHex`) and `parse_float/parse.zig:164,281` (`_` handling). rustty `value_types.rs:6298-6304`.

**Suggested fix.** Implement a `parse_float` port accepting Zig's grammar (sign, inf/nan/infinity, hex floats with `p` exponent, `_` separators) and share it with rustty-args' float field parsing.

#### S76-F1 — Keybinds.formatEntryDocs `docs=true` emits per-action help text where the oracle emits only a blank line

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/config/Config.zig:7549-7578`
- **Rustty:** `crates/rustty-config/src/value_types.rs:5111-5145, 5192-5214`

**Description.** In the oracle the docs branch does `const name = @tagName(v)` where `v` is the `Binding.Set.Value` union (tags `leader`/`leaf`/`leaf_chained`), then compares it against `help_strings.KeybindAction` decl names (action names, generated from the Action union field names per `src/helpgen.zig:8,125,130`). No tag ever matches, so `ghostty +list-keybinds --docs` prints just an extra `"\n"` before each root binding and never any `# action` / `#   help` lines. rustty's `v_tag_name` returns the leaf's ACTION tag (or the first chained action's tag) and `RealHelpStrings` has those decls, so it prints `# <action>` plus indented help lines for every root binding. `list-keybinds --docs` output therefore differs.

**Evidence.** Oracle `const name = @tagName(v);` with `v = next.value_ptr.*` of type `Binding.Set.Value` (`src/input/Binding.zig:2138-2148`). rustty `value_types.rs:5121` `let name = v_tag_name(v);` and `:5198-5214` `Value::Leaf(leaf) => leaf.action.tag_name()`; used by `rustty-cli/src/list_keybinds.rs:192`.

**Suggested fix.** Either reproduce the oracle literally (tag name of the `Value` union, i.e. never match) or record this as an intentional documented divergence; the PORT NOTE at `5066-5079` currently describes emitting real doc text without acknowledging that the oracle never does.

#### S76-F2 — RepeatableLink.parseCLI returns InvalidValue instead of NotImplemented, changing the diagnostic text

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** missing-error-path
- **Oracle:** `src/config/Config.zig:8713-8724` + `src/cli/args.zig:158-170`
- **Rustty:** `crates/rustty-config/src/value_types.rs:5876-5887`

**Description.** Oracle `RepeatableLink.parseCLI` always returns `error.NotImplemented`. In `cli/args.zig` that error is not InvalidValue/ValueRequired/InvalidField, so the diagnostic added for `link = …` is `"unknown error error.NotImplemented"`. rustty returns `Error::InvalidValue`, so `rustty-args/src/args.rs:537` produces the formatted invalid-value message instead. The user-visible config diagnostic differs. `rustty-args` has no equivalent non-base error variant (only `Error::ActionHelpRequested`).

**Evidence.** Oracle `return error.NotImplemented;`; `cli/args.zig:163-170 else => try std.fmt.allocPrintSentinel(arena_alloc, "unknown error {}", .{err}, 0)`. rustty `value_types.rs:5886 Err(Error::InvalidValue)`; `config_struct.rs:10188 b"link" => field_parsecli_self!(link)`; `args.rs:537`.

**Suggested fix.** Add a NotImplemented (or generic "unknown error") variant to the args error set, or special-case the `link` key.

#### S76-F3 — RepeatableCodepointMap.formatEntry drops the 1024-byte fixed buffer bound

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** missing-error-path
- **Oracle:** `src/config/Config.zig:8214-8253` (lenses correct to `:8163-8204`)
- **Rustty:** `crates/rustty-config/src/value_types.rs:5359-5397`

**Description.** The oracle renders each entry with `std.fmt.bufPrint(&buf /* [1024]u8 */, …) catch return error.OutOfMemory`, so a family name that pushes `U+XXXX=<family>` past 1024 bytes aborts the whole config dump with OutOfMemory. rustty builds the entry into a growable `Vec<u8>` and always succeeds. The sibling `RepeatableClipboardCodepointMap::format_entry` deliberately reproduces the same bound, so the two are inconsistent.

**Evidence.** Oracle `var buf: [1024]u8 = undefined;` plus two `bufPrint … catch return error.OutOfMemory` sites; rustty `let mut s: Vec<u8> = Vec::new(); s.extend_from_slice(...)` with no cap.

**Suggested fix.** Use the same `buf_print_parts(&mut [0u8;1024], …)` → `FormatError::OutOfMemory` path as the clipboard map.

#### S76-F4 — RepeatableCommand.formatEntry drops the 4096-byte fixed writer bound

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** missing-error-path
- **Oracle:** `src/config/Config.zig:9002-9033` (lenses correct to `:8937-8966`)
- **Rustty:** `crates/rustty-config/src/value_types.rs:6116-6157`

**Description.** Oracle writes `title:"…"[,description:"…"],action:"…"` into `std.Io.Writer.fixed(&buf /* [4096]u8 */)` with `catch return error.OutOfMemory`; an entry whose escaped title/description/action exceeds 4096 bytes fails the dump. rustty accumulates into an unbounded `Vec<u8>` and succeeds. The PORT NOTE at `6107-6115` acknowledges the missing cap.

**Evidence.** Oracle `var buf: [4096]u8 = undefined; var writer: std.Io.Writer = .fixed(&buf);` with four `catch return error.OutOfMemory` sites; rustty `let mut buf: Vec<u8> = Vec::new();`.

**Suggested fix.** Bound the accumulated entry at 4096 bytes and return `FormatError::OutOfMemory` when exceeded.

#### S76-F5 — RepeatableCodepointMap.equal compares family by content; oracle `std.meta.eql` compares the slice by pointer identity

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/config/Config.zig:8199-8210` (+ zig `std/meta.zig:657-661`)
- **Rustty:** `crates/rustty-config/src/value_types.rs:5317-5343, 8472-8515`

**Description.** Oracle `equal` uses `std.meta.eql(a, b)`; for the `family: ?[:0]const u8` slice that compares `a.ptr == b.ptr and a.len == b.len`, so two independently parsed configs with an identical `font-codepoint-map` compare UNEQUAL in the oracle (`Config.changed` reports a change on every reload when the map is non-empty), while a shallow clone (shared pointer) compares equal. rustty compares family bytes by content, so re-parsed maps compare equal. This is a documented deliberate divergence (PORT NOTE `5300-5316`, test `repeatable_codepoint_map_clone_equal`) but it is an observable difference in `Config.changed(.@"font-codepoint-map")`.

**Evidence.** Oracle `if (!std.meta.eql(a, b)) return false;`; zig `std/meta.zig:660 .slice => a.ptr == b.ptr and a.len == b.len`. rustty `a.descriptor.family != b.descriptor.family` (Vec content compare).

**Suggested fix.** Keep as documented divergence (it is the intended semantics) but record it in the deferral/divergence ledger; no code change recommended.

#### S77-F1 — Theme.formatEntry drops the fixed 4096-byte buffer OutOfMemory bound

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** missing-error-path
- **Oracle:** `src/config/Config.zig:10082-10098`
- **Rustty:** `crates/rustty-config/src/value_types.rs:6852-6879`

**Description.** The oracle formats a light/dark pair through `var buf: [4096]u8` and `std.fmt.bufPrint(&buf, "light:{s},dark:{s}", …) catch return error.OutOfMemory`, so a pair whose combined rendering exceeds 4096 bytes fails formatting with OutOfMemory. The port builds an unbounded `Vec` and always succeeds. `Theme.parseCLI` imposes no length bound on either component, so the case is reachable. `repeatable_string_map.rs:233-241` enforces the analogous 256-byte bound explicitly, so this type is inconsistent with that precedent.

**Evidence.** Oracle as quoted; port `let mut s: Vec<u8> = Vec::new(); s.extend_from_slice(b"light:"); …; format_entry_string(name, &s, writer)` with no length check.

**Suggested fix.** Return `FormatError::OutOfMemory` when `6 + light.len() + 6 + dark.len() > 4096`, mirroring the `RepeatableStringMap` guard.

#### S77-F2 — 'clone default' / 'clone preserves conditional state' / 'changed' use the bare field-default fixture instead of `Config.default(alloc)`

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** weakened-assertion
- **Oracle:** `src/config/Config.zig:10591-10635, 10777-10790` (lenses: `:10596-10629` + the `changed` test)
- **Rustty:** `crates/rustty-config/src/config_struct.rs:11583-11660`

**Description.** The reference tests build the fixture with `Config.default(alloc)`, whose port `default_config()` (`config_struct.rs:5839-5876`) additionally runs `keybind.init()`, `command_palette_entry.init()` and pushes the default URL-detection `link`. The port's three tests use `Config::default()` — documented at `4966-4969` as only "the data half of `default(alloc)`". The assertions are the same, but `clone`/`change_iterator` are never exercised over the default keybind set, palette entries or link list, so a clone regression in those fields would not be caught. The remaining integration tests in the same file correctly use `Config::default_config()`.

**Evidence.** Port `let source = Config::default(); let dest = source.clone_config();` (11592-11595, 11616, 11646) vs oracle `var source = try Config.default(alloc);`.

**Suggested fix.** Switch the three tests to `Config::default_config()`.

#### S78-F3 — Path.expand no longer fails when the base directory cannot be opened

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/config/path.zig:204-234`
- **Rustty:** `crates/rustty-config/src/path.rs:332-378`

**Description.** The reference does `var dir = try std.Io.Dir.openDirAbsolute(global.io(), base, .{})` before realpath: if the base directory does not exist / is not a directory / is inaccessible, `expand` returns that error (which the config loader logs as "error expanding paths"). The port never opens `base`; it calls `std::fs::canonicalize(base.join(path))` and on NotFound silently falls back to the pure-string `resolve`, so a missing base yields a fabricated absolute path instead of an error. Only reachable when the config-file dirname or home dir vanished between load and expand, hence low.

**Evidence.** `path.zig:204` vs `path.rs:334-347` (`canonicalize` then `Err(NotFound) => resolve(&[base, &path])`), with no base open and no propagated error.

**Suggested fix.** Open/stat `base` first and return its `io::Error` before attempting canonicalize, matching the `try openDirAbsolute` ordering.

#### S78-F4 — Path.expand diagnostics render the error with Rust Debug instead of Zig's `error.Name` form

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** other
- **Oracle:** `src/config/path.zig:176-190, 218-231`
- **Rustty:** `crates/rustty-config/src/path.rs:296-304, 361-369`

**Description.** Both diagnostic messages in `expand` interpolate the error with `{}`, which for a Zig error prints `error.<Name>`. The port uses `format!("{err:?}")`: for the expandHome path this prints `HomeDetectionFailed`/`BufferTooSmall` (no `error.` prefix), and for the realpath path it prints `std::io::Error`'s Debug form (`Os { code: 13, kind: PermissionDenied, message: "Permission denied" }`). The user-visible diagnostic text therefore differs.

**Evidence.** `path.zig:179` `"error expanding home directory for path {s}: {}"` and `:221` `"error resolving file path {s}: {}"` vs `path.rs:300` / `:365` `message.extend_from_slice(format!("{err:?}").as_bytes());`.

**Suggested fix.** Render errors in the reference's `error.<CamelName>` shape (a small `io::ErrorKind` → Zig error-name helper, `error.HomeDetectionFailed` for ExpandError).

#### S78-F5 — theme diagnostics: file-kind tag names collapsed to `unknown` and errors rendered via `io::Error` Display

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** other
- **Oracle:** `src/config/theme.zig:129-153, 177-201, 211-222, 259-277`
- **Rustty:** `crates/rustty-config/src/theme.rs:334, 430, 489, 586, 638-650`

**Description.** `not reading theme from "{s}": it is a {s}` uses `@tagName(stat.kind)`, which yields directory/sym_link/named_pipe/block_device/character_device/unix_domain_socket/whiteout/door/event_port/unknown. The port's `kind_name` only distinguishes directory and sym_link and reports every other non-regular kind as `unknown`, so `theme = /dev/null` reports "it is a unknown" instead of "it is a character_device". The `{}` error interpolations use `err.to_string()` ("Permission denied (os error 13)") instead of Zig's `error.AccessDenied`.

**Evidence.** `theme.zig:146-147` vs `theme.rs:638-650`; `theme.zig:133/181/215/272` `{}` vs `theme.rs:334/430/489/586` `err.to_string()`.

**Suggested fix.** Extend `kind_name` using `std::os::unix::fs::FileTypeExt` (fifo → named_pipe, socket → unix_domain_socket, block/char devices) and render io errors in the `error.<Name>` form.

#### S78-F6 — file_load.open drops both `log.warn` calls

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** other
- **Oracle:** `src/config/file_load.zig:150-167`
- **Rustty:** `crates/rustty-config/src/file_load.rs:319-340`

**Description.** The reference logs `unexpected file open error path={s} err={}` and `error getting file stat path={s} err={}` at warn level before mapping to `OpenFileError.FileOpenFailed`. The port keeps them only as comments, so the warnings are never emitted, even though the crate has a working `log_warn` helper (`config_struct.rs:5411`) used elsewhere for the same scoped-logger semantics.

**Evidence.** `file_load.zig:152-155` and `:162-165` vs `file_load.rs:324` and `:337` (comment lines, only `return Err(OpenFileError::FileOpenFailed)` executes).

**Suggested fix.** Emit the two warnings through the crate's logging path.

#### S78-F7 — Path.expand drops the two `log.debug` traces

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** other
- **Oracle:** `src/config/path.zig:192-195, 236-239`
- **Rustty:** `crates/rustty-config/src/path.rs:316, 380`

**Description.** `expanding file path from home directory: path={s}` and `expanding file path relative={s} abs={s}` are debug-level logs in the reference; the port carries them only as comments, so debug logging of path expansion is absent. Verification found this is a port-wide convention (103 `// log.debug` comment sites; no crate declares a `log`/`tracing` dependency), so it is cosmetic.

**Evidence.** `path.zig:192-195` / `:236-239` `log.debug(...)` vs `path.rs:316` / `:380` commented-out lines.

**Suggested fix.** Emit the two debug lines through the crate's logging facade.

#### S78-F8 — edit.rs `dirname` helper is not `std.fs.path.dirname` (trailing-slash / root inputs)

- **Status:** ⚠️ DISPUTED (semantic lens refuted; presence lens confirmed)
- **Kind:** behavioral-difference
- **Oracle:** `src/config/edit.zig:31`
- **Rustty:** `crates/rustty-config/src/edit.rs:244-255` (presence lens: `:246-255`)

**Description.** `std.fs.path.dirnamePosix` first strips trailing `/` characters and returns null for `/` or a bare name; the port's `dirname` takes the last `/` as-is: for `a/b/` it returns `a/b` (Zig: `a`), for `/` it returns `/` (Zig: null per the finder; see refutation), and on Windows it never uses `dirnameWindows`. Not reachable with the actual candidates (`xdg.config` / appSupportDir outputs have ≥2 components and no trailing separator), so this is a helper-fidelity note rather than a user-visible defect.

**Evidence.** `edit.zig:31` `if (std.fs.path.dirname(config_path.name)) |config_dir|` vs `edit.rs:249-254` `let idx = path.iter().rposition(|&b| b == b'/')?; if idx == 0 { return Some(&path[..1]); } Some(&path[..idx])`.

**Refuting lens — SEMANTIC (verbatim).** "Re-read both. The finder's factual claim about '/' is wrong: std.fs.path.dirnamePosix returns \"/\" for input \"/\" (it strips trailing slashes and returns \"/\" when end_index reaches 0), which is exactly what edit.rs:246-255 returns (idx==0 -> Some(&path[..1])); empty input is null on both sides. The only true divergence is trailing-separator input ('a/b/' -> Zig 'a', port 'a/b'), and the finder itself concedes it is unreachable: the sole call site is edit.zig:31 / edit.rs on config_path.name, which comes from file_load::default_app_support_path / default_xdg_path / legacy_* (edit.rs:225-240), all of which end in the file component 'config' with no trailing separator. The Windows arm is likewise unreachable in this macOS/Linux-first port. A difference confined to unreachable inputs is not an observable functional difference."

**Counter-note (presence lens, not refuted).** The trailing-separator and missing-Windows-arm divergences are real and no faithful implementation exists in the workspace (`config_struct.rs:5419-5424` duplicates the same simplified helper), but they are unreachable from the actual `configPath` candidates.

**Suggested fix.** Port `dirnamePosix`'s trailing-separator stripping and null-for-root behaviour (or reuse a shared `std.fs.path` port) and dispatch to a Windows arm as `crate::path` does for `is_absolute`/`basename`.

#### S79-F2 — All five sentry.zig log lines are elided (comments only), including the user-facing 'crash report written to disk path=' warning

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/crash/sentry.zig:185, 239, 307, 338, 362`
- **Rustty:** `crates/rustty-crash/src/sentry.rs:430, 558, 736, 796, 837` (presence-lens corrected)

**Description.** The oracle logs `sentry initialized database={s}`, `no thread state, skipping crash metadata`, `failed to persist crash report err={}`, `sentry envelope does not contain crash, discarding` and `crash report written to disk path={s}`. rustty keeps each as a `// log...` comment with `let _ = err;` / `let _ = path;` and emits nothing. The `crash report written to disk path=…` warning is the only way a user learns where the report landed. Verification confirms no `log`/`tracing` dependency exists in any crate, so nothing is relocated; this is a port-wide convention, hence low, but it is an observable stderr difference.

**Evidence.** `sentry.zig:362 log.warn("crash report written to disk path={s}", .{path});` vs `sentry.rs:792-793` comment + `let _ = path;`.

**Suggested fix.** Route these through whatever logging facility the port adopts (at minimum the two `log.warn` sites), keeping the scoped-logger name `sentry` and message text.

#### S79-F3 — Header JSON parser edge cases differ: duplicate keys and out-of-i64-range integers

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/crash/sentry_envelope.zig:72-77, 123-128`
- **Rustty:** `crates/rustty-crash/src/sentry_envelope.rs:146, 195` (presence lens: `:158`, `:226`, length arm `:268`)

**Description.** Oracle header parsing uses `std.json.parseFromSliceLeaky(std.json.Value, …)` whose `duplicate_field_behavior` default is `.@"error"` (duplicate object keys fail the parse, `std/json/static.zig:19-26` + `dynamic.zig:148-158`) and which turns integers outside i64 into `.number_string` (`dynamic.zig:36-43`; a `length` of that form then hits `else => error.EnvelopeItemLengthMalformed`). rustty uses `serde_json::from_slice::<Value>`: duplicate keys silently keep the last value; integers in (i64::MAX, u64::MAX] parse as u64 so `"length": 18446744073709551615` is accepted and only fails later as `EnvelopeItemPayloadTooShort` (different error tag); integers beyond u64 fail the whole parse instead of yielding a string-number value. All are malformed-envelope edge cases; sentry-native never emits them, hence low.

**Evidence.** `sentry_envelope.zig:72-77` vs `sentry_envelope.rs:146`; length handling `sentry_envelope.zig:139-146` vs `sentry_envelope.rs:213-221` (`n.as_u64()` accepts > i64::MAX).

**Suggested fix.** Reject duplicate keys in a post-parse walk (or a custom Visitor), and require `n.as_i64().is_some()` before the u64 cast in the `length` arm.

#### S79-F4 — Serialized header float/number formatting relies on serde_json rather than `std.json` Stringify

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** behavioral-difference
- **Oracle:** `src/crash/sentry_envelope.zig:229-234, 244-251, 505-511`
- **Rustty:** `crates/rustty-crash/src/sentry_envelope.rs:302-304, 320-322, 507-516` (presence lens: `:375`, `:399`)

**Description.** Oracle `serialize` renders header maps with `std.json.fmt(…, .{ .whitespace = .minified })`; rustty uses `serde_json::to_writer` (compact). Object key order (serde_json is pinned with `preserve_order`), string escaping, integers, bools and null are identical, but floats are formatted by different algorithms — Zig's `{}` on f64 (verified: `2.0` → `2`, `1e100` → 100 digits, `1e-7` → `0.0000001`) vs serde_json/ryu shortest round-trip (`2.0`, `1e100`, `1e-7`). A header containing a float could be re-emitted with different textual digits. No reference test covers a float header (the session test's float is inside the opaque payload, copied verbatim), so this is an unexercised edge case.

**Evidence.** `sentry_envelope.zig:229-234 writer.print("{f}\n", .{std.json.fmt(std.json.Value{ .object = self.headers }, json_opts)})` vs `sentry_envelope.rs:302-303 serde_json::to_writer(&mut *writer, &self.headers)?;`.

**Suggested fix.** Add a rustty-only round-trip test with a float-valued header to pin the expected text, and format `serde_json::Number` floats explicitly to match `std.json` if it diverges.

#### S79-F5 — Init-thread self-naming is gated on macOS only, not the full Darwin family

- **Status:** ✅ CONFIRMED (both lenses)
- **Kind:** missing-branch
- **Oracle:** `src/crash/sentry.zig:112-114`
- **Rustty:** `crates/rustty-crash/src/sentry.rs:285-296`

**Description.** Oracle names the init thread via `internal_os.macos.pthread_setname_np("sentry-init")` when `builtin.os.tag.isDarwin()` (macOS, iOS, tvOS, watchOS, visionOS). rustty gates the call on `#[cfg(target_os = "macos")]` because rustty-os's macos module is `#![cfg(target_os = "macos")]`, so an iOS build (where `build_options.rs:36-37` defaults SENTRY on) would leave the thread unnamed. The port's own standing rule (`.port/phase-2/DEFERRALS.md:2907-2915`) corrects exactly this class of narrow gating to `any(macos, ios)`. Cosmetic (debugger/crash-dump thread label only).

**Evidence.** `sentry.zig:112-114 if (builtin.os.tag.isDarwin()) { internal_os.macos.pthread_setname_np(&"sentry-init".*); }` vs `sentry.rs:293-296 #[cfg(target_os = "macos")] unsafe { internal_os::macos::pthread_setname_np(c"sentry-init".as_ptr()) };`.

**Suggested fix.** Widen the cfg to `any(target_os = "macos", "ios", "tvos", "watchos", "visionos")` once rustty-os exposes `pthread_setname_np` for those targets, or use `libc::pthread_setname_np` under `target_vendor = "apple"`.

## Performance-only differences (not findings)

- `Limit(comptime T, comptime default)` becomes `Limit<T: LimitInt, const DEFAULT: u128>` with `default_value()`; `ScrollbackLimitLines` carries `usize::MAX as u128` and converts back through `LimitInt::from_default` — identical observable values.
- Zig `?[:0]const u8` string fields (`language`, `title`, `class`, `x11-instance-name`) are owned `Option<Vec<u8>>` (no arena) — same observable bytes.
- Zig comptime `inline for` field dispatch is a hand-written `match` on field-name byte literals plus a Key-exhaustiveness test; `c_get`'s single comptime `inline else` becomes an explicit 209-arm match; `FileFormatter` iterates a precomputed `CONFIG_FIELDS` table.
- `FontShapingBreak`, `AlphaBlending`, `BackgroundImagePosition/Fit`, `WindowColorspace`, `WindowPaddingColor` are hoisted to `rustty-render-types` (cycle break) and re-exported; defaults preserved.
- `shallowClone` (a bitwise copy sharing the arena) becomes a full deep `clone_config()` with `_arena = true` — identical observable state, more copying.
- `loadFsFile`/`loadTheme` use a 2048-byte streaming reader in the oracle; the port reads the whole file into a `Vec<u8>` before handing it to `LineIterator`.
- `Replay.Iterator` borrows the step slice; the port takes an owned `Vec<ReplayStep>` and clones each step on `next_self` — same yields.
- `_conditional_set` is `std.EnumSet` → `HashSet<conditional::Key>`; the loaded-files cycle map is `StringHashMap` → `HashSet<Vec<u8>>` — same membership semantics.
- `std::fs::File::open` follows symlinks before `metadata()`, so the `sym_link` arm of `file_kind_name` is effectively unreachable; the oracle's `openFileAbsolute` + stat behaves the same.
- `Color::format_buf` returns an owned `Vec<u8>` instead of writing into the caller's 128-byte buffer; `ColorList::format_entry` accumulates into a growable `Vec` (the 64-colour cap makes the fixed buffer unreachable to overflow).
- `Keybinds.tables` uses `AutoArrayHashMapUnmanaged<Vec<u8>, Set>` in place of `StringArrayHashMapUnmanaged`; insertion order and get/getOrPut semantics preserved. `Keybinds::parse_cli` skips the reference's arena copy of colon-containing values (the Rust parse APIs copy retained bytes). `Keybinds::init` builds the digit goto_tab bindings from an explicit table instead of comptime `@field`.
- `Keybinds.formatEntryDocs`'s `[1024]u8` fixed writer is modeled by rustty-input's `RewindBuffer` (capacity 1024), so overflow → OutOfMemory is identical and pinned by a test.
- `RepeatableCodepointMap.parseCLI` clones the family `Vec` per entry instead of sharing one arena pointer; `RepeatableClipboardCodepointMap.parseCLI` hand-clones `Replacement` per range.
- `RepeatableCommand.clone/init`: `ensureUnusedCapacity`/`appendAssumeCapacity` → `Vec::reserve`/`push`; `errdefer` collapses to Drop; OOM paths are aborts per the port's allocation model.
- Bare enums are name-mapped through the `bare_enum_names!` table instead of `std.meta.stringToEnum`/`@tagName`; string sets verified identical.
- `MouseScrollMultiplier.formatEntry` uses `format!` instead of a 4096-byte fixed writer; two f64s can never approach 4096 bytes.
- `QuickTerminalSize.Size.toPixels`: `@intFromFloat` vs Rust `as u32` differ only for out-of-u32-range products, which is safety-checked UB in the oracle.
- `Duration.asMilliseconds`: the oracle's `divTrunc … catch maxInt(c_uint)` arm is unreachable (constant non-zero divisor); the port uses plain division plus the same saturation.
- `WindowDecoration.getGObjectType` (GTK `defineEnum` registration) has no Rust counterpart, but no oracle GTK use-site calls it.
- `Wasm.zig config_new`'s allocation/default failure arms are unreachable in the port because `Config::default_config()` is infallible.
- `RepeatableStringMap.clone` replaces `ensureTotalCapacity` + `putAssumeCapacity` with plain insertion-order `put`; same order and contents.
- `io.rs` / `path.rs` / `value_types.rs` format entries into growable `Vec`s but still enforce the same fixed-buffer OutOfMemory bounds (4096 / max_path_bytes+1).
- `command.rs`'s `ArgIteratorGeneral` owns a copy of `cmd_line` and advances `index` past the terminating whitespace before returning a token; the token sequence is identical.
- `Path::expand` uses `canonicalize(base.join(path))` instead of `openDirAbsolute` + `realPathFile`; for an existing target the resolved path is the same.
- `theme.rs` reads the resources dir through a `OnceLock` injection seam populated by rustty-core instead of a direct `global_state.resources_dir` read (R4 cycle break).
- `edit.rs` / `file_load.rs` / `theme.rs` return owned `Vec`/`String` instead of arena slices; the arena bookkeeping has no observable equivalent.
- `url.rs`: the oracle's comptime `++`-concatenated `pub const regex` is assembled at runtime by `regex() -> String`; output bytes identical.
- `sentry.rs`: static `dir_mem` + FixedBufferAllocator dupes become owned `Vec<u8>` inside `Mutex<Option<..>>` statics; the 4096-byte buffered file writer collapses to one `write_all`; the arena collapses to ownership.
- `sentry_envelope.rs`: the per-envelope arena is replaced by owned clones/`mem::take` (`Item.encode` returns a borrow of the in-place encoded item); `std.Io.Reader` streaming helpers are reimplemented over `BufRead::fill_buf/consume` with the same limit/EOF/StreamTooLong contracts.

## Refuted candidates

No finding in this area was refuted by both verifier lenses. Two findings were refuted by exactly one lens and are retained above as DISPUTED:

- **S74-F7 — wasm SHELL-env/flatpak lookup not gated off** — semantic lens: the residual wasm path is a provable no-op (`isFlatpak()` is compile-time false off Linux and `probable_cli_environment()` returns false on wasm, so the block breaks out before the `SHELL` lookup); presence lens disagreed and confirmed.
- **S78-F8 — `edit.rs dirname` is not `std.fs.path.dirname`** — semantic lens: the `/` claim is factually wrong (Zig also returns `"/"`), and the only real divergence (trailing separator, Windows arm) is unreachable from the actual `configPath` candidates; presence lens confirmed the textual divergence.
