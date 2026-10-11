/** Tests the porting link guard on fixtures: one passing and one failing fixture per rule. */
import assert from "node:assert/strict";
import { linkFailures } from "./lib/porting-links-guard.mjs";

/** COMMITS is a fixture commit map: one Rustty and one Ghostty commit. */
const COMMITS = {
  "58aa62a81": "r:58aa62a81e7db5f8762b0158acc661dafc31f230",
  "246f70287": "g:246f702876b924a1cb7cade1e99274d1470302fc",
};

/** FACTS is a fixture fact table whose facts all carry a link. */
const FACTS = { days: { href: "https://example.org/days" } };

/** Runs the guard on one fixture article. */
const run = (mdx, facts = FACTS) => linkFailures({ mdx, facts, commits: COMMITS });

// Facts: a linked fact passes; a fact without href fails.
assert.deepEqual(run('Over <Fact id="days" />.'), []);
assert.match(run('Over <Fact id="days" />.', { days: { href: "" } }).join("\n"), /fact days has no href/);

// Commit hashes: CommitLink, the allowlist, code blocks and markdown links pass; a bare hash fails.
assert.deepEqual(
  run(
    'At <CommitLink hash="58aa62a81" /> against <CommitLink hash="246f70287" />, tag `97e244c`, [`246f70287`](https://x).\n```sh\ngit show 58aa62a81\n```\n',
  ),
  [],
);
assert.match(run("Ghostty `246f70287`, the reference.").join("\n"), /bare commit hash `246f70287`/);
assert.match(run('<CommitLink hash="deadbee" />').join("\n"), /resolves in no repository/);

// Source paths: SourceLink, extension-only spans and non-source names pass; a bare path fails.
assert.deepEqual(
  run('See <SourceLink at="58aa62a81" path="Cargo.toml" lines="1-2" />, every `.rs` file, `RGB::parse`, `-sys`.'),
  [],
);
assert.match(run("The `src/terminal/Screen.zig` file.").join("\n"), /bare source path `src\/terminal\/Screen.zig`/);
assert.match(run("Edit `quirks.zig` first.").join("\n"), /bare source path `quirks.zig`/);
assert.match(run("At `src/Surface.zig:481-491`.").join("\n"), /bare source path `src\/Surface.zig:481-491`/);
assert.match(run("See `page.zig:979`.").join("\n"), /bare source path `page.zig:979`/);
assert.match(run("In `crates/rustty-vt/src/parser.rs#L10-L20`.").join("\n"), /bare source path/);
assert.deepEqual(run("The `:481-491` range and `x.rs:` stay."), []);
assert.match(run('<SourceLink at="0000000" path="a.rs" />').join("\n"), /resolves in no repository/);

console.log("porting link guard OK");
