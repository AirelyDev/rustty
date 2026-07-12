# D9 — stale-comment contract checklist

Detector D9 (BUN_PORT_LEARNINGS.md §4; PROPOSED R7 "Comments" bullet) kills the
cross-cutting comment hazard: **the code was ported faithfully, but the comment
still describes the OLD (Zig) primitive's failure behaviour.** A comment is not
neutral text — R1 preserves it, and a preserved comment that lies about the
Rust primitive's contract actively misleads the next porter.

Gate: `.port/tools/gates/check_stale_comments.sh` (grep heuristic, flags
CANDIDATES). Ledger: `.port/audits/stale-comments-D9.tsv`.

## The semantic gaps this catches

| Zig idiom | Rust port | The difference a stale comment hides |
|-----------|-----------|--------------------------------------|
| `reinterpretSlice(u16, buf)` | `bytemuck::cast_slice::<_, u16>(buf)` | Zig **silently drops** a trailing odd byte (`@divTrunc`); `cast_slice` **PANICS** on a len/align mismatch. |
| `x catch unreachable` | `.expect("…")` | Zig is **UB-on-error** in ReleaseFast (no message, no unwind); `.expect` **always panics** with the message. |
| `a +% b` / `a -% b` / `a *% b` | `a.wrapping_add(b)` etc. | A bare `a + b` **debug-panics** on overflow — neither the Zig wrap nor a documented invariant. |
| `a +| b` (saturating) | `a.saturating_add(b)` | Same: bare op is not saturating. |
| `&mut T` field / arg | `*mut T` raw pointer | Rust `&mut` is **noalias** (SB/TB); the raw port drops that guarantee and gains an aliasing/`unsafe` contract. |

## Review rule (apply to every flagged candidate)

1. **Read the flagged comment and the primitive on the flagged line.**
2. Ask: **does the comment state the RUST primitive's contract**, or only the
   old Zig behaviour?
   - Names/implies **non-panic** behaviour (`silently drops`, `never fails`,
     `cannot fail`, `wraps`, `infallible`, or just the Zig builtin `@intCast` /
     `+%` / `catch unreachable`) with **no mention** that the Rust primitive
     panics/asserts/aliases → **`needs-fix`**: rewrite the comment to the Rust
     contract (fidelity to *intent*, R7 "Comments"). Meaning is preserved; the
     primitive it describes is corrected.
   - Already states the panic/assert/aliasing contract (or the Zig line is
     quoted purely as a `// PORT NOTE:` provenance annotation right next to a
     correct Rust-contract sentence) → **`contract-ok`**.
   - The primitive is not actually a semantic-gap case (heuristic false hit,
     e.g. a `.expect` on a `Mutex` lock, an `as *const` that never came from a
     `&mut`) → **`false-positive`**.
3. **GUARANTEE-TEST class only:** the comment asserts "len/align **is
   guaranteed** X" next to a **panicking cast** (`cast_slice`/`align_to`/
   `bytes_of`). D9 requires a **test asserting X** (the property the panic
   depends on). Confirm such a test exists and names the property; if not →
   **`needs-fix`** (add the test; put it in the crate's own test module).
4. **Record the disposition** in `.port/audits/stale-comments-D9.tsv`:
   `path <TAB> line <TAB> {contract-ok|false-positive|needs-fix} <TAB> note`.
5. When every candidate is dispositioned and all `needs-fix` are resolved, add
   `# STATUS: SEALED` to the ledger. The gate then **binds** (a new
   un-dispositioned candidate is RED) — progress→binding, R5-clean.

## Per-chunk rule (prospective)

When a chunk introduces any semantic-gap primitive above, the porter writes the
comment against the **Rust** primitive's contract in the same commit. Run
`check_stale_comments.sh --list` on the touched crate before the sub-batch gate;
any new candidate is triaged in the ledger before the chunk is claimed done.
