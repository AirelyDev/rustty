# Integration-Tier Blueprint — quirks / Command / crash / termio (→ core → embedded)

Reviewed design (2026-07-01) for the tier after the apprt-api keystone. Authoritative for the next port effort. Build order + placements below are acyclicity-checked against the real Cargo graph.

## Placements (corrections to PORT_PLAN — apply these)
- **quirks — DONE, no action.** Already split (break #2): `rustty-util::quirks` (`inlineAssert`, leaf) + the one font quirk `disableDefaultFontFeatures` seamed via a local `trait Face` impl'd by `rustty-font::sprite::face`. 0 reference tests. The "needs placement" note is stale.
- **Command → `rustty-termio::command` (NOT `rustty-core`).** Placing it in core CYCLES: `termio/Exec.zig` constructs+starts Command, and `core` already deps `termio`, so Command-in-core ⇒ termio→Command(core) + core→termio cycle. Command's only consumers are termio/Exec + core's Surface (which already deps termio → reaches it as `rustty_termio::Command`). Command needs NO App/Surface concrete type (its `apprt` import is only the comptime `@hasDecl(pre_exec/post_fork)` check, realized ONLY by GTK = last). Today `RtPreExecInfo`/`RtPostForkInfo` are the trivial fallback (`init(&Config)`), needing only `rustty-config`'s `Config` type. FIX: remove "Command.zig" from `rustty-core/src/lib.rs` docstring; add it to `rustty-termio` (break "#13b"); re-export `rustty_termio::Command` at crate root.
- **crash → `rustty-crash` (existing stub) + a new local trait seam.** `dir.zig` + `sentry_envelope.zig` (8t) port as-is (only `rustty-os`). `sentry.zig`/`main.zig` port now EXCEPT `beforeSend`'s `ThreadState.surface`-dependent metadata block → seam it:
  ```rust
  pub trait ThreadStateSurface { fn screen_size(&self)->(u32,u32); fn grid_size(&self)->(u32,u32); fn cell_size(&self)->(u32,u32); }
  ```
  `ThreadState<S: ThreadStateSurface>` holds a raw `*const S` (keep the live/possibly-torn read the reference deliberately does; a snapshot would change semantics). Impl'd by core's Surface later. Do NOT reuse apprt-api's `CoreSurfaceHandle` (different concern). crash must NOT gain a `rustty-renderer` dep (renderer deps crash → cycle) — hence plain `(u32,u32)` tuples, acyclicity-required. New crash deps: `rustty-build-config` (setTag) + `sentry-sys` (vendored). `rustty-global` stays injected (allocator DI), not a hard dep. Source app-name segment for `ghostty/crash` XDG dirs from `rustty-identity` (R2), don't hardcode.
- **termio → `rustty-termio` (existing stub).** New Cargo deps required: `rustty-config`, `rustty-renderer`, `rustty-apprt-api` (its `surface::{AppMailbox,Message,SurfaceMessageEnvelope}` seam is EXACTLY `apprt.surface.Mailbox`), `rustty-global`. All four confirmed downward-safe (none dep termio).

## Test accounting (R1 contract — all achievable now, zero deferred-to-core test debt)
- crash: **9** (sentry_envelope 8 + main 1 reify).
- termio: **44** = Exec 10 + message 2 + shell_integration 21 + root 1 + **Command 10** (relocated).
- core: **1** (surface_mouse `keyToMouseShape`; App/Surface have 0; Command's 10 moved out).
- Add ~15 `check_test_counts.py` MAP entries (crash_*, termio_*, core_surface_mouse). Use termio=34 (raw contract) +10 Command = **44** authoritative (the behavioral heuristic undercounts refAllDecls blocks). `termio_command`'s zig glob points at `src/Command.zig` (physical location) though the Rust file is in termio.
- Untested-only deferrals (R1-safe, 0 tests): crash `ThreadState` concrete impl; termio `Thread/Options/Termio/backend/mailbox` xev field types.

## apprt-api growth-points this tier forces
- termio consumes the ALREADY-LANDED `surface::{AppMailbox, Message, SurfaceMessageEnvelope}` — NO seam.rs change. termio is a generic consumer (`M: AppMailbox`), doesn't touch App/Surface trait bodies.
- Command forces `RtPreExecInfo`/`RtPostForkInfo` — keep as trivial default in `rustty-termio::command` (GTK specializes later); do NOT add to apprt-api::seam now.

## Build order (crash first, xev deferred)
1. **crash** (~1 day) — smallest closed unit; validates the ThreadStateSurface seam pattern cheaply.
2. **termio dep wiring** (~1-2 days) — Cargo edges + struct shapes of the 0-test files (Options/Termio/backend/mailbox/message) against existing seams; defer xev field types to a local placeholder. Land minimal `renderer::message::Message` + `renderer::Thread::Mailbox` (= `BlockingQueue<Message,64>`, needs only datastruct — NO xev/GPU) + thin `DerivedConfig` placeholders in rustty-renderer.
3. **Command → termio::command** (~1 day) — 911 loc + 10 tests vs config/global/os; Windows path structural + POSIX-gated skip (like F3 DirectWrite).
4. **termio testable logic** (~2-3 days) — Exec `execCommand` (10t), shell_integration (21t, byte-exact per-shell strings), message (2t).
5. Fix docstring/PORT_PLAN corrections (core PORTS line; crate 23/24 counts; §5 edge table).

## Deferred decision points
- **D6 (xev / libxev)** — the async event-loop backend (`global.zig` `xev = @import("xev").Dynamic`; used by termio Thread/Options/Termio/mailbox/Exec `xev.Process`). Currently `rustty-global::seam::Xev = ()` placeholder. libxev is **Zig source, not C** → port faithfully as a new low crate `rustty-xev` (precedent: rustty-z2d), NOT a -sys. RECOMMENDATION: defer past this tier (those files have 0 tests); decide+port when a real "run a shell end-to-end, see output" milestone forces it. Track as its own stage.
- fontconfig-sys + libxml2-sys (Linux font discovery) — still deferred (off macOS embedding critical path; fontconfig needs build-time header-gen tooling).

## Then: core (Surface/App, fills apprt-api growth points + the mailbox/queue seam) → apprt/embedded (2250 loc, 71 C-ABI exports = the libghostty milestone, vt-c D2 cbindgen pattern) + none.zig → cli → bin.
