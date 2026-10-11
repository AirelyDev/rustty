// _target_dir.mjs — where cargo really puts this workspace's artifacts.
//
// The Node twin of `.port/tools/gates/_target_dir.sh`. A tool that reads a
// built artifact (librustty_vt_c.*, a .wasm) imports `targetDir()` instead of
// spelling `<repo>/target/...`: a literal path reads a different (stale or
// absent) file as soon as ci.sh or a caller sets CARGO_TARGET_DIR, which is
// what G-TGTDIR arm 1 forbids.
//
// The answer, in order:
//   1. RUSTTY_TARGET_DIR, when the calling gate already asked cargo (it passes
//      `rustty_target_dir`'s answer, so both languages read the same dir);
//   2. `cargo metadata`'s `target_directory`, which honours CARGO_TARGET_DIR,
//      CARGO_BUILD_TARGET_DIR and any `build.target-dir` config;
//   3. `<repo>/target`, if cargo metadata fails (the caller's own build has
//      already failed then).
import { execFileSync } from "node:child_process";
import { resolve } from "node:path";

export const REPO_ROOT = resolve(import.meta.dirname, "..", "..");

let cached;

export function targetDir() {
  if (cached !== undefined) return cached;
  if (process.env.RUSTTY_TARGET_DIR) {
    cached = resolve(REPO_ROOT, process.env.RUSTTY_TARGET_DIR);
    return cached;
  }
  try {
    const out = execFileSync(
      "cargo",
      ["metadata", "--no-deps", "--format-version", "1", "--offline"],
      { cwd: REPO_ROOT, stdio: ["ignore", "pipe", "ignore"], maxBuffer: 64 << 20 },
    );
    const dir = JSON.parse(out.toString()).target_directory;
    if (typeof dir === "string" && dir.length > 0) {
      cached = dir;
      return cached;
    }
  } catch {
    // fall through to the default
  }
  cached = resolve(REPO_ROOT, "target");
  return cached;
}
