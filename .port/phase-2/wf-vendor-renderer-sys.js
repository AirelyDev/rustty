export const meta = {
  name: 'vendor-renderer-sys',
  description: 'Sequentially vendor the 3 renderer-critical -sys crates (wuffs, glslang, spirv-cross) via the proven fetch->vendor->cc->bindgen->KAT template, per-crate commit',
  phases: [
    { title: 'wuffs-sys' },
    { title: 'glslang-sys' },
    { title: 'spirv-cross-sys' },
  ],
}

const RESULT_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['crate', 'ok', 'kat_green', 'commit', 'blockers', 'notes'],
  properties: {
    crate: { type: 'string' },
    ok: { type: 'boolean' },
    kat_green: { type: 'boolean' },
    commit: { type: 'string', description: 'commit sha, or empty if not committed' },
    blockers: { type: 'array', items: { type: 'string' } },
    notes: { type: 'string' },
  },
}

const COMMON = `OBEY the rustty PARITY CONSTITUTION + /Users/gael/projects/rustty/.port/PORT_RULES.md. R3 = vendor the SAME upstream C/C++ ghostty uses (no substitutes, no crates.io shortcut). Reference READ-ONLY at /Users/gael/projects/ghostty (Zig 0.15.2).

You are vendoring ONE -sys crate using the PROVEN template already used for zlib-sys/simdutf-sys/highway-sys/gettext-sys/freetype-sys/harfbuzz-sys/oniguruma-sys. STUDY these exemplars for the exact idioms before writing anything:
- crates-sys/zlib-sys (plain C, cc::Build, bindgen, KAT)
- crates-sys/simdutf-sys (C++ amalgamation: cpp(true), std=c++17)
- crates-sys/harfbuzz-sys (C++ amalgamation + DEP_FREETYPE_INCLUDE consumed from a sibling -sys via cargo:include)
- crates-sys/freetype-sys (C + bundled libpng + DEP_Z_INCLUDE include-chaining + a rich bindgen allowlist)
- crates-sys/oniguruma-sys (GENERATES config.h in build.rs from the reference's addConfigHeader over a .cmake.in template, placed FIRST on the include path) — see memory sys-generated-config-header

TEMPLATE STEPS:
1. Read the ghostty reference build files for this package: pkg/<PKG>/build.zig (the buildLib: exact source .c/.cpp list, include dirs, C vs C++, -D defines, C++ std, any addConfigHeader/generated headers, sub-deps) and the *.zig with @cInclude (the bindgen header set), and pkg/<PKG>/build.zig.zon.
2. Fetch the EXACT tarball (URL given below) with curl into a scratch dir under /private/tmp/claude-501/.../scratchpad (or crates-sys/<CRATE>/.fetch), verify it downloads, extract it. Vendor ONLY the source subtree the reference build.zig compiles + the LICENSE/COPYING file into crates-sys/<CRATE>/vendor/ (LICENSE at vendor/LICENSE). Do NOT vendor test corpora / docs / unused backends unless the reference build compiles them.
3. Write crates-sys/<CRATE>/build.rs: a cc::Build mirroring the reference buildLib EXACTLY — same source list, same include dirs, same -D defines, same C++ std, warnings off. cpp(true) for C++. If the reference generates a header (addConfigHeader / configure_file / a build_info template), GENERATE it in build.rs into OUT_DIR and put OUT_DIR first on the include path (the oniguruma-sys pattern). Consume sibling -sys includes via env DEP_<DEP>_INCLUDE where the reference links another pkg. Emit links="<name>" and cargo:include=<vendored include dir> so downstream crates get DEP_<CRATE_UPPER>_INCLUDE.
4. Write crates-sys/<CRATE>/wrapper.h that #includes exactly the reference @cInclude headers; run bindgen in build.rs with an allowlist matching the reference symbol set (the c.zig usage). Keep it minimal-but-complete (the symbols the ghostty Zig wrapper actually calls).
5. Write a KAT smoke test in src/lib.rs: initialize the lib and perform ONE real operation with a KNOWN ANSWER (see per-crate KAT below), asserting the result. Use &raw/externs, no static_mut_refs, no UB.
6. Register the crate: add it to the root Cargo.toml [workspace.members] AND [workspace.dependencies]; add the crate name to .port/tools/gates/check_leaks.sh FFI_CRATES (it has a C-FFI path -> G6 routes to ASan, Miri can't run C). If a downstream mapping in .port/tools/gates/check_test_counts.py is needed for the -sys (usually a -sys has KAT-only, no zig test contract — confirm it's handled/excluded, don't fabricate).
7. GATES, FOREGROUND, all must pass before commit: cargo build -p <CRATE>; cargo test -p <CRATE> (KAT passes); cargo clippy -p <CRATE> --all-targets -D warnings; cargo fmt --check; bash .port/tools/gates/check_acyclic.sh GREEN. Be patient with C++ builds (they can take minutes).
8. Commit with ONLY this crate's files + the shared registration edits (root Cargo.toml, check_leaks.sh, Cargo.lock). Leave any unrelated working-tree changes untouched. Message: "rustty: vendor <CRATE> (<lib> <version>, cc+bindgen+KAT) — R3" ending with the trailer line:
Co-Authored-By: Claude Opus 4.8 (1M context)

Return the structured result (crate, ok, kat_green, commit sha, blockers, notes). If a genuine hard blocker stops you (e.g. an unresolved transitive C dep needing its OWN new -sys), do NOT fabricate — commit whatever cleanly builds, and report the blocker precisely so the next batch can address it.`

const CRATES = [
  {
    phase: 'wuffs-sys', crate: 'wuffs-sys', pkg: 'wuffs',
    url: 'https://deps.files.ghostty.org/wuffs-122037b39d577ec2db3fd7b2130e7b69ef6cc1807d68607a7c232c958315d381b5cd.tar.gz',
    notes: `wuffs is a SINGLE-FILE C library (release/c/wuffs-*.c). The reference compiles it with -DWUFFS_IMPLEMENTATION and a set of -DWUFFS_CONFIG__MODULE__* enabling only the modules ghostty uses (BASE + the image codecs: ADLER32/CRC32/DEFLATE/ZLIB/PNG/etc. — read pkg/wuffs/build.zig for the EXACT module define list and mirror it). The ghostty wrapper is Rust/Zig hand-written over the wuffs C API (there may be a ghostty-authored wuffs.h/helper — check pkg/wuffs/src). The second build.zig.zon dep 'pixels' is TEST-DATA (decoded reference images) — do NOT vendor it into the crate; if a KAT needs a sample image, embed a tiny known PNG inline. KAT: decode a tiny known-answer PNG (or run a wuffs adler32/crc32 over known bytes for a known checksum) and assert the exact result.`,
  },
  {
    phase: 'glslang-sys', crate: 'glslang-sys', pkg: 'glslang',
    url: 'https://deps.files.ghostty.org/glslang-12201278a1a05c0ce0b6eb6026c65cd3e9247aa041b1c260324bf29cee559dd23ba1.tar.gz',
    notes: `glslang is C++ (heavy, many TUs across glslang/, SPIRV/, StandAlone resource_limits). Bindgen headers: glslang/Include/glslang_c_interface.h + glslang/Public/resource_limits_c.h. IMPORTANT: pkg/glslang has an 'override' dir — these are ghostty-provided generated/override headers (e.g. glslang/build_info.h, glslang/OSDependent/... ) that upstream normally generates via CMake/python. Mirror the reference build.zig: add pkg/glslang/override to the include path (vendor those override headers too, from the ghostty pkg dir — they are ghostty-authored build inputs, faithful to vendor), and compile the exact source list build.zig lists (glslang C++ + SPIRV + the C interface + resource_limits_c). C++ std per build.zig (likely c++17). KAT: glslang_initialize_process(); compile a trivial valid vertex shader ("void main(){}") to SPIR-V via the C interface and assert it produces non-empty SPIR-V words (and glslang_shader_get_info_log has no error); finalize.`,
  },
  {
    phase: 'spirv-cross-sys', crate: 'spirv-cross-sys', pkg: 'spirv-cross',
    url: 'https://deps.files.ghostty.org/spirv_cross-1220fb3b5586e8be67bc3feb34cbe749cf42a60d628d2953632c2f8141302748c8da.tar.gz',
    notes: `SPIRV-Cross is C++ (spirv_cross_c.h C API over the C++ core: spirv_cross_c.cpp, spirv_cross.cpp, spirv_parser.cpp, spirv_cross_parsed_ir.cpp, spirv_cfg.cpp + the target backends spirv_glsl/msl/hlsl/cpp/reflect the reference enables). Read pkg/spirv-cross/build.zig for the EXACT source list + which backends + -D defines (e.g. SPIRV_CROSS_C_API_GLSL/MSL/HLSL). Bindgen header: spirv_cross_c.h. C++ std per build.zig. KAT: spvc_context_create; take a tiny known SPIR-V blob (e.g. the words for a trivial shader — you may generate it inline as a const u32 array of a minimal valid SPIR-V module, or reuse glslang-sys if available at test time is NOT allowed cross-crate in a KAT, so embed a minimal SPIR-V const), parse it (spvc_context_parse_spirv), create an MSL or GLSL compiler, compile, and assert a non-empty output string. If a full valid SPIR-V const is impractical, at minimum assert spvc_context_create/spvc_context_get_last_error_string roundtrip + version symbol — but PREFER a real parse+compile KAT.`,
  },
]

const results = []
for (const c of CRATES) {
  phase(c.phase)
  try {
    const r = await agent(
      `${COMMON}\n\n=== THIS CRATE ===\nRust crate: ${c.crate}  (directory crates-sys/${c.crate}/)\nghostty package: pkg/${c.pkg}\nEXACT tarball URL: ${c.url}\nPER-CRATE NOTES: ${c.notes}`,
      { label: c.crate, phase: c.phase, schema: RESULT_SCHEMA }
    )
    results.push(r || { crate: c.crate, ok: false, kat_green: false, commit: '', blockers: ['agent returned null (skipped/died)'], notes: '' })
  } catch (e) {
    results.push({ crate: c.crate, ok: false, kat_green: false, commit: '', blockers: ['threw: ' + String(e)], notes: '' })
  }
  log(`${c.crate}: ${results[results.length - 1].ok ? 'OK ' + results[results.length - 1].commit : 'BLOCKED'}`)
}
return { results }
