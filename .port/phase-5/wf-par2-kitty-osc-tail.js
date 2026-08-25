export const meta = {
  name: 'p5-par2-kitty-osc-tail',
  description: 'Phase-5 tier 2: kitty graphics waves, OSC 99 notifications, platform tail',
  phases: [{ title: 'Port', detail: 'worktree-isolated, patch harvest; disjoint ownership' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

const COMMON = `
Phase-5 porter for the rustty port. ISOLATED worktree of ${REPO}; FIRST: git log --oneline -1
— if not 5a8f9e83 or newer, git reset --hard main. Oracle AT the pin 683d8db64: ${REF};
range 12967b68f..683d8db64.

READ FIRST: your assessment in ${REPO}/.port/phase-5/assessments/; the plan
${REPO}/docs/superpowers/plans/2026-08-25-phase5-upstream-resync.md (range-diff traps + SB
hazards BINDING); ${REPO}/.port/PORT_RULES.md. On main already: Phase-4 complete + P5-W1
(simd base64 Streaming, wuffs swizzle/rgbaSrcOver, paste core with 'W3 seam' markers, sys
random_secure hook, config clipboard_write_limit).

BINDING: R1 end-state (reverted intermediates NOT ported, surviving tests carried); R2; R7;
R9 SAFETY; DG5; SB raw-ptr discipline — graphics_storage s:*Screen sites stay raw-ptr;
collect-keys-first for remove-during-iteration; prove restructured aliasing on isolated
Miri harnesses (mirihar layout precedents in .port/phase-4/ and .port/phase-2/).
VERIFY: cargo build + crate lanes green; vt full lane at the end (~90s).
HARVEST: git add -A && git diff --cached > ${PATCHDIR}/<PATCHNAME>.patch (non-empty).
Report per schema; every upstream test carried by name.
`

const SCHEMA = {
  type: 'object',
  required: ['patch_path', 'summary', 'test_evidence', 'tests_carried', 'caveats'],
  properties: {
    patch_path: { type: 'string' }, summary: { type: 'string' },
    files_created: { type: 'array', items: { type: 'string' } },
    files_modified: { type: 'array', items: { type: 'string' } },
    tests_carried: { type: 'array', items: { type: 'string' } },
    test_evidence: { type: 'string' }, caveats: { type: 'string' },
  },
}

phase('Port')
const results = await parallel([
  () => agent(COMMON + `
YOUR TASK (PATCHNAME p5-kitty-spec): assessment A clusters **B1 (spec-conformance wave, 13
commits) + B2 (relative placements + placeholder lookup, 3 commits)** as the range-diff END
STATE over crates/rustty-vt/src/kitty/graphics_{command,exec,storage,image,unicode}.rs.
Test-count targets: exec 12->66, command 32->38, image 25->27, storage 38->71 (minus B3/B4's
share — read the assessment's per-commit test lists). Named traps: 242723223 makes value
parsing LENIENT where rustty hard-errors at graphics_command.rs:1139; f8b40a023 DELETES
'storage: delete all placements and images' which rustty carries — REPLACE it with the end
state; 4b915e3bc's C-key acceptance is REVERTED by f3e98fb72 (port the revert's end state);
DG2 anchor max_dimension-before-multiply MUST survive (the regression anchor test named in
PORT_RULES). B2: tagged internal/external PlacementId, parent chains, resolve_chain,
cascaded deletion — collect-keys-first, never removeByPtr-during-iteration; renderer +
c/kitty_graphics halves are OTHER waves (leave PORT NOTE seams compiling). s:*Screen stays
raw-ptr everywhere. DO NOT touch graphics_animation/pixel (sibling owns B3/B4) or
clipboard/dnd files. Run Miri on the storage+exec filters at the end.`,
    { label: 'p5-kitty-spec', phase: 'Port', schema: SCHEMA, isolation: 'worktree' }),

  () => agent(COMMON + `
YOUR TASK (PATCHNAME p5-kitty-anim): assessment A clusters **B3 (margin-aware placement
scrolling, 90bce0d2d) + B4 (animation + pixel)**. B3: two-phase scrollMarginsBegin/end guard
+ Terminal.zig +134 call sites (crates/rustty-vt/src/terminal.rs) + 11 storage tests. THE
SB HAZARD #1: ScrollMargins holds *Screen AND *Pin pointers into the placements map across
scroll mutations (call-end-exactly-once; *Pin dangles on rehash) — port with NonNull raw-ptr
guard per rust-mut-args-strongly-protected AND prove on an isolated non-FFI Miri harness at
.port/phase-5/mirihar-scroll-margins/ (copy .port/phase-4/mirihar-reflow-recycle's layout;
include a foil the harness proves Miri catches). B4: NEW graphics_animation.rs +
graphics_pixel.rs (wuffs swizzle landed in W1 behind the existing feature=wuffs edge) + the
exec +1485 animation end state. Traps: gap_ms u32->i32 BIT-PATTERN cast (golden-byte test),
Background field order flips to b,g,r,a (little-endian bit-cast — golden-byte test),
animation exec holds map-interior image pointers across evicting mutators (hazard #3:
re-lookup or collect-first), animationTick's renderer drive is ANOTHER wave (leave the vt
API + PORT NOTE). f3e98fb72 reverts 4b915e3bc within B4's files too — end state only.
You own graphics_animation/pixel/exec-animation-paths + terminal.rs scroll sites; the
sibling owns the rest of graphics_* — COORDINATE by not touching command/storage/image
except your exec animation arms and the 11 B3 storage tests (append-only test additions;
if a storage fn signature must change for B3, note it and make the minimal change).
Run Miri on your filters + the harness.`,
    { label: 'p5-kitty-anim', phase: 'Port', schema: SCHEMA, isolation: 'worktree' }),

  () => agent(COMMON + `
YOUR TASK (PATCHNAME p5-osc99): assessment B cluster **B-2 — OSC 99 Kitty desktop
notifications** (9 commits) + the osc metadata machinery: NEW
${REF}/src/terminal/osc/kitty_metadata.zig + ${REF}/src/terminal/osc/parsers/
kitty_desktop_notification.zig -> crates/rustty-vt/src/osc/. This owns the G1b osc drop=47
(minus any clipboard-parser share — check the drop attribution in your assessment). Also
B's osc churn rows + the 8c5bc3d29 OSC bulk-reading vt-side hunks (stream.zig
consumeOscString + osc.zig nextSlice/writeSlice — the simd side landed in W1). DO NOT touch
kitty/ or kitty_dnd_protocol.rs / kitty_clipboard_protocol.rs (other waves own them; the
one-line 5350d4a5f clipboard-parser touch belongs to W3). Every upstream test. Full vt lane
green at the end.`,
    { label: 'p5-osc99', phase: 'Port', schema: SCHEMA, isolation: 'worktree' }),

  () => agent(COMMON + `
YOUR TASK (PATCHNAME p5-tail): assessment E's non-kitty clusters [.port/phase-5/assessments/
E-platform-tail.md]: po re-mirror (4 catalogs, the Phase-4 par-1 method: learn the identity
transform from an existing catalog diff, mirror the oracle END STATE, msgfmt --check every
touched catalog), cli rows (1 ABSENT 1 PARTIAL), termio non-kitty rows (1 PARTIAL 1 ABSENT),
os rows, .github (2), typos.toml, valgrind.supp, pkg/afl if rustty mirrors it (check;
likely N-A — record). SKIP every kitty-clipboard-coupled row (bundle KC is W3's; the config
half landed in W1). Every upstream test; touched crate lanes green.`,
    { label: 'p5-tail', phase: 'Port', schema: SCHEMA, isolation: 'worktree' }),
])

return { tasks: results.filter(Boolean) }
