export const meta = {
  name: 'cvt-examples-port',
  description: 'Port ghostty c-vt/cpp-vt runtime examples 1:1 onto <rustty/vt.h>, build under the oracle-parity profile, diff stdout vs ghostty (report parity or the exact mismatch)',
  phases: [{ title: 'Port+parity', detail: 'one agent per example: symbol-swap .c, build.sh, README, cvt-parity diff' }],
}

const EXAMPLES = [
  'c-vt', 'c-vt-colors', 'c-vt-effects', 'c-vt-encode-focus', 'c-vt-encode-key',
  'c-vt-encode-mouse', 'c-vt-formatter', 'c-vt-grid-ref-tracked', 'c-vt-grid-traverse',
  'c-vt-kitty-graphics', 'c-vt-modes', 'c-vt-paste', 'c-vt-render', 'c-vt-selection',
  'c-vt-selection-gesture', 'c-vt-sgr', 'c-vt-size-report', 'c-vt-static', 'c-vt-stream',
  'cpp-vt-stream',
]

const SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['name', 'status', 'detail'],
  properties: {
    name: { type: 'string' },
    status: { type: 'string', enum: ['PARITY', 'DIFFERS', 'BUILD_FAIL', 'NEEDS_INPUT'] },
    detail: { type: 'string', description: 'for DIFFERS: the exact diff (ghostty < vs rustty >) + your hypothesis (intentional divergence vs rustty bug); for BUILD_FAIL: the compiler error; for PARITY: line count' },
    files: { type: 'array', items: { type: 'string' } },
  },
}

const RECIPE = `You port ONE ghostty c-vt example to rustty and verify byte-parity. Repo: /Users/gael/projects/rustty (work in the main tree). Oracle: /Users/gael/projects/ghostty (READ-ONLY).

The curated public C headers already exist (include/rustty/vt.h + vt/*.h) and the oracle-parity dylib is pre-built (target/release/librustty_vt_c.dylib, features identity-oracle+simd). A worked reference is example/c-vt-build-info (ported + build.sh + README, BYTE-IDENTICAL).

STEPS for example "<NAME>":
1. Read the ghostty source: /Users/gael/projects/ghostty/example/<NAME>/src/*.c (or .cpp for cpp-vt-*) and its README + build.zig. Note if it reads argv/stdin or is self-contained.
2. Create /Users/gael/projects/rustty/example/<NAME>/src/ and port each .c/.cpp by symbol-swap ONLY (the header now matches ghostty 1:1, so this is mechanical):
   sed -e 's|#include <ghostty/|#include <rustty/|g' -e 's/GHOSTTY_/RUSTTY_/g' -e 's/ghostty_/rustty_/g' -e 's/libghostty/librustty/g' -e 's/Ghostty/Rustty/g'
   Then audit: \`grep -niE 'ghostty' example/<NAME>/src/*\` — only genuine oracle-path/upstream refs may remain (fix stray product prose).
3. Copy the build.sh from example/c-vt-build-info/build.sh into example/<NAME>/. For a C++ example (main.cpp), change the compile to use clang++ (\`"$CC++"\` via \`xcrun --find clang++\`, add \`-std=c++17\`, and glob src/*.cpp). Otherwise use it verbatim.
4. Port the README: \`sed -e 's/GHOSTTY_/RUSTTY_/g' -e 's/ghostty_/rustty_/g' -e 's/Ghostty/Rustty/g' -e 's/libghostty/librustty/g' -e 's/zig build run/.\\/build.sh/g' <oracle README> > example/<NAME>/README.md\`.
5. Run the parity gate: \`.port/tools/cvt-parity.sh <NAME>\`. It builds+runs BOTH the rustty port and ghostty's \`zig build run\` and diffs stdout.

REPORT (structured):
- status=PARITY if "BYTE-IDENTICAL"; include the line count in detail.
- status=DIFFERS if the diff is non-empty: put the FULL diff in detail (the \`< ghostty / > rustty\` lines) PLUS your hypothesis — is it an intentional default-profile divergence (identity/version/branding — but note the parity build already uses identity-oracle+simd, so most identity deltas should be gone) OR a likely rustty bug (wrong value/behavior)? Do NOT fix rustty code — the controller triages.
- status=BUILD_FAIL if the rustty port won't compile: put the exact clang error in detail (likely a header symbol the port needs that the curated header names differently — report the specific missing symbol).
- status=NEEDS_INPUT if the example requires argv/stdin that \`zig build run\` can't supply (so both sides need a fixture): describe what input it needs.
Do NOT git commit. Keep all changes under example/<NAME>/ (no rustty crate/code edits).`

phase('Port+parity')
const results = await pipeline(
  EXAMPLES,
  (name) => agent(RECIPE.replaceAll('<NAME>', name), {
    label: `cvt:${name}`,
    phase: 'Port+parity',
    schema: SCHEMA,
    effort: 'medium',
  }).then((r) => r ? { ...r, name } : { name, status: 'BUILD_FAIL', detail: 'agent returned null' }),
)

const by = (s) => results.filter(Boolean).filter((r) => r.status === s).map((r) => r.name)
return {
  parity: by('PARITY'),
  differs: results.filter(Boolean).filter((r) => r.status === 'DIFFERS'),
  build_fail: results.filter(Boolean).filter((r) => r.status === 'BUILD_FAIL'),
  needs_input: by('NEEDS_INPUT'),
}
