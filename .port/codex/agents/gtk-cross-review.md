# Cross-review: platform targets, GTK overlaps, and terminal allocator

Audit universe:

- Rustty `5243303fbb2f2cfd0832edfb8fa49d72ce322d81`
- Ghostty `6ad1fe7d8cbda36c77b337a96c9bea8a77883699`
- Cross-reviewed report: `.port/codex/agents/platform-targets.md`
- Overlap report: `.port/codex/agents/gtk-bsd.md`

## Verdicts

| Candidate | Verdict | Severity recommendation | Duplicate disposition |
| --- | --- | --- | --- |
| PT-003, private/fixed WASM callback table | Uphold and broaden impact | P2, high confidence | Unique |
| PT-004, default Linux no-op winproto | Uphold with narrow impact wording | P2, high confidence | Exact duplicate of GTK-BSD-005 |
| PT-005, orphaned Linux resources | Uphold the repository-wiring gap | P4 under the audit rubric; high confidence in repository absence | Exact duplicate of GTK-BSD-006 |
| Terminal custom allocator ignored | Promote as a new finding | P2, high confidence | Unique |

PT-002 was not a requested focus, but comparison found it is also an exact
duplicate of GTK-BSD-001. The final catalog should keep one canonical entry for
each of PT-002/GTK-BSD-001, PT-004/GTK-BSD-005, and PT-005/GTK-BSD-006.

## PT-003 — uphold; impact is broader than stated

### Paired contract and artifact result

Ghostty deliberately sets `export_table = true` so browser embedders can add
callbacks for terminal effects, then removes the table maximum before installing
the WASM artifact
(`/Users/gael/projects/ghostty/src/build/GhosttyLibVt.zig:51-85`). Rustty's
advertised builder is only a direct Cargo release build
(`/Users/gael/projects/rustty/.port/tools/build-vt-wasm.sh:1-15`). Its standalone
xtask changes table limits but cannot add an export
(`xtask/src/wasm_patch_growable_table.rs:1-11,35-49`;
`xtask/src/main.rs:503-528`).

Fresh inspection reproduced all relevant states:

```text
Rustty direct:  exports=[], imports=[], table min=212 max=212
Rustty patched: exports=[], imports=[], table min=212 max=unbounded
Ghostty:        exports=[__indirect_function_table], table min=127 max=unbounded
```

The Ghostty artifact was inspected as an adversarial positive control with the
same analyzer. The Rustty patcher succeeds at its documented purpose, but the
result remains inaccessible to JavaScript. A private, unimported WebAssembly
table cannot be obtained through the JS API, and linear-memory access cannot
write table entries.

### Impact correction

PT-003 mentions only `rustty_sys_set`'s log and PNG-decoder callbacks
(`/Users/gael/projects/rustty/crates/rustty-vt-c/src/lib.rs:382-400`;
`crates/rustty-vt-c/src/sys.rs:223-279`). Those claims are correct: the
standalone vt-c artifact intentionally has no built-in Wuffs decoder
(`crates/rustty-vt/src/sys.rs:47-76`; `crates/rustty-vt/Cargo.toml:48-58`), so
Kitty PNG input is unsupported until the callback is installed
(`crates/rustty-vt/src/kitty/graphics_image.rs:223-229,767-782`).

However, the table is also required for the per-terminal effect callbacks that
Ghostty's build comment explicitly names. Rustty's public header documents
write-PTY, bell, title, PWD, enquiry, XTVERSION, size, color-scheme, device-
attributes, clipboard, notification, and progress callbacks
(`/Users/gael/projects/rustty/include/rustty/vt/terminal.h:54-113`). The setter
accepts them as raw function pointers and transmutes those pointers into callable
Rust functions (`crates/rustty-vt-c/src/terminal.rs:1282-1359`). The emitted
module exports `rustty_terminal_set` and `rustty_sys_set`, but no table.

Therefore a normal JS embedder cannot install *any* parity callback, including
the write-PTY callback needed for terminal query responses, not merely the PNG
and log callbacks described in PT-003. This strengthens the functional impact,
but P2 remains appropriate: state parsing/rendering and the additive WASM
allocation utilities remain usable, so the entire WASM library is not proven
unusable.

### Falsification attempts

- **Host imports:** Rustty's artifact has an empty import list, so callbacks are
  not supplied as imported WebAssembly functions.
- **Manual patch:** the repository patcher produced an unbounded table, but it
  remained unexported.
- **Built-in PNG path:** vt-c leaves Rustty VT's `wuffs` feature off by design;
  there is no default decoder to mitigate this particular callback.
- **Alternate export bridge:** repository searches found no exported callback
  registration bridge and no linker `--export-table`/equivalent configuration.

None falsified the finding. Keep PT-003, expand its impact to terminal effects,
and avoid saying the entire Rustty browser/WASM stack is broken.

## PT-004 — exact duplicate of GTK-BSD-005

Both findings have the same root cause, paired oracle, execution path, target,
and consequence:

- Ghostty detects GTK's compiled X11/Wayland targets and uses them as defaults
  (`/Users/gael/projects/ghostty/src/build/gtk.zig:8-26`;
  `src/build/Config.zig:208-218`).
- Rustty defaults to only `gtk`; `wayland` and `x11` are opt-in
  (`/Users/gael/projects/rustty/crates/rustty/Cargo.toml:21-44`).
- Without those features, the winproto probe reaches `App::None`
  (`crates/rustty-apprt/src/gtk/winproto.rs:113-145`), whose quick-terminal,
  native shortcut, urgency, appearance, and subprocess-environment operations
  are unsupported or no-ops (`gtk/winproto/noop.rs:63-102,137-162`).

Fresh inverse feature trees showed only `rustty-apprt/gtk` for the default Linux
build. Re-running with `--features wayland,x11` showed both native features,
which is a useful control: the implementations are present and the finding is
strictly about the product default/build orchestration.

P2/high confidence is supported. The final wording should lead with the most
definite common consequence: Rustty's default Wayland build reports no quick-
terminal support and never selects either native integration layer. It should
not imply that basic GDK windowing fails, or that all global shortcuts fail:
the no-op native binder deliberately permits an XDG portal fallback. PT-004 and
GTK-BSD-005 should be merged, not cataloged separately.

## PT-005 — exact duplicate of GTK-BSD-006; severity is the weak point

The factual root cause is verified:

- Ghostty's normal Linux resource build processes and installs the desktop,
  DBus, systemd, and AppStream templates
  (`/Users/gael/projects/ghostty/src/build/GhosttyResources.zig:246-369`) and
  its executable install depends on that resource install
  (`/Users/gael/projects/ghostty/build.zig:177-183`).
- Rustty's xtask command list and complete `resources()` body contain no Linux
  resource installer (`/Users/gael/projects/rustty/xtask/src/main.rs:57-82,469-500`).
  The project guide calls `dist/linux` packaging *source* assets and identifies
  the Flatpak/Snap/Nix workflows as forward references
  (`/Users/gael/projects/rustty/HACKING.md:82-106`).
- A fresh search across Rust, TOML, shell, workflow, and Nix production files
  for the template names and XDG install destinations returned no matches.

An external distribution can process the checked-in templates, so the report is
right to limit the claim to repository-supplied build/install wiring. It should
not state unconditionally that every end-user package lacks the resources.
Confidence in the repository-level absence can be raised from GTK-BSD-006's
medium-high to high; the explicit forward-reference documentation reinforces
the exhaustive negative search.

Severity is less secure. The audit rubric explicitly places verified
"packaging" defects in P4, while P2 covers material integration mismatches. The
wholesale desktop/DBus/systemd absence is more consequential than a metadata
typo, but no executable runtime failure was demonstrated and downstream
packagers can compensate using the existing templates. Under a strict reading
of the supplied rubric, I recommend P4. If synthesis has an established rule
that missing repository-wide desktop activation counts as material runtime
integration, P2 is defensible, but that exception should be stated. Regardless
of severity, PT-005 and GTK-BSD-006 are exact duplicates and must merge.

## New verified finding — `rustty_terminal_new` ignores its allocator

**Recommended severity:** P2  
**Confidence:** High  
**Scope:** public librustty-vt C ABI on every target

### Contract and divergence

Both public headers say the library uses explicit allocation through the passed
allocator and document how to supply one
(`/Users/gael/projects/rustty/include/rustty/vt/allocator.h:15-46`;
`/Users/gael/projects/ghostty/include/ghostty/vt/allocator.h:15-46`). Both
terminal constructors expose that allocator parameter
(`/Users/gael/projects/rustty/include/rustty/vt/terminal.h:1357-1375`;
`/Users/gael/projects/ghostty/include/ghostty/vt/terminal.h:1382-1400`).

Ghostty resolves the supplied allocator, uses it for the terminal, wrapper, IO
state, terminal internals, and parser stream
(`/Users/gael/projects/ghostty/src/terminal/c/terminal.zig:398-463`), then frees
those allocations through the same terminal allocator
(`src/terminal/c/terminal.zig:1103-1118`).

Rustty resolves the non-null C allocator into `_alloc` and immediately strands
it. Terminal internals use `allocator_api2::alloc::Global`, terminal and wrapper
storage use `Box`, and the parser stream uses a process-static global allocator
(`/Users/gael/projects/rustty/crates/rustty-vt-c/src/terminal.rs:953-1067`). Free
correctly mirrors those Rust-owned allocation choices with `Box::from_raw`
(`crates/rustty-vt-c/src/terminal.rs:2746-2808`), but that internal consistency
does not honor the caller's allocator contract.

### Paired C reproduction

The supplied `.port/codex/scripts/check-terminal-allocator.c` installs a valid
allocator whose allocation callback counts calls and always fails. Against
fresh/current Rustty and Ghostty native libraries it observed:

```text
Rustty:  result=0  terminal_nonnull=1 alloc_calls=0 free_calls=0
Ghostty: result=-1 terminal_nonnull=0 alloc_calls=1 free_calls=0
```

The public result enum identifies `0` as success and `-1` as out-of-memory
(`/Users/gael/projects/rustty/include/rustty/vt/types.h:70-86`). Thus Rustty not
only fails to use the allocator; it reports success when the reference and the
declared allocation-failure contract report OOM.

### Impact

C embedders cannot use the documented terminal allocator for arenas, quotas,
allocation accounting, deterministic failure injection, or alternative heaps.
They also cannot rely on allocator rejection causing
`RUSTTY_OUT_OF_MEMORY`. This is a functional C-ABI contract gap even though the
function signatures and allocator struct layouts match.

### Counter-check and severity

The strongest mitigating interpretation is that Rustty intentionally chose
Rust-owned storage and frees it consistently. The source explicitly documents
that choice, and the repro did not show a wrong-heap free, memory corruption, or
leak. That prevents P0/P1. It does not falsify the public contract: the allocator
parameter is documented, valid, accepted, and never called, while the oracle
uses it for the entire terminal lifecycle. P2 is appropriate for the material
memory-management and OOM-semantic mismatch.

## Synthesis actions

1. Keep PT-003 as the canonical WASM-table finding, broaden the impact from two
   sys callbacks to all terminal-effect callbacks, and retain P2/high.
2. Merge PT-004 with GTK-BSD-005 at P2/high; retain the portal/GDK caveats.
3. Merge PT-005 with GTK-BSD-006; use high confidence and decide P4 versus P2
   explicitly against the catalog's packaging-severity policy (P4 recommended
   under the literal rubric).
4. Merge PT-002 with GTK-BSD-001 at P1/high.
5. Add the ignored terminal allocator as a new, unique P2/high C-ABI finding.
