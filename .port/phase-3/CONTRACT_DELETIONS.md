# Contract entries UPSTREAM DELETED OR RENAMED — the other half of the frozen-pin drift

Read by `.port/tools/gates/check_tests_ported.py` (G1, name-set diff) and
`.port/tools/gates/check_assertions.py` (G2, crate assertion total) through the
shared loader `_gatelib.contract_deletions()`.

`.port/phase-0/inventory/` is the port's contract, captured at the ORIGINAL pin
`4749c4e93` and deliberately never regenerated (see `CENSUS_DELTA.md` for why —
rewriting it would restate the denominator of every historical completion claim).

`CENSUS_DELTA.md` records one direction of the resulting drift: the frozen
inventory **cannot see** the test blocks upstream has written since. This file
records the other direction, which the first full `ci.sh` run on merged `main`
surfaced: it **still demands** the tests upstream has since deleted — or renamed,
which the frozen name-set cannot tell apart from a deletion.

That is the same bidirectionality `P3-W10` found for G3b — an advance both adds
and removes reference material — and it is why "an advance takes the gate to zero
by construction" was wrong there too.

Porting a test that no longer exists upstream is not fidelity; it is the opposite.
The port followed upstream correctly in every row below.

## How this is kept honest

A row here does **not** get subtracted on trust. Every claim it makes is
re-derived at run time, and a row that fails any arm is **REJECTED** — the
exemption is refused, the gate prints `LEDGER-REJECTED` and stays RED:

1. the contract name is absent from the LIVE oracle tree (`../ghostty/src`),
   matched **exactly** — `test "<name>" {`, any indentation;
2. the named commit is an ancestor of the oracle's `HEAD`;
3. **that commit's own diff removed that exact declaration** — an arbitrary
   ancestor sha is not evidence;
4. for a RENAME row, each successor named in column 3 exists upstream today, was
   **added by that same commit**, and **resolves to a ported Rust fn** in the
   owning crate. A rename is excused only when the port demonstrably carries the
   renamed test.

So a bogus entry cannot hide a genuinely unported test: if the test still exists
upstream, (1) fails; if the port never carried the renamed successor, (4) fails.
A row that excuses nothing on a run is reported `LEDGER-STALE` and is RED too, so
exemptions cannot accumulate into a standing subtraction.

Arms 3 and 4 are new (2026-08-28). They immediately caught a wrong sha in the
original twelve — `os::args` cited `b5053153f`, *"terminal: log
unsupported-input messages once per distinct value"*, which never touched
`src/os/args.zig`; the deletion is `e8525c0fd`, the Zig 0.16 update that removed
the file. Arm 1 was also strengthened from a prefix match on a `": "`-split name:
as written it searched for `test "scrollUp with…"` when the declaration was
`test "Terminal: scrollUp with…"` — a string that never existed, so the arm
passed vacuously for every `X: Y` row — while `datastruct::get` collided with
every `test "getSize"` in the tree and could never be excused at all.

## The rows

Column 3 names the upstream successor(s) for a RENAME, or `—` for a deletion with
no successor. Verified 2026-07-31 (first twelve) and 2026-08-28 (the rest,
against pin `88f57ee66`).

### Deleted upstream — no successor

| contract entry | commit | successor | upstream change |
| --- | --- | --- | --- |
| `config::fromHex` | `2970e9a2a` | — | `Color.fromHex` folded into the color-utility C API |
| `os::appendEnv empty` | `e8525c0fd` | — | env handling moved to the captured `global.environ()` |
| `os::appendEnv existing` | `e8525c0fd` | — | ditto |
| `os::prependEnv empty` | `e8525c0fd` | — | ditto |
| `os::prependEnv existing` | `e8525c0fd` | — | ditto |
| `os::args` | `e8525c0fd` | — | `src/os/args.zig` deleted by the Zig 0.16 update |
| `vt::HashMap ensureUnusedCapacity with tombstones` | `fedd42e8d` | — | backward-shift deletion in page maps — tombstones no longer exist |
| `vt::PageList compact insufficient savings returns null` | `16e4b5e98` | — | page ownership tracked explicitly on nodes |
| `vt::PageList compact std_size page returns null` | `16e4b5e98` | — | ditto |
| `vt::Terminal: scrollUp with max_scrollback zero` | `86f81fb5b` | — | scrollback line limit exposed |
| `vt::Terminal: scrollUp with max_scrollback zero and left/right margin` | `86f81fb5b` | — | ditto |
| `vt::Terminal: scrollUp with max_scrollback zero and top margin` | `86f81fb5b` | — | ditto |
| `datastruct::SegmentedPool` | `e0ef934f7` | — | `SegmentedPool` replaced by `std.MemoryPool`; `datastruct/segmented_pool.zig` deleted, and `crates/rustty-datastruct/src/` has no counterpart file either |
| `datastruct::get` | `02f34835e` | — | *"datastruct: remove unused LRU implementation"* — `datastruct/lru.zig` deleted whole |
| `datastruct::getOrPut` | `02f34835e` | — | ditto |
| `datastruct::resize shrink and remove` | `02f34835e` | — | ditto |
| `datastruct::resize shrink without removal` | `02f34835e` | — | ditto |
| `lib::cutPrefix` | `1d6bc6829` | — | *"lib: remove cutPrefix implementation"* — `src/lib/string.zig` deleted; callers use `std.mem.cutPrefix` (the port's callers use a local `cut_prefix` helper for the same reason) |
| `vt::selectionString map allocation failure cleanup` | `f1948d505` | — | selection text is transferred into the string map rather than duplicated, so the failure path the test drove no longer exists |
| `vt-c::struct sizes are non-zero` | `9673a22b0` | — | the size smoke test was superseded by the manifest's exact `@sizeOf` assertions (`manifest describes the complete packed cell layout`) |

### Renamed upstream — the port carries the successor

| contract entry | commit | successor | upstream change |
| --- | --- | --- | --- |
| `lib::abi by removing a key` | `e77c2612e` | `stable values when removing a key` | *"lib: Zig enums have stable values"* renamed the test with the guarantee it states |
| `vt::size report csi_21_t title` | `38e891e6c` | `size report csi_21_t title disabled by default`, `size report csi_21_t title enabled` | *"terminal: require opt-in for title reports"* split the one test into the two arms of the new opt-in |
| `vt::storage: delete all placements and images` | `f8b40a023` | `storage: delete all visible placements and matching images` | *"terminal/kitty: fix various graphics deletion mismatches with spec"* — `d=a/A` limited to non-virtual placements intersecting the active screen |
| `vt-c::json parses` | `9673a22b0` | `manifest parses and is versioned` | *"libghostty: expand ABI type metadata"* — the JSON struct-layout export became a versioned manifest |
| `vt-c::mode_get and mode_set` | `cfc19e805` | `set and get mode` | *"libghostty: add configurable mode defaults, remove mode_set/get"* (ABI BREAKING) — modes moved onto the generic `set`/`get` with a `ModeConfig` |
| `vt-c::mode_get null` | `cfc19e805` | `set and get mode null` | the two null-receiver tests merged into one on the new API |
| `vt-c::mode_set null` | `cfc19e805` | `set and get mode null` | ditto |
| `vt-c::mode_get unknown mode` | `cfc19e805` | `set and get mode validate configuration` | the two unknown-mode tests merged into one on the new API |
| `vt-c::mode_set unknown mode` | `cfc19e805` | `set and get mode validate configuration` | ditto |
| `vt-c::render: colors get` | `b4079f00c` | `render: colors data get` | *"libghostty: add render state structured cursor read"* — the colors snapshot moved under `ghostty_render_state_get` |
| `vt-c::render: colors get invalid value` | `b4079f00c` | `render: aggregate get invalid value` | ditto |
| `vt-c::render: colors get supports truncated sized struct` | `b4079f00c` | `render: colors data get supports truncated sized struct` | ditto |

`e8525c0fd` is the same commit behind the `TERM_PROGRAM` / `containsUnemptyConstant`
port fix, and `fedd42e8d` behind the `getOrPutAdapted` consolidation recorded in
`SYMBOL_AUDIT.md` — independent evidence that the port tracked these changes and
the contract did not.
