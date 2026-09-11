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

## N9 — the Linux twins of `generic.zig` and `image.zig` were logging-dead while their macOS twins were live (CLOSED by task-14c-5)

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
- **size**: **35 genuinely unported call sites** across the two files (24 in
  `generic.zig`, 11 in `image.zig`), producing 36 emissions. The figure first written here
  was "roughly 43", from the 14c-3 fix round; it was wrong, and it is corrected in place
  rather than only below, because a reader greps the size bullet. The 43 counted six
  `generic.zig` sites (`:991`, `:1169`, `:1177`, `:1183`, `:1188`, `:1190`) that sit behind
  `if (comptime DisplayLink == void) return;` and therefore have **no Linux counterpart to
  port at all**; the Linux twin has no display-link field. See the state block below and
  task-14c-5 §2.1. That is a task, not a fix round; narrowing the three dispositions was
  the honest interim state.
- **state**: **CLOSED (task-14c-5, 2026-09-10).** Both Linux twins are restored.
  `generic/linux_impl.rs` carries `const LOG_SCOPE: &str = "generic_renderer";` at file
  scope and 25 live emissions covering 24 `generic.zig` call sites; `image/linux_impl.rs`
  carries `const LOG_SCOPE: &str = "renderer_image";` and 11 live emissions. Verified on
  the Linux VM (build / test / clippy `-D warnings` / workspace `fmt --check`, plus the
  test pass under `RUSTTY_REQUIRE_GL=1`).

  Corrections to this entry's own numbers, found by enumerating the reference rather
  than trusting them:

  - **"~43 genuinely unported call sites" was high.** The real figure is **35**
    restorable sites (24 in `generic.zig`'s Linux arm + 11 in `image.zig`'s), and the
    35 emissions are 36 because `generic.zig:1974` needs two arms (the io error and
    the reference's `error.StreamTooLong` 4 GiB cap).
  - **Six `generic.zig` sites have NO Linux counterpart at all** and were never
    "unported": `:991` (`displayLinkCallback`) and `:1169/:1177/:1183/:1188/:1190`
    (`syncDisplayLink`) are guarded by `if (comptime DisplayLink == void) return;`,
    and `DisplayLink` is `void` off macOS. Counting them as a Linux gap is what
    inflated the estimate.
  - **The "1 live emission" in `generic/linux_impl.rs` was not a `generic.zig` site.**
    It is `opengl/Frame.zig:65`'s present-failure, which task-14c-3 landed in that file
    because the port inlines `Frame.zig`'s completion tail into
    `opengl_frame_completed`. Against `generic.zig` the Linux twin stood at **zero**,
    not one — the same "a reference call lands in a different file than its name
    suggests" shape 14c-1 and 14c-2 each hit once.

  **What remains — nothing that is not already a WAIVED row's own residual.** Five
  reference sites are unrestorable on Linux, and they are the SAME five that are
  unrestorable on macOS, for the same reason (the Rust operation is infallible, so
  there is no failure arm and hoisting the warn would put a diagnostic on the SUCCESS
  path): `generic.zig:1558` (S115-F4), `:2912` and `:3181` (S116-F2), `image.zig:365`
  and `:447` (S117-F3). Each carries a PORT NOTE at its Linux code site and is recorded
  as a Linux-side residual in the row's `ADJUDICATION.md` entry. The three rows stay
  **WAIVED** — this task closed no rows and changed no row state.

  The `eprintln!` stand-in class showed up again: `generic/linux_impl.rs`'s
  `prep_background_image` had three of them AND the same collapsed `std::fs::read`
  that task-14c-1 had to split in the Metal twin (the reference logs a different
  message for open and for read). Both are fixed.
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

## N11 — restored `log` sites render Rust's derived formatter where Zig renders its builtin aggregate formatter, crate-wide across ~35 sites

- **id**: N11 (next free id checked at the time of writing: N1-N10 taken, N10 by another
  worker — ids have collided twice on this branch)
- **severity**: low (message-text divergence in a diagnostic; no state, control-flow or
  rendering difference)
- **kind**: log-text divergence / cross-cutting
- **root cause (widened 2026-09-10)**: this is NOT only about error values. The general
  shape is **Rust's derived `Debug` standing in for Zig's builtin aggregate formatter**,
  and it bites plain values as much as errors. Three non-error instances, all verified
  against captured records, not inferred:
  - `image.zig:122` and `:132` — the placement id renders as `Kitty(7)` where Zig's `{}`
    on the tagged union prints `renderer.image.Id{ .kitty = 7 }`. Present on **both**
    platform twins (`image/macos_impl.rs`, `image/linux_impl.rs`).
  - `generic.zig:2137` — the screen size renders as a full Rust struct dump where Zig
    prints `renderer.Size{ .screen = … }`. Ported at
    `crates/rustty-renderer/src/generic/linux_impl.rs:1470` and its macOS counterpart.

  **Mirroring the existing behaviour was the right call and should stay that way until the
  sweep.** `Id` has no Zig `format` method to port, so hand-writing a `Display` impl would
  be *inventing* fidelity rather than restoring it; and the two platform twins must keep
  the same public surface, so a divergence introduced on one arm alone would be a worse
  defect than the text gap. These are recorded here rather than filed separately because
  the root cause is identical and splitting them would fragment the eventual sweep.
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
  sites to `{err}`, **and decide the non-error aggregates named under root cause above**
  (`image::Id`, `renderer::Size`) in the same pass, on both twins together, and update the handful of tests that assert on a rendered payload —
  `opengl_impl::tests::display_realized_warns_and_continues_when_context_prep_fails` and
  `present_logs_when_the_srgb_re_enable_fails` both pin the current `Debug` form and will
  need their expected strings changed with it.
## N12 — the audit's line anchors leave whole reference-file HEADS unaudited, and no row will ever claim them (new gate G-LOGFILE)

- **id**: N12 (next free id checked 2026-09-10 against `NEW-FINDINGS.md`, `ADJUDICATION.md`
  and the plan's `progress.md`: N1-N11 taken, N12/N13 pre-allocated in `progress.md` for
  exactly these two entries and used here as allocated.)
- **severity**: medium (a systematic audit-coverage hole, not one defect; it has already
  produced real gaps — N4, N6, N7 and N9 are all instances)
- **kind**: audit-coverage / gate blind spot
- **oracle**: every reference file cited by a `LOG`-class ledger row, at pin `20abdb50a`.
- **rustty**: n/a — this is a finding about the AUDIT and the GATE, not about a code site.
- **description**: `G-PARITY` counts ROWS. Every audit row cites a RANGE of reference line
  numbers, so anything OUTSIDE every row's range is unaudited and **no row will ever claim
  it**. Several large reference files have their lowest cited anchor partway down the file,
  leaving an entirely unclaimed head. Measured by sweeping every `LOG` row, taking each
  row's minimum anchor, and counting the reference's own live `log.*` calls above it:

  | unclaimed head sites | oracle file | lowest row anchor | rows |
  |---|---|---|---|
  | 38 | `src/Surface.zig` | 2193 | S41-F1 (WAIVED), S42-F2 (CLOSED) |
  | 20 | `src/apprt/gtk/class/application.zig` | 812 | S49-F5, S50-F4 (both OPEN) |
  | 14 | `src/config/Config.zig` | 4680 | S74-F5, S75-F1, S75-F2 (all OPEN) |
  | 8 | `src/apprt/gtk/winproto/x11.zig` | 225 | S60-F6 (OPEN) |
  | 7 | `src/renderer/OpenGL.zig` | 237 | S113-F1, S113-F4 (both CLOSED) |
  | 5 | `src/main_ghostty.zig` | 129 | S44-F4 (WAIVED) |
  | 2 | `src/terminal/stream.zig` | 1352 | S163-F2, S164-F1 (both CLOSED) |
  | 2 | `src/apprt/gtk/class/surface.zig` | 1045 | S54-F4, S54-F6 (both OPEN) |
  | 1 each | `terminal/page.zig`, `terminal/Terminal.zig`, `renderer/Thread.zig`, `font/CodepointResolver.zig`, `apprt/embedded.zig`, `font/glyf_rasterize_png_test.zig` | | |

  **101 unclaimed head sites over 14 files.** This table was independently reproduced from
  the ledger and the pin by task 14gate and matches the original review count exactly.

  **A head being unclaimed does NOT mean it is unported.** Three of the six largest are
  already restored and a gate that flagged them would be ignored within a day:
  `Config.zig`'s head is live (`crates/rustty-config/src/config_struct.rs:5472, 5949, 5984,
  6151` — through that file's `log_info`/`log_warn` fn facade, not `log::`);
  `OpenGL.zig`'s head ports to `crates/rustty-renderer/src/opengl/gl/sys.rs:400, 409,
  578-582` — a DIFFERENT Rust file than the name suggests, and through a `macro_rules!`
  facade; `Surface.zig`'s head was restored by task 14d-1.

- **the fix is the gate, not the rows**: `.port/tools/gates/check_log_reconcile.py`
  (`G-LOGFILE`) reconciles per ORACLE FILE, comparing the reference's live `log.*` call
  sites against live port emissions in the Rust file(s) declared to port it
  (`.port/parity-closure/LOG-PORTMAP.tsv`). Because it counts the WHOLE file it does not
  care where a row's anchors fall, so an unaudited head cannot hide in it. It is HARD in
  `ci.sh` from day one.

- **what the gate measured, and this is the tracked debt**: 63 reference files, 894 live
  reference log sites, 753 live port emissions on the thinnest platform arm —
  **a deficit of 183 sites over 33 files**, frozen as the portmap's allowances. Full table:
  `python3 .port/tools/gates/check_log_reconcile.py --report`. The ones that are NOT
  explained by an already-OPEN row:

  | deficit | oracle file | rows | note |
  |---|---|---|---|
  | 7 | `src/terminal/stream_terminal.zig` | S166-F3 — **CLOSED** | the clipboard reply/grant family |
  | 3 | `src/terminal/Terminal.zig` | S141-F4, S142-F1, S143-F2 — **all CLOSED** | `cursor copy failed entering alt screen`, the grapheme-attach debug |
  | 2 | `src/font/face/coretext.zig` | S90-F2 — **CLOSED** | |
  | 2 | `src/font/face/freetype.zig` | S91-F8, S91-F10 — **both CLOSED** | |
  | 1 each | `PageList.zig`, `page.zig`, `kitty/graphics_storage.zig` | CLOSED | |

  A `CLOSED` row sitting above a live deficit is exactly PROCEDURE.md blind spot 2 ("the
  gate counts rows, not sites") measured for the first time. These are **not** closed by
  this entry.

- **CORRECTION, 2026-09-10 (task 14gate fix round 1).** The figures first written here were
  **232 over 34 files**, with `src/terminal/stream.zig` as the largest single instance at
  **49**, attributed to "repeated message families collapsed onto one port site". **That
  defect does not exist.** `stream.zig` reconciles: 128 reference sites against **138** port
  emissions. The 49 was a scanner defect, not a port defect.

  `check_log_reconcile.py`'s Rust stripper had no char/byte-literal branch.
  `crates/rustty-vt/src/stream.rs:3428` is `b'"' => {` — the DEC intermediate. That quote
  opened a phantom string, everything to the next `"` was blanked as string body, and the
  desync ran to end of file: ~1200 lines of real code erased before the scan, 59 live
  emissions invisible, all of them at line 3446 or later. The Zig side always handled char
  literals correctly (`zig_strip`); only the Rust side forgot, and Rust is where it matters,
  because that stripper is whole-file while a Zig desync cannot escape its line.

  Two things worked and one did not.
  - **The ratchet worked.** With the scanner fixed, the gate went RED by itself:
    `src/terminal/stream.zig: allowance=49, measured deficit=0`. The stale-allowance arm is
    what surfaced the phantom, exactly as designed.
  - **The synthetic self-test did not, and could not.** All 16 arms stayed green through
    the whole thing, because a scanner desync depends on real file content. The gate now
    carries a **live-tree arm** that checks the scanner against a naive grep over every
    mapped port file and fails if any raw `log::` occurrence is dropped without a comment
    marker to account for it. Reverting the fix makes that arm fail, first violation at
    `stream.rs:3446`.
  - **Blast radius, verified independently by running every mapped port file through both
    the old and the fixed stripper**: 33 of 68 files' stripped TEXT differs (any file with a
    char literal), but only `stream.rs` differs in emission COUNT — 79 -> 138. The other 32
    files' counts and allowances were and are correct. The bug was latent and general
    though: any port file that later gains an odd number of quote-bearing char literals
    would silently zero everything below it, and it fails FALSE-GREEN.

- **three port-side facts a naive scanner gets wrong**, all found while building the gate
  and all of which cause a FALSE report if missed:
  1. **There are four emission shapes, not one.** `log::warn!(target: SCOPE, …)` is only
     the commonest. `crates/rustty-renderer/src/opengl/gl.rs` defines `macro_rules!
     log_warn!` and its leaf modules emit through that; `crates/rustty-config/src/
     config_struct.rs` and much of `rustty-os` use a `fn log_warn(msg)` facade;
     `crates/rustty-vt-c/src/sys.rs::log_fn` ends in an embedder-installed **C callback
     pointer** and contains no Rust write primitive at all. A `log::`-only scan reads
     `rustty-vt-c` as emitting nothing when it in fact reconciles exactly (2/2, 5/5, 1/1,
     1/1) and reads `gtk/class/application.rs` as **zero** when it emits **96**.
  2. **`eprintln!` is a real emission in the GTK tree** — `rustty-apprt`'s GTK files
     reproduce Zig's default `logFn` OUTPUT inline, prefix and all
     (`eprintln!("warn(gtk): …")`). The prefix is the signal: a bare `eprintln!` is
     ordinary program output (help-text failures, test-harness skip lines) and counting it
     would manufacture emissions the reference never asked for.
  3. **A `log_*` helper with an empty body is a DROPPED diagnostic, not a facade.**
     `rustty-os/src/locale.rs` and `cf_release_thread.rs` carry 16 such no-op stubs between
     them, each holding the reference call in a comment and emitting nothing. The gate
     counts their call sites as zero and lists them under its own `NOTE:` line.

- **correction to the brief this task was given**: it stated that
  `crates/rustty-apprt/src/gtk/class/application.rs` has "**zero** facade emissions". It
  has **96**, through shape (2) above; the file is 15 short of the reference's 111, not
  111 short. The brief's three named false positives are all genuinely false positives at
  the HEAD level, but two of them (`Surface.zig` −3, `Config.zig` −8) do carry a whole-file
  deficit elsewhere in the file, so "the head is restored" and "the file reconciles" are
  different claims and this finding keeps them separate.

- **how found**: a review swept every LOG row's minimum anchor and counted above it; task
  14gate reproduced the sweep, then built the gate and measured the whole-file picture.
- **A FINER INSTANCE OF THE SAME CLASS, found 2026-09-10 by task-14d-2 — this entry
  describes unaudited HEADS only, and the hole is bigger than that.** The rows citing a
  file also leave unaudited GAPS *between* their cited ranges, which no minimum-anchor
  sweep can see. `Config.zig` is the worked example: its rows cite `:4331-4336`,
  `:4413-4448`, `:4671-4730`, `:4844-4849`, `:6437-6481`, `:7355-7397`, `:8199-8253`,
  `:8719-8724`, `:9002-9309` and `:10093-10790`, and four live `log.*` sites fall in the
  gaps between them — `:4563`, `:5396`, `:5400` (unrestorable, N16) and `:8887`/`:8893`
  (restorable, restored by task-14d-2 under their own heading). All five are BELOW the
  file's lowest anchor, so the head sweep counted none of them. Only the whole-file
  reconciliation `G-LOGFILE` performs catches this variant, which is a second argument for
  the gate being per-file rather than per-row. Any future sweep of this class must count
  set-difference against the union of a file's ranges, not against their minimum.
- **state**: **OPEN.** The gate and the corrected baseline are landed; the 183-site debt is
  not. Next step is a per-file adjudication pass in deficit order — `embedded.zig` (54, all
  54 reference calls survive only as R1 comments) first, then `application.zig` (15),
  `locale.zig` (14) and `cf_release_thread.zig` (9) — tightening the portmap allowance in
  the same commit as each restoration (the gate is a ratchet and goes RED on slack).

## N13 — two `self_.?` elisions turn a Zig safe-build panic into silent UB on an ABI-nullable pointer

- **id**: N13 (allocated with N12 above; ids checked 2026-09-10)
- **severity**: medium (undefined behaviour on a null pointer where the reference has a
  defined, loud failure; unreachable in practice today, but only by construction)
- **kind**: safety-check elision / uncited site
- **oracle**: `src/os/cf_release_thread.zig` `wakeupCallback` and `src/renderer/Thread.zig`
  `wakeupCallback`, both `const t = self_.?;`.
- **rustty**: `crates/rustty-os/src/cf_release_thread.rs:269` and
  `crates/rustty-core/src/renderer_seam.rs:1679` — both replace the unwrap with the comment
  `// const t = self_.?;  (non-null by construction)` and then use the raw `self_` pointer.
- **description**: the reference's `.?` on an optional pointer is a **checked** unwrap: in
  a safe build a null userdata panics with `attempt to use null value`, loudly and at the
  call site. The port elides the check entirely and dereferences `self_` unconditionally,
  so the same input is undefined behaviour instead.

  The "non-null by construction" justification is weaker than it reads, because the
  pointer is **nullable by ABI on this very path**:
  `crates/rustty-xev/src/common.rs:23-29`'s `userdata_value` returns
  `core::ptr::null_mut()` whenever the userdata type is `()`, faithfully reproducing the
  reference's `Userdata == void` special case. Any watcher registered with unit userdata
  therefore hands these callbacks a null `self_`. Today no caller does; nothing in the type
  system or in a debug assertion says so, and nothing fails loudly if that changes.
- **why it is here and not in a row**: no ledger row cites either line. It is **not a LOG
  defect** — no diagnostic was dropped at either site (both keep their `log.err` arms; the
  Thread one emits, the cf_release_thread one goes through a no-op stub tracked under N12's
  point 3). It surfaced only because both files were being read for the logging sweep.
- **the cheap fix**: `assert!(!self_.is_null(), "self_.? — the reference unwraps a checked
  optional here")` at both sites, or a `debug_assert!` plus a PORT NOTE if the release-mode
  branch is unacceptable. Either restores a defined failure and documents the invariant the
  comment currently only asserts in prose.
- **how found**: task 14gate, while enumerating `rustty-os` and `rustty-core` emission
  sites for the G-LOGFILE portmap.
- **state**: **OPEN.** Not fixed here — this task is a gate, and adding assertions to two
  callbacks is a code change outside its scope.
## N14 — the GTK subtree writes 244 diagnostics straight to stderr, bypassing the log facade entirely

- **id**: N14 (next free id checked 2026-09-10: N1-N13 taken, N12/N13 by this same task.)
- **severity**: medium (a whole subtree's diagnostics cannot be silenced, levelled, scoped
  or captured; no state or rendering difference)
- **kind**: facade bypass / cross-cutting
- **oracle**: every `log.*` in `src/apprt/gtk/**` — `class/application.zig` (111 sites),
  `class/surface.zig` (63), `class/window.zig` (20), `portal/OpenURI.zig` (19),
  `winproto/x11.zig` (11) and the rest of the subtree — all of which the reference routes
  through `std.log`, i.e. through `main_ghostty.zig`'s `logFn`.
- **rustty**: `crates/rustty-apprt/src/gtk/**`. **244 raw `eprintln!` calls shaped as log
  lines** (189 `"warn(…"`, 30 `"debug(…"`, 13 `"err(…"`, 7 `"info(…"`, 5 `"error(…"`)
  across 19 files, reproducing the reference's `logFn` OUTPUT — `level(scope): message` —
  inline, character for character. **94** of them are in `class/application.rs` alone, 59
  in `class/surface.rs`, 18 in `class/window.rs`, 13 in `portal/open_uri.rs`, 12 in
  `class/global_shortcuts.rs`, 10 in `class/split_tree.rs`, 9 in `post_fork.rs`.

  **The bypass is NOT confined to the GTK tree, and an earlier revision of this entry said
  it was.** It claimed one further site outside that tree
  (`crates/rustty-font/src/glyf_rasterize/glyf_rasterize_png_test.rs:418`) and that "it is
  the only one". **False.** That sweep only searched `crates/rustty-apprt/src/gtk/**` plus
  whatever the portmap already declared, and the portmap at the time covered 63 of the
  reference's 172 log-bearing files — so the claim was scoped to what had been looked at and
  written as if it were scoped to the tree. 31 further sites exist across five files, all
  verified by hand:

  | sites | port file | reference |
  |---|---|---|
  | 16 | `crates/rustty-vt/src/search/thread.rs` | `src/terminal/search/Thread.zig` |
  | 5 | `crates/rustty-vt/src/search/terminal.rs` | `src/terminal/search/terminal.zig` |
  | 4 | `crates/rustty-vt/src/search/screen.rs` | `src/terminal/search/screen.zig` |
  | 5 | `crates/rustty-font/src/sprite/face.rs` | `src/font/sprite/Face.zig` |
  | 1 | `crates/rustty-font/src/atlas.rs` | `src/font/Atlas.zig` |

  With the portmap completed (finding N15) the gate now measures **276 facade-bypassing
  emissions across 25 reference files' ports**, all capped. That is the honest total for the
  declared universe, and it is 39% larger than the GTK-only figure this entry first carried.

  **These figures were corrected in fix round 2 and the correction is instructive.** They
  were first filed as 199 / 157-22-9-6-5, with 98 in `application.rs`, from a line-oriented
  `grep`. The real numbers come from running **the gate's own `EPRINTLN_LOGFN` regex**,
  which matches multi-line call sites (`eprintln!(` with the format string on the next
  line) because `\s*` spans newlines. So a finding filed to document a facade bypass was
  itself counted informally rather than through the instrument built to stop informal
  counting — the same class of error as the scanner bug in fix round 1. The qualitative
  finding is unaffected and was understated: the bypass is 23% larger than reported.
- **description**: the reference's `logFn` (`src/main_ghostty.zig`) is a real funnel. Its
  port is `crates/rustty-core/src/logging.rs::log_with(level, scope, message, &Logging)`,
  and it does three things these sites do not:
  1. **it honours `Logging.stderr`** (`crates/rustty-core/src/logging.rs:88` returns early
     when it is false) — and `rustty_global::Logging`'s own doc says stderr logging is
     **disabled by default in lib mode**. So an embedder that has switched stderr logging
     off still gets all 199 of these on its stderr;
  2. **it routes to macOS unified logging** when `Logging.macos` is set
     (`logging.rs:56`) — these sites never reach it, so on macOS the GTK subtree is absent
     from `log stream` entirely;
  3. **it is the single point where level and scope filtering can ever be applied.**
     A raw `eprintln!` has no level to filter on and no scope to route by.

  The facade is not merely available, it is **already imported by the same crate**:
  `crates/rustty-apprt/src/lib.rs:189` does `use rustty_core::logging::{log_with, Level};`
  and `global.zig`'s eleven sites go through it correctly. The GTK subtree simply does not,
  and `rustty-apprt/Cargo.toml` does not list `log` as a dependency at all, so the
  `log::warn!` shape used by every other crate is not even reachable from there.
- **why this is a finding and not part of N12**: N12 is about diagnostics that are MISSING
  (`gtk/class/application.zig` is 15 sites short of the reference's 111). This is about the
  96 that are PRESENT and go to the wrong place. Restoring the missing 15 as further
  `eprintln!`s would close N12's arithmetic for that file and make this defect worse. The
  two must be fixed in one pass or the wrong one will be done first.
- **how found**: task 14gate. The gate had to learn the `eprintln!("warn(gtk): …")` shape
  in order to count these as emissions at all — a `log::`-anchored scan reads
  `class/application.rs` as emitting **zero** when it emits 96 — and recognising the shape
  is what exposed what the shape costs.
- **the gate now enforces it (fix round 2).** `LOG-PORTMAP.tsv` gained a **`bypass_cap`**
  column: how many of a file's emissions may bypass the facade, frozen at today's
  measurement, MAX over platform arms (a bypass on any arm is a bypass), with the same
  ratchet as the allowances — a cap larger than what is measured is RED, so routing sites
  through the facade must tighten it in the same commit. **276 sites are capped across 25
  files** once the portmap was completed (N15). Before that completion only 188 were, in six
  files, because the portmap covered only what the audit's `LOG` rows named.

  This exists because *"restore the missing sites"* and *"stop bypassing the facade"* pull
  in opposite directions. G-LOGFILE counts a logFn-shaped `eprintln!` as an emission, so
  before this column a stage could have closed `application.zig`'s 15-site deficit by adding
  fifteen more direct prints and the gate would have reported an improvement.
- **state**: **OPEN.** Scope: route the GTK subtree through `rustty_core::logging::log_with`
  (or add `log` to `rustty-apprt` and use `log::warn!(target: …)` as the rest of the
  workspace does), preserving each message verbatim, in the same pass that restores the
  sites N12 counts as missing. The count-based half of the gate is neutral to the fix (both
  shapes count as emissions); the `bypass_cap` half is what will move, downward, and the
  ratchet forces it to.
## N15 — the portmap was seeded from the audit's LOG rows, so it inherited the audit's blind spots: 112 reference files and 523 diagnostics were outside the gate entirely

- **id**: N15 (next free id checked 2026-09-10 against `NEW-FINDINGS.md` headings, a
  workspace grep for `\bN[0-9]+\b`, and the plan's `progress.md`: N1-N14 taken.)
- **severity**: high (not a defect in the port, a defect in the INSTRUMENT — the gate built
  to stop the audit's blind spots was reproducing them one level up)
- **kind**: gate coverage / audit-inheritance
- **oracle**: the whole reference tree at pin `20abdb50a`.
- **rustty**: `.port/parity-closure/LOG-PORTMAP.tsv`, `.port/tools/gates/check_log_reconcile.py`.
- **description**: `G-LOGFILE` reconciles the reference files listed in its portmap, and the
  portmap was seeded from the reference files cited by `LOG`-class ledger rows. **So the
  gate could only ever see what the audit had already looked at.** N12 records the
  unaudited-HEAD problem — regions of covered files that no row's line range claims. This is
  the same failure at FILE granularity, and it is much larger.

  Measured 2026-09-10 by walking the reference tree directly:

  | | before | after |
  |---|---|---|
  | reference files with live `log.*` sites | **172** | 172 |
  | live reference sites in them | **1417** | 1417 |
  | declared in `LOG-PORTMAP.tsv` | 60 (+3 with zero sites) | **170 (+2 excluded)** |
  | **undeclared** | **112** | **0** |
  | live sites the gate could not see | **523 (37%)** | **2** |
  | site coverage | **63%** | **99%** |

  The largest single log-bearing file in the entire reference — `src/termio/Exec.zig`, 58
  live sites — was not in the portmap. Nor were `termio/stream_handler.zig` (41),
  `terminal/tmux/control.zig` (19), `terminal/kitty/graphics_image.zig` (18),
  `terminal/search/Thread.zig` (18), `apprt/gtk/class/split_tree.zig` (15),
  `apprt/gtk/class/global_shortcuts.zig` (14), `termio/Thread.zig` (14).

  **And silence was indistinguishable from coverage.** Nothing in the gate's output
  distinguished "this file reconciles" from "this file was never looked at". A green
  G-LOGFILE read as "diagnostics reconcile" when it meant "the 63% we declared reconcile".

- **why the numbers here differ slightly from the review's**: the review reported 180 files
  / 120 undeclared / 550 sites, and `Exec.zig` at 61. This entry says 172 / 112 / 523 and
  58. The gap is **calls commented out in the reference itself**, which this gate
  deliberately does not count — demanding an emission the reference does not make would be a
  false positive, and the same definition is what reproduced the review's own head table
  (38/20/14/8/7/5) and its `generic.zig` inventory (33) exactly. The conclusion is identical
  either way.

- **the fix (task 14gate fix round 3)**: three parts.
  1. **The gate now enumerates the reference tree itself** rather than trusting the portmap
     to be complete. Any reference file with live `log.*` sites that is neither declared nor
     excluded is **RED**, with the count of sites it hides. A coverage hole is now a finding,
     not a silence.
  2. **The portmap declares all 172.** 170 with their port arms — resolved from PORT_PLAN's
     `crate_map`, then corroborated file by file (91 of 93 auto-resolved candidates confirmed
     by the port file naming the reference file or reproducing its message text; the
     remainder resolved by hand against the reference's own log strings, which is how
     `pkg/opengl/Program.zig` -> `opengl/gl/program.rs`, `src/main_c.zig` ->
     `rustty-apprt-c/src/lib.rs` and `src/build/*` -> `xtask/src/*` were pinned).
  3. **Two are declared EXCLUDED**, and an exclusion must name a category from a closed set
     (`no-port`, `build-tooling`, `vendored`, `unresolved`) or the gate is RED — a free-text
     excuse is how a coverage hole becomes permanent, whereas a category can be counted and
     argued. They are `pkg/sentry/build.zig` (`build-tooling`) and
     `pkg/macos/text/font_collection.zig` (`unresolved` — one debug dump in a vendored
     CoreText binding whose port target is not established; declared so the gap is counted
     rather than invisible).

- **the baseline grew, and that is the gate becoming truthful**: deficit **183 over 33 files
  -> 535 over 113 files**; facade bypass (N14) **188 over 6 files -> 276 over 25 files**;
  port emissions seen **753 -> 924**. **No diagnostic was lost and nothing regressed.** The
  ~350 additional deficit sites were always missing; the gate simply could not see the files
  they were missing from. Anyone comparing the two baselines must compare coverage first.
- **NOT fixed here, deliberately**: none of the 535 sites were restored. That is stage work
  and far larger than a fix round. The gate measures it, the portmap declares it, and the
  ratchet forces each restoration to tighten its row.
- **how found**: the fix-round-2 review, chasing N14's false completeness claim, asked why
  five obviously log-bearing files were not capped — and the answer was that they were not in
  the portmap at all, because no `LOG` row names them.
- **the exclusion list is now budgeted too (fix round 4).** Completing the portmap created a
  new lever and left it unratcheted. An exclusion RETIRES a reference file instead of
  reconciling it, so it erases that file's entire deficit: the review demonstrated that
  reclassifying `src/termio/Exec.zig` as `no-port: nothing implements this` made **58 sites
  of deficit vanish and the gate stayed GREEN** (coverage 99% -> 95%, exclusions 2 -> 3, and
  nothing counted either). Allowances ratchet and bypass caps ratchet; this did not, and it
  is the widest lever in the gate. The portmap now carries a required
  `#!exclusion-cap files=2 sites=2` directive, frozen at today's two files and two sites, on
  the same two-way ratchet: over budget is RED, UNDER budget is RED (tighten it in the commit
  that brings a file back into scope), and a missing directive is BLOCKED — a cap that can be
  disabled by deleting a line is not a cap. Re-running the review's probe now yields
  `RED: 3 excluded file(s), budget 2` and `RED: 60 excluded site(s), budget 2`.
- **two of the 110 new mappings are NOT corroborated**, and their portmap rows now say so at
  the row rather than only in a report: `src/unicode/props_uucode.zig` and
  `src/unicode/symbols_uucode.zig` were resolved by an exact filename-stem match alone — the
  Rust file neither names the reference file nor reproduces its one log message, so both
  independent checks that confirmed the other 107 failed there. One site each; accepted
  because the stem match is unambiguous, flagged because it is the weakest evidence in the
  file.
- **state**: **OPEN.** The instrument is fixed and the debt is declared. Working it down is a
  per-file adjudication pass in deficit order, starting with `termio/Exec.zig` (58),
  `apprt/embedded.zig` (54) and `termio/stream_handler.zig` (41).

## N16 — three `Config.zig` `log.warn` sites are UNRESTORABLE, not dropped: their Rust counterparts are infallible

- **id**: N16 (next free id checked 2026-09-10 against `NEW-FINDINGS.md`, `ADJUDICATION.md`
  and the plan's `progress.md`: N1-N15 taken, N15 by task-14gate round 3 the same day.)
- **severity**: low (three diagnostics the port cannot emit; no state, control-flow or
  rendering difference on either side)
- **kind**: unrestorable-diagnostic / unclaimed
- **oracle**: `src/config/Config.zig:4563` (`expandHome`, `global.environMap()` catch),
  `:5396` (`Replay.next`, `diag.clone` catch), `:5400` (`Replay.next`,
  `_diagnostics.append` catch) — pin `20abdb50a`.
- **rustty**: `crates/rustty-config/src/config_struct.rs` — the `expandHome` non-wasm arm
  and the `ReplayStep::Diagnostic` arm of `Replay::next`.
- **description**: each of the three reference sites is the body of a `catch |err|` arm.
  The port's counterpart for each is INFALLIBLE, so there is no failure arm to log from:
  - `global.environMap()` -> `rustty_global::environ_map()`, which returns a snapshot of
    the process-global captured environ and has no error path at all;
  - `diag.clone(arena_alloc)` -> `Diagnostic::clone_in(&self) -> Self`, which takes no
    allocator and returns no `Result`;
  - `self.config._diagnostics.append(arena_alloc, cloned)` ->
    `DiagnosticList::append(cloned)`, which returns `()`.

  Restoring any of the three would put a diagnostic on a SUCCESS path — the bug this plan
  has already shipped once (a scroll warning outside its `catch` block firing on every
  successful scroll). They are therefore recorded, not deferred.
- **how found**: task-14d-2, reconciling `src/config/Config.zig` site by site through
  `check_log_reconcile.py`. The file measures 29 reference sites against 26 live
  emissions after that task; these three ARE that residual 3, and the portmap's allowance
  and `reason` name them.
- **why this is recorded separately**: **no LEDGER row cites any of the three.** The rows
  that name `Config.zig` cite `:4331-4336`, `:4413-4448`, `:4671-4730`, `:4844-4849`,
  `:6437-6481`, `:7355-7397`, `:8199-8253`, `:8719-8724`, `:9002-9309`, `:10093-10790` —
  `:4563`, `:5396` and `:5400` fall in none of them. This is exactly the PROCEDURE.md
  blind-spot-4 / N12 class, one level finer: not an unaudited file HEAD, but unaudited
  GAPS between a file's cited ranges.
- **what would change this**: if `rustty-config` ever gives any of the three operations a
  real error path — most plausibly `environ_map()`, were the environ capture made lazy and
  fallible — the corresponding `log.warn` becomes restorable and this entry closes with
  it. A PORT NOTE at each of the three sites says so.
- **state**: **OPEN** as a tracked non-defect. `src/config/Config.zig`'s G-LOGFILE
  allowance of 3 is these three sites and nothing else; if that number ever exceeds 3,
  something else has been dropped.

---

## N17 — the `log` facade's `.default` scope (`target: ""`) rendered `warning(): msg`, not `warning: msg`

- **id**: N17 (next free id checked 2026-09-10 against `NEW-FINDINGS.md`, `ADJUDICATION.md`
  and the plan's `progress.md`: N1-N16 taken, N16 by task-14d-2 the same day.)
- **severity**: low (a two-character difference in every `.default`-scope diagnostic's
  prefix; no state, control-flow or rendering difference)
- **kind**: emission-fidelity / cross-seam spelling mismatch
- **state**: **FIXED by task-14d-3** in the same commit that found it. Recorded because the
  CLASS is worth knowing, not because anything is left open.
- **where**: `crates/rustty-core/src/logging.rs` — `AppLogger::log` (the facade -> sink
  adapter). Live victims at `crates/rustty-renderer/src/overlay.rs:349` and
  `crates/rustty-renderer/src/shadertoy.rs:833`, the workspace's only two
  `log::warn!(target: "")` sites before this stage.
- **description**: two documents describe the same thing with two different spellings, and
  nothing bridged them.
  - `DECISION-logging.md` §6 fixes the facade convention: pass `target: ""` for the
    reference's `.default` scope.
  - `logging::log_with` is the port of `main_ghostty.zig`'s `logFn`, whose `scope` is a Zig
    ENUM and whose default arm is `if (scope == .default)`. The port spells that arm
    `if scope == "default"`.

  So a `log::warn!(target: "", ...)` fell through to the SCOPED branch and rendered
  `warning(): Error drawing hyperlink border: ...` where the reference prints
  `warning: Error drawing hyperlink border: ...`. On macOS it additionally created an
  empty unified-logging category.
- **how found**: task-14d-3, converting `crates/rustty-apprt-c/src/lib.rs`'s three
  `src/main_c.zig` sites (which are `.default`-scope) from the direct sink call to the
  facade. The defect was in the destination, not in the sites being moved.
- **fix**: `scope_from_target(target)` in `logging.rs` maps `""` -> `"default"` and passes
  everything else through. Normalized at the FACADE ADAPTER rather than inside `log_with`,
  so `log_with` stays a faithful port of `logFn` (whose `scope` can never be empty) and the
  Rust-only spelling mismatch is confined to the Rust-only bridge. Test
  `empty_facade_target_maps_onto_the_reference_default_scope`; negative control run.
- **the general lesson**: when a convention is fixed in a DECISION doc and the code that
  consumes it is a port of something with a different type (an enum, here), the two
  spellings meet at exactly one function — and that function is where the port has to be
  written down and tested. No count-based gate can see this: both spellings emit.

---

## N18 — `App.keymap` is EAGER in the port and LAZY upstream, which costs `embedded.zig:251`'s diagnostic

- **id**: N18
- **severity**: low (one `log.warn` the port cannot emit; the structural difference itself
  is benign on every path the port exercises)
- **kind**: upstream-drift / structural, surfaced as an unrestorable diagnostic
- **state**: **CLOSED by task 15**, with `S46-F1` — the row that tracks the structural
  difference itself. `App.keymap` is now `UnsafeCell<Option<Keymap>>`, initialized lazily in
  `keyboardLayout`, and `embedded.zig:251`'s `log.warn("error initializing keymap err={}")`
  is live at that site. `App::init` became infallible in the same change (the reference's
  `try app.init(..)` only ever carried `config.clone`'s allocator error, and rustty's
  `clone_config` is infallible), which retired `AppNewError::Keymap` in `rustty-apprt-c`.
  `S46-F5`'s waiver drops from 8 unrestorable sites to 7 accordingly.
- **where**: reference `src/apprt/embedded.zig:243-256` (`App.keyboardLayout`) and `:194-200`
  (`App.init`); port `crates/rustty-apprt/src/embedded.rs` `App::init` / `App::keyboardLayout`
  (PORT NOTE at the latter).
- **description**: at pin `20abdb50a` the reference stores `keymap: ?input.Keymap = null` in
  `App.init` and LAZILY initializes it on the first `keyboardLayout()` call:

  ```zig
  self.keymap = input.Keymap.init() catch |err| {
      log.warn("error initializing keymap err={}", .{err});
      return .unknown;
  };
  ```

  This port initializes `keymap` EAGERLY in `App::init`, where the reference's own `init`
  does not log at all — it `try`s and propagates. So there is no arm in the port that
  corresponds to `:251`, and emitting the warning from `App::init` would put it on a path
  the reference does not log from.
- **how found**: task-14d-3, enumerating all 54 of `embedded.zig`'s reference sites through
  `check_log_reconcile.py` while closing S46-F5. `:251` is ABOVE that row's lowest cited
  anchor, i.e. the PROCEDURE.md blind-spot-4 / N12 class again.
- **what would change this**: porting the lazy keymap (`keymap: Option<Keymap>` +
  initialize-on-first-`keyboardLayout`). That is a structural port, not logging work, and
  it also changes when a keymap-init failure becomes visible to an embedder — which is why
  it is filed here rather than folded into a LOG row. **Done in task 15** (see state).

---

## N19 — a whole crate's diagnostics were invisible to G-LOGFILE: undeclared port file + an emission shape that counts as neither

- **id**: N19 (next free id checked 2026-09-10 against `NEW-FINDINGS.md`, `ADJUDICATION.md`
  and the plan's `progress.md`: N1-N18 taken, N17/N18 by task-14d-3 the same day.)
- **severity**: medium (a hole in the "99% of the reference's log sites are reconciled"
  claim, in the file the plan calls its largest single gap)
- **kind**: gate-coverage / measurement
- **state**: **FIXED by task-14d-3** for the instance found. The CLASS is open — see
  "what to do about it".
- **where**: `LOG-PORTMAP.tsv`'s `src/apprt/embedded.zig` row, and
  `crates/rustty-apprt-c/src/lib.rs`.
- **description**: two independent invisibilities stacked on the same 23 diagnostics.

  1. **The port file was not declared.** `src/apprt/embedded.zig`'s `CAPI` block ports to
     `crates/rustty-apprt-c/src/lib.rs` — a DIFFERENT CRATE from the rest of the file.
     The portmap declared only `crates/rustty-apprt/src/embedded.rs`. So 23 of the file's
     54 sites were mapped to a file that could not carry them, and no amount of work on
     the declared file could ever have credited them.
  2. **The emissions there counted as NEITHER an emission NOR a bypass.** 22 of the 23
     were live, as `eprintln!("error initializing app err={err:?}")`. `EPRINTLN_LOGFN`
     only matches an `eprintln!` whose format string carries a `warn(scope):`-style
     PREFIX. A bare `eprintln!` matches nothing, so it is not counted as an emission
     (`primitive_sites`) and not counted as a bypass (`direct_print_sites`) either.

  The combination is worse than either half. `src/apprt/embedded.zig` reported
  `54 reference / 0 port` — a maximally alarming number — while 22 of the 23 "missing"
  CAPI diagnostics were in fact being printed, just unmeasurably, from a file the gate
  had never been told about.
- **how found**: task-14d-3, closing S46-F5. The row's title claims the whole file, so
  every one of the 54 had to be located; 23 of them were not in the declared port file.
- **fix applied**: the arm is now
  `crates/rustty-apprt/src/embedded.rs+crates/rustty-apprt-c/src/lib.rs` (a declared
  overlap with `src/main_c.zig`, recorded in both `reason` cells), and all 22 live sites
  are converted to `log::` facade calls, so both ratchets can see them.
- **why this is the SAME disease N15 treated, in a place the treatment did not reach**:
  N15 fixed "the gate's FILE LIST was seeded from the audit" by deriving the reference-side
  universe from the reference tree. That closes the question "which REFERENCE files exist".
  It does not touch the other side: **the PORT-side arm of every row is still a human
  guess, and nothing checks it.** A reference file mapped to the wrong port file, or to
  only half of the right ones, reads as a deficit forever and no gate arm objects.
- **what to do about it (for the gate task)**: there is no cheap machine check for "is this
  arm the right file". There IS a cheap check for the SECOND half: a bare `eprintln!`
  inside a file that any portmap arm names is, with very high probability, a dropped
  diagnostic rather than program output — it could be listed under the existing `NOTE:`
  line the way empty `log_*` stubs already are. That would not have declared the missing
  crate, but it would have made 22 invisible diagnostics visible as a question.
- **the general lesson**: **deriving a gate's universe from the reference fixes only the
  reference side of a two-sided mapping.** N15's rule ("derive the universe from the
  reference, never from the audit") is right and incomplete: the port side of the same
  mapping is still audit-derived, and it inherits exactly the blind spots N15 was written
  to remove.

## N20 — stale pre-resync Zig comments reproduce reference log calls that no longer exist at the pin (MACHINE-CHECKED since 2026-09-11)


> **MACHINE-CHECKED SINCE 2026-09-11 (task 14gate fix round 5).** `G-LOGFILE` now rides the
> message-content scan (N21) to detect this automatically: every preserved reference comment
> whose message appears nowhere in the reference at the pin — live OR commented-out — is
> reported, and must be declared as `stale-comment` in `LOG-TEXT-DIVERGENCES.tsv` or the gate
> is RED. **21 are declared**, well beyond the three this entry was opened for.
>
> **Measured 2026-09-11 by walking `is_stranded_comment`'s branches in the order the code
> applies them**, over the 1581 preserved reference comments in mapped port files (down from
> 1588 now that test spans are excluded here too):
>
> | # | branch | count |
> |---|---|---|
> | 1 | message could not be extracted (empty) | 69 |
> | 2 | elided (`…` / `...`) | 17 |
> | 3 | short or one-word | 9 |
> | 4 | exact match in the reference index (live **or** commented-out) | 1459 |
> | 5 | substring of a real reference message | 0 |
> | 6 | literal-prefix match before the first placeholder | 6 |
> | 7 | **STRANDED** | **21** |
> | | **sum** | **1581** |
>
> This table has now been wrong twice. The first version attributed the filters by estimate
> (elided ~10, short ~5, substring ~24, commented-index 36) and not one component survived
> measurement, though they summed correctly. The second version was measured — but against a
> DIFFERENT branch order than the code uses (it split the index into live and commented-out
> and tested them first), so four of its six numbers were still wrong and the six summed to
> 1590 against a stated 1588. **The gate was right both times; the table about the gate was
> not.** It is now derived by executing the gate's own predicate, branch by branch, and the
> column sums to the stated total.
>
> The short-message threshold was also a real FALSE NEGATIVE at 14 characters: it suppressed
> `exec.rs:4421`'s `"DATA: {}"`, a genuine stranding. It is 6 now, which adds exactly that
> one site. So "each filter removes only a legitimately non-verbatim class" is true of the
> common case and is not a guarantee.
>
> Each declaration is classified from the message alone and is NOT individually traced to the
> upstream commit that removed it — that tracing is still open work.

- **id**: N20 (next free id checked 2026-09-10 against `NEW-FINDINGS.md`, `ADJUDICATION.md`
  and the plan's `progress.md`: N1-N19 taken, N17/N18/N19 by task-14d-3 the same day.)
- **severity**: low individually, medium as a class — each one is a standing invitation to
  introduce the INVERSE DEFECT (an emission the reference does not make), and the class is
  invisible to every gate the plan has.
- **kind**: fidelity / stale-comment
- **state**: **OPEN** as a class. The three instances found are each annotated at their code
  site and none of them received a live emission.
- **how found**: task-14e-1, restoring `rustty-termio`. The task's method is "read the Zig
  comment, add the live call beneath it" — which silently manufactures a wrong emission
  whenever the comment is stale. Enumerating the ORACLE's own `log.*` calls through the
  gate's scanner (`zig_sites`) rather than reading the port's comments is what separated
  them: each of the three has a comment in the port and NO counterpart in the reference.
- **instances**:
  - `crates/rustty-termio/src/stream_handler.rs`, the OSC 7 `UriHostTooLong` arm —
    `log.warn("failed to get full hostname for OSC 7 validation: {}", .{err})`. At pin
    `20abdb50a`, `std.Uri.getHost`'s error set is `error{UriMissingHost}` only; the arm and
    its message are from a superseded revision. The ARM still exists in the port (removing
    it is a behavioural change), so the port has a code path the reference does not.
  - `crates/rustty-termio/src/exec.rs`, `setNonblock` — the port's comment carried
    `read thread failed to get flags err={}` plus a second line,
    `this isn't a fatal error, but may cause performance issues`. At the pin the message
    renders the errno NAME (`err=E{s}`) and there is no second line at all. Corrected to
    the pin in this task.
  - `crates/rustty-termio/src/termio_impl.rs`, `processOutput`'s cursor-reset block —
    `log.warn("failed to get current time err={}", .{err})`. No such site in `Termio.zig`
    at the pin. Left as a comment (the surrounding `Instant::now()` is infallible in Rust
    regardless, so it could never have been emitted).
- **why this is recorded separately**: it is not a defect any row introduced or can close,
  and it is not confined to `rustty-termio` — the Phase 6 resync moved the pin under a
  tree whose Zig comments were transcribed against the OLD pin, so any file the resync
  touched can carry one. The three above were found in seven files; the reference has 173.
- **what to do about it**: the check is mechanical and belongs in `check_log_reconcile.py`
  as net-new coverage (R5): for every mapped port file, extract the message STRING out of
  each commented `log.*` line and assert it appears in the declared oracle file. A comment
  quoting a message the reference does not contain is either stale or invented, and both
  are worth a RED. Scoped as its own task, not folded into a restoration stage — the fix
  for a hit is a judgement call (correct the comment, or delete the arm) and should not be
  made under time pressure inside another task.
## N21 — a message that is PRESENT but WRONG reconciles perfectly: G-LOGFILE now compares text, not only counts

- **id**: N21 (next free id checked 2026-09-11 against `NEW-FINDINGS.md` headings, a
  workspace grep and `progress.md`: N1-N20 taken.)
- **severity**: medium (diagnostic-text fidelity; no state or control-flow difference, but
  it is invisible to every check this gate had until now)
- **kind**: gate coverage / message fidelity
- **oracle**: every `log.*` call in the reference at pin `20abdb50a`.
- **rustty**: `.port/tools/gates/check_log_reconcile.py`,
  `.port/parity-closure/LOG-TEXT-DIVERGENCES.tsv`.
- **description**: every check in `G-LOGFILE` up to this point COUNTS. A message that is
  emitted but wrong passes all of them. A stage found one funnel emitting only the `err=`
  half of `"error handling VT action action={} err={}"` — it emitted, so it reconciled — and
  then **reintroduced the same class four times in its own fix commit**, dropping the space
  before a placeholder so the port emits `mode:Flags { … }` where the reference emits
  `mode: <value>`. Discovering a defect class and recommitting it in the same change is the
  strongest available argument that it needs a machine.

  The check compares each live emission's format string against the reference call preserved
  as a comment above it, with **placeholder contents normalised and nothing else** — spacing
  is the payload, because `mode: {}` versus `mode:{}` is the bug. Runs of two or more spaces
  collapse to one, because an R1 comment often wraps one reference line across two comment
  lines; a one-space-to-zero change is still caught, a three-to-one change is not, and that
  is stated rather than hidden.

- **what it found across the port**: **12 divergences**, every one of them a knowing
  decision rather than a bug — 7 R2 identity renames (`ghostty` -> `rustty` / `APP_NAME`),
  2 format-shape differences with a PORT NOTE at the site, 1 absent payload (N18's
  `value={any}`), and 2 more with their reasons at the row. **The class is therefore not
  widespread — but that is only knowable because something went looking.**

  **CORRECTION (2026-09-11).** This first read *16*, and **three of them were phantoms
  produced by the checker's own pairing rule**, declared rather than diagnosed. The first
  `pair_messages` fell back to the oldest pending comment whenever an emission did not match
  one exactly, so the moment the port adds an emission the reference lacks, or restores a
  block out of order, a later emission consumes an earlier one's leftover comment and the
  check reports a divergence between two messages that were never a pair. A declared phantom
  is worse than an undetected one: it is permanent. Pairing is now BLOCK-SCOPED with a
  two-pass assignment — every exact match first, then best-similarity above a threshold, and
  **below the threshold the emission is left unchecked rather than falsely accused**. The
  three retractions are listed in `LOG-TEXT-DIVERGENCES.tsv`'s history; one of them
  (`open_uri.rs`) was a byte-exact port whose *comment* was abbreviated, and the comment was
  made verbatim instead.
- **what it CANNOT see, stated against the right denominator**: **884 of 1279 LIVE EMISSIONS
  (69%)** have a preserved reference comment to compare against. The first version of this
  line said 72%, over a denominator that counted only direct `log::`/print sites and left
  ~150 facade-routed emissions out of BOTH halves of the ratio. Three further facts print on
  every run, because the blindness is not random:
  - **3 crates have live emissions and not one checkable pair** — `rustty-config`,
    `rustty-util`, `rustty-vt-c` — so the check cannot see them at all;
  - **5 crates emit nothing**, so they are outside the ratio entirely rather than merely
    unchecked;
  - **30% of the 395 unchecked sites sit in two files** — `gtk/class/application.rs` (87)
    and `gtk/class/surface.rs` (33) — which are also the files with the largest deficits and
    the inline reproduction of reference output. **The highest-risk text in the port is the
    least checked**, and that is the argument for preserving the reference call at a site,
    not merely its message.
- **how a legitimate divergence is declared**: a row in
  `.port/parity-closure/LOG-TEXT-DIVERGENCES.tsv` — `kind`, `port_file`,
  `reference_message`, `port_message`, `reason` — with `kind` from a closed set
  (`identity-rename`, `error-name-literal`, `abbreviated-comment`, `format-shape`,
  `parameterised`, `payload-absent`, `stale-comment`) and a mandatory reason. **Undeclared
  divergence is RED**, and so is a declaration that matches nothing in the tree — a waiver
  that outlives what it excused is how an exception becomes permanent. Rows key on the
  MESSAGE PAIR, never on a line number, because line anchors in this plan have already
  drifted up to 70 lines when restoration moved code. The entry count is capped by a
  `#!divergence-cap` directive on the same two-way ratchet as everything else here.
- **proved by mutation, not by reading**: the first version of this checker (in the stage
  that found the original defect) used substring matching, scored zero mismatches, and
  **still scored zero when the bug was deliberately reintroduced**. So this one was
  mutation-tested against the live tree before it was trusted: truncating
  `"error building row y={y} err={err:?}"` to `"error building row err={err:?}"` in
  `generic/linux_impl.rs:2231` turns the gate RED naming the file, the line and both texts,
  and so does deleting the single space in `"error building rowy={y}"`. The first mutation
  I tried caught nothing — because that site has no preserved comment, which is how the 72%
  figure above came to be measured.
- **two states of "unchecked", not one (added 2026-09-11)**. An emission with NO preserved
  comment anywhere in its block is genuinely unknowable — that is the honest 31%. An emission
  that HAS candidate comments and matches none of them above the 0.55 similarity floor is a
  different and suspicious state, and the first version of this check put both in the same
  silent bucket. That is how a message truncated all the way down to its trailing clause
  (`"error building row y={} err={}"` -> `"err={err:?}"`, similarity **0.333**) moved from
  checked to unchecked with **zero signal and a GREEN gate** — the bigger the truncation, the
  quieter it got, which is exactly backwards. Lowering the floor would only bring back false
  accusations, so the second state is now counted, printed with both texts and the similarity
  score, declarable as `kind=unpairable` with a reason, and **ratcheted**: undeclared ones
  are capped at zero. One site exists today (`window.rs:1519`, similarity 0.548) and is
  declared.
- **the test-span exclusion was CLAIMED in three artifacts before it was implemented.**
  Round 6's commit message, this finding and `PROCEDURE.md` all said test code was skipped
  "for emissions, facade-bypassing prints and preserved comments alike"; the filtering
  existed only in the emission paths, and `glyf_rasterize_png_test.rs:426` is a live
  preserved comment inside a test function that the content check was still reading. It is
  implemented now, in `commented_reference_calls`, with self-test arms for both halves —
  there were none before, which is why nobody noticed.
- **state**: **OPEN** as a standing check. All 12 present divergences are declared with
  reasons; what is outstanding is the 31% of emissions the pass cannot reach, concentrated
  as described above, plus the note under N22 that the 1415 reference-site denominator is
  itself 8 too high.

---

## N22 — G-LOGFILE counts `log.*` sites inside reference functions that the reference never calls (CLOSED 2026-09-11 by a site-level dead-site declaration)

**Found 2026-09-10 (task-14e-2), while accounting for the last 10 sites of `src/font/shaper/`.**

`check_log_reconcile.py`'s `zig_sites()` is a LINE scanner. `zig_strip` correctly refuses to
count a `log.*` call that is **commented out** — `Surface.zig` alone carries six — but nothing
in it can see that a whole **function** is dead. Two reference files have one:

| reference | function | its only call site, at pin `20abdb50a` | sites counted |
|---|---|---|---|
| `src/font/shaper/coretext.zig` | `debugPositions` (`:690`) | `:503` — `//try self.debugPositions(...)` | 4 (`:775, :793, :806, :820`) |
| `src/font/shaper/harfbuzz.zig` | `debugPositions` (`:302`) | `:236` — `//try self.debugPositions(...)` | 4 (`:379, :396, :409, :421`) |

So **8 of the tree's 1415 "live reference log sites" are diagnostics the reference itself
never emits.** They are counted as deficits against the port, and any stage that "restores"
them creates eight emissions the reference does not make — the inverse defect (N20's class,
reached by a different route).

**Not fixed here, deliberately.** Teaching the scanner about dead functions means resolving
Zig call graphs, which is a much larger scanner than this gate has, and R5 requires a gate
change to be net-new coverage with a self-test arm and an argument. The two files' portmap
`reason` cells now say precisely this instead, so the allowances are not read as "8 dropped
diagnostics".

**The generalisable point.** A line-oriented liveness test on the REFERENCE side has the same
blind spot the port-side scanner had before its char-literal fix: it is exact about the syntax
it examines and silent about the context around it. When a gate measures "the reference does
X", check whether X is *reachable*, not only whether it is *written*.

The port is consistent with this reading already: `coretext.rs` and `harfbuzz.rs` both preserve
`debugPositions` verbatim as a comment with a PORT NOTE saying it is never called upstream.
Making it live would additionally need `unicode.table.get(cp).grapheme_boundary_class`, which
`rustty-unicode` does not carry (checked: no `grapheme_boundary_class` anywhere in that crate).

**state**: OPEN as a measurement caveat. No port change is owed.

---

## N23 — `src/config/CApi.zig` was 11-short and no row claimed it; 7 restored, 4 unrestorable

**Found 2026-09-10 (task-14e-2).** The file entered the portmap with N15 and no `LOG`-class
row cites it, so nothing in the audit ever looked at it.

All eleven `catch` arms wrote a bare `eprintln!`. That is worse than it looks: `logFn`'s
stderr branch is gated on `global.logging().stderr`, which `global.zig`'s `init` sets **false
when a CLI action is detected** precisely so action output stays clean — and a bare
`eprintln!` ignores that gate entirely. So `rustty config --help`-style paths could emit
diagnostics the reference suppresses, on top of losing the `error(config): ` prefix.

Seven are restored through `config_struct::log_err`, the crate's own sink (§6r4's one
documented exception to the facade). `log_err` is new; `log_info`/`log_warn`/`log_debug`
already existed, which is itself worth noting — the crate had no `err`-level sink at all, so
every `log.err` site in `rustty-config` had nowhere to go.

The remaining four are the **N16 class — UNRESTORABLE, not dropped**:


> **CLOSED 2026-09-11 (task 14gate round 8).** The eight sites are declared in
> `.port/parity-closure/LOG-DEAD-SITES.tsv` and subtracted from the site inventory and from
> their files' reference counts, so **the denominator is 1407, not 1415**, and every coverage
> figure derived from it is corrected. NOT via the file-level `DECLARED-EXCLUDED` mechanism:
> `coretext.zig` still has four genuinely live sites at `:254, :264, :277, :291`, so retiring
> the file would have erased real debt and spent the exclusion budget on something that is
> not an exclusion. The declaration is per SITE, a bogus line is RED, the kind comes from a
> closed set, a reason is mandatory, and the population is capped at 8 on a two-way ratchet
> because an undeclared dead site is undetectable by construction.
>
> **And it is net-new coverage, not a subtraction (R5): a port emission carrying a
> declared-dead site's message is RED.** Nothing else in this gate can catch someone
> faithfully restoring a call the reference never makes — every count check would applaud it,
> and "read the preserved comment and add the call beneath it" is the standard method that
> produces exactly that. Proved on the live tree: adding
> `coretext.zig:775`'s message to `crates/rustty-font-shaper/src/shaper/coretext.rs` turns
> the gate RED naming both sites.

- `CApi.zig:17` / `:40` `error allocating config err={}` — the reference's `state.alloc.create`
  failure arms. `Box::new` aborts on OOM rather than returning an error.
- `CApi.zig:22` `error creating config err={}` — `Config::default_config()` returns a `Config`,
  not a `Result`.
- `CApi.zig:45` `error cloning config err={}` — `clone_config()` likewise.

Each has the reference's full sequence preserved as a comment plus a PORT NOTE saying
explicitly not to "restore" it, and the portmap `reason` records the same. Allowance 11 -> 4.

**state**: the four are a standing, documented allowance. Nothing is outstanding.

---

## N24 — a WAIVED row whose "the port cannot produce this failure" rests on a *tracked defect*, not on a language property

**Recorded 2026-09-10 (task-14e-2 fix round 1), confirmed independently by that stage's review.**

`S94-F4` is `WAIVED` on two `src/font/shaper/coretext.zig` sites — `endFrame`'s
`toOwnedSlice` catch (`:277`) and its separate `wakeup.notify` catch (`:291`) — because the
port has no failure arm to hang them on. That is true, and it is **not the same kind of claim**
as the other four waivers of this shape recorded in the same stage:

| waiver | why the port cannot produce the failure | can it ever change? |
|---|---|---|
| `CApi.zig:17,22,40,45` | `Box::new` aborts on OOM; `default_config()`/`clone_config()` return a `Config`, not a `Result` | **no** — a language property |
| `PageCompression.zig:385` | `to_vec()` aborts on OOM | **no** — a language property |
| **`S94-F4`'s two** | the port replaced the reference's `CFReleaseThread` with an **mpsc stand-in**: `std::mem::take` allocates nothing and returns no `Result`, and an mpsc `send` IS the wakeup, so there is no separate notify step | **yes** — and `S94-F1` (CONFIRMED, OPEN) is the row that tracks undoing it |

So one of the port's own open defects is what makes the other row "not a defect". Restore the
`CFReleaseThread` and both failure arms return with it.

**Why this is not simply reopened as two OPEN rows.** The arms are not independently fixable —
they come back with `S94-F1`'s swap or not at all — so a second OPEN row would be a second
name for one piece of work, on a board that is already RED at 327. What was actually wrong was
that `WAIVED` read as final. It now reads as **contingent**, in the three tracked places
`PROCEDURE.md`'s carve-out standard requires: `S94-F4`'s own `owner` cell, the PORT NOTEs at
both sites in `coretext.rs`, and this entry — so the finale re-audit meets it as an item
rather than as a footnote inside a waived row.

**The generalisable rule.** A waiver's justification has a *kind*, and the kind decides whether
the waiver can expire. "Rust's allocator aborts" never expires. "The port does X instead" expires
the moment someone fixes X — and if nothing records the dependency, the waiver outlives the
reason for it. **When waiving on "the port cannot produce this failure", say WHY it cannot, and
if the why is another row, name that row in the `owner` cell.** This is the near neighbour of
the defect the 14e-1 review caught (a waiver contradicted by an open `missing-error-path` row);
here the ledger does not contradict the waiver, it *dates* it.

**state**: OPEN as a dependency. Discharged by whoever closes `S94-F1`, in the same change.
## N25 — five `rustty-font` diagnostics live only in test code, and were counted as if they were the port

- **id**: N25 (next free id checked 2026-09-11 against `NEW-FINDINGS.md` headings, a
  workspace grep for `\bN[0-9]+\b` and `progress.md`: N1-N24 taken.)
- **severity**: low-medium (five reference diagnostics are absent from every production build;
  no state or control-flow difference, and nothing regressed — this is debt that was
  previously miscounted as done)
- **kind**: unrestored sites / measurement correction
- **oracle**: `src/font/sprite/Face.zig` (4 live sites) and
  `src/font/glyf_rasterize_png_test.zig` (4 live sites).
- **rustty**: `crates/rustty-font/src/sprite/face.rs` and
  `crates/rustty-font/src/glyf_rasterize/glyf_rasterize_png_test.rs`.
- **description**: until 2026-09-11 `G-LOGFILE` counted emissions inside `#[cfg(test)]` /
  `#[test]` spans as if they were production code. When test spans were excluded
  (task 14gate round 6), these two files' counts fell:

  | port file | counted before | production only | test-only |
  |---|---|---|---|
  | `sprite/face.rs` | 5 | **1** | 4 |
  | `glyf_rasterize/glyf_rasterize_png_test.rs` | 1 | **0** | 1 |

  Their portmap allowances rose accordingly — `sprite/Face.zig` 0 -> 3,
  `glyf_rasterize_png_test.zig` 3 -> 4. **Nothing regressed and no diagnostic was removed.**
  Five of the reference's diagnostics were never emitted by a production build of
  `rustty-font`; the gate simply could not tell, because a test that writes a logging macro
  to prove one fires looks exactly like a site that fires.
- **why it is recorded here rather than left in the allowances**: a raised allowance with a
  generic reason is debt nobody owns and nobody meets. This entry is so the next stage
  working `rustty-font` meets it as a tracked item. The two rows' `reason` cells point at the
  cause; this entry is the item.
- **what closing it looks like**: move the four `sprite/Face.zig` sites and the one
  `glyf_rasterize_png_test.zig` site out of test scaffolding and onto the production paths
  that correspond to the reference's, then tighten both portmap allowances in the same commit
  — the gate is a ratchet and will go RED on the slack if they are not.
- **how found**: task 14gate round 6, as a side effect of implementing test-span exclusion;
  surfaced as a finding in round 8 at the coordinator's instruction, because it had been left
  visible only as a raised number.
- **state**: **OPEN.**

## N26 — the `checkGhosttyHEnum` tautology is four crates wide, not one file

- **id**: N26 (next free id checked 2026-09-10 against `NEW-FINDINGS.md` headings and a
  workspace grep for `\bN[0-9]+\b`: N1-N25 taken.)
- **severity**: medium (four more C-enum-sync tests that could not fail; same class as
  `S45-F1`, which cites only `src/apprt/action.zig`)
- **kind**: weakened-assertion / verification instrument
- **oracle**: `src/renderer.zig:52`, `src/apprt/ipc.zig:28,182`, `src/terminal/osc.zig:246`
  (each a `lib.checkGhosttyHEnum(T, "GHOSTTY_…_")` call).
- **rustty**: `crates/rustty-renderer/src/lib.rs` (`health_tests`),
  `crates/rustty-apprt-api/src/ipc.rs` (`tests`),
  `crates/rustty-vt/src/osc/mod.rs` (`tests`).
- **description**: `S45-F1` records that `run_henum` in `action_tests.rs` synthesized the "C
  header" side of `check_ghostty_henum` from the very descriptor under test, so all 17
  `ghostty_h_*` tests compared the enum to itself. The audit scoped the row to
  `src/apprt/action.zig`. Enumerating the port's **five** `check_ghostty_henum` call sites
  showed four of the five had the identical defect — the three files above plus
  `action_tests.rs` — **21 tests total**. Only `crates/rustty-vt/src/mouse.rs` read the real
  header, and it was the only site with a `*_probe_can_fail` self-test.
- **how found**: task 15, by grepping every caller of `check_ghostty_henum` rather than only
  the file the row cites. This is blind spot 3 (the anchors locate a region, they do not
  enumerate the work) applied across FILES rather than within one.
- **state**: **CLOSED** in the same commit as `S45-F1`. All five callers now parse the named
  enum block out of `include/rustty-embed.h` via the new shared
  `rustty_lib::c_header` module, and each of the four fixed files carries a
  `*_probe_can_fail` self-test that perturbs the header text three ways (dropped, renamed,
  revalued enumerator) and asserts the check goes red. The real header was also perturbed
  live once (`RUSTTY_ACTION_NEW_TAB = 9`) to prove the unmodified test fails.

## N27 — the vt enum-discriminant checker pairs header and Rust POSITIONALLY, so a header reorder is invisible

- **id**: N27
- **severity**: low-medium (a real gate, binding on values, blind to one drift class)
- **kind**: gate blind spot
- **rustty**: `.port/tools/check-vt-enum-discriminants.mjs:282-289`.
- **description**: the checker emits
  `_Static_assert((int)<header enumerator i> == <rust variant i's discriminant>)` by INDEX.
  It is genuinely independent — clang evaluates the header's own enumerators — and it does
  catch a value skew and a count skew. But because the pairing is positional and never
  compares the two NAMES, transposing two enumerators in the header is invisible: each
  assert is re-aimed at the enumerator that now occupies that slot, and both still hold.
  That is precisely the drift `checkGhosttyHEnum` catches, because the reference pairs by
  NAME (`<prefix><KEY>` uppercased).
- **why it is positional today**: the header's enumerator names and the Rust variant names
  differ in shape, so a name pairing needs the same `<prefix><UPPER_SNAKE>` derivation
  `check_ghostty_henum` uses. The mapping doc-line convention the checker already relies on
  (`C: Rustty…`) does not carry a prefix.
- **what closing it looks like**: derive the expected header enumerator name from the Rust
  variant name plus a per-enum prefix (declared beside the existing `PREFIX_OK` table), and
  FAIL when the positional partner's name does not match the derived one. Add a self-test arm
  that transposes two enumerators in a fixture header and asserts RED — without that arm the
  fix is unproven.
- **how found**: task 15, auditing every ABI gate for the `S45-F1` tautology at the
  controller's instruction. No other gate synthesised its expectations: `check_abi_schema.sh`
  validates the built dylib's emitted manifest against a hand-written JSON Schema and runs a
  negative control every invocation; `check_embed_h_abi.sh` clang-probes the real header;
  `check_rustty_h_abi.sh` diffs the product umbrella against the pinned oracle header;
  `check_rustty_vt_headers.sh` and `check-vt-struct-layouts.mjs` compile the real headers;
  `check_closed_enum_abi.sh` enforces a structural rule over Rust source and has no
  "expected" side to synthesise.
- **state**: **OPEN.**
