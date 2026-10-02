# Sanitizer suppressions and exclusions (G-ASAN, G-LSAN, G-MSAN)

The sanitizer gates (`.port/tools/gates/sanitizers.sh --gate asan|lsan|msan`,
registered in `.port/tools/ci.sh`; design:
`docs/superpowers/specs/2026-10-01-memory-safety-lanes-design.md` §B) run every
workspace crate's tests under AddressSanitizer, ASan + LeakSanitizer and
MemorySanitizer. This file is the register of everything those gates do NOT
check, in the same spirit as `MIRI_EXCEPTIONS.md`: each entry has a cited
reason and evidence, and nothing is suppressed or excluded silently.

## Criteria (all must hold for an entry)

1. **Not ours.** A suppression covers a finding inside a system library or
   vendored third-party C/C++, never in Rustty code. A finding in Rustty code
   is a bug: it gets an N id (and a UD when Ghostty has it too), a minimal fix
   and a regression test.
2. **Evidence.** The entry quotes the sanitizer's report (the leaking or
   faulting frames) from a named run, so a reader can see that the stack is
   entirely third-party below the call we make.
3. **Narrow.** The pattern names the third-party function or module, never a
   Rustty crate, never a whole test binary.
4. **Re-checkable.** Removing the entry and re-running the named command
   reproduces the report. A stale entry (it no longer matches anything) is
   removed.

An exclusion of a whole crate from a gate needs the same: the reason the crate
cannot run under that sanitizer at all (it does not build, or it links
uninstrumented code the sanitizer cannot model), and the evidence.

## Mechanics

- G-LSAN reads `.port/phase-2/lsan.supp` through
  `LSAN_OPTIONS=suppressions=…:print_suppressions=1`, so every suppressed leak
  is listed in the run's log. Each line in `lsan.supp` has a WHY comment above
  it (D6 rule c, `d6_cfg_mapping_audit.py`) pointing at its row here.
- Crate exclusions are the `ASAN_EXCLUDES`, `LSAN_EXCLUDES` and `MSAN_EXCLUDES`
  arrays in `sanitizers.sh`, each `"<crate>|<reason>"`. An entry naming a crate
  that is no longer a workspace member is RED. G-MSAN's scope is an allow-list:
  every member must be in `MSAN_SCOPE` or `MSAN_EXCLUDES`.
