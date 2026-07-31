# Rustty non-macOS parity audit

Start with [`FINDINGS.md`](FINDINGS.md): it is the authoritative, deduplicated
catalog after independent cross-review.

- [`COVERAGE.md`](COVERAGE.md) records whole-tree inventory, ABI, behavioral,
  platform, and test coverage.
- [`VERIFICATION.md`](VERIFICATION.md) records the final fresh verification run.
- [`REJECTED_AND_UNRESOLVED.md`](REJECTED_AND_UNRESOLVED.md) records falsified
  theories, stale-gate false positives, and environmental limits.
- [`findings.json`](findings.json) is the machine-readable canonical index.
- [`BASELINE.md`](BASELINE.md) and [`AUDIT_PLAN.md`](AUDIT_PLAN.md) pin the
  compared revisions and audit method.
- `agents/` contains the independent workstream reports and cross-reviews.
- `scripts/` and `evidence/` contain audit-only reproducibility material.

No Rustty or Ghostty product source was changed. The canonical count is 20
verified gaps: 5 P1, 9 P2, 3 P3, and 3 P4.
