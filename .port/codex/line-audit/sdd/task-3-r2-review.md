# Task 3 scoped re-review — fix round 2

Spec compliance: FAIL

Task quality: CHANGES_REQUIRED

## Edge status

### 1. Canonical risk-tag diagnostics and no-raise validation — OPEN

The exact round-1 counterexample is fixed. Direct `validate_primary` calls with
`risk_tags=[[], {}, 7, null, true, "parser", "parser"]` now return one indexed
diagnostic for every non-string JSON value plus the duplicate-string diagnostic.
Combining `risk_tags=[{}]` with a missing `contract` preserves all three
independent diagnostics. The change at `scripts/model.py:217-224` correctly
keeps invalid tags out of the duplicate-detection set.

However, the requested canonical-validator no-raise property is still false for
adjacent decoded-JSON field types. I independently reproduced all of these
direct model failures on the reviewed snapshot:

```text
validate_file(valid_file(repo=[]))                         -> TypeError
validate_file(valid_file(scope_class={}))                  -> TypeError
validate_primary(valid_primary(disposition=[]))            -> TypeError
validate_primary(valid_primary(counterparts=[{repo:[]}]))  -> TypeError
validate_review(valid_review(verdict=[]))                  -> TypeError
```

Each exception was `cannot use 'list'/'dict' as a set element`. The root is
`_enum` at `scripts/model.py:101-104`, which performs set membership before
checking that the value is a string. Even after `_enum` is made safe, the later
scope/disposition memberships at lines 182, 243, and 251 also need explicit
type guards. Counterpart and checked-source repository enums reach the same
unsafe helper through `_reference_diagnostics` at lines 135-147.

This remains Important. `scripts/verify_ledger.py` masks these failures by
rewriting unhashable enum values in `_schema_safe_value`, so the CLI remains
fail-closed, but Task 1 defines the canonical validators themselves as decoded
JSON dictionary validators that return deterministic diagnostic lists. Direct
callers still crash and lose every independent error accumulated before the
membership operation.

Required correction: make `_enum` type-safe for every JSON value and guard all
later set/dictionary memberships. Add direct model tests covering list and
object values for every enum site (`repo`, `content_kind`, `scope_class`,
`disposition`, counterpart/source `repo`, and `verdict`) while simultaneously
omitting another required field to prove independent diagnostics survive.

### 2. Parsed identity-only review attribution and semantic isolation — OPEN

Several important parts are correct:

- A unique schema-rejected Ghostty primary now attributes its valid review's
  `E_REVIEW_UNKNOWN` to Ghostty.
- A conflicting range ID observed once in Ghostty and once in Rustty is omitted
  from the attribution map; review schema diagnostics for it remain
  unattributed and the partition reconciles.
- A rejected primary does not enter semantic structures. Removing `contract`
  from `g-first` reduced covered, primary-final, and secondary-upheld totals
  from seven to six, created one uncovered line, and made its review unknown.
  Giving that rejected primary a stale but schema-valid counterpart hash did
  not emit `E_COUNTERPART`, proving counterpart resolution was not run.
- Removing `contract` from the `g-gap` parity-gap primary likewise left its line
  uncovered and caused the finding to fail resolution rather than admitting the
  rejected primary into finding structures.

The duplicate-identity control is incomplete, though. The index at
`scripts/verify_ledger.py:360-372` stores only a `set` of repositories. Two
parsed occurrences of the same `(range_id, repo)` collapse to one set member
and are treated as a unique identity.

Reproducer: append a schema-rejected copy of the valid Ghostty `g-first`
primary, retaining the same `range_id="g-first"` and `repo="ghostty"`, and
remove `falsification` from the `g-first` review. Observed:

```text
E_REVIEW_SCHEMA missing falsification  repo=ghostty
unattributed invalid diagnostics       0
E_RANGE_DUPLICATE                      absent
```

The review diagnostic should be unattributed because the parsed identity is
duplicated, exactly as the review package requires. Independently, the verifier
brief requires duplicate `range_id` values to produce `E_RANGE_DUPLICATE`.
Because duplicate detection currently runs only over `validated_primaries` at
lines 427-439, a valid ID duplicated by a schema-rejected record is silently
missed as a duplicate (although the separate schema error still keeps the
overall run invalid).

This remains Important: attribution is not exact for duplicate same-repository
identities and a required stable diagnostic is absent. The cross-repository
conflict test passes because the repository set has size two; it does not
exercise occurrence multiplicity.

Required correction: retain occurrence counts and repository identities for
every parsed record with individually valid `range_id` and `repo`. Attribute
only when exactly one occurrence exists. Emit `E_RANGE_DUPLICATE` for every
duplicate valid range ID regardless of whether all duplicate records pass the
rest of schema validation, without admitting rejected records into coverage,
reviews, counterparts, or findings. Add same-repository and cross-repository
duplicate regressions, including valid+rejected and rejected+rejected pairs.

## Verification evidence

The requested snapshot hashes matched:

```text
505f8c338d03ea8eaab3249e49cecb4d953a2d99b77659ab222ae6f8d9eaa321  scripts/model.py
0067e1bc6f4c7f28c08ae48813e9bf7c09e4161db0929e31856f132f5f8e08da  tests/test_model.py
80094241732f620d0ac90819257a1fb3840b5cf5f5342ce0651648914a4fe044  scripts/verify_ledger.py
e3e10f475f22c49a188c7bf72bfc8986ec1bb015045a3bc5d95b0f92baf61759  tests/test_verify_ledger.py
d2ce9a50a8ca31bc5f3bab936bc34bc9b5898af59ab3d5f34c31c29af5c0eed3  universe.jsonl
```

Pins remained:

```text
rustty   2bd45ccc79ae4ab6bdcb8cb95d5a99d0d8d44816
ghostty  6ad1fe7d8cbda36c77b337a96c9bea8a77883699
```

Fresh complete-suite result:

```text
Ran 63 tests in 24.495s
OK
```

That run includes the round-1 malformed-CLI matrix and stale primary and
counterpart actual-blob bounds regressions, so the prior crash/bounds fixes
remain intact. The round-2 changes touch only primary validation and parsed
primary attribution; the real ledger contains no JSONL primary records.
Therefore the already completed real control at the identical pins/universe
remains applicable, as directed by the review package.

All tested diagnostic partitions continued to satisfy repository totals plus
the explicit unattributed bucket equaling the global total. The open issue is
the correctness of attribution before that arithmetic partition, not the
partition sum itself.

## Decision

CHANGES_REQUIRED. The exact risk-tag and cross-repository ambiguity examples
are improved, and rejected-primary semantic isolation is verified, but the
canonical validators still raise for adjacent decoded JSON types and duplicate
same-repository parsed identities are incorrectly attributed while escaping
`E_RANGE_DUPLICATE`. Both requested round-2 edges remain OPEN; Task 4 must not
begin before another fix and scoped independent review.
