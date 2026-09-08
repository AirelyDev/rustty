# Parity audit — src/input

Six shards audited the `src/input` tree of the oracle (`/Users/gael/projects/ghostty`, pin `20abdb50a`) against rustty. 17 oracle units total 13,073 lines; the shards read 11,431 of them fully (the uncovered remainder is `Binding.zig:1-1642`, part 1/3, which no shard in this area was assigned). Every unit read has a rustty counterpart: 14 `ported`, 3 `partial` (`Binding.zig` foldedCodepoint slow path, `command.zig` `Command.C`, `helpgen_actions.zig` markdown header), 1 `replaced` (`Set.deinit` → Drop), 0 missing.
Findings: **7 CONFIRMED, 2 DISPUTED, 6 REFUTED, 0 UNVERIFIED**. By corrected severity (confirmed + disputed): **1 high, 2 medium, 6 low**.
The single most important gap is **S103-F1**: `Command.C` is a plain (non-`repr(C)`) Rust struct of four `Vec<u8>` yet is handed across the C ABI as `rustty_command_s` (four `const char*`), so the macOS command palette strides 32-byte elements over 96-byte Rust structs — garbage strings or a crash.

## Coverage

| Oracle unit (path:lines) | rustty counterpart(s) | Status | Note |
| --- | --- | --- | --- |
| `src/input/Binding.zig:1643-1692` (Action.hashIncremental / equal / bindingSetEqual) | `crates/rustty-input/src/binding.rs:2836-3020` | ported | Tag + payload hashed; `equal` structural per-variant; hash fn differs (DefaultHasher vs Wyhash) but is internal to `Set`. |
| `src/input/Binding.zig:1694-1741` (Trigger, Trigger.Key, Trigger.C) | `binding.rs:3041-3134` | ported | Defaults match; `TriggerC` carries mods as `u32` (header int-wide enum slot); size/offsets frozen 12/0/4/8, bit layout identical. |
| `src/input/Binding.zig:1743-1840` (Trigger.parse) | `binding.rs:3141-3257`; `rustty-vt/src/input/key_mods.rs:30-36,285-300`; `key.rs:604-606,648` | ported | Identical ordering: bool mod fields → alias table → key-already-set → `+` → key tag names (175 verified) → single UTF-8 codepoint → W3C → catch_all → backwards-compat → InvalidFormat. |
| `src/input/Binding.zig:1842-1961` (backwards_compatible_keys) | `binding.rs:3434-3643` | ported | 111 entries, every key/value pair verified identical incl. `physical:*`. |
| `src/input/Binding.zig:1963-1993` (isKeyUnset / hash / hashIncremental) | `binding.rs:3259-3295` | ported | Internal hash only. |
| `src/input/Binding.zig:1995-2012` (foldedCodepoint) | `binding.rs:3400-3426` | partial | ASCII fast path exact; Unicode `case_folding_full` slow path not ported (see S101-F1). |
| `src/input/Binding.zig:2014-2045` (Trigger.equal / foldedEqual / bindingSetEqual) | `binding.rs:3297-3330`, `3927-3941` | ported | Mods compared in full incl. sides; folded `[3]u21` deep-compared. |
| `src/input/Binding.zig:2047-2060` (Trigger.cval) | `binding.rs:3332-3356` | ported | catch_all payload `undefined` in oracle, zeroed in rustty (never read). |
| `src/input/Binding.zig:2062-2080` (Trigger.format) | `binding.rs:3358-3385` | ported | Same modifier order and fallbacks. |
| `src/input/Binding.zig:2082-2145` (Set, HashMap/ReverseMap, ChainParent) | `binding.rs:3904-4065`, `4421-4475` | ported | `ArrayHashMapUnmanaged` → insertion-ordered `CtxArrayHashMap`; ChainParent raw pointers → `(set_path, key)` locator. |
| `src/input/Binding.zig:2147-2244` (Set.Value format + formatEntries) | `binding.rs:4153-4295`, `4314-4418` | ported | Rewind-to-pos prefix sharing reproduced; `leaf_chained` format arm is dead code in both. |
| `src/input/Binding.zig:2246-2321` (Leaf / LeafChained / GenericLeaf / Entry) | `binding.rs:4067-4150` | ported | `Entry` replaced by entry index (`get_index`/`value_at`/`key_at`). |
| `src/input/Binding.zig:2323-2341` (Set.deinit) | `binding.rs:4488-4491` | replaced | Ownership/Drop frees everything the oracle frees manually; no-op shim kept. |
| `src/input/Binding.zig:2343-2400` (Set.parseAndPut) | `binding.rs:4986-5057` | ported | Validate-then-reset-then-recurse; same error mapping; chain_parent path made root-relative. |
| `src/input/Binding.zig:2402-2526` (Set.parseAndPutRecurse) | `binding.rs:5068-5211` | ported | Leader recursion, empty-set removal, unbind restore, chain-only-at-root all mirrored. |
| `src/input/Binding.zig:2528-2617` (Set.put / putFlags) | `binding.rs:4560-4649` | ported | `track_reverse=!performable`, reverse-entry hash matching, `reverse.put` keeps old key. |
| `src/input/Binding.zig:2619-2678` (Set.appendChain) | `binding.rs:4830-4910` | ported | `NoChainParent`; leaf → chained conversion before `fixupReverseForAction`. |
| `src/input/Binding.zig:2680-2733` (Set.get / getTrigger / getEvent) | `binding.rs:4498-4537`, `4662-4720` | ported | Lookup order physical → utf8 → unshifted_codepoint → catch_all(+mods) → catch_all(−mods). |
| `src/input/Binding.zig:2735-2810` (Set.remove / removeExact / fixupReverseForAction) | `binding.rs:4740-4826` | ported | chain_parent reset, swapRemove, exact-`equal` fixup. |
| `src/input/Binding.zig:2812-2857` (Set.clone) | `binding.rs:4913-4945` | ported | Deep-clones leaves/chained/leaders/reverse keys; chain_parent null. |
| `src/input/Binding.zig:2859-2882` (Set.Context) | `binding.rs:3921-3951` | ported | hash truncated to u32; eql = bindingSetEqual. |
| `src/input/Binding.zig:2884-3302` (10 test blocks) | `binding.rs:5319-6022` | ported | All assertions present with identical expected values. |
| `src/input/Binding.zig:3285-4924` (78 test blocks) | `binding.rs:6025-8108` | ported | All 78 same-named counterparts, matching assertion counts; two low relaxations (S102-F1, S102-F2). |
| `src/input/KeymapDarwin.zig` (289) | `crates/rustty-input/src/keymap_darwin.rs` | ported | Keymap/Error/State/Translation, init/reload/sourceId/reinit/translate, CarbonMods + tests all equivalent; one error-tag difference (S102-F3). |
| `src/input/KeymapNoop.zig` (39) | `crates/rustty-input/src/keymap_noop.rs`; `lib.rs:48-58` | ported | Identical; `lib.rs` selects Darwin/Noop mirroring `input.zig`. |
| `src/input/Link.zig` (79) | `crates/rustty-input/src/link.rs` | ported | regex/action/highlight, `oniRegex`, clone, equal; `oni_regex` behind default-ON `oniguruma` feature for wasm only. |
| `src/input/command.zig` (768) | `crates/rustty-input/src/command.rs`; `rustty-config/src/value_types.rs:5943-6018`; `include/rustty-embed.h:423-428` | partial | All 88 `actionCommands` arms verified 1:1; `Command.C` is not a C-ABI struct (S103-F1). |
| `src/input/config.zig` (8) | `crates/rustty-input/src/config.rs`; `rustty-vt/src/input/config.rs` | ported | `OptionAsAlt` ported twice, both `repr(C)`, discriminants 0..3. |
| `src/input/function_keys.zig` (315) | `crates/rustty-vt/src/input/function_keys.rs` | ported | Every entry, mods, mode and sequence byte compared; `keys` test present with Small.Max 38 inlined. |
| `src/input/helpgen_actions.zig` (112) | `crates/rustty-input/src/helpgen_actions.rs`; `help_strings.rs` | partial | Format/formatFieldName/formatDocLine/generate ported; markdown header has one extra trailing newline (S103-F2). |
| `src/input/key.zig` (876) | `crates/rustty-vt/src/input/key.rs`; `rustty-apprt/src/embedded.rs:2944` | ported | All 176 Key tags in identical order; every helper + `codepoint_map` (64 entries) + all 4 tests present. |
| `src/input/key_encode.zig` (2602) | `crates/rustty-vt/src/input/key_encode.rs` | ported | Options/encode/kitty/legacy/ctrlSeq/CsiUMods/KittyMods/KittySequence; all 95 test blocks have counterparts; one alt-prefix byte difference (S104-F1). |
| `src/input/key_mods.zig` (913) | `crates/rustty-vt/src/input/key_mods.rs:25-366`; `crates/rustty-input/src/key_mods.rs:1-1425` | ported | Split across two crates for acyclicity; all 24 test blocks present; header bit layout matches. Padding-bit round-trip differs (S105-F2). |
| `src/input/keyboard.zig` (58) | `crates/rustty-input/src/keyboard.rs` | ported | Layout, mapAppleId, detectOptionAsAlt identical; no oracle tests. |
| `src/input/keycodes.zig` (673) | `crates/rustty-input/src/keycodes.rs` | ported | 120 `code_to_key` pairs and 243 `raw_entries` (all 6 columns) verified by script; extra wasm arm only (S105-F1, refuted). |
| `src/input/kitty.zig` (134) | `crates/rustty-vt/src/input/kitty.rs` | ported | 81 entries verified identical (key, code, final, modifier, order). |
| `src/input/mouse.zig` (110) | `crates/rustty-vt/src/input/mouse.rs` | ported | All enums + `ScrollMods` with test; header enums match. Padding-bit round-trip differs (S105-F3, refuted). |
| `src/input/mouse_encode.zig` (781) | `crates/rustty-vt/src/input/mouse_encode.rs` | ported | All functions and 23 test blocks present; stale `from_terminal` seam (S106-F1) and commented X10 log (S106-F2). |
| `src/input/paste.zig` (392) | `crates/rustty-vt/src/input/paste.rs` | ported | Prefix/suffix, max_frame_size, encode/encode_mut/encode_writer/is_safe; all 17 tests present. |

**Partially-read units:** none. Every shard reported `units_partial: []`; all listed units were read in full. The only in-scope text not covered by these shards is `src/input/Binding.zig:1-1642` (part 1/3), which was not assigned to any shard in this area.

## Findings

### High

#### S103-F1 — Command.C is not a C-ABI struct but is handed to C as `rustty_command_s` (command-palette-entry ABI mismatch)

- **Status:** ✅ CONFIRMED (both lenses failed to refute)
- **Kind:** behavioral-difference · **Severity:** high
- **Oracle:** `src/input/command.zig:23-29, 64-77, 121-128`
- **rustty:** `crates/rustty-input/src/command.rs:47-58, 130-149`

**Description.** The oracle's `Command.C` is `extern struct { action_key, action, title, description: [*:0]const u8 }` (`ghostty_command_s`, 4 pointers = 32 bytes) and `RepeatableCommand.cval()` returns `{ commands = value_c.items.ptr, len }`, so `ghostty_config_get(cfg, &list, "command-palette-entry")` hands the app an array of those C structs. The Rust `C` is `#[derive(Clone, Debug, PartialEq)] pub struct C { action_key: Vec<u8>, action: Vec<u8>, title: Vec<u8>, description: Vec<u8> }` — no `#[repr(C)]`, each field a 24-byte `Vec` header (96 bytes/element, unspecified field order). `RepeatableCommand.value_c: Vec<rustty_input::command::C>` (`value_types.rs:5947`) and `RepeatableCommandC { commands: *const rustty_input::command::C, len }` (`value_types.rs:5955-5973`) are written verbatim into the caller's buffer by `c_get.rs:306 g_cval!(command_palette_entry)`. The consumer `include/rustty-embed.h:423-428` declares `rustty_command_s` as four `const char*`, and `macos/Sources/RusttyKit/Rustty.Config.swift:714-723` builds `UnsafeBufferPointer<rustty_command_s>(start: v.commands, count: v.len)` then `String(cString: cValue.action_key)` (`Rustty.Command.swift:37`). Swift therefore strides 32 bytes over 96-byte Rust structs and dereferences `Vec` ptr/cap/len words as C strings: garbage titles or a crash for the macOS command palette (`TerminalCommandPalette.swift:130`) and App Intents (`CommandEntity.swift:94,122`).

**Evidence.** `command.zig:24-29` `pub const C = extern struct { action_key: [*:0]const u8, ... }` vs `command.rs:52-58` `pub struct C { pub action_key: Vec<u8>, ... }` (no `repr(C)`); `value_types.rs:5968-5973` `commands: self.value_c.as_ptr()`; `c_get.rs:190-198` `std::ptr::write(ptr_raw.cast(), cv)`; `rustty-embed.h:423-428`; `Rustty.Config.swift:719-721`.

**Suggested fix.** Make `C` a `#[repr(C)]` struct of four `*const c_char` (NUL-terminated) that borrow the owning `Command`/arena-allocated action string (as the oracle does), keep the owned NUL buffers alongside in `RepeatableCommand`, and add an A4-style size assertion `size_of::<C>() == 4 * size_of::<*const c_char>()` against `rustty-embed.h`.

### Medium

#### S101-F1 — Trigger.foldedCodepoint Unicode slow path (case_folding_full) not ported — non-ASCII triggers are not case-folded

- **Status:** ✅ CONFIRMED
- **Kind:** missing-branch · **Severity:** medium
- **Oracle:** `src/input/Binding.zig:1995-2012`
- **rustty:** `crates/rustty-input/src/binding.rs:3400-3412`

**Description.** The oracle folds a non-ASCII codepoint via `uucode.get(.case_folding_full, cp).with(&buffer, cp)` and copies the (1..3 codepoint) result into the `[3]u21` array; this feeds both `Trigger.hashIncremental` and `Trigger.foldedEqual`, i.e. the `Set`'s forward-map `Context`, so `ctrl+É` in a config matches a key event whose UTF-8 text is `é` (and vice versa), and Cyrillic/Greek/Latin-extended letters match case-insensitively. rustty's `folded_codepoint` returns `[cp, 0, 0]` for every non-ASCII codepoint (the PORT NOTE at 3388-3399 admits the table is not ported; no case-folding lookup exists anywhere in `crates/rustty-unicode`). Observable effect: `Set::get_event` (`binding.rs:4662`) step 2 (single-codepoint utf8) and step 3 (unshifted_codepoint) fail to match a Unicode trigger whose config spelling differs in case from the event's text (e.g. keybind `ctrl+É=...` never fires for a ctrl+é keypress; `ß`/`ẞ` pairs, Greek final sigma, etc.). The `unshifted_codepoint` fallback masks the common lowercase-config/uppercase-event case, but not the reverse nor codepoints whose folding is not the simple unshifted form.

**Evidence.** Oracle 2003-2010: `var buffer: [1]u21 = undefined; const slice = uucode.get(.case_folding_full, cp).with(&buffer, cp); var array: [3]u21 = [_]u21{0} ** 3; @memcpy(array[0..slice.len], slice); return array;`. rustty 3400-3412: `if ascii_is_alphabetic(cp) { return [ascii_to_lower(cp), 0, 0]; } ... [cp, 0, 0]` with PORT NOTE 3392-3394. No `case_folding` symbol exists under `crates/rustty-unicode/src`.

**Suggested fix.** Port the Unicode `CaseFolding.txt` (status C+F) table into rustty-unicode (e.g. a `case_folding_full(cp) -> &[u21]` accessor generated alongside the existing uucode-derived tables) and use it in `folded_codepoint`, copying up to 3 codepoints exactly as the oracle does; add a test with a non-ASCII pair through `Set::get_event` / `Trigger::folded_equal`.

#### S104-F1 — Alt-ESC prefix writes the byte UTF-8-encoded instead of raw for bytes 0x80..=0xFF

- **Status:** ✅ CONFIRMED
- **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/input/key_encode.zig:407-412, 528-534`
- **rustty:** `crates/rustty-vt/src/input/key_encode.rs:483-488, 637-641`

**Description.** The oracle emits the alt-prefixed byte with `writer.print("\x1B{c}", .{byte})`; Zig's `{c}` (`printAsciiChar`) writes the single raw `u8`. The port does `write!(writer, "\x1B{}", byte as char)`, which converts the `u8` to a Unicode scalar via Latin-1 and Display-encodes it as UTF-8. For `byte < 0x80` output is identical; for `0x80..=0xFF` the oracle writes ESC + 1 byte while rustty writes ESC + 2 bytes (e.g. `0xFC` → `C3 BC`). Reachable: `legacy_alt_prefix` returns `u8::try_from(event.unshifted_codepoint)` for any unshifted codepoint ≤ 0xFF (Latin-1 letters such as `ü`/`é` on German/French layouts) when utf8 is empty or multi-byte, and also returns `utf8[0]` for a single raw byte ≥ 0x80. With `alt_esc_prefix` on (and `macos_option_as_alt` set on macOS) alt+ü encodes as `ESC FC` in ghostty but `ESC C3 BC` in rustty. Both call sites are affected. The nearby C0 path (`key_encode.rs:471-474`) already does the correct raw `write_all`, confirming these two sites are the anomaly.

**Evidence.** Oracle `key_encode.zig:411` `try writer.print("\x1B{c}", .{byte});` and `:533` same; rustty `key_encode.rs:486` / `:640` `write!(writer, "\x1B{}", byte as char)`. Zig std `Io/Writer.zig` maps `{c}` to `printAsciiChar`, which writes `@as(*const [1]u8, &c)` verbatim.

**Suggested fix.** Replace both sites with `writer.write_all(&[0x1B, byte])`.

### Low

#### S102-F1 — parse_chain: first `p.next()` result is discarded instead of `try`-checked

- **Status:** ✅ CONFIRMED
- **Kind:** weakened-assertion · **Severity:** low
- **Oracle:** `src/input/Binding.zig:3484-3489`
- **rustty:** `crates/rustty-input/src/binding.rs:6280-6286` (specifically `:6283`)

**Description.** In the "Chain can't be part of a sequence" block the oracle does `_ = try p.next();`, so an unexpected error from the leader element fails the test; the port does `let _ = p.next();`, silently discarding an `Err`. The following `assert_eq!(Err(Error::InvalidFormat), p.next())` still runs, so the port would pass even if the first `next()` already returned `InvalidFormat` (i.e. the leader was not parsed as a leader).

**Evidence.** Oracle 3487 `_ = try p.next();` vs rustty 6283 `let _ = p.next();`.

**Suggested fix.** Replace with `p.next().unwrap();` (or `assert!(p.next().is_ok())`).

#### S103-F2 — Markdown header ends with three newlines instead of two

- **Status:** ✅ CONFIRMED
- **Kind:** different-constant · **Severity:** low
- **Oracle:** `src/input/helpgen_actions.zig:45-58`
- **rustty:** `crates/rustty-input/src/helpgen_actions.rs:66-80`

**Description.** The Zig multiline literal has 9 `\\` lines (the last two empty) joined by 8 newlines, so the header ends `...keybinding actions.\n\n`. The Rust byte string ends with three `\n`, i.e. one extra blank line at the top of every generated markdown actions reference (`webgen.rs:168/308` callers).

**Evidence.** `helpgen_actions.zig:48-56` (Zig omits the newline of the final line ⇒ 2 trailing `\n`) vs `helpgen_actions.rs:76-79` (`\n\` continuations emit 3 trailing `\n`).

**Suggested fix.** Drop one `\n` so the header ends with exactly two newlines.

#### S103-F3 — `w3c` test iterates the hand-maintained TAG_NAMES table rather than every enum field, and the doc-claimed completeness test does not exist

- **Status:** ✅ CONFIRMED
- **Kind:** weakened-assertion · **Severity:** low
- **Oracle:** `src/input/key.zig:731-741` (test at `:768-778`)
- **rustty:** `crates/rustty-vt/src/input/key.rs:1417-1426, 583-588`

**Description.** The oracle test uses `inline for (@typeInfo(Key).@"enum".fields)`, so every `Key` variant is round-tripped by construction. The Rust test iterates `TAG_NAMES`; a variant missing from `TAG_NAMES` would be silently skipped (and `tag_index` would hit `unreachable!()` at runtime). `key.rs:587` claims "the `key: every tag has a name` test already pins the table against the variant set", but no such test exists in the workspace (the only `#[test]` fns in `key.rs` are `from_ascii_should_not_return_keypad_keys`, `keypad_keys`, `w3c`, `ctrl_or_super_test`). Today `TAG_NAMES` does contain all 176 variants in oracle order, so there is no current behavioural gap; this is test strength only.

**Evidence.** `key.zig:736` `inline for (@typeInfo(Key).@"enum".fields) |field|` vs `key.rs:1421` `for (key, _) in TAG_NAMES`; `key.rs:587` references a non-existent test.

**Suggested fix.** Add a test asserting `TAG_NAMES.len() == <variant count>` and that each index `i` has `TAG_NAMES[i].0 as usize == i`, or derive the table with a macro.

#### S106-F2 — X10 coordinate-overflow `log.info` line is a no-op

- **Status:** ✅ CONFIRMED
- **Kind:** other · **Severity:** low
- **Oracle:** `src/input/mouse_encode.zig:123-126`
- **rustty:** `crates/rustty-vt/src/input/mouse_encode.rs:222-225`

**Description.** The oracle logs `X10 mouse format can only encode X/Y up to 223` at info level before returning when `cell.x`/`cell.y` > 222. The port keeps the early return but the log call is a comment because rustty-vt has no logging facade (no `log`/`tracing` dependency in `crates/rustty-vt/Cargo.toml`; `stream.rs` applies the same treatment). Control flow is identical; observable only as a missing log line. Workspace-wide grep for the literal finds only the comment — no relocated emission.

**Evidence.** Oracle `mouse_encode.zig:124` `log.info("X10 mouse format can only encode X/Y up to 223", .{});` vs rustty `mouse_encode.rs:223` `// log.info(...)`.

**Suggested fix.** Wire a crate-level log facade (or the `log` crate) and emit the info line; this is a crate-wide gap, not specific to this file.

#### S105-F2 — Mods drops the `_padding: u6` bits: `from_int()`/`int()` do not round-trip bits 10..15 as the Zig `@bitCast` does

- **Status:** ⚠️ DISPUTED (semantic lens refuted; presence lens could not)
- **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/input/key_mods.zig:29-40, 81-83, 116-118, 464-468`
- **rustty:** `crates/rustty-vt/src/input/key_mods.rs:82-91, 211-219, 260-262, 319-351`

**Description.** The oracle `Mods` is `packed struct(u16)` with an explicit `_padding: u6 = 0` field, so `@bitCast(u16) -> Mods` preserves bits 10..15 and `int()`/`equal()`/`empty()` observe them (`unset` and `RemapSet.apply` `@bitCast` and keep any padding bits present in the input). rustty's `Mods` omits the padding field; `Mods::from_int(v)` masks to bits 0..9 and `int()` can never emit bits 10..15, so `Mods::from_int(0x8001).int() == 0x0001` whereas Zig yields `0x8001`, and `empty()`/`equal()` differ for such values. Only observable if a host passes undefined high bits through the C ABI `ghostty_input_mods_e` (all defined flags are bits 0..9), hence low severity.

**Evidence.** `key_mods.zig:39` `_padding: u6 = 0,`; `:81-83` `int` is `@bitCast(self)`; `:116-118` `unset` returns `@bitCast(self.int() & ~other.int())`; `:468` `return @bitCast(mods_int)`. rustty `key_mods.rs:90` `// _padding: u6 = 0,` (field absent), `:211-219` `int()` ORs only bits 0..9, `:319-351` `from_int` reads only bits 0..9. Presence lens additionally traced the egress: `crates/rustty-apprt-c/src/lib.rs:1749-1782` returns `result.int() as c_int` whereas `apprt/embedded.zig:2019-2027` round-trips the padding bits back to the host.

**Refuting lens — semantic (refuted, medium confidence), verbatim:** "Confirmed the mechanical difference: key_mods.zig:39 declares `_padding: u6 = 0` inside packed struct(u16) so @bitCast round-trips bits 10..15, while key_mods.rs:82-91 omits the field and int() (rs:210-219) / from_int (rs:318-351) only touch bits 0..9. But `_padding` is an explicitly reserved-zero field: every producer in the oracle (Mods literals, binding() at key_mods.zig:105-112 which uses a struct literal that re-defaults padding to 0, RemapSet.apply at 455-468 which ORs only real mod bits) yields padding==0, and no consumer ever reads it. The only divergent input is a host passing undefined high bits through ghostty_input_mods_e (embedded.zig:2019-2021, 2337), where the oracle would merely echo the garbage back and rustty zeroes it; the C header defines only bits 0..9, so this is out-of-contract input, not an observable behavioural difference. Refuted as inconclusive/theoretical."

**Suggested fix.** Carry a 6-bit `padding` field in `Mods` populated by `from_int` from bits 10..15 and emitted by `int()`, or document the truncation as intentional at the C-ABI ingress.

#### S106-F1 — `Options.from_terminal` is a stale seam: takes `(event, format, size)` instead of `(&Terminal, size)`

- **Status:** ⚠️ DISPUTED (semantic lens refuted; presence lens could not)
- **Kind:** other · **Severity:** low
- **Oracle:** `src/input/mouse_encode.zig:39-50`
- **rustty:** `crates/rustty-vt/src/input/mouse_encode.rs:90-112`

**Description.** The oracle `Options.fromTerminal(t: *const Terminal, size)` reads `t.flags.mouse_event` / `t.flags.mouse_format` itself. The rustty port's PORT NOTE says "Terminal lands in vt-12; until the flags struct is wired, this constructor takes the already-read event/format values directly (the seam)". `crate::terminal::Terminal` now exists with `flags.mouse_event`/`flags.mouse_format` (`crates/rustty-vt/src/terminal.rs:347-348`) and the sibling `key_encode.rs:105 from_terminal(t: &Terminal)` already takes the terminal, so the seam is stale. The only caller (`crates/rustty-core/src/surface.rs:8140-8144`) reads the same two flags and passes them in, so there is no behavioural difference today, but the public API shape diverges from the oracle and every future caller must replicate the flag reads.

**Evidence.** Oracle `mouse_encode.zig:41-49` returns `.event = t.flags.mouse_event, .format = t.flags.mouse_format`; rustty `mouse_encode.rs:100-112` `pub fn from_terminal(event, format, size)`; `terminal.rs:347-348` has the flags.

**Refuting lens — semantic (refuted, medium confidence), verbatim:** "Read oracle src/input/mouse_encode.zig:39-50 (fromTerminal reads t.flags.mouse_event/mouse_format) and rustty crates/rustty-vt/src/input/mouse_encode.rs:90-112. The seam note is indeed stale (crates/rustty-vt/src/terminal.rs:347-348 exposes flags.mouse_event/mouse_format, and the sibling crates/rustty-vt/src/input/key_encode.rs:105 takes &Terminal), but no observable behaviour differs: the sole caller crates/rustty-core/src/surface.rs:8140-8144 passes exactly self.io.terminal.flags.mouse_event/mouse_format and self.size, and the C-API path never uses Options::fromTerminal on either side (oracle setopt_from_terminal reads the flags directly; mirrored at crates/rustty-vt-c/src/mouse_encode.rs:445-464). Same values, same result, same defaults (any_button_pressed=false, last_cell=None). API-shape/tech-debt nit only, not a functional-parity difference."

**Suggested fix.** Change the signature to `from_terminal(t: &crate::terminal::Terminal, size: renderer_size::Size)` reading the two flags, update `surface.rs:8140`, and drop the seam PORT NOTE.

### Unverified (over the verification cap)

None — every finding in this area was verified by both lenses.

## Performance-only differences (not findings)

- `Set` maps: the oracle uses `std.ArrayHashMapUnmanaged` with an index hash table (`store_hash=true`); rustty uses `CtxArrayHashMap`, an insertion-ordered `Vec<(K, u32 stored hash, V)>` with a linear hash-then-eql scan (`binding.rs:3960-4065`). Same insertion order and `getOrPut`/`put`/`swapRemove`/`swapRemoveAt` semantics; only lookup cost differs.
- Hash functions: the oracle uses `Wyhash(0)` with `autoHash`/`autoHashStrat(.DeepRecursive)`; rustty uses `DefaultHasher` over tag *names* plus payload bytes (`binding.rs:2853-2965, 3274-3295, 4082-4088`). Hashes are internal to the `Set` contexts and reverse-map matching, never exposed.
- `ChainParent`: the oracle stores raw `key_ptr`/`value_ptr`/`set` pointers; rustty stores a `(set_path, key)` locator re-resolved on each use (`binding.rs:4462-4465, 4544-4555`). Same observable state transitions. Test helpers compare the record structurally instead of by pointer identity.
- `RewindBuffer` is heap-backed (`Vec<u8>`) but capped at the oracle's 1024-byte fixed buffer, so the OutOfMemory boundary is identical (`binding.rs:4328-4369`).
- `KeymapDarwin.translate`: the oracle resolves the Space keycode at comptime (`@compileError` if absent); the port scans `keycodes::entries()` at runtime per call (the `None => TranslateFailed` arm at `keymap_darwin.rs:216` is unreachable).
- `Link.oni_regex` is gated behind the default-ON `oniguruma` cargo feature purely to allow a wasm32 build; every native build compiles it identically to the oracle's unconditional import.
- `function_keys.rs`: the `keys` table is built once via `LazyLock` at runtime instead of comptime; `KeyEntryArray` is a `Vec<(Key, Vec<Entry>)>` scanned linearly instead of `std.EnumArray` O(1). Identical entries/order, empty slice for unset keys.
- `command.rs`: `defaults()`/`defaults_c()` rebuild the sorted `Vec` on each call instead of a comptime constant (all titles unique, so unstable-sort ties do not arise). `helpgen_actions::generate` re-creates the `AllocatingWriter` instead of `clearRetainingCapacity`.
- `key.rs`: `KeyEvent.binding_hash` uses `DefaultHasher` (SipHash) instead of `Wyhash(0)`+`autoHash`; only compared against `keyboard.last_trigger` in-process. `from_w3c` writes into a growable `Vec` instead of a 128-byte fixed buffer (same result). `w3c()` returns a precomputed `W3C_NAMES` table (verified equal for all 176 tags); `printable`/`codepoint`/`from_ascii` scan `CODEPOINT_MAP` at runtime.
- `key.rs`: `Key::imgui_key` is feature-gated (`imgui`, on for gtk); the embedded/macOS inspector uses a duplicate `key_to_imgui_key` in `rustty-apprt/src/embedded.rs:2944` with the same mapping (the `unimplemented!()` comment at `embedded.rs:2806-2814` is stale).
- `config.zig`: `OptionAsAlt` is duplicated in rustty-vt and rustty-input; both `repr(C)` with matching discriminants.
- `key_encode.rs`: `pc_style_function_key` returns `Option<String>` cloned from the static table; `KittySequence.text` is a cloned `Vec<u8>`; `KittyMods::seq_int` returns `u16` (oracle `u9`) and `CsiUMods::seq_int` `u8` (oracle `u4`). Output bytes identical. `copy_to_buf` is ported but unused, as in the oracle.
- `keycodes.rs`: `entries()` rebuilds an owned `Vec<Entry>` per call (oracle is a comptime slice); `code_to_key_get` is a linear scan over the same 120 pairs instead of `std.StaticStringMap`.
- `kitty.rs`: `entries` built once via `LazyLock` instead of comptime; identical data.
- `RemapSet.finalize`: Zig's `ArrayHashMap.sort` is `std.mem.sortContext` (insertion sort); rustty's `AutoArrayHashMapUnmanaged::sort` wraps the same predicate in `sort_by`, which for ≤20 elements is also insertion sort. A `RemapSet` holds ≤8 keys, so the order is identical and the non-total comparator cannot trigger Rust's total-order panic path.
- `RemapSet.formatEntry`: Zig uses a fixed 64-byte stack buffer; rustty a growable `Vec<u8>`. The longest possible string is 23 bytes, so the OutOfMemory path is unreachable in both.
- `paste.rs`: `encode_writer` stages each chunk through a local 512-byte buffer then `write_all`s it (one extra copy per chunk; output identical because the transform is byte-position independent). `index_of_any`/`index_of_scalar`/`index_of`/`replace_scalar` are scalar loops in place of `std.mem`'s vectorized helpers.
- `mouse_encode.rs`: `should_report`/`button_code`/`pos_*` take `&Options` rather than by value because `Options` holds a `&mut Option<Coordinate>` and cannot be `Copy`.

## Refuted candidates

Checked and cleared — both verification lenses refuted these:

- **S102-F2 — `set_clone_with_leaf_chained_containing_allocated_data` omits the pointer-inequality assertion.** The oracle's `.text.ptr != ...` guard against a shallow copy is unfalsifiable in the port: `Action::Text` holds an owned `Vec<u8>` and `clone_set` builds each value via `clone_in`, so aliasing is impossible without `unsafe`; byte-equality asserts plus the type system cover the property.
- **S102-F3 — `translate` maps UTF-16 decode failure to `Error::TranslateFailed` instead of propagating the unicode error.** Only the error tag differs on an already-failing path; no production caller of `Keymap.translate` exists at pin `20abdb50a` (only commented-out test bodies), and the input is UCKeyTranslate's own well-formed UTF-16.
- **S105-F1 — `native_of()` has a wasm arm returning 0 where the oracle is `@compileError("unsupported platform")`.** The three native columns match exactly and `compile_error!` is preserved for every non-wasm unknown target; on wasm the oracle has no compilable behaviour to diverge from, and the divergence is documented in a PORT NOTE.
- **S105-F3 — `ScrollMods` drops the `_padding: u4` bits in `from_int()`/`int()`.** `ScrollMods` is never converted back to an integer, compared with `equal()`, or returned to a host on either side — it is consumed field-wise (`precision`/`momentum`) everywhere, so the padding bits are unobservable.
- **S106-F3 — "encodeWriter chunks through a small writer buffer" no longer exercises the multi-chunk loop.** The port reproduces the same 4-byte-staging `Sink`, the same input and the same single assertion; the 512-vs-4 internal chunk size is behaviour-neutral because the transform is position-independent. Coverage observation, not an assertion loss.
- **S106-F4 — `posToPixels`: `@intFromFloat` panics on out-of-range/NaN where Rust `as i32` saturates.** Divergence exists only where the oracle's behaviour is a safety panic (or UB in ReleaseFast) — NaN/inf or |v| > 2³¹ — and no such value is reachable: `Pos` is `f32` fed through `Coordinate::convert`'s Terminal arm, a padding subtraction with no division.
