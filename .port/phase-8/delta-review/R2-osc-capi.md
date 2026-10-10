# Phase 8 delta review R2: OSC parsing and the libghostty-vt C API

Range `12752b2ac..a60e9e2a5` (`origin/main` in `../ghostty`, read with `git show` / `git diff`; the
oracle working tree stayed at the pin). Rustty at `401b95b0e`. Read-only review: no Rustty code was
changed and nothing was built. Upstream line numbers are at `a60e9e2a5`; Rustty line numbers are at
`401b95b0e`.

**Scope.** `src/terminal/osc.zig`, `src/terminal/osc/**`, `src/terminal/c/**`, `include/ghostty/vt/*.h`,
`src/lib_vt.zig`, `src/lib/windows_dll.zig`, `example/c-vt-{compression,effects,snapshot}`,
`test/esctest/**`, `src/benchmark/{OscParser,TerminalStream}.zig`, the `freeCount` halves of
`src/datastruct/{untouched_pool,wasm_page_pool}.zig`, and the stream / stream_terminal halves that the
new effects and OSC commands need. 22 non-merge commits touch this area:

`7b11f3dca 520d8f55a a141f9bdb a7cf951eb 438d584f0 7bb45ba34 02c51decf 51d3ca168 9a4787d90 fa604a170
2b0ceff7d b1bfd1d9c 36953bca8 8834e161c 88e66cbc6 9272a2f7c 7e2d1793d 25b04b410 edcaf6670 82ae67b9f
aca9bf031 bae2c3cdb`.

`9272a2f7c` (DECRQCRA/XTCHECKSUM core, `xt_checksum.zig`), and the PageList/Screen/snapshot-history
halves of `8834e161c` and `88e66cbc6`, belong to R1 (terminal core). This report maps only the C-API,
header and example halves of those commits and lists the core pieces as dependencies.

**Method.** Full diffs of every file above; `test "…"` blocks enumerated per file; Rustty mapped by
grep over the whole workspace (`crates/`, `crates-sys/`, `include/`, `example/`, `test/`,
`.github/`, `.port/`); records checked: `UPSTREAM-DEFECTS.md`, `NEW-FINDINGS.md`,
`VERIFICATION_DEBT.md`, `docs/ghostty-bug-fixes.md`, `.port/phase-0/corpora/MANIFEST.md`.

**Rustty baseline.** Every C enum is exactly at the pin: `RUSTTY_TERMINAL_OPT_*` ends at
`RENDER_HOLD = 41` (`include/rustty/vt/terminal.h:1705-1706`), `RUSTTY_TERMINAL_DATA_*` at
`MOUSE_SHAPE = 41` (`:2120-2121`), `RUSTTY_OSC_COMMAND_*` at `KITTY_DESKTOP_NOTIFICATION = 26`
(`osc.h:69-70`), `RUSTTY_OSC_DATA_*` at `CHANGE_WINDOW_TITLE_STR = 1` (`osc.h:95-96`),
`RUSTTY_SNAPSHOT_DECODER_OPT_*` at 1 and `_DATA_*` at 8 (`snapshot.h:161-163, 254-256`), and
`RUSTTY_TERMINAL_UNKNOWN_SEQUENCE_*` has only `APC = 0` (`terminal.h:354-355`). All of them keep
the `_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE` sentinel. Every new value upstream adds is an append, so
no existing discriminant moves.

### Upstream tests in scope

Added (55):

| file | tests (exact upstream names) | sha |
|---|---|---|
| `osc.zig` | `Parser end with CAN or SUB cancels the command` | 520d8f55a |
| `osc/parsers/unknown.zig` (NEW) | `OSC unknown: prefix state names are the bytes consumed`, `…: disabled by default`, `…: reported`, `…: supported commands are unchanged`, `…: aborted sequences are dropped`, `…: truncated at the limit`, `…: limit smaller than the identifier`, `…: fixed tier does not allocate`, `…: allocating tier`, `…: large limit without allocator uses fixed buffer`, `…: allocation failure falls back to fixed buffer`, `…: allocation failure while growing truncates`, `…: nextSlice matches per-byte parsing` (13) | 7b11f3dca (end state after 520d8f55a) |
| `osc/parsers/program_status.zig` (NEW) | `OSC 7501: query`, `…: minimal report`, `…: all keys`, `…: base64 padding is optional`, `…: whitespace around keys and values`, `…: last value wins`, `…: malformed pairs and unknown keys are skipped`, `…: kind and progress only apply to some states`, `…: out of range progress is absent`, `…: invalid app is ignored`, `…: ids`, `…: text must decode to safe UTF-8`, `…: empty text is absent`, `…: discarded reports`, `…: no allocator` (15) | bae2c3cdb |
| `Parser.zig` | `osc: 105 empty` | 36953bca8 |
| `c/osc.zig` | `command data null`, `unknown` | 02c51decf, 7b11f3dca |
| `c/terminal.zig` | `get memory_usage`, `get memory_usage size rule`, `set program_status callback`, `set semantic_prompt callback`, `set reset callback`, `checksum report requires explicit opt in`, `checksum extension survives resets` | 8834e161c, bae2c3cdb, 7bb45ba34, 7e2d1793d |
| `c/paste.zig` | `terminal_paste refused write ignored by the reader` | fa604a170 |
| `c/sys.zig` | `decode_png output is zeroed before the callback` | aca9bf031 |
| `c/snapshot.zig` | `snapshot decoder compresses history when requested` | 88e66cbc6 |
| `stream_terminal.zig` | `unknown OSC effect callback`, `program_status effect callback`, `semantic_prompt effect callback`, `reset effect callback`, `window_title not changed by cancelled OSC` (+ R1's `DECRQCRA disabled`, `DECRQCRA enabled`, `XTCHECKSUM ignored without xt checksum report`) | 7b11f3dca, bae2c3cdb, 7bb45ba34, 520d8f55a |
| `stream.zig` | (R1) `stream: DECRQCRA dispatch`, `stream: XTCHECKSUM dispatch` | 9272a2f7c |
| `PageList.zig` | (R1) `PageList memory usage`, `PageList PageAllocation compresses prepended pages unless visible` | 8834e161c, 88e66cbc6 |
| `snapshot/history.zig` | (R1) `HISTORY decode compresses history pages when requested` | 88e66cbc6 |

Changed (no renames, none removed): `stream_terminal.zig` `unknown APC effect callback` (now counts
`.osc` arms and asserts an APC limit does not enable OSC capture); `c/terminal.zig`
`set unknown_sequence callback` (OSC arms; `UNKNOWN_MAX_BYTES` sets both limits) and
`get mouse_shape` (one row: empty OSC 22 → `text`, b1bfd1d9c); `c/types.zig`
`manifest uses public enum names` (asserts `SEMANTIC_PROMPT = 42`, `RESET = 43`,
`GhosttySemanticPromptKind.INVALID = 0 / COMMAND_END = 4`, `…PromptKind.SECONDARY = 3`);
`c/snapshot.zig` `decoder option and empty source` (compress_history get/set);
`untouched_pool.zig` `UntouchedPool: create, destroy, reuse` (`freeCount` asserts);
`PageList.zig` `PageList PageAllocation finalizes pages and preserves live state` (`finalize(.prepend, .{})`).

## 1. Summary table

Status is Rustty's state today. Size: S under 2 h, M under 1 day, L more than 1 day. "Obs." says
whether the change is observable behaviour (Y) or internal/infra (N).

| # | commit(s) | theme | obs.? | Rustty status | size | top risk |
|---|---|---|---|---|---|---|
| 1 | `7b11f3dca` (+`520d8f55a`) | OSC parser captures unimplemented OSCs (`unknown_max_bytes`, `Command.unknown`, `unknownOrInvalid`, `captureFailed`, bridge states at `end`) | Y (opt-in) | ABSENT | M | the fixed-capture fast path checks `MAX_BUF`, not the capture's limit; prefix recovery must be a `const` table, not `State::tag_name()` |
| 2 | `7b11f3dca` | stream `osc_unknown` action, `Stream.Options.osc_unknown_max_bytes`, `UnknownSequence::Osc` | Y | ABSENT | S | content borrows the OSC parser across the effect |
| 3 | `7b11f3dca` | C: `rustty_osc_set`, `RusttyOscOption`, `RusttyOscTerminator`, `OSC_COMMAND_UNKNOWN = 27`, `OSC_DATA_UNKNOWN_{CONTENT,TRUNCATED,TERMINATOR} = 2..4`, `TERMINAL_UNKNOWN_SEQUENCE_OSC = 1`, `RusttyTerminalUnknownOscSequence`, `UNKNOWN_MAX_BYTES` sets both limits | Y (ABI) | ABSENT | M | `rustty_osc_set` must clear the stored command (SB); D11 `c_int` narrowing; new export breaks A5 until added |
| 4 | `520d8f55a` | CAN / SUB cancel an in-progress OSC; `rustty_osc_end` documented to return NULL | Y | ABSENT | S | none (the scalar path already passes the byte to `end`) |
| 5 | `bae2c3cdb` | OSC 7501 program status parser (states `75/750/7501`, `OSC_COMMAND_PROGRAM_STATUS = 28`) | Y | ABSENT | M | base64 helper lives `pub(crate)` inside `apc/glyph/request.rs`; limits as `const` |
| 6 | `bae2c3cdb` | `program_status` effect, query reply, RIS `clear`; C option 46, `RusttyTerminalProgramStatus` (88 B), 2 enums | Y (ABI) | ABSENT | M | trampoline re-enters the terminal; option 46 is numbered after R1's 44/45 |
| 7 | `7bb45ba34` | `semantic_prompt` and `reset` effects (Zig + C options 42/43, structs, 2 enums, `SemanticPrompt` export) | Y (ABI) | ABSENT | M | the effect must not fire when the terminal rejects the sequence (Rustty's `vt()` swallows the error); RIS effect order |
| 8 | `02c51decf`, `51d3ca168` | `rustty_osc_command_data(NULL, …)` answers false | Y (C) | ALREADY-HAS: **UD312 FIXED-SAME** | S | test name differs from upstream's |
| 9 | `fa604a170`, `2b0ceff7d` | a paste fails after any refused write, even if the reader returns true | Y (C) | ABSENT | S | none |
| 10 | `aca9bf031` | `decode_png` output zeroed before the callback; header contract | Y (C) | ALREADY-HAS (behaviour); docs + test ABSENT | S | none |
| 11 | `82ae67b9f` | allocator `alignment` documented as a log2 bit count | doc | ABSENT (Rustty's header has the old, wrong text; the code already passes log2) | S | none |
| 12 | `9a4787d90` | same-size resize documented as not a no-op | doc | ABSENT | S | none |
| 13 | `36953bca8` | OSC 105 reaches the color parser | Y | ABSENT (`osc_105` op exists but no state reaches it) | S | none |
| 14 | `b1bfd1d9c` | empty OSC 22 resets the mouse shape (`mouse_shape_reset` action) | Y | ABSENT | S | termio arm differs from libvt's (R1/R3 overlap) |
| 15 | `8834e161c` | `RUSTTY_TERMINAL_DATA_MEMORY_USAGE = 42`, sized `RusttyTerminalMemoryUsage` (112 B); example | Y (ABI) | ABSENT (`memory_stats`, `can_reclaim` exist) | M (C half S) | sized-struct rule; core half is R1's |
| 16 | `88e66cbc6` | snapshot decoder `COMPRESS_HISTORY` (opt 2, data 9); example | Y (ABI) | ABSENT | S (C half) | core half (`PageAllocation.finalize` options) is R1's |
| 17 | `7e2d1793d`, `25b04b410` | C options `XT_CHECKSUM_REPORT = 44`, `XT_CHECKSUM_EXTENSION = 45`; lib exports `xt_checksum`, `device_attributes` | Y (ABI) | ABSENT | S (C half) | blocked on R1's `xt_checksum` port; numbering for #6 |
| 18 | `a141f9bdb`, `a7cf951eb` | Windows DLL runs C++ global constructors (simdutf); CI loads the vt DLL with `c-vt-effects` | Y (Windows crash) | N/A likely (rustc's CRT entry runs `.CRT$XCU`), UNPROVEN | S | rwin proof needed; CI step worth porting regardless |
| 19 | `438d584f0`, `edcaf6670` | esctest against libghostty-vt, report-only CI job | N (test infra) | ABSENT (MANIFEST TODO-2 open since phase 0) | M | needs R1's DECRQCRA; python3; pinned network fetch |
| 20 | `7b11f3dca` | benchmark options `unknown-max-bytes`, `osc-unknown-max-bytes` | N | ABSENT | S | G-BENCH fence for #1 |

## 2. Per-theme detail

### 2.1 Unknown OSC capture in the parser (#1)

Upstream (`src/terminal/osc.zig` at `a60e9e2a5`):

- `Command.unknown: Unknown` and `Command.program_status` appended to the union (`:173, :178`) and to
  the `LibEnum` key list (`"unknown"`, `"program_status"` after `"kitty_desktop_notification"`), so
  C `GHOSTTY_OSC_COMMAND_UNKNOWN = 27`, `PROGRAM_STATUS = 28`.
- `Command.Unknown` (`:229-261`): `content: []const u8`, `truncated: bool`, `terminator: Terminator`,
  plus `Unknown.C = extern struct { truncated: bool, content: lib.String, terminator: Terminator.C }`
  and `cval()`.
- `Parser.unknown_max_bytes: usize` (`:403`, default 0 in `init`), two new states `unknown`,
  `unknown_truncated`, and new prefix states `75`, `750`, `7501`, `105`.
- Every `else => self.state = .invalid` in `next` becomes `else => self.unknownOrInvalid(c)`.
- `noinline fn unknownOrInvalid(self, c: ?u8)` (`:746-776`): with `unknown_max_bytes == 0`
  (`@branchHint(.likely)`) it is exactly the old `.invalid`. Otherwise it recovers the digits read so
  far from the **state's tag name** (`inline else => |s| @tagName(s)`), begins a capture (fixed
  `buffer[0..min(max, MAX_BUF)]`, or allocating when `max > MAX_BUF` and an allocator exists, falling
  back to fixed on failure), writes the prefix and then `c`.
- `inline fn captureFailed` (`:784-806`) replaces `self.state = .invalid` in `next` and `nextSlice`:
  for `.unknown` it pins `cap.max_bytes = cap.trailing().len` and moves to `.unknown_truncated`; for
  supported OSCs it is still `.invalid`.
- `end` (`:1090-…`): bridge states that are not OSCs themselves (`3, 30, 300, 55, 552, 6, 75, 750,
  77`, `:1118`) call `unknownOrInvalid(null)` and then `parsers.unknown.parse`; `.unknown` /
  `.unknown_truncated` go to `parsers.unknown.parse`. The old `=> null` arms for those prefixes are
  gone.
- `parsers/unknown.zig` (NEW, 352 lines at end state): `parse` builds `Command.unknown` from
  `cap.trailing()`; the 13 tests above, including one that iterates every prefix state and checks
  feeding its tag name reaches it.

Rustty: `crates/rustty-vt/src/osc/mod.rs` — `Command` `:324`, `CommandKey` `:416-443`, `MAX_BUF`
`:688`, `Parser` `:694`, `State` `:741-795`, `Capture`/`CaptureBacking` `:822-836`,
`capture_trailing` `:1165`, `next_slice` `:1216-1251`, `next` `:1260-1290`, `end` `:1556`.
`crates/rustty-vt/src/osc/parsers/mod.rs` has no `unknown` or `program_status`. ABSENT.

What to port:

1. `Unknown<'p>` (borrowing, `Copy`, like the other `Command<'p>` payloads; N-finding at
   `NEW-FINDINGS.md:5740-5760` describes the borrowed-command design) and `CommandKey::Unknown`,
   `CommandKey::ProgramStatus` appended.
2. `Parser.unknown_max_bytes`, `State::Unknown`, `State::UnknownTruncated`, `S75`, `S750`, `S7501`,
   `S105`; `unknown_or_invalid` as `#[inline(never)]` (upstream measured a 4 % osc-parser loss when it
   was inlined), `capture_failed` as `#[inline(always)]`.
3. **Prefix recovery.** Upstream relies on `@tagName`. Rustty's `State::tag_name()` (`:803-809`)
   formats `Debug` into a `String`: it allocates and is not usable here (and would trip the vt
   alloc-ban gate). Port it as a `const fn prefix(self) -> &'static [u8]` match (the comptime
   `@tagName` table). Port the `prefix state names are the bytes consumed` test against that table.
4. **Fixed-capture limit.** Upstream's fixed unknown capture is a *shorter slice* of the buffer
   (`self.buffer[0..@min(max_bytes, MAX_BUF)]`), so its per-byte bound is `max_bytes`. Rustty's
   per-byte fast path checks `*len < MAX_BUF` only (`osc/mod.rs:1284`), deliberately (PERF note
   `:1271-1281`). Unported, an unknown OSC with `unknown_max_bytes = 8` would keep 2048 bytes and never
   set `truncated`. Compare against `cap.max_bytes` (equal to `MAX_BUF` for every supported fixed
   capture, so the branch shape is unchanged) and measure with the osc-parser bench (G-BENCH). If it
   costs, split a `Fixed` variant that carries its own limit.
5. `captureFailed`'s pin of `max_bytes` must also stop the allocating arm's growth (`:1292-1305`
   checks `w.len() >= cap.max_bytes`, so pinning `cap.max_bytes` is sufficient).

Tests: the 13 `OSC unknown:` tests. `allocation failure while growing truncates` needs a
FailingAllocator with `fail_index = 1`, `resize_fail_index = 0`; Rustty's `VtAllocator` C arm can
express that through a counting C allocator (precedent: `osc_allocation_failure_sweep_through_the_c_api`).

Risks: hot path (G-BENCH osc-parser, `comptime-flattening-perf-hazard` memory: no `OnceLock`, no
`HashMap`, no allocating helper); the `unknown_or_invalid` `assert!(capture.is_none())` is an
invariant assert and ports.

UD candidates: none. Every prefix state that is not an OSC is in the bridge list; I checked the
state set.

### 2.2 CAN / SUB cancel an OSC (#4)

Upstream `osc.zig:1090-1096`: `end` returns null first when `terminator_ch` is `0x18` or `0x1A`. The
same commit removes the equivalent check from `unknown.parse`. Header `osc.h` gains an "Ending a
Sequence" section and `ghostty_osc_end` now says it returns NULL for invalid or cancelled sequences
(the old text "never NULL" was false: UD312's root cause).

Rustty: the VT parser calls `osc_parser.end(Some(c))` on the `osc_string` exit
(`crates/rustty-vt/src/parser.rs:747-755`), and the stream's OSC fast path sends CAN/SUB to the
scalar path (`crates/rustty-vt/src/stream.rs:1687-1689` "CAN/SUB abort ... go through the state
machine below"). So the one-line check in `Parser::end` (`osc/mod.rs:1556`) is the whole behavioural
port. ABSENT. Port the header text (`include/rustty/vt/osc.h:156-184`, which still says "never NULL")
and both tests (`Parser end with CAN or SUB cancels the command`, `window_title not changed by
cancelled OSC`). S.

### 2.3 OSC 7501 program status (#5, #6)

Upstream `osc/parsers/program_status.zig` (NEW, 731 lines): limits as `pub const` (sequence 4096,
body `4096 - 7 - 2`, key 16, msg 2732/2048, title 256/192, app 32, id 128/32/8); `Command =
union(enum) { query: Terminator, report: Report }`; `State` (`idle, working, done, blocked, error,
clear`) and `Kind` (`permission, question, auth`) as `lib.Enum`; `Report.readOption` /
`writeText`; `validate` (every key and every value checked even if a later pair replaces it; invalid
id discards the report); `parseProgress` (0..100, digits only); `decodeText` (base64 with optional
padding, then `encoding.isSafeUtf8`). Parser: `.@"7"` → `'5'` → `75` → `750` → `7501` → `';'` →
`captureTrailing(.allocating)`; `end` `.@"7501"` → `program_status.parse` (`osc.zig:1176`).

Stream (`stream.zig:2794`) dispatches `.program_status`; stream_terminal
(`stream_terminal.zig:225, :944-967`): `effects.program_status`; `.query` replies
`ESC ] 7501 ; ? <same terminator>` through `write_pty` only while the effect is set; RIS calls
`programStatusReport(.{ .state = .clear })` after the progress removal (`:700-716`).

C (`c/terminal.zig:195-226, :701-730, :1333, :1444-1452`): `GhosttyProgramStatusState` (=`State`,
0..5), `GhosttyProgramStatusKind` (`none` first, then the 3 kinds: 0..3), `GhosttyTerminalProgramStatus
{ size_t size; State state; Kind kind; int8_t progress; GhosttyString id, app, title, message; }`,
`GhosttyTerminalProgramStatusFn(terminal, userdata, const GhosttyTerminalProgramStatus*)`,
`GHOSTTY_TERMINAL_OPT_PROGRAM_STATUS = 46`. The trampoline is **installed dynamically** (like
clipboard read/write) so a NULL callback leaves the query unanswered. Absent text is an empty string,
absent kind `none`, absent progress `-1`. Title and message are decoded into stack buffers of
`max_title_bytes` / `max_msg_bytes`.

Rustty: no 7501 anywhere (grep). ABSENT. `ValueIterator` exists
(`crates/rustty-vt/src/osc/kitty_metadata.rs:63-90`, runtime key + alphabet); `is_safe_utf8` exists
(`osc/encoding.rs:14`); a `std.base64` port with `calc_size_for_slice` exists but is `pub(crate)`
inside `crates/rustty-vt/src/apc/glyph/request.rs:1215`. Move it to a shared module (or
`rustty-zigstd`) rather than adding a third base64.

Layout of `RusttyTerminalProgramStatus`: 64-bit `size 0, state 8, kind 12, progress 16, id 24,
app 40, title 56, message 72`, size 88; wasm32 `0, 4, 8, 12, 16, 24, 32, 40`, size 48. Freeze it as a
`types.rs` manifest row (`.initStruct("GhosttyTerminalProgramStatus", …)`), with `initEnum` rows for
both enums.

Tests: 15 `OSC 7501:` parser tests; `program_status effect callback`; `set program_status callback`.

Risks: the effect is a C callback that can re-enter the terminal while `id`/`app` borrow the OSC
parser's buffer (see §2.9). Option 46 sits after R1's 44/45: either land R1's C option slots first or
reserve 44/45 in the enum with the R1 commit.

UD candidates: none found. The parser rejects whole reports on any limit breach; the trampoline's
`catch unreachable` writes are bounded by `validate`.

### 2.4 Semantic prompt and reset effects (#7)

Upstream `stream_terminal.zig`: `effects.semantic_prompt` (`:234`), `effects.reset` (`:250`),
`Handler.SemanticPrompt` (`:407-…`: `kind`, `prompt_kind`, `exit_code: ?i32`, `command`, `err`;
`Kind = lib.Enum{invalid, prompt_start, input_start, output_start, command_end}`, `PromptKind =
lib.Enum{primary, right, continuation, secondary}`), and `fn semanticPrompt` (`:885-937`):
`try self.terminal.semanticPrompt(cmd)` first, so **a rejected sequence never reaches the effect**;
`fresh_line` (`L`) reports nothing; `A`, `N`, `P` → `prompt_start` with `k=`; `B`, `I` →
`input_start`; `C` → `output_start` with the command line decoded into a gpa buffer of
`options_unvalidated.len` (empty on allocation or decode failure); `D` → `command_end` with
`exit_code` and `err`. RIS order (`:700-716`): render-hold release, terminal reset, grants cleared,
progress `remove`, program status `clear`, then `effects.reset`.

C (`c/terminal.zig:228-247, :732-754, :1329-1330`): `GhosttySemanticPromptKind`,
`GhosttySemanticPromptPromptKind`, `GhosttyTerminalSemanticPrompt { size_t size; Kind kind; PromptKind
prompt_kind; bool has_exit_code; int32_t exit_code; GhosttyString command; GhosttyString error; }`
(64-bit size 56; wasm32 36), `GhosttyTerminalResetFn(terminal, userdata)`,
`OPT_SEMANTIC_PROMPT = 42`, `OPT_RESET = 43`; both trampolines always installed. `lib_vt.zig` exports
`SemanticPrompt`. C `ghostty_terminal_reset()` does **not** fire `reset` (only RIS through the
stream does); that is the documented contract, not a defect.

Rustty: `crates/rustty-vt/src/stream_terminal.rs` `Effects` `:252-436` (no `semantic_prompt`,
`reset`, `program_status`), `full_reset` `:993-1018` (ends at the progress removal),
`semantic_prompt` `:1447-1449` is `self.vt("semantic_prompt", |t| t.semantic_prompt(v))`. ABSENT.

What to port: `semantic_prompt` must branch on the `Result` of `t.semantic_prompt(v)` and call the
effect only on `Ok` (the `vt()` helper logs and swallows, so it cannot be used as is). The
`output_start` buffer goes through `term().gpa()` fallibly (`try_reserve`-style), as the reference's
`alloc.alloc … catch` reports the step with an empty command. Rustty's semantic prompt parser already
has `read_option` / `write_command_line` / `ERR` / `EXIT_CODE` / `PROMPT_KIND`
(`osc/parsers/semantic_prompt.rs:64-70, 252, 263, 335`).

Tests: `semantic_prompt effect callback`, `reset effect callback`, `set semantic_prompt callback`,
`set reset callback`, and the four new asserts in `manifest uses public enum names`
(`crates/rustty-vt-c/src/types.rs:3722` area).

### 2.5 C API surface: OSC parser and unknown sequences (#3)

Upstream `c/osc.zig`: `pub const Option = enum(c_int) { unknown_max_bytes = 0 }` and `set` (NULL
value → 0; NULL parser → `invalid_value`; option range check only under `runtime_safety`);
`CommandData` gains `unknown_content = 2` (`lib.String`), `unknown_truncated = 3` (`bool`),
`unknown_terminator = 4` (`Terminator.C`); `Terminator = osc.Terminator.C`
(`GhosttyOscTerminator { ST = 0, BEL = 1 }`). `lib_vt.zig` exports `ghostty_osc_set`.
`c/terminal.zig`: `UnknownSequence.Tag` becomes `{apc, osc}`, value union gains `osc:
UnknownOscSequence` (`= osc.Command.Unknown.C`), `.unknown_max_bytes` writes **both**
`apc_handler.unknown_max_bytes` and `stream.parser.osc_parser.unknown_max_bytes` (`:1601-1606`).
`terminal.h` now includes `osc.h` and documents an "Unsupported Sequences" section.

Rustty: `crates/rustty-vt-c/src/osc.rs` (`end` `:194-221`, `CommandData` `:249-251`, range guard
`:289-293`), `crates/rustty-vt-c/src/lib.rs:1603-1683` (`rustty_osc_*` exports, no `set`),
`crates/rustty-vt-c/src/terminal/effects.rs:1211-1238` (`unknown_sequence_trampoline`, APC arm only),
`crates/rustty-vt-c/src/terminal/options.rs:883-893` (`UnknownMaxBytes` writes the APC limit only),
`include/rustty/vt/terminal.h:354-402`. ABSENT.

ABI notes:

- `RusttyTerminalUnknownSequenceValue` is a 128-byte padded union (`terminal.h:380-392`). The new
  `RusttyTerminalUnknownOscSequence` (64-bit: `truncated 0, content 8, terminator 24`, size 32;
  wasm32 size 16) fits, so the union and the by-value `RusttyTerminalUnknownSequence` do not change
  size. Add the `osc` member to the union declaration and the manifest.
- `rustty_osc_set(parser, RusttyOscOption option, const void* value)`: per gate D11
  (`check_closed_enum_abi.sh`) receive `option` as `c_int` and narrow with a `match`, answering
  `RUSTTY_INVALID_VALUE` for out-of-range values in every build. Upstream checks only under
  `runtime_safety` (§4 Q6, UD113).
- New enums `RusttyOscOption` and `RusttyOscTerminator` need `_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE`
  (`RUSTTY_OSC_OPT_MAX_VALUE`, `RUSTTY_OSC_TERMINATOR_MAX_VALUE`).
- A5 (`check_rustty_vt_headers.sh`) diffs the export list against the oracle's `lib_vt.zig`; once the
  pin advances it is RED until `rustty_osc_set` is exported.

**Hazard (SB): `rustty_osc_set` and the stored command.** Rustty's vt-c wrapper keeps the last
command (with its borrows lifetime-erased into the parser buffer) and clears it in every entry point
that touches the parser (`osc.rs:200-219`, PORT NOTE). `end` forms `&mut *parser_` (`:201`). If `set`
forms `&mut Wrapper` without clearing first, the retag invalidates the stored command's borrow and a
later `rustty_osc_command_data(…UNKNOWN_CONTENT…)` reads through a dead tag. The header says the
command is valid "until the next call to any `rustty_osc_*` function", so `set` should clear it like
`next`/`reset`/`end`/`free` (or write the field by raw place). Cover it with a Miri test: `end` →
`set` → `command_type` on the old pointer must not be used (document) and a fresh `end` must work.

Tests: `command data null`, `unknown` (c/osc.zig); the changed `set unknown_sequence callback`.

### 2.6 Smaller C-API commits (#8–#12, #14)

- **#8 `02c51decf`/`51d3ca168`** — upstream now does `command_ orelse return false`. This is exactly
  **UD312** (`UPSTREAM-DEFECTS.md:2554-2567`), which Rustty fixed in R13
  (`crates/rustty-vt-c/src/osc.rs:337-347`, PORT NOTE). Verdict **FIXED-SAME**. Action: drop the
  "reference defect" PORT NOTE (the code becomes faithful), rename
  `command_data_on_a_null_command_is_false` to the upstream name `command_data_null` (keep its extra
  assertions), set UD312 `upstream status: fixed upstream in 51d3ca168`, and update the row in
  `docs/ghostty-bug-fixes.md:123` (Rustty no longer differs).
- **#9 `2b0ceff7d`** — `readTrampoline` checks `sink.write_failed` before `ok`. Rustty
  (`crates/rustty-vt-c/src/paste.rs:357-369`) still has the old shape: a reader that ignores a
  refused write and returns `true` pastes text with a hole. ABSENT, not recorded. Port with the
  test `terminal_paste refused write ignored by the reader` (allocator lever: fail every allocation
  from the first write on; expects `RUSTTY_OUT_OF_MEMORY`, one read, zero pty writes).
- **#10 `aca9bf031`** — Rustty already passes a zeroed `Image`
  (`crates/rustty-vt-c/src/sys.rs:246-251`; Rust has no `undefined`) and rejects `data == NULL`
  (`:264-266`). ALREADY-HAS. Port the `sys.h` text (the zeroing guarantee, the `data_len` exact-size
  rule, the RGBA layout; `include/rustty/vt/sys.h:46-135`), update the stale `// var out: Image =
  undefined;` quote, and port `decode_png output is zeroed before the callback`.
- **#11 `82ae67b9f`** — Rustty's `allocator.h:113-114` says "a power of two between 1 and 16", but
  `rustty_lib::allocator::Alignment` crosses the C boundary as the log2
  (`crates/rustty-lib/src/allocator.rs:18-30`). Port the new text verbatim (renamed). Doc only.
- **#12 `9a4787d90`** — `terminal.h:2177` "this is a no-op" → upstream's sentence. Doc only.
- **#14 `b1bfd1d9c`** — stream `mouse_shape` with an empty value emits `mouse_shape_reset`
  (`stream.zig:2735`); libvt handler sets `.text` (`stream_terminal.zig:720`), termio sets `.default`
  under mouse tracking else `.text`. Rustty: `crates/rustty-vt/src/stream/dispatch.rs:186-200`
  (empty value → `from_string` fails → warn), `stream_terminal.rs:1391-1393`. ABSENT. The C test row
  (`get mouse_shape`) belongs here; the termio arm to R3.

### 2.7 Memory usage query and snapshot history compression (#15, #16)

`8834e161c` C half (`c/terminal.zig:1718-1765, :1997-2009`): `GHOSTTY_TERMINAL_DATA_MEMORY_USAGE = 42`,
`GhosttyTerminalMemoryUsage` (sized: `size_t size; bool compression_supported;` then 12 `uint64_t`,
primary × 6 then alternate × 6; 64-bit size 112, wasm32 104). `out.size < sizeof` → `INVALID_VALUE`
and the struct untouched; otherwise the whole struct is written with `size` preserved as the caller's.
`compression_supported = mem.canReclaim(.strict)`. The alternate fields stay 0 until the alternate
screen exists. Core (R1): `PageList.MemoryUsage`/`memoryUsage`, `Screen.memoryUsage`
(`image_bytes = kitty_images.total_bytes`), `UntouchedPool.freeCount`, `WasmPagePool.freeCount`.
Rustty has the inputs: `PageList::memory_stats` (`crates/rustty-vt/src/pagelist/compress.rs:651`),
`estimated_resident_bytes` (`:361`), `mem::can_reclaim` (`crates/rustty-vt/src/mem.rs:214`),
`UntouchedPoolBase.free` (`crates/rustty-datastruct/src/untouched_pool.rs:168`), `WasmPagePool`
(`wasm_page_pool.rs:158`). ABSENT. C data enum: `crates/rustty-vt-c/src/terminal/data.rs:23-75`.
Example: `example/c-vt-compression/src/main.c` (+29 lines) and README.

`88e66cbc6` C half (`c/snapshot.zig`): `DecoderOption.compress_history = 2`, `DecoderData
.compress_history = 9` (bool, `no_value` once failed), wrapper field defaulting false, passed into
`decoder.ready(… .{ .compress_history = … })`. Rustty `crates/rustty-vt-c/src/snapshot.rs:46-79,
:418`; `crates/rustty-vt/src/snapshot/codec.rs:392-396` (`DecodeOptions` has only
`max_continuation_bytes`). ABSENT. Core (R1): `PageAllocation.finalize(location, FinalizeOptions)`
compresses a prepended page unless it is the viewport's top-left page; `history` decode passes
`page_options`. Example: `example/c-vt-snapshot` (+25 lines, README).

Tests: `get memory_usage`, `get memory_usage size rule`, `snapshot decoder compresses history when
requested`, changed `decoder option and empty source`.

### 2.8 XT checksum C options (#17)

`7e2d1793d`: `OPT_XT_CHECKSUM_REPORT = 44` (`bool*`, NULL → false; writes
`handler.xt_checksum_report`), `OPT_XT_CHECKSUM_EXTENSION = 45` (`uint8_t*`, NULL → 0, `> 31` →
`INVALID_VALUE`, `terminal.setDefaultXtChecksum`). `25b04b410`: `lib_vt.zig` re-exports
`xt_checksum` and `device_attributes` (Rustty already has `pub mod device_attributes`,
`crates/rustty-vt/src/lib.rs:110`). Both depend on R1's `9272a2f7c` port. Tests: `checksum report
requires explicit opt in`, `checksum extension survives resets`. The C halves are S once R1 lands.

### 2.9 Re-entrant effect callbacks (Miri hazard, #2, #6, #7)

Rustty recovers the wrapper from the handler by exposed provenance
(`crates/rustty-vt-c/src/terminal/effects.rs:460-500`) and publishes the trampoline's handler pointer
in `live_handler` for the callback's duration (`LiveHandler`, `crates/rustty-vt-c/src/terminal.rs:310-338`,
N452), so a callback that calls `rustty_terminal_*` reaches the same handler through `handler_ptr`
(`:297-307`). The three new trampolines (`program_status`, `semantic_prompt`, `reset`) and the OSC arm
of `unknown_sequence` must follow that pattern exactly (raw handler pointer derived once, then
`LiveHandler::publish`), as `render_hold_trampoline_raw` does (`effects.rs:750-777`).

New aliasing shape: the unknown-OSC `content` and the program-status `id`/`app`/`err` slices borrow
the **OSC parser's buffer inside `wrapper.stream.parser`**, not the handler. The stream dispatches
with a command borrowed from `(*this).parser.osc_parser` and a `&mut (*this).handler`
(`stream.rs:1669-1680`, `stream/dispatch.rs:96`). Inside the callback a C caller may legally call
`rustty_terminal_set(…UNKNOWN_MAX_BYTES…)`, which upstream implements as a write to
`wrapper.stream.parser.osc_parser.unknown_max_bytes`. In Rustty that write must be a raw place write
(as the APC arm already is, `options.rs:891-892`); forming `&mut Stream`, `&mut Parser` or
`&mut osc::Parser` there would retag the buffer bytes that a protected argument
(`UnknownSequence<'b>` passed by value to the trampoline) still borrows. Add vt-c Miri tests in which
each new callback re-enters the terminal (`set UNKNOWN_MAX_BYTES`, `get MEMORY_USAGE`, `get TITLE`)
and then reads its borrowed strings. LIFETIMES.tsv gets rows for the three new `Effects` fields, like
the existing `Effects render_hold` row.

### 2.10 Windows DLL global constructors (#18)

`src/lib/windows_dll.zig` (NEW, 91 lines) exports a `DllMain` that walks `.CRT$XCA..XCZ` (MSVC) or
`__CTOR_LIST__` (MinGW), because Zig's `_DllMainCRTStartup` for a DLL never reaches the CRT bootstrap,
and simdutf built with `SIMDUTF_NO_LIBCXX` keeps its kernel pointer in a static that a constructor
must set. Rustty builds simdutf the same way (`crates-sys/simdutf-sys/build.rs:54-70`). rustc's
`cdylib` for `x86_64-pc-windows-msvc` links the CRT's `_DllMainCRTStartup`, which runs `_initterm`
over `.CRT$XC*`, and the `-gnu` target links `dllcrt2.o`. So Rustty most likely does not have the
crash. This is UNPROVEN: no Rustty test loads `rustty_vt_c.dll` (the Windows job loads only
`rustty_apprt_c.dll`, `.github/workflows/ci.yml:745-768`). Recommendation (§4 Q2): NON-PORT of
`windows_dll.zig`, plus a port of `a7cf951eb`'s CI step against `rustty_vt_c.dll`, with a multi-byte
UTF-8 write added to `example/c-vt-effects`, run once on rwin first.

### 2.11 esctest harness (#19)

Upstream `test/esctest/` is a standalone Zig project (`build.zig`, `build.zig.zon` with a path
dependency on `../../` and a lazily fetched, hash-pinned esctest2 tarball at
`2798f12149a19c3295e9b4853ab2da4b2eff1b2b`), and `src/main.zig` (262 lines). It uses the **Zig module**
`ghostty-vt`, not the C library. It `forkpty`s `python3 esctest.py --expected-terminal=xterm
--xterm-checksum=411 --xterm-reverse-wrap=411 --timeout=0.2 --logfile=/tmp/esctest-libghostty-<pid>.log
[user args]` in `share/esctest`, at 80x25. It feeds every pty read into a `Terminal` built with
`default_xt_checksum = {positive, no_attributes, no_trim, full}`, `xt_checksum_report = true`,
`write_pty`, `size` (cell 10x20) and `device_attributes` (VT520, level 5, ANSI color). It treats `EIO`
as EOF (Linux), checks `python3` on PATH, and copies the log to stdout. CI job `test-esctest`
(`.github/workflows/test.yml`) is `continue-on-error: true`: it writes the tally and the failing list
to the step summary and uploads the log. `edcaf6670` enables DECRQCRA in it.

Rustty has no harness. `.port/phase-0/corpora/MANIFEST.md:197-223` records "Category 4 — vttest /
esctest conformance (0 goldens — TODO-2)". `nix/devShell.nix:59, :165` has `vttest` but not esctest.
Port design (§4 Q1):

- `test/esctest/` as a Cargo package (`publish = false`, workspace member so it shares the lock and
  `target/`), binary `esctest-rustty`, `#[cfg(unix)]` body (Windows: a stub that prints "unix only"),
  depending on `rustty-vt` and `libc` (`forkpty`, `execvp`, `waitpid`). The same 80x25 setup,
  arguments, effects and log handling, ported line for line from `main.zig`.
- esctest2 fetched by `test/esctest/fetch.sh` at the same commit with a sha256 check into
  `target/esctest2/`. No network access from `build.rs`. The binary takes the esctest directory from
  `--esctest-dir` or `RUSTTY_ESCTEST_DIR`, because Cargo has no `share/` install step.
- CI: a report-only `esctest` job in `.github/workflows/ci.yml` (`continue-on-error: true`, same
  summary and artifact). Locally, a `G-ESCTEST` gate in `.port/tools/ci.sh` that runs both harnesses
  (the oracle's in a hand-made worktree, never moving the oracle's HEAD) and goes RED on any test that
  Ghostty passes and Rustty fails. That differential is the parity value; the absolute tally is not.
- Hard dependency: R1's DECRQCRA/XTCHECKSUM port (`xt_checksum`, `Terminal` option
  `default_xt_checksum`, `Handler.xt_checksum_report`). Without it most esctest checks cannot read the
  screen.

### 2.12 Benchmarks (#20)

`OscParser.zig` gains `--unknown-max-bytes`; `TerminalStream.zig` gains `--osc-unknown-max-bytes`,
which also installs a do-nothing `unknown_sequence` callback (`doNotOptimizeAway(content.len)`).
Rustty: `crates/rustty-bench/src/osc_parser.rs`, `terminal_stream.rs`. ABSENT. Port them; they are
the measurement for the #1 fixed-capture change.

## 3. Proposed workstreams

| WS | content | items | depends on | effort |
|---|---|---|---|---|
| W1 OSC parser | CAN/SUB in `end`; `S105`; unknown capture (`Unknown`, states, `unknown_or_invalid`, `capture_failed`, `const` prefix table, bridge arms, fixed-capture limit); `parsers/unknown.rs`; `parsers/program_status.rs` + shared base64; 30 parser tests; bench options; G-BENCH osc-parser before/after | #1, #4, #5 (parser), #13, #20 | none | M–L (about 1.5 d) |
| W2 stream + effects | `Stream` option `osc_unknown_max_bytes`; handler methods `osc_unknown`, `mouse_shape_reset`, `program_status`; `UnknownSequence::Osc`; `SemanticPrompt` type + effect gated on `Ok`; `program_status` effect + query reply; `reset` effect; RIS order; 5 stream_terminal tests; `pub use` of `SemanticPrompt` | #2, #6 (Zig), #7 (Zig), #14 | W1 | M (1 d) |
| W3 vt-c + headers | `rustty_osc_set` (clears the stored command; `c_int` narrowing), OSC option/terminator enums and data 2–4, `OSC_COMMAND_UNKNOWN/PROGRAM_STATUS`; terminal options 42, 43, 46 (44/45 with R1), data 42; structs + manifest rows; four trampolines with `LiveHandler`; `UNKNOWN_MAX_BYTES` → both limits by raw place; paste fix; sys and allocator and resize docs; UD312 closure; snapshot opt 2 / data 9; tests; Miri re-entry tests; A5 / G-ABISCHEMA / D11 / G-CENSUS re-baseline | #3, #6 (C), #7 (C), #8–#12, #15–#17 (C) | W2; R1 core halves for #15–#17 | M–L (about 1.5–2 d) |
| W4 examples + Windows | `c-vt-effects` unknown-OSC arm + multi-byte write; `c-vt-compression` memory usage; `c-vt-snapshot` compress; rwin run of `c-vt-effects` against `rustty_vt_c.dll`; CI step | #15, #16, #18 | W3 | S + one rwin lane |
| W5 esctest | `test/esctest` package, fetch script, CI report-only job, `G-ESCTEST` differential gate, MANIFEST TODO-2 closed | #19 | R1 DECRQCRA | M (1 d) |

Order: W1 → W2 → W3 (after R1's xt_checksum and memory/snapshot core land, or with 44/45 reserved) →
W4 → W5. W1 and R1's work are independent and can run in parallel worktrees.

## 4. Open questions

**Q1. Where does the esctest port live, and is it a gate?** Recommendation: `test/esctest/` (the
upstream path, per `test/README.md`'s PORTS mapping) as a unix-only workspace member, with a hash-pinned
fetch script and a report-only CI job as upstream. Add a local `G-ESCTEST` that compares Rustty's
failing set with the oracle harness's failing set at the pin. Parity means "fails exactly where
Ghostty fails", which the differential checks directly.

**Q2. Port `windows_dll.zig`?** Recommendation: no (NON-PORT, the bug is Zig's DLL entry point, not
Ghostty's code) **after** proving on rwin that `rustty_vt_c.dll` handles a multi-byte UTF-8 write.
Port the CI step from `a7cf951eb` against `rustty_vt_c.dll` so it stays proven. If rwin shows the
crash (for example on `-gnu`), port the constructor walk.

**Q3. How should the fixed unknown capture enforce its limit without slowing supported OSCs?**
Recommendation: compare `len < cap.max_bytes` in the per-byte fast path (`max_bytes == MAX_BUF` for
every supported fixed capture) and measure osc-parser and stream benches. Only if that regresses,
give the unknown capture its own `CaptureBacking` variant. Never call `State::tag_name()` there.

**Q4. UD312 now that upstream fixed it?** Recommendation: FIXED-SAME. Keep the entry, set its upstream
status to "fixed upstream in `51d3ca168`", remove the "reference defect" PORT NOTE, rename the test to
`command_data_null` and update `docs/ghostty-bug-fixes.md:123`.

**Q5. Who ports the core halves of `8834e161c` / `88e66cbc6` / `9272a2f7c`?** Recommendation: R1's
workstreams (PageList, Screen, pools, snapshot history, xt_checksum), merged before W3's C halves.
W3 can reserve option 44/45 and data 42 numbering so its own items need not wait.

**Q6. `rustty_osc_set` and the new option setters: check enum ranges only in debug builds, as
upstream does?** Recommendation: no. This is **UD113**'s class (`UPSTREAM-DEFECTS.md:1651-1663`:
libghostty-vt range-checks enum arguments only in safety-checked builds), and `c/osc.zig set` is a new
site of it. Add the site to UD113's reference list and keep Rustty's unconditional `c_int` narrowing.
No new UD id needed.

**Q7. Should `rustty_terminal_reset()` fire the new `reset` (and program-status `clear`) effects?**
Recommendation: no. Upstream fires them only for RIS from the stream and documents the effect as
"after the running program performs a full reset". The embedder that calls the C reset already knows.
Parity, no defect.

**UD candidates from this scope:** none new. Two upstream fixes land on Rustty records or behaviour
it already had: UD312 (FIXED-SAME), and `aca9bf031` (Rustty already zero-initialised, unrecorded).
`2b0ceff7d` fixes an unrecorded Rustty bug that W3 ports. UD113 gains one site.
