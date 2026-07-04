export const meta = {
  name: 'apprt-embedded-design',
  description: 'Design blueprint for the apprt/embedded milestone (libghostty C-ABI: embedded.zig App/Surface backend + 71 export-fn CAPI + none.zig) + adversarial verification against the reference, ghostty.h, and the Cargo graph.',
  phases: [ { title: 'Design' }, { title: 'Verify' } ],
}

const CONTEXT = `Project: rustty — faithful Zig->Rust port of ghostty. Reference READ-ONLY at /Users/gael/projects/ghostty (Zig 0.15.2); port at /Users/gael/projects/rustty. Rules: /Users/gael/projects/rustty/.port/PORT_RULES.md (R1 test-count contract; R3 byte-faithful every line+comment, byte-slices -> &[u8]/&str never lossy String; R5 never weaken). Binding gates: check_acyclic.sh (Tarjan over crates/*/Cargo.toml), check_test_counts.py, scoped-G3b comment coverage.

MILESTONE STATE (all committed, gate-green): the ENTIRE terminal + integration tier is ported — rustty-vt (2019t crown jewel), vt-c (308t C-ABI, the PROVEN cbindgen precedent), config (189t), font stack, input, os, renderer (19t headless: cell/cursor/state/shaders/message/Thread::Mailbox; GPU R-d = Phase-3 placeholder leaves in rustty-core::renderer_seam), crash (12t), termio (44t incl Command + full stream_handler), **rustty-apprt-api (20t — the GENERIC App/Surface/CoreSurfaceHandle/Target trait seam; core just ADDED the growth-point methods: App::{config,keyboard_layout}, Surface::{type Core, core, rt_app, close, get_content_scale, get_size, get_title, supports_clipboard, clipboard_request->Result<bool,_>, set_clipboard, get_cursor_pos, default_termio_env})**, and NOW **rustty-core COMPLETE** (Surface.zig 6036 + App.zig 604 + surface_mouse — the generic App<A>/Surface<A>, 15t). rustty-apprt is a STUB whose Cargo.toml already deps core/apprt-api/vt/font/renderer/inspector/config/input/os. rustty-xev = D6 event-loop placeholder crate. Inspector = Phase-3 stub (Event only).

DOCTRINE (read the memory-equivalent in /Users/gael/projects/rustty/.port/phase-2/CORE_BLUEPRINT.md and its "## C0 DECISION"): big GUI files port LIVE via INHABITED placeholder structs with unimplemented!() methods confining panics to GPU/xev/inspector LEAVES; the generic backend param A stops at the mailbox seam; type Core self-referential bound COMPILES. The C-ABI precedent is rustty-vt-c: cbindgen + hand-authored #[repr(C)] with PINNED [u64;N] padding + const size-asserts + a C/C++ compile smoke test (vt-c "D2" pattern) — study crates/rustty-vt-c.

REFERENCE SCOPE (grepped): src/apprt/embedded.zig = 2250 loc, 0 tests — App struct (line 29), Surface struct (line 410), CAPI struct (line 1244) with 71 \`export fn ghostty_*\` (the libghostty C API, source of include/ghostty.h). src/apprt/none.zig = 19 loc (IPC-only headless backend). src/apprt/browser.zig = 4 loc.`

phase('Design')
const blueprint = await agent(`${CONTEXT}

You are the ARCHITECT. Produce a CONCISE, decision-oriented implementation blueprint (write it to /Users/gael/projects/rustty/.port/phase-2/EMBEDDED_BLUEPRINT.md, do NOT commit; also return it as final text) for the apprt/embedded milestone — the libghostty C-ABI. Ground EVERY claim by READING the actual reference (embedded.zig App/Surface/CAPI, none.zig, the apprt-api seam.rs/surface.rs, rustty-vt-c's build.rs/cbindgen setup, rustty-core's surface.rs to see what trait methods core actually calls). Cover:

1. **Achievable-now vs Phase-3 split (the framing decision).** embedded.zig is the libghostty embedding — a HOST app drives it via C callbacks + the real GPU renderer (Phase-3 R-d) + xev event loop (D6). Enumerate which of embedded's App/Surface methods + 71 CAPI exports are portable NOW (the C-ABI surface, config, clipboard/cursor/size/title delegating to host C function-pointer callbacks, action dispatch) vs which hit GPU/xev placeholder leaves. Give the honest fraction. RECOMMEND whether this milestone lands now as "the C-ABI contract + non-GPU backend (verified by const-asserts + cbindgen header + C smoke compile, rendering deferred)" — and whether cli/bin should interleave/precede it for a tangible headless deliverable. Be decisive.

2. **The concrete backend: embedded App/Surface implementing apprt-api's traits.** How does the embedded backend satisfy A: apprt_api::App / A::Surface? It holds host C callbacks (function pointers) for setClipboard/getCursorPos/etc. Give the struct shapes + how each apprt-api growth-point method (the list core added) maps to a stored C callback or a real body. This is the FIRST concrete backend — it proves the trait seam. Where does A get bound (the embedded App IS the concrete A; core is Surface<EmbeddedApp>)?

3. **The C-ABI (the deliverable).** Where does the 71-export CAPI live — rustty-apprt directly (#[no_mangle] extern "C"), or a separate rustty-apprt-c crate mirroring rustty-vt-c? Recommend, with the vt-c precedent. For the ghostty.h struct types crossing the ABI (ghostty_config_t, ghostty_surface_config_s, the callback function-pointer typedefs, etc.): cbindgen strategy + which need PINNED [u64;N] padding + const size-asserts (vt-c D2). How is ghostty.h generated/verified (cbindgen + a C compile smoke test)? Enumerate the 71 exports grouped by subsystem (app lifecycle, surface lifecycle, input, config, clipboard, ...).

4. **none.zig + browser.zig.** none is IPC-only (19 loc). Does it implement the full apprt-api App/Surface traits or a minimal subset? Faithful placement + whether it's the cli/headless backend. browser is a 4-loc stub — defer or trivial.

5. **Acyclicity + Cargo deps.** rustty-apprt already deps core/apprt-api/etc. Confirm the C-ABI crate (if separate) placement is acyclic. Any new deps (objc2/coretext for macOS embedded? — check what embedded.zig actually imports; much macOS window glue may be Phase-3).

6. **Test/verification accounting.** embedded.zig has 0 unit tests → the gate is C-ABI const-size-asserts + cbindgen header generation + a C/C++ compile-and-link smoke test (KAT calling a few ghostty_* fns) + G3b comment coverage + the check_test_counts MAP entries (0-test files). Spell out the verification anchors precisely (mirror vt-c's approach).

7. **Chunked execution plan** (skeleton->fill; the file is 2250 loc): chunk list with embedded.zig line/section ranges, per-chunk G3b, order, the C-ABI smoke KAT as its own chunk.

8. **Risks/ambiguities** + resolutions (esp. the macOS objc/window glue fraction, the callback-pointer ABI, the GPU/xev boundary honesty).

Keep it tight and decision-oriented.`, { label: 'architect', phase: 'Design' })

phase('Verify')
const verdict = await agent(`${CONTEXT}

You are the ADVERSARIAL REVIEWER. The blueprint is at /Users/gael/projects/rustty/.port/phase-2/EMBEDDED_BLUEPRINT.md (uncommitted — read it). Try to BREAK it against the actual reference + repo; verify independently, trust nothing:
1. **C-ABI completeness**: grep embedded.zig yourself for EVERY \`export fn\` — count them, list any the blueprint's export inventory missed or mis-grouped. Check the ghostty.h struct/callback types crossing the ABI are all accounted for with a layout strategy.
2. **Cycle traps**: does the proposed C-ABI crate placement / any new dep create a cycle in the real Cargo graph? Does the concrete-backend binding (core = Surface<EmbeddedApp>, EmbeddedApp in rustty-apprt which deps core) typecheck without an apprt-api->apprt or core->apprt edge?
3. **Trait-impl completeness**: does the embedded backend as designed implement EVERY apprt-api App/Surface trait method core calls? Grep core's surface.rs/app.rs for the rt_app./rt_surface. calls and diff against the blueprint's backend method map. Any missing method = core won't link against this backend.
4. **Achievable-now honesty**: are the "portable now" methods really free of GPU/xev, or does some (e.g. surface init, draw, size) transitively pull the renderer/event-loop? Sanity-check the fraction. Is the cli/bin sequencing recommendation sound?
5. **vt-c pattern fidelity**: does the proposed cbindgen + const-assert + C-smoke approach actually match what rustty-vt-c does (read its build.rs)? Any ABI-layout landmine (the callback function-pointer typedefs, tagged unions, enum repr)?
6. **Anything else that sinks it** (macOS objc glue mis-scoped, a 0-test file that actually has tests, browser.zig, IPC).
Return numbered findings (CONFIRMED-ISSUE with file:line evidence / FALSE-ALARM / RECOMMENDATION) + an overall verdict: ACCEPT / ACCEPT-WITH-FIXES (list fixes) / REDESIGN. Do NOT edit the blueprint.`, { label: 'adversary', phase: 'Verify' })

return { blueprint_len: (blueprint || '').length, verdict }
