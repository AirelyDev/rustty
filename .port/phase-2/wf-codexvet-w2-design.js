export const meta = {
  name: 'codexvet-w2-design',
  description: 'Design docs for wave-2 platform remediations (Windows OS layer, FreeBSD enablement, Linux GL image rendering)',
  phases: [{ title: 'Design' }, { title: 'Adversary' }],
}

const RUSTTY = '/Users/gael/projects/rustty'
const GHOSTTY = '/Users/gael/projects/ghostty'

const COMMON = `You are a design agent for Rustty — a full-fidelity Rust port of Ghostty (Zig). RUSTTY=${RUSTTY} (read code freely; write ONLY your one design doc under ${RUSTTY}/.port/codex-vet/designs/), read-only oracle GHOSTTY=${GHOSTTY} (pin 6ad1fe7d, NEVER modify).
Context: an independent audit confirmed these platform parity gaps (full evidence in ${RUSTTY}/.port/codex-vet/STATE.json — read the .vet object of each finding id named below). They were previously deferred by the macOS-first plan; the user has now directed they be fixed. A fix workflow (wave w1) is CURRENTLY editing other files in this repo — treat the tree as read-only except your design doc, and expect minor churn in files you read.
Follow the repo's design doctrine (.port/PORT_RULES.md, existing *_DESIGN.md files in .port/phase-2/ as style oracles). The doc must be directly executable by fix agents: exact files, exact oracle cites (file:line at the pin), porting order, cfg strategy, and a concrete verification plan per step.
Verification environments available: macOS host (primary; cargo test), \`cargo check --target x86_64-pc-windows-msvc\` and \`--target x86_64-unknown-freebsd\` (rust-std installed, check-only, no system libs), and a Fedora 44 aarch64 Linux VM reachable as \`ssh rvm\` with GTK 4.22.4 + cargo (sync via rsync of the working tree to rvm:~/rustty-vmwork; established gate pattern in .port/tools/gates/ws19_portal_openuri_gate.sh).`

const DESIGNS = [
  {
    key: 'windows-os',
    findings: 'F-001',
    out: 'W2D_WINDOWS_OS.md',
    brief: `Design the Windows compile restoration for the shared OS layer (finding F-001): make \`cargo check -p rustty-os -p rustty-vt -p rustty-vt-c --target x86_64-pc-windows-msvc\` pass, restoring the ConPTY path to type-checked parity with the oracle.
Scope per the vet evidence: (1) rustty-os Cargo.toml gates libc to unix but windows-visible modules use it (path.rs, locale.rs, +others — enumerate ALL); (2) eager compile_error! sites: page_allocator.rs:358-366 windows VM-mapping arm (port Zig std.heap.page_allocator's VirtualAlloc/VirtualFree/MEM_RESERVE|COMMIT semantics — oracle: ghostty PageList std.heap.page_allocator on non-darwin + Zig std source for the Windows PageAllocator), passwd.rs:182-188 (mirror Ghostty's lazy-compile semantics: the windows arm must compile but error only when called — Zig @compileError is lazy, Rust compile_error! is eager; design the faithful Rust equivalent, e.g. cfg'd runtime unreachable/unsupported error per the port's established doctrine for such cases); (3) WindowsPty (pty.rs cfg(windows)): missing std::error::Error impl, E0308s — reconcile against oracle src/pty.zig ConPTY arm; (4) kernel32 shims needed (which APIs, via windows-sys crate or hand-rolled extern "system" — check what the repo already does in rustty-os/src/windows.rs and prefer its existing pattern; adding a windows-sys dependency needs justification against the repo's dependency doctrine).
Deliverable: numbered work items each with files, oracle cites, the exact cfg/dep-table changes, and the check command that proves it. End state gate: the three cargo check commands above green on macOS host, plus a proving unit-test plan for logic that can run on macOS (e.g. path parsing behind cfg(test) shims where the repo already has that pattern).`,
  },
  {
    key: 'freebsd',
    findings: 'F-002, F-003, F-004',
    out: 'W2D_FREEBSD.md',
    brief: `Design FreeBSD enablement to type-check parity (findings F-002 GTK runtime selection, F-003 OpenGL renderer gating, F-004 xev kqueue backend). Read each finding's .vet evidence first.
Scope: (1) F-004 kqueue: rustty-xev's kqueue backend is macOS-only; oracle is the pinned libxev FreeBSD arms (find them in the ghostty tree's vendored libxev or the pinned package the port used — see crates/rustty-xev provenance docs) — design the FreeBSD kevent ABI arms (struct kevent layout differs: ident/filter/flags/fflags/data/udata widths), timer semantics, and process-watcher (EVFILT_PROC) differences; (2) F-003 renderer: widen the OpenGL module + Renderer export + GL dep tables from cfg(target_os="linux") to cfg(any(linux, freebsd)) — enumerate every gate site from the vet evidence and check the GL loader crates' FreeBSD support; (3) F-002 apprt: widen the GTK module/dep-table gates to include freebsd (gtk4-rs crates support FreeBSD via pkg-config) and the main_rustty.rs launch arm.
Verification reality: \`cargo check --target x86_64-unknown-freebsd\` works for pure-Rust crates (rustty-os, rustty-xev, rustty-vt) but -sys crates needing pkg-config (gtk4) will fail at build-script stage on macOS — design the check matrix accordingly: full freebsd check for os/xev/vt/renderer-minus-gtk; for apprt, cfg-widening correctness argued by construction + a documented follow-up on a real FreeBSD env. State explicitly which parts remain unverifiable and how the design de-risks them (e.g. mirror the exact linux cfg patterns).
Deliverable: numbered work items with files, oracle cites, exact cfg changes, dep-table additions, and per-item check commands.`,
  },
  {
    key: 'linux-gl-images',
    findings: 'F-005, F-012, F-020',
    out: 'W2D_LINUX_GL_IMAGES.md',
    brief: `Design the Linux OpenGL image-rendering completion (findings F-005 Kitty graphics never rendered, F-012 background-image not rendered, F-020 inspector renderer-overlay controls have no OpenGL consumer). Read each finding's .vet evidence first.
The macOS Metal twin in crates/rustty-renderer/src/generic.rs is the ported, test-green reference implementation of the SAME oracle (ghostty src/renderer/generic.zig at the pin) — the design is fundamentally: port the macOS twin's image/kitty/background-image/overlay arms into generic/linux_impl.rs + the opengl module, exactly as the w1 custom-shaders fix (F-013) did for its slice; read the fresh F-013 diff (git diff crates/rustty-renderer once w1 lands it — if not landed yet, read its fix plan in STATE.json) to match its structure and avoid conflicts.
Scope: (1) F-005: Kitty image upload (texture creation/update from the decoded image storage), placement draw pass, unload/eviction — enumerate the generic.zig image pipeline (imageInit/imageUpload/drawImagePlacements at the pin) and the existing macOS twin functions; design their GL equivalents (texture formats incl. gray/gray-alpha/rgb/rgba expansion, GL_UNPACK_ALIGNMENT, premultiplication) using the opengl module's existing abstractions; (2) F-012: background-image pass (bgimage pipeline: load via rustty-wuffs, texture, draw with fit/fill/stretch/tile modes + opacity) mirroring the macOS twin; (3) F-020: wire the inspector's renderer-overlay controls to the GL renderer the same way the macOS twin consumes them (identify the overlay seam in the inspector + macOS consumer sites).
Verification: shared-logic unit tests runnable on macOS where possible (e.g. placement math, texture-size/format selection); full pipeline on the Linux VM (ssh rvm, rsync working tree to ~/rustty-vmwork, cargo test -p rustty-renderer --features ... plus any existing live-GL gates — see .port/tools/gates/ and the W8-03 skip-accounting work landing in w1). Design the test plan explicitly, including which assertions are live-GL-gated.
Deliverable: numbered work items with files, oracle cites (generic.zig + macOS twin line refs), porting order (upload -> draw -> eviction -> bgimage -> overlay), and per-item verification commands.`,
  },
]

const A = typeof args === 'string' ? JSON.parse(args) : args // unused; keep pattern

const results = await pipeline(
  DESIGNS,
  (d) =>
    agent(
      `${COMMON}

ASSIGNMENT (findings ${d.findings}): ${d.brief}

Write the design doc to ${RUSTTY}/.port/codex-vet/designs/${d.out} (overwrite if present). Return a <=600-char summary of the design's key decisions and any open risks.`,
      { label: `design:${d.key}`, phase: 'Design' }
    ),
  (summary, d) =>
    agent(
      `${COMMON}

You are the ADVERSARY reviewing the design doc ${RUSTTY}/.port/codex-vet/designs/${d.out} (findings ${d.findings}). Attack it: (1) fidelity — does every work item cite the oracle at the pin and match its actual behavior (read the oracle yourself at the cited lines)? (2) completeness — does it cover the full scope of the findings' vet evidence, or does it silently drop arms? (3) executability — could a fix agent implement each item without re-deriving the design? (4) verification honesty — are the "verified" claims actually provable with the stated commands, and are unverifiable residues stated plainly? (5) conflicts — does it collide with wave-1 fixes (check STATE.json fix plans for overlapping files)?
EDIT the design doc in place to fix every defect you find (you own the doc now; keep its structure). Return a <=500-char summary: defects found and fixed, plus any residual risk a human should know.
Design author's summary for context: ${String(summary || '').slice(0, 600)}`,
      { label: `adversary:${d.key}`, phase: 'Adversary' }
    )
)

return { designs: DESIGNS.map((d, i) => ({ key: d.key, out: d.out, review: results[i] })) }
