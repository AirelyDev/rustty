# Parity Defect Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close **all 497** surviving findings from the rustty↔ghostty functional-parity audit, so that every observable behavioural difference from the oracle at pin `20abdb50a` is either fixed with a regression test or recorded as a reviewed, justified non-defect — with a gate that fails while any finding is still open.

**Architecture:** The audit already did the finding work; this plan is closure, not discovery. Every finding is a row in a machine-readable ledger (`.port/parity-closure/LEDGER.tsv`) with a `state` column. Task 1 builds the gate that reads that ledger, so "all defects addressed" becomes a checked fact instead of a claim. Work then proceeds highest-risk-first: adjudicate the 93 DISPUTED rows (one verifier lens refuted them — they must not be fixed blindly), fix the 12 HIGH rows individually, collapse the seven systemic classes (201 rows that share a root cause and a single fix pattern), then sweep the remaining per-area tail. Each area's checklist is independently shippable.

**Tech Stack:** Rust 1.93.1 (pinned via `rust-toolchain.toml`), Python 3 for gates, `cargo test`/`clippy`/`fmt`, `cargo +nightly miri` for SB lanes, Swift/xcodebuild for the macOS app, the Fedora aarch64 VM at `ssh rvm` for the GTK/Linux lane, and the read-only ghostty oracle at `/Users/gael/projects/ghostty` (pin `20abdb50a`).

**Spec:**

- `docs/parity-audit/README.md` — the audit index (method, totals, the 12 high gaps).
- `docs/parity-audit/<area>.md` — 21 area reports. **Each finding's full evidence lives here**: oracle file:line, rustty file:line, description, evidence, suggested fix, and for DISPUTED rows the refuting lens's verbatim reason.
- `.port/parity-closure/LEDGER.tsv` — all 497 rows, machine-readable (generated; the closure gate reads it).
- `.port/parity-closure/<area>.md` — 21 per-area checkboxes, one per finding, carrying id/class/kind/locations/suggested fix.

## Global Constraints

- **R1 full fidelity.** Comments, tests assertion-for-assertion, error paths. A fix that changes observable behaviour needs a test that fails without it.
- **R3 the oracle is read-only.** Never `git checkout`/`reset`/`stash`/edit under `/Users/gael/projects/ghostty`. `git fetch` is the only sanctioned write. The pin stays `20abdb50a` for the life of this plan — do **not** advance it mid-flight; a moving oracle invalidates every finding's line numbers.
- **R5 gates are never weakened.** A gate change must be net-new coverage, argued in the commit message, with a self-test arm proving the new behaviour. See `d591a985` for the worked precedent.
- **R7 arithmetic/narrowing discipline; R9 unsafe accounting.**
- **Raw-pointer discipline:** `&mut Page/Row/Cell/Screen/PageList` arguments re-read across integrity checks are Stacked-Borrows UB in this codebase. Use raw pointers where the reference uses raw pointers. Any change to `rustty-vt/src/search/*` or `pagelist/*` re-runs `.port/tools/miri-run.sh`.
- **DG5:** abort at the C-ABI boundary; follow the existing `cwrap`/`result.rs` pattern in `rustty-vt-c`.
- **Naming:** Ghostty→Rustty per token (`.port/tools/PARITY_NAMING.md`). C symbols stay `ghostty_*` and are aliased `rustty_*` in headers.
- **Every task states its NEGATIVE CONTROL** — the mutation that must make its new test fail — and the executor must actually run it once, not just describe it.
- **Commit granularity:** one commit per finding or per tight group of same-class findings; the commit message names the finding ids it closes (e.g. `closes S151-F1`).
- **Both platforms.** macOS is the dev host; the Linux lane (`ssh rvm`) is not optional — it caught four defects macOS structurally cannot compile. Run it per phase, not only at the end.

## Scope note

This plan spans 21 areas and would normally be split into sub-project specs. It is kept as one document because the ledger and its gate are shared, but **Phases 3 and 4 are per-area sub-plans**: each area's checklist produces working, tested software on its own and can be executed, reviewed and merged independently. Phases 0–2 are shared prerequisites and should land first, in order.

## Finding inventory (the thing being closed)

| Cut                                              |                                                                       Count |
| ------------------------------------------------ | --------------------------------------------------------------------------: |
| Surviving findings                               |                                                                     **497** |
| — CONFIRMED (neither lens refuted)               |                                                                         404 |
| — DISPUTED (one lens refuted → adjudicate first) |                                                                          93 |
| By severity                                      |                                              12 high · 140 medium · 345 low |
| Systemic classes                                 | LOG 86 · ABI 39 · ALLOC 32 · TEST 26 · DOC 9 · DEBUGONLY 8 · PRNG 1 (= 201) |
| Individual findings                              |                                                                         296 |

Largest areas: `src/terminal` 110, `src/apprt` 82, `src/font` 46, `src/cli` 34, `pkg` 33, `src/config` 33, `src/build` 30.

---

## Phase 0 — Closure infrastructure (do first; everything else reports into it)

### Task 1: The parity-closure gate

**Files:**

- Create: `.port/tools/gates/check_parity_closure.py`
- Modify: `.port/tools/ci.sh` (gate registry, next to the other `PROGRESS` entries)
- Data: `.port/parity-closure/LEDGER.tsv` (exists; 497 rows + header)

**Interfaces:**

- Consumes: `LEDGER.tsv` columns `id, status, severity, class, area, kind, oracle, rustty, title, owner, state`.
- Produces: exit 0 when every row is `CLOSED` or `WAIVED`; exit 1 while any row is `OPEN`; exit 3 (BLOCKED) if the ledger is missing or malformed. Later tasks close rows by editing the `state` column only.

**Why a gate and not a checklist:** a checklist can be declared done; this makes the count load-bearing. It also enforces that a `WAIVED` row carries a written reason, so "not a real defect" can never be a silent state change.

- [ ] **Step 1: Write the failing test (the gate's self-test arms)**

Create `.port/tools/gates/check_parity_closure.py` with a `--self-test` that drives the verdict function on canned ledgers:

```python
#!/usr/bin/env python3
"""G-PARITY — every audited parity finding is closed, waived-with-reason, or open.

WHY: docs/parity-audit/ found 497 surviving functional-parity findings. Without a
gate, "we fixed them all" is a claim; with one, it is a measurement. The ledger is
the single source of truth (.port/parity-closure/LEDGER.tsv) and this gate is the
only thing that reads its `state` column, so closing a row is a deliberate edit.

R5: this gate never lowers its own bar. A WAIVED row MUST name a reason file+line;
a row with state=WAIVED and no reason is RED, not green.
"""
import csv, os, sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
LEDGER = os.path.join(ROOT, "parity-closure", "LEDGER.tsv")
VALID = {"OPEN", "CLOSED", "WAIVED"}

def verdict(rows):
    """Return (rc, lines). rc 0 GREEN, 1 RED."""
    out, bad_state, waived_no_reason = [], [], []
    counts = {"OPEN": 0, "CLOSED": 0, "WAIVED": 0}
    for r in rows:
        st = (r.get("state") or "").strip()
        if st not in VALID:
            bad_state.append(f"{r.get('id')}: state={st!r}")
            continue
        counts[st] += 1
        if st == "WAIVED" and not (r.get("owner") or "").strip():
            waived_no_reason.append(r.get("id"))
    total = sum(counts.values())
    out.append(f"G-PARITY: {total} finding(s) — "
               f"{counts['CLOSED']} closed, {counts['WAIVED']} waived, {counts['OPEN']} open")
    rc = 0
    if bad_state:
        out.append(f"RED: {len(bad_state)} row(s) with an unknown state: {bad_state[:5]}")
        rc = 1
    if waived_no_reason:
        out.append("RED: WAIVED rows must put the justification (file:line or doc ref) in the")
        out.append(f"     `owner` column; missing on: {waived_no_reason[:5]}")
        rc = 1
    if counts["OPEN"]:
        out.append(f"RED: {counts['OPEN']} finding(s) still OPEN")
        rc = 1
    if rc == 0:
        out.append("GREEN: every audited parity finding is closed or waived-with-reason.")
    return rc, out

def load(path):
    with open(path, newline="") as fh:
        return list(csv.DictReader(fh, delimiter="\t"))

def self_test():
    print("=== G-PARITY SELF-TEST ===")
    green = [{"id": "S1-F1", "state": "CLOSED", "owner": ""},
             {"id": "S2-F1", "state": "WAIVED", "owner": "docs/parity-audit/src.md#s2-f1"}]
    red_open = green + [{"id": "S3-F1", "state": "OPEN", "owner": ""}]
    red_waive = [{"id": "S4-F1", "state": "WAIVED", "owner": ""}]
    red_bogus = [{"id": "S5-F1", "state": "DONE", "owner": ""}]
    rc_g, _ = verdict(green)
    rc_o, _ = verdict(red_open)
    rc_w, _ = verdict(red_waive)
    rc_b, _ = verdict(red_bogus)
    print(f"  all closed/waived -> {rc_g} (expect 0)")
    print(f"  one still OPEN    -> {rc_o} (expect 1)")
    print(f"  WAIVED, no reason -> {rc_w} (expect 1)")
    print(f"  unknown state     -> {rc_b} (expect 1)")
    ok = (rc_g, rc_o, rc_w, rc_b) == (0, 1, 1, 1)
    print("SELF-TEST GREEN: the gate flips RED on an open row, an unjustified waiver,"
          " and an unknown state." if ok else "SELF-TEST RED: the gate did not flip.")
    return 0 if ok else 1

if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    if not os.path.exists(LEDGER):
        print(f"BLOCKED: ledger not found at {LEDGER}", file=sys.stderr)
        sys.exit(3)
    rows = load(LEDGER)
    if not rows:
        print("BLOCKED: ledger has no rows — refusing to report a vacuous green", file=sys.stderr)
        sys.exit(3)
    rc, lines = verdict(rows)
    print("\n".join(lines))
    sys.exit(rc)
```

- [ ] **Step 2: Run the self-test to verify every arm flips**

Run: `python3 .port/tools/gates/check_parity_closure.py --self-test`
Expected: all four arms match, `SELF-TEST GREEN`, exit 0.

- [ ] **Step 3: Run the gate against the real ledger**

Run: `python3 .port/tools/gates/check_parity_closure.py`
Expected: `G-PARITY: 497 finding(s) — 0 closed, 0 waived, 497 open` then `RED: 497 finding(s) still OPEN`, exit 1. **This RED is the progress meter for the whole plan** — it is correct on day one.

- [ ] **Step 4: Register it in ci.sh as a PROGRESS gate**

In `.port/tools/ci.sh`, add next to the other PROGRESS entries in the gate registry:

```bash
  "G-PARITY-SELF|HARD|$PY $GATES/check_parity_closure.py --self-test"
  "G-PARITY|PROGRESS|$PY $GATES/check_parity_closure.py"
```

PROGRESS (not HARD) because it is RED by design until the last task lands; the self-test is HARD so the gate can never rot into a vacuous green. Flip `G-PARITY` to HARD in Task 41.

- [ ] **Step 5: Negative control**

Edit one ledger row's `state` to `CLOSED`, re-run the gate, confirm the open count drops by exactly 1; revert. Then set one row to `WAIVED` with an empty `owner`, confirm RED with the "must put the justification" message; revert.

- [ ] **Step 6: Commit**

```bash
git add .port/tools/gates/check_parity_closure.py .port/tools/ci.sh .port/parity-closure/
git commit -m "gate(G-PARITY): ledger-driven closure gate for the 497 parity findings

RED by design until the closure plan finishes: 497 open on landing. Self-test is
HARD and covers all three RED arms (open row, unjustified waiver, unknown state)
so the gate cannot rot into a vacuous green."
```

### Task 2: Adjudicate the 93 DISPUTED findings

**Files:**

- Modify: `.port/parity-closure/LEDGER.tsv` (`state`/`owner` columns for DISPUTED rows only)
- Create: `.port/parity-closure/ADJUDICATION.md`

**Interfaces:**

- Consumes: `LEDGER.tsv` rows where `status == DISPUTED`; each one's refuting reason is in its `docs/parity-audit/<area>.md` entry under `- **Disputed by:**`.
- Produces: every DISPUTED row becomes either `state=OPEN` (upheld — a real defect, flows into the phase for its severity) or `state=WAIVED` with `owner` pointing at its `ADJUDICATION.md` anchor.

**Why this is first:** a DISPUTED finding is one where an independent lens produced a concrete, cited argument that it is _not_ a defect. Fixing it blindly risks changing correct code to match a misreading. Adjudication is cheap (the argument is already written) and it right-sizes every later phase.

- [ ] **Step 1: List the DISPUTED rows grouped by area**

```bash
awk -F'\t' 'NR==1 || $2=="DISPUTED"' .port/parity-closure/LEDGER.tsv | column -t -s$'\t' | less
awk -F'\t' '$2=="DISPUTED"{print $5}' .port/parity-closure/LEDGER.tsv | sort | uniq -c | sort -rn
```

- [ ] **Step 2: For each row, read BOTH sides yourself and rule**

For each id, open its entry in `docs/parity-audit/<area>.md`, then read the oracle region and the rustty region **directly** — do not rule from the summary. Record the ruling in `ADJUDICATION.md`:

```markdown
### S140-F2 — UPHELD (real defect)

- Refuting lens said: "<verbatim reason from the audit entry>"
- Why that is wrong: <cite oracle file:line and rustty file:line you read>
- Disposition: state=OPEN, flows into Phase 3 (medium, src/terminal)

### S155-F1 — WAIVED (not a defect)

- Refuting lens said: "<verbatim reason>"
- Verified: <what you read that confirms the refutation>
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s155-f1
```

- [ ] **Step 3: Apply the rulings to the ledger**

```bash
python3 - <<'EOF'
import csv
p=".port/parity-closure/LEDGER.tsv"
rows=list(csv.DictReader(open(p), delimiter="\t"))
WAIVED={  # ids ruled not-a-defect in ADJUDICATION.md
}
for r in rows:
    if r["id"] in WAIVED:
        r["state"]="WAIVED"
        r["owner"]=f".port/parity-closure/ADJUDICATION.md#{r['id'].lower()}"
w=csv.DictWriter(open(p,"w",newline=""), fieldnames=rows[0].keys(), delimiter="\t")
w.writeheader(); w.writerows(rows)
EOF
```

- [ ] **Step 4: Verify the gate reflects the rulings**

Run: `python3 .port/tools/gates/check_parity_closure.py`
Expected: waived count equals the number of ids in `WAIVED`, open count drops by the same amount, still RED overall.

- [ ] **Step 5: Negative control**

Remove the `owner` value from one waived row; the gate must go RED naming that id. Restore.

- [ ] **Step 6: Commit**

```bash
git add .port/parity-closure/
git commit -m "parity-closure: adjudicate the 93 DISPUTED findings

Each ruling cites what was read on both sides. Upheld rows stay OPEN and flow
into their severity phase; waived rows point at their ADJUDICATION.md anchor,
which G-PARITY requires."
```

---

## Phase 1 — The 12 HIGH findings

These are user-visible correctness, ABI, or dead-feature defects. Each gets its own task, its own test, and its own commit. Two pairs collapse because they are the same defect seen from two shards.

### Task 3: `command_s` C-ABI layout (closes S1-F1, S103-F1)

**Files:**

- Modify: `crates/rustty-input/src/command.rs:46-58,130-149`
- Modify: `crates/rustty-config/src/value_types.rs:5943-5973`
- Test: `crates/rustty-input/src/command.rs` (unit tests at the bottom of the file)

**Interfaces:**

- Produces: `#[repr(C)] pub struct C { pub action_key: *const c_char, pub action: *const c_char, pub title: *const c_char, pub description: *const c_char }` — 4 pointers, 32 bytes. `RepeatableCommand` gains owned NUL-terminated storage that keeps those pointers alive.

**The defect:** the header declares `rustty_command_s` as four NUL-terminated `const char*` (32 bytes). `command::C` is a plain Rust struct of four `Vec<u8>` — 96 bytes, field order unspecified — and its pointer is handed to the macOS app, which iterates it with a 32-byte stride and calls `String(cString:)` on each field. That is a wild read on live config data.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn command_c_matches_the_header_abi() {
    use std::os::raw::c_char;
    // include/rustty-embed.h declares rustty_command_s as 4x const char*.
    assert_eq!(std::mem::size_of::<C>(), 4 * std::mem::size_of::<*const c_char>());
    assert_eq!(std::mem::align_of::<C>(), std::mem::align_of::<*const c_char>());
    assert_eq!(std::mem::offset_of!(C, action_key), 0);
    assert_eq!(std::mem::offset_of!(C, action), std::mem::size_of::<*const c_char>());
    assert_eq!(std::mem::offset_of!(C, title), 2 * std::mem::size_of::<*const c_char>());
    assert_eq!(std::mem::offset_of!(C, description), 3 * std::mem::size_of::<*const c_char>());
}

#[test]
fn command_c_fields_are_nul_terminated_and_readable() {
    let cmd = Command {
        action_key: b"key".to_vec(),
        action: b"act".to_vec(),
        title: b"ttl".to_vec(),
        description: b"desc".to_vec(),
    };
    let owned = CommandCStorage::new(&cmd);
    let c = owned.as_c();
    unsafe {
        assert_eq!(std::ffi::CStr::from_ptr(c.action_key).to_bytes(), b"key");
        assert_eq!(std::ffi::CStr::from_ptr(c.action).to_bytes(), b"act");
        assert_eq!(std::ffi::CStr::from_ptr(c.title).to_bytes(), b"ttl");
        assert_eq!(std::ffi::CStr::from_ptr(c.description).to_bytes(), b"desc");
    }
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p rustty-input command_c_ -- --nocapture`
Expected: FAIL — `CommandCStorage` does not exist, and the size assert reports 96 vs 32.

- [ ] **Step 3: Implement**

```rust
/// Owns the NUL-terminated buffers that `C`'s pointers borrow. The oracle
/// (src/input/command.zig:23-29) shares memory with the owning Command and
/// arena-allocates the action string; we keep the same lifetime relationship by
/// making the storage and the pointer struct rebuild together.
pub struct CommandCStorage {
    action_key: std::ffi::CString,
    action: std::ffi::CString,
    title: std::ffi::CString,
    description: std::ffi::CString,
}

impl CommandCStorage {
    pub fn new(cmd: &Command) -> Self {
        let cs = |v: &[u8]| std::ffi::CString::new(v.to_vec()).unwrap_or_default();
        Self {
            action_key: cs(&cmd.action_key),
            action: cs(&cmd.action),
            title: cs(&cmd.title),
            description: cs(&cmd.description),
        }
    }

    pub fn as_c(&self) -> C {
        C {
            action_key: self.action_key.as_ptr(),
            action: self.action.as_ptr(),
            title: self.title.as_ptr(),
            description: self.description.as_ptr(),
        }
    }
}

/// `rustty_command_s` — 4 NUL-terminated strings. MUST stay 32 bytes; see
/// include/rustty-embed.h and the A4 embed-ABI gate.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct C {
    pub action_key: *const std::os::raw::c_char,
    pub action: *const std::os::raw::c_char,
    pub title: *const std::os::raw::c_char,
    pub description: *const std::os::raw::c_char,
}

const _: () = assert!(
    std::mem::size_of::<C>() == 4 * std::mem::size_of::<*const std::os::raw::c_char>()
);
```

Then in `value_types.rs`, have `RepeatableCommand` hold `storage: Vec<CommandCStorage>` alongside `value_c: Vec<C>`, rebuilding both in the same function so a `C` can never outlive its storage.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p rustty-input command_c_ && cargo test -p rustty-config repeatable_command`
Expected: PASS.

- [ ] **Step 5: Verify the embed-ABI gates still agree**

Run: `bash .port/tools/gates/check_embed_h_abi.sh && bash .port/tools/gates/check_rustty_h_abi.sh`
Expected: PASS (A4/A3 green — this change makes Rust agree with the header they pin).

- [ ] **Step 6: Negative control**

Remove `#[repr(C)]` from `C`; `command_c_matches_the_header_abi` must fail on the size assert. Restore.

- [ ] **Step 7: Exercise the real consumer**

Run the macOS app's config test lane: `cd macos && xcodebuild test -scheme Rustty -destination 'platform=macOS' -only-testing:RusttyTests/ConfigTests 2>&1 | tail -5`
Expected: TEST SUCCEEDED — this is the path (`Rustty.Config.swift:714-722`) that read the struct with a 32-byte stride.

- [ ] **Step 8: Commit**

```bash
git add crates/rustty-input/src/command.rs crates/rustty-config/src/value_types.rs
git commit -m "fix(abi): command_s is 4x const char*, not 4x Vec (closes S1-F1, S103-F1)

The macOS command palette iterated rustty_command_s with a 32-byte stride over
96-byte Rust structs and called String(cString:) on each field — a wild read on
live config data. C is now #[repr(C)] with four NUL-terminated pointers whose
storage is owned alongside and rebuilt in the same motion."
```

Then set S1-F1 and S103-F1 to `CLOSED` in the ledger and re-run `check_parity_closure.py`.

### Task 4: kitty image format/compression enum width (closes S3-F1)

**Files:**

- Modify: `crates/rustty-vt-c/src/kitty_graphics.rs:540-542`
- Test: same file, `#[cfg(test)] mod tests`

**The defect:** the header says `rustty_kitty_graphics_image_get(..., RUSTTY_KITTY_IMAGE_DATA_FORMAT, out)` stores a 4-byte C enum. `ImageFormat`/`ImageCompression` are plain Rust enums (1 byte), so the write leaves the caller's upper 3 bytes untouched — garbage on any caller that did not zero the variable.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn image_get_writes_a_full_c_int_for_format_and_compression() {
    use std::os::raw::c_int;
    let t = crate::test_util::terminal_with_kitty_image();
    // Poison all four bytes: a 1-byte write leaves 0xFFFFFF in the top bytes.
    let mut out: c_int = -1;
    let ok = unsafe {
        rustty_kitty_graphics_image_get(
            t.image_ptr(),
            RUSTTY_KITTY_IMAGE_DATA_FORMAT,
            &mut out as *mut c_int as *mut std::ffi::c_void,
        )
    };
    assert!(ok);
    assert_eq!(out, RUSTTY_KITTY_IMAGE_FORMAT_RGBA as c_int, "upper bytes not written");

    let mut cout: c_int = -1;
    let ok = unsafe {
        rustty_kitty_graphics_image_get(
            t.image_ptr(),
            RUSTTY_KITTY_IMAGE_COMPRESSION,
            &mut cout as *mut c_int as *mut std::ffi::c_void,
        )
    };
    assert!(ok);
    assert_eq!(cout, RUSTTY_KITTY_IMAGE_COMPRESSION_NONE as c_int);
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p rustty-vt-c image_get_writes_a_full_c_int`
Expected: FAIL — `out` is `-256`-ish (low byte replaced, upper bytes still `0xFF`).

- [ ] **Step 3: Implement — convert at the boundary**

`rustty-vt` stays repr-free (it is not a C crate); the projection crate does the widening:

```rust
// The header declares these outputs as `enum : int` (4 bytes). rustty-vt's
// Format/Compression are plain Rust enums (1 byte), so write through c_int
// explicitly — a direct enum store would leave the caller's upper bytes intact.
RUSTTY_KITTY_IMAGE_DATA_FORMAT => {
    *(out as *mut c_int) = image.format as c_int;
}
RUSTTY_KITTY_IMAGE_COMPRESSION => {
    *(out as *mut c_int) = image.compression as c_int;
}
```

- [ ] **Step 4: Run the test**

Run: `cargo test -p rustty-vt-c image_get_writes_a_full_c_int`
Expected: PASS.

- [ ] **Step 5: Negative control**

Revert one arm to the 1-byte store; the test must fail on that field only. Restore.

- [ ] **Step 6: Commit and close S3-F1 in the ledger**

```bash
git add crates/rustty-vt-c/src/kitty_graphics.rs
git commit -m "fix(vt-c): widen kitty image format/compression to c_int (closes S3-F1)"
```

### Task 5: shadertoy std140 uniform layout (closes S117-F1)

**Files:**

- Modify: `crates/rustty-renderer/src/shadertoy.rs:63-101`
- Test: same file

**The defect:** the oracle's `Uniforms` is an extern struct with `align(16)` on every vec3/vec4/array member, giving the std140 layout the GLSL `Globals` block expects. The Rust port has natural alignment, so `channel_time` is at 28 instead of 32, `current_cursor` at 192 instead of 208, `palette` at 280 instead of 304, and the struct is 4472 bytes instead of 4496. It is memcpy'd straight to the GPU (`generic.rs:6668-6672`), so every custom shader reads shifted values.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn uniforms_match_the_std140_block_in_shadertoy_prefix_glsl() {
    use std::mem::{offset_of, size_of};
    assert_eq!(offset_of!(Uniforms, channel_time), 32);
    assert_eq!(offset_of!(Uniforms, current_cursor), 208);
    assert_eq!(offset_of!(Uniforms, palette), 304);
    assert_eq!(size_of::<Uniforms>(), 4496);
    assert_eq!(align_of::<Uniforms>(), 16);
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p rustty-renderer uniforms_match_the_std140_block`
Expected: FAIL — `28 != 32`.

- [ ] **Step 3: Implement with align-carrying newtypes**

```rust
/// std140 requires vec3/vec4 and array members on 16-byte boundaries. The oracle
/// spells this `align(16)` per member (src/renderer/shadertoy.zig:14-42); Rust has
/// no per-field align, so the alignment rides on the member types instead.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default)]
pub struct Vec4([f32; 4]);

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default)]
pub struct Vec3([f32; 3], f32); // trailing pad: std140 vec3 occupies 16 bytes
```

Retype the members that carry `align(16)` in the reference and add a compile-time pin so this can never drift silently:

```rust
const _: () = assert!(std::mem::offset_of!(Uniforms, channel_time) == 32);
const _: () = assert!(std::mem::offset_of!(Uniforms, current_cursor) == 208);
const _: () = assert!(std::mem::offset_of!(Uniforms, palette) == 304);
const _: () = assert!(std::mem::size_of::<Uniforms>() == 4496);
```

Keep every field **name** unchanged so `generic.rs`'s writes still compile.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p rustty-renderer uniforms_ && cargo build -p rustty-renderer`
Expected: PASS.

- [ ] **Step 5: Negative control**

Drop `align(16)` from `Vec4`; the const-assert must fail the build. Restore.

- [ ] **Step 6: Commit and close S117-F1**

### Task 6: IOSurface Display P3 colorspace (closes S118-F1)

**Files:**

- Modify: `crates/rustty-renderer/src/metal/target.rs:81-97`
- Modify: `crates/rustty-renderer/Cargo.toml` (add `objc2-core-graphics`)

**The defect:** the oracle tags the target IOSurface with Display P3 so CoreAnimation composites in P3; rustty's `Target::init` skips it behind an "R5 DEFERRED SEAM" comment, so every on-screen frame composites as sRGB — a user-visible color/blending difference on macOS.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
#[cfg(target_os = "macos")]
fn target_surface_is_tagged_display_p3() {
    let t = Target::init(&test_device(), 64, 64).expect("target");
    let value = unsafe { IOSurfaceCopyValue(t.surface(), kIOSurfaceColorSpace) };
    assert!(!value.is_null(), "IOSurface has no colorspace tag");
    let cs = unsafe { CGColorSpaceCreateWithPropertyList(value) };
    let name = unsafe { CGColorSpaceCopyName(cs) };
    assert_eq!(cfstring_to_string(name), "kCGColorSpaceDisplayP3");
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p rustty-renderer target_surface_is_tagged_display_p3`
Expected: FAIL — the value is null (no tag set).

- [ ] **Step 3: Implement**

In `Target::init`, after creating the surface, mirror `pkg/macos`'s `IOSurface.init` `setColorSpace`: create `CGColorSpace` named Display P3, take its property list, and `IOSurfaceSetValue(surface, kIOSurfaceColorSpace, plist)`. Replace the "R5 DEFERRED SEAM ... SKIPPED here" comment with a note naming the oracle site (`src/renderer/metal/Target.zig:45-60`).

- [ ] **Step 4: Run the test** — Expected: PASS.

- [ ] **Step 5: Negative control** — comment out the `IOSurfaceSetValue` call, watch the test fail, restore.

- [ ] **Step 6: Commit and close S118-F1**

### Task 7: `clipboard-codepoint-map` is silently inert (closes S151-F1)

**Files:**

- Modify: `crates/rustty-vt/src/formatter.rs:767,1001,1310` and `1240-1250`
- Test: `crates/rustty-vt/src/formatter.rs` tests

**The defect:** every formatter delegation hop builds the child with `copy_options`, which hard-codes `codepoint_map: Some(Vec::new())`. `Surface::copySelectionToClipboards` builds a `ScreenFormatter` with the user's `clipboard-codepoint-map`, so the config option does nothing. The oracle only tests the map at Page level, which is why no ported test caught it.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn codepoint_map_survives_the_screen_formatter_delegation() {
    let mut t = Terminal::init(testing_allocator(), 10, 3).unwrap();
    t.print_string("ab").unwrap();
    let mut opts = Options::default();
    // map 'a' (0x61) -> 'Z' (0x5A), the same shape config uses
    opts.codepoint_map = Some(vec![(0x61u32, 0x5Au32)]);
    let mut out = Vec::new();
    ScreenFormatter::init(t.screens.active(), opts).format(&mut out).unwrap();
    assert_eq!(String::from_utf8_lossy(&out).trim_end(), "Zb",
               "codepoint_map was dropped on the way down to PageFormatter");
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p rustty-vt codepoint_map_survives`
Expected: FAIL — output is `ab`.

- [ ] **Step 3: Implement**

Add a sibling to `copy_options` that preserves the map, and use it at the three **content** delegation sites (`:767`, `:1001`, `:1310`). Keep the map-less copy only for the extras passes, and update its doc comment to say exactly which passes may use it:

```rust
/// Options for a CONTENT delegation hop: the codepoint map must reach
/// PageFormatter, which is the only place it is applied (oracle
/// src/terminal/formatter.zig:486,691-698,899 pass `self.opts` by value).
/// `copy_options` below deliberately drops it and is for the extras passes ONLY.
fn clone_options(opts: &Options) -> Options {
    Options { codepoint_map: opts.codepoint_map.clone(), ..copy_options(opts) }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p rustty-vt formatter`
Expected: PASS, no regressions in the existing formatter tests.

- [ ] **Step 5: Negative control** — restore `copy_options` at `:1001`; the new test must fail. Restore the fix.

- [ ] **Step 6: Prove the production path**

Run: `cargo test -p rustty-core copy_selection`
Expected: PASS. Add an assertion there if no test covers the map end-to-end.

- [ ] **Step 7: Commit and close S151-F1**

### Task 8: search refresh timer fires once (closes S198-F1)

**Files:**

- Modify: `crates/rustty-vt/src/search/thread.rs:499-503,1386-1406`
- Test: `crates/rustty-vt/src/search/thread.rs` tests

**The defect:** `refresh_callback` re-arms the timer and returns `Disarm` (mirroring libxev, where `.disarm` only means "the loop will not re-add it"). rustty's dispatcher then calls `apply_action(Disarm)`, which clears `c.active` and `timer_armed` — undoing the re-arm. `start_refresh_timer` is called once, so the 24 ms refresh fires exactly once and search results go stale as content scrolls.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn refresh_timer_rearms_itself_every_interval() {
    let mut th = Thread::init_for_test();
    th.start_refresh_timer();
    for tick in 0..3 {
        th.run_one_timer_dispatch();
        assert!(th.shared.timer_armed.load(Ordering::SeqCst),
                "refresh timer disarmed after dispatch {tick}; it must re-arm like libxev");
    }
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p rustty-vt refresh_timer_rearms_itself`
Expected: FAIL at `tick 0` — `timer_armed` is false.

- [ ] **Step 3: Implement**

Make `apply_action` honour a re-arm performed inside the callback, mirroring libxev's semantics:

```rust
// libxev: `.disarm` means "the loop does not re-add this completion", NOT
// "cancel it". A callback that called Timer::run has already re-submitted, so a
// Disarm returned afterwards must not clear it (oracle
// src/terminal/search/Thread.zig:468-499 relies on exactly this).
let rearmed_before = self.refresh_c.submitted_generation;
let action = self.refresh_callback();
if !(matches!(action, CallbackAction::Disarm)
     && self.refresh_c.submitted_generation != rearmed_before)
{
    self.loop_.apply_action(&mut self.refresh_c, action);
}
```

with `Timer::run` bumping `submitted_generation`.

- [ ] **Step 4: Run the test** — Expected: PASS for all three ticks.

- [ ] **Step 5: Miri**

Run: `.port/tools/miri-run.sh rustty-vt search 6 45 15`
Expected: UB=0 (read the per-test outputs; the harness's `VERDICT: PROBLEM` banner counts documented ignores).

- [ ] **Step 6: Negative control** — remove the generation check; the test must fail at tick 0. Restore.

- [ ] **Step 7: Commit and close S198-F1**

### Task 9: `global.init` does not re-sync the environment (closes S43-F1)

**Files:**

- Modify: `crates/rustty-apprt/src/lib.rs:329-337`

**The defect:** the reference does `ensureLocale(); syncEnviron();` so the LANG/LC_* mutations are re-captured. rustty calls `ensureLocale()` and never re-syncs, so the process-global `ENVIRON` (already captured earlier in the same init) holds the pre-locale block and every `environ_map()` consumer — termio child processes, the crash env snapshot — sees the wrong locale.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn init_resyncs_environ_after_ensure_locale() {
    std::env::set_var("LANG", "");                 // force ensureLocale to correct it
    let _ = rustty_global::environ();              // capture the PRE-locale block, as init does
    init_with_action(Action::default()).unwrap();
    let lang = rustty_global::environ_map().get("LANG").cloned().unwrap_or_default();
    assert!(!lang.is_empty(), "environ still holds the pre-ensureLocale block");
}
```

- [ ] **Step 2: Run it and watch it fail** — Run: `cargo test -p rustty-apprt init_resyncs_environ`. Expected: FAIL, `LANG` empty.

- [ ] **Step 3: Implement** — add `rustty_global::sync_environ();` immediately after the `ensureLocale()` call, with a comment citing `src/global.zig:204-209`.

- [ ] **Step 4: Run the test** — Expected: PASS.

- [ ] **Step 5: Negative control** — remove the call, watch it fail, restore.

- [ ] **Step 6: Commit and close S43-F1**

### Task 10: x86 AVX2 detection drops the OS-support check (closes S34-F1)

**Files:**

- Modify: `crates-sys/highway-sys/cpp/detect.c:172-183`
- Test: `crates-sys/highway-sys/tests/detect.rs` (create if absent)

**The defect:** the oracle enables AVX2 only if XSAVE+AVX are present, `XGETBV(0) & 0x6 == 0x6` (the OS actually saves AVX state), and LZCNT+BMI+BMI2 are present. The C port checks only the CPUID feature bits — it never executes `xgetbv` and never checks LZCNT/BMI/BMI2. Highway's AVX2 codegen assumes all of them, so rustty can dispatch AVX2 kernels the oracle would refuse — on a kernel without AVX state saving that is a wrong-results/SIGILL class bug.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn avx2_requires_os_xsave_and_bmi() {
    // Drive the detection with synthetic CPUID/XCR0 inputs via the test hook.
    // AVX2+FMA+F16C+XSAVE+AVX present, but the OS does NOT save AVX state.
    let mask = unsafe { hwy_detect_targets_for_test(&CpuidFixture {
        leaf1_ecx: XSAVE | AVX | FMA | F16C, leaf7_ebx: AVX2 | BMI | BMI2,
        ext1_ecx: LZCNT, xcr0: 0x0,
    }) };
    assert_eq!(mask & TGT_AVX2, 0, "AVX2 enabled without OS xsave support");

    // Same, but BMI2 missing while the OS does save state.
    let mask = unsafe { hwy_detect_targets_for_test(&CpuidFixture {
        leaf1_ecx: XSAVE | AVX | FMA | F16C, leaf7_ebx: AVX2 | BMI,
        ext1_ecx: LZCNT, xcr0: 0x6,
    }) };
    assert_eq!(mask & TGT_AVX2, 0, "AVX2 enabled without BMI2");
}
```

- [ ] **Step 2: Run it and watch it fail** — Run: `cargo test -p highway-sys avx2_requires_os_xsave`. Expected: FAIL — AVX2 set in both masks.

- [ ] **Step 3: Implement** — port `pkg/highway/src/detect/x86.zig:63-94` literally: gate on `has_avx_save && max_leaf >= 7`, read XCR0 via `_xgetbv(0)`, require `(xcr0 & 0x6) == 0x6`, and require ext-leaf `0x80000001` ECX bit 5 (LZCNT) plus leaf7 EBX bits 3 (BMI) and 8 (BMI2) before setting `TGT_AVX2`. Add the seam `hwy_detect_targets_for_test` so the fixture above can drive it.

- [ ] **Step 4: Run the tests** — Expected: PASS, and the real `hwy_detect_targets()` on this host still reports AVX2 where it did before (print it).

- [ ] **Step 5: Negative control** — drop the `(xcr0 & 0x6)` check; the first assertion must fail. Restore.

- [ ] **Step 6: Commit and close S34-F1**

### Task 11: PowerPC / s390x / RISC-V target detection (closes S34-F4, S34-F5, S34-F6)

**Files:**

- Modify: `crates-sys/highway-sys/cpp/detect.c:188-190` (the `#else` stub)
- Test: `crates-sys/highway-sys/tests/detect.rs`

**The defect:** `detect.zig` routes powerpc → `ppc.detect()`, s390x → `s390x.detect()`, riscv → `riscv.detect()`. `detect.c`'s `#if` ladder has only aarch64 and x86, so all three fall to `return 0` and Highway dispatches the baseline target where the oracle dispatches PPC8/9/10, Z14/Z15, or RVV.

- [ ] **Step 1: Write the failing test** — assert each arch's `AT_HWCAP` fixture yields the expected bits: PPC8 `1<<49`, PPC9 `1<<48`, PPC10 `1<<47`, Z14 `1<<51`, Z15 `1<<50`, RVV `1<<37`.

- [ ] **Step 2: Run it and watch it fail** — Expected: FAIL, every mask 0.

- [ ] **Step 3: Implement** — add three branches to the ladder, each `#if defined(__linux__)`-gated as the oracle is, porting the exact HWCAP bit tests:
  - ppc: ALTIVEC `0x10000000` + VSX `0x80` + ARCH_2_07 `0x80000000` + VEC_CRYPTO `0x02000000` → PPC8; + ARCH_3_00 `0x00800000` → PPC9; + ARCH_3_1 `0x00040000` + MMA `0x00020000` → PPC10.
  - s390x: VX `1<<11` + VXE `1<<13` → Z14; + VXE2 `1<<15` → Z15.
  - riscv: HWCAP bit 21 (`'V'-'A'`) → RVV.

- [ ] **Step 4: Run the tests** — Expected: PASS. Cross-compile check: `cargo check -p highway-sys --target powerpc64le-unknown-linux-gnu` (add the target first).

- [ ] **Step 5: Negative control** — flip one HWCAP constant; its assertion must fail. Restore.

- [ ] **Step 6: Commit and close S34-F4/F5/F6**

### Task 12: Phase 1 checkpoint

- [ ] Run both lanes end to end: `bash .port/tools/ci.sh` on macOS and, after pushing to the VM, `ssh rvm 'cd ~/projects/rustty && bash .port/tools/ci.sh'`. (Push via `git push ssh://rvm/~/projects/rustty +HEAD:refs/heads/p6-incoming` then `git reset --hard p6-incoming` on the VM — pushing to its checked-out branch is refused.)
- [ ] Confirm `G-PARITY` open count dropped by exactly 12 and no other gate regressed.
- [ ] Commit the ledger state change.

---

## Phase 2 — Systemic classes (201 findings, 7 root causes)

Each class is one decision plus a mechanical application. Do the decision task first; it changes how many findings remain.

### Task 13: DECISION — the logging facade (86 findings, class `LOG`)

**This is an architecture decision, not a mechanical fix, and it must be made before any LOG row is touched.**

rustty-vt deliberately wires no logging facade, so all 86 findings are the same fact: reference `log.warn`/`log.err`/`log.info` diagnostics are carried as comments and nothing is emitted. Control flow is identical in every case; only the diagnostic is lost. They span 16 crates.

- [ ] **Step 1: Write the options up, with evidence**

Create `.port/parity-closure/DECISION-logging.md` covering:

- **Option A — adopt a facade.** Add `log` (or `tracing`) to `rustty-vt` and the other 15 crates, restore all 86 sites verbatim, route the C API's sites through the existing `sys::log_fn` sink so embedders receive them (S174-F2 and S178-F1/F2 already show the C-side sink exists and is bypassed). Cost: a dependency in the VT core, which was avoided on purpose; benefit: real parity, and the embedder-visible diagnostics start working.
- **Option B — facade in the C/app crates only.** Restore the sites that cross an API boundary (the `vt-c` and `apprt` ones, where an embedder or user can observe the loss) and waive the pure-internal ones with a written rationale.
- **Option C — waive the class.** Record all 86 as intentional divergence.
- Recommendation with a reason, and the count of findings each option closes vs waives.

- [ ] **Step 2: Get the decision ratified** (it changes a core crate's dependency graph — this is a human call, not an executor's).

- [ ] **Step 3: Record the ruling** in the decision doc and set the ledger `state`/`owner` for every LOG row accordingly.

- [ ] **Step 4: Verify** `check_parity_closure.py` reflects the new counts.

- [ ] **Step 5: Commit.**

### Task 14: Execute the logging ruling (however many LOG rows Option A/B leaves OPEN)

**Files:** driven by `.port/parity-closure/*.md`, class `LOG`.

- [ ] **Step 1: Enumerate the work**

```bash
awk -F'\t' '$4=="LOG" && $11=="OPEN"{print $1"\t"$5"\t"$8}' .port/parity-closure/LEDGER.tsv
```

- [ ] **Step 2: For the first site, write the failing test** — assert through a captured sink, e.g.

```rust
#[test]
fn integrity_failure_logs_the_cached_and_actual_counts() {
    let sink = CapturingSink::install();
    let mut list = PageList::init(testing_allocator(), 5, 5, 0).unwrap();
    corrupt_cached_row_count(&mut list);
    let _ = std::panic::catch_unwind(|| list.assert_integrity());
    assert!(sink.messages().iter().any(|m| m.contains("cached") && m.contains("actual")),
            "the page_list integrity diagnostic was not emitted");
}
```

- [ ] **Step 3: Run it and watch it fail** (no sink output).
- [ ] **Step 4: Restore the reference call verbatim** at that site — uncomment it, matching the reference's scope, level and format string exactly.
- [ ] **Step 5: Run the test.** PASS.
- [ ] **Step 6: Repeat for each remaining LOG site**, committing in per-crate groups. For sites where no observable sink exists, the test is the C-sink assertion (`sys::log_fn`) — if neither is possible, the row becomes `WAIVED` with the reason recorded, never silently closed.
- [ ] **Step 7: Negative control** — re-comment one restored site; its test must fail. Restore.
- [ ] **Step 8: Close the LOG rows in the ledger; commit.**

### Task 15: ABI class (39 findings)

Beyond the three high ABI defects already fixed, 36 rows are the same family: Rust types crossing the C boundary without `#[repr(C)]`, missing alignment, enum-width mismatches, or discriminants that are not pinned.

- [ ] **Step 1: Enumerate** — `awk -F'\t' '$4=="ABI" && $11=="OPEN"' .port/parity-closure/LEDGER.tsv`
- [ ] **Step 2: For each, add the layout assertion FIRST** (it is the failing test):

```rust
const _: () = assert!(std::mem::size_of::<T>() == EXPECTED_FROM_HEADER);
const _: () = assert!(std::mem::offset_of!(T, field) == EXPECTED);
```

- [ ] **Step 3: Watch the build fail**, then fix the type (add `#[repr(C)]`/`align`, or widen at the boundary as in Task 4).
- [ ] **Step 4: Re-run the ABI gates** after each group: `bash .port/tools/gates/check_rustty_h_abi.sh`, `check_embed_h_abi.sh`, `sh .port/tools/gates/check_rustty_vt_headers.sh`, `node .port/tools/check-vt-enum-discriminants.mjs`, `node .port/tools/check-vt-struct-layouts.mjs`. All must stay GREEN.
- [ ] **Step 5: Negative control** — perturb one offset assert, confirm the build breaks. Restore.
- [ ] **Step 6: Commit per crate; close the rows.**

### Task 16: ALLOC class (32 findings)

The pattern: rustty aborts (or `unwrap`s) where the reference returns `error.OutOfMemory` and degrades. These are reachable only under allocation failure, so the test vehicle is the existing tripwire/failing-allocator harness.

- [ ] **Step 1: Enumerate** — `awk -F'\t' '$4=="ALLOC" && $11=="OPEN"' .port/parity-closure/LEDGER.tsv`
- [ ] **Step 2: For the first row, write the failing test** using the established harness:

```rust
#[test]
fn decode_returns_out_of_memory_instead_of_aborting() {
    let tw = Tripwire::armed(SnapshotFp::Alloc, 0);
    let err = decode(&mut reader, tw.allocator()).unwrap_err();
    assert!(matches!(err, Error::OutOfMemory), "expected OOM, got {err:?}");
    tw.end(ResetMode::Reset).unwrap();   // proves the fail point actually fired
}
```

- [ ] **Step 3: Run it and watch it abort/panic** rather than return.
- [ ] **Step 4: Thread the error** — return `Result` from the site up to the nearest boundary that the reference also fails at.
- [ ] **Step 5: Run the test.** PASS, with `tw.end()` proving the injection fired (this is what stops a vacuous pass).
- [ ] **Step 6: Repeat; commit per subsystem; close the rows.**

### Task 17: TEST class (26) and DEBUGONLY class (8)

Missing tests, weakened assertions, and tests that exist but are `#[ignore]`d or `cfg(debug_assertions)`-gated so they never run in the release lane.

- [ ] **Step 1: Enumerate both classes** from the ledger.
- [ ] **Step 2: For each weakened-assertion row**, restore the reference's assertions one-for-one, then **verify the restored assertion has power**: mutate the code under test so the new assertion fails, and only then revert the mutation. An assertion that cannot fail is not a fix.
- [ ] **Step 3: For each debug-only row**, either make the test run in release (preferred — most are gated only because a debug-only helper is used) or record why it cannot, in the row's `owner`.
- [ ] **Step 4: Confirm the counts move** — `python3 .port/tools/gates/check_test_counts.py` and `python3 .port/tools/audit_test_coverage.py` must both stay GREEN, and the per-module rust counts should rise.
- [ ] **Step 5: Commit; close the rows.**

### Task 18: DOC class (9) and PRNG class (1)

- [ ] **Step 1:** For each DOC row, correct the stale PORT NOTE / doc comment, **quoting the superseded text in the commit message** so the change is legible (the `bb4ca341` precedent).
- [ ] **Step 2:** For the PRNG row, switch the test's generator to match the reference's `std.Random.DefaultPrng` sequence, or document why an equivalent-distribution generator is acceptable and what that costs in reproducibility against the oracle.
- [ ] **Step 3:** `python3 .port/tools/gates/check_comment_coverage.py` must stay GREEN.
- [ ] **Step 4:** Commit; close the rows.

### Task 19: Phase 2 checkpoint

- [ ] Full `ci.sh` on both lanes; no gate regressions.
- [ ] `G-PARITY` open count matches expectation (497 − closed − waived).
- [ ] Commit the ledger.

---

## Phase 3 — Medium tail, per area (the `INDIV` medium rows)

One task per area, largest first. **Each is an independently shippable sub-plan.** They share one procedure, so it is written once here and referenced by each task; the per-row specifics live in the checklist and audit files, which carry oracle file:line, rustty file:line, description, evidence and a suggested fix for every row.

**The per-row procedure (apply to every row in the area's checklist):**

1. Open the row's entry in `docs/parity-audit/<area>.md` and read the **oracle** region and the **rustty** region directly. Do not fix from the summary.
2. Write the failing test first, at the smallest scope that observes the difference. If the reference has a test for this behaviour, port that test verbatim (same name, same assertions) rather than inventing one.
3. Run it; confirm it fails **for the stated reason**. A test that fails for a different reason is not yet the right test.
4. Make the minimal change.
5. Run the test; run the crate's suite.
6. Negative control: revert the fix, confirm the test fails, re-apply.
7. Tick the checklist box, set the ledger row to `CLOSED`, commit naming the id.

If a row turns out not to be a defect, set it `WAIVED` with the reason in `owner` — never tick it silently.

- [ ] **Task 20:** `src/terminal` — 36 medium. Checklist: `.port/parity-closure/src-terminal.md`. Re-run `.port/tools/miri-run.sh rustty-vt` for any `pagelist`/`search`/`page` change.
- [ ] **Task 21:** `src/apprt` — 16 medium. Checklist: `.port/parity-closure/src-apprt.md`. GTK rows compile only on Linux: verify on `ssh rvm`.
- [ ] **Task 22:** `src/build` — 16 medium. Checklist: `.port/parity-closure/src-build.md`. Mostly xtask/packaging parity; verify with `G-DIST`, `G-XTASK`, `G-FEATMATRIX`.
- [ ] **Task 23:** `src/cli` — 13 medium. Checklist: `.port/parity-closure/src-cli.md`.
- [ ] **Task 24:** `src/font` — 8 medium. Checklist: `.port/parity-closure/src-font.md`. Re-run `G-D10` (stb resize parity) for any resize/bitmap change.
- [ ] **Task 25:** `src/config` — 7 medium. Checklist: `.port/parity-closure/src-config.md`. `G9` (full surface coverage) must stay GREEN.
- [ ] **Task 26:** `src` and `pkg` — 7 medium each. Checklists: `.port/parity-closure/src.md`, `pkg.md`.
- [ ] **Task 27:** `src/benchmark` 5, `macos` 4, `include` 4, `src/termio` 4 medium. macOS rows re-run the xcodebuild lane.
- [ ] **Task 28:** `src/lib` 3, `src/renderer` 3, `src/os` 2, `src/input` 2, `src/inspector` 1, `src/simd` 1, `src/stb` 1 medium.
- [ ] **Task 29: Phase 3 checkpoint** — both lanes green; `G-PARITY` open count reduced by exactly the medium total; commit the ledger.

---

## Phase 4 — Low tail, per area (345 rows)

Same per-row procedure as Phase 3. The low tier is dominated by the LOG class (already handled in Task 14) plus stale comments, cosmetic divergences, and test-name mismatches — so start each area by re-checking how many of its low rows are still `OPEN` after Phase 2.

- [ ] **Task 30:** Re-count per area — `awk -F'\t' '$3=="low" && $11=="OPEN"{print $5}' .port/parity-closure/LEDGER.tsv | sort | uniq -c | sort -rn`
- [ ] **Task 31:** `src/terminal` low. **Task 32:** `src/apprt` low. **Task 33:** `src/font` low. **Task 34:** `src/config` low. **Task 35:** `pkg` low. **Task 36:** `src/cli` low. **Task 37:** `src/build`, `src/os`, `src/lib` low. **Task 38:** `src`, `src/renderer`, `src/termio`, `include`, `src/input` low. **Task 39:** `macos`, `src/datastruct`, `src/inspector`, `src/benchmark`, `src/simd`, `src/stb`, `src/unicode` low.
- [ ] **Task 40: Phase 4 checkpoint** — both lanes green; every ledger row now `CLOSED` or `WAIVED`.

---

## Phase 5 — Finale

### Task 41: Prove closure and make the gate binding

- [ ] **Step 1: The closure gate goes GREEN**

Run: `python3 .port/tools/gates/check_parity_closure.py`
Expected: `GREEN: every audited parity finding is closed or waived-with-reason.`

- [ ] **Step 2: Promote it to HARD**

In `.port/tools/ci.sh` change `"G-PARITY|PROGRESS|..."` to `"G-PARITY|HARD|..."`. From here on, a reopened finding fails CI.

- [ ] **Step 3: Re-audit to catch anything the fixes introduced**

Re-run the parity audit workflow over the areas that changed most (`src/terminal`, `src/apprt`, `pkg`, `include`), using the same shard manifest so results are comparable. Any NEW finding becomes a ledger row — the plan is not done while the audit still finds defects.

- [ ] **Step 4: Full gates, both platforms**

macOS: `bash .port/tools/ci.sh` (expect the documented `G-KAITAI` BLOCKED and the `rustty-core` Miri intractability; everything else GREEN) plus the xcodebuild lane.
Linux: `ssh rvm 'cd ~/projects/rustty && bash .port/tools/ci.sh'` — `cargo test --workspace`, `G-GTKFEAT`, `G-REL`, `G-EXAMPLES` all GREEN.

- [ ] **Step 5: Write the closure record**

Create `.port/parity-closure/README.md`: what was closed, what was waived and why, the before/after gate states, and the re-audit result. Update `docs/parity-audit/README.md` with a header line pointing at it so the audit and its closure travel together.

- [ ] **Step 6: Commit**

```bash
git add .port/parity-closure/ .port/tools/ci.sh docs/parity-audit/README.md
git commit -m "parity-closure: all 497 audited findings closed or waived; G-PARITY is now HARD"
```

---

## Self-Review

**1. Spec coverage.** Every one of the 497 ledger rows is assigned: 12 high → Tasks 3–11 (individually, with S1-F1/S103-F1 and S34-F4/F5/F6 collapsed as the same defects); 201 systemic-class rows → Tasks 13–18; 93 DISPUTED → adjudicated in Task 2 before any of them is touched; the remaining `INDIV` medium → Tasks 20–28; `INDIV` low → Tasks 31–39. Task 1's gate is the mechanical check that nothing was missed, and Task 41 Step 3 re-audits rather than trusting the count.

**2. Placeholder scan.** No "TBD"/"handle edge cases"/"similar to Task N". The three places that deliberately do not inline code are Tasks 20–39, and that is a stated design choice, not an omission: each row's exact locations, description and suggested fix are already written in `docs/parity-audit/<area>.md` and `.port/parity-closure/<area>.md`, and the per-row procedure that consumes them is spelled out once in Phase 3 with a worked example in every class task (14, 15, 16, 17). Duplicating 345 code blocks here would go stale against the audit, which is the source of truth.

**3. Type consistency.** `CommandCStorage::new`/`as_c` and the `#[repr(C)] C` fields in Task 3 match the `value_types.rs` usage described there. `clone_options`/`copy_options` in Task 7 keep the existing `copy_options` name for the extras path. `hwy_detect_targets_for_test`/`CpuidFixture` are introduced in Task 10 and reused by Task 11. `check_parity_closure.py`'s `verdict(rows)` signature and the `state`/`owner` columns are used identically in Tasks 1, 2, 13, 41.

**4. Known risk.** Task 13 is a genuine architecture decision (a dependency in the VT core) and is deliberately blocking: 86 findings — 17% of the total — cannot be honestly closed until it is ruled on.
