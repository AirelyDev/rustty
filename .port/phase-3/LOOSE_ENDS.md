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
