/**
 * NOT_IN_GHOSTTY lists the pointer-ledger file names that are not under
 * Ghostty's src/ at the lens commit: libxev's (vendored) and libvaxis's
 * Tty.zig. They render as plain text; scripts/verify-porting-paths.mjs checks
 * every other ledger file exists at the lens commit. Pure, so Node can import
 * it under type stripping.
 */
export const NOT_IN_GHOSTTY: ReadonlySet<string> = new Set([
  "Loop.zig",
  "ThreadPool.zig",
  "Tty.zig",
  "backend/kqueue.zig",
  "heap.zig",
  "queue.zig",
  "queue_mpsc.zig",
  "watcher/stream.zig",
]);
