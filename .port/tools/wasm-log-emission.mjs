// wasm-log-emission.mjs — EXECUTE a restored wasm logging site end to end and
// OBSERVE the record arrive at the host sink.
//
// Every wasm-only logging restoration in the parity-closure plan
// (`.port/parity-closure/DECISION-logging.md`) has so far been verified to
// COMPILE for wasm32-unknown-unknown and nothing more; the two installer
// regression gates (`rustty-web-canvas`'s and `rustty-font-shaper`'s
// `install_contract_tests`) check the WIRING — that every unmangled export opens
// with `install_wasm_log_backend()` — by scanning SOURCE. Neither observes a
// byte reaching a sink. This script closes that gap: it is the only EXECUTED
// evidence for the whole class.
//
// The observation channel is the sink itself. `crates/rustty-os/src/wasm/log.rs`
// declares it as a wasm import:
//
//     #[link(wasm_import_module = "env")] extern "C" { #[link_name = "log"] ... }
//
// so a JS `env.log(ptr, len)` that decodes out of `ex.memory.buffer` IS the
// terminal end of the chain
//     log::warn!(target: "<scope>", ...)
//       -> WasmLogger::log            (rustty-os, registered by install_log_backend)
//       -> freestanding::log          (level_txt + "(" scope "): " + message)
//       -> env.log(ptr, len)          <- captured here.
//
// DEBUG ARTIFACT ONLY. The release build strips these call sites at compile time
// through `log`'s `release_max_level_off` feature — `strings` finds ZERO hits for
// the restored text in the release .wasm and one in the debug .wasm. A release
// run would pass while observing an emission that does not exist, so the
// artifact is asserted to be a debug one before anything else runs.
//
// Run:
//   .port/tools/build-web-canvas-wasm.sh            # DEBUG, no --release
//   node .port/tools/wasm-log-emission.mjs
//
// Env:
//   RUSTTY_WC_WASM        override the artifact path (used for the counterfactual
//                         pre-3a90678c build; see --expect-silence)
//   RUSTTY_WC_LOG_TEXT    override the expected message (self-test: a wrong value
//                         must make arm A go RED)
//
//                         NOTE (fix round 1, finding 2): this is an env-settable
//                         GATE INPUT, the shape ci.sh's own notes record killing
//                         once as a spoofing hole (G1b: an override that could
//                         redirect a probe away from what the script checks). It
//                         is safe HERE, and only here, because it can only make
//                         the gate STRICTER or RED, never green: arm A demands
//                         that the artifact ACTUALLY EMIT the value, as the sole
//                         record on the path (see below), so spoofing a pass
//                         would require supplying a string the wasm genuinely
//                         emitted — i.e. proving the very thing the gate asserts.
//                         The always-on arms (the DEBUG-artifact check and both
//                         negative controls) are NOT overridable. Do not add an
//                         override that can weaken an assertion.
// Flags:
//   --expect-silence      INVERT arm A: assert the driven path emits NOTHING.
//                         Only meaningful against an artifact whose exports do
//                         not install the backend (the pre-fix failure mode).
//
// PRE-FIX COUNTERFACTUAL (reproduce in ~1 min; do NOT commit the edit):
//   sed -i '' '/^    rustty_font_shaper::install_wasm_log_backend();$/d' \
//     crates/rustty-web-canvas/src/lib.rs
//   .port/tools/build-web-canvas-wasm.sh && cp \
//     target/wasm32-unknown-unknown/debug/rustty_web_canvas.wasm /tmp/prefix.wasm
//   git diff --stat crates/rustty-web-canvas/src/lib.rs   # then restore the file
//   RUSTTY_WC_WASM=/tmp/prefix.wasm node .port/tools/wasm-log-emission.mjs --expect-silence
// That artifact is byte-identical to the shipped one except for the twelve
// deleted install calls — i.e. exactly the state of this artifact before
// 3a90678c — and the record is DROPPED. The same artifact without
// --expect-silence goes RED, and the shipped artifact WITH it goes RED, so the
// two builds are distinguished in both directions.
import { readFileSync, statSync } from "node:fs";
import { makeWebCanvasEnv } from "./web-canvas-glue.mjs";
import { makeMockCanvas } from "./web-canvas-mock.mjs";
import { join } from "node:path";
import { targetDir } from "./_target_dir.mjs";

const EXPECT_SILENCE = process.argv.includes("--expect-silence");

const WASM = process.env.RUSTTY_WC_WASM
  ? process.env.RUSTTY_WC_WASM
  : join(targetDir(), "wasm32-unknown-unknown", "debug", "rustty_web_canvas.wasm");

// The CSS "font" value driven through the failing path. `OTTO` is a real
// font-FILE magic (CFF/OpenType) and pure ASCII, so `validate_css_font_str`
// rejects it as font bytes AND `String::from_utf8_lossy` renders it verbatim —
// the expected message below is exact, not approximate.
const BAD_FONT = "OTTOgarbage-font-bytes";
const GOOD_FONT = "monospace";

// `freestanding::log` composes `level_txt` ++ "(" ++ scope ++ "): " ++ message.
// level "warning" = `log::Level::Warn`; scope "deferred_face" =
// `deferred_face::LOG_SCOPE`; the body is the restored
// `crates/rustty-font-shaper/src/deferred_face.rs` `has_codepoint` warn.
const EXPECTED =
  process.env.RUSTTY_WC_LOG_TEXT ??
  `warning(deferred_face): failed to init face for codepoint check ` +
    `face=${BAD_FONT} err=InvalidFontStr`;

// The restored text as it appears in the artifact's data segments. Its PRESENCE
// is what distinguishes a debug artifact from a release one (two reviews proved
// release strips it), so it doubles as the debug assertion.
const DEBUG_MARKER = "failed to init face for codepoint check";

const CP_A = "A".codePointAt(0);
const P_TEXT = 0,
  P_EMOJI = 1,
  P_NULL = 2;

// ---------------------------------------------------------------------------
// Artifact: must exist (never skip green) and must be a DEBUG build.
// ---------------------------------------------------------------------------
let bytes;
try {
  bytes = readFileSync(WASM);
} catch {
  console.error(`BLOCKED: wasm artifact not found: ${WASM}`);
  console.error(`         Build it with: .port/tools/build-web-canvas-wasm.sh   (NO --release)`);
  console.error(`         Exiting 3 (environment missing) rather than passing vacuously.`);
  process.exit(3);
}
if (!Buffer.from(bytes).includes(DEBUG_MARKER)) {
  console.error(
    `FAIL: ${WASM} does not contain ${JSON.stringify(DEBUG_MARKER)} — this is a RELEASE\n` +
      `      artifact (log's release_max_level_off strips the call sites at compile time).\n` +
      `      A release run would pass while observing an emission that does not exist.\n` +
      `      Rebuild WITHOUT --release: .port/tools/build-web-canvas-wasm.sh`,
  );
  process.exit(1);
}
console.log(`artifact: ${WASM}`);
console.log(`          ${statSync(WASM).size} bytes, DEBUG (carries the restored message text)`);

const mod = await WebAssembly.compile(bytes);

// ---------------------------------------------------------------------------
// A fresh instance per arm: `install_log_backend` is `Once`-guarded per module
// INSTANCE (its statics live in that instance's linear memory), so each arm gets
// an un-installed backend and an empty capture buffer.
// ---------------------------------------------------------------------------
async function freshInstance() {
  const captured = [];
  let inst;
  const dec = new TextDecoder();
  const env = {
    ...makeWebCanvasEnv({
      memory: () => inst.exports.memory,
      createCanvas: makeMockCanvas(),
    }),
    // THE SINK. `rustty-os::wasm::log::freestanding::log`'s `js::log(ptr, len)`.
    log(ptr, len) {
      captured.push(dec.decode(new Uint8Array(inst.exports.memory.buffer, ptr, len)));
    },
  };
  inst = await WebAssembly.instantiate(mod, { env });
  return { ex: inst.exports, captured };
}

// Drive ONLY the three `rustty-web-canvas`-owned deferred/alloc exports. None of
// the seven `rustty-font-shaper` `rustty_wc_face_*` / `rustty_wc_shaper_*`
// exports is touched, so this is exactly the host that lost the record before
// the artifact-wide installer fix (3a90678c): all three of these were among the
// twelve exports that did not install a backend.
function driveDeferred(ex, fontStr, presentationPayload, presentationQuery) {
  const raw = new TextEncoder().encode(fontStr);
  const p = ex.rustty_wc_alloc(raw.length); //          export 1/3 — silent
  new Uint8Array(ex.memory.buffer).set(raw, p);
  const df = ex.rustty_wc_deferred_new(p, raw.length, presentationPayload); // 2/3 — silent
  ex.rustty_wc_free(p, raw.length);
  if (df === 0) throw new Error("rustty_wc_deferred_new returned null");
  const rc = ex.rustty_wc_deferred_has_codepoint(df, CP_A, presentationQuery); // 3/3 — LOGS
  ex.rustty_wc_deferred_free(df);
  return rc;
}

const fail = (m) => {
  console.error(`FAIL: ${m}`);
  process.exit(1);
};

// --- ARM A (positive): the restored site fires, with the EXACT text ---------
{
  const { ex, captured } = await freshInstance();
  const rc = driveDeferred(ex, BAD_FONT, P_TEXT, P_NULL);
  if (rc !== 0) fail(`has_codepoint on an invalid CSS font returned ${rc}, expected 0`);

  if (EXPECT_SILENCE) {
    if (captured.length !== 0)
      fail(`--expect-silence: the driven path emitted ${captured.length} record(s): ${JSON.stringify(captured)}`);
    console.log(
      `arm A (--expect-silence): OK — the deferred surface emitted NOTHING.\n` +
        `          This is the PRE-FIX failure mode: the restored site ran (has_codepoint\n` +
        `          returned false) but no backend was installed, so the record was dropped.`,
    );
  } else {
    if (captured.length === 0)
      fail(
        `the restored site produced NO log record. Expected: ${JSON.stringify(EXPECTED)}\n` +
          `      (backend not installed by this export? see rustty-web-canvas's install_contract_tests)`,
      );
    // EXACTLY ONE record, and it is the expected one (fix round 1, finding 3).
    // `includes()` alone would not bound the record count, so an artifact that
    // also emitted unrelated chatter on this path would still pass. The drive
    // below touches three exports of which only `has_codepoint` logs, so one
    // record is the whole truth about this path; if a future restoration adds a
    // second one here, this arm SHOULD go red and be updated deliberately.
    if (captured.length !== 1 || captured[0] !== EXPECTED)
      fail(
        `expected EXACTLY ONE record and that it be the restored one.\n` +
          `      expected: [${JSON.stringify(EXPECTED)}]\n` +
          `      captured: ${JSON.stringify(captured)}`,
      );
    console.log(`arm A (positive): OK — captured ${JSON.stringify(EXPECTED)} (sole record)`);
  }
}

// --- ARM B (negative control 1): the fast path must be SILENT ---------------
// Same three exports, same codepoint; only the requested presentation differs.
// A mismatched presentation returns false WITHOUT initializing a face, so the
// restored site is not reached and NOTHING may be captured. Without this arm a
// harness that merely captures unrelated chatter would pass.
if (!EXPECT_SILENCE) {
  const { ex, captured } = await freshInstance();
  const rc = driveDeferred(ex, BAD_FONT, P_TEXT, P_EMOJI);
  if (rc !== 0) fail(`presentation-mismatch fast path returned ${rc}, expected 0`);
  if (captured.length !== 0)
    fail(
      `negative control: the presentation fast path emitted ${captured.length} record(s): ` +
        JSON.stringify(captured),
    );
  console.log("arm B (negative control): OK — the fast path emitted 0 records");
}

// --- ARM C (negative control 2): a WELL-FORMED font must not warn -----------
// The same export, the same slow path, a valid CSS font value: `Face::init_named`
// SUCCEEDS, so the warn must not appear.
//
// It is NOT silent, though: the success path emits `face initialized: {}` at
// DEBUG level, and `install_log_backend` sets LevelFilter::Debug under
// debug_assertions. Asserting "zero records" here would be wrong against this
// code. Asserting the exact record that IS produced is stronger anyway — it
// pins the level/scope mapping for a second site, at a second level.
if (!EXPECT_SILENCE) {
  const { ex, captured } = await freshInstance();
  const rc = driveDeferred(ex, GOOD_FONT, P_TEXT, P_NULL);
  if (rc !== 1) fail(`has_codepoint on a valid CSS font returned ${rc}, expected 1`);
  const warned = captured.filter((m) => m.includes(DEBUG_MARKER));
  if (warned.length !== 0)
    fail(`a well-formed font still produced the failure warn: ${JSON.stringify(warned)}`);
  const wantDebug = `debug(font_face): face initialized: ${GOOD_FONT}`;
  if (!captured.includes(wantDebug))
    fail(
      `expected the success-path debug record ${JSON.stringify(wantDebug)}\n` +
        `      captured: ${JSON.stringify(captured)}`,
    );
  // A SECOND restored site, observed at a second level in the same run: the
  // `log.debug("metrics font={s} value={}")` in `calc_metrics`. Only its prefix
  // is asserted — the numbers come from the mock canvas's TextMetrics and are
  // not the thing under test.
  const metricsPrefix = `debug(font_face): metrics font=${GOOD_FONT} value=FaceMetrics {`;
  if (!captured.some((m) => m.startsWith(metricsPrefix)))
    fail(
      `expected a record starting ${JSON.stringify(metricsPrefix)}\n` +
        `      captured: ${JSON.stringify(captured)}`,
    );
  console.log(
    `arm C (well-formed control): OK — 0 failure warns; the success path's\n` +
      `          ${JSON.stringify(wantDebug)} and ${JSON.stringify(metricsPrefix + " ... }")}\n` +
      `          arrived instead (a second restored site, at a second level)`,
  );
}

console.log(
  EXPECT_SILENCE
    ? "\nWASM LOG EMISSION (counterfactual): PASS ✓ — no backend, no record."
    : "\nWASM LOG EMISSION: PASS ✓ — a restored wasm logging site was EXECUTED and\n" +
        "its record was OBSERVED arriving at the host sink, driven only through exports\n" +
        "that did not install a backend before 3a90678c.",
);
