# Shared cross-review: GTK lifecycle/locale/diagnostic and PageList OOM

**Audit date:** 2026-07-31  
**Rustty revision:** `5243303fbb2f2cfd0832edfb8fa49d72ce322d81`  
**Ghostty revision:** `6ad1fe7d8cbda36c77b337a96c9bea8a77883699`  
**Method:** independent source/call-path review of selected claims in
`.port/codex/agents/gtk-bsd.md`, plus a separate review of the PageList
allocation-failure contract. Neither the prior report nor production source was
modified.

## Verdict summary

| Candidate | Verdict | Cross-review severity | Correction or qualification |
|---|---|---|---|
| GTK-BSD-002 application Quit confirmation | **Verified** | **P2**, high confidence | Downgrade from P1. The application-wide Quit action is a reachable dead end, but working window/tab/surface confirmation paths provide an alternate way to close the app; this is a material action/integration failure, not target unavailability or broad product unusability. |
| GTK-BSD-003 `quit_now` destruction | **Verified** | **P2**, high confidence | Retain P2. The omission is more than cosmetic lingering: subsequent teardown does not supply the missing window-finalization cascade and may reach core teardown with live surface resources. No interactive reproduction was available, so do not elevate it. |
| GTK-BSD-004 launch-time `LANG` | **Verified** | **P2**, high confidence | Retain P2. The captured value is stranded and the exact child environment removes `LANG`. An explicit user `env = LANG=...` entry can mask it, but there is no automatic fallback. |
| GTK-BSD-007 debug warning | **Verified** | **P4**, high confidence | Retain P4. The resource exists and two parent templates instantiate the type, but the subclass never binds or initializes that resource, so it is an empty diagnostic bin. |
| PageList fallible-allocation erasure | **Verified** | **P3**, high confidence | One root cause has two manifestations: preserved-page inspection panics instead of returning OOM, and the exact compressed-buffer allocation cannot be rejected opportunistically. Existing tests cover other rejection causes, not these allocation boundaries. |

The only correction to the selected GTK catalog is GTK-BSD-002's P1 → P2
severity. If adopted, the unchanged `gtk-bsd.md` executive count should no
longer count this item among its P1 findings.

## GTK-BSD-002 — verified dead end, but P2 is better calibrated than P1

Rustty has two independently reachable application-Quit entries, and both end at
the same incomplete method:

- `crates/rustty-apprt/src/gtk/class/application.rs:770-803` registers the
  `app.quit` GAction; its callback at `:1807-1814` calls `Application::quit`.
- The general apprt action switch sends `Action::Quit` to the same method at
  `:2989-2990`.
- `Application::bind_core_app` installs the live core
  `needsConfirmQuit()` probe at `:2523-2530`.
- When that probe is true, `Application::quit` at `:2832-2869` finds a possible
  parent, prints a stale Task-24 warning, presents no dialog, and leaves
  `running` true.

Ghostty completes this branch: `src/apprt/gtk/class/application.zig:601-644`
constructs `CloseConfirmationDialog.new(.app)`, connects `close-request` to the
final quit handler, and presents it. Rustty already has all the unused pieces:
`close_confirmation_dialog.rs:89-128` binds/initializes the template,
`:132-160` maps the close response to `close-request`, and `:174-188` constructs
and presents any `Target`, including `Target::App`. The headed dialog test even
constructs and presents `Target::App`
(`crates/rustty-apprt/tests/gtk_close_confirmation.rs:94-111`), but it never
connects that response to application quit.

### Alternate-path challenge

The finding survives the call-path challenge: no other app-level handler
presents the dialog, and both direct GAction and core/apprt Quit dispatch use the
same warning-only method. However, the P1 severity does not survive calibration.
Window close presents a functional confirmation dialog and destroys the window
(`class/window.rs:1749-1769`); tab close has working close/cancel callbacks
(`:1772-1809`); and surface close also presents and completes through the dialog
(`class/split_tree.rs:643-691`). A user can therefore close the app through its
windows/tabs/surfaces even though the application-wide Quit action cannot be
confirmed.

That is a material, common application-action mismatch and merits P2. It is not
equivalent to GTK-BSD-001's inability to launch a supported target, nor is the
whole application broadly unusable. P1 would be defensible only if all quit
routes were blocked or the dead end made the application generally
unrecoverable; the inspected alternatives disprove that stronger claim.

## GTK-BSD-003 — verified P2; later teardown does not mitigate it

Ghostty's `quitNow` enumerates toplevels, filters to its own window subclass, and
destroys each before clearing the run-loop flag
(`src/apprt/gtk/class/application.zig:646-672`). Rustty retains the enumeration
and the fcitx safety comment but makes the body `let _ = window`, then clears
`running` (`crates/rustty-apprt/src/gtk/class/application.rs:2871-2895`). This is
reachable from explicit application Quit whenever confirmation is not needed
(`:2844-2853`), including while ordinary terminal windows still exist.

The proposed mitigation—later teardown performs the same work—is false:

1. After the loop, `run_gtk` calls `app_runtime.terminate()` and then
   `core_app.destroy()` (`crates/rustty/src/main_rustty.rs:380-400`).
2. GTK `App::terminate` only calls `Application::deinit`
   (`crates/rustty-apprt/src/gtk/app.rs:130-136`).
3. `deinit_private` clears application config, protocol state, thunks, language,
   and CSS resources (`class/application.rs:1854-1904`); it does not enumerate or
   destroy windows.
4. Core `App::destroy` calls `App::deinit`, whose surface loop invokes each
   runtime surface's `deinit`, then expects the shared font-grid set to be empty
   (`crates/rustty-core/src/app.rs:235-266`). GTK's runtime-surface `deinit` is
   intentionally a no-op because actual teardown belongs to GObject finalization
   (`crates/rustty-apprt/src/gtk/surface.rs:173-176`).
5. The real GObject surface finalizer is where the core surface is removed,
   renderer/I/O teardown is joined, and the core surface is destroyed
   (`class/surface.rs:719-791`). That cascade is precisely what destroying the
   owning window is meant to trigger.

A normally initialized core surface obtains a shared font grid at
`crates/rustty-core/src/surface.rs:1795-1809` and releases it only during core
surface deinit at `:2775-2783`. Therefore the missing window destruction is also
a plausible route to the debug assertion that the grid set is empty during core
app teardown, and in non-asserting builds it short-circuits the intended
thread/resource finalization before process exit. This strengthens the contract
consequence beyond merely leaving a window visible for a moment.

It remains P2 rather than P1: process exit follows immediately, individually
closing windows takes the sound path, and this host could not run an interactive
Linux GTK session to demonstrate the assertion/race consequence. The verified
claim is the missing ordered shutdown integration; the stronger runtime failure
is source-supported risk, not an observed crash.

## GTK-BSD-004 — verified P2; only explicit user override masks it

The value flow has no hidden fallback:

- Rustty captures launch-time `LANG` before any UI-language override at
  `crates/rustty-apprt/src/gtk/class/application.rs:2293-2303` and stores its
  `CString` at `:2392-2402`.
- `Application::saved_language()` nevertheless returns literal `None` at
  `:2197-2208`. This is the only accessor and the stored value is otherwise only
  cleared during deinit (`:1883-1885`).
- `Surface::default_termio_env` begins from the inherited environment but removes
  `LANG` when that accessor returns `None`
  (`crates/rustty-apprt/src/gtk/class/surface.rs:2337-2361`).
- Core surface initialization passes that exact map into termio
  (`crates/rustty-core/src/surface.rs:2133-2156,2178-2214`). `Command` documents
  that a supplied map is exact and not merged with the process environment
  (`crates/rustty-termio/src/command.rs:211-216`), and the POSIX execution path
  serializes it as `envp` (`:609-622`).

Ghostty returns the stored language at
`src/apprt/gtk/class/application.zig:475-478` and writes it into the child map at
`src/apprt/gtk/class/surface.zig:1593-1606`. Rustty therefore drops launch-time
`LANG` even when `config.language` is unset; when it is set, Rustty correctly
keeps the UI override out of the child only by removing the variable entirely,
instead of restoring the user's original value.

One mitigation exists: termio applies the user's `config.env` after constructing
the default map (`crates/rustty-termio/src/exec.rs:2212-2221`), so an explicit
`env = LANG=...` can reintroduce it. A shell startup file may also set a locale.
Neither is an automatic parity path, and non-shell commands receive the exact
defective environment. The common child-process locale/configuration mismatch
supports P2.

## GTK-BSD-007 — verified P4; compiled resource is not a binding

Rustty's `DebugWarning` is a unit `AdwBin` subclass with no `CompositeTemplate`,
no `class_init`, and no `instance_init`
(`crates/rustty-apprt/src/gtk/class/debug_warning.rs:18-52`). In contrast,
Ghostty binds the version-selected resource in class initialization and calls
`initTemplate` per instance
(`src/apprt/gtk/class/debug_warning.zig:9-46`).

The apparent mitigation—the Rustty warning blueprint is present—is insufficient.
`crates/rustty-apprt/assets/ui/1.3/debug-warning.blp:4-8` defines the banner, and
the resource inventory registers its name, but a GTK subclass template is not
applied merely because it exists in the resource bundle. No other Rust code sets
a child on the bin. Both the main window
(`assets/ui/1.5/window.blp:150-155`) and inspector window
(`assets/ui/1.5/inspector-window.blp:22-30`) instantiate the empty type when
their `debug` property is true. Those getters use `cfg!(debug_assertions)`
(`class/window.rs:379-383`; `class/inspector_window.rs:164-168`), and the
workspace `release-safe` profile enables debug assertions (`Cargo.toml:327-331`).

This is a missing performance-warning diagnostic, not terminal behavior or a
build failure. P4 remains the right severity.

## PL-OOM-001 — PageList erased two recoverable allocation boundaries

**Verdict:** verified as one platform-neutral allocator-contract root with two
observable sites.  
**Severity/confidence:** P3 / high.

### A. Preserved-page inspection panics instead of returning OOM

Ghostty's `Node.pagePreservingState` is explicitly fallible: it accepts a caller
allocator, uses `try alloc.alignedAlloc`, and returns
`Allocator.Error!PreservedPage`
(`src/terminal/PageList.zig:147-190`). Its test named
`PageList preserved page keeps compressed storage` injects failure and proves
that OOM is returned while storage, statistics, and encoded bytes remain
unchanged (`:7723-7802`). Live Ghostty consumers preserve that recovery:
terminal search propagates the allocator error
(`src/terminal/search/sliding_window.zig:545-579`), while the inspector catches
it and displays a disabled placeholder (`src/inspector/widgets/pagelist.zig:103-114,705-714`).

Rustty removes the allocator/error result. For a compressed node,
`Node::page_preserving_state` calls raw `alloc_zeroed` and asserts the pointer is
non-null (`crates/rustty-vt/src/pagelist/mod.rs:504-548`). The corresponding test
explicitly says the failing-allocation arm is not ported because there is no
injectable seam (`crates/rustty-vt/src/pagelist/tests.rs:2995-3009`); its live
success assertions at `:3015-3091` prove storage preservation only when allocation
succeeds.

The test's stated reason is itself stale. Rustty already has an isolated test
binary with a counting/failing `#[global_allocator]`
(`crates/rustty-vt/tests/viewport_fingerprint_alloc.rs:1-81`), and that test uses
`Vec::try_reserve` to surface injected failure as OOM. The same process-isolated
technique can reach PageList; lack of a Zig-style per-object allocator makes the
production API infallible, but does **not** make the allocation behavior
untestable.

This is not test-only code. Rustty search calls it inside functions which still
claim `Result<..., OutOfMemory>`
(`crates/rustty-vt/src/search/sliding_window.rs:882-932`), so the preserved-page
allocation bypasses that error contract with a panic. Rustty inspector has two
live calls and comments that Ghostty's graceful placeholder is unreachable
(`crates/rustty-inspector/src/widgets/pagelist.rs:334-359,1734-1758`). No caller
catches a recoverable allocation error because none is returned.

### B. Opportunistic compression cannot reject its exact encoded allocation

Ghostty compression has two distinct allocations. Page-pool scratch is released
after compression, while `compression.Page.init` retains an exact-sized encoded
copy using `try alloc.dupe` and includes `Allocator.Error` in its result
(`src/terminal/compress/Page.zig:84-131`). `PageList.compressPage` catches that
specific OOM and returns false before changing the node
(`src/terminal/PageList.zig:4265-4341`). The incremental traversal records the
candidate's serial before attempting it (`:4178-4228`), so the next invocation
continues at the following page.

The Ghostty test `PageList incremental compression advances after allocation
failure` preheats the pool so the injected next failure hits the exact encoded
allocation, then proves the first page remains resident and the next invocation
compresses the second (`src/terminal/PageList.zig:7257-7283`).

Rustty's corresponding `CompressedPage::init` copies the encoded slice with
`to_vec()` but returns only codec errors
(`crates/rustty-vt/src/compress/page.rs:110-159`). `compress_page` can recover
from scratch allocation failure at
`crates/rustty-vt/src/pagelist/compress.rs:394-423`, but it has no result arm for
failure of that retained `Vec` allocation. Standard `Vec` allocation failure
goes to Rust's global allocation-error path rather than returning `false`, so
the opportunistic compression policy cannot continue to the next serial.

The missing test is acknowledged verbatim at
`crates/rustty-vt/src/pagelist/tests.rs:3984-4011`. The nearby passing tests do
not close the gap:

- `pagelist_incremental_compression_advances_after_failure` uses an
  incompressible input (`:3437-3458`), so `CompressedPage::init` returns `None`
  before retaining an encoded buffer.
- `pagelist_incremental_compression_advances_after_decommit_failure` injects the
  later strict-decommit failure (`:3461-3496`), after encoded allocation has
  succeeded.
- `CompressionScratch::init` itself uses fallible allocator APIs and maps their
  errors (`pagelist/compress.rs:33-55`), so its recovery does not cover the
  separate `to_vec` allocation.

### Severity and non-macOS scope

These are process-ending outcomes under genuine allocation exhaustion where
Ghostty either propagates OOM or skips opportunistic work. They remain P3 because
OOM is a narrow edge condition and no ordinary-input failure was demonstrated.
They are not merely missing tests: search and inspector contain live preserved-
page calls, and incremental compression is driven from the renderer seam.

For this non-macOS audit, the runtime consequence is principally 64-bit Linux.
Rustty enables retained-mapping strict compression only on 64-bit Linux and
Darwin (`crates/rustty-vt/src/mem.rs:86-133`); other targets normally have no
compressed node for these paths. The underlying allocator/API mismatch is shared
Rust code, but FreeBSD, Windows, and WASM currently return compression
unsupported before reaching the encoded-allocation site.

## Observed verification and limitations

The existing static GTK reproducer remained green:

```sh
bash .port/codex/scripts/gtk-bsd/check_runtime_gaps.sh
```

It reported all twelve predicates `PASS`, including the warning-only quit branch,
no-op toplevel loop, hardcoded `saved_language` result, and empty debug-warning
class. This host has no Linux display/native GTK session, so no interactive GTK
claim was promoted beyond the source-supported behavior.

Fresh targeted PageList tests also passed:

```sh
cargo test -p rustty-vt \
  pagelist::tests::pagelist_preserved_page_keeps_compressed_storage -- --exact

cargo test -p rustty-vt \
  pagelist::tests::pagelist_incremental_compression_advances_after -- --nocapture
```

The first ran 1/1; the second ran 2/2 (incompressible and decommit failures).
Those green results are counterevidence against broad state-machine breakage,
but not against PL-OOM-001: source and test comments confirm the allocation
failure arms are absent rather than passing.

I also reproduced both missing allocation boundaries with an isolated probe
under `.port/codex/evidence/shared-cross-review/pagelist-oom-probe/`. It follows
the repository's existing one-test-binary failing-global-allocator pattern and
does not modify Rustty:

```sh
cargo run --offline --manifest-path \
  .port/codex/evidence/shared-cross-review/pagelist-oom-probe/Cargo.toml
```

Observed result: the preserved-page allocation panicked at
`pagelist/mod.rs:535` and the probe verified the node remained compressed. A
child process then injected failure at the exact retained encoded-buffer
allocation; it printed `memory allocation of 4626 bytes failed` and terminated
with `SIGABRT`. The parent probe observed that failure and exited 0. This
directly falsifies the comments' “cannot occur/cannot inject” rationale while
confirming the product behavior asserted by PL-OOM-001.
