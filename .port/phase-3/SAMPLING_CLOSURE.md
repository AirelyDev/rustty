# Sampling closure — the ten declared-sampled coverage gaps

`P3-W0 T4`. Each item below is an assessment author's own statement that they did
**not** have the coverage their conclusion might appear to claim. Treating
sampling as coverage is how `f2a7652ab` would have smuggled `omitFramePointer`
past this program, so each is closed here with a per-hunk verdict rather than an
argument.

Every command in this file is reproducible; the oracle is read with `git show` /
`git diff` only and is never checked out.

---

## 0. The filter itself — NEGATIVE CONTROL (step 6)

A residue filter that swallows a constant change is worse than no filter.

```sh
printf -- '-    const threshold = 1024;\n+    const threshold = 512;\n' >> /tmp/a_016.diff
grep '^[+-]' /tmp/a_016.diff | grep -vE 'std\.|posix\.|windows\.|@(Type|Struct|Enum|Union|Int|Fn)|ArrayList|Io\.|testing\.io|\.data\b' | tail -2
```

Output:

```
-    const threshold = 1024;
+    const threshold = 512;
```

Both synthetic lines survive. The filter is not hiding constant changes.
`/tmp/a_016.diff` was restored from `/tmp/a_016.diff.orig` afterwards.

---

## 1. `e8525c0fd`'s ~2,000 unsampled lines in area A's three big files (step 1)

```
src/terminal/PageList.zig + Screen.zig + Terminal.zig
total changed lines: 2,724
after the plan's mechanical filter: 1,710
```

The 1,710 are dominated by two classes that the plan's filter does not name.
Both are subtracted here **explicitly**, so the subtraction is auditable:

| class | shape | count |
|---|---|---|
| `Io` threading in tests | `init(alloc, …)` -> `init(io, alloc, …)` / `init(io_impl, alloc, …)` | ~1,100 |
| codepoint field form | `content.codepoint` under the new `packed struct(u24){data,_pad}` | ~270 |

That leaves **339 lines, 316 of them code**. Every one was read in context. They
fall into exactly seven mechanical classes, and **none is a changed constant, a
changed control-flow decision, or a changed error-handling arm**:

1. `Io` threading proper — `self.io()`, `.io = io`, `old.io`, `alt.io`,
   `return self.screens.active.io`.
2. Codepoint construction — `.content = .{ .codepoint = 'x' }` becomes
   `.{ .codepoint = @bitCast(@as(u24, 'x')) }`. Same values.
3. `pool.pages.arena.child_allocator` -> `pool.pages.allocator`
   (std `MemoryPool`/`ArenaAllocator` API rename).
4. `initPreheated` -> `initCapacity` (std `MemoryPool` API rename), same
   `preheat` argument.
5. The arena `BufNode` walk in the page-decommit path rewritten against the new
   std arena internals: `unmanaged.arena_state.used_list` / `free_list`,
   `BufNode.Size` as a `packed struct(usize)` with a `resizing` flag and
   `toInt()`, `@atomicLoad(..., .monotonic)` instead of a plain field read, and
   `@ptrCast(node)` instead of `@fieldParentPtr("node", node)`. Structure
   tracking, not policy: the same nodes are zeroed with the same `@memset`.
6. The grapheme/codepoint test-vector array reflowed onto different lines. Set
   equality checked: `'é' 0xFF 0x301 0x4E00 0x4E01 0x1F600 0x200D 0xFE0F 'x' 'y'
   0x1F9D1 0x0308 0xAD 0x3042 0xAC00 'q' 'r' 's' 't' 'u' 'v' 'w' '1' '2'
   0x1F1E6 0x1F1E7 0x1100 0x1161 0x11A8 0x200C 0x0430 0x03B1 'a' 'b' 'Z' '0' ' '
   0x10 0x1F 0x7F` — identical on both sides, only the wrapping moved. The word
   boundary character table (`0 ' ' '\t' '\'' '"' '│' '`' '|' ':' ';' ',' '(' ')'
   '[' ']' '{' '}' '<' '>' '$'`) likewise.
7. Writer API — `stream.getWritten()` -> `writer.buffered()`,
   `stream.writer()` dropped.

**VERDICT: all mechanical.** Area A's `page.zig` finding generalizes to the
other three files. No action.

---

## 2. `src/build/SharedDeps.zig` + `GhosttyLibVt.zig`, line by line (step 2)

Read in full, both commits, 33 + 283 changed lines. Justified because
`f2a7652ab` **proved** the hazard. Expected outcome was "nothing hiding"; that
is **not** what the reading found. Two items:

### 2a. ACTION — `GhosttyLibVt.zig` gains `LibsystemOverrideStep` (`f2a7652ab`)

Both the fat archive (`libghostty-vt-fat.a`) and the plain static archive
(`libghostty-vt-static.a`) are now post-processed on Darwin so that **libSystem's
libc/libm win over the bundled compiler-rt** for consumers of the archive. It is
a no-op off Darwin. This is a real change to what the published static library
exposes at link time, not a build-system refactor.

Owner: **`P3-W9`** (packaging). Trigger: rustty publishes a static
`librustty_vt` archive; the same duplicate-symbol/override question applies to
Rust's `compiler_builtins`.

### 2b. ACTION — `terminalOptions` becomes optimize-mode dependent (`e8525c0fd`)

```
-    self.config.terminalOptions(.ghostty).add(b, step.root_module);
+    self.config.terminalOptions(.ghostty, optimize).add(b, step.root_module);
```

A build option set now varies with the optimize mode. This is precisely the
`omitFramePointer` shape the plan warns about, in a *different* function.

Owner: **`P3-W9`** (config / build). Trigger: re-vetting
`Config.terminalOptions`; the option values themselves live in
`src/build/Config.zig`, outside the two files this step covers, so this item is
a pointer, not a closure.

### Everything else in these two files is mechanical

Zig 0.16 build-API migration (`step.linkX` -> `step.root_module.linkX`,
`linkSystemLibrary2` -> `linkSystemLibrary`, `linkLibC()` ->
`root_module.link_libc = true`, `linkFramework("OpenGL")` gains an options
argument, `captureStdOut()` gains an options argument,
`std.fs.accessAbsolute` -> `std.Io.Dir.accessAbsolute`,
`LibCInstallation.findNative` gains explicit allocator/io/environ arguments);
`@cImport` replaced by the `translate_c` dependency for GTK/Adwaita/gio and the
generated `ghostty_resources.h`; and the `uucode` module hoisted to a
singleton (`uucode_mod`) because Zig 0.16's module model rejects the same file
belonging to two modules in one Compile graph. The removed `addUucode` helper is
the counterpart of that hoist.

---

## 3. Area F's mechanical font files and the golden PNG pairs (step 3)

### 3a. Source residue

```
git -C ../ghostty diff 4749c4e93..origin/main -- src/font | grep '^[+-]' | grep -vE '…'
816 residue lines
```

Distilled to unique code lines, the classes are: `Io` threading in tests
(`terminal.Terminal.init(io, alloc, …)`), the lock migration
(`self.lock.lock()` -> `lockUncancelable(global.io())`,
`lockShared()` -> `lockSharedUncancelable(global.io())`), the reader migration
(`fbs.reader()` -> `compat_reader`), and new `global.zig` imports.

One class is **not** mechanical and is recorded as an ACTION:

### 3b. ACTION — sprite cursor glyph metrics

```
+        try testing.expectEqual(20, glyph.height);
+        try testing.expectEqual(18, glyph.offset_y);
+        try testing.expectEqual(16, glyph.offset_y);
+        try testing.expectEqual(16, glyph.height);
+        try testing.expectEqual(14, glyph.offset_y);
+        try testing.expectEqual(12, glyph.height);
+        const glyph = try face.renderGlyph(alloc, &atlas, @intFromEnum(Sprite.cursor_rect), .{ .grid_metrics = face.metrics });
+        … cursor_hollow_rect … cursor_bar …
```

`renderGlyph` for the three cursor sprites now takes `.{ .grid_metrics = … }`
and the asserted heights/offsets are new values. Owner: **`P3-W8`** (UI tiers).

### 3c. Golden PNG pixel decode — 37 pairs, all of them

The plan asked for the remaining 21; all **37** changed PNGs in the range were
decoded instead. Method: `git show <rev>:<path>` into Pillow, convert to `L`,
compare `tobytes()`.

**A first attempt at this sweep reported `0 / 37` pixel differences and was
WRONG.** It used `ImageChops.difference(a.convert("RGBA"), b.convert("RGBA"))
.getbbox()`, and `getbbox()` on an RGBA image considers only the **alpha**
channel — which is uniformly opaque in every one of these files, so the bbox is
always `None`. The comparator was vacuous in exactly the way this whole task is
about. The corrected script (`/tmp/pngdiff2.py`) therefore opens with a
**self-check that asserts a known-different pair is detected** before it will
report any result.

Corrected result:

| | count |
|---|---|
| byte-only churn, pixels identical | **31** |
| genuine pixel differences | **6** |

The six:

```
U+2500...U+25FF-11x21+2.png   L(240,496)    3 px differ, max delta 16, bbox (123,472,154,489)
U+2500...U+25FF-9x17+1.png    L(208,400)    4 px differ, max delta 16, bbox (107,381,134,394)
U+E000...U+E0FF-11x21+2.png   L(240,496)   76 px differ, max delta 48, bbox (78,346,117,367)
U+E000...U+E0FF-12x24+3.png   L(288,576)   76 px differ, max delta 64, bbox (94,402,140,426)
U+E000...U+E0FF-18x36+4.png   L(416,864)  120 px differ, max delta 48, bbox (135,603,203,639)
U+E000...U+E0FF-9x17+1.png    L(208,400)   60 px differ, max delta 48, bbox (68,279,101,296)
```

This is **exactly** area F's predicted set — `U+2500…{11x21+2, 9x17+1}` and
`U+E000…{11x21+2, 12x24+3, 18x36+4, 9x17+1}`, the six files in `d0e72a3ab`'s
stat. **No seventh file differs in pixels.** Area F's structural argument that
`d0e72a3ab` is the only rendering change in the range **holds**; nothing to
escalate. The other 31 are `z2d.png_exporter` output churn, confirmed
pixel-identical rather than assumed to be.

(`d0e72a3ab` was independently confirmed to be an ancestor of `origin/main` and
**not** of the pin `4749c4e93`, and to change these pixels against its own
parent — so the diff being non-empty is attributable to it.)

---

## 4. The two "wholly absent" benchmarks (step 4) — **THE PREMISE IS STALE**

Area B's finding, carried into the plan, is that `benchmark/HyperlinkMap.zig`
and `benchmark/ApcParser.zig` are wholly absent from rustty and that
`crates/rustty-bench/src/cli.rs` has 9 `Action` variants against upstream's 11.

That was true when area B was written. It is **not true on `main`**:

```
crates/rustty-bench/src/hyperlink_map.rs   354 lines   b8d24a6 "bench: HyperlinkMap benchmark + hyperlink-map action (WS10 T11)"
crates/rustty-bench/src/apc_parser.rs      255 lines
crates/rustty-bench/src/cli.rs             11 Action variants, including HyperlinkMap and ApcParser
```

WS10 T11 ported both after the assessment was written. **`P3-W5 T8/T9` and
`P3-W0 T7 step 2`'s "RED today" expectation are both based on a closed gap** —
see `REVIEW_CLASSES.md` for how T7 step 2 was re-derived.

Both oracle files were still read end to end for this closure:

* `HyperlinkMap.zig` (155 lines) — fixed-capacity open-addressed hyperlink cell
  map; `entries` must be a power of two >= 16; `set_entries = entries / 16` is
  the page's `hyperlink_bytes` budget (one map slot per
  `hyperlink_cell_multiplier` set entries); `load-percent` clamped to 100;
  `entry_count = max(1, entries * load / 100)`; two modes, `lookup` (every
  populated cell must resolve, `orelse return error.BenchmarkFailed`) and
  `churn` (clear, `hyperlink_set.use`, re-`setHyperlink`); a capacity
  pre-check that errors `UnexpectedCapacity`; the test drives both modes at
  `entries = 64`.
* `ApcParser.zig` (144 lines) — parse-only APC throughput through the real
  `Stream`; the handler dispatches `apc_start`/`apc_put`/`apc_put_slice`/
  `apc_end`, and completed commands are `doNotOptimizeAway`'d then `deinit`'d
  so execution (image decode/storage) is excluded; a 64 KiB read buffer
  deliberately matching termio `Exec.zig`'s `buffer_capacity`; `setup` opens the
  data file and `teardown` closes it; no data file means `step` is a no-op.

`crates/rustty-bench/src/screen_clone.rs` (524 lines) and `terminal_stream.rs`
(367 lines) were read end to end; both carry the oracle source as `//` comments
alongside the port, and both expose the same option sets as their oracle
counterparts.

---

## 5. `src/Surface.zig`'s mechanical residue (step 5)

```
git -C ../ghostty diff 4749c4e93..origin/main -- src/Surface.zig
320 changed lines; residue after the mechanical filter: 246
```

Area E's own note is that "a semantic change disguised inside a
`lockUncancelable` hunk would be missed", so the residue was read rather than
counted. It is ~90% the mutex/io migration (`renderer_state.mutex.lock()` ->
`lockUncancelable(global.io())`, `mailbox.push(global.io(), …)`,
`setName(global.io(), …)`, `file.close(global.io())`, `mutex.* = .{}` ->
`.init`). Three items survive:

### 5a. The occlusion rewrite (`6c8c07981`) — CONFIRMED behaviour-changing

```
+visible: bool = true,
+    if (self.visible == visible) return;
+    self.visible = visible;
+    const report_visibility = self.io.terminal.modes.get(.report_visibility);
+    self.io.terminal.flags.visible = visible;
+    if (report_visibility) { … }
```

`Surface` now carries the visibility state itself and **returns early when the
new state equals the old**, so repeated same-state occlusion callbacks no longer
push a renderer message. This is not additive. It also wires
`terminal.flags.visible` and the `report_visibility` mode. Owner: **`P3-W7`**
(OS / termio / platform) per the plan's own assignment of `6c8c07981`.

### 5b. Multi-click timing loses its failure path

```
-        const time = std.time.Instant.now() catch |err| time: {
-            log.err("error reading time, mouse multi-click won't work err={}", .{err});
-            break :time null;
-        };
-            .time = time,
+            .time = std.Io.Timestamp.now(global.io(), .awake),
```

`Instant.now()` was fallible and its failure disabled multi-click for that
press; `std.Io.Timestamp.now(io, .awake)` is infallible, so the `null` branch
and its `log.err` are gone. Mechanical in origin, but it deletes a
documented degraded mode. Record only — rustty's clock read is already
infallible, so there is nothing to port. Owner: none.

### 5c. `env.remove("GHOSTTY_LOG")` deleted

A one-line removal in the surface's child-environment construction. Owner:
**`P3-W7`** with the rest of the termio env work; noted here so it is not
attributed to the mutex migration it sits inside.

---

## Summary

| step | gap | verdict |
|---|---|---|
| 1 | `e8525c0fd` x 3 big terminal files, 1,710-line residue | all mechanical, 7 named classes, **no action** |
| 2 | `SharedDeps.zig` + `GhosttyLibVt.zig` | **2 ACTIONS** (2a libSystem override, 2b optimize-dependent terminalOptions) — owner `P3-W9` |
| 3 | area F font residue + 37 golden PNGs | **1 ACTION** (3b sprite cursor metrics, owner `P3-W8`); PNG sweep confirms area F's 6 files and no seventh |
| 4 | `HyperlinkMap` / `ApcParser` benches | **premise stale** — both ported by WS10 T11; oracle files read anyway |
| 5 | `Surface.zig` 246-line residue | **2 ACTIONS** (5a occlusion, owner `P3-W7`; 5c `GHOSTTY_LOG`, owner `P3-W7`) + 1 recorded non-action |
| 6 | filter negative control | filter does **not** swallow a constant change |

Five actions found in material that four assessments had declared "expected to
be mechanical". That is the return on doing this step rather than asserting it.
