# New findings discovered during closure work

This file records defects found while closing a `LEDGER.tsv` row that the
original parity audit did not catch. It is **not** part of the audit's fixed
denominator (497 rows) — never add a row to `LEDGER.tsv` for something
recorded here; that file's row count must stay stable. Shape mirrors a ledger
row: id, severity, kind, oracle file:line, rustty file:line, description, how
found, and current state.

---

## N1 — `GlobalState::new` performs an extra, unfaithful environ capture

- **id**: N1
- **severity**: medium (source-level fidelity divergence; masked a `high`-rated
  behavioral defect on the only reachable success path, see note below)
- **kind**: missing-branch / extra-behavior (the reference's `.main` `InitOpts`
  arm reuses a caller-supplied environ; the port's `GlobalState::new`
  unconditionally re-captures from the live process instead)
- **oracle**: `src/global.zig` — `GlobalState.init`'s `InitOpts` union,
  `.main => |m| m.environ` (around the `self.environ = switch (opts) { ... }`
  assignment referenced in `new_with_environ`'s own doc comment,
  `crates/rustty-global/src/lib.rs:415-424`). The oracle never re-reads the
  process environment at `GlobalState` construction time — it takes whatever
  block the caller already captured.
- **rustty (pre-fix)**: `crates/rustty-global/src/lib.rs:411-413`
  (`GlobalState::new` — `Self::new_with_environ(alloc, resources_dir,
  environ::Environ::from_process())`, an unconditional FRESH capture) called
  from `crates/rustty-apprt/src/lib.rs:398` (pre-fix:
  `rustty_core::GlobalState::new((), resources_dir)`).
- **description**: `GlobalState::new` grabs a brand-new
  `Environ::from_process()` snapshot and republishes it into the shared
  process-global `ENVIRON` (via `new_with_environ`'s
  `environ::set_environ(env.clone())`) every time it is called, rather than
  reusing the environ the caller already captured/maintained. In
  `init_with_action`, this call happens unconditionally, strictly after
  `ensureLocale()` on the only reachable success path — so it accidentally
  republishes whatever the live process environment happens to be at that
  point, REGARDLESS of whether anything upstream explicitly re-synced it.
- **how found**: while closing S43-F1 (`global.init` not calling
  `sync_environ()` after `ensureLocale()`), an end-to-end regression test
  (mutate `LANG`, call `init_with_action`, assert `environ_map()` reflects the
  correction) was found to pass identically whether the `sync_environ()` fix
  was present or absent. Instrumenting both call sites showed
  `GlobalState::new`'s internal fresh capture — a few lines after
  `ensureLocale()` — was silently doing the republish job instead. This
  masking meant S43-F1's originally-claimed behavioral impact (termio
  child-process environments, a later `sync_environ()` caller seeing a stale
  locale) did not actually reproduce today; see the severity note on S43-F1's
  ledger closure.
- **state**: **CLOSED** — fixed in the same commit that closed S43-F1
  (`crates/rustty-apprt/src/lib.rs:398`, changed from `GlobalState::new((),
  resources_dir)` to `GlobalState::new_with_environ((), resources_dir, env)`
  where `env` is the already-captured, already-`sync_environ()`-resynced
  block — matching the oracle's `.main` arm exactly and removing the
  redundant/unfaithful re-capture). Commit: see the S43-F1 closure commit in
  `crates/rustty-apprt/src/lib.rs`'s history.
- **note on a deadlock found while fixing this**: the first attempt at the
  fix wrote `rustty_global::environ().clone()` inline as a call argument to
  `new_with_environ`. `rustty_global::environ()` returns an `EnvironRef`
  guard wrapping a live `RwLock` READ lock; Rust's temporary-lifetime rule
  keeps a temporary created while evaluating a `let` statement's initializer
  alive until the end of that ENCLOSING statement — so the read guard stayed
  held for the full duration of the `new_with_environ` call, which itself
  takes a WRITE lock on the same `RwLock` internally
  (`environ::set_environ`). `std::sync::RwLock` is not reentrant, so this was
  a same-thread self-deadlock (reproduced: the test hung indefinitely,
  confirmed via `sample`/`lsof`/process inspection that no other process or
  lock was involved). Fixed by binding the clone to a local in its own
  statement first (`let env = rustty_global::environ().clone(); let mut
  state = GlobalState::new_with_environ((), resources_dir, env);`), so the
  read guard is dropped before the write-locking call begins. This is purely
  an implementation detail of the fix, not a further oracle divergence — no
  new finding recorded for it — but is called out here and at the call site
  in `lib.rs` because it is exactly the kind of hazard a future edit to that
  line could reintroduce silently.

---

## N2 — one `log.warn` in `tmux/viewer.zig` has no restorable Rust counterpart

- **id**: N2
- **severity**: low (test-helper diagnostic only; no shipped code path)
- **kind**: unportable-by-signature (the port's test helper has a different
  error-reporting contract than the reference's, and one of its two format
  arguments has no `Debug` impl to render)
- **oracle**: `src/terminal/tmux/viewer.zig:1497` —
  `log.warn("testViewer step failed i={} step={}", .{ i, step });` inside
  `fn testViewer(viewer: *Viewer, steps: []const TestStep) !void`, in the
  `step.run(viewer) catch |err|` arm.
- **rustty**: `crates/rustty-vt/src/tmux/viewer.rs:2606-2624` — left as a
  fidelity comment with a PORT NOTE stating both obstructions.
- **description**: two independent reasons, either sufficient. (1) The port's
  `TestStep::run` returns `()` and reports a failed step by panicking — the
  port of the reference's `!void` plus `try`-assertions — so there is no error
  value to `catch` and therefore no arm for the emission to live in.
  (2) `step={}` would require `Debug` on `TestStep`, which holds a
  `Box<dyn Fn>`; closures have no `Debug` impl, so the argument cannot be
  rendered without changing the helper's representation.
- **how found**: while closing S209-F2 (all `log.*` diagnostics in the tmux
  viewer). The site is NOT among that row's cited oracle anchors — the audit
  scoped the row to non-test code — so its absence does not make S209-F2's
  closure a hole. It is recorded here so the finale re-audit sees it as a
  tracked item in its own right rather than as a footnote in a closed row's
  `owner` cell.
- **controller ruling (2026-09-09)**: S209-F2 stays **CLOSED**, not `WAIVED`.
  The two states available carry different meanings — `CLOSED` is "every cited
  anchor restored", `WAIVED` is "not a defect" — and neither describes
  "complete within scope, with one out-of-scope site permanently unportable".
  `CLOSED` is correct on the row's own anchors, the exception is written into
  the tracked ledger's `owner` cell and into a PORT NOTE at the code site, and
  this entry gives the leftover its own visible record. Cost if wrong: the
  finale re-audit re-derives this one test-only site and finds it already
  explained in three places.
- **state**: **OPEN — will not fix.** Closing it requires changing
  `TestStep::run`'s contract to return a `Result` and giving `TestStep` a
  hand-written `Debug`, both of which would make the port's test helper LESS
  faithful to the reference in order to restore one of its diagnostics.

---

## N3 — the port shares one "seen" table between two `logUnsupportedOnce` sites the reference keeps separate

- **id**: N3
- **severity**: low (exactly one suppressed diagnostic per process; no state or
  rendering difference)
- **kind**: behavioral-difference (a warning the reference emits twice per
  process — once from each of two call sites — the port emits once)
- **oracle**: `src/terminal/stream.zig:734` and `:1185`, both calling
  `logUnsupportedOnce("ignoring UTF-8-decoded C1 controls, first: 0x{x}", .{…},
  0x80)`. The helper (`stream.zig:3040`) declares its `Static.seen` table
  **inside** the generic function, so Zig monomorphizes one table per
  `@TypeOf(args)` instantiation. The two sites pass different argument types —
  `:734` passes `cp`, a `u32` element of `cp_buf: [4096]u32` (`stream.zig:671`);
  `:1185` passes `c`, a narrower integer — so they are two distinct
  instantiations with **two separate tables**.
- **rustty**: `crates/rustty-vt/src/stream.rs:1151` and `:1847` both pass
  `&SEEN_UTF8_DECODED_C1` (`stream.rs:4719`) — one shared table.
- **description**: the port's `log_unsupported_once` takes the table by
  reference rather than instantiating it per argument type, so the two sites
  share suppression state. The first site to fire claims key `0x80`; the second
  then finds the key already present and never emits. The reference emits from
  both. Net effect: exactly one warning suppressed per process, on a path that
  is itself a rare-input diagnostic.
- **how found**: during task 14r-a's fix round, while verifying that a PORT NOTE
  in `stream.rs` correctly described which statics are shared between call
  sites. The note's sharing claim was accurate about the port and wrong about
  the reference. Recorded as a `KNOWN DIVERGENCE` comment at the static's
  definition.
- **why not fixed in that round**: it is a behavioral change outside the two
  rows that round was fixing (S163-F2, S164-F1), made on a branch two other
  workers were committing to. The correct fix is to give each site its own
  static and assert both emit — small, but it needs its own test and its own
  review rather than riding along inside a test-isolation fix.
- **state**: **OPEN** — to be fixed with a test that drives both call sites in
  one process and asserts two records, not one.

---

## N5 — `S94-F1`'s mpsc stand-in rests on a justification that is false, and the real port exists

- **id**: N5
- **severity**: medium (a subsystem the port replaced with a stand-in under a PORT NOTE
  that misstates why; the faithful implementation is already in the tree and unused)
- **kind**: unported-work / stale-deferral
- **oracle**: `src/font/shaper/coretext.zig:76-81` — the `CFReleaseThread` the shaper
  starts and hands released CoreFoundation objects to.
- **rustty**: `crates/rustty-font-shaper/src/shaper/coretext.rs:36-55` — an mpsc channel
  stand-in, formerly carrying a PORT NOTE calling the CFReleaseThread seam "deferred".
- **description**: the PORT NOTE's premise was that the release thread had not been
  ported. It had: `rustty_os::cf_release_thread` is a real, inhabited implementation on
  the live kqueue backend. Task 14c-4 verified this while closing the file's logging rows
  and corrected the note in place, but did not perform the swap — replacing a channel
  stand-in with the real thread is a runtime refactor, not a logging change, and belonged
  in neither that task's scope nor a fix round on a shared branch.
- **how found**: task 14c-4, while closing `S94-F4` and the other `shaper/coretext.rs`
  rows. Found by checking the PORT NOTE's factual claim rather than accepting it —
  the same method that found [[N4]] and the `codepoint_resolver` control-flow divergence.
- **why this is recorded separately from S94-F1**: `S94-F1` stays `OPEN` and is the row
  that tracks the swap. This entry exists because the row's own text describes the defect
  as a logging omission, which it is not, and because the corrected PORT NOTE now reads as
  a satisfactory answer when it is only an accurate description of an outstanding gap.
  This project has a standing lesson that stale deferral notes manufacture fake gaps; the
  inverse also holds — a corrected note can conceal a real one.
- **state**: **OPEN — needs an owner.** The swap is a runtime refactor: replace the mpsc
  stand-in with `rustty_os::cf_release_thread`, on the real kqueue backend, with a test
  that proves released objects reach it. Schedule it as its own task; it is not logging
  work and must not ride along inside a `LOG`-class row's closure.

---

## N4 — `DeferredFace.zig` has three live `log.warn` sites and no `LOG` row names the file

- **id**: N4
- **severity**: low (three diagnostics; no state or rendering difference)
- **kind**: dropped-diagnostic / **ledger under-coverage**
- **oracle**: `src/font/DeferredFace.zig:403` (`failed to init face for codepoint
  check face={s} err={}`), `:430` (`error creating deferred face err={}`), `:463`
  (`error loading deferred face err={}`). All three are live — the file's only
  other `log` mention, `:303`, is commented out upstream.
- **rustty**: `crates/rustty-font-shaper/src/deferred_face.rs` — all three were
  carried as PORT NOTE comments, two of them explicitly justified with the (now
  false) claim "this crate wires no logger".
- **description**: the file has four rows in `src/font` — `S85-F2`, `S85-F3`,
  `S85-F5`, `S85-F6` — and **not one is `class=LOG`**. The audit that produced the
  ledger found four defects in this file and did not find its dropped
  diagnostics, so no row's cited anchors would ever have led an executor here.
  This is the under-count shape `PROCEDURE.md` blind spot 3 was written for, and
  it is the second instance found in one stage (the first being the fifth,
  unclaimed site in `face/web_canvas.zig:317`).
- **how found**: task 14c-4, by enumerating the reference's OWN `log.*` calls
  per file across the whole of `src/font` and reconciling the count against the
  live emissions in the port — not by following any row's anchors. The same
  method found N6 in `discovery.zig`.
- **disposition**: `:403` and `:463` are **restored** and live, on the
  reference's `deferred_face` scope. `:430` is **WAIVED**: its only reference
  failure mode is OOM from `alloc.dupeZ`/`alloc.create`, and Rust's `Vec`/`Box`
  ABORT on allocation failure rather than returning, so the port has no reachable
  failure arm and no `null` return — the emission could only sit on the success
  path. PORT NOTE at the site; adjudicated at
  `.port/parity-closure/ADJUDICATION.md`, batch 14c-4, heading
  `N4 — deferred_face_new's dropped warn`.
- **why this is recorded separately from any row**: there is no row to close.
  Without this entry the work is invisible to the finale re-audit, and the file
  would read as fully covered because its four rows are all ticked.
- **state**: **CLOSED for the two restorable sites, WAIVED for the third.** The
  residual item for the finale is not code but process: **no `LOG`-class row
  exists for `DeferredFace.zig`, and the ledger should not be read as if the
  absence of a row means the absence of a defect.**

---

## N6 — `discovery.zig`'s two table-parse `log.warn`s were discarded by `.ok()`, and no row names them

- **id**: N6
- **severity**: low (two diagnostics; the `null`/`None` degradation was already
  faithful, only the warning was lost)
- **kind**: dropped-diagnostic / **ledger under-coverage**
- **oracle**: `src/font/discovery.zig:762` (`error parsing head table: {}`) and
  `:779` (`error parsing OS/2 table: {}`), both in the `catch` arm of an
  `opentype.*.init` inside the CoreText descriptor walk. These are the file's
  ONLY two `log.*` calls, and both are live.
- **rustty**: `crates/rustty-font-shaper/src/discovery.rs` — both parses were
  `.ok()`, discarding the error and the diagnostic together, with the reference's
  `log.warn` kept as a PORT NOTE comment. The file carried no `LOG_SCOPE` at all,
  even though task 14c-4's own scope table assigns `discovery.zig` the scope
  `discovery`.
- **description**: same shape as N4 — no ledger row names either site, so no
  executor following cited anchors would have reached them. Distinct from the
  waived cases in this stage: `Head::init` and `OS2::init` both return `Result`,
  so these are genuine reachable failure arms and the "no diagnostic on a success
  path" rule does not apply.
- **how found**: task 14c-4 fix round 1, in response to review item F4, which
  itself came from applying blind spot 3's enumeration to the one `src/font` file
  the stage had not swept.
- **disposition**: both **restored**. The `catch` arms were extracted into
  `parse_head_table` / `parse_os2_table` free fns taking `&[u8]`, because
  manufacturing a CTFont with a malformed table is not something the suite can
  do — the extraction is what makes the failure arm reachable from a test.
  Covered by four tests (a malformed table warns exactly once and still degrades
  to `None`; a well-formed one emits nothing).
- **state**: **CLOSED.** As with N4, the residual for the finale is the process
  point: two files in `src/font` had live diagnostics that the ledger never
  named.

## N7 — `metal/buffer.zig`'s two null-contents warns are unrestorable and unclaimed

- **id**: N7
- **severity**: low (two diagnostics that cannot fire in rustty; no behavioural divergence
  on any reachable path)
- **kind**: unrestorable-diagnostic / uncited-site
- **oracle**: `src/renderer/metal/buffer.zig:98` and `:155` —
  `log.warn("buffer contents ptr is null", .{}); return error.MetalFailed;` inside the
  `orelse` guard on `msgSend(?[*]u8, objc.sel("contents"), .{})`, in `sync` and
  `syncFromArrayLists` respectively.
- **rustty**: `crates/rustty-renderer/src/metal/buffer.rs` — both `dst` blocks, which call
  objc2's `MTLBuffer::contents()`.
- **description**: objc2 types `contents()` as returning `NonNull<c_void>`, so the
  reference's null guard has no counterpart. There is no failure arm for the `log.warn` to
  live in, and lifting it out of the arm would put a diagnostic on the success path — the
  production-bug shape this plan has already shipped once. Both sites keep the reference
  call as an R1 comment with a PORT NOTE saying why it is not restorable.
- **how found**: task 14c-2, enumerating the ORACLE's own `log.*` calls per file rather
  than trusting the ledger's anchors (PROCEDURE.md blind spot 3). `buffer.zig` is not cited
  by `S119-F1` or by any other row.
- **why this is recorded separately**: no row claims these sites, so no row's `CLOSED`
  state rests on them and the blind-spot-1 carve-out is not in play. This entry exists so
  the gap is tracked rather than living only in a gitignored report.
- **state**: **WAIVED-by-construction.** Nothing to schedule unless the port ever moves off
  objc2's `NonNull` contract for `contents()`; if it does, restore both warns with
  `error.MetalFailed` as the return.

---

## N8 — `initPostPipelines`'s error set collapsed to one variant, so its restored log always prints the same thing

- **id**: N8
- **severity**: low (a diagnostic that is present and correctly placed but permanently
  under-informative; no state or rendering difference)
- **kind**: error-type degeneration / behavioral-difference
- **oracle**: `src/renderer/metal/shaders.zig:151` —
  `log.warn("error initializing postprocess shaders err={}", .{err});` in the
  `initPostPipelines(...) catch |err| err: { ... break :err &.{}; }` arm. `err` there
  ranges over the inferred error union of `initPostPipelines`, which spans allocation
  failure, `error.MetalFailed` from either `checkError` site, and the shader-source
  decode path — so Zig's `{}` renders a DIFFERENT `error.<Name>` per cause.
- **rustty**: `crates/rustty-renderer/src/metal/shaders.rs` — `init_post_pipelines`
  returns `Result<Vec<Pipeline>, Error>` where `Error` has exactly one variant,
  `MetalFailed`. Every failure in that call tree is flattened into it (four separate
  `.map_err(|_| Error::MetalFailed)` / `.map_err(|_nserr| ...)` sites feed it).
- **description**: the restored emission is at the right place, at the right level,
  with the reference's verbatim message text — but its one argument can only ever
  render `MetalFailed`, so the diagnostic cannot distinguish an out-of-memory from a
  spirv-cross-produced MSL that failed to decode as UTF-8 from a Metal compile error.
  The reference distinguishes all three. This is an ERROR-TYPE divergence, not a
  logging one: it predates the log restoration and would exist identically if the log
  had never been dropped.
- **how found**: task 14c-2, restoring `S119-F1`'s three sites. It is the shape the
  brief warned about ("a function's error type had degenerated so the restored message
  would print `err=()`") in a milder form — degenerate to one NAMED variant rather than
  to `()`, which is why it survives a casual read of the restored line.
- **why this is recorded separately from S119-F1**: `S119-F1` is legitimately CLOSED —
  all three of its cited anchors are live emissions with verbatim text, which is what
  the row asks for. Widening a Metal error set is a runtime refactor and must not ride
  along inside a `LOG`-class row's closure (the same principle applied to [[N5]]). But
  the divergence is knowable and permanent, and right now it lives only in a row
  disposition inside a CLOSED row, where the finale re-audit would not meet it as a
  tracked item.
- **state**: **OPEN — needs an owner.** Closing it means giving
  `metal::shaders::Error` (and the `metal::pipeline::Error` it folds in) variants that
  distinguish the reference's causes — at minimum allocation/decode/Metal-compile —
  and threading them through the four flattening `map_err`s. Note the same flattening
  is what makes every other `{err:?}` site in this crate unverifiable against the
  oracle's rendering (task-14c-1 fix round, concern 1); this row is simply the one
  case where the collapse is total and therefore provable without a ruling.

---

## N9 — the Linux twins of `generic.zig` and `image.zig` are still logging-dead, while their macOS twins are live and their rows are CLOSED

- **id**: N9
- **severity**: medium (a Linux/FreeBSD build is silent on ~30 diagnostics the reference
  emits and the macOS build now emits; no state or rendering difference)
- **kind**: uncited-site / twin drift
- **oracle**: `src/renderer/generic.zig` (the `rebuildCells`/`rebuildRow`/preedit/cursor/
  overlay/background-image `log.warn` family) and `src/renderer/image.zig` (the kitty
  placement / upload family).
- **rustty**: `crates/rustty-renderer/src/generic/linux_impl.rs` and
  `crates/rustty-renderer/src/image/linux_impl.rs`.
- **description**: stage 14c restored these call sites in the **macOS** twins only.
  Measured at the end of task-14c-3:

  | file | live `log::` emissions | remaining commented reference calls |
  |---|---|---|
  | `generic.rs` (macOS twin) | 31 | 44 |
  | `generic/linux_impl.rs` | **1** | 16 |
  | `image/macos_impl.rs` | 11 | 20 |
  | `image/linux_impl.rs` | **0** | 19 |

  (The "commented" column counts comment ANCHORS, not sites — a multi-line block comment
  reproducing one reference call and its one-line stub both match, so it roughly
  double-counts. PROCEDURE.md blind spot 3.) The macOS numbers are the correct ones; the
  Linux twins are the same reference calls, not ported.
- **how found**: task-14c-3, enumerating the oracle's own `log.*` calls across the
  Linux-only files it was in (PROCEDURE.md blind spot 3). The stage restored the OpenGL
  ones it owned (`OpenGL.zig`, `opengl/Frame.zig`, `opengl/shaders.zig`, and the
  `pkg/opengl` logger's routing) and deliberately did NOT take on the two big twins —
  they are a different unit of work, they are Linux-only (no local typecheck), and one of
  them (`image/linux_impl.rs`) already has an OPEN row of its own, `S117-F2`.
- **affected rows**: exactly three — **`S115-F4`**, **`S116-F2`** (both
  `src/renderer/generic.zig`, task-14c-4) and **`S117-F3`** (`src/renderer/image.zig`,
  task-14c-2). All three are `WAIVED` with `owner=.port/parity-closure/ADJUDICATION.md#…`,
  and their adjudication text now says explicitly that the restoration is macOS-twin only
  and points here.
  *(Corrected in fix round 1. The first version of this entry named `S113-F2`, `S117-F3`
  and `S119-F1`. `S113-F2` (`renderer/Metal.zig`) and `S119-F1` (`renderer/metal/
  shaders.zig`) are Metal-ORACLE rows: there is no Linux twin of those oracle files at
  all, so they are unaffected. `S115-F4` and `S116-F2` were omitted and are affected —
  their oracle IS `generic.zig`, whose Linux twin is `generic/linux_impl.rs`.)*
- **why this is recorded separately**: those three rows name only the macOS twin in their
  `rustty` column, so their `WAIVED` state does not rest on these sites and the
  blind-spot-1 carve-out is not in play. But a reader of those rows would reasonably
  believe the diagnostics are restored, and on Linux they are not. This entry is so the
  finale re-audit meets that as a tracked item.
- **size**: roughly **43 genuinely unported call sites** across the two files (the
  reviewer's count in the 14c-3 fix round). That is a task, not a fix round; narrowing the
  three dispositions is the honest interim state.
- **state**: **OPEN.** Scheduling note for whoever takes it: it is mechanical (the macOS
  twin is the patch), it needs `const LOG_SCOPE: &str = "generic_renderer";` in
  `generic/linux_impl.rs` and `"renderer_image"` in `image/linux_impl.rs`, several arms
  currently discard their payload with `.is_err()` / `Err(_err)` and must bind it, and
  **it cannot be typechecked on macOS** — the VM lane (`PROCEDURE.md`, Linux rows) is
  mandatory, not optional.
- **why the two twins really are mutually exclusive** (checked in fix round 1, because it
  was disputed and the disputing read stopped one level too early): `generic.rs` is not
  itself platform-gated, but all 32 of its `log::` emissions sit inside its
  `#[cfg(target_os = "macos")] mod macos_impl` (which begins at `generic.rs:1095`), and
  `generic/linux_impl.rs` is reached only through
  `#[cfg(any(target_os = "linux", target_os = "freebsd"))]`. Never both in one build.

---

## N10 — `web-canvas-instantiate.mjs` was dead for ~1 day: this plan's own log backend added an `env.log` import the glue did not supply

- **id**: N10
- **severity**: low-medium (no shipped artifact affected — the break is in a verification
  script, not in `rustty_web_canvas.wasm` itself. Medium only because the dead script is
  the sole headless check for the whole freestanding web-canvas deliverable, so while it
  was dead nothing was checking `W9F-03`/`W9F-04` or the "0 wasi imports" link invariant.)
- **kind**: tooling regression introduced by this plan's own work (not a port defect, not
  an oracle divergence)
- **oracle**: none — `.port/tools/*.mjs` are rustty-only harnesses with no ghostty analogue.
- **rustty**: `.port/tools/web-canvas-glue.mjs` (`makeWebCanvasEnv`) broke
  `.port/tools/web-canvas-instantiate.mjs`.
- **description**: `install_wasm_log_backend()` registers `rustty-os`'s `WasmLogger`, which
  makes `rustty_web_canvas.wasm` import `env.log` — in BOTH profiles (release strips the
  restored call-site bodies but keeps the import). `makeWebCanvasEnv` supplied only the
  fourteen `rustty_wc_*` Canvas ops, and a missing wasm import is a hard `LinkError`, so
  from the moment the backend landed:

  ```
  $ node .port/tools/web-canvas-instantiate.mjs
  LinkError: WebAssembly.instantiate(): Import #14 "env" "log": function import requires a callable
  ```

  Every one of that script's checks — 0 wasi imports, instantiation, face/atlas/shaper
  callability, the `(c4)` DeferredFace arms, the `(c5)` BGRA byte-order assertion, the
  `(c6)` font-file-bytes rejection — was unreachable behind that throw.
- **broken by**: `9a41c0c2` ("task-14c-4 (part 4): give rustty_web_canvas.wasm a log
  backend"), which put the first `install_wasm_log_backend()` calls into
  `rustty-font-shaper`'s seven exports and therefore introduced the `env.log` import.
  `3a90678c` (the artifact-wide fix) widened the installer to the other twelve exports but
  did not cause this; the import already existed.
- **closed by**: `2ca1a947` (task-14wasm-proof). `makeWebCanvasEnv` now takes an optional
  `log` and supplies a discarding default, so every existing caller works unchanged and the
  new `.port/tools/wasm-log-emission.mjs` passes a capturing one. `node
  .port/tools/web-canvas-instantiate.mjs` → ALL OK again.
- **how found**: task-14wasm-proof, while reusing `web-canvas-glue.mjs` to build the
  log-emission harness. Running the sibling script once, to confirm the glue still worked,
  is what surfaced it — it was not found by any gate, test, or review.
- **why this is recorded**: **no gate runs `web-canvas-instantiate.mjs`.** It is referenced
  by two mutation-proven `.port/phase-2/DEFERRALS.md` entries (`W9F-03`, `W9F-04`) as their
  standing evidence, and by `.port/tools/CI.md` nowhere. That is exactly why the rot went
  unnoticed: a harness with no gate is a harness whose failure nobody observes. The same
  argument is why `wasm-log-emission.mjs` was wired into `ci.sh`'s opt-in `--webgl` lane
  rather than left as a documented command.
- **state**: **CLOSED** for the glue defect itself.

  **RESIDUAL, OPEN, and deliberately left for a future worker:**
  `web-canvas-instantiate.mjs` is *still ungated*, so the exact rot mechanism demonstrated
  here remains open for that script — the next import the artifact grows will kill it again,
  silently. The fix is three lines in `run_webgl_lane` (`.port/tools/ci.sh`), beside the
  log-emission gate that now runs there, using the RELEASE artifact that lane already
  builds. Judged out of scope for task-14wasm-proof by its reviewer; recorded here so
  whoever next touches that lane trips over it.

---

## N11 — restored `log` sites render `{err:?}` where Zig's `{}` renders `error.<Name>`, crate-wide across ~35 sites

- **id**: N11 (next free id checked at the time of writing: N1-N10 taken, N10 by another
  worker — ids have collided twice on this branch)
- **severity**: low (message-text divergence in a diagnostic; no state, control-flow or
  rendering difference)
- **kind**: log-text divergence / cross-cutting
- **oracle**: every `log.*("… err={}", .{err})` in `src/renderer` and `pkg/opengl` — e.g.
  `generic.zig:852, 1436, 1558, 2627, 3461`, `renderer/Metal.zig`, `renderer/image.zig`,
  `renderer/opengl/Frame.zig:65`, `renderer/OpenGL.zig:242, 308`.
- **rustty**: `crates/rustty-renderer/src/**` — 31 sites in `generic.rs`, plus `metal/*`,
  `image/macos_impl.rs`, `overlay.rs`, `opengl/opengl_impl.rs`, `generic/linux_impl.rs`,
  `opengl/shaders.rs`. ~35 in total at the time of writing.
- **description**: Zig's `{}` applied to an error VALUE prints `error.` **plus** the name
  (`std.fmt`'s error-set arm), so the oracle emits e.g. `… err=error.OpenGLOutdated`. The
  port's uniform convention is `err={err:?}`, i.e. the Rust error's `Debug`, which prints
  `OpenGLOutdated` — or, for a tuple variant, `EglUnavailable("…")`. The prefix, and
  sometimes the shape, differ. Task-14c-1 already hit and fixed this for ONE site where
  the rendering was knowable and hard-codeable (`error.StreamTooLong`, its fix round 1);
  every other site formats an arbitrary error value and was left as `{err:?}`.
- **the pattern already exists in this tree**: `crates/rustty-font-shaper/src/shaper/
  web_canvas.rs:455-466` gives its error type a small `Display` impl that writes
  `error.{name}` with the comment "Zig renders a caught error as `error.Name` under
  `{}`". `rustty-termio/src/command.rs:180` and `:1511` and
  `rustty-apprt/src/gtk/cgroup.rs:51` do the same. Applying that shape to the renderer's
  error types and switching the sites from `{err:?}` to `{err}` is the fix.
- **how found**: task-14c-3, restoring `OpenGL.zig:242`/`:308` and noticing its own two
  new sites inherit the divergence. Raised in that task's report; recorded here in its
  fix round 1 at the coordinator's instruction, because it is crate-wide and does not
  belong inside any one closed row.
- **why this is recorded separately**: it is not a defect any single row introduced or can
  close. No row's `CLOSED`/`WAIVED` state rests on it, and churning ~35 sites inside a fix
  round is exactly the kind of unscoped change this plan avoids.
- **state**: **OPEN**, scheduled as its own task. Scope: give the renderer's error types
  (`opengl::opengl_impl::Error`, `opengl::gl::errors::Error`, `metal::*`, the shaper and
  collection errors that reach a log site) the `error.{name}` `Display` impl, switch the
  sites to `{err}`, and update the handful of tests that assert on a rendered payload —
  `opengl_impl::tests::display_realized_warns_and_continues_when_context_prep_fails` and
  `present_logs_when_the_srgb_re_enable_fails` both pin the current `Debug` form and will
  need their expected strings changed with it.
