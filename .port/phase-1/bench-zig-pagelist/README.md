# Zig reference PageList bench (phase-1 parity)

Standalone Zig harness that measures the ghostty reference `PageList` for the
canonical page-alloc workload, used to compare against the rustty port.

It does **not** modify the reference tree. It builds an exe **outside**
`/Users/gael/projects/ghostty` that imports `src/lib_vt.zig` (the libghostty-vt
module root) as the `terminal` module, reconstructing `terminal_options`
faithfully (`artifact=.lib`, `simd=false`, `oniguruma=false`,
`slow_runtime_safety=false`).

## Workload (identical to the rustty bench)

`init(80,24,1GiB)` -> `grow()` to >=2000 rows -> scroll across viewport ->
`clone()` full screen -> `resize` cols 200/40/80 (reflow) -> `reset()` -> `deinit()`.

## Run

```
zig build -Doptimize=ReleaseFast
./zig-out/bin/zigbench --iters=1000 --warmup=100   # latency
/usr/bin/time -l ./zig-out/bin/zigbench --iters=1  # peak RSS
./zig-out/bin/zigbench --info                       # page geometry (MEASURED)
./zig-out/bin/zigbench --accounting                 # per-stage page-pool allocs (MEASURED)
./zig-out/bin/zigbench --arena-sim=9                 # underlying mmap count (MEASURED, arena replay)
```

The `build.zig` hard-codes `/Users/gael/projects/ghostty/src` as the reference
source path; adjust if the reference lives elsewhere.

Captured numbers: `RESULTS.txt` (also `/tmp/bench_zig.txt`).

NOTE on syscall tracing: dtruss/fs_usage need root under SIP and non-interactive
sudo was unavailable, so the underlying-mmap count is MEASURED via `--arena-sim`
(real `std.heap.ArenaAllocator` over the real `std.heap.page_allocator`, identical
mechanics to `PagePool`) rather than dtruss.
