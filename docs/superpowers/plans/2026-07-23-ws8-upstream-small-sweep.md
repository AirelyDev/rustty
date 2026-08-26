# WS8 — Upstream Small-Fix Sweep Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the BL-2, BL-3, BL-6, BL-7 backlog buckets from `.port/phase-2/MAIN_PARITY.md` (free-list page memory return-to-OS, ~29 pending-small runtime-safety fixes, kitty/gfx generation stamps, the OSC 72 kitty drag-and-drop parser) and complete the `91f0cf67d` tabstops-on-resize PARTIAL row, striking every landed hash in the ledger.

**Architecture:** Each upstream commit is a small, focused Zig diff against pin-era ported files. WS8 ports them one at a time, assertion-for-assertion with their test hunks, grouped into task-groups by rustty subsystem (title/pwd/hyperlink aliasing in `terminal.rs`/`screen.rs`; selection geometry in `selection.rs`/`selection_gesture.rs`; PageList iteration/pin/scroll in `pagelist/*.rs`; search fixes in `search/*.rs`; pool/capacity invariants in `pagelist/grow.rs`+`page.rs`; the BL-2 madvise decommit path; the BL-6 kitty/gfx generation-stamp refactor; the BL-7 new OSC 72 parser). No commit is force-fit: any hunk without a portable Rust seam is recorded as debt with a one-line reason, never silently dropped.

> **EXECUTION CORRECTIONS (recorded WS8 T12/T13, evidence-based — see the commit
> bodies):**
>
> 1. The Tech Stack line below says "the `libc` crate is deliberately NOT a
>    dependency". `libc` IS already a `rustty-vt` dependency (for the POSIX
>    shared-memory medium in `kitty/graphics_image.rs`, and dev-only in the bench
>    harness). The binding constraint honored by T12 is the narrower and correct
>    one from the Task 12 body: `madvise` is HAND-ROLLED in the style of
>    `page.rs`'s `mmap`/`munmap`, and WS8 added no new dependency anywhere.
> 2. Task 13's `#[ignore]`d-test RSS probe is UNWORKABLE and was replaced by a
>    standalone example (`crates/rustty-vt/examples/rss_probe.rs`). A libtest
>    build always has `cfg(test)` true, so `decommit_pool_item` compiles its
>    memset arm — and the memset COMMITS the pages: the probe was run as written
>    and recorded RSS going UP (grow 8.8 MiB -> clear 17.4 MiB). macOS also needs
>    `ri_phys_footprint` (`proc_pid_rusage`) rather than `ps -o rss`, because
>    `MADV_FREE_REUSABLE` moves the footprint, not the RSS.
> 3. Task 18's expected `rustty-vt --lib` count of 2069 is stale: the branch's
>    real baseline at `19d8a10` is 2058 (not 2035), and WS8 added 38 vt lib tests
>    (more oracle tests were ported than the plan enumerated), giving **2096**.

**Tech Stack:** Rust 1.93.1 (pinned), oracle git repo at `/Users/gael/projects/ghostty` (origin/main, fetched — the local pin `4749c4e` predates all WS8 commits), hand-rolled `extern "C"` `madvise` bindings for BL-2 in the style of `page.rs`'s `mmap`/`munmap` (the `libc` crate is deliberately NOT a dependency — see Task 12), existing `.port/tools/miri-run.sh` runner.

## Global Constraints (inherited from 2026-07-23-rustty-completion-ROADMAP.md)

- R1 full fidelity vs the ghostty oracle at `../ghostty`; the oracle is reference-only — no build-time dependency on it (R3).
- R2 product identity: rustty naming with per-token casing preservation (Ghostty→Rustty, ghostty→rustty). No `ghostty_*` C-ABI forwarders (deleted upstream at `e4cc52d`; see WS5 plan note) — WS8 touches no C-ABI surface except BL-6's kitty/gfx C API, which stays `rustty_*`-only.
- R4/R5: acyclic crate graph; gates (`check_acyclic.sh`, A4 ABI gate, Miri lanes) are invariant — never weakened to pass.
- DG5: C-ABI panic policy is ABORT — no unwind across the ABI.
- Toolchain: pinned Rust 1.93.1 (`rust-toolchain.toml`). LTO stays banned.
- Ledger discipline: strike every landed WS8 hash in `.port/phase-2/MAIN_PARITY.md` in the same PR; update `DEFERRALS.md`/`VERIFICATION_DEBT.md` if they carry stale WS8-adjacent rows.
- Commit style: `subsystem: summary (WS8 T<n>)`. Branch per WS; merge to main only with both platforms green.
- Zig byte-slices → `Vec<u8>`/`&[u8]`, never lossy `String` (memory: zig-byteslice-to-rust-rawbytes).
- Raw-pointer discipline for C-ABI args (memory: rust-mut-args-strongly-protected): out-params are `*mut T`, never `&mut T` re-read across calls.
- `&mut Page`/`&mut Row`/`&mut [Cell]` args re-read across `assert_integrity` is Stacked-Borrows UB — use raw-ptr args (memory: rust-mut-args-strongly-protected). Applies directly to several BL-3 `screen.rs`/`pagelist` fixes below.
- Known pre-existing Miri RED: the vt search subsystem has documented Stacked-Borrows UB (memory: search-screen-sb-ub-finding) — search objects hold raw `Screen` aliases invalidated by `&mut Screen`; G6 vt Miri lane stays RED for that reason, NOT fixed by WS8. The two BL-3 search fixes (T-Search-1/2 below) are logic-only ports that do not touch the aliasing pattern; they must not be blocked on, or mistaken for, a fix of that UB.
- When porting oracle doc prose, strip snark/gag editorializing (memory: strip-ghostty-gag-prose) — none of the WS8 commit messages need this, they're dry.

## Oracle Access (READ THIS FIRST)

The local ghostty checkout `/Users/gael/projects/ghostty` has its working tree pinned at `4749c4e93`, older than every WS8 commit. All WS8 commits live on `origin/main` (fetched at plan-authoring time, head `e6e26e165`). Do NOT move the pinned working tree.

- Read a commit's diff: `git -C /Users/gael/projects/ghostty show <hash>`
- Read a touched file's CURRENT (post-commit) state: `git -C /Users/gael/projects/ghostty show origin/main:<path>` — do NOT read the working-tree file (it's the old pin).
- Scope discipline: `origin/main` also contains the unrelated BL-1 compression series and BL-4 perf series and BL-8 GTK-tier commits. Port ONLY the hunks belonging to the hashes named in each task below.

---

## File Structure

- `crates/rustty-vt/src/terminal.rs` — modify: `set_title`/`set_pwd` aliasing (T-Alias-1/2), title/pwd NUL-terminator reservation (T-Alias-3/4), print grapheme boundary handling (T-Resize-2)
- `crates/rustty-vt/src/screen.rs` — modify: `start_hyperlink_once` aliasing (T-Alias-5), implicit hyperlink id wraparound (T-Alias-6), `select()` aliasing (T-Sel-1), selection clone page-width clamp (T-Sel-3), `select_line` destination width (T-Sel-4), `clear_rows`/`clear_cells` page-width + style-ref batching (T-Resize-1, T-Pool-3), resize cursor cells previous-width fix (T-Resize-3)
- `crates/rustty-vt/src/selection.rs` — modify: mirrored-corner clamp (T-Sel-2), selection-row page-width clamp (T-Sel-5)
- `crates/rustty-vt/src/selection_gesture.rs` — modify: drag geometry bound (T-Sel-6)
- `crates/rustty-vt/src/pagelist/iter.rs` — modify: count-limited `PageIterator` traversal fix (T-Page-1)
- `crates/rustty-vt/src/pagelist/pin.rs` — modify: width-aware pin traversal (T-Page-2), unrepresentable pin coordinate rejection (T-Page-3), pin wrapping at row boundaries (T-Page-5)
- `crates/rustty-vt/src/pagelist/mod.rs` (or wherever cell/screen coordinate types live — located during T-Page-4) — modify: widen cell screen coordinates
- `crates/rustty-vt/src/pagelist/scroll.rs` — modify: minimum prompt/row scroll delta (T-Page-6/7)
- `crates/rustty-vt/src/pagelist/grow.rs` — modify: `increase_capacity` zero-capacity growth fix (T-Pool-2)
- `crates/rustty-vt/src/pagelist/mod.rs`, `crates/rustty-vt/src/page.rs`, `crates/rustty-vt/src/screen.rs` — modify: pool reuse zeroing-invariant comments/asserts (T-Pool-1)
- `crates/rustty-vt/src/search/{active,sliding_window,viewport}.rs` — modify: ignore empty search needles (T-Search-1)
- `crates/rustty-vt/src/search/screen.rs` — modify: reset cached results after resize (T-Search-2)
- `crates/rustty-bench/` (exact file located during T-Bench-1) — modify: terminal-stream 64KiB read chunking
- `crates/rustty-vt-c/src/` (render seam file located during T-Grapheme-1) — modify: UTF-8 grapheme length overflow fix
- `crates/rustty-vt/src/bitmap_allocator.rs` or `pagelist/*` free-list module (located during T-Madvise-1) — modify: BL-2 return free-listed page memory to the OS
- `crates/rustty-vt/src/kitty/*.rs` (all `transmit_time` sites, located during T-Gfx-1) — modify: BL-6 generation-stamp refactor
- `crates/rustty-vt-c/src/` kitty graphics C API (located during T-Gfx-1) — modify: generation-stamp C projection
- `crates/rustty-vt/src/osc/mod.rs` — modify: `State::S72`, `Command::KittyDndProtocol`, `CommandKey::KittyDndProtocol`, dispatch wiring (T-Dnd-2)
- `crates/rustty-vt/src/osc/parsers/mod.rs` — modify: register `pub mod kitty_dnd_protocol;` (T-Dnd-2)
- `crates/rustty-vt/src/osc/parsers/kitty_dnd_protocol.rs` — create: the OSC 72 parser (T-Dnd-1)
- `crates/rustty-vt/src/stream.rs` — modify: add `Command::KittyDndProtocol` to the unimplemented-OSC dispatch arm (T-Dnd-3)
- `crates/rustty-vt/src/terminal.rs` (tripwire module, located during T-Tabstops-1) — modify or confirm-as-debt: extend failure injection to `Tabstops::init`-equivalent allocation, or record why not portable
- `.port/phase-2/MAIN_PARITY.md` — modify: strike every landed hash
- `.port/audits/GHOSTTY_LIFETIMES.tsv` — modify if BL-2/BL-6 introduce new raw-pointer-holding structs

---

### Task 1: Branch + oracle verification

**Files:** none (setup)

- [ ] **Step 1: Create the branch**

```bash
cd /Users/gael/projects/rustty && git checkout main && git pull && git checkout -b ws8-upstream-small-sweep
```

- [ ] **Step 2: Verify the oracle has every WS8 commit on origin/main**

```bash
cd /Users/gael/projects/ghostty
for h in 9b466e9c5 cfce1cd56 dace6d1c6 a14a619ce 0c299000f fdd255c78 1d5b6f90e 04810273a b287f6d1a e727a3658 b6f34be44 607160657 08e376f23 a55850c98 a9f5b7eba fa8cae88b d6e24d985 30b42f42a e6e4a9fdc afbf5ba15 c753fe4a4 0ff4e41b2 5bc6588e4 627518447 c442eb4b3 8307349ec 8d663a76e 4c5b1d5d5 aea63d71f 896aca499 bdc0b6c19 8fe5913f7 91c87e2cf dd6c09bcf a6132c18b c488ccda6 91f0cf67d; do
  git merge-base --is-ancestor $h origin/main && echo "$h OK" || echo "$h MISSING"
done
```

Expected: 36 × `OK`. If any `MISSING`: `git -C /Users/gael/projects/ghostty fetch origin` and re-check.

- [ ] **Step 3: Baseline sanity build**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib 2>&1 | tail -5
```

Expected: green, 2035 passed (the T19 baseline recorded in MAIN_PARITY.md §1).

---

## BL-7 — OSC 72 kitty drag-and-drop parser (`91c87e2cf` + `dd6c09bcf` + `a6132c18b` + `c488ccda6`)

New parser, structurally identical to the already-ported `kitty_clipboard_protocol.rs` (OSC 5522). No chunking reassembly or action semantics — this is parse-only, matching upstream scope exactly (`stream.zig`/`stream.rs` logs it as unimplemented).

### Task 2: `kitty_dnd_protocol` parser module (`91c87e2cf`, cleaned up per `dd6c09bcf`/`a6132c18b`/`c488ccda6`)

**Files:**

- Create: `crates/rustty-vt/src/osc/parsers/kitty_dnd_protocol.rs`
- Test: same file, `#[cfg(test)] mod tests`

**Interfaces:**

- Consumes: `crate::osc::{Command, Parser, State, Terminator}` (same imports as `kitty_clipboard_protocol.rs:19`).
- Produces: `pub struct OSC { metadata: Vec<u8>, payload: Option<Vec<u8>>, terminator: Terminator }`, `pub enum EventType`, `pub enum Option_`, `pub enum OptionValue`, `pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<&'a mut Command>` — consumed by Task 3 (`osc/mod.rs` dispatch) and Task 4 (`stream.rs`).

- [ ] **Step 1: Read the 4 oracle commits (already fetched — final combined state is what matters)**

```bash
git -C /Users/gael/projects/ghostty show 91c87e2cf -- src/terminal/osc/parsers/kitty_dnd_protocol.zig
git -C /Users/gael/projects/ghostty show origin/main:src/terminal/osc/parsers/kitty_dnd_protocol.zig
```

The second command is the FINAL post-`c488ccda6` state (192 lines) — port from this, not the intermediate diffs. It has the corrected doc comments (`i` = multiplexer session ID everywhere, not "drop ID"; `y` doubles as a 1-based MIME index for `request_data`; `X`/`Y` are pixel offsets / image dims / directory handles depending on event).

- [ ] **Step 2: Write the failing tests** — port all 11 tests from `dd6c09bcf`'s test hunk (quoted in full below), following `kitty_clipboard_protocol.rs`'s test-module conventions exactly (`Parser::init(None)`, feed bytes with `p.next(ch)`, `p.end(Some(0x1b))`):

```rust
#[cfg(test)]
mod tests {
    use super::{EventType, Option_};
    use crate::osc::{Command, Parser, Terminator};

    #[test]
    fn osc_72_metadata_only_no_payload() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=a";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(b"t=a".as_slice(), dnd.metadata.as_slice());
        assert!(dnd.payload.is_none());
    }

    #[test]
    fn osc_72_metadata_and_empty_payload() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=a;";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(b"t=a".as_slice(), dnd.metadata.as_slice());
        assert_eq!(b"".as_slice(), dnd.payload.as_ref().unwrap().as_slice());
    }

    #[test]
    fn osc_72_metadata_and_non_empty_payload() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=a:i=5;text/plain text/uri-list";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(b"t=a:i=5".as_slice(), dnd.metadata.as_slice());
        assert_eq!(
            b"text/plain text/uri-list".as_slice(),
            dnd.payload.as_ref().unwrap().as_slice()
        );
    }

    #[test]
    fn osc_72_read_option_t_valid_event_types() {
        let cases: &[(&str, EventType)] = &[
            ("72;t=a", EventType::AcceptDrops),
            ("72;t=A", EventType::StopAcceptingDrops),
            ("72;t=m", EventType::DropMove),
            ("72;t=M", EventType::DropDropped),
            ("72;t=r", EventType::RequestData),
            ("72;t=R", EventType::RequestError),
            ("72;t=o", EventType::OfferDrag),
            ("72;t=p", EventType::PresentData),
            ("72;t=P", EventType::ChangeDragImage),
            ("72;t=e", EventType::DragOfferEvent),
            ("72;t=E", EventType::DragOfferError),
            ("72;t=k", EventType::UriListData),
            ("72;t=q", EventType::Query),
        ];
        for (input, expected) in cases {
            let mut p: Parser = Parser::init(None);
            for ch in input.bytes() {
                p.next(ch);
            }
            let cmd = p.end(Some(0x1b)).unwrap().clone();
            let dnd = match cmd {
                Command::KittyDndProtocol(dnd) => dnd,
                _ => unreachable!(),
            };
            assert_eq!(*expected, dnd.read_option(Option_::T).as_type().unwrap());
        }
    }

    #[test]
    fn osc_72_read_option_t_unknown_value_returns_null() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=z";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert!(dnd.read_option(Option_::T).is_null());
    }

    #[test]
    fn osc_72_read_option_integer_keys() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=m:i=3:x=10:y=5:X=320:Y=200:o=1:m=0";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(3, dnd.read_option(Option_::I).as_int().unwrap());
        assert_eq!(10, dnd.read_option(Option_::X).as_int().unwrap());
        assert_eq!(5, dnd.read_option(Option_::Y).as_int().unwrap());
        assert_eq!(320, dnd.read_option(Option_::CapX).as_int().unwrap());
        assert_eq!(200, dnd.read_option(Option_::CapY).as_int().unwrap());
        assert_eq!(1, dnd.read_option(Option_::O).as_int().unwrap());
        assert_eq!(0, dnd.read_option(Option_::M).as_int().unwrap());
    }

    #[test]
    fn osc_72_read_option_negative_sentinel() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=m:x=-1:y=-1";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(-1, dnd.read_option(Option_::X).as_int().unwrap());
        assert_eq!(-1, dnd.read_option(Option_::Y).as_int().unwrap());
    }

    #[test]
    fn osc_72_read_option_case_sensitive_key_matching() {
        let mut p: Parser = Parser::init(None);
        // x=10 must not be returned when asking for CapX (the "X" key)
        let input = "72;x=10:Y=200";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(10, dnd.read_option(Option_::X).as_int().unwrap());
        assert!(dnd.read_option(Option_::CapX).is_null());
        assert_eq!(200, dnd.read_option(Option_::CapY).as_int().unwrap());
        assert!(dnd.read_option(Option_::Y).is_null());
    }

    #[test]
    fn osc_72_read_option_absent_key_returns_null() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=a";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert!(dnd.read_option(Option_::I).is_null());
        assert!(dnd.read_option(Option_::X).is_null());
        assert!(dnd.read_option(Option_::CapX).is_null());
        assert!(dnd.read_option(Option_::M).is_null());
    }

    #[test]
    fn osc_72_read_option_malformed_integer_returns_null() {
        let mut p: Parser = Parser::init(None);
        let input = "72;x=notanumber";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x1b)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert!(dnd.read_option(Option_::X).is_null());
    }

    #[test]
    fn osc_72_bel_terminator_recorded() {
        let mut p: Parser = Parser::init(None);
        let input = "72;t=q";
        for ch in input.bytes() {
            p.next(ch);
        }
        let cmd = p.end(Some(0x07)).unwrap().clone();
        let dnd = match cmd {
            Command::KittyDndProtocol(dnd) => dnd,
            _ => unreachable!(),
        };
        assert_eq!(Terminator::Bel, dnd.terminator);
    }
}
```

(PORT NOTE: Zig's `Option` enum uses bare `x`/`X` tag names, which Rust cannot — `Option_::X` and `Option_::CapX` are chosen names for the lowercase/uppercase pair, mirroring the `Option_` disambiguation already established for `Option` in `kitty_clipboard_protocol.rs:123-125`. Pick `X`/`CapX`, `Y`/`CapY` or an equally clear pair; keep it consistent between the enum, `tag_name()`, and all call sites.)

- [ ] **Step 3: Run to verify compile failure** (module doesn't exist yet)

```bash
cargo test -p rustty-vt --lib osc::parsers::kitty_dnd_protocol 2>&1 | tail -10
```

Expected: COMPILE FAIL (`kitty_dnd_protocol` module not found; `Command::KittyDndProtocol` not found — both land in Task 3).

- [ ] **Step 4: Write the parser module.** Model directly on `crates/rustty-vt/src/osc/parsers/kitty_clipboard_protocol.rs` (same file structure: header PORT NOTEs, `OSC` struct, `read_option`, value enum, `Option_` enum with `tag_name`/`read`/`null_value`, `parse` fn, `trim_whitespace` helper). Full content:

```rust
//! Kitty's drag and drop protocol (OSC 72)
//! Specification: https://sw.kovidgoyal.net/kitty/drag-and-drop-protocol/
//!
//! PORTS: ghostty src/terminal/osc/parsers/kitty_dnd_protocol.zig
//!
//! PORT STATUS: ported — full parse logic + `readOption`/`Option_`/`EventType`
//! machinery + tests (WS8 BL-7, oracle 91c87e2cf + dd6c09bcf + a6132c18b +
//! c488ccda6 — ported from the final post-c488ccda6 state).
//!
//! No chunking reassembly or event-semantics handling — this module only
//! parses individual OSC 72 events, matching upstream scope exactly. The
//! action/dispatch layer (not yet ported) owns chunked-transfer reassembly
//! and drag-state semantics.

// PORT NOTE: `const assert = @import("../../../quirks.zig").inlineAssert;` —
// retargeted to `debug_assert!` at the call site (matching the convention used
// by `kitty_clipboard_protocol.rs`).
// PORT NOTE: `const log = std.log.scoped(.kitty_dnd_protocol);` — Rustty has no
// logging facade wired into this crate; ported as this note.
use crate::osc::{Command, Parser, State, Terminator};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OSC {
    /// The raw metadata that was received. Parse individual values with
    /// `read_option`.
    ///
    /// PORT NOTE (fidelity): reference `metadata: []const u8` — RAW bytes.
    pub metadata: Vec<u8>,
    /// The raw payload. Its meaning and encoding depend on the event type
    /// (`t` key).
    ///
    /// PORT NOTE (fidelity): reference `payload: ?[]const u8` — optional RAW bytes.
    pub payload: Option<Vec<u8>>,
    /// The terminator used for this OSC, so any response can match it.
    pub terminator: Terminator,
}

impl OSC {
    pub fn read_option(&self, key: Option_) -> OptionValue {
        key.read(&self.metadata)
    }
}

/// Values for the `t` (event type) metadata key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    /// ('a') Terminal registers itself as willing to accept drops.
    AcceptDrops,
    /// ('A') Terminal unregisters itself; drops should no longer be forwarded.
    StopAcceptingDrops,
    /// ('m') Pointer is moving over the terminal while a drag is in progress.
    /// Carries `x`/`y` cursor position; -1 signals the drag left the window.
    DropMove,
    /// ('M') Items were dropped onto the terminal.
    /// Carries `x`/`y` drop position and `i` (multiplexer session ID).
    DropDropped,
    /// ('r') Terminal requests data for a specific MIME type from the drag source.
    /// Carries `i` (multiplexer session ID) and `y` (1-based MIME type index).
    RequestData,
    /// ('R') Error response to a `request_data` event.
    RequestError,
    /// ('o') Terminal offers data for an outgoing drag (drag-out from terminal).
    OfferDrag,
    /// ('p') Drag source presents the actual payload for a previously requested MIME type.
    /// Carries `i` (multiplexer session ID), `o` (operation), and `m` (chunking flag).
    PresentData,
    /// ('P') Replace the current drag image with a new one.
    /// Payload is the image data; `X`/`Y` carry image dimensions in pixels.
    ChangeDragImage,
    /// ('e') Notification of an event on an outgoing drag offer (e.g., accepted or rejected).
    DragOfferEvent,
    /// ('E') Error on an outgoing drag offer.
    DragOfferError,
    /// ('k') URI list data delivered as part of a drag or clipboard transfer.
    UriListData,
    /// ('q') Query terminal capabilities related to the drag-and-drop protocol.
    Query,
}

impl EventType {
    pub fn init(str: &[u8]) -> Option<EventType> {
        if str.len() != 1 {
            return None;
        }
        match str[0] {
            b'a' => Some(EventType::AcceptDrops),
            b'A' => Some(EventType::StopAcceptingDrops),
            b'm' => Some(EventType::DropMove),
            b'M' => Some(EventType::DropDropped),
            b'r' => Some(EventType::RequestData),
            b'R' => Some(EventType::RequestError),
            b'o' => Some(EventType::OfferDrag),
            b'p' => Some(EventType::PresentData),
            b'P' => Some(EventType::ChangeDragImage),
            b'e' => Some(EventType::DragOfferEvent),
            b'E' => Some(EventType::DragOfferError),
            b'k' => Some(EventType::UriListData),
            b'q' => Some(EventType::Query),
            _ => None,
        }
    }
}

/// Metadata keys defined by the protocol. Keys are case-sensitive: `x` and
/// `X` are distinct (as are `y`/`Y`).
///
/// PORT NOTE: the reference `Option` enum uses bare `x`/`X`/`y`/`Y` tag names.
/// Rust identifiers are case-sensitive too, but `X`/`Y` collide with nothing
/// here — however to keep the lowercase/uppercase pair visually unambiguous
/// at call sites (and consistent with the `Option_`-not-`Option` naming
/// already established for `kitty_clipboard_protocol`), the uppercase members
/// are spelled `CapX`/`CapY`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Option_ {
    /// Event type. Maps to `EventType`; present in every OSC 72 sequence.
    T,
    /// Chunking flag. `0` = this is the final (or only) chunk; `1` = more chunks follow.
    M,
    /// Multiplexer session ID. Echoed back in responses so a terminal
    /// multiplexer (e.g. tmux) can route data to the correct pane.
    I,
    /// Drop operation. `0` = reject, `1` = copy, `2` = move, `3` = copy or move.
    O,
    /// Cursor column in cell units (zero-based). -1 signals the drag has left the window.
    X,
    /// Cursor row in cell units (zero-based). Also used as a 1-based MIME type index
    /// in some events (e.g. `request_data`). -1 signals the drag has left the window.
    Y,
    /// Pixel offset from the left edge of the cell; also used as image width
    /// (with `change_drag_image`) or as a symlink/directory marker.
    CapX,
    /// Pixel offset from the top edge of the cell; also used as image height
    /// (with `change_drag_image`) or as a parent directory handle.
    CapY,
}

/// The dependent value produced by `Option_::read`. The active variant
/// matches the `Option_::Type()` of the option that produced it (`T` reads a
/// `EventType`; every other key reads an `i32`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionValue {
    Int(Option<i32>),
    Type(Option<EventType>),
}

impl OptionValue {
    pub fn as_int(&self) -> Option<i32> {
        match self {
            OptionValue::Int(v) => *v,
            _ => panic!("option value is not an integer"),
        }
    }

    pub fn as_type(&self) -> Option<EventType> {
        match self {
            OptionValue::Type(v) => *v,
            _ => panic!("option value is not an event type"),
        }
    }

    pub fn is_null(&self) -> bool {
        match self {
            OptionValue::Int(v) => v.is_none(),
            OptionValue::Type(v) => v.is_none(),
        }
    }
}

impl Option_ {
    fn tag_name(self) -> &'static [u8] {
        match self {
            Option_::T => b"t",
            Option_::M => b"m",
            Option_::I => b"i",
            Option_::O => b"o",
            Option_::X => b"x",
            Option_::Y => b"y",
            Option_::CapX => b"X",
            Option_::CapY => b"Y",
        }
    }

    /// Look up an option in the metadata string. Returns the null variant if
    /// the key is absent, malformed, or its value cannot be parsed as the
    /// target type. The default values from the spec are NOT substituted
    /// here; callers should apply defaults explicitly so missing-vs-present
    /// can still be distinguished where it matters.
    pub fn read(self, metadata: &[u8]) -> OptionValue {
        let value: &[u8] = 'value: {
            let name = self.tag_name();
            let mut pos: usize = 0;
            while pos < metadata.len() {
                while pos < metadata.len() && metadata[pos].is_ascii_whitespace() {
                    pos += 1;
                }
                if pos >= metadata.len() {
                    return self.null_value();
                }
                // Case-sensitive match: x and X must not be confused.
                if !metadata[pos..].starts_with(name) {
                    pos = match metadata[pos..].iter().position(|&b| b == b':') {
                        Some(rel) => pos + rel,
                        None => return self.null_value(),
                    };
                    pos += 1;
                    continue;
                }
                pos += name.len();

                while pos < metadata.len() && metadata[pos].is_ascii_whitespace() {
                    pos += 1;
                }
                if pos >= metadata.len() {
                    return self.null_value();
                }
                if metadata[pos] != b'=' {
                    return self.null_value();
                }

                let end = match metadata[pos..].iter().position(|&b| b == b':') {
                    Some(rel) => pos + rel,
                    None => metadata.len(),
                };
                let start = pos + 1;
                break 'value trim_whitespace(&metadata[start..end]);
            }
            return self.null_value();
        };

        match self {
            Option_::T => OptionValue::Type(EventType::init(value)),
            Option_::M | Option_::I | Option_::O | Option_::X | Option_::Y | Option_::CapX
            | Option_::CapY => OptionValue::Int(parse_i32(value)),
        }
    }

    fn null_value(self) -> OptionValue {
        match self {
            Option_::T => OptionValue::Type(None),
            _ => OptionValue::Int(None),
        }
    }
}

// PORT NOTE: `std.fmt.parseInt(i32, value, 10) catch null`.
fn parse_i32(value: &[u8]) -> Option<i32> {
    core::str::from_utf8(value).ok()?.parse::<i32>().ok()
}

// PORT NOTE: `std.mem.trim(u8, slice, &std.ascii.whitespace)`.
fn trim_whitespace(s: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = s.len();
    while start < end && s[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && s[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &s[start..end]
}

pub fn parse<'a>(parser: &'a mut Parser, terminator_ch: Option<u8>) -> Option<&'a mut Command> {
    debug_assert!(parser.state == State::S72);

    let data: &[u8] = {
        let cap = if let Some(c) = parser.capture.as_ref() {
            c
        } else {
            parser.state = State::Invalid;
            return None;
        };
        cap.trailing(&parser.buffer)
    };

    let (metadata, payload): (&[u8], Option<&[u8]>) = 'result: {
        let sep = match data.iter().position(|&b| b == b';') {
            Some(sep) => sep,
            None => break 'result (data, None),
        };
        (&data[0..sep], Some(&data[sep + 1..data.len()]))
    };

    let metadata = metadata.to_vec();
    let payload = payload.map(|p| p.to_vec());

    parser.command = Command::KittyDndProtocol(OSC {
        metadata,
        payload,
        terminator: Terminator::init(terminator_ch),
    });

    Some(&mut parser.command)
}

// tests module from Step 2 goes here
```

(Verify the exact `Capture::trailing` signature and `Terminator::init` at the call site of `kitty_clipboard_protocol.rs:355,376` before finalizing — copy them verbatim, don't guess.)

- [ ] **Step 5: Register the module** in `crates/rustty-vt/src/osc/parsers/mod.rs`:

```rust
pub mod kitty_dnd_protocol;
```

placed alphabetically between `pub mod kitty_color;` and `pub mod kitty_text_sizing;`. Also add to the `ref_all_decls` test:

```rust
let _ = super::kitty_dnd_protocol::parse as fn(_, _) -> _;
```

placed alphabetically between the `kitty_color` and `kitty_text_sizing` lines.

- [ ] **Step 6: Run** — still fails (Task 3 wires `Command::KittyDndProtocol` + `State::S72`):

```bash
cargo build -p rustty-vt 2>&1 | tail -20
```

Expected: compile errors pointing at `Command::KittyDndProtocol` (unresolved) in the test module — confirms Task 2's code is otherwise self-consistent.

- [ ] **Step 7: Commit** (staged together with Task 3, since the module doesn't compile standalone — see Task 3 Step 6 for the actual commit point). No commit here.

---

### Task 3: Wire `Command::KittyDndProtocol` + `State::S72` into `osc/mod.rs`

**Files:**

- Modify: `crates/rustty-vt/src/osc/mod.rs`

**Interfaces:**

- Consumes: `crate::osc::parsers::kitty_dnd_protocol::{OSC, parse}` (Task 2).
- Produces: `Command::KittyDndProtocol(KittyDndProtocol)` variant, `CommandKey::KittyDndProtocol`, `State::S72` — consumed by Task 4 (`stream.rs`).

- [ ] **Step 1: Add the `Command` variant** (`crates/rustty-vt/src/osc/mod.rs:184`, right after `KittyClipboardProtocol`):

```rust
    KittyClipboardProtocol(KittyClipboardProtocol),

    /// Kitty drag and drop protocol (OSC 72)
    /// https://sw.kovidgoyal.net/kitty/drag-and-drop-protocol/
    KittyDndProtocol(KittyDndProtocol),

    /// OSC 3008. Hierarchical context signalling (UAPI spec).
```

- [ ] **Step 2: Add the type alias** (near line 310, right after the `KittyClipboardProtocol` alias):

```rust
/// PORT NOTE: `parsers.kitty_dnd_protocol.OSC` re-exported as
/// `Command.KittyDndProtocol`.
pub type KittyDndProtocol = parsers::kitty_dnd_protocol::OSC;
```

- [ ] **Step 3: Add the `CommandKey` discriminant** (line 300-301, `#[repr(i32)]` enum — order matters, append right after `KittyClipboardProtocol`, before `ContextSignal`, exactly mirroring the oracle's `Key` list order from the `91c87e2cf` diff):

```rust
    KittyClipboardProtocol,
    KittyDndProtocol,
    ContextSignal,
```

- [ ] **Step 4: Add the `key()` match arm** (line 262, right after `KittyClipboardProtocol`):

```rust
            Command::KittyClipboardProtocol(_) => CommandKey::KittyClipboardProtocol,
            Command::KittyDndProtocol(_) => CommandKey::KittyDndProtocol,
            Command::ContextSignal(_) => CommandKey::ContextSignal,
```

- [ ] **Step 5: Add the `reset()` no-op arm** (line 733, join the existing no-alloc group — the DnD `OSC` owns no allocations of its own beyond the `Vec<u8>`s Rust already drops automatically, matching the oracle's `=> {}` arm):

```rust
            | Command::KittyTextSizing(_)
            | Command::KittyClipboardProtocol(_)
            | Command::KittyDndProtocol(_)
            | Command::ContextSignal(_) => {}
```

- [ ] **Step 6: Add `State::S72`** to the `State` enum (line 542-543, between `S66` and `S77`):

```rust
    S66,
    S72,
    S77,
```

- [ ] **Step 7: Wire the state-machine transition** in `next()` (line 1017-1021, extend the existing `State::S7` arm):

```rust
            State::S7 => match c {
                b';' => self.capture_trailing(CaptureMode::Fixed),
                b'2' => self.state = State::S72,
                b'7' => self.state = State::S77,
                _ => self.state = State::Invalid,
            },

            State::S72 => match c {
                b';' => self.capture_trailing(CaptureMode::Allocating),
                _ => self.state = State::Invalid,
            },
```

(The oracle uses allocating-mode capture here — same as `S52`/`S66`/`S5522` — because the protocol allows payloads up to 4096 bytes per chunk plus metadata, exceeding the fixed buffer.)

- [ ] **Step 8: Wire the dispatch** in `end()` (line 1117-1119, insert between `S66` and `S77`):

```rust
            State::S66 => parsers::kitty_text_sizing::parse(self, terminator_ch),

            State::S72 => parsers::kitty_dnd_protocol::parse(self, terminator_ch),

            State::S77 => None,
```

- [ ] **Step 9: Run the new tests**

```bash
cargo test -p rustty-vt --lib osc::parsers::kitty_dnd_protocol 2>&1 | tail -20
```

Expected: 11/11 PASS (all tests from Task 2 Step 2).

- [ ] **Step 10: Run the full osc module + `ref_all_decls`**

```bash
cargo test -p rustty-vt --lib osc:: 2>&1 | tail -10
```

Expected: all PASS, including `parsers::tests::ref_all_decls`.

- [ ] **Step 11: Commit** (Tasks 2 + 3 together, since Task 2 doesn't compile standalone):

```bash
git add crates/rustty-vt/src/osc
git commit -m "osc: port kitty drag-and-drop protocol OSC 72 parser (BL-7, WS8 T2-T3)"
```

---

### Task 4: Wire `Command::KittyDndProtocol` into the `stream.rs` unimplemented-OSC dispatch

**Files:**

- Modify: `crates/rustty-vt/src/stream.rs:2577-2590`

**Interfaces:**

- Consumes: `osc::Command::KittyDndProtocol` (Task 3).

- [ ] **Step 1: Write the failing test** — assert the dispatch match is exhaustive by constructing a `Command::KittyDndProtocol` and feeding it through whatever harness the existing `KittyClipboardProtocol` no-op case is tested with. Find that harness first:

```bash
grep -n "kitty_clipboard_protocol\|KittyClipboardProtocol" /Users/gael/projects/rustty/crates/rustty-vt/src/stream.rs
grep -rn "fn.*unimplemented\|osc_command\|fn action\b" /Users/gael/projects/rustty/crates/rustty-vt/src/stream.rs | head -5
```

If `stream.rs` has no dedicated per-Command dispatch test (likely — this arm is a silent no-op, matching upstream `log.debug`-only behavior), skip the standalone unit test: the real regression protection here is the Rust compiler's **exhaustive match** — Step 2's edit is REQUIRED for `cargo build` to succeed once Task 3 adds the new `Command` variant, so:

- [ ] **Step 1b: Confirm the compile failure exists first** (this stands in for "write failing test" since the failure is a compiler error, not a test):

```bash
cargo build -p rustty-vt 2>&1 | grep -A3 "non-exhaustive\|KittyDndProtocol"
```

Expected: FAIL — `error[E0004]: non-exhaustive patterns: ... KittyDndProtocol ... not covered` at `stream.rs`'s OSC dispatch `match`.

- [ ] **Step 2: Add the arm** (`crates/rustty-vt/src/stream.rs:2586-2588`):

```rust
            | osc::Command::KittyTextSizing(_)
            | osc::Command::KittyClipboardProtocol(_)
            | osc::Command::KittyDndProtocol(_)
            | osc::Command::ContextSignal(_) => {
                // log.debug("unimplemented OSC callback: {}", .{cmd});
            }
```

- [ ] **Step 3: Run**

```bash
cargo build -p rustty-vt 2>&1 | tail -10
cargo test -p rustty-vt --lib 2>&1 | tail -5
```

Expected: green build; full `rustty-vt` test count = 2035 + 11 (Task 2's new tests) = 2046 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/rustty-vt/src/stream.rs
git commit -m "stream: dispatch OSC 72 kitty dnd as unimplemented, matching upstream scope (BL-7, WS8 T4)"
```

---

## Scope of this plan revision (READ BEFORE Task 5)

Tasks 5–11 below port the **12 BL-3 commits that were source-verified against
current `main` (74226dd)**: `cfce1cd56`, `04810273a`, `dace6d1c6`, `fdd255c78`,
`a14a619ce`, `1d5b6f90e`, `0c299000f`, `b6f34be44`, `607160657`, `a9f5b7eba`,
`fa8cae88b`, `08e376f23`.

The remaining BL-3 rows in `.port/phase-2/MAIN_PARITY.md` §3 (`9b466e9c5`,
`b287f6d1a`, `e727a3658`, `a55850c98`, `d6e24d985`, `30b42f42a`, `e6e4a9fdc`,
`afbf5ba15`, `c753fe4a4`, `0ff4e41b2`, `5bc6588e4`, `627518447`, `c442eb4b3`,
`8307349ec`, `8d663a76e`, `4c5b1d5d5`, `aea63d71f`) are **NOT** in this plan
revision and MUST stay marked `pending-small BL-3` in the ledger. The
`T-Page-*`/`T-Search-*`/`T-Pool-*`/`T-Bench-1`/`T-Grapheme-1`/`T-Resize-*`
entries in the File Structure section above are placeholders for that follow-up
pass; do not invent tasks for them here. Task 19 strikes ONLY the hashes listed
in its table.

---

## BL-3 — pending-small runtime-safety fixes (12 verified commits)

### Task 5: Establish the failing-allocation injection convention in `terminal.rs`

Upstream's title/pwd/hyperlink OOM tests use `std.testing.FailingAllocator`.
Rustty's `Allocator` is the infallible `allocator_api2::Global` handle, so this
crate uses a **tripwire fail-point module** instead (`rustty_util::tripwire`).
That convention already exists in `tabstops.rs`, `screen.rs`, `pagelist/mod.rs`
and `search/screen.rs` — but **not** in `terminal.rs`. This task locates the
house idiom and lands the `terminal.rs` instance so Tasks 6–7 can arm it.

**Files:**

- Modify: `crates/rustty-vt/src/terminal.rs` (new `#[cfg(test)]` tripwire block,
  placed immediately before `mod tests` at `terminal.rs:4964`)

**Interfaces:**

- Consumes: `rustty_util::tripwire::{Module, module, ResetMode, UntrippedError}`,
  `crate::pagelist::OutOfMemory`.
- Produces (used by Task 7):
  - `#[cfg(test)] pub(crate) enum SetStringFp { Reserve }`
  - `#[cfg(test)] thread_local! { pub(crate) static SET_STRING_TW: RefCell<Module<SetStringFp, OutOfMemory>> }`
  - `#[cfg(test)] fn set_string_tw_check(point: SetStringFp) -> Result<(), OutOfMemory>`

- [ ] **Step 1: Read the three existing instances of the house idiom** so the new
      one matches them character-for-character in shape:

```bash
sed -n '330,350p' /Users/gael/projects/rustty/crates/rustty-vt/src/tabstops.rs
sed -n '450,482p' /Users/gael/projects/rustty/crates/rustty-vt/src/screen.rs
sed -n '505,530p' /Users/gael/projects/rustty/crates/rustty-vt/src/tabstops.rs
```

Expected: a `#[cfg(test)] enum <Fn>Fp`, a `#[cfg(test)] thread_local!` holding
`RefCell<rustty_util::tripwire::Module<Fp, OutOfMemory>>`, a `#[cfg(test)] fn
<fn>_tw_check(point) -> Result<(), OutOfMemory>` helper, and test-side arming via
`TW.with(|tw| tw.borrow_mut().errorAlways(Fp::X, OutOfMemory))` +
`TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap())`.
There is **no** failing-allocator harness in this crate — the tripwire IS the
convention. Record that conclusion; do not introduce a second mechanism.

- [ ] **Step 2: Write the failing test** — append to `terminal.rs`'s
      `mod tests` (this is a self-test of the new seam; it fails to compile until
      Step 3 lands the module):

```rust
    // Self-test for the terminal.rs tripwire seam introduced for the
    // set_title/set_pwd allocation-failure tests (WS8 T5). Mirrors the
    // tabstops.rs `RESIZE_TW` usage exactly.
    #[test]
    fn terminal_set_string_tripwire_seam_arms_and_resets() {
        SET_STRING_TW.with(|tw| {
            tw.borrow_mut()
                .errorAlways(SetStringFp::Reserve, OutOfMemory)
        });
        assert_eq!(Err(OutOfMemory), set_string_tw_check(SetStringFp::Reserve));
        SET_STRING_TW.with(|tw| {
            tw.borrow_mut()
                .end(rustty_util::tripwire::ResetMode::Reset)
                .unwrap()
        });
        assert_eq!(Ok(()), set_string_tw_check(SetStringFp::Reserve));
    }
```

- [ ] **Step 3: Run to verify it fails**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib terminal_set_string_tripwire_seam 2>&1 | tail -15
```

Expected: COMPILE FAIL — `cannot find value SET_STRING_TW in this scope`,
`cannot find type SetStringFp`, `cannot find function set_string_tw_check`.

- [ ] **Step 4: Add the tripwire module** to `crates/rustty-vt/src/terminal.rs`,
      immediately above `#[cfg(test)] mod tests {` (currently line 4964):

```rust
// PORT NOTE: upstream's `setTitle`/`setPwd` allocation-failure tests
// (fdd255c78, 1d5b6f90e) use `std.testing.FailingAllocator`. This port's
// `Allocator` is the infallible `allocator_api2::Global` handle, so the crate's
// established failure-injection seam is a `rustty_util::tripwire` fail-point
// module held in a `#[cfg(test)] thread_local!` (same idiom as `tabstops.rs`'s
// `RESIZE_TW`, `screen.rs`'s `SELECTION_STRING_TW` and `pagelist/mod.rs`'s
// `INIT_TW`). The single `Reserve` point sits exactly where the reference's
// `ensureTotalCapacity` can fail, in both `set_pwd` and `set_title`.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum SetStringFp {
    Reserve,
}

#[cfg(test)]
thread_local! {
    pub(crate) static SET_STRING_TW:
        core::cell::RefCell<rustty_util::tripwire::Module<SetStringFp, OutOfMemory>> =
        core::cell::RefCell::new(rustty_util::tripwire::module());
}

// `try setString_tw.check(point)`. Only compiled in test builds; the call sites
// are `#[cfg(test)]`-gated so production `set_pwd`/`set_title` are unchanged,
// mirroring the Zig `enabled = builtin.is_test` gate.
#[cfg(test)]
#[inline]
fn set_string_tw_check(point: SetStringFp) -> Result<(), OutOfMemory> {
    SET_STRING_TW.with(|tw| tw.borrow_mut().check(point))
}
```

- [ ] **Step 5: Run the test**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib terminal_set_string_tripwire_seam 2>&1 | tail -5
```

Expected: 1 passed.

- [ ] **Step 6: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-vt/src/terminal.rs
git commit -m "terminal: add setString tripwire fail-point seam for OOM tests (BL-3, WS8 T5)"
```

---

### Task 6: `screen.rs` hyperlink fixes — dupe-before-end + wrapping implicit IDs (`cfce1cd56`, `04810273a`)

**Files:**

- Modify: `crates/rustty-vt/src/screen.rs:3040-3141` (`start_hyperlink`,
  `start_hyperlink_once`), plus a new `#[cfg(test)]` tripwire block next to the
  existing `SELECTION_STRING_TW` block (`screen.rs:460-481`)
- Test: `crates/rustty-vt/src/screen.rs`, `mod tests`, next to
  `screen_hyperlink_start_end` (`screen.rs:11959`) and `screen_hyperlink_reuse`

**Interfaces:**

- Consumes: `rustty_util::tripwire` (idiom confirmed in Task 5),
  `crate::screen::StartHyperlinkOnceError` (`screen.rs:537`),
  `crate::hyperlink::{Hyperlink, HyperlinkId}`, `crate::size::OffsetInt`.
- Produces: `#[cfg(test)] pub(crate) enum StartHyperlinkFp { DupeLink }` +
  `START_HYPERLINK_TW` + `start_hyperlink_tw_check` (local to `screen.rs`; no
  later task consumes them).

**PORT FINDING (record in the commit message, verbatim):** `cfce1cd56`'s
use-after-free is **not reachable in rustty as written**, because
`Screen::start_hyperlink` (`screen.rs:3049-3051`) materialises `uri.to_vec()` /
`HyperlinkId::Explicit(id.to_vec())` into an OWNED `Hyperlink` _before_
`start_hyperlink_once` is ever called, so the `source: Hyperlink` value that
`start_hyperlink_once` receives never borrows the cursor's hyperlink. The
reorder below is therefore an **R1 fidelity port + regression lock**, not a live
memory-safety fix. Port it anyway (R1) and port the test, which does have
real value: it pins the ownership property that makes the bug unreachable.
`04810273a` IS a live bug: `+= 1` / `-= 1` on `size::OffsetInt` panics on
overflow in debug builds where Zig's `+%=`/`-%=` wraps.

- [ ] **Step 1: Read both oracle commits**

```bash
git -C /Users/gael/projects/ghostty show cfce1cd56
git -C /Users/gael/projects/ghostty show 04810273a
```

- [ ] **Step 2: Write the three failing tests** in `screen.rs`'s `mod tests`,
      immediately after `screen_hyperlink_start_end`:

```rust
    // test "Screen: hyperlink accepts its current values"
    #[test]
    fn screen_hyperlink_accepts_its_current_values() {
        let mut s = mkscreen(5, 5, 0);

        s.start_hyperlink(b"http://example.com", Some(b"current"))
            .unwrap();

        // PORT NOTE: the reference passes `current.uri` / `current.id.explicit`
        // straight back into `s.startHyperlink`. Rust's borrow checker forbids
        // holding `&s.cursor.hyperlink` across the `&mut self` call, so we read
        // the current values through the RAW pointer the cursor already stores
        // (`cursor.hyperlink: Option<*mut Hyperlink>`) — the same aliasing the
        // reference test exercises, expressed in the way this crate allows.
        let link: *mut crate::hyperlink::Hyperlink = s.cursor.hyperlink.unwrap();
        let (uri_ptr, uri_len) = unsafe { ((*link).uri.as_ptr(), (*link).uri.len()) };
        let (id_ptr, id_len) = match unsafe { &(*link).id } {
            crate::hyperlink::HyperlinkId::Explicit(v) => (v.as_ptr(), v.len()),
            crate::hyperlink::HyperlinkId::Implicit(_) => unreachable!(),
        };
        let uri: &[u8] = unsafe { core::slice::from_raw_parts(uri_ptr, uri_len) };
        let id: &[u8] = unsafe { core::slice::from_raw_parts(id_ptr, id_len) };
        s.start_hyperlink(uri, Some(id)).unwrap();

        let link: *mut crate::hyperlink::Hyperlink = s.cursor.hyperlink.unwrap();
        assert_eq!(b"http://example.com".as_slice(), unsafe { &(*link).uri[..] });
        match unsafe { &(*link).id } {
            crate::hyperlink::HyperlinkId::Explicit(v) => {
                assert_eq!(b"current".as_slice(), &v[..])
            }
            crate::hyperlink::HyperlinkId::Implicit(_) => unreachable!(),
        }

        s.deinit();
    }

    // test "Screen: implicit hyperlink ID wraps" (success half)
    #[test]
    fn screen_implicit_hyperlink_id_wraps() {
        let mut s = mkscreen(5, 5, 0);

        s.cursor.hyperlink_implicit_id = size::OffsetInt::MAX;
        s.start_hyperlink(b"http://example.com", None).unwrap();

        assert_eq!(0 as size::OffsetInt, s.cursor.hyperlink_implicit_id);
        let link: *mut crate::hyperlink::Hyperlink = s.cursor.hyperlink.unwrap();
        match unsafe { &(*link).id } {
            crate::hyperlink::HyperlinkId::Implicit(v) => {
                assert_eq!(size::OffsetInt::MAX, *v)
            }
            crate::hyperlink::HyperlinkId::Explicit(_) => unreachable!(),
        }

        s.deinit();
    }

    // test "Screen: implicit hyperlink ID wraps" (allocation-failure rollback half)
    #[test]
    fn screen_implicit_hyperlink_id_wrap_rolls_back_on_allocation_failure() {
        let mut s = mkscreen(5, 5, 0);

        s.cursor.hyperlink_implicit_id = size::OffsetInt::MAX;
        // PORT NOTE: the reference swaps in a `testing.FailingAllocator`; this
        // crate's failure-injection seam is the tripwire fail point at the
        // `Box::new(source.dupe())` allocation in `start_hyperlink_once`.
        START_HYPERLINK_TW.with(|tw| {
            tw.borrow_mut()
                .errorAlways(StartHyperlinkFp::DupeLink, OutOfMemory)
        });
        assert_eq!(
            Err(pagelist::grow::IncreaseCapacityError::OutOfMemory),
            s.start_hyperlink(b"http://example.com", None)
        );
        START_HYPERLINK_TW
            .with(|tw| tw.borrow_mut().end(rustty_util::tripwire::ResetMode::Reset).unwrap());

        assert_eq!(size::OffsetInt::MAX, s.cursor.hyperlink_implicit_id);

        s.deinit();
    }
```

- [ ] **Step 3: Run to verify they fail**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib screen_hyperlink_accepts screen_implicit_hyperlink 2>&1 | tail -20
```

Expected: COMPILE FAIL on `START_HYPERLINK_TW` / `StartHyperlinkFp` (unresolved).
After Step 4's tripwire block lands but before Step 5/6,
`screen_implicit_hyperlink_id_wraps` FAILS with
`attempt to add with overflow` at `screen.rs:3055`.

- [ ] **Step 4: Add the `start_hyperlink` tripwire block** to `screen.rs`,
      immediately after the existing `selection_string_tw_check` helper (line 481):

```rust
// PORT NOTE: `04810273a`'s rollback test uses `std.testing.FailingAllocator` on
// `alloc.create(Hyperlink)`. The Rust analogue of that allocation is the
// `Box::new(source.dupe())` in `start_hyperlink_once`, which aborts rather than
// returning on OOM, so the injectable seam is a tripwire fail point (same idiom
// as `SELECTION_STRING_TW` above).
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum StartHyperlinkFp {
    DupeLink,
}

#[cfg(test)]
thread_local! {
    pub(crate) static START_HYPERLINK_TW:
        core::cell::RefCell<rustty_util::tripwire::Module<StartHyperlinkFp, OutOfMemory>> =
        core::cell::RefCell::new(rustty_util::tripwire::module());
}

#[cfg(test)]
#[inline]
fn start_hyperlink_tw_check(point: StartHyperlinkFp) -> Result<(), OutOfMemory> {
    START_HYPERLINK_TW.with(|tw| tw.borrow_mut().check(point))
}
```

- [ ] **Step 5: Apply the `cfce1cd56` reorder** in `start_hyperlink_once`
      (`screen.rs:3107-3125`) — delete the leading `self.end_hyperlink();` and move
      it to after the dupe, and add the tripwire check at the allocation:

```rust
    fn start_hyperlink_once(
        &mut self,
        source: hyperlink::Hyperlink,
    ) -> Result<(), StartHyperlinkOnceError> {
        // Allocate our new Hyperlink entry in non-page memory. This
        // lets us quickly get access to URI, ID.
        // PORT NOTE: `alloc.create(Hyperlink)` + `link.* = source.dupe(alloc)`
        // maps to a heap `Box`; `Box::into_raw` yields the `*mut Hyperlink` the
        // cursor stores. On the insert-error path we reclaim the box (mirroring
        // the reference's `errdefer alloc.destroy(link)` / `link.deinit`).
        #[cfg(test)]
        start_hyperlink_tw_check(StartHyperlinkFp::DupeLink)
            .map_err(|_| StartHyperlinkOnceError::OutOfMemory)?;
        let link: *mut hyperlink::Hyperlink = Box::into_raw(Box::new(source.dupe()));

        // End any prior hyperlink only after duplicating the new value. The
        // source slices are allowed to reference our current hyperlink.
        self.end_hyperlink();

        // Insert the hyperlink into page memory
        let page: *mut Page =
            unsafe { core::ptr::addr_of_mut!((*(*self.cursor.page_pin).node).data) };
```

(The rest of the function — the `insert_hyperlink` match, the box reclamation on
the `Insert` error path, and the `Save it all` block — is unchanged.)

- [ ] **Step 6: Apply the `04810273a` wrapping arithmetic** at the three
      `hyperlink_implicit_id` sites in `start_hyperlink`:

`screen.rs:3055`:

```rust
                let implicit = self.cursor.hyperlink_implicit_id;
                // defer self.cursor.hyperlink_implicit_id +%= 1;
                self.cursor.hyperlink_implicit_id =
                    self.cursor.hyperlink_implicit_id.wrapping_add(1);
```

`screen.rs:3072` and `screen.rs:3093` (both inside the `errdefer`-mirroring
rollback arms):

```rust
                        // .implicit => self.cursor.hyperlink_implicit_id -%= 1
                        self.cursor.hyperlink_implicit_id =
                            self.cursor.hyperlink_implicit_id.wrapping_sub(1);
```

- [ ] **Step 7: Run the three new tests plus the existing hyperlink suite**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib hyperlink 2>&1 | tail -10
```

Expected: all PASS (3 new + the pre-existing `screen_hyperlink_*` tests).

- [ ] **Step 8: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-vt/src/screen.rs
git commit -m "screen: dupe hyperlink before ending prior + wrap implicit IDs (BL-3 cfce1cd56/04810273a, WS8 T6)"
```

---

### Task 7: `terminal.rs` `set_title`/`set_pwd` — atomic terminator + alias-safe copy (`fdd255c78`+`dace6d1c6`, `1d5b6f90e`+`a14a619ce`)

rustty is **two commits behind on each field**: the terminator was appended with
a second fallible operation (leaving a non-empty unterminated buffer on OOM,
which breaks `get_pwd`/`get_title`'s `[0..len-1]` sentinel invariant), and
`extend_from_slice` is a **memcpy** that is UB when the source aliases the
destination — reachable through the vt-c raw-pointer FFI seam when a caller
re-passes `get_pwd()`/`get_title()`'s return.

**Files:**

- Modify: `crates/rustty-vt/src/terminal.rs:4529-4540` (`set_pwd`), `:4556-4567`
  (`set_title`)
- Test: `crates/rustty-vt/src/terminal.rs`, `mod tests`

**Interfaces:**

- Consumes: `SetStringFp`, `SET_STRING_TW`, `set_string_tw_check` (Task 5);
  `Terminal::init(Global, opts(cols, rows))` / `t.deinit(Global)` /
  `t.get_pwd()` / `t.get_title()` (all existing).
- Produces: no new public API — `set_pwd`/`set_title` keep the signature
  `pub fn set_pwd(&mut self, pwd: &[u8]) -> Result<(), OutOfMemory>`.

**CRITICAL translation notes (do not deviate):**

- `std.mem.copyForwards` → **`core::ptr::copy` (memmove)**. NEVER
  `copy_from_slice` or `extend_from_slice` — both are `memcpy` and are UB on
  overlapping ranges. That IS the bug being fixed.
- `ensureTotalCapacity` is **grow-only and amortized** → `Vec::reserve`, **not**
  `reserve_exact`.
- All mutation goes through a raw `*mut Vec<u8>` (`core::ptr::addr_of_mut!`) so
  no `&mut Vec<u8>` retag sits between deriving the source pointer and the
  memmove (memory: `rust-mut-args-strongly-protected`).

- [ ] **Step 1: Read the four oracle commits**

```bash
git -C /Users/gael/projects/ghostty show 1d5b6f90e -- src/terminal/Terminal.zig
git -C /Users/gael/projects/ghostty show a14a619ce -- src/terminal/Terminal.zig
git -C /Users/gael/projects/ghostty show fdd255c78 -- src/terminal/Terminal.zig
git -C /Users/gael/projects/ghostty show dace6d1c6 -- src/terminal/Terminal.zig
git -C /Users/gael/projects/ghostty show origin/main:src/terminal/Terminal.zig | sed -n '3550,3670p'
```

The last command shows the FINAL combined post-`dace6d1c6` state — port from it.

- [ ] **Step 2: Write the four failing tests** in `terminal.rs`'s `mod tests`:

```rust
    // test "Terminal: setPwd preserves a sentinel on allocation failure"
    #[test]
    fn terminal_set_pwd_preserves_a_sentinel_on_allocation_failure() {
        let mut t = Terminal::init(Global, opts(5, 1)).unwrap();

        // PORT NOTE: the reference primes the list with
        // `ensureTotalCapacityPrecise(alloc, 3)` and then fails the NEXT
        // allocation via `FailingAllocator`. Here the injectable seam is the
        // `Reserve` tripwire point (see the terminal.rs tripwire PORT NOTE).
        SET_STRING_TW.with(|tw| {
            tw.borrow_mut()
                .errorAlways(SetStringFp::Reserve, OutOfMemory)
        });
        assert_eq!(Err(OutOfMemory), t.set_pwd(b"pwd"));
        SET_STRING_TW.with(|tw| {
            tw.borrow_mut()
                .end(rustty_util::tripwire::ResetMode::Reset)
                .unwrap()
        });

        assert!(t.get_pwd().is_none());

        t.deinit(Global);
    }

    // test "Terminal: setPwd accepts its current value"
    #[test]
    fn terminal_set_pwd_accepts_its_current_value() {
        let mut t = Terminal::init(Global, opts(5, 1)).unwrap();

        t.set_pwd(b"file:///tmp").unwrap();

        // PORT NOTE: the reference writes `t.setPwd(t.getPwd().?)`. Borrowck
        // forbids holding the `&[u8]` from `get_pwd` across the `&mut self`
        // call, so we re-form the SAME aliasing slice from raw parts. This is
        // exactly the aliasing the vt-c FFI seam can produce.
        let (ptr, len) = {
            let cur = t.get_pwd().unwrap();
            (cur.as_ptr(), cur.len())
        };
        let cur: &[u8] = unsafe { core::slice::from_raw_parts(ptr, len) };
        t.set_pwd(cur).unwrap();

        assert_eq!(b"file:///tmp".as_slice(), t.get_pwd().unwrap());

        t.deinit(Global);
    }

    // test "Terminal: setTitle preserves a sentinel on allocation failure"
    #[test]
    fn terminal_set_title_preserves_a_sentinel_on_allocation_failure() {
        let mut t = Terminal::init(Global, opts(5, 1)).unwrap();

        SET_STRING_TW.with(|tw| {
            tw.borrow_mut()
                .errorAlways(SetStringFp::Reserve, OutOfMemory)
        });
        assert_eq!(Err(OutOfMemory), t.set_title(b"title"));
        SET_STRING_TW.with(|tw| {
            tw.borrow_mut()
                .end(rustty_util::tripwire::ResetMode::Reset)
                .unwrap()
        });

        assert!(t.get_title().is_none());

        t.deinit(Global);
    }

    // test "Terminal: setTitle accepts its current value"
    #[test]
    fn terminal_set_title_accepts_its_current_value() {
        let mut t = Terminal::init(Global, opts(5, 1)).unwrap();

        t.set_title(b"Rustty").unwrap();

        let (ptr, len) = {
            let cur = t.get_title().unwrap();
            (cur.as_ptr(), cur.len())
        };
        let cur: &[u8] = unsafe { core::slice::from_raw_parts(ptr, len) };
        t.set_title(cur).unwrap();

        assert_eq!(b"Rustty".as_slice(), t.get_title().unwrap());

        t.deinit(Global);
    }
```

- [ ] **Step 3: Run to verify they fail**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib terminal_set_pwd terminal_set_title 2>&1 | tail -20
```

Expected: the two `*_preserves_a_sentinel_*` tests FAIL with
`assertion `left == right` failed: left: Ok(()), right: Err(OutOfMemory)` (no
tripwire check is on the path yet). The two `*_accepts_its_current_value` tests
either FAIL with corrupted output or pass by luck — either way they are not yet
protected, which Step 4 fixes.

- [ ] **Step 4: Rewrite `set_pwd`** (`terminal.rs:4528-4540`):

```rust
    /// Set the pwd for the terminal.
    // PORT NOTE (aliasing): `pwd` MAY alias `self.pwd` — a caller can hand
    // `get_pwd()`'s return straight back through the vt-c FFI seam. Every
    // mutation below therefore goes through a raw `*mut Vec<u8>` and the copy
    // uses `core::ptr::copy` (memmove, = Zig `std.mem.copyForwards`). NEVER use
    // `extend_from_slice`/`copy_from_slice` here: both are `memcpy` and are UB
    // on overlapping ranges. That is upstream a14a619ce.
    pub fn set_pwd(&mut self, pwd: &[u8]) -> Result<(), OutOfMemory> {
        // if (pwd.len == 0) { self.pwd.clearRetainingCapacity(); return; }
        if pwd.is_empty() {
            self.pwd.clear();
            return Ok(());
        }

        // const capacity = std.math.add(usize, pwd.len, 1) catch
        //     return error.OutOfMemory;
        let capacity = match pwd.len().checked_add(1) {
            Some(c) => c,
            None => return Err(OutOfMemory),
        };

        // try self.pwd.ensureTotalCapacity(self.gpa(), capacity);
        //
        // PORT NOTE: `ensureTotalCapacity` is GROW-ONLY and amortized, so this
        // is `reserve`, NOT `reserve_exact`. Rust's `reserve` aborts rather
        // than returning on real OOM, so the failure path is exercised by the
        // `Reserve` tripwire point (see the terminal.rs tripwire PORT NOTE).
        // Growing can reallocate, which would invalidate an aliased `pwd` —
        // that cannot happen in the aliasing case, because an aliased source is
        // a subslice of `self.pwd`, hence `capacity <= self.pwd.capacity()`.
        #[cfg(test)]
        set_string_tw_check(SetStringFp::Reserve)?;
        let v: *mut Vec<u8> = core::ptr::addr_of_mut!(self.pwd);
        // SAFETY: `v` is the live `self.pwd`; `pwd` is readable for `pwd.len()`
        // bytes and may overlap `*v`'s buffer, which `core::ptr::copy`
        // (memmove) tolerates. `set_len` exposes `capacity` bytes that are all
        // written before any read, and `u8` has no drop glue.
        unsafe {
            if (*v).capacity() < capacity {
                (*v).reserve(capacity - (*v).len());
            }
            // self.pwd.items.len = capacity;
            (*v).set_len(capacity);
            // std.mem.copyForwards(u8, self.pwd.items[0..pwd.len], pwd);
            core::ptr::copy(pwd.as_ptr(), (*v).as_mut_ptr(), pwd.len());
            // self.pwd.items[pwd.len] = 0;
            *(*v).as_mut_ptr().add(pwd.len()) = 0;
        }
        Ok(())
    }
```

- [ ] **Step 5: Rewrite `set_title`** (`terminal.rs:4555-4567`) — the identical
      shape over `self.title`:

```rust
    /// Set the title for the terminal, as set by escape sequences (e.g. OSC 0/2).
    // PORT NOTE (aliasing): `t` MAY alias `self.title` — a caller can hand
    // `get_title()`'s return straight back through the vt-c FFI seam. Every
    // mutation below therefore goes through a raw `*mut Vec<u8>` and the copy
    // uses `core::ptr::copy` (memmove, = Zig `std.mem.copyForwards`). NEVER use
    // `extend_from_slice`/`copy_from_slice` here: both are `memcpy` and are UB
    // on overlapping ranges. That is upstream dace6d1c6.
    pub fn set_title(&mut self, t: &[u8]) -> Result<(), OutOfMemory> {
        // if (t.len == 0) { self.title.clearRetainingCapacity(); return; }
        if t.is_empty() {
            self.title.clear();
            return Ok(());
        }

        // const capacity = std.math.add(usize, t.len, 1) catch
        //     return error.OutOfMemory;
        let capacity = match t.len().checked_add(1) {
            Some(c) => c,
            None => return Err(OutOfMemory),
        };

        // try self.title.ensureTotalCapacity(self.gpa(), capacity);
        //
        // PORT NOTE: grow-only + amortized => `reserve`, NOT `reserve_exact`.
        #[cfg(test)]
        set_string_tw_check(SetStringFp::Reserve)?;
        let v: *mut Vec<u8> = core::ptr::addr_of_mut!(self.title);
        // SAFETY: as in `set_pwd` above.
        unsafe {
            if (*v).capacity() < capacity {
                (*v).reserve(capacity - (*v).len());
            }
            // self.title.items.len = capacity;
            (*v).set_len(capacity);
            // std.mem.copyForwards(u8, self.title.items[0..t.len], t);
            core::ptr::copy(t.as_ptr(), (*v).as_mut_ptr(), t.len());
            // self.title.items[t.len] = 0;
            *(*v).as_mut_ptr().add(t.len()) = 0;
        }
        Ok(())
    }
```

- [ ] **Step 6: Run the four tests plus every existing title/pwd test**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib set_pwd set_title 2>&1 | tail -10
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib pwd title 2>&1 | tail -10
```

Expected: all PASS.

- [ ] **Step 7: Prove the aliasing tests are Stacked-Borrows clean under Miri**
      (memory: `shaper-hook-sb-clean-interleave` — do NOT hand-reason SB):

```bash
cd /Users/gael/projects/rustty && .port/tools/miri-run.sh -p rustty-vt --lib \
  terminal_set_pwd_accepts_its_current_value \
  terminal_set_title_accepts_its_current_value 2>&1 | tail -30
```

Expected: 2 passed, **no** `Undefined Behavior` diagnostic.

If Miri reports `attempting a read access ... tag does not exist in the borrow
stack` (i.e. the `reserve`/`set_len` retag popped the source pointer's tag),
apply this fallback — hoist the source to a raw pointer BEFORE any `&mut`-shaped
operation touches the Vec, and drive `set_len` through the raw Vec pointer only:

```rust
        let src: *const u8 = pwd.as_ptr();
        let src_len: usize = pwd.len();
        let v: *mut Vec<u8> = core::ptr::addr_of_mut!(self.pwd);
        unsafe {
            if (*v).capacity() < capacity {
                (*v).reserve(capacity - (*v).len());
            }
            let dst: *mut u8 = (*v).as_mut_ptr();
            (*v).set_len(capacity);
            core::ptr::copy(src, dst, src_len);
            *dst.add(src_len) = 0;
        }
```

Re-run the Miri command; expected 2 passed with no UB. Apply the same shape to
`set_title` if it is needed for `set_pwd`.

- [ ] **Step 8: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-vt/src/terminal.rs
git commit -m "terminal: reserve title/pwd terminator atomically + alias-safe memmove (BL-3 1d5b6f90e/a14a619ce/fdd255c78/dace6d1c6, WS8 T7)"
```

---

### Task 8: `screen.rs` `select()` — preserve aliased selection pins (`0c299000f`)

`Screen::select` (`screen.rs:3291-3311`) unconditionally calls
`old.deinit(self)`, which untracks BOTH old pins. A caller passing the screen's
CURRENT tracked selection back in shares those pins with the replacement, so the
replacement is left pointing at freed pool entries.

**Files:**

- Modify: `crates/rustty-vt/src/screen.rs:3298-3310`
- Test: `crates/rustty-vt/src/screen.rs`, `mod tests`, next to
  `screen_select_replaces_existing_pins` (`screen.rs:~11924`)

**Interfaces:**

- Consumes: `crate::selection::{Selection, Bounds, Order}` (`Bounds::Tracked`'s
  fields are already `*mut Pin`, `selection.rs:71`), `PageList::untrack_pin`
  (`pagelist/pin.rs:85`, takes `*mut Pin`), `Selection::order(&Screen) -> Order`.
- Produces: no signature change —
  `pub fn select(&mut self, sel_: Option<Selection>) -> Result<(), OutOfMemory>`.

- [ ] **Step 1: Read the oracle commit**

```bash
git -C /Users/gael/projects/ghostty show 0c299000f
```

- [ ] **Step 2: Write the failing test** in `screen.rs`'s `mod tests`,
      immediately after `screen_select_replaces_existing_pins`:

```rust
    // test "Screen: reselecting tracked selection preserves its pins"
    #[test]
    fn screen_reselecting_tracked_selection_preserves_its_pins() {
        let mut s = mkscreen(10, 2, 0);

        s.select(Some(Selection::init(
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 1, y: 0 }))
                .unwrap(),
            s.pages
                .pin(point::Point::Active(point::Coordinate { x: 3, y: 0 }))
                .unwrap(),
            false,
        )))
        .unwrap();

        // `Selection` is `Copy`, so this hands our own tracked selection back
        // to `select` by value — exactly the aliasing upstream fixes.
        let current = s.selection.unwrap();
        s.select(Some(current)).unwrap();

        assert_eq!(selection::Order::Forward, s.selection.unwrap().order(&s));

        s.deinit();
    }
```

- [ ] **Step 3: Run to verify it fails**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib screen_reselecting_tracked_selection 2>&1 | tail -20
```

Expected: FAIL — an assertion/panic from `order()` resolving a pin whose pool
entry was released (in a debug build this surfaces as a `pin_is_valid` /
tracked-pin assertion, or as a wrong `Order`).

- [ ] **Step 4: Apply the fix** — replace the `// Untrack prior selection`
      block at `screen.rs:3304-3307`:

```rust
        // Untrack prior selection pins that aren't also owned by the
        // replacement. A caller may pass our current tracked selection back to
        // us by value, so releasing both old pins unconditionally would leave
        // the replacement pointing at freed pool entries.
        if let Some(old) = self.selection.take() {
            // const new_bounds = tracked_sel.bounds.tracked;
            // PORT NOTE: `track` always yields `Bounds::Tracked`, so the
            // reference's unchecked `.tracked` field access is faithful; the
            // untracked arm is unreachable.
            let (new_start, new_end): (*mut Pin, *mut Pin) = match tracked_sel.bounds {
                selection::Bounds::Tracked { start, end } => (start, end),
                selection::Bounds::Untracked { .. } => unreachable!(),
            };
            match old.bounds {
                selection::Bounds::Untracked { .. } => old.deinit(self),
                selection::Bounds::Tracked { start, end } => {
                    if !core::ptr::eq(start, new_start) && !core::ptr::eq(start, new_end) {
                        self.pages.untrack_pin(start);
                    }
                    if !core::ptr::eq(end, new_start) && !core::ptr::eq(end, new_end) {
                        self.pages.untrack_pin(end);
                    }
                }
            }
        }
```

- [ ] **Step 5: Run the new test plus the whole selection suite**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib screen_select 2>&1 | tail -10
```

Expected: all PASS, including `screen_select_replaces_existing_pins` (which
asserts `tracked + 2 == s.pages.count_tracked_pins()` — the non-aliasing path is
unchanged).

- [ ] **Step 6: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-vt/src/screen.rs
git commit -m "screen: preserve aliased selection pins in select() (BL-3 0c299000f, WS8 T8)"
```

---

### Task 9: `selection.rs` page-width clamps — mirrored corners + contained rows (`b6f34be44`, `a9f5b7eba`)

Both fixes are the same class: a column taken from one page is applied to a pin
on a _different_, possibly narrower page during incomplete reflow, producing an
out-of-bounds pin that panics when resolved. Both live in `selection.rs` and
share the mixed-width-page test harness, so they are one task.

**Files:**

- Modify: `crates/rustty-vt/src/selection.rs:177-210` (`top_left`,
  `bottom_right`), `:367-447` (`contained_row_cached`)
- Test: `crates/rustty-vt/src/selection.rs`, `mod tests` (helpers
  `screen_init(cols, rows, max_scrollback)` at `:629`, `pin_screen` at `:643`)

**Interfaces:**

- Consumes: `Pin { node: *mut Node, x: size::CellCountInt, y: u32, garbage: bool }`,
  page width via `unsafe { (*pin.node).data.size.cols }` (there is **no**
  `Node::cols()` helper in this port — the reference `node.cols()` maps to that
  field read), `PageList::split(Pin) -> Result<(), SplitError>`
  (`pagelist/grow.rs:25`), `Selection::contained_row(&Screen, Pin) -> Option<Selection>`
  (`selection.rs:338`).
- Produces: no signature change. `contained_row_cached` keeps its `s: &Screen`
  parameter (dropping it would break the call from `contained_row`); it simply
  stops reading `s.pages.cols`.

- [ ] **Step 1: Read both oracle commits**

```bash
git -C /Users/gael/projects/ghostty show b6f34be44
git -C /Users/gael/projects/ghostty show a9f5b7eba
```

- [ ] **Step 2: Write the two failing tests** in `selection.rs`'s `mod tests`:

```rust
    // test "Selection: rectangle corners clamp across mixed-width pages"
    #[test]
    fn selection_rectangle_corners_clamp_across_mixed_width_pages() {
        let mut s = screen_init(4, 2, 0);

        let first = s.pages.pages.first;
        s.pages
            .split(Pin {
                node: first,
                x: 0,
                y: 1,
                garbage: false,
            })
            .unwrap();
        let second = unsafe { (*first).next };
        // PORT NOTE: `second.page().size.cols = 2` — a deliberate mid-reflow
        // narrow page, written directly through the node pointer.
        unsafe { (*second).data.size.cols = 2 };

        let sel = Selection::init(
            Pin { node: first, x: 3, y: 0, garbage: false },
            Pin { node: second, x: 1, y: 0, garbage: false },
            true,
        );
        assert_eq!(Order::MirroredForward, sel.order(&s));

        let bottom_right = sel.bottom_right(&s);
        // Resolving the pin is what panics when it is out of bounds.
        let _ = bottom_right.row_and_cell();
        assert!(Pin { node: second, x: 1, y: 0, garbage: false }.eql(bottom_right));

        s.deinit();
    }

    // test "Selection: containedRow clamps mixed-width pages"
    #[test]
    fn selection_contained_row_clamps_mixed_width_pages() {
        let mut s = screen_init(4, 3, 0);

        let first = s.pages.pages.first;
        s.pages
            .split(Pin { node: first, x: 0, y: 2, garbage: false })
            .unwrap();
        s.pages
            .split(Pin { node: first, x: 0, y: 1, garbage: false })
            .unwrap();
        let middle = unsafe { (*first).next };
        let last = unsafe { (*middle).next };
        unsafe { (*middle).data.size.cols = 2 };

        let linear = Selection::init(
            Pin { node: first, x: 1, y: 0, garbage: false },
            Pin { node: last, x: 1, y: 0, garbage: false },
            false,
        );
        let linear_row = linear
            .contained_row(&s, Pin { node: middle, x: 0, y: 0, garbage: false })
            .unwrap();
        let _ = linear_row.end().row_and_cell();
        assert!(Pin { node: middle, x: 0, y: 0, garbage: false }.eql(linear_row.start()));
        assert!(Pin { node: middle, x: 1, y: 0, garbage: false }.eql(linear_row.end()));

        let rectangle = Selection::init(
            Pin { node: first, x: 1, y: 0, garbage: false },
            Pin { node: last, x: 3, y: 0, garbage: false },
            true,
        );
        let rectangle_row = rectangle
            .contained_row(&s, Pin { node: middle, x: 0, y: 0, garbage: false })
            .unwrap();
        let _ = rectangle_row.end().row_and_cell();
        assert!(Pin { node: middle, x: 1, y: 0, garbage: false }.eql(rectangle_row.start()));
        assert!(Pin { node: middle, x: 1, y: 0, garbage: false }.eql(rectangle_row.end()));

        s.deinit();
    }
```

- [ ] **Step 3: Run to verify they fail**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib selection_rectangle_corners_clamp selection_contained_row_clamps 2>&1 | tail -25
```

Expected: FAIL — an out-of-bounds panic inside `row_and_cell()` (x = 3 on a
2-column page), or an `eql` assertion failure on the unclamped x.

- [ ] **Step 4: Clamp all four mirrored arms** — `selection.rs:177-210`:

```rust
    /// Returns the top left point of the selection.
    pub fn top_left(self, s: &Screen) -> Pin {
        match self.order(s) {
            Order::Forward => self.start(),
            Order::Reverse => self.end(),
            Order::MirroredForward => {
                let mut p = self.start();
                // p.x = @min(self.end().x, p.node.cols() - 1);
                let cols = unsafe { (*p.node).data.size.cols };
                p.x = core::cmp::min(self.end().x, cols - 1);
                p
            }
            Order::MirroredReverse => {
                let mut p = self.end();
                let cols = unsafe { (*p.node).data.size.cols };
                p.x = core::cmp::min(self.start().x, cols - 1);
                p
            }
        }
    }

    /// Returns the bottom right point of the selection.
    pub fn bottom_right(self, s: &Screen) -> Pin {
        match self.order(s) {
            Order::Forward => self.end(),
            Order::Reverse => self.start(),
            Order::MirroredForward => {
                let mut p = self.end();
                let cols = unsafe { (*p.node).data.size.cols };
                p.x = core::cmp::min(self.start().x, cols - 1);
                p
            }
            Order::MirroredReverse => {
                let mut p = self.start();
                let cols = unsafe { (*p.node).data.size.cols };
                p.x = core::cmp::min(self.end().x, cols - 1);
                p
            }
        }
    }
```

- [ ] **Step 5: Clamp `contained_row_cached`** — `selection.rs:367-447`. Add
      the unused-parameter binding at the top and replace the four column sites:

```rust
    ) -> Option<Selection> {
        // _ = s;
        // PORT NOTE: upstream drops the screen read entirely (`_ = s;`). The
        // parameter must stay for the `contained_row` call site's signature, so
        // we mirror the discard.
        let _ = s;
        if p.y < tl.y || p.y > br.y {
            return None;
        }

        // Rectangle case: we can return early as the x range will always be the
        // same. We've already validated that the row is in the selection.
        if self.rectangle {
            return Some(Selection::init(
                {
                    let mut copy: Pin = pin;
                    // copy.x = @min(tl.x, copy.node.cols() - 1);
                    let cols = unsafe { (*copy.node).data.size.cols };
                    copy.x = core::cmp::min(tl.x, cols - 1);
                    copy
                },
                {
                    let mut copy: Pin = pin;
                    // copy.x = @min(br.x, copy.node.cols() - 1);
                    let cols = unsafe { (*copy.node).data.size.cols };
                    copy.x = core::cmp::min(br.x, cols - 1);
                    copy
                },
                true,
            ));
        }
```

and both remaining `copy.x = s.pages.cols - 1;` sites (the `p.y == tl.y`
top-left-line arm at `:410` and the full-line fallthrough at `:442`) become:

```rust
                {
                    let mut copy: Pin = pin;
                    // copy.x = copy.node.cols() - 1;
                    copy.x = unsafe { (*copy.node).data.size.cols } - 1;
                    copy
                },
```

- [ ] **Step 6: Run the selection suite**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib selection:: 2>&1 | tail -10
```

Expected: all PASS, including the pre-existing `selection_top_left`,
`selection_bottom_right` and `selection_contained_row` tests.

- [ ] **Step 7: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-vt/src/selection.rs
git commit -m "selection: clamp mirrored corners and contained rows to page width (BL-3 b6f34be44/a9f5b7eba, WS8 T9)"
```

---

### Task 10: `screen.rs` page-width clamps — cloned selections + line selection (`607160657`, `fa8cae88b`)

Same class as Task 9, on the `screen.rs` side: `Screen::clone`'s selection
fallback pins use the _desired_ screen width (`pages.cols`) or an unclamped
rectangle column, and `Screen::select_line` assigns the previous pin a column
taken from the NEXT page.

**Files:**

- Modify: `crates/rustty-vt/src/screen.rs:971-999` (`clone` selection fallback
  pins), `:3531` (`select_line`'s `'end_pin` block)
- Test: `crates/rustty-vt/src/screen.rs`, `mod tests`, next to
  `screen_clone_contains_subset_of_selection` and the
  `screen_select_line_semantic_*` tests

**Interfaces:**

- Consumes: `PageList::track_pin(Pin) -> Result<*mut Pin, OutOfMemory>`,
  `PageList::split(Pin)`, `PageList::pause_integrity_checks(bool)`
  (`pagelist/mod.rs:1103`), `Screen::cursor_set_semantic_content`,
  `Screen::test_write_string`, page width via
  `unsafe { (*node).data.size.cols }`.
- Produces: no signature changes.

- [ ] **Step 1: Read both oracle commits**

```bash
git -C /Users/gael/projects/ghostty show 607160657
git -C /Users/gael/projects/ghostty show fa8cae88b
```

- [ ] **Step 2: Write the two failing tests** in `screen.rs`'s `mod tests`:

```rust
    // test "Screen: clone clamps clipped selections to mixed-width pages"
    #[test]
    fn screen_clone_clamps_clipped_selections_to_mixed_width_pages() {
        let mut s = mkscreen(4, 3, 0);

        let first = s.pages.pages.first;
        s.pages
            .split(Pin { node: first, x: 0, y: 2, garbage: false })
            .unwrap();
        s.pages
            .split(Pin { node: first, x: 0, y: 1, garbage: false })
            .unwrap();
        let middle = unsafe { (*first).next };
        let last = unsafe { (*middle).next };
        unsafe { (*middle).data.size.cols = 2 };

        s.select(Some(Selection::init(
            Pin { node: first, x: 0, y: 0, garbage: false },
            Pin { node: last, x: 3, y: 0, garbage: false },
            false,
        )))
        .unwrap();
        let mut linear = s
            .clone(
                point::Point::Screen(point::Coordinate { x: 0, y: 0 }),
                Some(point::Point::Screen(point::Coordinate { x: 0, y: 1 })),
            )
            .unwrap();
        let linear_end = linear.selection.unwrap().end();
        let _ = linear_end.row_and_cell();
        assert_eq!(1 as size::CellCountInt, linear_end.x);
        linear.deinit();

        s.select(Some(Selection::init(
            Pin { node: first, x: 3, y: 0, garbage: false },
            Pin { node: last, x: 3, y: 0, garbage: false },
            true,
        )))
        .unwrap();
        let mut rectangle = s
            .clone(
                point::Point::Screen(point::Coordinate { x: 0, y: 1 }),
                None,
            )
            .unwrap();
        let rectangle_start = rectangle.selection.unwrap().start();
        let _ = rectangle_start.row_and_cell();
        assert_eq!(1 as size::CellCountInt, rectangle_start.x);
        rectangle.deinit();

        s.deinit();
    }

    // test "Screen: selectLine semantic boundary across mixed-width pages"
    #[test]
    fn screen_select_line_semantic_boundary_across_mixed_width_pages() {
        let mut s = mkscreen(4, 2, 0);

        s.cursor_set_semantic_content(SemanticContentInput::ClearExplicit);
        s.test_write_string(b"ABCD").unwrap();
        s.cursor_set_semantic_content(SemanticContentInput::Output);
        s.test_write_string(b"E").unwrap();

        let first = s.pages.pages.first;
        s.pages
            .split(Pin { node: first, x: 0, y: 1, garbage: false })
            .unwrap();
        let second = unsafe { (*first).next };
        unsafe { (*first).data.size.cols = 2 };
        s.pages.pause_integrity_checks(true);

        assert_eq!(2 as size::CellCountInt, unsafe { (*first).data.size.cols });
        assert_eq!(4 as size::CellCountInt, unsafe { (*second).data.size.cols });

        let mut sel = s
            .select_line(SelectLine {
                pin: Pin { node: first, x: 1, y: 0, garbage: false },
                whitespace: None,
                semantic_prompt_boundary: false,
            })
            .unwrap();
        assert!(Pin { node: first, x: 0, y: 0, garbage: false }.eql(sel.start()));
        assert!(Pin { node: first, x: 1, y: 0, garbage: false }.eql(sel.end()));
        sel.deinit(&mut s);

        s.pages.pause_integrity_checks(false);
        s.deinit();
    }
```

Before running, confirm the exact spellings of `Screen::clone`'s two point
parameters, `SelectLine`'s field defaults, and the `cursor_set_semantic_content`
input enum (the reference `.{ .input = .clear_explicit }` / `.output`):

```bash
grep -n "pub fn clone(" -A 12 /Users/gael/projects/rustty/crates/rustty-vt/src/screen.rs
grep -n "fn cursor_set_semantic_content" -A 6 /Users/gael/projects/rustty/crates/rustty-vt/src/screen.rs
grep -n "screen_select_line_semantic" -A 20 /Users/gael/projects/rustty/crates/rustty-vt/src/screen.rs | head -40
```

Copy the call shapes verbatim from the neighbouring
`screen_clone_contains_subset_of_selection` and
`screen_select_line_semantic_boundary_first_cell_of_row` tests rather than
guessing.

- [ ] **Step 3: Run to verify they fail**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib screen_clone_clamps_clipped screen_select_line_semantic_boundary_across 2>&1 | tail -25
```

Expected: FAIL — `assertion `left == right` failed: left: 3, right: 1` on the
clone x assertions, and an out-of-bounds pin panic / wrong end pin for
`select_line`.

- [ ] **Step 4: Clamp the clone fallback pins** — `screen.rs:971-999`:

```rust
                        // We move the top pin back in bounds to the top row.
                        // const node = pages.pages.first.?;
                        let node = pages.pages.first;
                        break 'start pages.track_pin(Pin {
                            node,
                            // .x = if (sel.rectangle) @min(ordered.tl.x, node.cols() - 1) else 0
                            x: if sel.rectangle {
                                let cols = unsafe { (*node).data.size.cols };
                                core::cmp::min(unsafe { (*ordered.tl).x }, cols - 1)
                            } else {
                                0
                            },
                            y: 0,
                            garbage: false,
                        })?;
                    }
                };

                // If we got to this point it means that the selection is not
                // fully out of bounds, so we move the bottom right pin back
                // in bounds if it isn't already.
                let end_pin: *mut Pin = match pin_remap.get(&ordered.br) {
                    Some(&p) => p,
                    None => {
                        // const node = pages.pages.last.?;
                        let node = pages.pages.last;
                        let cols = unsafe { (*node).data.size.cols };
                        pages.track_pin(Pin {
                            node,
                            // .x = if (sel.rectangle) @min(ordered.br.x, node.cols() - 1)
                            //      else node.cols() - 1
                            x: if sel.rectangle {
                                core::cmp::min(unsafe { (*ordered.br).x }, cols - 1)
                            } else {
                                cols - 1
                            },
                            y: unsafe { (*node).data.size.rows } - 1,
                            garbage: false,
                        })?
                    }
                };
```

- [ ] **Step 5: Fix `select_line`'s destination width** — `screen.rs:3531`,
      inside the `'end_pin` block. `p.node` becomes `prev.node`:

```rust
                    if start_offset == 0 && cells[0].semantic_content() != v {
                        let mut prev = p.up(1).unwrap();
                        // prev.x = prev.node.cols() - 1;
                        prev.x = unsafe { (*prev.node).data.size.cols } - 1;
                        break 'end_pin prev;
                    }
```

(Leave the OTHER `copy.x = unsafe { (*p.node).data.size.cols } - 1;` at
`screen.rs:3546` alone — there `copy` IS `p`, so `p.node` is already the
destination node. Upstream changed only the one site.)

- [ ] **Step 6: Run the affected suites**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib screen_clone screen_select_line 2>&1 | tail -10
```

Expected: all PASS.

- [ ] **Step 7: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-vt/src/screen.rs
git commit -m "screen: clamp cloned selections and line selection to page width (BL-3 607160657/fa8cae88b, WS8 T10)"
```

---

### Task 11: `selection_gesture.rs` — bound drag geometry (`08e376f23`)

**Files:**

- Modify: `crates/rustty-vt/src/selection_gesture.rs:799-843` (`drag_selection`)
- Test: `crates/rustty-vt/src/selection_gesture.rs`, `mod tests` (helpers
  `test_press` at `:1101`, `test_drag` at `:1124`, `test_pin` at `:1164`)

**Interfaces:**

- Consumes: `Geometry { columns: u32, cell_width: u32, padding_left: u32, screen_height: u32 }`
  (`selection_gesture.rs:723`), `SelectionGesture::press(&mut self, &mut Terminal, Press<'_>) -> Result<Option<Selection>, OutOfMemory>`
  (`:283`), `SelectionGesture::drag(&mut self, &mut Terminal, Drag<'_>) -> Option<Selection>` (`:325`).
- Produces: no signature change —
  `fn drag_selection(click_pin, drag_pin, click_x: u32, drag_x: u32, rectangle_selection: bool, geometry: Geometry) -> Option<Selection>`.

**`08e376f23` pt.1 is a VERIFIED NON-ISSUE in Rust — documentation only, no code
change.** Upstream adds `pixelFromFloat` because Zig's `@intFromFloat` has UB /
a safety panic on NaN and on values outside the destination range. Rust's
`f64 as u32` cast is **saturating and NaN-safe by language definition** (RFC
2884, stable since 1.45): `NaN as u32 == 0`, `f64::INFINITY as u32 == u32::MAX`,
negatives clamp to 0. The existing `f64::max(0.0, self.left_click_xpos) as u32`
at `selection_gesture.rs:355-356` already has exactly `pixelFromFloat`'s
semantics (`f64::max` also returns the non-NaN operand, so `max(0.0, NaN) == 0.0`).
Do NOT add a `pixel_from_float` helper. DO port upstream's
`SelectionGesture drag clamps unrepresentable positions` test as a
documentation-value regression lock, and record the non-issue in the commit
message and in the MAIN_PARITY strike row (Task 19).

- [ ] **Step 1: Read the oracle commit**

```bash
git -C /Users/gael/projects/ghostty show 08e376f23
```

- [ ] **Step 2: Write the three failing tests** in
      `selection_gesture.rs`'s `mod tests`:

```rust
    // test "SelectionGesture drag clamps unrepresentable positions"
    //
    // PORT NOTE (08e376f23 pt.1 — VERIFIED NON-ISSUE in Rust): upstream adds
    // `pixelFromFloat` because Zig's `@intFromFloat` panics on NaN and on
    // out-of-range values. Rust's `f64 as u32` is saturating and NaN-safe by
    // language definition (RFC 2884), and `f64::max(0.0, NaN) == 0.0`, so
    // `selection_gesture.rs:355-356` already has `pixelFromFloat`'s semantics.
    // This test is a documentation-value regression lock, not a bug fix.
    #[test]
    fn selection_gesture_drag_clamps_unrepresentable_positions() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();
        let mut gesture = SelectionGesture::init();

        let mut press_event = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        press_event.xpos = 10.0;
        let _ = gesture.press(&mut t, press_event).unwrap();

        let positive = gesture
            .drag(&mut t, test_drag(&mut t, 1, 1, f64::INFINITY, 50.0))
            .unwrap();
        assert!(test_pin(&mut t, 1, 1).eql(positive.start()));
        assert!(test_pin(&mut t, 1, 1).eql(positive.end()));

        assert_eq!(
            None,
            gesture.drag(&mut t, test_drag(&mut t, 1, 1, f64::NAN, 50.0))
        );

        gesture.deinit(&mut t);
        t.deinit(Global);
    }

    // test "SelectionGesture drag saturates overflowing geometry"
    #[test]
    fn selection_gesture_drag_saturates_overflowing_geometry() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();
        let mut gesture = SelectionGesture::init();

        let press_event = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        let _ = gesture.press(&mut t, press_event).unwrap();

        let mut drag_event = test_drag(&mut t, 1, 1, 10.0, 50.0);
        drag_event.geometry.columns = u32::MAX;
        drag_event.geometry.cell_width = u32::MAX;
        assert_eq!(None, gesture.drag(&mut t, drag_event));

        gesture.deinit(&mut t);
        t.deinit(Global);
    }

    // test "SelectionGesture drag rejects empty geometry"
    #[test]
    fn selection_gesture_drag_rejects_empty_geometry() {
        let mut t = Terminal::init(Global, opts(5, 5)).unwrap();
        let mut gesture = SelectionGesture::init();

        let press_event = test_press(&mut t, 1, 1, Some(std::time::Instant::now()));
        let _ = gesture.press(&mut t, press_event).unwrap();

        let mut drag_event = test_drag(&mut t, 1, 1, 10.0, 50.0);
        drag_event.geometry.columns = 0;
        drag_event.geometry.cell_width = 0;
        assert_eq!(None, gesture.drag(&mut t, drag_event));

        gesture.deinit(&mut t);
        t.deinit(Global);
    }
```

Before running, confirm the constructor/teardown spellings used by the
neighbouring tests (the reference is `var gesture: SelectionGesture = .init;`
plus `defer gesture.deinit(&t);`):

```bash
grep -n "SelectionGesture::init\|gesture.deinit\|fn opts(" /Users/gael/projects/rustty/crates/rustty-vt/src/selection_gesture.rs | head -10
```

Use whatever those tests use verbatim.

- [ ] **Step 3: Run to verify they fail**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib selection_gesture_drag_saturates selection_gesture_drag_rejects selection_gesture_drag_clamps 2>&1 | tail -25
```

Expected: `*_saturates_overflowing_geometry` FAILS with
`attempt to multiply with overflow` at `selection_gesture.rs:834`;
`*_rejects_empty_geometry` FAILS with
`attempt to calculate the remainder with a divisor of zero` at
`selection_gesture.rs:839`. `*_clamps_unrepresentable_positions` should already
PASS (that is pt.1's non-issue — record the observation).

- [ ] **Step 4: Apply the fix** — `selection_gesture.rs:826-843`:

```rust
    // if (geometry.columns == 0 or geometry.cell_width == 0) return null;
    if geometry.columns == 0 || geometry.cell_width == 0 {
        return None;
    }

    // We only include cells in the selection if the threshold point lies
    // between the start and end points of the selection. A threshold of
    // 60% of the cell width was chosen empirically because it felt good.
    let threshold_point: u32 = f64::round(geometry.cell_width as f64 * 0.6) as u32;

    // We use this to clamp the pixel positions below.
    // const pixel_span = std.math.mul(u32, geometry.columns, geometry.cell_width)
    //     catch std.math.maxInt(u32);
    // const max_x = pixel_span - 1;
    let pixel_span: u32 = geometry
        .columns
        .checked_mul(geometry.cell_width)
        .unwrap_or(u32::MAX);
    let max_x = pixel_span - 1;
```

(`pixel_span` is `>= 1` because both factors are non-zero after the early
return, so `- 1` cannot underflow.)

- [ ] **Step 5: Run the gesture suite**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib selection_gesture 2>&1 | tail -10
```

Expected: all PASS (3 new + existing).

- [ ] **Step 6: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-vt/src/selection_gesture.rs
git commit -m "selection_gesture: bound drag geometry; pixelFromFloat is a Rust non-issue (BL-3 08e376f23, WS8 T11)"
```

---

## BL-2 — return free-listed page memory to the OS (`896aca499`)

The `PageList` page pool (`PagePool = DynamicMemoryPoolAligned<PageBackingAlloc>`,
`pagelist/mod.rs:251`) never returns memory to the OS: destroyed pages are zeroed
and free-listed until the surface exits, so any operation that shrinks the page
count keeps its high-water RSS forever. Pool items cannot be individually freed
(they live inside arena chunks) but they ARE page-aligned and page-multiple
sized, so they can be decommitted while they sit in the free list.

### Task 12: `decommit_pool_item` / `recommit_pool_item` + pool wiring

**Files:**

- Modify: `crates/rustty-vt/src/pagelist/mod.rs` — new module-private
  `decommit_pool_item`/`recommit_pool_item` + per-OS `madvise` bindings, placed
  immediately after the `destroy_node_ext` function (currently ending at
  `pagelist/mod.rs:1512`); wire into `create_page_ext` (`:1404`, the pooled
  branch), `destroy_node_ext` (`:1500-1506`), and `init_pages`' pooled branch
- Test: `crates/rustty-vt/src/pagelist/tests.rs` (or `pagelist/mod.rs`'s test
  module — whichever holds the existing pool tests; located in Step 2)

**Interfaces:**

- Consumes: `Self::page_memory(page) -> (*mut u8, usize)` and
  `Self::page_memory_len(node) -> usize` (existing private helpers in
  `pagelist/mod.rs`), `std_size() -> usize`, `pool.pages.create() -> Result<NonNull<u8>, _>`,
  `pool.pages.destroy(NonNull<u8>)`.
- Produces:
  - `fn decommit_pool_item(item: *mut u8, item_size: usize, dirty_len: usize)`
  - `fn recommit_pool_item(item: *mut u8, item_size: usize)`
    Both module-private to `pagelist/mod.rs`; no other task consumes them.

**PLATFORM PRIMITIVE — MANDATORY CONVENTION.** This crate deliberately avoids
the `libc` crate and hand-rolls its syscall bindings: see
`crates/rustty-vt/src/page.rs:249-313`, where `Backing::alloc`/`Backing::free`
declare `extern "C" { #[link_name = "mmap"] fn libc_mmap(...); }` with per-OS
`MAP_*`/`PROT_*` const blocks. **Extend that exact pattern. Do NOT add the `libc`
crate to any `Cargo.toml`.** (Note: the plan header's Tech Stack line names the
`libc` crate for BL-2 — that line is superseded by this paragraph; correct it
when this task lands.)

- [ ] **Step 1: Read the oracle commit**

```bash
git -C /Users/gael/projects/ghostty show 896aca499
```

- [ ] **Step 2: Locate the existing pool-reuse test and the `#[cfg(test)]`
      backing-allocator split**

```bash
grep -n "PageBackingAlloc\|page_backing_alloc\|TrackingAllocator" /Users/gael/projects/rustty/crates/rustty-vt/src/pagelist/mod.rs | head
grep -rn "destroy_node_ext\|zero\|write_bytes" /Users/gael/projects/rustty/crates/rustty-vt/src/pagelist/tests.rs | head
```

Expected: under `cfg(test)` the pool's backing allocator is a
`TrackingAllocator` over `page_allocator()` — **not** raw OS pages — so
`madvise` is neither safe nor meaningful in test builds. This is exactly
upstream's `if (comptime builtin.is_test)` guard.

- [ ] **Step 3: Write the failing test** — the zero-invariant is the contract
      the decommit must preserve. Add to the pagelist test module:

```rust
    // A pooled page buffer returning to the free list must read back as all
    // zeroes, so the next `create()` that reuses it hands out observably-zero
    // memory. `decommit_pool_item` is what upholds this after WS8 T12.
    #[test]
    fn pagelist_decommit_pool_item_zeroes_the_dirty_region() {
        let mut buf = vec![0xAAu8; 4096];
        let ptr = buf.as_mut_ptr();
        super::decommit_pool_item(ptr, buf.len(), 512);
        assert!(buf[0..512].iter().all(|&b| b == 0));
        // Bytes past `dirty_len` are untouched by the test-build path.
        assert!(buf[512..].iter().all(|&b| b == 0xAA));
    }
```

- [ ] **Step 4: Run to verify it fails**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib pagelist_decommit_pool_item 2>&1 | tail -10
```

Expected: COMPILE FAIL — `cannot find function decommit_pool_item in module super`.

- [ ] **Step 5: Add the primitives** to `crates/rustty-vt/src/pagelist/mod.rs`,
      immediately after `destroy_node_ext`:

```rust
/// Zero a pool item buffer that is about to be returned to the pool free list
/// and, where supported, tell the OS it can reclaim the backing memory while
/// the buffer sits unused. Without the decommit, a pool that shrinks
/// (scrollback clears, pruning churn, resets) retains its high-water RSS
/// forever, since memory-pool free lists never return memory to the OS.
///
/// `dirty_len` is the length of the page that lived in this item, which may be
/// smaller than the item; bytes beyond it are already zero.
///
/// The invariant required for page reuse (see `create_page_ext`): after this
/// call the entire item reads back as zeroes. The pool writes its free-list
/// node into the first pointer-size bytes afterwards, which is safe because
/// page initialization always overwrites them.
///
/// PORT NOTE (platform primitive): this crate hand-rolls its syscall bindings
/// rather than depending on the `libc` crate — see `page.rs`'s
/// `Backing::alloc`/`Backing::free` (`mmap`/`munmap`) and its per-OS const
/// blocks. `madvise` follows the same pattern below.
fn decommit_pool_item(item: *mut u8, item_size: usize, dirty_len: usize) {
    // In test builds the buffer comes from the tracking allocator over the
    // page allocator, not necessarily a bare OS mapping, so madvise is neither
    // safe nor meaningful. (Zig: `if (comptime builtin.is_test)`.)
    #[cfg(test)]
    {
        let _ = item_size;
        // SAFETY: `item` is a live pool item of at least `dirty_len` bytes.
        unsafe { core::ptr::write_bytes(item, 0, dirty_len) };
        return;
    }

    // MADV_DONTNEED on a private anonymous mapping immediately reclaims the
    // pages and guarantees they read back zero-filled on the next access, so
    // we can skip the memset entirely. We use MADV_DONTNEED rather than
    // MADV_FREE deliberately: MADV_FREE doesn't reduce RSS until memory
    // pressure and doesn't guarantee zeroes on the next read.
    #[cfg(all(not(test), target_os = "linux"))]
    {
        // SAFETY: `item` is page-aligned and `item_size` bytes long (the pool
        // dispenses page-aligned, page-multiple items).
        if unsafe { madvise(item as *mut core::ffi::c_void, item_size, MADV_DONTNEED) } == 0 {
            return;
        }
        // SAFETY: as above; fall back to the plain memset if madvise failed.
        unsafe { core::ptr::write_bytes(item, 0, dirty_len) };
        return;
    }

    // Darwin has no MADV_DONTNEED-with-zero-fill guarantee, so we zero in place
    // and then mark the item reusable, which removes it from the process
    // footprint immediately. Reuse is paired with MADV_FREE_REUSE in
    // `recommit_pool_item` so footprint accounting stays correct.
    #[cfg(all(not(test), target_os = "macos"))]
    {
        // SAFETY: `item` is a live pool item of at least `dirty_len` bytes.
        unsafe { core::ptr::write_bytes(item, 0, dirty_len) };
        // SAFETY: `item` is page-aligned and `item_size` bytes long.
        if unsafe { madvise(item as *mut core::ffi::c_void, item_size, MADV_FREE_REUSABLE) } != 0 {
            // SAFETY: as above.
            unsafe { madvise(item as *mut core::ffi::c_void, item_size, MADV_FREE) };
        }
        return;
    }

    // Every other platform (incl. wasm32, which has no madvise) keeps the
    // existing memset behavior.
    #[cfg(all(not(test), not(target_os = "linux"), not(target_os = "macos")))]
    {
        let _ = item_size;
        // SAFETY: `item` is a live pool item of at least `dirty_len` bytes.
        unsafe { core::ptr::write_bytes(item, 0, dirty_len) };
    }
}

/// Pair for `decommit_pool_item` on the reuse side. Only Darwin needs this:
/// MADV_FREE_REUSABLE must be balanced by MADV_FREE_REUSE when the pool hands
/// the buffer back out, otherwise the process footprint accounting drifts.
fn recommit_pool_item(item: *mut u8, item_size: usize) {
    #[cfg(all(not(test), target_os = "macos"))]
    {
        // SAFETY: `item` is page-aligned and `item_size` bytes long.
        unsafe { madvise(item as *mut core::ffi::c_void, item_size, MADV_FREE_REUSE) };
    }
    #[cfg(not(all(not(test), target_os = "macos")))]
    {
        let _ = item;
        let _ = item_size;
    }
}

// Minimal libc binding for madvise. We avoid pulling the `libc` crate as a new
// dependency; this signature and these constants are stable on the platforms we
// target. Mirrors the `mmap`/`munmap` bindings in `page.rs`.
#[cfg(all(not(test), any(target_os = "linux", target_os = "macos")))]
extern "C" {
    #[link_name = "madvise"]
    fn madvise(addr: *mut core::ffi::c_void, len: usize, advice: i32) -> i32;
}

#[cfg(all(not(test), target_os = "linux"))]
const MADV_DONTNEED: i32 = 4;

#[cfg(all(not(test), target_os = "macos"))]
const MADV_FREE: i32 = 5;
#[cfg(all(not(test), target_os = "macos"))]
const MADV_FREE_REUSABLE: i32 = 7;
#[cfg(all(not(test), target_os = "macos"))]
const MADV_FREE_REUSE: i32 = 8;
```

- [ ] **Step 6: Verify the constants against the platform headers** (do not
      trust the literals above without this check):

```bash
grep -n "MADV_FREE_REUSABLE\|MADV_FREE_REUSE\|define MADV_FREE" \
  "$(xcrun --show-sdk-path)/usr/include/sys/mman.h"
ssh rvm "grep -n 'MADV_DONTNEED' /usr/include/asm-generic/mman-common.h"
```

Expected: macOS `MADV_FREE 5`, `MADV_FREE_REUSABLE 7`, `MADV_FREE_REUSE 8`;
Linux `MADV_DONTNEED 4`. Fix the constants if the headers disagree.

- [ ] **Step 7: Run the new test**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib pagelist_decommit_pool_item 2>&1 | tail -5
```

Expected: 1 passed.

- [ ] **Step 8: Wire the decommit site** — `pagelist/mod.rs:1497-1506`, inside
      `destroy_node_ext`, replacing the plain `write_bytes` + `destroy` pair:

```rust
        if Self::page_memory_len(node) <= std_size() {
            // Reset the memory to zero (and decommit it, where supported) so it
            // can be reused.
            //
            // PORT NOTE (audit #4): this is the REUSED-buffer zeroing. A pooled
            // buffer returning to the free-list MUST read back as zero here so
            // the next `create()` reusing it hands back observably-zero memory
            // — fresh slots are lazily OS-zeroed, but a reused slot is not
            // re-`mmap`ed. `decommit_pool_item` upholds that on every platform
            // (Linux via MADV_DONTNEED's zero-fill guarantee, everywhere else
            // via the memset).
            let (ptr, len) = Self::page_memory(page);
            decommit_pool_item(ptr, std_size(), len);
            // SAFETY: `ptr` was handed out by `pool.pages.create()` (a standard
            // pooled buffer) and has not been destroyed.
            unsafe { pool.pages.destroy(NonNull::new(ptr).unwrap()) };
        } else {
            Self::free_non_standard_page(node);
        }
```

- [ ] **Step 9: Wire the recommit site** — `pagelist/mod.rs:1403-1412`, inside
      `create_page_ext`'s pooled branch:

```rust
        let page_buf: NonNull<u8> = if pooled {
            // break :buf try pool.pages.create(); + recommitPoolItem(buf)
            match pool.pages.create() {
                Ok(b) => {
                    recommit_pool_item(b.as_ptr(), std_size());
                    b
                }
                Err(_) => {
                    // errdefer pool.nodes.destroy(page).
                    unsafe { pool.nodes.destroy(page) };
                    return Err(OutOfMemory);
                }
            }
        } else {
```

- [ ] **Step 10: Wire the second recommit site in `init_pages`** — the pooled
      branch guarded by `init_pages_tw_check(InitPagesFp::PageBufStd)` at
      `pagelist/mod.rs:993`. Locate the `pool.pages.create()` call directly beneath
      it and apply the identical `recommit_pool_item(b.as_ptr(), std_size())` on the
      success arm:

```bash
sed -n '990,1010p' /Users/gael/projects/rustty/crates/rustty-vt/src/pagelist/mod.rs
```

Add `recommit_pool_item(<buf>.as_ptr(), std_size());` immediately after that
`create()` succeeds, matching upstream's `initPages` hunk.

- [ ] **Step 11: Run the full pagelist suite**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib pagelist 2>&1 | tail -10
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib 2>&1 | tail -5
```

Expected: all PASS with no change in count beyond the 1 new test.

- [ ] **Step 12: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-vt/src/pagelist/mod.rs crates/rustty-vt/src/pagelist/tests.rs
git commit -m "pagelist: decommit free-listed page memory back to the OS (BL-2 896aca499, WS8 T12)"
```

---

### Task 13: BL-2 verification — Miri exception registration + empirical RSS probe

`madvise` on OS-backed memory through a raw pointer is outside Miri's memory
model. The established precedent is `page.rs`'s `Backing::alloc`/`Backing::free`
(`mmap`/`munmap`), which is covered by NO Miri harness — no `.port/phase-2/mirihar-*`
touches `page.rs` or `pagelist`. Register this in the SAME category, and verify
the OS-return behavior empirically instead, matching upstream's own approach.

Note that the `#[cfg(test)]` branch of `decommit_pool_item` is what runs under
`cargo test` and Miri, so the pool's zero-invariant stays Miri-provable; only the
non-test `madvise` branches are excepted.

**Files:**

- Modify: `.port/phase-2/MIRI_EXCEPTIONS.md` (new section)
- Modify: `.port/phase-2/VT_MIRI_SWEEP.md` (cross-reference row)
- Create: `/private/tmp/.../ws8-rss-probe.rs` is NOT wanted — the probe runs as a
  `#[ignore]`d test inside `crates/rustty-vt/src/pagelist/tests.rs`

**Interfaces:**

- Consumes: `PageList` public API (`init`, `grow`, `erase`/scrollback clear,
  `deinit`) — the exact method names are confirmed in Step 3.
- Produces: an `#[ignore]`d test
  `pagelist_scrollback_clear_returns_rss_to_the_os` used only as a manual probe.

- [ ] **Step 1: Add the Miri exception entry** to
      `.port/phase-2/MIRI_EXCEPTIONS.md`, as a new top-level section after the
      existing "The exception list" section:

```markdown
## Category 2 — raw syscall memory management (outside Miri's model)

Miri models Rust's abstract machine; it does not model `mmap`/`munmap`/`madvise`
on OS-backed pages. Code that manages its own mappings therefore cannot be run
under Miri at all — this is a _model_ limitation, not a soundness gap, and it is
NOT a regression to chase.

| Site                                                                                                            | Syscall                                                                                            | Miri coverage                                                                                                                                                                                            | Complement                                                                                                                                                                                                                     |
| --------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `crates/rustty-vt/src/page.rs` `Backing::alloc`/`Backing::free` (~250-313)                                      | `mmap`/`munmap`                                                                                    | none (pre-existing; no `.port/phase-2/mirihar-*` touches `page.rs` or `pagelist`)                                                                                                                        | native + ASan over all 210 PageList tests; page tracking allocator asserts no leaks                                                                                                                                            |
| `crates/rustty-vt/src/pagelist/mod.rs` `decommit_pool_item`/`recommit_pool_item` (WS8 BL-2, oracle `896aca499`) | `madvise` (`MADV_DONTNEED` on Linux; `MADV_FREE_REUSABLE`/`MADV_FREE_REUSE`/`MADV_FREE` on Darwin) | none — the `madvise` arms are `#[cfg(not(test))]`, so Miri and `cargo test` both execute the plain-memset `#[cfg(test)]` arm instead, which IS Miri-provable and IS what upholds the pool zero-invariant | native + ASan; the zero-invariant unit test `pagelist_decommit_pool_item_zeroes_the_dirty_region`; the empirical RSS probe `pagelist_scrollback_clear_returns_rss_to_the_os` (`#[ignore]`, run manually on macOS and on `rvm`) |

Criterion note: the WS8 entry is _stronger_ than the `page.rs` precedent, because
the exact code path Miri would need to verify (the zero-invariant) is still run
under Miri via the `#[cfg(test)]` arm. Only the OS-hint calls — which have no
observable effect on program semantics, only on RSS — are unverifiable, and those
are covered by the RSS probe.
```

- [ ] **Step 2: Cross-reference it** in `.port/phase-2/VT_MIRI_SWEEP.md` — add
      one row to the summary table near line 15:

```markdown
| Raw syscall memory mgmt | 2 sites | `page.rs` mmap/munmap + `pagelist/mod.rs` madvise decommit (WS8 BL-2) → MIRI_EXCEPTIONS.md "Category 2"; the `#[cfg(test)]` arms ARE Miri-run, only the OS hints are excepted |
```

- [ ] **Step 3: Locate the PageList APIs the probe needs**

```bash
grep -n "pub fn init(\|pub fn deinit(\|pub fn grow(\|pub fn erase(\|scrollback" \
  /Users/gael/projects/rustty/crates/rustty-vt/src/pagelist/mod.rs | head -20
```

Use the exact signatures found; the probe below uses `PageList::init` /
`grow` / the scrollback-erase entry point / `deinit`.

- [ ] **Step 4: Write the RSS probe** as an `#[ignore]`d test in
      `crates/rustty-vt/src/pagelist/tests.rs`:

```rust
    // BL-2 empirical verification (WS8 T13). `madvise` has no observable effect
    // on program semantics, only on resident set size, so it cannot be asserted
    // by a normal unit test — and the `#[cfg(test)]` build compiles the memset
    // arm anyway. Run this MANUALLY against a release build on each platform:
    //
    //   cargo test -p rustty-vt --release --lib \
    //     pagelist_scrollback_clear_returns_rss_to_the_os -- --ignored --nocapture
    //
    // Expected: "after clear" RSS is within ~2 MiB of "before grow", i.e. the
    // ~10 MiB grown scrollback was returned. WITHOUT the decommit the "after
    // clear" figure stays at the "after grow" high-water mark.
    #[test]
    #[ignore = "manual RSS probe — see BL-2 / MIRI_EXCEPTIONS.md Category 2"]
    fn pagelist_scrollback_clear_returns_rss_to_the_os() {
        fn rss_bytes() -> u64 {
            #[cfg(target_os = "macos")]
            {
                let out = std::process::Command::new("ps")
                    .args(["-o", "rss=", "-p", &std::process::id().to_string()])
                    .output()
                    .expect("ps");
                let kb: u64 = String::from_utf8_lossy(&out.stdout)
                    .trim()
                    .parse()
                    .expect("rss kb");
                kb * 1024
            }
            #[cfg(target_os = "linux")]
            {
                let statm = std::fs::read_to_string("/proc/self/statm").expect("statm");
                let pages: u64 = statm
                    .split_whitespace()
                    .nth(1)
                    .expect("resident field")
                    .parse()
                    .expect("resident pages");
                pages * 4096
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            {
                0
            }
        }

        let before = rss_bytes();

        // Grow ~10 MiB of scrollback, then clear it. Substitute the exact
        // constructor/grow/erase spellings found in Step 3.
        let mut list = PageList::init(10_000_000, 80, 24).unwrap();
        while list.total_size() < 10_000_000 {
            list.grow().unwrap();
        }
        let after_grow = rss_bytes();
        list.erase_scrollback();
        let after_clear = rss_bytes();

        println!(
            "RSS before={} MiB  after_grow={} MiB  after_clear={} MiB",
            before / (1024 * 1024),
            after_grow / (1024 * 1024),
            after_clear / (1024 * 1024)
        );
        assert!(
            after_clear + 2 * 1024 * 1024 < after_grow,
            "scrollback clear did not return RSS: grow={} clear={}",
            after_grow,
            after_clear
        );

        list.deinit();
    }
```

- [ ] **Step 5: Run the probe on macOS**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --release --lib \
  pagelist_scrollback_clear_returns_rss_to_the_os -- --ignored --nocapture 2>&1 | tail -10
```

Expected: PASS with a printed line showing `after_clear` back near `before`.

- [ ] **Step 6: Run the probe on Linux**

```bash
ssh rvm "cd ~/projects/rustty && git fetch origin && git checkout ws8-upstream-small-sweep && git pull && cargo test -p rustty-vt --release --lib pagelist_scrollback_clear_returns_rss_to_the_os -- --ignored --nocapture 2>&1 | tail -10"
```

Expected: PASS (this exercises the `MADV_DONTNEED` arm, which macOS does not).

- [ ] **Step 7: Commit** (paste both platforms' printed RSS lines into the
      commit body as the evidence record):

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/MIRI_EXCEPTIONS.md .port/phase-2/VT_MIRI_SWEEP.md \
        crates/rustty-vt/src/pagelist/tests.rs
git commit -m "pagelist: register madvise Miri exception + empirical RSS probe (BL-2, WS8 T13)"
```

---

## BL-6 — kitty/gfx generation stamps (`bdc0b6c19` + `8fe5913f7`)

Replaces `Image.transmit_time: Instant` with a process-global monotonic u64
generation counter. Two problems are solved: a retransmission of the same image
ID with identical dimensions was undetectable by anything comparing
width/height/format/length; and `dirty` (the only storage-wide signal) is also
set by scrolling and resizing, which move placements without changing contents.

### Task 14: `rustty-vt` core — generation counter, storage/image stamps, mutation-aware delete

**Files:**

- Modify: `crates/rustty-vt/src/kitty/graphics_storage.rs` — new
  `GENERATION_COUNTER`/`next_generation`, `ImageStorage.generation` (:54-91),
  `mark_mutated`, `set_limit` (:148), `add_image` (:180), `add_placement` (:233),
  `image_by_number` (:298-318), `delete` (:322), `evict_image` (:721),
  `Candidate` (:829), removal of `instant_order` (:838)
- Modify: `crates/rustty-vt/src/kitty/graphics_image.rs` — `Image.generation`
  replacing `transmit_time` (:762-765, :784), removal of the transmit-path
  `Instant::now()` (:629) and the `InternalError` variant
- Modify: `crates/rustty-vt/src/kitty/graphics.rs` (the module that re-exports
  `ImageStorage` etc.) — re-export `next_generation`
- Modify: `crates/rustty-vt/src/kitty/graphics_exec.rs` — drop the
  `InternalError` arm from `encode_error`
- Test: `crates/rustty-vt/src/kitty/graphics_storage.rs` + `graphics_exec.rs`
  test modules

**Interfaces:**

- Produces (consumed by Tasks 15 and 16):
  - `pub fn crate::kitty::graphics_storage::next_generation() -> u64`
    (re-exported as `crate::kitty::graphics::next_generation`)
  - `pub generation: u64` on `ImageStorage` (default 0)
  - `pub generation: u64` on `crate::kitty::graphics_image::Image` (default 0)
- Consumes: `std::sync::atomic::{AtomicU64, Ordering}`, existing
  `ImageStorage::{images, placements, dirty}` fields.

**DECISION POINT — 32-bit atomic fallback (`8fe5913f7`), resolved in Step 2 of
this task, NOT deferred.** Upstream added a mutex-backed counter because the Zig
compiler rejects 64-bit atomics on targets whose largest atomic is 32 bits (e.g.
`arm-linux-androideabi`). Rust expresses the same constraint as
`cfg(target_has_atomic = "64")` and simply does not provide `AtomicU64` where it
is unavailable. Step 2 checks rustty's ACTUAL target matrix and lands one of two
concrete outcomes — never "TODO".

- [ ] **Step 1: Read both oracle commits**

```bash
git -C /Users/gael/projects/ghostty show bdc0b6c19 -- src/terminal/kitty/graphics_storage.zig src/terminal/kitty/graphics_image.zig src/terminal/kitty/graphics.zig src/terminal/kitty/graphics_exec.zig
git -C /Users/gael/projects/ghostty show 8fe5913f7
```

- [ ] **Step 2: Resolve the 32-bit-atomic decision with a concrete check.**
      Enumerate every target rustty is built for and test `target_has_atomic="64"`
      on each:

```bash
cd /Users/gael/projects/rustty
grep -rn "target\b" .cargo/config.toml rust-toolchain.toml 2>/dev/null
grep -rn "wasm32\|aarch64\|x86_64\|target =" .github/workflows/*.yml .port/tools/gates/*.sh 2>/dev/null | grep -o '[a-z0-9_]*-[a-z0-9_-]*' | sort -u
for t in aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu wasm32-unknown-unknown; do
  echo -n "$t: "; rustc --target "$t" --print cfg 2>/dev/null | grep -c 'target_has_atomic="64"'
done
```

Expected: `1` for every target rustty actually builds (all are 64-bit or wasm32,
and wasm32 reports `target_has_atomic="64"`).

- **If every target reports 1** (the expected outcome): use a plain
  `static GENERATION_COUNTER: AtomicU64` with NO fallback, and record in the
  module doc comment: _"PORT NOTE (8fe5913f7): upstream's comptime-selected
  mutex fallback for targets without 64-bit atomics is not needed — every target
  in rustty's matrix (`aarch64-apple-darwin`, `x86_64-apple-darwin`,
  `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
  `wasm32-unknown-unknown`) reports `target_has_atomic = \"64\"`, verified WS8
  T14. `8fe5913f7` is therefore ported as this note (N/A), not as code."_
  Mark `8fe5913f7` in MAIN_PARITY.md as **ported (N/A — not applicable to
  rustty's target matrix)**, with the check command in the row.
- **If ANY target reports 0**: land the fallback in the same commit —

```rust
#[cfg(target_has_atomic = "64")]
static GENERATION_COUNTER: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

#[cfg(not(target_has_atomic = "64"))]
static GENERATION_COUNTER: std::sync::Mutex<u64> = std::sync::Mutex::new(0);

/// Returns the next generation stamp. Stamps are unique and strictly
/// monotonically increasing process-wide, starting at 1 (0 is reserved to mean
/// "never stamped").
pub fn next_generation() -> u64 {
    #[cfg(target_has_atomic = "64")]
    {
        GENERATION_COUNTER.fetch_add(1, core::sync::atomic::Ordering::Relaxed) + 1
    }
    #[cfg(not(target_has_atomic = "64"))]
    {
        let mut g = GENERATION_COUNTER.lock().unwrap();
        *g += 1;
        *g
    }
}
```

and mark `8fe5913f7` as fully ported.

- [ ] **Step 3: Write the failing tests** in `graphics_storage.rs`'s test
      module (all three are direct ports of `bdc0b6c19`'s test hunk):

```rust
    // test "storage: generation stamps on image add and replace"
    #[test]
    fn storage_generation_stamps_on_image_add_and_replace() {
        let mut t = Terminal::init(Global, opts(3, 3)).unwrap();
        let mut s = ImageStorage::default();

        // Fresh storage has generation zero (never mutated).
        assert_eq!(0u64, s.generation);

        s.add_image(Global, Image { id: 1, width: 1, height: 1, ..Image::default() })
            .unwrap();
        let gen1 = s.generation;
        assert!(gen1 > 0);
        assert_eq!(gen1, s.image_by_id(1).unwrap().generation);

        // A second image gets a strictly greater stamp.
        s.add_image(Global, Image { id: 2, width: 1, height: 1, ..Image::default() })
            .unwrap();
        let gen2 = s.generation;
        assert!(gen2 > gen1);
        assert_eq!(gen2, s.image_by_id(2).unwrap().generation);

        // Retransmitting the same image ID (identical dimensions) gets a fresh
        // stamp: this is what makes same-sized retransmissions detectable.
        s.add_image(Global, Image { id: 1, width: 1, height: 1, ..Image::default() })
            .unwrap();
        let gen3 = s.generation;
        assert!(gen3 > gen2);
        assert_eq!(gen3, s.image_by_id(1).unwrap().generation);

        // Image 2 kept its stamp.
        assert_eq!(gen2, s.image_by_id(2).unwrap().generation);

        s.deinit(Global, unsafe { &mut *t.screens.active });
        t.deinit(Global);
    }

    // test "storage: generation bumps on placement and delete"
    #[test]
    fn storage_generation_bumps_on_placement_and_delete() {
        let mut t = Terminal::init(Global, opts(3, 3)).unwrap();
        let mut s = ImageStorage::default();

        s.add_image(Global, Image { id: 1, ..Image::default() }).unwrap();
        let gen_add = s.generation;

        s.add_placement(Global, 1, 1, test_placement(&mut t, 1, 1)).unwrap();
        let gen_place = s.generation;
        assert!(gen_place > gen_add);

        // Reads don't change the generation.
        let _ = s.image_by_id(1);
        let _ = s.image_by_number(1);
        assert_eq!(gen_place, s.generation);

        s.delete(Global, &mut t, command::Delete::All(true));
        assert!(s.generation > gen_place);

        s.deinit(Global, unsafe { &mut *t.screens.active });
        t.deinit(Global);
    }

    // test "storage: generation bumps when setLimit evicts or disables"
    #[test]
    fn storage_generation_bumps_when_set_limit_evicts_or_disables() {
        let mut t = Terminal::init(Global, opts(3, 3)).unwrap();
        let mut s = ImageStorage::default();

        s.add_image(
            Global,
            Image { id: 1, width: 1, height: 1, data: b"1234".to_vec(), ..Image::default() },
        )
        .unwrap();
        let gen_add = s.generation;

        // Lowering the limit evicts the image and must mark a mutation.
        s.dirty = false;
        s.set_limit(Global, unsafe { &mut *t.screens.active }, 1).unwrap();
        assert!(s.dirty);
        assert!(s.generation > gen_add);
        assert_eq!(0usize, s.images.len());
        let gen_evict = s.generation;

        // Disabling (limit=0) resets the storage and must mark a mutation.
        s.dirty = false;
        s.set_limit(Global, unsafe { &mut *t.screens.active }, 0).unwrap();
        assert!(s.dirty);
        assert!(s.generation > gen_evict);

        s.deinit(Global, unsafe { &mut *t.screens.active });
        t.deinit(Global);
    }
```

Before running, copy the neighbouring tests' exact spellings for `Terminal::init`

- `opts`, the `Image` literal, and the placement-construction helper (the
  reference `trackPin(&t, .{ .x = 1, .y = 1 })`):

```bash
grep -n "fn opts(\|ImageStorage::default\|add_placement(\|Location::Pin" \
  /Users/gael/projects/rustty/crates/rustty-vt/src/kitty/graphics_storage.rs | head -20
```

Reuse the existing helper if one exists; otherwise define `test_placement` in
the test module next to the other helpers, mirroring `trackPin`.

- [ ] **Step 4: Run to verify they fail**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib storage_generation 2>&1 | tail -20
```

Expected: COMPILE FAIL — no field `generation` on `ImageStorage`, no field
`generation` on `Image`.

- [ ] **Step 5: Add the counter and `next_generation`** at the top of
      `crates/rustty-vt/src/kitty/graphics_storage.rs`, after the existing imports:

```rust
/// Process-global counter backing all generation stamps (see
/// `ImageStorage::generation` and `Image::generation`). This is global rather
/// than per-storage so that stamps are unique across every storage in the
/// process: two mutation events never produce the same value, even across
/// separate screens (main vs. alt), storage resets, or separate terminals. This
/// lets consumers use a generation value alone as a cache key without any
/// ambiguity.
///
/// Atomic because separate terminals may mutate their storages from different
/// threads.
static GENERATION_COUNTER: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Returns the next generation stamp. Stamps are unique and strictly
/// monotonically increasing process-wide, starting at 1 (0 is reserved to mean
/// "never stamped").
pub fn next_generation() -> u64 {
    GENERATION_COUNTER.fetch_add(1, core::sync::atomic::Ordering::Relaxed) + 1
}
```

plus whichever module doc note Step 2 resolved.

- [ ] **Step 6: Add `ImageStorage.generation` + `mark_mutated`.** Field, in the
      struct at `graphics_storage.rs:54-91`, immediately after `dirty`:

```rust
    /// Dirty is set to true if placements or images change. This is
    /// purely informational for the renderer and doesn't affect the
    /// correctness of the program. The renderer must set this to false
    /// if it cares about this value.
    ///
    /// Note that dirty is also set by scrolling and resizing (outside of this
    /// struct) because those move placement pins, even though the set of
    /// images/placements itself is unchanged. See `generation` for a signal
    /// that only tracks content mutations.
    ///
    /// Invariant: dirty is always set when generation changes (`mark_mutated`
    /// sets both); dirty set without a generation change means a geometry-only
    /// event.
    pub dirty: bool,

    /// Generation stamp of the last content mutation to this storage: any image
    /// transmit/replace, placement add, or delete of either. Zero means the
    /// storage has never been mutated (and is therefore empty).
    ///
    /// Unlike `dirty`, this is NOT updated by scrolling/resizing, so an
    /// unchanged generation means the placement set and all image data are
    /// identical; only placement geometry (pins) may have moved. Values come
    /// from a process-global monotonic counter, so a value observed from any
    /// storage never recurs for different content, even across screen switches
    /// or storage resets.
    ///
    /// This field must only be written via `mark_mutated`.
    pub generation: u64,
```

Add `generation: 0,` to the `Default` impl (`graphics_storage.rs:105-115`),
immediately after `dirty: false,`. Then add the method inside `impl ImageStorage`,
right after `enabled()` (`:139-142`):

```rust
    /// Record a content mutation: marks the storage dirty and assigns a fresh
    /// generation stamp. Must be called by anything that changes the set of
    /// images or placements (or image contents).
    ///
    /// Do NOT call this for geometry-only events (scrolling, resizing, screen
    /// switches); those must set only the dirty flag directly. Bumping the
    /// generation for geometry changes would break the contract that an
    /// unchanged generation means unchanged contents.
    fn mark_mutated(&mut self) {
        self.dirty = true;
        self.generation = next_generation();
    }
```

- [ ] **Step 7: Replace `Image.transmit_time` with `Image.generation`** in
      `crates/rustty-vt/src/kitty/graphics_image.rs`.

At `:762-765`, replace the field and its PORT NOTE:

```rust
    /// Unique, monotonically increasing stamp assigned each time an image is
    /// added to (or replaced in) an `ImageStorage`. A changed generation for a
    /// given image ID means the image contents may have changed, even if the
    /// dimensions and byte length are the same (e.g. a retransmission of the
    /// same ID). Stamps order by transmission time. Zero means "never stored".
    ///
    /// PORT NOTE: replaces the pre-`bdc0b6c19` `transmit_time: std.time.Instant`.
    pub generation: u64,
```

At `:784`, `transmit_time: None,` becomes `generation: 0,`.

At `:629`, delete the whole transmit-time assignment (upstream deletes it):

```rust
        // (deleted — bdc0b6c19: `self.image.transmit_time = Instant::now()` and
        // its `InternalError` failure path are gone; the stamp is now assigned
        // by `ImageStorage::add_image`.)
```

Also delete the now-unused `InternalError` variant from `Image::Error` (grep for
it) and its arm in `graphics_exec.rs`'s `encode_error`:

```bash
grep -rn "InternalError" /Users/gael/projects/rustty/crates/rustty-vt/src /Users/gael/projects/rustty/crates/rustty-vt-c/src
```

Remove every hit that belongs to `Image::Error` (upstream removes
`error.InternalError => r.message = "EINVAL: internal error"`).

- [ ] **Step 8: Stamp on add, mutate on placement, mutate on reset.**

`add_image` (`graphics_storage.rs:227`) — replace `self.dirty = true;`:

```rust
        // Stamp the stored image with a fresh generation. This gives every
        // add/replace a unique stamp even when the same image ID is
        // retransmitted with identical dimensions, so consumers (e.g. renderer
        // texture caches) can detect content changes.
        self.mark_mutated();
        let generation = self.generation;
        self.images.get_mut(&id).unwrap().generation = generation;
        Ok(())
```

`add_placement` (`graphics_storage.rs:~279`) — replace its `self.dirty = true;`
with `self.mark_mutated();`.

`set_limit` (`graphics_storage.rs:160-163`) — inside the `limit == 0` branch,
after the struct is replaced:

```rust
        if limit == 0 {
            let image_limits = self.image_limits;
            self.deinit(alloc, s);
            *self = ImageStorage {
                image_limits,
                ..ImageStorage::default()
            };
            self.mark_mutated();
        }
```

- [ ] **Step 9: Make `delete` mutation-aware** — `graphics_storage.rs:322`.
      Capture the counts at entry and compare at exit, and delete the seven
      `self.dirty = true;` statements upstream removed from the `All`, `Column`,
      `Row`, `Z`, `Range`, `delete_by_id` and `delete_by_number` arms:

```rust
    /// Delete placements, images.
    pub fn delete(&mut self, alloc: Allocator, t: &mut Terminal, cmd: command::Delete) {
        // Deletes only ever remove placements/images, so comparing counts
        // before and after tells us whether anything actually changed. Only
        // then do we mark a mutation. This matters because a delete-all runs on
        // every screen clear (e.g. `ESC [ 2 J`), and we don't want empty clears
        // to dirty the image state or bump the generation.
        //
        // PORT NOTE: Zig's `defer` runs on every return path; Rust has no
        // `defer`, so the body is wrapped in an inner closure-free block and
        // the check runs once after it. `delete` returns `()` and has no early
        // `return`s, so a trailing check is exactly equivalent — verify that
        // when porting and, if any arm gains an early `return`, convert it to
        // `break 'delete` on a labeled block instead.
        let placements_before = self.placements.len();
        let images_before = self.images.len();

        match cmd {
            // ... every existing arm, verbatim, with its `self.dirty = true;`
            // statement DELETED (7 sites: All, Column, Row, Z, Range,
            // delete_by_id, delete_by_number) ...
        }

        if self.placements.len() != placements_before || self.images.len() != images_before {
            self.mark_mutated();
        }
    }
```

Find the seven sites with:

```bash
grep -n "self.dirty = true" /Users/gael/projects/rustty/crates/rustty-vt/src/kitty/graphics_storage.rs
```

(and note `delete_by_id`/`delete_by_number` are separate `fn`s — their
`self.dirty = true;` lines are deleted too, since `delete`'s trailing check now
covers them.)

- [ ] **Step 10: Switch the newest-image comparison** —
      `graphics_storage.rs:310`:

```rust
        for kv in self.images.values() {
            if kv.number == image_number {
                if newest.is_none() || kv.generation > newest.as_ref().unwrap().generation {
                    newest = Some(kv.clone());
                }
            }
        }
```

- [ ] **Step 11: Switch eviction ordering and mark eviction as a mutation** —
      `graphics_storage.rs:721-800` and the `Candidate` struct at `:829`:

```rust
/// The candidate type used during eviction (`evictImage`).
//
// PORT NOTE: the reference declares this as a function-local `const Candidate =
// struct {...}`. Rust has no local-to-method struct visible across closures, so
// it is hoisted to a private module-level struct with identical fields.
struct Candidate {
    id: u32,
    generation: u64,
    used: bool,
}
```

Delete the `instant_order` helper (`graphics_storage.rs:838-846`) entirely and
every remaining reference to it. In `evict_image`, the push becomes
`generation: img.generation,` and the comparator becomes:

```rust
        candidates.sort_unstable_by(|lhs, rhs| {
            // PORT NOTE: the reference `lessThan` returns `bool`; we port it to
            // a total `Ordering` comparator. `lessThan(lhs, rhs)` true => `Less`.

            // If their usage matches, then it's based on the generation stamp,
            // which orders by transmit time. (Stamps are unique but tie-break
            // by ID anyway to stay deterministic for hand-built test images.)
            if lhs.used == rhs.used {
                return if lhs.generation == rhs.generation {
                    lhs.id.cmp(&rhs.id)
                } else {
                    lhs.generation.cmp(&rhs.generation)
                };
            }

            // If not used, then its a better candidate
            if !lhs.used {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });
```

and add the eviction mutation flag around the removal loop:

```rust
        // Evicting anything is a content mutation. This matters for the
        // set_limit path in particular, which doesn't otherwise mark it.
        // PORT NOTE: Zig `defer if (any_evicted) self.markMutated();` — the
        // function's success paths are the `return true` inside the loop and
        // the trailing `Ok(false)`, so the flag is checked at both.
        let mut any_evicted = false;
```

setting `any_evicted = true;` at each of the two removal sites (placement
removal and image removal), and calling `if any_evicted { self.mark_mutated(); }`
immediately before EACH `return`/tail of the function.

- [ ] **Step 12: Re-export `next_generation`** from the kitty graphics facade
      module (upstream: `pub const nextGeneration = storage.nextGeneration;`):

```bash
grep -n "pub use\|pub const ImageStorage\|graphics_storage" /Users/gael/projects/rustty/crates/rustty-vt/src/kitty/graphics.rs
```

Add, alongside the existing re-exports:

```rust
pub use crate::kitty::graphics_storage::next_generation;
```

- [ ] **Step 13: Port the two `graphics_exec.rs` tests** from `bdc0b6c19`:

```bash
git -C /Users/gael/projects/ghostty show bdc0b6c19 -- src/terminal/kitty/graphics_exec.zig | sed -n '/kittygfx retransmit same id/,$p'
```

Port `kittygfx retransmit same id gets fresh image generation` and
`kittygfx delete then retransmit same id gets fresh generation` into
`graphics_exec.rs`'s test module, following that file's existing
`command::Parser::parse_string` / `execute(...)` test conventions verbatim
(copy the call shapes from the neighbouring `kittygfx *` tests).

- [ ] **Step 14: Build and run**

```bash
cd /Users/gael/projects/rustty && cargo build -p rustty-vt 2>&1 | tail -20
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib kitty 2>&1 | tail -10
```

Expected: build green (all `transmit_time` references inside `rustty-vt` gone);
all kitty tests PASS including the 5 new ones. `rustty-renderer` and
`rustty-vt-c` will NOT build yet — Tasks 15 and 16 fix them.

```bash
cd /Users/gael/projects/rustty && grep -rn "transmit_time" crates/rustty-vt/src
```

Expected: no output.

- [ ] **Step 15: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-vt/src/kitty
git commit -m "kitty/gfx: add generation stamps, delete transmit time (BL-6 bdc0b6c19, WS8 T14)"
```

---

### Task 15: `rustty-renderer` — swap the texture-cache staleness key to the generation

**Files:**

- Modify: `crates/rustty-renderer/src/image.rs` — `overlay_update` (~~:292-302),
  `prep_image` (~~:930-1004), `prep_kitty_image` (~:1045-1053), the `ImageMap`
  value struct (:1167-1173)

**Interfaces:**

- Consumes: `rustty_vt::kitty::graphics::next_generation() -> u64` and
  `Image::generation: u64` (Task 14).
- Produces: `prep_image(&mut self, alloc, id: Id, generation: u64, pending: Image::Pending) -> Result<(), PrepImageError>`
  and the map value field `generation: u64` (was `transmit_time: Instant`).

- [ ] **Step 1: Read the oracle's renderer hunk**

```bash
git -C /Users/gael/projects/ghostty show bdc0b6c19 -- src/renderer/image.zig
```

- [ ] **Step 2: Confirm the compile failure exists first** (the failing "test"
      here is the compiler — `rustty-renderer` cannot build against Task 14's
      `Image` any more):

```bash
cd /Users/gael/projects/rustty && cargo build -p rustty-renderer 2>&1 | grep -c "transmit_time"
```

Expected: a non-zero count of `no field transmit_time on type ...Image` errors.

- [ ] **Step 3: Swap the map value field** — `crates/rustty-renderer/src/image.rs:1167-1173`:

```rust
// pub const ImageMap = std.AutoHashMapUnmanaged(Id, struct { image, generation });
pub struct ImageMapValue {
    pub image: Image,

    /// The generation of the terminal image this was created from (see
    /// `rustty_vt::kitty::graphics_image::Image::generation`). Used to detect
    /// staleness: a differing generation for the same ID means the contents
    /// changed and the texture must be replaced. Zero is never a valid stored
    /// generation so it marks "not yet uploaded".
    pub generation: u64,
}
```

(Keep whatever the struct is actually named in this file — read line 1167 and
change only the field. Do not rename the type.)

- [ ] **Step 4: Swap `prep_image`'s parameter and comparisons** —
      `crates/rustty-renderer/src/image.rs:925-1005`:

```rust
        id: Id,
        generation: u64,
```

```rust
        // If this image exists and its generation is the same it is the
        // identical image so we don't need to send it to the GPU.
                if entry.generation == generation {
                    return Ok(());
                }
```

```rust
                    // Zig `.generation = 0`; overwritten below before any read.
                    generation: 0,
```

```rust
        entry.generation = generation;
```

- [ ] **Step 5: Swap the two call sites.** `overlay_update` (~:292-302):

```rust
        // Overlays are always considered new content, so we take a fresh
        // generation stamp to force replacing any existing one.
        let generation = rustty_vt::kitty::graphics::next_generation();
        // ...
        self.prep_image(alloc, Id::Overlay, generation, pending)?;
```

`prep_kitty_image` (~:1045-1053) — the whole `Option<Instant>` unwrap goes away:

```rust
        // try self.prepImage(alloc, .{ .kitty = image.id }, image.generation, .{ ... });
        self.prep_image(
            alloc,
            Id::Kitty(image.id),
            image.generation,
            /* the existing `Pending { width, height, ... }` literal, unchanged */
        )
```

Delete the two PORT NOTE comment blocks at `:983` and `:1047` that explain the
`Option<Instant>` modeling, and drop the now-unused `use std::time::Instant;`
import if nothing else in the file needs it.

- [ ] **Step 6: Build and test**

```bash
cd /Users/gael/projects/rustty && cargo build -p rustty-renderer 2>&1 | tail -10
cd /Users/gael/projects/rustty && cargo test -p rustty-renderer 2>&1 | tail -5
cd /Users/gael/projects/rustty && grep -rn "transmit_time" crates/rustty-renderer/src
```

Expected: build green, tests PASS, no `transmit_time` hits.

- [ ] **Step 7: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-renderer/src/image.rs
git commit -m "renderer: key texture staleness on kitty generation stamps (BL-6 bdc0b6c19, WS8 T15)"
```

---

### Task 16: `rustty-vt-c` — `Data::Generation` + `ImageData::Generation` + header

**Files:**

- Modify: `crates/rustty-vt-c/src/kitty_graphics.rs` — `Data` enum (:118-123),
  `get` dispatch (:161-172), `get_typed` (:195-226), `ImageData` enum (:290-304),
  `image_get_typed` (:429-460)
- Modify: `include/rustty/vt/kitty_graphics.h` — the `RusttyKittyGraphicsData`
  enum (:112-122), the `RusttyKittyImageData` enum (:287-346), and the two
  documentation blocks upstream added
- Test: `crates/rustty-vt-c/src/kitty_graphics.rs` test module

**Interfaces:**

- Consumes: `ImageStorage::generation: u64` and `Image::generation: u64`
  (Task 14).
- Produces: `Data::Generation = 2`, `ImageData::Generation = 9` and their
  `get`/`image_get`/`image_get_multi` dispatch arms;
  `RUSTTY_KITTY_GRAPHICS_DATA_GENERATION = 2`,
  `RUSTTY_KITTY_IMAGE_DATA_GENERATION = 9` in the C header.

**R2 naming:** the oracle's `GHOSTTY_KITTY_*` macros become `RUSTTY_KITTY_*`
(per-token casing preserved). There are NO `ghostty_*` forwarders.

- [ ] **Step 1: Read the oracle's C-API hunk and header hunk**

```bash
git -C /Users/gael/projects/ghostty show bdc0b6c19 -- src/terminal/c/kitty_graphics.zig
git -C /Users/gael/projects/ghostty show bdc0b6c19 -- include/ghostty/vt/kitty_graphics.h
```

- [ ] **Step 2: Write the failing tests** in `crates/rustty-vt-c/src/kitty_graphics.rs`'s
      test module (ports of the oracle's `storage generation via get` and
      `image generation via image_get_multi`; follow the file's existing test
      conventions for building a `KittyGraphics` handle):

```rust
    #[test]
    fn kitty_graphics_storage_generation_via_get() {
        // Build the handle exactly as the neighbouring tests do.
        let (mut t, graphics) = test_graphics();

        // Fresh storage: generation zero.
        let mut gen0: u64 = 1;
        assert_eq!(
            Result::Success,
            unsafe { get(graphics, Data::Generation, (&mut gen0) as *mut u64 as *mut c_void) }
        );
        assert_eq!(0u64, gen0);

        // Transmit bumps the generation.
        test_transmit(&mut t, "a=t,t=d,f=24,i=1,s=1,v=2;////////");
        let mut gen1: u64 = 0;
        assert_eq!(
            Result::Success,
            unsafe { get(graphics, Data::Generation, (&mut gen1) as *mut u64 as *mut c_void) }
        );
        assert!(gen1 > gen0);

        test_graphics_deinit(t);
    }

    #[test]
    fn kitty_graphics_image_generation_via_image_get_multi() {
        let (mut t, graphics) = test_graphics();
        test_transmit(&mut t, "a=t,t=d,f=24,i=1,s=1,v=2;////////");

        let img = unsafe { image_get_handle(graphics, 1) };
        assert!(!img.is_null());

        let mut id: u32 = 0;
        let mut generation: u64 = 0;
        let keys = [ImageData::Id, ImageData::Generation];
        let values: [*mut c_void; 2] = [
            (&mut id) as *mut u32 as *mut c_void,
            (&mut generation) as *mut u64 as *mut c_void,
        ];
        let mut written: usize = 0;
        assert_eq!(
            Result::Success,
            unsafe {
                image_get_multi(img, 2, keys.as_ptr(), values.as_ptr(), &mut written)
            }
        );
        assert_eq!(2usize, written);
        assert_eq!(1u32, id);
        assert!(generation > 0);

        // The image's stamp equals the storage's current stamp.
        let mut storage_gen: u64 = 0;
        assert_eq!(
            Result::Success,
            unsafe {
                get(graphics, Data::Generation, (&mut storage_gen) as *mut u64 as *mut c_void)
            }
        );
        assert_eq!(storage_gen, generation);

        test_graphics_deinit(t);
    }
```

Before running, read the file's existing tests and reuse their real
handle-construction / transmit / teardown helpers instead of the placeholder
names `test_graphics`/`test_transmit`/`test_graphics_deinit`:

```bash
grep -n "mod tests" -A 60 /Users/gael/projects/rustty/crates/rustty-vt-c/src/kitty_graphics.rs | head -80
```

- [ ] **Step 3: Run to verify they fail**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt-c --lib kitty_graphics 2>&1 | tail -20
```

Expected: COMPILE FAIL — `no variant Generation on enum Data` /
`no variant Generation on enum ImageData`.

- [ ] **Step 4: Add `Data::Generation`** — `crates/rustty-vt-c/src/kitty_graphics.rs:118-123`:

```rust
/// C: GhosttyKittyGraphicsData -> RusttyKittyGraphicsData (R2)
// pub const Data = enum(c_int) { invalid = 0, placement_iterator = 1, generation = 2, ... };
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Data {
    Invalid = 0,
    PlacementIterator = 1,
    Generation = 2,
}
```

`get`'s dispatch (`:169-172`) gains the arm:

```rust
    match data {
        Data::Invalid => Result::InvalidValue,
        // inline else => getTyped(graphics_, comptime_data, @ptrCast(@alignCast(out)))
        Data::PlacementIterator | Data::Generation => unsafe { get_typed(graphics_, data, out) },
    }
```

`get_typed`'s match (`:198-226`) gains, after the `PlacementIterator` arm:

```rust
        // .generation => out.* = storage.generation,
        Data::Generation => {
            // SAFETY: `out` points to a writable `u64` (caller contract);
            // `storage` is the live `ImageStorage` behind the handle.
            unsafe { *(out as *mut u64) = (*storage).generation };
            Result::Success
        }
```

- [ ] **Step 5: Add `ImageData::Generation`** — `:290-304`:

```rust
pub enum ImageData {
    Invalid = 0,
    Id = 1,
    Number = 2,
    Width = 3,
    Height = 4,
    Format = 5,
    Compression = 6,
    DataPtr = 7,
    DataLen = 8,
    Generation = 9,
}
```

`image_get_typed`'s match (`:437-456`) gains, after the `DataLen` arm:

```rust
        // .generation => out.* = image.generation,
        ImageData::Generation => unsafe { *(out as *mut u64) = image.generation },
```

(`image_get`'s dispatch at `:363-367` already uses a `_ =>` catch-all, so it
needs no edit — confirm that when editing.)

- [ ] **Step 6: Update the C header** — `include/rustty/vt/kitty_graphics.h`.

In `RusttyKittyGraphicsData` (line 121, after `..._PLACEMENT_ITERATOR = 1,`):

```c
  /**
   * Generation stamp of the last content mutation to this storage: any
   * image transmit/replace, placement add, or delete. Zero means the
   * storage has never been mutated (and is therefore empty).
   *
   * If the generation is unchanged since a previous query, the set of
   * placements and all image data are identical, so placement iteration and
   * image staleness checks can be skipped entirely. Note that placement
   * *geometry* may still have changed (scrolling and resizing move placements
   * without changing the storage contents), so rendering geometry such as
   * rustty_kitty_graphics_placement_render_info() must still be recomputed for
   * frames marked dirty.
   *
   * Stamps are unique and monotonically increasing process-wide: a value
   * observed from any storage never recurs for different content, even across
   * screen switches (main vs. alternate screen have independent storages) or
   * terminal resets. It is therefore safe to key caches on this value alone.
   *
   * Output type: uint64_t *
   */
  RUSTTY_KITTY_GRAPHICS_DATA_GENERATION = 2,
```

In `RusttyKittyImageData` (line 345, after `..._DATA_LEN = 8,`):

```c
  /**
   * Generation stamp assigned when this image was added to (or replaced in)
   * the storage. A changed generation for a given image ID means the pixel
   * contents may have changed even when the dimensions, format, and data
   * length are identical (e.g. a retransmission of the same ID). Texture
   * caches must treat a differing value as stale. Zero means "never stored".
   *
   * Output type: uint64_t *
   */
  RUSTTY_KITTY_IMAGE_DATA_GENERATION = 9,
```

Also port upstream's four documentation-only clarifications (they are pure prose
and carry no snark, so port them verbatim modulo R2 naming): the "## Change
Detection" block in the file header, and the notes on `..._FORMAT`,
`..._COMPRESSION`, `..._DATA_PTR` and `..._DATA_LEN` stating that stored image
data is always post-inflate/post-decode.

- [ ] **Step 7: Run the header gate**

```bash
cd /Users/gael/projects/rustty && cargo build -p rustty-vt-c --release 2>&1 | tail -5
cd /Users/gael/projects/rustty && .port/tools/gates/check_rustty_vt_headers.sh 2>&1 | tail -20
```

Expected: `VT-HEADERS: GREEN`. The gate's step (a) syntax-checks the headers,
step (b) cross-checks exported symbols, step (c) checks struct layouts — none of
these hard-code the kitty enum members, so **no edit to
`check_rustty_vt_headers.sh` or `check-vt-struct-layouts.mjs` is required**;
confirm that by re-reading them before concluding:

```bash
grep -n "KITTY" /Users/gael/projects/rustty/.port/tools/gates/check_rustty_vt_headers.sh \
                /Users/gael/projects/rustty/.port/tools/check-vt-struct-layouts.mjs
```

Expected: no output. If there IS output, extend whichever list it belongs to
with the two new members and re-run the gate.

- [ ] **Step 8: Extend the C example** — `example/c-vt-kitty-graphics/src/main.c`
      already uses `RUSTTY_KITTY_GRAPHICS_DATA_PLACEMENT_ITERATOR` (:135) and
      `RUSTTY_KITTY_IMAGE_DATA_DATA_LEN` (:180). Port upstream's example hunk:

```bash
git -C /Users/gael/projects/ghostty show bdc0b6c19 -- example/c-vt-kitty-graphics/src/main.c
```

Apply the equivalent (a `uint64_t gen` query plus its printout), renamed per R2.

- [ ] **Step 9: Run the vt-c tests and the workspace build**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt-c 2>&1 | tail -10
cd /Users/gael/projects/rustty && cargo build --workspace 2>&1 | tail -5
cd /Users/gael/projects/rustty && grep -rn "transmit_time" crates/
```

Expected: tests PASS, workspace builds, no `transmit_time` anywhere.

- [ ] **Step 10: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-vt-c/src/kitty_graphics.rs include/rustty/vt/kitty_graphics.h \
        example/c-vt-kitty-graphics/src/main.c
git commit -m "vt-c: expose kitty graphics + image generation stamps (BL-6 bdc0b6c19, WS8 T16)"
```

---

## tabstops PARTIAL closure (`91f0cf67d`)

### Task 17: Make the `Tabstops::resize` tripwire arm-able from `terminal.rs`

**DECISIVE VERDICT (verified against current `main`): this is a VISIBILITY gap,
not an architectural one, and it closes ENTIRELY.** `Tabstops::init`
(`tabstops.rs:132-141`) already calls `self.resize(alloc, cols)`, and
`Tabstops::resize` (`:241-293`) already has a `#[cfg(test)]` tripwire checkpoint
at exactly the allocation point (`resize_tw_check(ResizeFp::DynamicAlloc)?`,
`:260`). `Terminal::resize` (`terminal.rs:4442-4457`) calls `Tabstops::init`, so
the existing checkpoint is ALREADY on the exact path upstream's `FailingAllocator`
test exercises. The allocate-before-deinit parity fix from `91f0cf67d` is
likewise already in place at `terminal.rs:4442-4457`. The ONLY missing piece is
that `ResizeFp` and `RESIZE_TW` are private to `tabstops.rs`, so `terminal.rs`'s
tests cannot arm them.

After this task, **nothing from `91f0cf67d` remains recorded as debt** — the
`PARTIAL` row in `.port/phase-2/MAIN_PARITY.md` §1 and its §3 twin both become
fully-ported strikes (Task 19).

**Files:**

- Modify: `crates/rustty-vt/src/tabstops.rs:330-347` (visibility + accessors)
- Modify: `crates/rustty-vt/src/terminal.rs:4438-4441` (stale PORT NOTE)
- Test: `crates/rustty-vt/src/terminal.rs`, `mod tests`

**Interfaces:**

- Produces (consumed by the test in this task):
  - `#[cfg(test)] pub(crate) enum crate::tabstops::ResizeFp { DynamicAlloc }`
  - `#[cfg(test)] pub(crate) fn crate::tabstops::resize_tw_error_always(point: ResizeFp)`
  - `#[cfg(test)] pub(crate) fn crate::tabstops::resize_tw_end(mode: ResetMode) -> Result<(), UntrippedError>`
- Consumes: `Terminal::resize(&mut self, alloc: Allocator, cols: CellCountInt, rows: CellCountInt)`
  (`terminal.rs:4414`), `Tabstops::get(&self, col: usize) -> bool` (`tabstops.rs:204`),
  `TABSTOP_INTERVAL` (the reference's interval-8 default).

- [ ] **Step 1: Read the oracle commit**

```bash
git -C /Users/gael/projects/ghostty show 91f0cf67d
```

- [ ] **Step 2: Write the failing test** in `terminal.rs`'s `mod tests`:

```rust
    // test "Terminal: resize preserves tabstops on allocation failure"
    #[test]
    fn terminal_resize_preserves_tabstops_on_allocation_failure() {
        let mut t = Terminal::init(Global, opts(10, 1)).unwrap();

        // PORT NOTE: the reference swaps in a `testing.FailingAllocator`. This
        // port's seam is the `Tabstops::resize` tripwire, whose only fail point
        // (`DynamicAlloc`) sits exactly at the dynamic-stops allocation that
        // `Tabstops::init` reaches from `Terminal::resize`. 513 columns is past
        // `PREALLOC_COLUMNS`, so the allocation IS attempted.
        crate::tabstops::resize_tw_error_always(crate::tabstops::ResizeFp::DynamicAlloc);
        assert_eq!(Err(OutOfMemory), t.resize(Global, 513, 1));
        crate::tabstops::resize_tw_end(rustty_util::tripwire::ResetMode::Reset).unwrap();

        // The original Tabstops was never replaced: cols is unchanged and the
        // original interval-8 stop is still present.
        assert_eq!(10 as size::CellCountInt, t.cols);
        assert!(t.tabstops.get(8));

        t.deinit(Global);
    }
```

- [ ] **Step 3: Run to verify it fails**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib terminal_resize_preserves_tabstops 2>&1 | tail -20
```

Expected: COMPILE FAIL — `enum ResizeFp is private`, `function
resize_tw_error_always not found in module crate::tabstops`.

- [ ] **Step 4: Widen the visibility and add the accessors** —
      `crates/rustty-vt/src/tabstops.rs:330-347`:

```rust
// PORT NOTE: tripwire fail-point enum + thread-local module for `resize`,
// mirroring the reference `resize_tw = tripwire.module(enum { dynamic_alloc },
// resize)`. Only compiled in test builds; see the PORT NOTE on `resize`.
//
// `pub(crate)` (not file-private) so `terminal.rs`'s
// `terminal_resize_preserves_tabstops_on_allocation_failure` test can arm this
// fail point: `Terminal::resize` -> `Tabstops::init` -> `Tabstops::resize`
// reaches it, which is exactly the path upstream 91f0cf67d's FailingAllocator
// test exercises.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum ResizeFp {
    DynamicAlloc,
}

#[cfg(test)]
thread_local! {
    pub(crate) static RESIZE_TW:
        core::cell::RefCell<rustty_util::tripwire::Module<ResizeFp, OutOfMemory>> =
        core::cell::RefCell::new(rustty_util::tripwire::module());
}

// `try tw.check(point)` for the `resize` tripwire module.
#[cfg(test)]
#[inline]
fn resize_tw_check(point: ResizeFp) -> Result<(), OutOfMemory> {
    RESIZE_TW.with(|tw| tw.borrow_mut().check(point))
}

/// `resize_tw.errorAlways(point, error.OutOfMemory)` — cross-module accessor so
/// `terminal.rs`'s tests can arm this module's fail points.
#[cfg(test)]
pub(crate) fn resize_tw_error_always(point: ResizeFp) {
    RESIZE_TW.with(|tw| tw.borrow_mut().errorAlways(point, OutOfMemory));
}

/// `resize_tw.end(mode)` — cross-module accessor (see above).
#[cfg(test)]
pub(crate) fn resize_tw_end(
    mode: rustty_util::tripwire::ResetMode,
) -> Result<(), rustty_util::tripwire::UntrippedError> {
    RESIZE_TW.with(|tw| tw.borrow_mut().end(mode))
}
```

Then update `tabstops.rs`'s own test at `:519-528` to go through the new
accessors, so there is exactly one arming idiom:

```rust
        // resize_tw.errorAlways(.dynamic_alloc, error.OutOfMemory);
        resize_tw_error_always(ResizeFp::DynamicAlloc);
        // const result = t.resize(testing.allocator, prealloc_columns * 2);
        let result = t.resize(Global, PREALLOC_COLUMNS * 2);
        // try testing.expectError(error.OutOfMemory, result);
        assert_eq!(Err(OutOfMemory), result);
        // try resize_tw.end(.reset);
        resize_tw_end(ResetMode::Reset).unwrap();
```

- [ ] **Step 5: Update the now-stale PORT NOTE** in
      `crates/rustty-vt/src/terminal.rs:4436-4441` — delete the parenthetical
      claiming the test hunk is unportable:

```rust
        // PORT NOTE (parity 91f0cf67d): allocate and initialize the replacement
        // FIRST, release the old tab stops only after allocation succeeds. A
        // failed resize retains the original tab stops and remains safe to
        // destroy. The upstream FailingAllocator test hunk is ported as
        // `terminal_resize_preserves_tabstops_on_allocation_failure`, arming the
        // `Tabstops::resize` tripwire fail point that this path reaches through
        // `Tabstops::init` (WS8 T17 — the MAIN_PARITY PARTIAL row is now closed).
```

- [ ] **Step 6: Run both tests**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib tabstops terminal_resize_preserves_tabstops 2>&1 | tail -10
```

Expected: all PASS.

- [ ] **Step 7: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-vt/src/tabstops.rs crates/rustty-vt/src/terminal.rs
git commit -m "tabstops: expose resize tripwire so terminal tests can arm it; closes 91f0cf67d PARTIAL (WS8 T17)"
```

---

## Finalization

### Task 18: Both-platform build gate + test lanes + acyclic gate

**Files:** none (verification only)

- [ ] **Step 1: macOS workspace build**

```bash
cd /Users/gael/projects/rustty && cargo build --workspace 2>&1 | tail -5
```

Expected: `Finished` with no errors and no new warnings.

- [ ] **Step 2: Linux workspace build**

```bash
ssh rvm "cd ~/projects/rustty && git fetch origin && git checkout ws8-upstream-small-sweep && git pull && cargo build --workspace 2>&1 | tail -5"
```

Expected: `Finished`. (This is the gate that catches the per-OS `madvise` cfg
arms from Task 12 — the Linux `MADV_DONTNEED` branch never compiles on macOS.)

- [ ] **Step 3: Full test lanes on macOS**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-vt --lib 2>&1 | tail -5
cd /Users/gael/projects/rustty && cargo test -p rustty-vt-c 2>&1 | tail -5
cd /Users/gael/projects/rustty && cargo test -p rustty-renderer 2>&1 | tail -5
```

Expected: all green. `rustty-vt --lib` count = 2035 baseline + 11 (T2 OSC 72)

- 1 (T5) + 3 (T6) + 4 (T7) + 1 (T8) + 2 (T9) + 2 (T10) + 3 (T11) + 1 (T12)
- 3 (T14 storage) + 2 (T14 exec) + 1 (T17) = **2069 passed** (the T13 RSS probe
  is `#[ignore]`d and does not count). If the number differs, reconcile before
  proceeding — do not adjust the expectation to match.

* [ ] **Step 4: Full test lanes on Linux**

```bash
ssh rvm "cd ~/projects/rustty && cargo test -p rustty-vt --lib 2>&1 | tail -5 && cargo test -p rustty-vt-c 2>&1 | tail -5 && cargo test -p rustty-renderer 2>&1 | tail -5"
```

Expected: identical counts, all green.

- [ ] **Step 5: Acyclic crate-graph gate**

```bash
cd /Users/gael/projects/rustty && .port/tools/gates/check_acyclic.sh 2>&1 | tail -10
```

Expected: GREEN. (WS8 adds no new crate edges — BL-6 only touches crates that
already depend on `rustty-vt`.)

- [ ] **Step 6: Header/ABI gate**

```bash
cd /Users/gael/projects/rustty && .port/tools/gates/check_rustty_vt_headers.sh 2>&1 | tail -10
```

Expected: `VT-HEADERS: GREEN`.

- [ ] **Step 7: vt Miri lane, for no NEW regressions**

```bash
cd /Users/gael/projects/rustty && .port/tools/miri-run.sh -p rustty-vt --lib 2>&1 | tail -30
```

Expected: the SAME pre-existing failures as the pre-WS8 baseline — specifically
the 9 documented search-subsystem Stacked-Borrows failures (memory:
`search-screen-sb-ub-finding`), which WS8 does not fix and must not be blocked
on. Any NEW `Undefined Behavior` diagnostic in `terminal.rs`, `screen.rs`,
`selection.rs`, `selection_gesture.rs`, `pagelist/`, or `kitty/` is a WS8
regression and MUST be fixed before Task 19. Capture the before/after failure
lists and diff them:

```bash
cd /Users/gael/projects/rustty && git stash list  # ensure clean
# baseline for comparison:
git -C /Users/gael/projects/rustty worktree add /tmp/ws8-baseline main 2>/dev/null || true
cd /tmp/ws8-baseline && /Users/gael/projects/rustty/.port/tools/miri-run.sh -p rustty-vt --lib 2>&1 | grep -E "^(test .* FAILED|error: Undefined Behavior)" | sort > /tmp/ws8-miri-before.txt
cd /Users/gael/projects/rustty && .port/tools/miri-run.sh -p rustty-vt --lib 2>&1 | grep -E "^(test .* FAILED|error: Undefined Behavior)" | sort > /tmp/ws8-miri-after.txt
diff /tmp/ws8-miri-before.txt /tmp/ws8-miri-after.txt
git -C /Users/gael/projects/rustty worktree remove /tmp/ws8-baseline
```

Expected diff: additions only where a NEW passing test appears; no new failures.

- [ ] **Step 8: No commit** — this task produces evidence, not changes. Record
      the six command outputs in the Task 19 commit body.

---

### Task 19: Strike the landed hashes in `.port/phase-2/MAIN_PARITY.md`

**Files:**

- Modify: `.port/phase-2/MAIN_PARITY.md`

- [ ] **Step 1: Strike the §1 PARTIAL row.** Replace the `91f0cf67d` row at
      `MAIN_PARITY.md:34` — the PARTIAL debt is fully closed by Task 17, so it moves
      out of §1 entirely (§1 lists open partials). Delete the row from §1 and mark
      the §3 row as ported (Step 3 below).

- [ ] **Step 2: Update the BL bullets** at `MAIN_PARITY.md:50-55`:

- BL-2 (`:50`) → mark **PORTED (WS8 T12/T13)**, noting the Miri exception is
  registered in `MIRI_EXCEPTIONS.md` "Category 2" and the RSS probe is the
  empirical verification.
- BL-3 (`:51`) → keep as an open bucket, but note that the 12 commits listed in
  Step 3 landed in WS8 and the remaining 17 rows are the follow-up pass.
- BL-6 (`:54`) → mark **PORTED (WS8 T14/T15/T16)**; update the evidence line to
  `grep -rn transmit_time crates/` → 0 hits.
- BL-7 (`:55`) → mark **PORTED (WS8 T2-T4)**; evidence
  `grep -r kitty_dnd crates/rustty-vt/src` → non-zero hits.

- [ ] **Step 3: Strike exactly these §3 rows** (and no others — the rows listed
      in "Scope of this plan revision" stay `pending-small BL-3`):

| Hash        | New status                                                                                                                                                                   |
| ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cfce1cd56` | ported (WS8 T6) — R1 fidelity reorder; UAF was already unreachable in rustty (owned `Vec<u8>` dupe in `start_hyperlink`), regression test ported                             |
| `04810273a` | ported (WS8 T6)                                                                                                                                                              |
| `1d5b6f90e` | ported (WS8 T7)                                                                                                                                                              |
| `a14a619ce` | ported (WS8 T7)                                                                                                                                                              |
| `fdd255c78` | ported (WS8 T7)                                                                                                                                                              |
| `dace6d1c6` | ported (WS8 T7)                                                                                                                                                              |
| `0c299000f` | ported (WS8 T8)                                                                                                                                                              |
| `b6f34be44` | ported (WS8 T9)                                                                                                                                                              |
| `a9f5b7eba` | ported (WS8 T9)                                                                                                                                                              |
| `607160657` | ported (WS8 T10)                                                                                                                                                             |
| `fa8cae88b` | ported (WS8 T10)                                                                                                                                                             |
| `08e376f23` | ported (WS8 T11) — pt.2 fixed; pt.1 (`pixelFromFloat`) is a VERIFIED NON-ISSUE in Rust (`f64 as u32` is saturating + NaN-safe by RFC 2884), ported as a regression test only |
| `896aca499` | ported (WS8 T12/T13) — Miri exception registered (MIRI_EXCEPTIONS.md Category 2), verified by RSS probe on macOS + Linux                                                     |
| `bdc0b6c19` | ported (WS8 T14/T15/T16)                                                                                                                                                     |
| `8fe5913f7` | ported (WS8 T14) — resolve to "N/A: every rustty target reports `target_has_atomic=\"64\"`" or "ported" per Task 14 Step 2's actual outcome                                  |
| `91f0cf67d` | ported (WS8 T17) — PARTIAL closed; tripwire made `pub(crate)`, `terminal_resize_preserves_tabstops_on_allocation_failure` added                                              |
| `91c87e2cf` | ported (WS8 T2-T3)                                                                                                                                                           |
| `dd6c09bcf` | ported (WS8 T2-T3)                                                                                                                                                           |
| `a6132c18b` | ported (WS8 T2-T3)                                                                                                                                                           |
| `c488ccda6` | ported (WS8 T2-T3)                                                                                                                                                           |
| `688750903` | ported (n/a-merge; constituents landed WS8 T2-T4)                                                                                                                            |

- [ ] **Step 4: Check `DEFERRALS.md` and `VERIFICATION_DEBT.md`** for stale
      WS8-adjacent rows and update any that this WS closed:

```bash
grep -n "91f0cf67d\|transmit_time\|tabstops\|FailingAllocator\|madvise\|kitty_dnd" \
  /Users/gael/projects/rustty/.port/phase-2/DEFERRALS.md \
  /Users/gael/projects/rustty/.port/phase-2/VERIFICATION_DEBT.md
```

Update or delete every hit that WS8 resolved; leave the rest untouched.

- [ ] **Step 5: Verify no `libc` crate crept in**

```bash
cd /Users/gael/projects/rustty && grep -rn '^libc' crates/*/Cargo.toml Cargo.toml
```

Expected: no output.

- [ ] **Step 6: Commit** (paste Task 18's six gate outputs into the body):

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/MAIN_PARITY.md .port/phase-2/DEFERRALS.md \
        .port/phase-2/VERIFICATION_DEBT.md \
        docs/superpowers/plans/2026-07-23-ws8-upstream-small-sweep.md
git commit -m "parity: strike WS8 landed hashes (BL-2/BL-6/BL-7 + 12 BL-3 + 91f0cf67d) (WS8 T19)"
```

---

### Task 20: Merge to main and clean up

**Files:** none (git flow, per the roadmap: "branch per WS; merge to main only
with both platforms green").

- [ ] **Step 1: Re-confirm both platforms are green on the final tree**

```bash
cd /Users/gael/projects/rustty && cargo build --workspace 2>&1 | tail -3 && cargo test -p rustty-vt --lib 2>&1 | tail -3
ssh rvm "cd ~/projects/rustty && git pull && cargo build --workspace 2>&1 | tail -3 && cargo test -p rustty-vt --lib 2>&1 | tail -3"
```

Expected: both `Finished` + `test result: ok. 2069 passed`.

- [ ] **Step 2: Merge**

```bash
cd /Users/gael/projects/rustty
git checkout main && git pull
git merge --no-ff ws8-upstream-small-sweep -m "merge: WS8 upstream small-fix sweep (BL-2, BL-6, BL-7, 12 BL-3, 91f0cf67d)"
```

- [ ] **Step 3: Verify the merge result builds and tests clean**

```bash
cd /Users/gael/projects/rustty && cargo build --workspace 2>&1 | tail -3 && cargo test -p rustty-vt --lib 2>&1 | tail -3
```

Expected: `Finished`, `test result: ok. 2069 passed`.

- [ ] **Step 4: Delete the branch**

```bash
cd /Users/gael/projects/rustty && git branch -d ws8-upstream-small-sweep
ssh rvm "cd ~/projects/rustty && git checkout main && git pull && git branch -D ws8-upstream-small-sweep 2>/dev/null; true"
```

- [ ] **Step 5: Confirm the working tree is clean**

```bash
cd /Users/gael/projects/rustty && git status --short
```

Expected: no output.
