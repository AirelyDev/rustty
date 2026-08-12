# codex-vet — independent vetting + remediation of the Codex parity audit

Purpose: every severity-bearing finding reported by the Codex line-audit
(`.port/codex/line-audit/waves/INDEX.md`, 659 registry rows) is independently
vetted from primary sources, and every confirmed, fixable-now gap is remediated
and verified. This directory is the resumable ledger.

## Files

- `STATE.json` — the master state. One entry per finding row (179 rows,
  deduplicated into 147 vet groups under `groups`, keyed by tier P1–P4).
  Per-finding `vet` holds the independent verdict; `fix` holds remediation
  status. `workflows` records every Workflow runId dispatched.
- `README.md` — this file.

## Vocab

- `vet.verdict`: `confirmed` | `not_a_gap` | `already_fixed` |
  `needs_deeper_evidence`
- `fix.status`: `pending` | `fixed` | `verified` | `deferred_platform` |
  `known_deferred` | `wontfix_scope` | `no_fix_needed`

## Resume protocol

1. Read `STATE.json`. Any group whose lead finding has `vet == null` still
   needs vetting; any `vet.verdict == "confirmed"` with `fix == null` or
   `fix.status == "pending"` still needs remediation/verification.
2. Vet workflows: `.port/phase-2/wf-codexvet-vet.js`, invoked with
   `args = {tier, groups: [...]}` (groups built from `STATE.json`). Re-run for
   only the unvetted groups.
3. Fix workflows: `.port/phase-2/wf-codexvet-fix*.js` (authored per remediation
   batch after verdicts land).
4. After each workflow completes, its structured results are merged back into
   `STATE.json` (script: regenerate via the merge snippets in git history, or
   just update fields by hand — `STATE.json` is the single source of truth).

Repos: RUSTTY=/Users/gael/projects/rustty (live main),
GHOSTTY=/Users/gael/projects/ghostty (read-only oracle, pin 6ad1fe7d).
Rustty audit pin was dc054bb; vetting always re-checks current main.
