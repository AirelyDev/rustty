# Phase 4 assessment — area E: os / termio / cli / app-level / crash / terminfo / renderer-zig-side

Range: `6ad1fe7d8..12967b68f` (543 commits, 359 non-merge).
Scope: `src/os/**`, `src/termio/**`, `src/cli/**`, `src/Command.zig`,
`src/Surface.zig`, `src/App.zig`, `src/crash/**`, `src/terminfo/**`,
`src/shell-integration/**`, `src/renderer/**`.

Area log: `git -C ../ghostty log --oneline --no-merges 6ad1fe7d8..12967b68f -- <paths>`
→ **53 commits**. Area diffstat: **40 files, +1,503 / −450**
(`src/Command.zig` and `src/shell-integration/` have **zero** commits in the range).

Rustty side: `crates/rustty-{os,termio,cli,core,crash,renderer,terminfo,args}`.

---

## 0. Coverage statement — read this before trusting any row below

**Exhaustively read, hunk by hunk (restricted to my paths):** all 53 commits.
Specifically, I ran `git show <hash> -- <my paths>` for every commit and read
the complete output for 51 of them. Two partial reads, both deliberate:

- `a177ba90a` (`generic.zig` +109/−47): read the first ~200 diff lines fully
  (the whole `syncDisplayLink` restructure, `init`, `setMacOSDisplayID`,
  `setFocus`, `setVisible`, the new `visible` field) and the ~20-line tail.
  Nothing between was skipped — the sed windows covered the full hunk set.
- `15c50c1db`: I read the full `crash/`, `cli/`, `os/homedir.zig`,
  `termio/shell_integration.zig` hunks and the header of the `os/xdg.zig`
  hunk. The xdg remainder is the same mechanical `io: std.Io` first-param
  threading shown in homedir (verified by the diffstat shape, not line-read).
  Its non-my-file hunks (`src/config/*`) belong to area G and were skimmed
  only to understand the caller contract.

Commits whose diffs touch other areas' files (e.g. `53c6fdbe7` apprt,
`3650bef4e` gtk, `daeed25b3`/`c454a3bf4` font, `15c50c1db` config): I read the
foreign hunks where needed to fix the end-state contract (noted per-row), but
their verdicts belong to the owning area.

**rustty side:** for each upstream change I grepped for the specific rustty
symbol and read the matching region (`file:line` cited per row). I did not
read the area crates end-to-end. Absence verdicts are grep-based with the
grep pattern implied by the cited symbol; presence/equivalence verdicts are
read-based.

**Not run:** no builds, no test runs. The w0 gate snapshots
(`w0/g1b-after-at-12967b68f.txt`) are used as ground truth for test-count
deltas.

**Superseded-chain handling (per the binding range-diff rule):** the
desktop-notification truncation chain (`6cadad06f` → `ae6d97ea7` →
`53c6fdbe7`) and the warmup chain (`c454a3bf4` → `131b293db` → `de1336fad` →
`931a538a3`) are assessed at END STATE, with every commit listed and its test
contribution noted. No commit in the range was reverted outright.

---

## 1. Summary — the numbers

Unlike Phase 3 (where ~78% of area churn was Zig-0.16 mechanical), this range
is almost entirely **semantic**: bug fixes with tests, small features, and
perf work. rustty has ported essentially none of it yet — this is expected,
Phase 4 IS the porting phase and this assessment drives it.

| Verdict | Count | Notes |
|---|---|---|
| **ABSENT** | **46** | see clusters below |
| **PARTIAL** | **6** | `2b32b5b75`, `c247e455c`, `0060d89b5`, `f17b425aa`, `1125fa26d`, `957ed21d5` — in each, Rust ownership/`Drop` or prior porting already provides part of the end state; the remainder (usually the new tests) is absent |
| **N-A** | **1** | `2f7fbadb0` — free-on-discard is provided by Rust ownership on every discard path (evidence in §3.1) |
| **PORTED** | **0** | |

**Headline gaps, ranked by user-visible severity:**

1. **`4b4a5b241`** — IOSurfaceLayer display callback not cleared before layer
   release. rustty's `release()` is a Drop no-op that never clears the
   callback (`crates/rustty-renderer/src/metal/iosurface_layer.rs:156`), so a
   host-view-retained layer can invoke the callback with a freed renderer
   context. **This is a live use-after-free hazard of the callback-ctx class
   (R9)**, not just a missing port.
2. **`9e6e2ea96`** — rustty carries the bug upstream fixed: after ~100k frames
   the terminal-state cleanup counter saturates and never resets
   (`crates/rustty-renderer/src/generic.rs:4666-4670` has no `= 0`), so every
   subsequent frame deinits and rebuilds the full render state.
3. **`0060d89b5`** — rustty sets `keyboard.sequence_set` BEFORE the fallible
   `encodeKey` (`crates/rustty-core/src/surface.rs:6740-6753`); an encode
   error corrupts the key-sequence state. Upstream's reorder is exactly
   applicable because `encodeKey` is fallible in Rust too.
4. **Desktop-notification truncation bundle** — rustty still truncates titles/
   bodies at a raw byte boundary (`stream_handler.rs:2193-2216`), splitting
   UTF-8 codepoints handed to platform notification APIs that require valid
   UTF-8.
5. **SSH terminfo cluster** — 4 commits, entirely absent, including the
   content-derived cache **version** (stale caches suppress required
   installs) and the removal of the `infocmp` short-circuit (stale remote
   entries never refreshed). Owns 1 of the area's 2 dropped G1b tests.

---

## 2. Per-commit verdict tables

Legend: "blocked on" names the owning area whose primitive must land first.
Cost S/M/L; risk per PORT_RULES vocabulary.

### 2.1 SSH terminfo / cache (cli + terminfo)

| # | Commit | What changed | Verdict | Evidence | Tests carried | Cost/Risk |
|---|---|---|---|---|---|---|
| P4-E1 | `8fca64957` "cli: report ssh terminfo cache failures" | `+ssh-cache` `runInner` gains an injected `cache: DiskCache` param (caller resolves `defaultPath` and reports it); every failure names the cache file; error strings lowercased after "Error: "; `{t}` formatting. | **ABSENT** | `crates/rustty-cli/src/ssh_cache.rs:272` `run_inner` has no cache param; `:305` still resolves `default_path` inside. | NEW `test "runInner fails cleanly when the cache is unusable"` (4 ops × exit-code-1 against a dir-shaped cache path). | S / LOW |
| P4-E2 | `ca3dc9eea` "cli: classify ssh cache errors in DiskCache" | NEW `DiskCache.isFailure(err)` (InvalidCacheKey/CacheLocked = healthy-cache errors); `+ssh` read AND write paths route reporting through it. | **ABSENT** | `grep -n isFailure crates/rustty-cli/src/ssh_cache/disk_cache.rs` → 0; `ssh.rs` has no classification. | NEW `test isFailure` (4 assertions) — **this is the G1b `cli_ssh_cache_diskcache drop=1`**. | S / LOW |
| P4-E3 | `fff9f62f3` "cli: always install embedded SSH terminfo" | Removes the `infocmp xterm-ghostty && exit 0` short-circuit from the remote install script (both verbose/quiet variants) + doc-comment rewrite. Also a `Config.zig` doc hunk (area G). | **ABSENT** | `crates/rustty-cli/src/ssh.rs:795,802` still emit `infocmp {ti} >/dev/null 2>&1 && exit 0`. | none new | S / LOW. R2 note: rustty's script templates on `TERMINFO_PRIMARY_NAME`; the deletion removes that interpolation site. |
| P4-E4 | `69b9abf09` "cli: version SSH terminfo cache entries" | `terminfo/ghostty.zig` gains comptime `version` (Wyhash of the encoded source, hex, `@setEvalBranchQuota(100_000)`), re-exported from `terminfo/main.zig`; `DiskCache.add/contains` take `version: []const u8`; `contains` matches version; `add` updates an existing entry's `terminfo_version`; `+ssh`/`+ssh-cache` pass `terminfopkg.version`. | **ABSENT** | `disk_cache.rs:154` `add(&self, key, timestamp)`, `:377` `contains(&self, key)` — old shapes; `disk_cache.rs:241` still stamps `TERMINFO_PRIMARY_NAME`; no `version` in `crates/rustty-terminfo/src/lib.rs`. | Reworked `"disk cache operations"` (adds v1-vs-v2 mismatch + list-version assertions), `"disk cache cleans up temp files"`, `"disk cache prune"`, `"add survives allocation failure"` — assertion-for-assertion updates. | **M / MEDIUM.** The Rust `version` must hash the RUSTTY-encoded source (identity fork R2): under `identity-oracle` it must equal a hash of the ghostty bytes; the const is content-derived so this falls out automatically — do NOT hardcode. Needs a Wyhash with Zig-`std.hash.Wyhash(0)` parity or the version strings diverge from nothing (self-consistent is sufficient — the value is only compared to itself; note this explicitly when porting). |

**Atomic bundle A:** P4-E1..E4 land together (the `runInner` signature, the
`isFailure` routing, and the versioned `contains` interlock; `fff9f62f3`
without `69b9abf09` re-installs on every connect only until first cache write,
then never refreshes again — worse than either alone).

### 2.2 Desktop-notification truncation (termio ↔ apprt)

| # | Commit | What changed | Verdict | Evidence | Tests carried | Cost/Risk |
|---|---|---|---|---|---|---|
| P4-E5 | `6cadad06f` | Introduces `copyUtf8Z` (validate-loop form) in `stream_handler.zig` + 2 tests. | **ABSENT** (superseded in range) | — | its 2 tests were superseded by E6's | — |
| P4-E6 | `ae6d97ea7` "avoid rescanning UTF-8 prefixes" | `copyUtf8Z` rewritten: back up from a continuation byte only when `len < src.len` (O(1) instead of revalidating the prefix per step); adds `isUtf8ContinuationByte`. Tests rewritten: per-sequence-length boundary test (1/2/3/4-byte), boundary-keep test, fits test. | **ABSENT** (superseded in range) | — | 3 tests (the surviving set, moved by E7) | — |
| P4-E7 | `53c6fdbe7` "apprt: own desktop notification truncation" | END STATE: helper + tests move into `apprt/surface.zig` as `Message.DesktopNotification` (named struct, `[63:0]u8`/`[255:0]u8`, `init(title, body)` ctor); `stream_handler.sendDesktopNotification` collapses to `surfaceMessageWriter(.{ .desktop_notification = .init(title, body) })`. | **ABSENT** | rustty still byte-truncates inline: `crates/rustty-termio/src/stream_handler.rs:2193-2216` (`title_buf[..title_len]` copy with no UTF-8 backup). apprt-api side has no `DesktopNotification::init`: `grep -rn "fn init" crates/rustty-apprt-api/src/surface.rs`. | The 3 end-state tests live in `apprt/surface.zig` → port into rustty-apprt-api's surface message module. | **S / LOW-MEDIUM.** Port the END STATE only (constructor in apprt-api, forwarding in stream_handler). **Coupling: area F** owns `src/apprt/surface.zig`. Behavior bug while absent: invalid UTF-8 handed to platform notification APIs (GLib/NSUserNotification reject or garble it). |

### 2.3 termio infrastructure

| # | Commit | What changed | Verdict | Evidence | Tests carried | Cost/Risk |
|---|---|---|---|---|---|---|
| P4-E8 | `e0ef934f7` "termio: replace SegmentedPool with std.MemoryPool" | `Exec.ThreadData` drops `write_req_pool`/`write_buf_pool` (SegmentedPool × 2, 32-prealloc) for one `write_pool: std.heap.MemoryPool(Write)` where `Write = { td: *ThreadData, req: xev.WriteRequest, buf: [64]u8 }`; `queueWrite` checks out one `Write` per chunk; `ttyWrite` callback keys off `*Write` and destroys it via `w.td.write_pool`. Bench: keystroke echo 3.93→0.96 ns/op. | **ABSENT** | `crates/rustty-termio/src/exec.rs:61` `use rustty_datastruct::SegmentedPool`, `:564-565` both pools live, `:1393-1405` getGrow pair, `:1538-1539` put pair. | none new (existing `subprocess`/`readthread` KATs cover behavior) | **M / HIGH (SB).** The pooled `Write` holds an INTRUSIVE `xev.WriteRequest` plus a back-pointer, and the buffer must stay pointer-stable from `queueWrite` until the xev completion fires on the termio thread. Per the LIFETIMES prospective prong (R11) this needs its rows BEFORE porting; the node must be handed to xev as a raw pointer, never through `&mut` re-borrows (memory `rust-mut-args-strongly-protected`, `intrusive-ptr-and-placeholder-fill`). A `Box`-per-write with a free-list is the honest Rust analogue of `std.heap.MemoryPool`. |
| P4-E9 | `2f7fbadb0` "termio: free resources for discarded messages" | Adds `Message.deinit` (frees `change_config` ptr, `write_alloc` data) and calls it on: Surface readonly `queueIo` (folds E20's arm), drain-mode `drainMailbox` pop, `Mailbox.deinit` residual pops, wakeup-notify failure, and failed `queue.push`. | **N-A — provided by Rust ownership, all five paths verified** | rustty `Message` OWNS its resources: `ChangeConfig { ptr: Box<DerivedConfig> }` (`message.rs:78`), `WriteAlloc(Alloc<u8>)` over an owned buffer (`rustty-datastruct/src/message_data.rs:28-45`). Discard paths: (1) readonly `queueIo` returns and drops the moved `msg` (`surface.rs:2871-2890`); (2) drain arm pops and drops (`thread.rs` drainMailbox drain branch); (3) `Mailbox::deinit` drops `Box<Queue>` whose `Vec<Option<T>>` drops residual messages (`blocking_queue.rs:36-38,115`); (4) wakeup-failure `return` drops the handed-back msg (`mailbox.rs:150-153`); (5) `push`/`push_or_return` consume-and-drop on failure (`blocking_queue.rs` PORT NOTE). | none new | 0 / NONE. Record only: no `Message::deinit` glue should be invented (R1 no-addition). |
| P4-E10 | `f5911d696` "message.zig doc comment grammar" | "the messages are IO thread sends are also very few" → "the number of messages we send to the IO thread is also very small"; 3× "pty" → "PTY" in doc comments. | **ABSENT** | `crates/rustty-termio/src/message.rs:38-39` still carries the old (ungrammatical) text; `:47` "sent to the pty". | none | S / NONE. Pure R1 comment parity. |
| P4-E11 | `b53728241` "terminal/apc: support reporting unknown APC sequences" (stream_handler hunk) | `apcEnd` renames `cmd`→`result` and adds `.unknown => return` arm. | **ABSENT** | `crates/rustty-termio/src/stream_handler.rs:621-631` — no `Unknown` arm in `apcCommand`. | none here (the apc.zig tests are area A/D's — the G1b `apc drop=4` module) | S / LOW. **Blocked on A/D** landing `apc::Command::Unknown`. |
| P4-E12 | `68beeeb3f` "terminal: add stream continuation tracking" (Termio hunk) | `Termio.init`: `.terminal_stream = .initAlloc(alloc, handler)` → `.init(.{ .allocator = alloc, .handler = handler })` (new Stream options struct that carries continuation opts). | **ABSENT** (call-site) | `crates/rustty-termio/src/termio_impl.rs:702` still `Stream::init_alloc(...)`. | none here (stream_continuation tests are area A's) | S here / LOW. **Blocked on A** (`src/terminal/stream_continuation.zig`, Stream init options). |
| P4-E13 | `4371871bc` "terminal/kitty: evict without scratch allocation" (Termio hunk) | `try self.terminal.setKittyGraphicsSizeLimit(...)` loses its `try` — eviction no longer allocates, cannot fail. | **ABSENT** (call-site) | `termio_impl.rs:1047-1057` still treats it as fallible (`size_limit.is_ok()` gating + a PORT NOTE explaining the errdefer dance that upstream just deleted). | none here | S here / LOW. **Blocked on A/D** kitty `graphics_storage` rework (G1b `kitty drop=11` is theirs). When it lands, delete rustty's `is_ok()` gate AND its PORT NOTE (stale-comment detector D9). |

### 2.4 Surface.zig — write lifecycle & input

| # | Commit | What changed | Verdict | Evidence | Tests carried | Cost/Risk |
|---|---|---|---|---|---|---|
| P4-E14 | `957ed21d5` "core: free allocated writes in read-only mode" | readonly `queueIo` frees `write_alloc` payloads; NEW test `"queueIo frees allocated writes in readonly mode"`. | **PARTIAL** | Free: N-A — rustty's match returns and the moved `msg` drops (`surface.rs:2876-2886`); superseded by E9's `msg.deinit()` form anyway. **Test: ABSENT** — `grep "queueIo frees" crates/rustty-core/src/surface.rs` → 0. | 1 test to port (as a leak-checked drop test — Miri covers the assertion Zig's testing.allocator made). | S / LOW |
| P4-E15 | `c11fe5486` "core: avoid copying OSC 52 clipboard responses" | `completeClipboardReadOSC52` transfers its built buffer via `.write_alloc` (`errdefer` free) instead of `Message.writeReq` copy+free. | **ABSENT** | `surface.rs:13023-13026` still `Message::writeReq(&buf)` (copies) then drops `buf`. Rust port: `Message::WriteAlloc(Alloc { data: buf })` — a move, no errdefer needed. | none new | S / LOW |
| P4-E16 | `fb4c56159` "core: transfer long key encoding buffer" | `encodeKey` long path: `WriteReq.init` copy → `.{ .alloc = .{ .data = try alloc_writer.toOwnedSlice() } }`; deletes the "double allocation" comment. | **ABSENT** | `surface.rs:7235-7254` keeps the copy AND the now-deleted comment (D9 flag when porting). | none new | S / LOW |
| P4-E17 | `0060d89b5` "core: fix encoded key request cleanup" | (a) child_exited path deinits the write_req before `close()`; (b) removes the too-broad `errdefer write_req.deinit()`; (c) leader arm: append with free-on-error, and **`sequence_set` assigned only after all fallible work**. | **PARTIAL** | (a),(b): N-A — Drop frees `write_req` at the `return Ok(Closed)` (`surface.rs:6528-6540`, PORT NOTE already says the errdefer never fires). **(c): ABSENT and live** — `surface.rs:6740-6753` sets `self.keyboard.sequence_set = Some(...)` BEFORE `self.encodeKey(...)?`; an encode error leaves the new set active, exactly the failure upstream fixed (Vec push is infallible, but `encodeKey` is not). | none new | **S / MEDIUM** — the (c) reorder is 3 lines and closes a real state-corruption path. |
| P4-E18 | `f1d225020` "core: avoid allocating for small pwd change actions" | `handleMessage .pwd_change`: `stackFallback(256, alloc).dupeZ` instead of unconditional heap dupe. | **ABSENT** | `surface.rs:3338` `w.slice().to_vec()` — always heap. Rust analogue: 256-byte stack buffer with heap fallback (e.g. the SmallVec-free two-arm pattern already used for SURFACE_ID) + `PERF(port)` if kept plain. | none new | S / LOW (perf-only; R10 marker if deferred). |
| P4-E19 | `f17b425aa` "surface: use id instead of intFromPtr" | `deinit` log + `App.performAllAction` warn log use `surface.id` instead of `@intFromPtr`; gtk command_palette sort key likewise (area F). | **PARTIAL** | `Surface.id` already exists (`surface.rs:293`). The two log sites exist only as verbatim comments (`surface.rs:2801`, `app.rs:736-739`) still citing `@intFromPtr` — update the comment text (R1 comment fidelity, D9). | none | S / NONE |
| P4-E20 | `8b7c57c75` "gtk: add window title renaming" (Surface hunk) | `performBindingAction` arms: `.prompt_window_title` → `performAction(.prompt_title, .window)`; `.set_window_title` → dupeZ + `performAction(.set_window_title, …)`. | **ABSENT** | `grep prompt_window_title\|SetWindowTitle crates/rustty-core/src/surface.rs crates/rustty-input/src/binding.rs` → 0. | none here | S / LOW. **Blocked on input** (`Binding.Action` variants) + **F** (apprt action arms; gtk impl). |
| P4-E21 | `25b2d8a38` "input,apprt: add new_tab_to_new_window" (Surface hunk) | `.move_tab_to_new_window` binding arm → `performAction`. | **ABSENT** | `grep move_tab_to_new_window crates/rustty-core crates/rustty-input` → 0. | none here | S / LOW. Blocked on input + F. |
| P4-E22 | `488b7109b` + `48d85eaeb` (bundle; assess end state) | NEW `pub fn mouseReportingActive`: lock renderer mutex (`lockUncancelable(global.io())` per the fixup), call `isMouseReporting`, unlock. GTK caller forwards middle click (area F). | **ABSENT** | `grep mouseReportingActive crates/rustty-core/src/surface.rs` → 0 (only `isMouseReporting` at `:8055`). | none | S / LOW. Port the 48d85eaeb end state directly (the plain-lock intermediate is a trap). rustty's mutex is the guard-less zigstd model — the lock/defer-unlock pair maps 1:1. **Coupling F** (the gtk caller). |
| P4-E23 | `77537c806` "macos: handled untrusted OSC8 hyperlinks more carefully" | Surface: `DerivedConfig.link_osc8` (from config `link-osc8`); `linkAtPos` gates the OSC8 hyperlink check on it; `processLinks` sends `.kind = .osc8` (was `.unknown`). os/open.zig: on macOS `kind == .osc8` → `error.UnsafeOSC8Link` (fail closed; apprt owns the policy), `.osc8 => unreachable` in the spawn switch, + NEW test `"macOS OSC 8 links have no generic opener fallback"`. | **ABSENT** | No `osc8` kind anywhere: `grep -n osc8 crates/rustty-os/src/open.rs` → comment only; `surface.rs:4415` `link::Action::OpenOsc8` still routes to the generic opener; no `link_osc8` DerivedConfig field. | 1 test (os/open) | **M / MEDIUM (security-relevant).** Fail-closed ordering matters: land the `os/open.rs` guard + `Kind::Osc8` WITH the Surface `.osc8` switch, or rustty keeps handing producer-controlled URLs to `open`. **Coupling: apprt-api** (`OpenUrl.Kind` enum — C-ABI enum, check `rustty-embed.h` byte-parity per the embed-abi gate), **F** (macOS policy impl), **G** (config `link-osc8` option). |
| P4-E24 | `2b32b5b75` "core: close retained temp directory handles" | TempDir gains `CloseMode {delete, retain}` + `close(mode)` (always releases dir AND parent handles); `deinit` = `close(.delete)`; `writeScreenFile` uses a `retain_tmp_dir` flag, set only after successful dispatch; 2 fd-leak regression tests (BADF fcntl probes + FileNotFound / retained-dir assertions). | **PARTIAL** | Semantics: already equivalent — rustty's `TmpDirGuard` (armed → deinit-on-drop, disarmed on success only; `surface.rs:12372-12405,12681`) IS the end-state retain/delete behavior, and the fd leak **cannot occur**: rustty's TempDir models Dir as a PATH (`tempdir.rs:107-133`, `close(&mut self) {}` no-op) — no handles to leak. **ABSENT:** the `close(CloseMode)` public API + the 2 tests. | Port the 2 tests with the fd-BADF probes N-A'd (path model — say so in a PORT NOTE) and the on-disk assertions kept (deleted-after-deinit; retained-after-close(.retain)). | S / LOW |

### 2.5 App.zig + edit-config + warmup

| # | Commit | What changed | Verdict | Evidence | Tests carried | Cost/Risk |
|---|---|---|---|---|---|---|
| P4-E25 | `0a183c923` "core/gtk: allow editing Ghostty config in a Ghostty window" | NEW `src/os/edit.zig`: `getConfigEditCommand(alloc, path, opts)` — $VISUAL/$EDITOR resolution with `default_editor` enum (failure/vi/nano), shell-escaped command build, `error.NoEditorConfigured`; re-exported from os/main + `test { _ = edit; }`. `App.Message.open_config` becomes `OpenConfig` enum (os_open/new_window), threaded through `drainMailbox`/`performAction`. `cli/edit_config.zig` rewritten onto the shared helper (error text now prints `Command:` + OSC8-linked `Path:`). | **ABSENT** | No `crates/rustty-os/src/edit.rs` (ls verified); `crates/rustty-core/src/app.rs:435,647,870` `Message::OpenConfig` payloadless; `crates/rustty-cli/src/edit_config.rs:178-182` still inline VISUAL/EDITOR logic. | os/edit has no `test{}` beyond the module ref; edit_config has none. | **M / LOW-MEDIUM.** **Coupling F** (gtk `open_config` action payload + the new-window editor surface) and **apprt-api** (`Action::OpenConfig` gains a value — C-ABI surface). The os/edit half + App enum + cli rewrite are freestanding and can land first. |
| P4-E26 | `c454a3bf4` + `131b293db` + `de1336fad` + `931a538a3` (warmup chain; END STATE) | `App.create` spawns detached warmup threads inline (comments folded back by `931a538a3`): `font.Discover.warmup` if declared; `renderer.Renderer.API.warmup` if declared. `Metal.warmup` (end state): chooseDevice + release; newCommandQueue + release; build+discard `Shaders.init` for both `bgra8unorm_srgb`/`bgra8unorm`. Measured: renderer init 2777µs→1466µs, GraphicsAPI.init 4227µs→~900µs. | **ABSENT** | `grep -rn warmup crates/rustty-core/src/app.rs crates/rustty-renderer/src/metal.rs` → 0. | none | **M / MEDIUM.** Port end state only (the `threadedWarmup` fn of the intermediates was dissolved — porting it is the superseded-commit trap). `@hasDecl` gates map to trait-default or cfg probes on the renderer/font seams. **Coupling: font** (`Discover.warmup` — CoreText DB prime) — the App hunk can land with a no-op font arm if font lags. Thread-safety claim ("Metal device queries are thread-safe") should carry the upstream comment verbatim. |

### 2.6 renderer

| # | Commit | What changed | Verdict | Evidence | Tests carried | Cost/Risk |
|---|---|---|---|---|---|---|
| P4-E27 | `cde7f9343` "renderer: simplify cell row storage" | `Contents.fg_rows` drops ArrayListCollection for `[]CellTextRow` owned directly; `bg_cells` default `&.{}` + **in-place `realloc`** on resize; cursor lists initCapacity(1) up front; `resize` rebuilds then swaps under `errdefer comptime unreachable` + calls `reset()`; every test rewritten `.lists[i]` → `[i]`; NEW test `"Contents resize grows and shrinks"`. Also `generic.zig` `syncFromArrayLists(self.cells.fg_rows)`. | **ABSENT** | `crates/rustty-renderer/src/cell.rs:20,106,122,165-177` — ArrayListCollection throughout. | NEW resize test — **this is the G1b `renderer_cell drop=1`**. | **M / MEDIUM.** **Atomic bundle B** with the upstream DELETION of `src/datastruct/array_list_collection.zig` (this was its only consumer): port this, then delete `crates/rustty-datastruct/src/array_list_collection.rs` + its tests (deletion owner: the datastruct area lane — coordinate, same pattern as the Phase-3 G-OSDEL gate). The `realloc` keep-contents-then-`reset()` subtlety: bg realloc may preserve stale bytes; `reset()` after swap is what re-zeroes — don't reorder. |
| P4-E28 | `a177ba90a` "macos: tolerate display link creation failures" | Display link becomes LAZY: created inside new `syncDisplayLink(display_id, draw_now)` (logs-and-continues if no active display, e.g. locked session); `init` no longer creates it (removes the error path that aborted renderer init); `loopEnter`/`setFocus`/`setVisible`/`setMacOSDisplayID` all route through sync; NEW `visible: bool` field; `Thread.drainMailbox` passes `&self.draw_now` into `setMacOSDisplayID`; display change re-adds the link. Also `xev` now taken via `global.xev` (Zig build-graph detail). | **ABSENT** | `crates/rustty-renderer/src/generic.rs:2128-2267` — display link still created in `init` with the errdefer dance; `set_visible` (`:2607`) still start/stop only; no `syncDisplayLink`; no `visible` field. | none | **M-L / MEDIUM.** Renderer is placeholder-live territory; the restructure touches the D1-adjacent `DisplayLink` callback registration (`draw_now: *xev.Async` crossing threads — callback-ctx class, keep the raw-ptr contract documented). `global.xev` import shuffle is N-A for Rust (crate dep already direct). |
| P4-E29 | `4b4a5b241` "renderer/metal: clear callback before layer release" | `IOSurfaceLayer.release` sets `setDisplayCallback(null, null)` BEFORE `layer.release()` — host view can retain the layer past renderer teardown; prevents CA display pass invoking the callback on a freed renderer. | **ABSENT — live UAF hazard** | `crates/rustty-renderer/src/metal/iosurface_layer.rs:152-158`: `release(&mut self)` is a documented no-op; the `Retained` drop releases WITHOUT clearing the callback. If the NSView side retains the layer (it does, that's the whole point of the fix), the callback-ctx (renderer ptr) dangles. | none | **S / HIGH (R9 callback-ctx).** Port as: clear callback in `release()` AND in `Drop` (defense in depth, same observable policy as the abort_on_unwind precedent). Add to the unsound-safe ledger until landed. |
| P4-E30 | `9e6e2ea96` "renderer: reset terminal state cleanup counter" | `terminal_state_frame_count = 0` inside the periodic-cleanup branch. | **ABSENT** | `generic.rs:4666-4670`: deinit + `= RenderState::empty()` but NO counter reset — rustty rebuilds the full state EVERY frame after ~12min at 120Hz. | none | **S / — port immediately.** One line. |
| P4-E31 | `daeed25b3` (generic.zig hunk only) | `defer self.font_shaper.endFrame()` hoisted to the TOP of `updateFrame`-equivalent so CoreText deferred-release flushes even when the rebuild fails mid-way (OOM); removes the tail call. (The coretext null/OOM half is the font area's.) | **ABSENT** | `generic.rs:5038` — `end_frame()` still a straight-line tail call; any `?` early-return skips it (deferred-release objects accumulate). | none | S / LOW-MEDIUM. Rust shape: a small Drop guard over `&mut font_shaper` or a `defer!`-style scope — mind the borrow across the body (shaper-hook precedent: use the raw-self helper if a protected `&mut` would span aliased calls). |
| P4-E32 | `6760c6482` + `b5e86a428` + `d0c516f8f` + `b8222f4a8` (kitty pending-image renderer side; END STATE) | `image.zig`: pending-payload gating — `kittyUpdate` marks-for-unload when `image.data.isPending()`; `prepKittyPlacement`/virtual-placement path skip pending; `prepKittyImage` unwraps `image.data.bytes() orelse unreachable`. One evolving test `"kitty renderer ignores pending payloads and removes replaced placements"` (end name), growing per commit: pin-count assertions (b5e86a428/d0c516f8f pass `t.screens.active` into `addPendingImage`/`addPlacement`), retransmit-clears-placements (b8222f4a8: placements.count()==0 + trackedPins restored). | **ABSENT** | No kitty `ImageData` pending union in rustty (`grep Pending crates/rustty-vt/src/kitty/graphics_image.rs` → 0); renderer `image.rs` has no `data.is_pending()` gating (the existing `is_pending` hits at `image/macos_impl.rs:1345` are the renderer's own upload-state enum — a different concept, do not confuse them when porting). | 1 evolving test (port end state, assertion-for-assertion incl. the tracked-pin counts). | **M here / blocked.** **Atomic bundle C with A/D:** storage APIs gain a `screen` param and eviction/replacement releases pins — the renderer test asserts A/D behavior. The G1b `kitty drop=11` is the A/D side of this same bundle. The pin-release halves are pin-lifecycle (raw-Pin territory) — A/D owns the SB analysis; the renderer side only reads. |

### 2.7 crash + os support (no-global-state restructure)

| # | Commit | What changed | Verdict | Evidence | Tests carried | Cost/Risk |
|---|---|---|---|---|---|---|
| P4-E33 | `15c50c1db` "crash: do not use global state" | `crash/dir.zig` `defaultDir(io, alloc, environ_map)`; `ReportIterator` methods take `io`. `crash/sentry.zig`: statics `dir_mem`/`state_dir_`/`cache_dir_`; `init(gpa, environ_map)`; `Transport.sendInternal` uses `state_dir_` (error if unset) instead of `global.alloc()`; new file-local `cacheDir(io, alloc, environ_map)`. `os/homedir.zig` `home`/`expandHome` + `os/xdg.zig` fns take `io` (mechanical); `homeUnix`'s `/bin/sh -c "cd && pwd"` subprocess now gets `.environ_map = environ_map` (semantic: child env = snapshot). cli `crash_report.zig` builds the env map and passes io+map. `termio/shell_integration.zig` setupBash `homedir.home(global.io(), …)` (mechanical). Config hunks: area G. | **ABSENT** (semantic core) / N-A (io-param threading — Rust has no `std.Io` provider) | `crates/rustty-crash/src/sentry.rs:167` `init()` takes nothing; `:319-338` initThread reads `rustty_global::environ()`/`environ_map()` live — the exact main-thread-`setenv`-vs-init-thread-read race upstream removed (same class as the Rust-2024 `set_var` unsafety and the `parallel-cargo-test-exposes-c-init-races` memory). `crates/rustty-crash/src/dir.rs:20` `defaultDir()` param-less. | none new | **M / MEDIUM.** Port: snapshot the env map in the caller (global-owning area's hunk — **coupling**) and MOVE it into the init thread (a82637b53 end state, below); statics for `state_dir_`/`cache_dir_` (Rust: `OnceLock<PathBuf>`-shaped, byte-typed). The homeUnix subprocess env snapshot is semantic — rustty's `home()` shell fallback must pass the captured env, not inherit. |
| P4-E34 | `a82637b53` "crash: resolve sentry directories on the init thread" | END STATE over E33: `init(gpa, environ_map /* by value, owned */)` spawns `initThread(gpa, environ_map)` which deinits the map; ALL dir resolution moves onto the init thread (macOS `NSFileManager URLForDirectory:` was the single largest global.init cost: 2967-4018µs → 30-70µs); every early-return path deinits the map (build-off, Windows, spawn-failure). | **ABSENT** | same evidence as E33 — rustty resolves nothing off-thread and owns no snapshot. | none | S on top of E33 / LOW. In Rust the by-value map ownership is just a move into the thread closure — the four manual `map.deinit()` arms collapse into Drop (record as N-A-by-ownership glue, mirroring E9's reasoning). |
| P4-E35 | **Suspected upstream regression (flag, decision needed):** `15c50c1db`'s new `cacheDir` INVERTED the macOS XDG_CACHE_HOME test. Pre-range: XDG_CACHE_HOME set-and-nonempty → use XDG path; else NSCachesDirectory. At HEAD (verified in the working tree, `src/crash/sentry.zig` `fn cacheDir`): set-and-nonempty → **NSCachesDirectory**; unset → xdg.cache fallback. The comment ("if XDG env vars are set we will respect them") now contradicts the code. `a82637b53` did not touch it. | — | oracle working tree read | — | **Escalate before porting:** R1 says port the end state, but this looks like an accidental inversion (comment/code disagreement is the tell). Recommend: port end-state bytes, add `TODO(port): upstream cacheDir XDG inversion, ghostty#?` + file upstream. Do NOT silently "fix" it (R12 doctrine — divergence needs documentation either way). |
| P4-E36 | `f36854345` "libghostty: skip stack traces in release panic handlers" (os half) | NEW `src/os/stderr.zig`: `write(bytes)` — raw best-effort stderr bypassing std.Io (~70KB savings): freestanding no-op; Windows NtWriteFile; POSIX write loop with INTR retry and partial-write continuation; + smoke `test write`. Wired into os/main. Consumer: `src/lib_vt.zig` panic/log handlers (area C). | **ABSENT** | no `crates/rustty-os/src/stderr.rs` (ls verified). | 1 smoke test | **S / LOW — joint decision with area C.** The MOTIVE (Zig std.Io code-size) is toolchain-specific (quirks_memset precedent E34-P3), but the PRIMITIVE (signal-safe raw stderr write for panic paths) is meaningful in Rust too: libghostty-vt's DG5 abort path wants a no-alloc, no-lock writer. Recommend porting the POSIX arm as `libc::write` loop (INTR + partial-write semantics identical); Windows arm stays `cfg(windows)`. If area C rules the whole panic-handler feature N-A, downgrade to N-A with that citation. |
| P4-E37 | `d61920d80` "lib-vt: disable logging in wasm release builds" (os half) | `os/wasm/log.zig` gains `pub fn noop(...)` — comptime-parameters log fn that compiles out all logging machinery (~220KB in ReleaseFast wasm). Consumer: lib_vt std_options (area C). | **ABSENT** | `crates/rustty-os/src/wasm/log.rs` (79 lines) has no noop. | none | S / LOW. Rust shape: a no-op arm in the wasm log shim, selected by `cfg(not(debug_assertions))` — R8 build-switch preservation: keep it profile-switchable, never fold. **Coupling C** (lib_vt logFn wiring). |

### 2.8 cli (remaining) + os/i18n + terminfo

| # | Commit | What changed | Verdict | Evidence | Tests carried | Cost/Risk |
|---|---|---|---|---|---|---|
| P4-E38 | `034506f14` "gtk: add +new-tab cli action" | NEW `src/cli/new_tab.zig` (272 lines): Options (class, surface-id, working-directory resolution incl. `~` expansion + `GHOSTTY_SURFACE_ID` fallback), parseManuallyHook with `-e` passthrough, `runArgs` → `apprt.App.performIpc(..., .new_tab, .{ .surface_id, .arguments })`; `ghostty.zig` Action `@"new-tab"` (run + options arms). | **ABSENT** | no `crates/rustty-cli/src/new_tab.rs` (ls verified); `crates/rustty-cli/src/rustty.rs:225-331` Action enum has NewWindow but no NewTab. | none (0 `test{}` in the new file) | **M / LOW.** `new_window.rs` is the 1:1 template. **Coupling F**: `performIpc` must grow the `.new_tab` IPC action (+ apprt-api ipc payload struct with `surface_id`). |
| P4-E39 | `3650bef4e` "+new-window -e shell integration" (cli hunks only; gtk core is area F) | new_tab.zig/new_window.zig doc-comment updates advertising `--shell-integration=<mode>` passthrough. | **ABSENT** | `grep shell-integration crates/rustty-cli/src/new_window.rs` → 0. | (the `applyCommandOverrides` fn + its 7-assertion test live in gtk surface.zig — **area F's**, flag it to them) | S / NONE here. |
| P4-E40 | `1d6bc6829` "lib: remove cutPrefix implementation" (cli hunk) | new_window.zig drops `lib.cutPrefix` for `std.mem.cutPrefix` (Zig 0.16); upstream also deleted `src/lib/string.zig`. | **ABSENT** | `crates/rustty-cli/src/new_window.rs:35,117,125` still `rustty_lib::cut_prefix`; `crates/rustty-lib/src/string.rs:3` even carries the comment "Once Ghostty has been ported to 0.16 this can be deleted" — that day arrived. | string.zig's test moves/dies with it | S / LOW. Rust std analogue: `<[u8]>::strip_prefix` — same semantics (Option of the tail). **Atomic bundle D:** switch BOTH callers (this one + any other `cut_prefix` users — grep shows new_window only in my area) then delete `rustty-lib/src/string.rs` (deletion owner: lib area lane; propose a G-OSDEL-style row). |
| P4-E41 | `987f44260` "cli: g/G aliases in list-themes" | Home/End matchers gain `'g'`/`'G'`; help lines "Home, g" / "End, G". | **ABSENT** | `crates/rustty-cli/src/list_themes.rs:914` matches `[home, kp_home]` only; `:1341` help text `b"Home"`. | none | S / NONE |
| P4-E42 | `c247e455c` "config: refill after ignored line boundaries" | `cli/args.zig` LineIterator: when a comment/blank line consumes the buffered data, `fillMore()` before `continue` (parsing previously stopped silently at the boundary); NEW test `"LineIterator refills after ignored line at buffer boundary"` (2 sub-cases: `"#\nA\n"` w/ 2-byte buffer, `"\nA\n"` w/ 1-byte buffer). | **PARTIAL** | Code: **N-A, structurally** — rustty's LineIterator holds the whole source slice, no refill exists to miss (`crates/rustty-args/src/args.rs:1699,1780-1786`; the crate's own PORT NOTE at `args/tests.rs:1484-1485` already documents this for the earlier buffered-reader tests, and those were ported as identical-output assertions). **Test: ABSENT** — `grep "refills after ignored" crates/rustty-args` → 0. | port the 2 sub-cases as identical-output assertions (`"#\nA\n"` → `--A`, `"\nA\n"` → `--A`) per the in-file precedent | S / NONE |
| P4-E43 | `df23bef0e` + `1125fa26d` (i18n pair; end state) | `N_(msgid)` marker fn (mark-for-translation, returns msgid); `_` doc note + `if (@inComptime()) return msgid;` + NEW test `"_ returns msgid at comptime"`. Callers (command palette) are gtk/F; Latvian po is G. | **ABSENT** (`N_`, doc, test) / **N-A** (`@inComptime` mechanics) | `grep "fn N_" crates/rustty-os/src/i18n.rs` → 0. Rust has no `@inComptime`; the equivalent contract is `_` being a plain fn whose const-context use returns the msgid — a `const fn` fast-path is not expressible over dgettext. Port `N_` + doc comment; port the test as a plain call asserting identity. | 1 test | S / NONE |
| P4-E44 | `30e5eabe9` "'da' locale" | `os/i18n_locales.zig` locales += "da". | **ABSENT** | `crates/rustty-os/src/i18n_locales.rs:50` ends at `"kk","be","eu"`. | none | S / NONE. Coupling G owns the `po/` file itself. |
| P4-E45 | `bb1f5908f` "terminfo: advertise overline support" | ghostty.zig capabilities += `Smol` (`\E[53m`), `Rmol` (`\E[55m`) after Smulx. | **ABSENT** | `grep Smol crates/rustty-terminfo/src/entry.rs` → 0 (Smulx present at `entry.rs:105`). | none (covered by existing `encode` golden — WHICH MUST BE REGENERATED: the encoded bytes change, and so does the E4 `version` hash) | S / LOW |
| P4-E46 | `74efadb44` "lib-vt: answer XTGETTCAP queries" (terminfo half) | `Source.names`/`Capability.string` become `[:0]const u8`; `xtgettcapMap` values null-terminated (zero-padded buffers, `[0..len :0]` slices; numeric buf 10→11) so responses hand straight to the C `write_pty` callback without copying. The consumer (stream_terminal answering XTGETTCAP, `GHOSTTY_TERMINAL_OPT_TERMINFO_NAME`, TN interception) is areas A + C. | **ABSENT** (my half) | `crates/rustty-terminfo/src/source.rs:92` `xtgettcap_map() -> HashMap<Vec<u8>, Vec<u8>>` — no sentinel; `crates/rustty-vt/src/stream_terminal.rs:595` still discards `.xtgettcap`. | Source.zig test updates (existing `xtgettcap_map_test` gets sentinel-aware assertions) | **S here / M overall.** The NUL-termination exists so `write_pty` avoids a copy; in Rust the callback takes ptr+len, so the sentinel is only needed if the C-API contract (area C) requires NUL — decide jointly; if C keeps ptr+len, record the divergence as a PORT NOTE instead of faking sentinels. **Atomic bundle E with A/C** (`terminal→terminfo` is a NEW dependency edge: check `cycle_check` — rustty-vt would depend on rustty-terminfo; verify acyclicity BEFORE porting, or mirror upstream by moving the table). **R2:** the map must be built from the RUSTTY source; TN is intercepted upstream precisely so the embedded name is configurable — that logic is identity-sensitive, `identity-oracle` must flip it. |

### 2.9 wasm/log — covered as P4-E37 above; `src/os/wasm/*` has no other commits.

---

## 3. Partials — the exact missing hunks

1. **P4-E17 (`0060d89b5`)** — move `self.keyboard.sequence_set = Some(&**set as *const binding::Set);` (`surface.rs:6743`) to AFTER the `encodeKey`+push block (`:6751-6753`), with upstream's comment "Setup the next set we'll look at only after all fallible work." 3 lines.
2. **P4-E24 (`2b32b5b75`)** — add `CloseMode` + `close(mode)` to `rustty-os/src/tempdir.rs` (path model: `.delete` = current deinit body, `.retain` = no-op) and port the 2 tests minus the fd-BADF probes (N-A'd with a PORT NOTE).
3. **P4-E42 (`c247e455c`)** — 2 identical-output test cases in `crates/rustty-args/src/args/tests.rs`.
4. **P4-E14 (`957ed21d5`)** — the readonly-queueIo leak test (Miri is the leak assertion).
5. **P4-E19 (`f17b425aa`)** — comment-text updates at `surface.rs:2801`, `app.rs:736-739`.
6. **P4-E43 (`1125fa26d`)** — `N_` + doc + identity test in `rustty-os/src/i18n.rs`.

## 4. Deletions in/adjacent to the range that touch my consumers

Upstream deleted **no files in my path set** this range (`src/os/edit.zig`,
`src/os/stderr.zig`, `src/cli/new_tab.zig` are ADDITIONS). But three upstream
deletions elsewhere strand my consumers:

| Deleted upstream | rustty file to delete | My consumer to migrate first | Bundle |
|---|---|---|---|
| `src/datastruct/segmented_pool.zig` | `crates/rustty-datastruct/src/segmented_pool.rs` | `rustty-termio/src/exec.rs` (P4-E8) | B/E8 |
| `src/datastruct/array_list_collection.zig` | `crates/rustty-datastruct/src/array_list_collection.rs` | `rustty-renderer/src/cell.rs` (P4-E27) | B/E27 |
| `src/lib/string.zig` (cutPrefix) | `crates/rustty-lib/src/string.rs` | `rustty-cli/src/new_window.rs` (P4-E40) | D |

(`src/datastruct/lru.zig` also died upstream but its consumer is not in my area.)
Phase-3's `check_deleted_os_modules.sh` (G-OSDEL) is the precedent: propose
extending it (or a sibling gate) with these three rows once the migrations land.
**G-OSDEL itself stays GREEN** — nothing in this range resurrects
`rustty_os::{args,env}`; the E13/E15/E16 world it protects is undisturbed
(P4-E33's env-snapshot work builds on it).

## 5. Atomic bundles & cross-area couplings

**Bundles (must land together or be worse than absent):**
- **A** — SSH cluster (P4-E1..E4): signatures interlock; partial landings
  either spam warnings or permanently stop refreshing remote terminfo.
- **B** — P4-E8 + segmented_pool deletion; P4-E27 + array_list_collection
  deletion (datastruct lane owns the delete; my lane owns the consumer).
- **C** — kitty pending/pin-release (P4-E13, E32) with A/D's
  `graphics_storage`/`ImageData` rework and its `kitty drop=11` tests.
- **D** — P4-E40 + `rustty-lib` string.rs deletion.
- **E** — P4-E46 with A's stream_terminal XTGETTCAP + C's
  `GHOSTTY_TERMINAL_OPT_TERMINFO_NAME`; acyclicity check REQUIRED first.
- **F** — notification truncation (P4-E5..E7) with area F's
  `apprt/surface.zig` Message.DesktopNotification.
- **G** — crash restructure (P4-E33/E34) with the global-owning lane's
  `sentry.init` caller change (env-map snapshot ownership).

**Couplings by area:**
- **F (apprt/gtk/macos):** DesktopNotification ctor+tests (E7); OpenUrl
  `Kind.osc8` + macOS policy (E23); `performIpc .new_tab` (E38);
  `open_config` payload (E25); `applyCommandOverrides` + 7-assertion test
  (E39 — flag to F explicitly, it is easy to orphan); prompt/set window
  title + move_tab_to_new_window action arms (E20/E21); `mouseReportingActive`
  gtk caller (E22).
- **A/D (terminal):** `apc::Command::Unknown` (E11); Stream continuation init
  options (E12); kitty storage screen-param APIs + pending union (E13/E32);
  stream_terminal XTGETTCAP consumer (E46).
- **input:** three new `Binding.Action` variants (E20/E21).
- **G (config):** `link-osc8` option (E23); ssh-terminfo doc hunk (E3);
  `15c50c1db` config/* hunks; `po/lv.po`, `po/da.po`.
- **C (lib/libghostty):** lib_vt panic handler ↔ os/stderr (E36); wasm log
  noop wiring (E37); XTGETTCAP C-API opt (E46).
- **font:** `Discover.warmup` (E26); coretext null/OOM (E31's sibling hunks).
- **global lane:** sentry.init caller (E33/E34).

**UB/SB hazards (PORT_RULES §R9/R11):**
1. E29 IOSurfaceLayer callback-ctx UAF — live today, fix is independent of
   everything else. Highest priority in the area.
2. E8 pooled `Write` nodes: intrusive xev.WriteRequest + raw back-pointer;
   LIFETIMES rows before port; raw-ptr handoff, no `&mut` re-borrow across
   the completion window; Miri harness advised (mirihar precedent).
3. E28 display-link lazy creation: `draw_now: *xev.Async` captured by a CVDisplayLink
   callback across threads — document the ctx lifetime (Thread owns draw_now,
   link stopped before thread teardown; verify rustty's teardown order keeps that).
4. E31 endFrame guard: a Drop guard holding `&mut font_shaper` across a body
   that also touches `self` risks a protected-reference overlap — use the
   raw-self helper pattern if needed (memory `shaper-hook-sb-clean-interleave`).

## 6. Dropped-test targets from `w0/g1b-after-at-12967b68f.txt` in this area

| Module | drop | Owning commit | Row |
|---|---|---|---|
| `cli_ssh_cache_diskcache` | 1 | `ca3dc9eea` (`test isFailure`) | P4-E2 |
| `renderer_cell` | 1 | `cde7f9343` (`"Contents resize grows and shrinks"`) | P4-E27 |

All other RED modules (formatter/apc/stream/kitty/modes/input_key_encode/
font_shaper_harfbuzz/apprt_api_surface) belong to areas A/D/F. Note the
range also ADDS tests G1b does not count as drops because module totals grew
on both sides or the module keys don't split them out (e.g. ssh_cache's
`"runInner fails cleanly"`, os/open's OSC8 test, TempDir's retain test,
Surface's readonly-queueIo test, the message.zig deinit-adjacent none) — the
per-row "Tests carried" column is the authoritative checklist, not G1b alone.

## 7. Notes for the porting workflow

- **Zero commits touched `src/Command.zig` or `src/shell-integration/`** in
  this range; the Phase-3 open items there (`posix_waitpid` ECHILD §5.2,
  bounded `killPid` §5.1, `execvpeZ` err-latch §3.3) are UNCHANGED by this
  range and remain tracked in the Phase-3 assessment / DEFERRALS.md.
- The termio pipeline (Phase-3 E1) was also untouched this range — no rebase
  pressure added, the E3 `yieldToDemand` PORT NOTE remains valid.
- `f5911d696` + `2f7fbadb0` both edit `message.zig`; port the comment fix
  (E10) even though the deinit (E9) is N-A — do not let the N-A verdict
  swallow the comment hunk.
- Negative controls worth wiring when porting: E30's counter reset (assert a
  second post-threshold frame does NOT deinit state); E17's reorder (encode
  failure leaves `sequence_set` unchanged); E2's isFailure (flip a variant,
  test fails); E4's version (mutate the terminfo table, cached `contains`
  must miss).

*Assessed 2026-08-17 against oracle working tree at 12967b68f. No code modified.*
