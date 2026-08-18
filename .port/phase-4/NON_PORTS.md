# Phase-4 non-ports (deliberate, evidenced — the `quirks_memset` class)

Rows here are NOT deferrals: each records a range item with a written argument that a
faithful port would be wrong or meaningless in Rust, plus the condition that would reopen it.

| item | upstream | verdict | argument | reopens if |
|---|---|---|---|---|
| `src/lib/TinyIo.zig` (1,522) + the `82df79ec8` module-export rider | NEW file; used by the streaming C APIs as a lightweight blocking `std.Io` implementation | **NON-PORT (provisional until W5 Bundle S lands)** | TinyIo exists to satisfy Zig 0.16's `std.Io` VTable interface (~60 fn pointers) so lib code avoids `std.Io.Threaded`'s TLS/vtable bloat — a Zig-stdlib/linker concern. rustty has NO `std.Io` abstraction anywhere (rustty-zigstd models only the concrete types actually used); the Rust streaming C APIs read/write via `std::io`/`std::fs` directly. Porting it would invent the entire `std.Io` interface + ~20 raw POSIX externs into a leaf crate with ZERO consumers (R9 density increase, dead code). Its ~20 tests drive Zig stdlib machinery (`testing.tmpDir`, `File.Reader.appendRemaining`, `Io.Mutex`) with no Rust counterpart. | W5 Bundle S (snapshot/io/streaming C API) discovers a genuine need for a pluggable blocking-IO seam; the Bundle-S task MUST re-read this row and either port the needed subset or extend this argument. |

Cross-references: `.port/phase-3/NON_PORTS.md` (the class precedent, `quirks_memset`),
assessment `C-capi-lib-wasm.md` (Bundle S), the W1 lib agent's full rationale in the
`wf-w1-foundations` run journal.
