# Phase 5 assessment — area E-platform-tail: termio / cli / os / config / po / .github / typos / valgrind

Range: `12967b68f..683d8db64` (245 commits, 169 non-merge).
Scope (oracle paths): `src/termio/**`, `src/cli/**`, `src/os/**`, `src/config/**`,
`po/**`, `.github/**`, `typos.toml`, `valgrind.supp`, `src/App.zig`,
`src/main_ghostty.zig`.

Area log: `git -C ../ghostty log --oneline --no-merges 12967b68f..683d8db64 -- <paths>`
→ **65 commits**. Area diffstat: **21 files, +1,585 / −767**.
**Zero commits touched `src/App.zig` or `src/main_ghostty.zig`** in this range.

Rustty side: `crates/rustty-{termio,cli,os,config}`, `po/`, `.github/`,
`typos.toml`, `valgrind.supp` (repo root).

---

## 0. Coverage statement — read this before trusting any row below

**Exhaustive, not sampled** — the filter is exactly the scope path set above.
All 65 commits were enumerated with a per-commit `git show --stat` restricted
to my paths; hunk-level reads:

- **Read hunk-by-hunk (full diff text):** the range END-STATE diff
  (`git diff 12967b68f..683d8db64 -- <paths>`) for every file EXCEPT the four
  `po/*.po` catalogs — that is the complete end state of `src/termio/*` (all 5
  files incl. the two new stream_handler tests), `src/cli/*` (all 4),
  `src/os/*` (both), `src/config/*` (both, incl. the new Config test),
  `.github/*`, `typos.toml`, `valgrind.supp`. Additionally the per-commit
  intermediate diffs for `8c7a34d4c`, `e8d8945b5`, `bc2f7d7d2` (they rewrite
  functions introduced earlier in the same range) and `a436a9edc`.
- **po catalogs:** assessed by the Phase-4 par-1 mirror method (end-state byte
  mirror + identity transform), NOT line-read: per-commit stats, oracle range
  diff sizes (bg 1,009 / nl 999 / pt_BR 1,041 / zh_TW 979 diff lines), and
  presence probes (rustty `pt_BR.po` has 0 hits for the range's new
  "Redefinir" vs oracle's 7; all four rustty `PO-Revision-Date` headers equal
  the oracle's 12967b68f values, e.g. pt_BR `2026-02-18` vs oracle-HEAD
  `2026-08-17`). Byte-verifying foreign-language content is out of scope per
  the Phase-4 G precedent ("byte-mirroring is the contract").
- **rustty side:** per-upstream-change symbol greps plus targeted reads of the
  matching regions (`file:line` cited per row): `stream_handler.rs`,
  `termio_impl.rs` (prepareInput + threadEnter bodies read in full),
  `message.rs`, `thread.rs`, `shell_integration.rs`, `list_themes.rs`,
  `help.rs`, `ssh.rs`, `ssh_cache.rs`, `tempdir.rs`, `macos.rs`,
  `formatter_file.rs`, `value_types.rs` (Limit), `release-tip.yml`,
  `typos.toml`, `valgrind.supp`. Absence verdicts are grep-based
  (`kitty_clipboard|KittyClipboard|kitty_dnd|clipboard_write_limit` → 0 hits
  in rustty-termio and rustty-config). I did not read the area crates
  end-to-end.
- **Provenance checks run:** the OSC 5522 parser (`4eee1ac6a`) and OSC 72 dnd
  parser (`91c87e2cf`) are PRE-range (`git merge-base --is-ancestor` →
  ancestor of 12967b68f) — their rustty ports
  (`crates/rustty-vt/src/osc/parsers/kitty_{clipboard,dnd}_protocol.rs`) are
  Phase-4 work and are NOT evidence for this range. `typos.toml` at 12967b68f
  had no `Datas` (verified via `git show`). The oracle `test "cacheDir paths"`
  pre-dates the range (`67794d3f6`).

**Not run:** no builds, no test runs. `w0/g1b-after-at-683d8db64.txt` is
ground truth for test-count deltas.

**Superseded-chain handling (binding range-diff rule):** the Kitty clipboard
termio cluster is an 11-commit build-up chain
(`bcf44b40e` → `0ce9054bf` → `8c7a34d4c` → `af9470b19` → `25c61e852` →
`928c7f0e7` → `5350d4a5f` → `70f006575` → `e8d8945b5` → `bc2f7d7d2`, plus
`af8d28a94`) in which later commits rewrite earlier functions
(`kittyClipboardRead`'s mime dup form, `kittyClipboard`'s Metadata error
handling, `kittyClipboardWriteAlias`'s empty-mime path, `WriteState`
error-set). Assessed at END STATE; every commit and its test contribution is
listed. **No commit in the range was reverted outright.** The pt_BR po chain
(17× "Update po/pt_BR.po" + review-suggestion commits) is likewise pure
end-state; individual intermediates were not read.

---

## 1. Summary — the numbers

Phase 4's finale (d63dd9fc) absorbed everything through 12967b68f, so ABSENT
is the expected default — and that is what the evidence shows: **nothing from
this range has been ported except one .github mirror line** that arrived via
an independent path.

| Verdict | Count | Notes |
|---|---|---|
| **ABSENT** | **52** | Kitty clipboard termio/config cluster (11), cli list-themes preview (1), shell_integration (1), os/macos commonDir (1), typos (1), po (37) |
| **PARTIAL** | **4** | `9566a1a87`, `88dc6f972`, `1ffdd7415`, `c5f4f00ed` — in each, Rust ownership / prior N-A'd porting already provides the behavioral end state; the remainder is comment-mirror currency (or, for 88dc6f972, two of three files) |
| **PORTED** | **1** | `2303bcf08` — rustty `.github/workflows/release-tip.yml:461` already carries `# v7.0.1` on the identical pinned SHA (likely rustty's own pinact lane; end state byte-matches) |
| **N-A** | **8** | 8× VOUCHED.td updates — rustty carries no `.github/VOUCHED.td` (Phase-4 G precedent: superseded by rustty's `AI_POLICY.md`) |

**Headline gaps, ranked:**

1. **Kitty clipboard protocol termio integration (11 commits, ~560 net
   lines)** — the entire OSC 5522 handler layer in `stream_handler.zig`
   (read requests, chunked write transactions with base64 spooling, session
   password grants, permission-prompt exemption, EFBIG size limit), the
   `Termio.kittyClipboardGrant` seam, two new `Message` variants, two
   `Thread.drainMailbox` arms, and the `clipboard-write-limit-bytes` config
   option. Blocked on area A/D's NEW `terminal/kitty/clipboard*.zig`
   subsystem (the G1b `kitty drop=202` debt) and area F's apprt
   `ClipboardRequest.Kitty{Read,Write}`.
2. **po re-mirror (37 commits, 4 catalogs)** — bg, nl, pt_BR, zh_TW moved
   substantially (~1,000 diff lines each); rustty's catalogs sit at the
   12967b68f state. Mechanical via the Phase-4 par-1 method.
3. **cli list-themes preview** (`d9857eaba`) — the preview pane still renders
   Zig 0.13 sample code (rustty `list_themes.rs:1869,2033`); upstream
   rewrote it to valid Zig 0.16 (+4 lines of sample, pane height 24→28).
4. Small tail: shell_integration alloc-shape, os/macos commonDir signature,
   ssh/ssh_cache doc reformat, typos/valgrind mirrors, comment-currency
   partials.

**No security-critical or UB-class gap is open in this area for this range**
(contrast Phase 4's E29 UAF). The Kitty clipboard cluster is
security-RELEVANT (clipboard access policy) but fails CLOSED while absent:
rustty parses OSC 5522 and discards it
(`crates/rustty-vt/src/stream.rs:3469` lists
`osc::Command::KittyClipboardProtocol(_)` under ignored commands) — no
clipboard access happens at all.

---

## 2. Per-commit verdict tables

Legend: "blocked on" names the owning area whose primitive must land first.
Cost S/M/L; risk per PORT_RULES vocabulary.

### 2.1 Kitty clipboard protocol — termio + config cluster (atomic bundle KC)

All 11 rows: **ABSENT**. Shared evidence:
`grep -rn "kitty_clipboard\|KittyClipboard\|clipboard_write_limit" crates/rustty-termio/src crates/rustty-config/src` → 0 hits
(the only hits in the repo are the PRE-range OSC parser in rustty-vt).
`crates/rustty-termio/src/stream_handler.rs` has no kitty arms;
`message.rs` has no `KittyClipboardGrant`; `thread.rs` has no grant arms;
`crates/rustty-config/src/config_struct.rs` has no
`clipboard-write-limit-bytes` (Limit machinery exists at
`value_types.rs:45-54`). Ordered oldest-first (chain order):

| # | Commit | What changed (my paths only) | End-state disposition | Tests carried |
|---|---|---|---|---|
| P5-E1 | `bcf44b40e` "dispatch OSC 5522 as a kitty_clipboard stream action, unhandled" | stream_handler `execStreamAction` gains a no-op `.kitty_clipboard => {}` arm (the Action variant itself is A's `stream.zig`, in-range `f2d4b32be`). | Superseded: the arm became `try self.kittyClipboard(value)` (P5-E5). | none |
| P5-E2 | `0ce9054bf` "macos: implement Kitty clipboard protocol reads (OSC 5522)" | NEW `kittyClipboard`/`kittyClipboardRead`: Metadata.parse in a scratch arena; payload = requested MIME list (Payload.init, invalid → silent drop); targets-type `.` = listing; `max_read_mimes` cap; builds `apprt.ClipboardRequest.KittyRead` in a request-owned arena and sends `.kitty_clipboard_read` to the surface. (macOS half: area F.) | Read path survives, reshaped by E3/E4/E6/E9. | none |
| P5-E3 | `8c7a34d4c` "reads serve all clipboard content types" | `mimes` become `[:0]const u8` dupeZ'd into the arena (were arena-shared sub-slices); comment updated. | Final form. | none |
| P5-E4 | `af9470b19` "reads support pw/name session grants" | `kitty_clipboard_grants: Grants` field + deinit; `kittyClipboardGrant` on StreamHandler AND `Termio.kittyClipboardGrant` (locks renderer mutex); `Message.kitty_clipboard_grant_read/write: KittyClipboardGrant {alloc, pw}` + deinit arm; `Thread.drainMailbox` arms (free pw after use); fullReset clears grants; read path computes `granted` ("password without a name is no password"). | All survive; `granted` computation later split into `kittyClipboardReadGranted` (E6). | none |
| P5-E5 | `25c61e852` "macos: implement Kitty clipboard protocol writes" | Write transactions: `kitty_clipboard_write: ?*WriteState` field + deinit abort; `kittyClipboardWriteBegin` (replaces in-flight; `clipboard_write == .deny` → EPERM up-front), `WriteData` (chunk spool; empty mime = commit), `WriteAlias`, `WriteCommit`/`WriteSend` (arena-owned `ClipboardRequest.KittyWrite` → surface), `WriteFinish`/`WriteStatus` (Response.encode → `.write_alloc` reply), `WriteAbort`. | Survives; error routing refined by E8/E10/E11. | none |
| P5-E6 | `928c7f0e7` "exempt listing reads from permission prompts" | `kittyClipboardReadGranted(pw, mimes_len)`: `readPromptExempt(mimes_len)` short-circuit so a listing never burns a one-time grant. | Final form. | NEW `test "kitty clipboard read: targets-only never consumes a one-time grant"` (4 expects) |
| P5-E7 | `5350d4a5f` "make write limit configurable" | Config: NEW `@"clipboard-write-limit-bytes": Limit(usize, 64*1024*1024) = .default` + 26-line doc; `Termio.DerivedConfig.clipboard_write_limit` + init/changeConfig threading; StreamHandler field + doc. | Final form. | NEW Config test `test "clipboard write limit"` (3 expects: default / 1234 / unlimited=maxInt) |
| P5-E8 | `70f006575` "reject oversized writes" | WriteBegin passes `.{ .max_size = self.clipboard_write_limit }`; WriteData maps `error.TooLarge` → EFBIG finish, OOM → EIO finish then re-raise; Config doc reworded (EFBIG semantics). | Final form. | NEW `test "kitty clipboard write: oversized text replies EFBIG"` (SPSC mailbox, 4-byte limit, base64 "Hell"+"o", asserts the encoded `\x1B]5522;type=write:status=EFBIG:id=macos\x1B\\` reply and empty transaction) |
| P5-E9 | `e8d8945b5` "text input validation to spec" | Metadata.parse errors split: `InvalidValue` on wdata/walias aborts the in-flight transaction with EINVAL (via `Metadata.operation`); read payload gains `isValidUtf8()` gate; walias empty-mime now EINVAL-aborts (was silent ignore). | Final form. | none (E8's test unchanged) |
| P5-E10 | `bc2f7d7d2` "base64 handling to spec" | WriteData/WriteCommit gain `error.Invalid` → EINVAL-abort arms (bad base64 chunk stream; bad final padding); `kittyClipboardWriteSend` error set gains `Invalid`. | Final form (the end state read in §0). | none |
| P5-E11 | `af8d28a94` "terminal/kitty: drag and drop stream handler" | stream_handler: `.kitty_dnd,` added to the Unimplemented arm group (next to `.title_push/.title_pop`). (The dnd subsystem + Action variant: area A/D.) | Final form. | none |

**Cost/Risk for the whole cluster: L / MEDIUM.** Port the END STATE only —
E1's no-op arm, E2's `try`-ing Metadata.parse, E3's pre-dupeZ mime slices,
E5's pre-`max_size` WriteState.init, and E9-pre alias semantics are all
superseded-commit traps. Blocked on: **A/D** (`terminal/kitty/clipboard*.zig`
types: `Grants`, `WriteState`, `Metadata`, `Payload`, `Response`, `Status`,
`readPromptExempt`, `max_read_mimes`, `targets_mime` — the G1b `kitty`
drop=202 debt — plus `stream.zig`'s `.kitty_clipboard`/`.kitty_dnd` Action
variants, `f2d4b32be`/dnd, part of the `stream_terminal` drop=38 debt) and
**F/apprt** (`apprt.ClipboardRequest.KittyRead/KittyWrite` structs,
`.kitty_clipboard_read/_write` surface messages, the surface-thread policy +
macOS clipboard impl). The config option (P5-E7's Config half) is
freestanding and can land first with its test.

### 2.2 termio — non-kitty

| # | Commit | What changed | Verdict | Evidence | Tests carried | Cost/Risk |
|---|---|---|---|---|---|---|
| P5-E12 | `9566a1a87` "termio: release initial input resources" | `prepareInput` → ArrayList w/ `errdefer` closing already-opened files; `.raw` no longer arena-duped (borrows); `Input.deinit()` (closes file); `threadEnter` gains `defer` deinit-ing all inputs; `.file` arm restructured with `defer self.alloc.free(contents)` after queueWrite. | **PARTIAL** | Behavior: **N-A by Rust ownership, all paths verified** — `Input::File(std::fs::File)` closes on drop; an error return from `prepareInput` drops the partially-built `Vec` (`termio_impl.rs:225-247`); `threadEnter` consumes `inputs` via `into_iter().flatten()` and any error path drops the remainder (`termio_impl.rs:882-921`); file contents (`read_input_file` → owned `bytes`) drop at arm end. **ABSENT:** (a) the mirrored Zig comment blocks still quote the OLD reference body (`termio_impl.rs:758-796` quotes the pre-range `threadEnter`, `:224` note and `:228` mirror quote the old `alloc.dupe` form) — R1 comment currency / D9; (b) rustty still CLONES raw inputs (`Input::String(v.clone())`, `:230`) where upstream stopped duplicating — alloc-parity divergence, mark `PERF(port)` or borrow. | none | S / NONE (comment + note only) |
| P5-E13 | `8e8d76b63` "shell-integration: avoid owning temporary commands" | `setupBash`/`setupNushell`: `dupeZ(try cmd.toOwnedSlice())` → `dupeZ(cmd.buffer.written())` — dupe from the borrowed buffer instead of materializing an owned intermediate. | **ABSENT** | `crates/rustty-termio/src/shell_integration.rs:533,718` still `cmd.toOwnedSlice()` with the OLD mirror comment (`:528,717`). Rust shape: dupe from the builder's borrowed bytes; drops one allocation per setup. | none (existing `bash`/`nushell` KATs cover output bytes — identical output) | S / NONE |

### 2.3 cli

| # | Commit | What changed | Verdict | Evidence | Tests carried | Cost/Risk |
|---|---|---|---|---|---|---|
| P5-E14 | `d9857eaba` "cli: update list-themes preview sample to valid Zig 0.16 syntax" | Preview pane: sample program rewritten for Zig 0.16 (`main(init: std.process.Init)`, buffered `std.Io.File.stdout().writer(init.io, &buf)`, `w.writeAll`, trailing `try w.flush()`); 4 extra sample lines; pane height 24→28; row_offsets reflowed; prompt version `v0.13.0`→`v0.16.0`. | **ABSENT** | `crates/rustty-cli/src/list_themes.rs:1869` still `std.Io.getStdOut().writer()` sample text, `:2033` still `" v0.13.0 "`; height still 24. | none (`cli_list_themes` 0 zig / 12 rust — rustty's own render tests; re-golden if they assert pane content) | S / NONE — mechanical content mirror, but check rustty's 12 list_themes tests for preview-content assertions before/after. |
| P5-E15 | `88dc6f972` "docs: reformatting for help book support" | help.zig: `` `-h, `--help` `` → `` `-h`, `--help` `` (backtick fix). ssh.zig + ssh_cache.zig: example blocks re-indented 3→5 spaces (markdown code-block for the help book) + blank line after "Examples:" in ssh_cache. | **PARTIAL** | help hunk: **already end-state** — `crates/rustty-cli/src/help.rs:51` has the corrected backticks (pre-existing; independent fix or transcription-correct Phase-4 port). ssh/ssh_cache hunks: **ABSENT** — `ssh.rs:305` and `ssh_cache.rs:141-147` still 3-space indent, no blank line. | none | S / NONE. NOTE: these doc comments feed `help_strings` generation — verify rustty's helpgen output shape after re-indenting (the extra indent turns them into code blocks). |

### 2.4 os

| # | Commit | What changed | Verdict | Evidence | Tests carried | Cost/Risk |
|---|---|---|---|---|---|---|
| P5-E16 | `1ffdd7415` "os: remove flaky TempDir handle checks" | Both TempDir tests DROP the fd-BADF `fcntl(F.GETFD)` probes (fd numbers can be reused → flaky) and gain the comment "Don't check the raw handles after close: descriptor numbers can be reused immediately, so a valid number does not imply that we leaked it."; `builtin` import removed. On-disk assertions kept. | **PARTIAL** | Behavior: **already end-state** — rustty's tempdir tests never ran the probes; they exist only as a commented-out N-A mirror block with a PORT NOTE (`crates/rustty-os/src/tempdir.rs:289-343`, path-model rationale). **ABSENT:** the mirror text is now STALE — upstream deleted the probe code the comment block quotes; replace the quoted-probe block + PORT NOTE with the new upstream comment (the N-A rationale happens to be upstream's own reason now). R1 comment currency / D9. | test bodies shrink (assertion-for-assertion: the on-disk `FileNotFound` / retained-dir asserts remain; the BADF expects are DELETED upstream — rustty must not resurrect them) | S / NONE |
| P5-E17 | `a436a9edc` "macos: avoid temporary path component allocation" | `commonDir(alloc, directory, sub_paths: []const []const u8)` → `(…, sub_path: []const u8)`; callers stop building `&.{bundle_id, sub_path}`; the temp `paths` array alloc/`@memcpy`/free is replaced by a single `std.fs.path.join(alloc, &.{base_dir, build_config.bundle_id, sub_path})`. | **ABSENT** | `crates/rustty-os/src/macos.rs:138-140` still `sub_paths: &[&str]`; call sites `:54,:70` still pass `&[build_config::BUNDLE_ID, sub_path]`; `:176-178` still builds the temp `Vec<&str>`. | none (the pre-range `cacheDir paths` test is context, untouched) | S / LOW — output bytes identical; existing path tests are the guard. |

### 2.5 config + valgrind

| # | Commit | What changed | Verdict | Evidence | Tests carried | Cost/Risk |
|---|---|---|---|---|---|---|
| P5-E18 | `c5f4f00ed` "fix up recent Valgrind errors" | (a) formatter_file.zig: `if (default) \|d\|` → `\|*d\|` — capture by pointer so `d.changed(...)` stops copying the whole Config by value (undefined padding tripped Valgrind). (b) valgrind.supp: NEW suppression for Zig std DEFLATE's intentional undefined `base[0]` increment (`fun:compress.flate.Compress.huffman.buildValues`). | **PARTIAL** | (a) code: **N-A structurally** — rustty already borrows: `crates/rustty-config/src/formatter_file.rs:110` `if let Some(d) = &default { d.changed(self.config, key) }`; Config is not `Copy`, the by-value copy cannot exist. **ABSENT:** the mirrored Zig comment `:109` still quotes the old `\|d\|` — update to `\|*d\|` (D9). (b) **N-A** — the suppressed frame is a Zig-stdlib symbol that can never appear in a rustty binary (rustty `valgrind.supp` is the rebranded GTK-runtime mirror; adding a Zig `compress.flate` frame would be dead text). Record the divergence: rustty's valgrind.supp intentionally omits Zig-toolchain-only suppressions. | none | S / NONE |
| — | (`70f006575`/`5350d4a5f` Config.zig hunks) | counted in §2.1 rows P5-E7/E8 (`clipboard-write-limit-bytes` option + doc + test). | ABSENT | see P5-E7 | `test "clipboard write limit"` | — |

### 2.6 .github + typos

| # | Commit | What changed | Verdict | Evidence | Cost/Risk |
|---|---|---|---|---|---|
| P5-E19 | `2303bcf08` "ci: update actions/upload-artifact pinned tag comment" | release-tip.yml: `# v6.0` → `# v7.0.1` on the SAME pinned SHA `043fb46d…`. | **PORTED** (equivalent) | rustty `.github/workflows/release-tip.yml:461` — identical SHA, comment `# v7.0.1`. rustty runs its own pinact (`.github/pinact.yml`), which is the likely independent source; either way the end state byte-matches the oracle hunk. Guard: pinact CI keeps SHA↔tag comments consistent. | 0 |
| P5-E20 | 8× "Update VOUCHED list" / "Sync CODEOWNERS vouch list": `bb00a5c98` `a36dc245b` `5834a0e3d` `da0093671` `d03cd1f53` `2021d2add` `74a133ea1` `311d38376` | `.github/VOUCHED.td` roster additions (11 lines total incl. one struck entry). | **N-A** ×8 | rustty carries no `.github/VOUCHED.td` (`ls .github` verified); Phase-4 G precedent (its §"28× Update VOUCHED list" row): the vouched-contributor roster is ghostty's AI-policy instrument, superseded by rustty's own `AI_POLICY.md`. Classified by filename (single-file commits), same as Phase 4. | 0 |
| P5-E21 | `1bc188739` "typos" | typos.toml: `Datas = "Datas"` added under "Swift oddities" (guards Swift `…Datas` identifiers). | **ABSENT** | rustty `typos.toml` has no `Datas` (grep verified; absent at 12967b68f upstream too, so this is range work). Mirror the line — rustty's typos.toml is a live mirror and the macOS-Swift tier will hit the same identifiers. | S / NONE |

### 2.7 po — 4 catalogs, 37 commits, end-state mirror (Phase-4 par-1 method)

All 37: **ABSENT** (single mechanical work item: re-mirror bg/nl/pt_BR/zh_TW
from the oracle at 683d8db64 via the learned identity transform —
`io.airely.rustty` domain, R2 per-token casing renames
(memory `rename-casing-fidelity`), translator identities verbatim — then
`msgfmt --check` all four + the apprt `i18n_msgid_coverage` and
`xtask translations` gates).

Evidence of absence: rustty `po/{bg,nl,pt_BR,zh_TW}.po` `PO-Revision-Date`
headers all equal the oracle's 12967b68f values (bg `2026-02-09`,
nl `2026-02-18`, pt_BR `2026-02-18`, zh_TW `2026-02-18`) vs oracle-HEAD
(bg `2026-08-13`, nl `2026-08-14`, pt_BR `2026-08-17`, zh_TW `2026-08-14`);
range-distinctive string "Redefinir": 0 in rustty pt_BR vs 7 in oracle.

| Catalog | Commits (end state) |
|---|---|
| `pt_BR.po` (31) | `ece96a14c` (base update) → `8a0a9faf8` ("Redefinir" for resets) → 25× single/small `Update po/pt_BR.po` (`6ab3ef947` `123dc0ccb` `1c34e0df8` `6c29a0a28` `d6871cd92` `6fe197d86` `1225632d1` `31a10a7b5` `1b8da6b40` `58c4796c1` `0149ef5f7` `17a66279b` `be892fc6c` `cdefb6809` `ed13a09a5` `e49392671` `e820ef48b` `03cb485f6` `3bdbad7d8` `132f154e5` `65621664e` `9581f7dd4` `430202099` `5a55a345a` `ae7080e6c`) → `bde2047a6` `d2f08cb0b` (review batches) → `e8a7097f6` ("aplicativo") → `959c0daea` (revision date). Pure end-state; the intermediates are the textbook superseded-chain. |
| `zh_TW.po` (4) | `08c80d253` (missing translations) → `08df9f6e2` (EOF blank line) → `b5716a871` (review refinements) → `f0eaaea6f` (reset/copy/placeholder refinements) |
| `nl.po` (1) | `fc5ac17a6` (complete Dutch for v1.4) |
| `bg.po` (1) | `bfc200c85` (bg_BG update) |

(31 + 4 + 1 + 1 = 37, matching `git log --oneline --no-merges
12967b68f..683d8db64 -- po/<file>` per catalog; all four files are covered
by the one re-mirror.)

Cost: S / NONE (mechanical; msgfmt is the gate). No msgid changes in-range
(no `.pot` diff), so `i18n_msgid_coverage` should be unaffected — verify.

---

## 3. Partials — the exact missing hunks

1. **P5-E12 (`9566a1a87`)** — update the mirrored Zig comment blocks in
   `crates/rustty-termio/src/termio_impl.rs` (`:224-231` prepareInput mirror;
   `:758-796` threadEnter mirror; the in-body `.file`-arm mirror `:889-900`)
   to the new reference text; decide the `.raw` no-dupe (borrow vs
   `PERF(port)` on the clone at `:230`).
2. **P5-E15 (`88dc6f972`)** — re-indent the Examples blocks in
   `crates/rustty-cli/src/ssh.rs:303-315` and `ssh_cache.rs:140-147`
   (+ blank line after "Examples:" in ssh_cache); help.rs already done.
3. **P5-E16 (`1ffdd7415`)** — in `crates/rustty-os/src/tempdir.rs:289-343`
   replace the commented-out BADF-probe mirror + its N-A PORT NOTE with the
   new upstream comment (both tests).
4. **P5-E18 (`c5f4f00ed`)** — one comment token in
   `crates/rustty-config/src/formatter_file.rs:109` (`|d|` → `|*d|`).

## 4. Deletions

Upstream deleted **no files in my path set** this range (matches the
phase-wide "no upstream deletions" note in the Phase-5 README). No rustty
deletions required; no G-OSDEL impact.

## 5. Atomic bundles, cross-area couplings, SB/UB hazards

**Bundles:**

- **KC (Kitty clipboard)** — P5-E1..E10 land together as one end state, and
  only after/with: **A/D**'s `terminal/kitty/clipboard*.zig` port (Grants /
  WriteState / Metadata / Payload / Response / Status + their 5 files' tests
  = the bulk of G1b `kitty drop=202`) and the `stream.zig`
  `.kitty_clipboard` Action variant; **F/apprt**'s
  `ClipboardRequest.KittyRead/KittyWrite` + surface messages + macOS policy/
  clipboard impl. Partial landings are worse than absent: a handler without
  the surface side would ACK protocol operations it cannot perform (today's
  parse-and-discard fails closed). The Config half (P5-E7 option + test) is
  freestanding and may land first.
- **DND-arm** — P5-E11's one line lands with A/D's `.kitty_dnd` Action
  variant (their `kitty/dnd*.zig` + stream dispatch); nothing else in my
  area depends on it.
- **po re-mirror** — one commit, all four catalogs (msgfmt gate).

**Couplings by area:**

- **A/D (terminal):** everything in bundle KC above; the `.kitty_dnd`
  variant; note `Metadata`/osc kitty_metadata is part of their G1b `osc
  drop=47` debt.
- **F (apprt/macos):** `ClipboardRequest.KittyRead/KittyWrite` (arena-owned
  in Zig — see hazards), `.kitty_clipboard_read/_write` surface message arms,
  permission-prompt policy, macOS pasteboard read/write impl (the "macos:"
  halves of E2/E4/E5 live in `macos/` — area F's file set).
- **C (lib/capi):** none found in my hunks this range.
- **G:** none — `src/config` is in MY file set this phase (option + test are
  rows P5-E7/E8).

**SB/UB hazards (R9/R11):**

1. **Arena-crossing ClipboardRequest (E2/E5): do NOT mirror the Zig shape.**
   Upstream allocates the request struct FROM an arena, then moves the arena
   INTO that same struct ("The arena must be copied in last") and sends the
   pointer cross-thread; the surface deinits the arena to free everything. A
   literal Rust translation is a self-referential owner (UB-prone and
   un-droppable). The honest Rust analogue: the request struct OWNS its
   `Vec`/`CString` fields (+ the struct itself boxed into the surface
   message); Drop replaces `arena.deinit()`. PORT NOTE required; the
   cross-thread handoff is a move of a `Send` value — no raw ptr needed.
   Coordinate the deinit contract with F (surface must free exactly once —
   ownership makes double-free unrepresentable, but the F-side message enum
   must take the Box by value, not a raw ptr).
2. **`kitty_clipboard_write: ?*WriteState`** (manual create/destroy +
   `errdefer`) → `Option<Box<WriteState>>`; the E5 abort/finish paths become
   `take()` + drop. No raw-ptr discipline needed; do not introduce one.
3. **`Termio.kittyClipboardGrant`** locks `renderer_state.mutex` then calls
   into `terminal_stream.handler` — same raw shared-state pattern as
   `colorSchemeReport` (existing precedent `termio_impl.rs` queueMessage
   `(*self.renderer_state).mutex`); reuse that exact seam, no new aliasing.
4. **EFBIG test construction:** upstream builds `var handler: StreamHandler
   = undefined;` and initializes 6 fields. Rust cannot mirror `undefined` —
   the test needs a minimal constructed handler (SPSC mailbox + renderer
   State + locked mutex, as the Zig test does explicitly). Watch that the
   renderer `State{ .terminal = undefined }` field maps to rustty's existing
   test seam for renderer state (grep the stream_handler tests F/A wrote in
   Phase 4 for precedent) — no `MaybeUninit` in tests.
5. No Page/Row/Cell/Screen raw-ptr surface anywhere in this area's range
   work; no decoder/iterator aliasing (the base64 spooling lives in A/D's
   WriteState).

## 6. G1b dropped-test mapping for this area

`w0/g1b-after-at-683d8db64.txt` RED modules — **none are owned by this
area**: `kitty` 202 / `osc` 47 / `stream_terminal` 38 / `input_paste` 7 /
`parser` 1 all live in `src/terminal` (areas A/D), including every test of
the kitty clipboard/dnd SUBSYSTEMS my handler consumes.

**Masking warning (memory `derived-gate-green-can-be-bookkeeping`): this
area's 3 new upstream tests are invisible to G1b.** The gate is count-based
per module and rustty's pre-existing rust-side surplus swallows them:

| Module | zig | rust | Status | Hidden debt |
|---|---|---|---|---|
| `termio_stream_handler` | 2 | 4 | ok | BOTH zig tests are the range's NEW kitty tests (grant-consumption, EFBIG) — rustty has neither; the 4 rust tests are different, pre-existing ones |
| `config_struct` | 116 | 154 | ok | includes the NEW `clipboard write limit` test — absent in rustty, masked by the 38-test surplus |

G1b can therefore reach GREEN at the finale with all three tests still
missing. The per-row "Tests carried" column in §2.1 is the authoritative
checklist; wire these three into the finale acceptance explicitly.

## 7. Notes for the porting workflow

- **Negative controls:** P5-E6 grant test (make `readPromptExempt` return
  false for mimes_len==0 → first expect fails); P5-E8 EFBIG (raise the limit
  to 5 → expectError arm fails); P5-E7 config test (change the default
  constant → first expect fails); P5-E17 (drop `bundle_id` from the join →
  path tests fail); po re-mirror (msgfmt --check on a truncated catalog
  fails).
- The kitty-clipboard chain refactors its own functions repeatedly — an
  agent porting commit-by-commit would write `kittyClipboardRead` three
  times. Range-diff end state ONLY (the §0 end-state read is the source).
- P5-E14's list_themes rewrite is content-only but large; rustty's 12
  `cli_list_themes` tests may golden-assert pane rows — regenerate, don't
  hand-patch (memory `rename-casing-fidelity`: cargo clean before goldens if
  identity-transformed).
- The `clipboard-write-limit-bytes` doc comment must keep upstream's
  cross-reference to OSC 52's separate hardcoded limit verbatim (R1), with
  ghostty→rustty token renames only; strip nothing (memory
  `strip-ghostty-gag-prose` does not apply — no gag prose here).
- `Limit(usize, 64*1024*1024)`: spell as the existing
  `Limit<usize, {64 * 1024 * 1024}>` const-generic (`value_types.rs:45-54`
  precedent); derived constant stays the same EXPRESSION (R8/D3), never
  `67108864`.
- P5-E19 is the range's only PORTED row and it arrived via rustty's own
  pinact — when re-vetting at the finale, do not count it as evidence that
  any porting lane ran.

*Assessed 2026-08-25 against oracle working tree at 683d8db64 (= origin/main). No code modified.*
