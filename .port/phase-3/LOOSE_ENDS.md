# LOOSE_ENDS — in-code markers of unfinished work

Branch: `loose-ends`. Oracle pin: `../ghostty` @ `6ad1fe7d8`.

This is the sweep of every in-code marker of unfinished work: `unimplemented!()` /
`todo!()` call sites, `TODO`/`FIXME`/`XXX:`/`HACK` comments, `#[ignore]` without a
reason, `unreachable!()`-as-stand-in, and `PLACEHOLDER`/`STUB`/`NOT PORTED`/`DEFERRED`
markers.

## Method (and why the headline counts are misleading)

The two headline greps count **lines mentioning a marker**, not markers:

| grep | raw | actual sites |
|---|---|---|
| `unimplemented!()\|todo!()` | 64 | **62** real call sites (the other 2 are prose/an assert message) |
| `TODO\|FIXME\|XXX:\|HACK` | 172 | 172, but see the ORACLE class below |

The load-bearing distinction for the `TODO` class is **who wrote the marker**. Ghostty
(and its vendored Zig deps: libxev, z2d, vaxis, zf, oniguruma, Zig `std`) carry their own
`TODO`/`FIXME`/`HACK` comments, and R1 requires the port to reproduce them verbatim. A
faithfully-carried upstream marker is not rustty debt — deleting it would be an R1
regression, and "closing" it would mean diverging from the oracle.

So the classification below adds a fourth class to the three in the brief:

- **ORACLE** — the marker exists in the oracle (or a vendored Zig dep) and is carried
  verbatim for R1. Not rustty debt. **Keep.**
- **DOCTRINE** — an argued inhabited-placeholder leaf.
- **STALE** — surrounding work has landed; the marker is out of date.
- **REAL GAP** — behaviour the oracle has that rustty does not.

Cross-check was mechanical, not eyeballed: `/tmp/le/xcheck.py` normalizes every rustty
marker line (strip comment leader, un-rebrand `rustty`→`ghostty`, collapse whitespace,
casefold) and looks it up in a corpus built from `ghostty/src` + `ghostty/pkg` +
`ghostty/macos` + `ghostty/vendor` + every extracted tarball in `~/.cache/zig/p`.
`/tmp/le/rev.py` runs the same comparison in reverse — oracle markers **missing** from
rustty — which is where all the real findings came from.

## Summary

### `unimplemented!()` / `todo!()` — 62 sites

| class | count | disposition |
|---|---|---|
| DOCTRINE | 62 | all argued; **0 changed** |
| STALE | 0 | — |
| REAL GAP | 0 | — |

**No undocumented `unimplemented!()` holes.** Every site is either a documented
platform/test leaf or a test double. Breakdown:

| file:line | marker | class | why |
|---|---|---|---|
| `crates/rustty-xev/src/lib.rs:660,666,669,675,678,681,687,690,696,699` | `unimplemented!("{}", MSG)` | DOCTRINE | `no_backend_seam` — the `#[cfg(not(macos/linux/windows/wasi))]` arm. libxev's `backend.zig` maps every supported target onto io_uring/epoll/kqueue/wasi_poll/iocp; freestanding wasm has no WASI ABI to poll. There is **no reference body to port**. Argued in the 40-line header at `lib.rs:585-609`. |
| `crates/rustty-vaxis/src/tty.rs:1524` | `TestTty.next_event` | DOCTRINE | Windows-API compat only; the oracle's own body is `error.SkipZigTest`. |
| `crates/rustty-apprt/src/embedded.rs:2290` | `Inspector::renderMetal` non-Darwin | DOCTRINE | Metal is Darwin-only and the sole caller is itself `#[cfg(target_os = "macos")]` — structurally unreachable. |
| `crates/rustty-apprt/src/gtk/class/inspector_widget.rs:109,120` | `_ => unimplemented!()` | DOCTRINE | The `_` arm of a GObject `property`/`set_property` `match` over a **one-property** pspec list. Idiomatic gtk4-rs; unreachable by construction (GObject only dispatches registered pspecs). |
| `crates/rustty/src/main_rustty.rs:299` | GUI launch leaf | DOCTRINE | Reached only for a configured GUI runtime with **no compiled backend** (e.g. Linux `--no-default-features`). The GTK arm above it is LIVE (`run_gtk`, diverging), and macOS is `.none`. Guarded by `crates/rustty/tests/gui_leaf.rs`, which asserts the Gtk arm does **not** reach it. |
| `crates/rustty-termio/src/backend.rs:180` | `Backend.childExitedAbnormally` | DOCTRINE | Uninstantiated in the reference: `termio.Exec` declares no `childExitedAbnormally`, and Zig's lazy compilation means the oracle's own delegation has **never been type-checked**. The marker is a tripwire that fires if a caller appears first. |
| `.../gtk/class/application.rs` ×8, `rustty-core/src/lib.rs` ×10, `rustty-core/src/app.rs` ×8, `rustty-core/src/renderer_seam.rs` ×13, `rustty-core/tests/surface_visibility_kat.rs` ×8 | `"test double: not exercised"` etc. | DOCTRINE | `#[cfg(test)]` / test-only backend dummies. Every one carries a message naming the test that does not drive it. Not port debt. |

### `TODO` / `FIXME` / `XXX:` / `HACK` — 172 comments

| class | count | disposition |
|---|---|---|
| ORACLE (carried verbatim for R1) | 145 | **keep** — deleting any is an R1 regression |
| ORACLE, meta-commentary about an oracle marker | 27 | **keep** — see below |
| rustty-authored genuine debt | **0** | — |

The 27 that the forward matcher did not resolve are all false negatives of the matcher,
verified by hand. They fall into two shapes:

1. **Vendored-dep markers** the matcher's corpus reached but whose surrounding line
   differs — e.g. `panic!("TODO: sendmsg with buffer")` in `rustty-xev/src/{epoll,io_uring}.rs`
   is verbatim `libxev/src/backend/{epoll,io_uring}.zig:{706,489}`.
2. **Prose *about* an oracle marker** — port notes that quote or explain an upstream
   TODO rather than reproduce it, e.g. `rustty-apprt/src/gtk/portal/open_uri.rs:15`
   ("Unlike `GlobalShortcuts` — where the reference carries an unimplemented TODO —
   `OpenURI.zig` DOES additionally re-point the subscription … and so does this port"),
   `rustty-renderer/src/row.rs:4`, `rustty-cli/src/list_themes.rs:193`,
   `rustty-apprt-api/src/action_tests.rs:7`.

**There is no rustty-authored `TODO` anywhere in `crates/`.** Every marker in the tree
traces to the oracle or to a vendored Zig dependency.

### `#[ignore]` — 70 attributes

| class | count |
|---|---|
| with an argued reason in the preceding comment, or an inline `= "reason"` | 70 |
| **undocumented** | **0** |

Checked mechanically (`/tmp/le/ign.py`: any `#[ignore]` with no `//` comment in the 12
lines above it). Zero hits. The reasons cluster into: needs a live display
(`xvfb-run … -- --ignored`), needs a live device/WindowServer, needs a Windows/WASI host,
and one *load-bearing* honesty ignore at
`crates/rustty-vt/src/compress/lz4_differential.rs:593` — Zig's `error.SkipZigTest`
reports SKIPPED while a Rust early-return reports PASSED, so `#[ignore]` is what keeps
`cargo test` output from lying about a budget that did not run.

### `unreachable!()` — 761

Not swept individually. Spot-checking found no stand-in-for-unported-logic pattern; the
overwhelming majority are ports of Zig's own `unreachable` (which is an assertion in Zig,
not a TODO). Flagged as **not audited** rather than claimed clean.

### `PLACEHOLDER` / `STUB` / `NOT PORTED` / `DEFERRED` — 26

All argued. Two deserve to be called out because they are genuine "the oracle has code and
we do not" — **recorded, deliberately not ported**:

| site | oracle | why not ported |
|---|---|---|
| `crates/rustty-renderer/src/opengl/gl/buffer.rs:13` | `Buffer.zig:139-179` `Binding.attribute` | The oracle's body ends in `try b.enableAttribArray(idx)` — and `Buffer.Binding` **has no such method** (only `VertexArray.Binding` does). It has zero call sites, so Zig's lazy compilation means it has never been compiled. Porting it means inventing behaviour it never had. Its three helpers (its only real callers) ARE ported; the live path is `VertexArray.Binding`, fully ported. |
| `crates/rustty-renderer/src/opengl/gl/program.rs:12` | `Program.zig:90-125` `setUniform` | `value: anytype` dispatched over seven unrelated types, **zero call sites in the entire reference**. Rust cannot express it without inventing a `UniformValue` trait, and there is no oracle to validate the invented shape against. The live uniform path is the UBO one, already ported. |

Both are recoverable with zero churn the moment a call site names the arms it needs. They
are the honest residue of this sweep, not oversights.

---

## REAL GAPs found and FIXED

All four came from the **reverse** check — oracle markers absent from rustty — which
turned out to be a much better gap detector than the forward one. Three of them are
behavioural, not cosmetic.

### 1. `Environ::contains_unempty_constant` / `contains_constant` missing — FIXED

`crates/rustty-global/src/environ.rs`

The oracle calls `global.environ().containsUnemptyConstant(…)` at two sites
(`Config.zig:5183`, `crash/sentry.zig:113`). `std/process/Environ.zig:589` defines it as
**present AND non-empty**. rustty's `Environ` had `get_posix`/`get_alloc`/`create_map` but
neither predicate, so both call sites had been open-coded — wrongly (see 2 and 3).

Added both, byte-typed (the oracle's `comptime key` exists only so the Windows arm can
widen a literal to WTF-16; rustty's block is byte-oriented on every target, so the key
stays a runtime `&[u8]`).

Tests: `environ_contains_constant_vs_unempty_constant`,
`environ_contains_unempty_constant_respects_first_entry_wins`.

**Mutation proof.** Injected `self.get_posix(key).is_some()` (dropping the emptiness
check) in place:

```
test environ::tests::environ_contains_unempty_constant_respects_first_entry_wins ... FAILED
test environ::tests::environ_contains_constant_vs_unempty_constant ... FAILED
  assertion failed: !e.contains_unempty_constant(b"EMPTY")
test result: FAILED. 6 passed; 2 failed;
```

Reverted in place (edit, not `mv`); `test result: ok. 8 passed; 0 failed`.

### 2. `TERM_PROGRAM` read the process env, not the captured global environ — FIXED

`crates/rustty-config/src/config_struct.rs` — `probable_cli_environment`

```rust
- if let Ok(v) = std::env::var("TERM_PROGRAM") { if !v.is_empty() { return true; } }
+ if rustty_global::environ::environ().contains_unempty_constant(b"TERM_PROGRAM") {
+     return true;
+ }
```

Oracle: `if (global.environ().containsUnemptyConstant("TERM_PROGRAM")) return true;`

Two divergences, both real:

- **Wrong source.** `global.init(.{ .c = … })` installs a **caller-supplied** environ
  block (rustty: `rustty_global::environ::set_environ`). Every libghostty embedder that
  passes its own environ was having this heuristic silently read the host process's
  environment instead. The heuristic gates default-shell and working-directory
  resolution, so this is user-visible.
- The commented reference body was **STALE** — it still quoted the pre-`global.environ()`
  `std.posix.getenv("TERM_PROGRAM")` shape. Replaced with the current oracle line, and
  the oracle's own WASI `TODO` (dropped in the original port) restored above it.

### 3. `SHELL` read via `std::env::var` — lossy for non-UTF-8 — FIXED

`crates/rustty-config/src/config_struct.rs` — default-shell resolution

```rust
- if let Ok(value) = std::env::var("SHELL") { … Command::Shell(value.into_bytes()) }
+ let Some(value) = rustty_global::environ::environ().get_alloc(b"SHELL") else {
+     break 'shell_env;
+ };
+ … Command::Shell(value)
```

Oracle: `global.environ().getAlloc(alloc, "SHELL")`, whose result is a `[]u8`.

Same wrong-source problem as (2), **plus** a byte-fidelity violation: `std::env::var`
returns `Err` on a non-UTF-8 value, so a shell at a non-UTF-8 path was silently dropped
and the config fell through to the passwd lookup. This is exactly the
`zig-byteslice-to-rust-rawbytes` rule — `SHELL` is a byte slice upstream and must not
round-trip through `String`. The log line now goes through `String::from_utf8_lossy`
(display only); the stored `Command::Shell` keeps the raw `Vec<u8>`.

The commented reference body was likewise STALE (`std.process.getEnvVarOwned`) and is
replaced with the current oracle text.

### 4. `XDG_CACHE_HOME` presence check had the wrong semantics — FIXED

`crates/rustty-crash/src/sentry.rs` — Sentry cache-dir selection, macOS arm

```rust
- if std::env::var_os("XDG_CACHE_HOME").is_none() {
+ if !rustty_global::environ::environ().contains_unempty_constant(b"XDG_CACHE_HOME") {
```

Oracle: `if (global.environ().containsUnemptyConstant("XDG_CACHE_HOME")) break :macos;`

With `XDG_CACHE_HOME=""` set, the oracle treats it as unset and uses
`NSCachesDirectory`; rustty took the XDG path and wrote crash dumps somewhere else.
Same captured-environ correction as (2)/(3). Commented reference body was STALE
(`std.posix.getenv`).

---

## Undocumented hole found masquerading as doctrine — FIXED

`crates/rustty-global/src/lib.rs:314` carried this PORT NOTE:

> the `.c` arm's `return error.UnsupportedOSForCApi` on Windows is carried by
> `rustty-apprt-c`'s `rustty_init`, which owns the argc/argv seam.

**It was not.** `grep` over `crates/rustty-apprt-c` for `UnsupportedOSForCApi`,
`UNICODE_STRING`, or any Windows guard: zero hits. `rustty_init` had no
`#[cfg(target_os = "windows")]` arm at all. The deferral note was pointing at code that
did not exist — exactly the "derived-gate green is bookkeeping" failure mode.

Fixed by making the claim true: added the guard to `rustty_init`
(`crates/rustty-apprt-c/src/lib.rs`) as a `#[cfg(target_os = "windows")]` early
`return 1` with the same logged message the oracle's `ghostty_init` produces when
`state.init()` fails, and quoted the oracle's UNICODE_STRING `TODO` at that site.
Narrowed the `rustty-global` note to say precisely what now exists.

Caveat, stated plainly: this arm **cannot be compiled or run on this host** (macOS).
It is trivially correct by inspection (an early `return 1` with a `let _ = (argc, argv)`),
but it is unverified. Recorded here rather than claimed green.

---

## Oracle markers restored (R1 comment fidelity)

The reverse check found 39 oracle marker lines with no counterpart in rustty. After
discarding false positives — test-data strings (`"aXXXXfg"`, `"xxxyy😀"`), `build/*.zig`
(build system, not ported), `HACKING.md` prose, Zig-`std` `segmented_list.zig` (not
ported; rustty uses a `Vec`-backed `SegmentedPool`), `lib/compat/thread.zig` (a Zig 0.16
shim with no Rust analogue) — **13 were genuine drops in ported code**. The surrounding
code is ported in every case; only the marker was lost. All restored verbatim:

| oracle | rustty | marker |
|---|---|---|
| `Command.zig:18` | `rustty-termio/src/command.rs` module doc | `TODO: This may have changed a lot now with the new I/O implementations in >= 0.16.0` |
| `gtk/class/tab.zig:219` | `.../class/tab.rs` (initial-surface OOM) | `TODO: We should make our "no surfaces" state more aesthetically pleasing` |
| `gtk/class/split_tree.zig:1174` | `.../class/split_tree.rs` `on_idle` | the 8-line `This is a hack, it relies on the timing of property notifcations` block |
| `gtk/class/command_palette.zig:585` | `.../class/command_palette.rs` `new_jump` | `TODO: Replace with surface id whenever Rustty adds one` (R2-renamed) |
| `gtk/winproto/x11.zig:359` | `.../winproto/x11.rs` `get_motif_hints` | `FIXME: Maybe we should switch to libxcb one day.` |
| `gtk/class/application.zig:2738` | `.../class/application.rs` `toggle_split_zoom` | `TODO: pass surface ID when we have that` |
| `gtk/class/surface.zig:882` | `.../class/application.rs` `toggle_command_palette` | `TODO: pass the surface with the action` |
| `gtk/class/window.zig:240` | `.../class/window.rs` `surface_init` field | `Kind of hacky to have this but this lets us know…` (7 lines) |
| `gtk/class/window.zig:377` | `.../class/window.rs` action table | `TODO: accept the surface that toggled the command palette` |
| `gtk/class/window.zig:2072` | `.../class/window.rs` `toggle_command_palette` | same, `as a parameter` |
| `gtk/class/window.zig:2127` | `.../class/window.rs` `action_toggle_command_palette` | same |
| `gtk/class/window.zig:2144` | `.../class/window.rs` `action_toggle_inspector` | same |
| `renderer/metal/shaders.zig:227` | `rustty-renderer/src/metal/gpu_abi.rs` | `TODO: Maybe put these in a packed struct, like for OpenGL.` |
| `font/SharedGrid.zig:256` | `rustty-font-shaper/src/shared_grid.rs` | `FIXME: This gets a massive manual error set for now…` |
| `font/main.zig:47` | `rustty-font-shaper/src/lib.rs` `options` | `TODO: we need to modify the build config for wasm builds.` |

Deliberately **not** restored (attributed paraphrases already present, restoring verbatim
would duplicate):

- `gtk/class/surface.zig:1632` "This is a hack because it ties ourselves (optionally) to
  the Window class" — `surface.rs:2381` already restates it with attribution and the
  reason it is punted.
- `gtk/class/global_shortcuts.zig:225,428` — paraphrased at `global_shortcuts.rs:794,568`.
- `renderer/opengl/RenderPass.zig:61` — paraphrased at `opengl/render_pass.rs:116`.
- `renderer/OpenGL.zig:408,413` — carried at `opengl/opengl_impl.rs:604,608` with a
  `(ghostty)` attribution tag.
- `renderer/OpenGL.zig:173,212,232` — the `apprt.embedded` no-op arms of
  `threadEnter`/`threadExit`/`displayRealized`. rustty's OpenGL impl does not switch on
  `apprt.runtime` at all (documented at `opengl_impl.rs:39-43`), so there is no site to
  attach these to. Structural deviation, already argued.

## Left alone, deliberately

- **145 + 27 ORACLE `TODO`s.** Carrying them is the point. Not touched.
- **62 `unimplemented!()` sites.** All argued; not touched.
- **761 `unreachable!()`.** Not individually audited — see above. This is the one class in
  this sweep where "clean" is unproven rather than proven.
  **SUPERSEDED: audited in APPENDIX A (1 defect found and fixed).**
- **`Binding.attribute` / `setUniform`.** Recorded above; porting either means inventing.

## Verification

- `cargo check --workspace --all-targets` — clean.
- `cargo test -p rustty-global --no-fail-fast` — 8 lib + 1 integration pass.
- `cargo test -p rustty-config --no-fail-fast` — 197 pass.
- `cargo test -p rustty-crash -p rustty-apprt-c -p rustty-termio --no-fail-fast` — all
  pass. (`rustty-apprt-c --test smoke` fails on a cold `target/` because it needs the
  cdylib published first; `cargo build -p rustty-apprt-c` then re-run → pass.
  Pre-existing, environmental, not from this branch.)
- `cargo fmt --all` applied.

---

# APPENDIX A — the `unreachable!()` class, audited (P3 `unreachable-audit` lane)

Branch: `unreachable-audit`. Oracle pin: `../ghostty` @ `6ad1fe7d8`.

The sweep above left this class explicitly unproven ("761 `unreachable!()`. Not
individually audited"). It is now audited. **One real defect found and fixed**
(`rustty-vt/src/tmux/control.rs`); everything else is faithful, and the residue that is
*not* proven is enumerated at the bottom rather than waved through.

## Scope correction: the brief's grep undercounts by 74

`grep -rn "unreachable!()"` returns **687**, not 761. The 761 figure is
`grep -rn "unreachable!("` — the difference is **74 message-carrying sites**
(`unreachable!("...")`), which the `()` grep silently drops. Those 74 are the same class
and were audited too. Three further "sites" in the 687 are comment/doc prose, not code
(`rustty-vt/src/style.rs:274`, `rustty-renderer/src/cell.rs:245`,
`rustty-renderer/src/opengl/gl/shader.rs:92`).

## Partitions and the rule used for each

| # | partition | rule | n | verdict |
|---|---|---|---|---|
| P0 | **test code** | site is inside a `mod tests` block (brace-matched), a `tests.rs`, or under a `#[test]`/test-helper fn | **301** | FAITHFUL |
| P1 | **prod, `_ =>` catch-all of a 1-real-arm match** | a one-prong match is never written in Zig; it is `x.foo` on a `union(enum)` — a safety-checked tag assert | **143** | FAITHFUL |
| P2 | **prod, not a `_ =>` arm** (named-variant arm, bare stmt, `unwrap_or_else(\|_\| unreachable!())`) | pair each site with its oracle line and read it | **156** | 152 FAITHFUL / **1 DEFECT** / 3 prose |
| P3 | **prod, `_ =>` catch-all of a MULTI-arm match** — the highest-risk shape | pair each site with its oracle `else =>` prong and read it | **87** | FAITHFUL |
| P4 | **message-carrying `unreachable!("…")`** (missed by the brief's grep) | as P2/P3 | **74** | FAITHFUL |

Corroborating mechanical signal: **111 of the 386 production sites carry the literal
oracle line (`// else => unreachable,` / `// … catch unreachable`) as a reference comment
within 8 lines above** — direct proof, no inference needed. The other 275 required reading
the oracle.

Two sub-classes were pulled out and audited exhaustively because they are the most
dangerous semantically:

- **Error-path panics** — `Err(_) => unreachable!()` and
  `.unwrap_or_else(|_| unreachable!())`. **11 sites, all 11 hand-verified.** Every one is
  either a literal oracle `catch unreachable` (`Surface.zig` `threadEnter`/`queueRender`,
  `os/file.zig:126` `randomBasename`, `os/cf_release_thread.zig:189`,
  `terminal/c/selection.zig:204`) or a Rust-only move-semantics/`Vec::try_into` artifact
  with no oracle counterpart and no reachable path.
- **`_ =>` over a non-exhausted enum in a hot parse path** — checked against the oracle
  switch in every case; see the DEFECT below, which is exactly this shape.

## The defect (found, fixed, mutation-proven)

**`crates/rustty-vt/src/tmux/control.rs:315`** ← `ghostty/src/terminal/tmux/control.zig`
(eight sites: `:228, :259, :299, :337, :367, :398, :433, :459`, all falling into the
shared cleanup at `:481-485`).

The oracle, on an Oniguruma **regex non-match**, does:

```zig
var region = re.search(line, .{}) catch |err| {
    log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", ...);
    break :cmd;                       // -> buffer.clearRetainingCapacity();
};                                    //    state = .idle; return null;
```

rustty's `parse_notification_oni` correctly returns `Ok(None)` on search failure, but the
caller's `else if is_oni_command(cmd)` arm was split by `#[cfg]`: the
`#[cfg(not(feature = "oniguruma"))]` half did the right cleanup, while the
`#[cfg(feature = "oniguruma")]` half was `unreachable!()` with the comment "Handled by
`parse_notification_oni`; unreachable here" — **false**, since only the *successful*-match
path is handled there.

Impact: with `--features oniguruma` (the faithful configuration — upstream Ghostty builds
*with* oniguruma), a malformed `%output` / `%window-add` / `%client-session-changed` line
arriving from the PTY turned a logged, recoverable no-op into a **panic driven by
untrusted input**. This is precisely the failure shape this lane was hunting: an oracle
arm that does something real, replaced by a panic.

Fix: the arm is now unconditional (`buffer.clear(); state = Idle; Ok(None)`) — the
reference's `break :cmd`. No silent fallback was introduced; this *is* the oracle's
behaviour.

Regression test: `tmux::control::tests::tmux_oni_command_regex_mismatch_returns_to_idle`
(not feature-gated — the behaviour must be identical in both builds; asserts no
notification, empty buffer, `State::Idle`, and that the parser still works afterwards).

Mutation proof — fault re-injected (the old `#[cfg(feature = "oniguruma")] { unreachable!() }`),
`cargo test -p rustty-vt --features oniguruma --lib tmux_oni_command_regex_mismatch`:

```
running 1 test
test tmux::control::tests::tmux_oni_command_regex_mismatch_returns_to_idle ... FAILED

---- tmux::control::tests::tmux_oni_command_regex_mismatch_returns_to_idle stdout ----
thread '...' panicked at crates/rustty-vt/src/tmux/control.rs:325:17:
internal error: entered unreachable code

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2316 filtered out
```

Reverted in place (edit, not `mv`) + `touch`; test back to `ok`.

## Suspects raised and then cleared

- `rustty-xev/src/net.rs:267`, `rustty-xev/src/linux/net.rs:266` — flagged unverifiable
  (Zig 0.16's std dropped `std/net.zig`). Cleared against
  `zig@0.15/0.15.2/lib/zig/std/net.zig:227-248`, which is a literal
  `else => unreachable` with the identical `AF.UNIX` comment block. FAITHFUL.
- `rustty-vt/src/terminal.rs:1850` (`match width`) ← `Terminal.zig:1466` `else => unreachable`.
  `rustty-vt/src/screen.rs:5101` ← `Screen.zig:3775` `else => unreachable`. Both FAITHFUL —
  checked because a dropped `0 =>` / zero-width prong here would be a live print-path bug.
- `rustty-core/src/renderer_seam.rs:1717` — reported as a "dropped prong" vs
  `Thread.zig:711-720` (`error.NotFound => {}` + `else => { log.warn; unreachable; }`).
  Not a gap: rustty's `xev::CancelError` on this platform is `enum { Canceled }`
  (`kqueue.rs:2399`), a single variant which *is* the success case, so the other prongs are
  uninhabitable. Re-expand when D6 lands; the reference is preserved verbatim in-comment.
- `rustty-input/src/binding.rs:4137` (`Value.format` has no `leaf_chained` arm) — the
  ORACLE's `Value.format` switch (`Binding.zig:2121-2137`) genuinely omits the
  `.leaf_chained` prong while `formatEntries` handles it. There is no oracle behaviour to
  port; Rust just requires the arm to be spelled.

## Recorded, not fixed

- **`rustty-core/src/surface.rs:5959`** — `get_content_scale().unwrap_or_else(|_| unreachable!())`
  against `Surface.zig:2530`, where Zig's bare `try` in a `void` fn type-checks only because
  the comptime-selected apprt's error set is *empty* (`apprt/embedded.zig:650`). rustty's
  seam trait returns an opaque `ApprtError`, so a compile-time guarantee became a runtime
  panic. Harmless today; **must be revisited when a fallible apprt backend (GTK / live
  macOS) lands**, where Zig would refuse to compile rather than panic.
- **`rustty-renderer/src/generic.rs:5006` (+ `:5021`, `:5036`)** faithfully reproduce a
  **latent upstream Ghostty bug**: `generic.zig:2067/2077/2087` do `cursor_text.color.r` on
  the 3-variant `TerminalColor`, so `cursor-text = cell-foreground` (a valid config value
  handled correctly at `generic.zig:2527`) panics in ReleaseSafe. The port documents this
  and chose the panic over silently skipping. Correct under R1 — **report upstream, do not
  fix locally**.
- **Dropped `log.warn`/`log.err` emission around `unreachable`** — e.g. `Exec.zig:1672/1792/1809`
  → `exec.rs:4559/4807/4844` panic without the preceding log line. This is the tree-wide
  `std.log.scoped` facade deferral (`DEFERRALS.md:2054-2057`), not an `unreachable!()`
  defect: the *control flow* is preserved in every case, only the string output is gone.
  `rustty-vt` alone has 122 such elisions in `stream.rs`; `rustty-config`/`rustty-termio`
  *do* have working log macros, so the elision is crate-local and inconsistent. Worth a
  decision by the logging lane, out of scope here.
- **`rustty-termio/src/command.rs`** — the `unreachable!()`s are faithful, but the
  surrounding errno mapping is stale against oracle HEAD (`Command.zig:436-458` now routes
  the mac `dup2` + both `fcntl` calls through `PosixCall.f` with a full errno table:
  `BUSY→FileBusy` rather than retry, `INTR` retry). Upstream-sweep item, not this class.

## What remains unchecked — be blunt

- **113 of the 143 P1 sites were bulk-classified, not opened.** The rule (each arm-pattern
  maps 1:1 onto one of 31 libxev constructs that *were* read in full) is sound and the 30
  opened sites include every one whose single arm was not a plain payload destructure, but
  the residual risk is concentrated in `rustty-xev/src/io_uring.rs` (40 unopened callback
  trampolines).
- **P0 (301 test sites) was verified by a 25-site random sample, not exhaustively.** 25/25
  were genuine test code or test helpers. A defect hidden in a test-only `unreachable!()`
  would weaken a test, not ship a bug.
- The reverse check (oracle `else =>` prongs with real bodies vs. their rustty counterpart)
  covered **562 non-trivial survivors of 784 raw hits**, of which ~90 were individually
  traced and the rest categorized by body shape. **~470 were covered only at the
  category/pattern level**, heaviest in `apprt/gtk/**`, `cli/**`, `build/**`, `extra/**`,
  and the `expr`/`capture` prongs of `terminal/kitty/graphics_command.zig` and
  `terminal/c/**`.
- **Only `unreachable!()` was audited.** `panic!`, `.unwrap()`, `.expect()`, and `assert!`
  can hide the identical defect shape and were not swept. A 40-site spot-sweep of
  `_ => panic!/todo!/unimplemented!` found all but ~4 in `#[cfg(test)]` helpers and the
  production ones faithful to oracle `@panic`, but that is a sample, not a sweep.

## Verification (this lane)

- `cargo check --workspace --all-targets` — clean.
- `cargo test -p rustty-vt --no-fail-fast` — 2303 lib + 26 integration pass, 0 fail.
- `cargo test -p rustty-vt --features oniguruma --no-fail-fast` — 2314 lib + 26 pass, 0 fail.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean;
  `cargo clippy -p rustty-vt --features oniguruma --all-targets -- -D warnings` — clean.
- `cargo fmt --all` applied.

---

# APPENDIX B — untrusted-input panic sweep + the two Appendix-A coverage gaps (P3 `panic-paths` lane)

Branch: `panic-paths`. Oracle pin: `../ghostty` @ `6ad1fe7d8`.

Appendix A closed `unreachable!()` and named three things it did not prove. This lane
closes two of them and sweeps the class it explicitly excluded (`panic!` / `.unwrap()` /
`.expect()` / `assert!`), working **backwards from the untrusted inputs** rather than
auditing every `.unwrap()` in the tree.

**One real defect found and fixed** (`rustty-config` → `rustty-renderer` `font_features`),
one **latent upstream bug confirmed reachable** and deliberately left faithful, three
non-panic fidelity deltas recorded, and the io_uring + reverse-check gaps closed with hard
numbers.

## The defect (found, fixed, mutation-proven)

### `RendererConfig::font_features` aborted on a config file containing a NUL byte

`crates/rustty-config/src/renderer_config.rs` (impl) +
`crates/rustty-renderer/src/config.rs` (trait) + `generic.rs` / `generic/linux_impl.rs` /
`test_support.rs` (consumers).

The accessor was:

```rust
fn font_features(&self) -> Vec<CString> {
    // config `RepeatableString.list` is `Vec<Vec<u8>>` of non-NUL bytes
    // (value_types.rs `value.to_vec()`), so `CString::new` never hits an interior
    // NUL — the `.expect` is safe (RISK-5, adversary-CONFIRMED).
    self.font_feature.list.iter()
        .map(|s| CString::new(s.clone()).expect("font-feature contains no interior NUL"))
        .collect()
}
```

**The comment's claim is false, and the "adversary-CONFIRMED" tag was bookkeeping.**
`RepeatableString::parse_cli` (`crates/rustty-config/src/value_types.rs:2869`) stores the
config line's value **verbatim** — `self.list.push(value.to_vec())` — with no NUL
rejection anywhere on the path. Probed empirically before any edit: a config file line
`font-feature = a<0x00>b` lands in the list as `[97, 0, 98]` and `font_features()` aborts.

Oracle: `Config.zig:249` `@"font-feature": RepeatableString`, cloned at
`renderer/generic.zig:597` into `[:0]const u8` items, whose **only** consumer is
`font/shaper/harfbuzz.zig:68` → `FeatureList.appendFromString(alloc, feature_str)`, which
reads the Zig slice **by length**, never as a C string. `dupeZ` keeps the interior NUL and
appends a sentinel; `appendFromString` fails to match a feature tag and the entry is
dropped. **The reference recovers. rustty aborted.** The call site is `DerivedConfig::init`
(`crates/rustty-renderer/src/generic.rs:480`), i.e. **renderer init** — a hostile or merely
mangled config/theme file crashed the terminal at startup.

Fix: carry the **raw bytes** — `fn font_features(&self) -> Vec<Vec<u8>>` — which is also
what the four downstream consumers immediately reconstructed by hand
(`.map(|c| c.as_bytes().to_vec())`), so the `CString` round-trip was pure loss.

**Truncating at the first NUL was rejected as a silent fallback**: `liga\0junk` would then
silently ENABLE `liga`, which the reference does not do. No invented behaviour; this is the
oracle's.

Regression test: `renderer_config::tests::font_features_carry_interior_nul_bytes`. It feeds
the actual malformed config **text** through `LineIterator` + `Config::load_iter` (not the
internal list), and asserts the bytes survive intact.

**Mutation proof.** Fault re-injected in place (the `CString::new(...).expect(...)`),
`cargo test -p rustty-config --lib font_features_carry_interior_nul`:

```
running 1 test
test renderer_config::tests::font_features_carry_interior_nul_bytes ... FAILED

---- renderer_config::tests::font_features_carry_interior_nul_bytes stdout ----
thread '...' panicked at crates/rustty-config/src/renderer_config.rs:125:22:
font-feature contains no interior NUL: NulError(4, [108, 105, 103, 97, 0, 106, 117, 110, 107])

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 197 filtered out
```

Reverted in place (edit, not `mv`) + `touch`; back to `test result: ok. 1 passed`.

## Latent UPSTREAM bug, confirmed reachable, deliberately NOT fixed

### tmux control-mode pane/session id overflow

`crates/rustty-vt/src/tmux/control.rs:384, 421, 458, 502, 538, 575, 576, 649` —
`parse_int_usize(&line[starts[1]..ends[1]]).unwrap()`.

Oracle `terminal/tmux/control.zig:233-237` (and its seven siblings):

```zig
const id = std.fmt.parseInt(
    usize,
    line[@intCast(starts[1])..@intCast(ends[1])],
    10,
) catch unreachable;
```

The regexes are `^%output %([0-9]+) (.+)$` etc. — the digit run is **unbounded**, so a
capture that matches can still overflow `usize`. Confirmed reachable with a throwaway probe
(added, run, then removed — no test left in the tree):

```
test tmux::control::tests::PROBE_overflow_pane_id ... FAILED
thread '...' panicked at crates/rustty-vt/src/tmux/control.rs:384:83:
called `Option::unwrap()` on a `None` value
```
(input: `%output %99999999999999999999999999 x\n`, `--features oniguruma`)

**FAITHFUL, not fixed.** The oracle's `catch unreachable` panics in ReleaseSafe and is **UB
in ReleaseFast** (Ghostty's ship mode), where `id` would hold garbage. rustty's panic
matches ReleaseSafe and is strictly safer. Same disposition as the `renderer/generic.rs`
`cursor-text = cell-foreground` case in Appendix A: **report upstream, do not fix locally**
— a local "fix" would be inventing recovery the reference does not have. No comment was
added at the site (the G3b comment lane owns `crates/rustty-vt/**` comments); recorded here
instead.

## Coverage gap 1 — the 113 bulk-classified P1 sites / io_uring — CLOSED

`crates/rustty-xev/src/io_uring.rs` has **63** `unreachable!()` sites, not the ~40 Appendix
A estimated. **All 63 opened individually and paired with libxev**
(`ghostty/zig-pkg/libxev-0.0.0-86vtcwIRFADbH4hk-EjROXxlrKIRPQdA41XiTSytYO-F/src`).

**63 FAITHFUL, 0 divergences, 0 unlocatable.** Every one shadows a Zig union-payload access
(`c.op.X.y` / `r.X`), a literal `else => unreachable`, or a test-local `catch unreachable`.
Strongest single match: rustty `:2080` `Operation::Noop => unreachable!()` ←
`backend/io_uring.zig:665` `.noop => unreachable,` in the identical switch. Groups: loop
internals (3), async watcher (2), process wait (2), generic stream mixins replicated per
watcher type (44 — read/recv/write/send/close/poll/pread/pwrite/accept/connect/shutdown/
recvmsg/sendmsg, `watcher/{stream,tcp,udp,file}.zig`), test-only callbacks (11, each a
literal `catch unreachable` in the oracle's own test block).

**The residual risk Appendix A flagged for this file did not materialise.** The
bulk-classification rule was sound.

Two **non-panic** notes surfaced while reading:

- `io_uring.rs:2894` collapses `Result::Poll(Err(_)) => Err(ProcessWaitError::Unexpected)`
  where Zig propagates the actual `PollError` (`Canceled` stays `Canceled` via error-set
  coercion into `WaitError`). Error-value fidelity loss, not a panic path.
- `io_uring.rs:3563/4431/5056/5936` hardcode `Ok(PollEvent::Read)` where Zig does
  `@enumFromInt(c.op.poll.events)`. Sound today (all four call sites hardcode
  `libc::POLLIN`, `io_uring.rs:3527-3530`, and `PollEvent` has only `read` upstream);
  silently breaks if a POLLOUT variant is ever added.

## Coverage gap 2 — the reverse `else =>` check — TIGHTENED

Rule: `grep -rn "else =>" --include=*.zig` over the named areas; discard as **trivial**
`else => unreachable` / `{}` / `null` / a bare literal / `@compileError` / a bare comptime
type expression.

| oracle area | raw | trivial | non-trivial |
|---|---|---|---|
| `apprt/gtk/**` | 41 | 15 | **26** (4 in `apprt/gtk/build/blueprint.zig`) |
| `cli/**` | 44 | 10 | **34** |
| `build/**` | 24 | 7 | **17** |
| `terminal/c/**` | 27 | 6 | **21** |
| `extra/**` | 12 | 3 | **9** |
| `terminal/kitty/graphics_command.zig` | 18 | 4 | **14** |
| total | 166 | 45 | **121** |

`src/build/**` is the Zig build system and is **not ported** — confirmed: no rustty crate
mirrors it (`rustty-build-config` maps to `src/build_config.zig`, a different file;
`GhosttyXcodebuild|MetallibStep|SharedDeps|GitVersion|GhosttyResources` hit only Cargo
`build.rs` files). Same for `apprt/gtk/build/blueprint.zig` (blueprint-compiler driver).
**Excluded: 17 + 4 = 21 → 100 in scope.**

**75 of 100 individually verified; 25 remain category-level; 0 DROPPED; 0
REPLACED-BY-PANIC.**

- `terminal/kitty/graphics_command.zig` — **14/14**, all REPRODUCED (`:125,:136,:210,:219,
  :292,:480,:491,:527,:630,:638,:712,:774,:805,:1000` → `graphics_command.rs:162,176,268,
  277,364,722,736,775,886,894,986,1051,1087,1346`). Every `unreachable!()` in that file is
  at `:1358`+, inside `#[cfg(test)]`.
- `terminal/c/**` — **13/21**. All REPRODUCED. `rustty-vt-c/src/osc.rs:230-238` in fact
  *strengthens* the oracle: it takes the raw `c_int` and returns `false` with a warning for
  out-of-range, matching the oracle's `runtime_safety` guard unconditionally.
- `cli/**` (→ `rustty-cli` + `rustty-args`) — **22/34**. All REPRODUCED, including all four
  `CommaSplitter.zig` `error.IllegalEscape` prongs and `list_themes.zig:146`'s
  open-failure `continue`. The only non-test panic in either crate is
  `ssh_cache.rs:715 _ => unreachable!()` (days-in-month; month is 1–12 by construction and
  the `std.time.epoch` original is also `unreachable`).
- `apprt/gtk/**` — **22/22** (not sampled: after removing trivial + blueprint prongs only 22
  remained, 13 of them the homogeneous `winproto.zig` union-dispatch group). The two
  highest-risk candidates are both faithfully reproduced *including the recovery*:
  `class/global_shortcuts.zig:466` `log.warn("unrecognized response code")` →
  `global_shortcuts.rs:544`, and `portal/OpenURI.zig:495` `log.err` **+
  `openUrlFallback(...)`** → `open_uri.rs:515-518`. `crates/rustty-apprt/src/gtk/**`
  contains **zero** non-test `_ => unreachable!/panic!/todo!` arms.
- `extra/**` — **4/9** (spot sample of the three distinct `bash.zig` codegen shapes plus one
  `zsh` fallback). All REPRODUCED.

Two **non-panic** semantic deltas found here and recorded, not fixed:

1. `apprt/gtk/class/application.zig:1129` — oracle `else => |e| return e` **propagates** a
   non-`FileTooBig` css read error out of `syncAppearance`;
   `crates/rustty-apprt/src/gtk/class/application.rs:1216-1224` logs and `continue`s to the
   next path. rustty is *more* forgiving. `crates/rustty-apprt/**` is another lane's file —
   handed over, not touched.
2. `terminal/c/terminal.zig:583`/`:917` — the oracle re-validates the incoming enum via
   `std.enums.fromInt` under `runtime_safety` and returns `.invalid_value` + a warning.
   `rustty-vt-c/src/terminal.rs:1265`/`:2209` take a typed Rust enum, so an out-of-range C
   value is UB rather than a graceful `invalid_value`. `osc.rs` got this right; this is
   inconsistent with it. Debug-only upstream, so low severity, but it is a lost recovery
   path.

## The untrusted-input sweep — surfaces, method, results

Method: a scanner that strips `mod *test*` blocks and `#[cfg(test)]`/`#[test]` items by
**rustfmt-indentation block matching**. An earlier brace-counting version was wrong —
braces inside string/char literals made one `mod key_tests` span swallow all 12 361 lines
of `config_struct.rs`; the corrected scanner finds 12 production sites in that file where
the naive one found 0. Files named `tests.rs` / `*_tests.rs` / `bench.rs` excluded.
Patterns: `panic!(`, `.unwrap()`, `.expect(`, `assert!(`, `assert_eq!(`, `assert_ne!(`,
`debug_assert`.

Production-code panic-site counts, and what was done:

| surface | prod sites | how swept |
|---|---|---|
| `rustty-vt/src/**` (all) | 628 | full inventory; the **PTY-facing subset** (`parser`, `stream`, `stream_terminal`, `csi`, `sgr`, `dcs`, `apc/**`, `osc/**`, `tmux/**`, `kitty/**`, `input/**`, `hyperlink`, `clipboard`, `charsets`, `modes`, `utf8_decoder`, `device_*`, `ansi`, `color`, `x11_color`) opened for every non-`debug_assert` site |
| `rustty-config/src/**` | 24 | all 7 `unwrap/expect/panic` opened; **1 DEFECT** (above) |
| `rustty-args/src/**` (the actual config-text parser) | 5 | all opened — clean |
| `rustty-termio/src/**` | 36 | all `unwrap/expect/panic` opened (read-only for `stream_handler.rs`, another lane's file) |
| `rustty-font/**`, `rustty-font-shaper/**`, `rustty-renderer/**`, `rustty-wuffs`, `rustty-zigimg`, kitty PNG/APC decode | ~60 | ~23 opened by rule (parsing/decode path, not a lock `.unwrap()`, not a platform-cfg arm) |
| `rustty-cli/src/**`, `rustty-terminfo/src/**` | ~24 | `unwrap/expect/panic` reviewed |

**Result: 1 divergence (fixed), 1 faithful-but-upstream-buggy site (recorded).** Everything
else opened is FAITHFUL. Load-bearing negatives, each checked against the oracle:

- **The VT stream path is clean.** `apc/mod.rs` `feed`/`feed_slice`/`end` reproduce all six
  oracle `catch |err| { log.warn; p.deinit(); state = .ignore; }` recoveries
  (`apc.zig:101,108,137,146,171,180`) as `is_err()` branches. `dcs.rs:95,190` reproduce
  `dcs.zig:32,115`. `osc/parsers/kitty_color.rs:98` matches `kitty_color.zig:64`'s
  `error{InvalidFormat}`-only switch **exactly** (`RGB.parse`'s error set is
  `error{InvalidFormat}`, `color.zig:657` — the port did not silently widen it).
- **`osc9.rs`'s 17 `capture.as_ref().unwrap()`** are all dominated by the
  `if parser.capture.is_none() { state = Invalid; return None; }` guard at `:14`.
- **`osc/parsers/{context_signal,kitty_clipboard_protocol,kitty_dnd_protocol}.rs`'s 10
  `_ => panic!("… is not a …")`** are Rust-only tag asserts on an enum whose variant is
  chosen by the *field/option key*, not by the parsed value
  (`read_option(Option_::Type).as_type()`). In Zig these are plain struct-field accesses.
  Not input-reachable.
- **`kitty/graphics_{exec,image,storage,command}.rs`'s `.unwrap()`s** are all oracle `.?` or
  `orelse`-guarded: `graphics_exec.zig:141,191,316,320`, `graphics_image.zig:92`,
  `graphics_storage.zig:302-305`, `graphics_command.zig:469` (`.png => unreachable, // Must
  be validated before` — carried verbatim).
- **`crates/rustty-font/src/opentype/{sfnt,head,hhea,os2,post,svg,glyf}.rs` and
  `glyf_rasterize.rs` contain ZERO panic sites outside `#[cfg(test)]`.** This is the most
  attacker-exposed surface in the tree (a malicious `.ttf`) and it is uniformly
  `Result`-based. Same for `rustty-wuffs`'s `png.rs`/`jpeg.rs` and
  `rustty-renderer/src/shadertoy.rs` (custom-shader loading).
- **`font/face/freetype.rs:875` `panic!("unsupported pixel mode")`** *is* reached with a
  `pixel_mode` straight out of an attacker's font file — and is FAITHFUL:
  `font/face/freetype.zig:710-711` is `log.warn(...); @panic("unsupported pixel mode");`.
  The oracle's *other* `pixel_mode` switch (`freetype.zig:594`, `else =>` →
  `error.BitmapHandlingError`) is reproduced as an error return, not a panic.
- **`renderer/generic.rs` `load_background`** reproduces every oracle `log.warn` +
  `break :load_background` as `break 'load_background None`; the two decodes use `?`,
  matching the oracle's `try`. No `.unwrap()` in the block.
- **The `#[cfg(feature = …)]`-split panic shape that produced the Appendix-A tmux defect is
  now unique.** A tree-wide scan (any `#[cfg(…feature…)]` with a panic macro within 12
  lines) returns **6 hits, all inside `mod tests`** in `control.rs`.
- **`rustty-config/src/string.rs`** (config escape-sequence decoding) is a careful port: the
  `\u{…}` accumulator uses `wrapping_mul`/`wrapping_add` + the oracle's `> 0x10ffff` bound,
  so the Zig-std overflow semantics are preserved rather than becoming a debug overflow
  panic.
- **`config_struct.rs:5761,6596`** (`assert!(Path::is_absolute(...))`) and **`:5837`**
  (`dirname(path).unwrap()`) are verbatim oracle `assert(std.fs.path.isAbsolute(path))` /
  `std.fs.path.dirname(path).?`.

Two sites left alone with a note rather than changed:

- `config_struct.rs:8410` `.expect("metric modifier fits in 1024-byte buffer")` — the oracle
  (`font/Metrics.zig:497`) does `catch return error.OutOfMemory`, i.e. it *propagates*.
  Unreachable in practice (`{}`-formatting an `f64` is ≤ ~310 chars against a 1024-byte
  buffer), so it is faithful in effect but not in form. Flagged, not fixed — fixing means
  threading a new error through the formatter chain for a case that cannot occur.
- `rustty-terminfo/src/source.rs:107,126` `panic!` ← oracle `@compileError`
  (`terminfo/Source.zig:87,104`). rustty runs at runtime what the oracle runs at comptime
  over the same static table, so the panic is reachable only if that static table changes —
  equivalent to a compile error. Faithful in effect.

## What remains unproven — be blunt

- **`rustty-vt` prod sites: 628 total; the ~120 in the PTY-facing subset were opened, the
  rest were not.** The unopened remainder is `screen.rs` (77), `page_impl.rs` (49),
  `terminal.rs` (47), `search/**` (85), `pagelist/**` (~110), `hash_map.rs`, `compress/**` —
  screen/pagelist internals, driven by the stream only *indirectly* through
  already-validated coordinates. That is a judgement call about reachability, not proof.
- **`assert!` was treated as presumptively faithful outside the PTY subset.** Zig's
  `std.debug.assert` is compiled OUT in ReleaseFast (Ghostty's ship mode) and ON in
  ReleaseSafe; rustty's `assert!` is always on. So rustty is stricter than shipped Ghostty
  at every one of the ~170 prod `assert!` sites in `rustty-vt`. This lane verified the
  PTY-facing ones map 1:1 onto oracle `assert(` lines (all 14 in `apc/glyph/request.rs`
  match `request.zig:116-118,248-250,319-323,381-383`; `stream.rs:790`, `parser.rs:384`,
  `charsets.rs:129`, `color.rs:52,74` likewise). **The other ~155 were not paired.**
- **`debug_assert` (173 prod sites in `rustty-vt`) was not audited at all.** It is the
  closest analogue to ReleaseFast-erased `std.debug.assert`, so it is the *least* likely to
  hide this defect — but that is an argument, not a check.
- **Slice-index-out-of-bounds, integer-overflow-in-debug, and division-by-zero were not
  swept.** These are Rust panics with no Zig `unreachable` counterpart and are a plausible
  second defect shape on exactly the same font/PNG/parse paths.
  `crates/rustty-wuffs/src/swizzle.rs`'s raw pointer/slice arithmetic is the most likely
  candidate. Not looked at.
- **`crates/rustty-core/src/surface.rs`, `crates/rustty-termio/src/stream_handler.rs`,
  `crates/rustty-apprt/**`, `crates/rustty-xev/**`, `crates/rustty-bench/**` were read but
  never edited** (concurrent lanes own them). `stream_handler.rs`'s four flagged
  `.unwrap()`s (`:1560`, `:1967`, `:1971`, `:1979`) were checked and are all faithful
  (oracle `setPwd("") catch unreachable`, `colors.foreground.get().?`).
- **Font-shaper `discovery.rs` (42 hits), `shared_grid*.rs`, coretext
  `NonNull::new(...).unwrap()` (12), `deferred_face.rs` platform arms (8), and lock
  `.unwrap()`s (~10) were excluded by rule, not verified.**
- **25 of the 100 in-scope reverse `else =>` prongs remain category-level** (terminal/c 8,
  cli 12, extra 5). By construction they are the least interesting residue — 8 are the
  mechanical `inline else => …Typed()` FFI dispatchers in a crate with an exhaustive panic
  scan showing nothing, 7 are bare `else => return err` that Rust's `?` reproduces
  structurally — but they were not opened one by one.
- **P0 (Appendix A's 301 test-code `unreachable!()` sites) is still a 25-site sample.** Not
  re-touched here.

## Verification (this lane)

- `cargo check --workspace --all-targets` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt --all` applied.
- `cargo test -p rustty-vt --lib` — 2319 pass, 0 fail, 3 ignored.
- `cargo test -p rustty-vt --features oniguruma --lib` — 2330 pass, 0 fail, 3 ignored.
- `cargo test -p rustty-config -p rustty-renderer --no-fail-fast` — 198 + 75 pass, 0 fail.
