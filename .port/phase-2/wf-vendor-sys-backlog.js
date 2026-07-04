export const meta = {
  name: 'vendor-sys-backlog',
  description: 'Sequentially vendor the 3 tractable remaining -sys (libpng, sentry, dcimgui) via the proven fetch->vendor->cc->bindgen->KAT template, per-crate commit; fontconfig+libxml2 deferred',
  phases: [
    { title: 'libpng-sys' },
    { title: 'sentry-sys' },
    { title: 'dcimgui-sys' },
  ],
}

const RESULT_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['crate', 'ok', 'kat_green', 'commit', 'blockers', 'notes'],
  properties: {
    crate: { type: 'string' }, ok: { type: 'boolean' }, kat_green: { type: 'boolean' },
    commit: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' },
  },
}

const COMMON = `OBEY the rustty PARITY CONSTITUTION + /Users/gael/projects/rustty/.port/PORT_RULES.md. R3 = vendor the SAME upstream C/C++ ghostty uses (no substitutes). Reference READ-ONLY at /Users/gael/projects/ghostty.

Vendor ONE -sys crate via the PROVEN template (exemplars to STUDY first: crates-sys/zlib-sys plain C; crates-sys/freetype-sys C + bundled deps + DEP_Z_INCLUDE include-chaining + rich bindgen allowlist; crates-sys/harfbuzz-sys C++ + DEP_FREETYPE_INCLUDE; crates-sys/simdutf-sys C++ amalgamation; crates-sys/oniguruma-sys GENERATED config.h from a template; the just-landed crates-sys/{wuffs,glslang,spirv-cross}-sys). TEMPLATE:
1. Read pkg/<PKG>/build.zig (exact source list, include dirs, C vs C++, -D defines, generated/provided headers, sub-deps) + the *.zig @cInclude set + build.zig.zon.
2. curl the EXACT tarball (URL below) into a scratch dir, extract, vendor ONLY the compiled source subtree + LICENSE into crates-sys/<CRATE>/vendor/ (LICENSE at vendor/LICENSE). Do not vendor tests/docs/unused backends.
3. build.rs: cc::Build mirroring the reference buildLib EXACTLY (same sources/includes/defines/C++ std; cpp(true) for C++; warnings off; MSVC ubsan opt-outs; -fPIC on freebsd/musl). If the reference generates OR ghostty PROVIDES a config header (e.g. pnglibconf.h), place it correctly (generated → OUT_DIR first on include path per oniguruma-sys; ghostty-provided → vendor it). Consume sibling -sys includes via env DEP_<DEP>_INCLUDE (add that -sys as a Cargo dependency so Cargo sets the var). Emit links + cargo:include (→ DEP_<CRATE_UPPER>_INCLUDE) for downstream.
4. wrapper.h #including the reference @cInclude headers; bindgen in build.rs with an allowlist matching the reference symbol set; macOS -isysroot via xcrun for libclang.
5. KAT smoke test in src/lib.rs: init + ONE real known-answer operation (per-crate below). No UB, no static_mut_refs.
6. Register: root Cargo.toml [workspace.members] + [workspace.dependencies]; add <CRATE> to .port/tools/gates/check_leaks.sh FFI_CRATES (C-FFI → G6 ASan). -sys crates are KAT-only (not in check_test_counts.py's parity crate_map — confirm, don't fabricate).
7. GATES FOREGROUND all green before commit: cargo build -p <CRATE>; cargo test -p <CRATE> (KAT passes); cargo clippy -p <CRATE> --all-targets -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh GREEN. Be patient with C++.
8. Commit ONLY this crate's files + shared registration edits (root Cargo.toml, check_leaks.sh, Cargo.lock). Leave unrelated working-tree changes untouched. Message "rustty: vendor <CRATE> (<lib> <version>, cc+bindgen+KAT) — R3" ending with:
Co-Authored-By: Claude Opus 4.8 (1M context)

Return the structured result. If a hard blocker stops you (e.g. an unresolved transitive C dep needing its OWN new -sys), do NOT fabricate — commit whatever cleanly builds and report the blocker precisely.`

const CRATES = [
  {
    phase: 'libpng-sys', crate: 'libpng-sys', pkg: 'libpng',
    url: 'https://deps.files.ghostty.org/libpng-1220aa013f0c83da3fb64ea6d327f9173fa008d10e28bc9349eac3463457723b1c66.tar.gz',
    notes: `libpng 1.6 (C). Depends on zlib → add crates-sys/zlib-sys as a Cargo dep and pass its DEP_Z_INCLUDE to the cc include path (freetype-sys does exactly this). CONFIG HEADER: ghostty PROVIDES pkg/libpng/pnglibconf.h (a pre-generated config) — vendor THAT file (copy from the ghostty pkg dir) onto the include path rather than running libpng's configure; mirror pkg/libpng/build.zig's source list + defines. bindgen over wrapper.h (#include <png.h>) allowlist png_.*/PNG_.*. KAT: png_access_version_number() == 10643 (1.6.43) OR create+destroy a png_struct read pointer (png_create_read_struct) and assert non-null then png_destroy_read_struct. NOTE: freetype-sys currently bundles its own libpng copy; this standalone libpng-sys is the faithful separate ghostty package (pkg/libpng) and will back dcimgui-sys — do NOT modify freetype-sys.`,
  },
  {
    phase: 'sentry-sys', crate: 'sentry-sys', pkg: 'sentry',
    url: 'https://deps.files.ghostty.org/sentry-1220446be831adcca918167647c06c7b825849fa3fba5f22da394667974537a9c77e.tar.gz',
    notes: `sentry-native (C). Read pkg/sentry/build.zig CAREFULLY for the EXACT compiled backend + transport ghostty selects (likely the in-process backend + a minimal/none transport; it compiles a specific subset of src/*.c + a platform module — mirror it precisely, do NOT pull crashpad/breakpad/curl unless the reference build.zig compiles them). bindgen over wrapper.h (#include <sentry.h>) allowlist sentry_.*/SENTRY_.*. KAT (no network, no real init): sentry_value_new_object() / sentry_value_new_string("x") roundtrip + sentry_value_get_type asserts, then sentry_value_decref — a pure value-API known-answer, avoiding sentry_init (which starts a transport/thread).`,
  },
  {
    phase: 'dcimgui-sys', crate: 'dcimgui-sys', pkg: 'dcimgui',
    url: 'https://deps.files.ghostty.org/DearBindings_v0.17_ImGui_v1.92.5-docking.tar.gz',
    notes: `Dear ImGui 1.92.5 (docking) via DearBindings 0.17 C API (C++). Read pkg/dcimgui/build.zig + pkg/dcimgui/ext.cpp for the exact source list (imgui core .cpp + dcimgui.cpp bindings + the freetype font backend if enabled). Sub-deps: freetype (add crates-sys/freetype-sys dep → DEP_FREETYPE_INCLUDE) and libpng (add crates-sys/libpng-sys dep → DEP_PNG_INCLUDE — libpng-sys is vendored EARLIER in this same run, so it is available). cpp(true), C++ std per build.zig, IMGUI_ENABLE_FREETYPE etc. defines exactly as the reference. bindgen over wrapper.h (#include <dcimgui.h>) allowlist ImGui_.*/ImGui.*/Im.*. KAT: ImGui_CreateContext(NULL) non-null → ImGui_GetIO() non-null → set a field → ImGui_DestroyContext() (basic context lifecycle, no rendering backend). If the C++ build is heavy, be patient.`,
  },
]

const results = []
for (const c of CRATES) {
  phase(c.phase)
  try {
    const r = await agent(
      `${COMMON}\n\n=== THIS CRATE ===\nRust crate: ${c.crate} (directory crates-sys/${c.crate}/)\nghostty package: pkg/${c.pkg}\nEXACT tarball URL: ${c.url}\nPER-CRATE NOTES: ${c.notes}`,
      { label: c.crate, phase: c.phase, schema: RESULT_SCHEMA }
    )
    results.push(r || { crate: c.crate, ok: false, kat_green: false, commit: '', blockers: ['agent returned null'], notes: '' })
  } catch (e) {
    results.push({ crate: c.crate, ok: false, kat_green: false, commit: '', blockers: ['threw: ' + String(e)], notes: '' })
  }
  log(`${c.crate}: ${results[results.length - 1].ok ? 'OK ' + results[results.length - 1].commit : 'BLOCKED'}`)
}
return { results }
