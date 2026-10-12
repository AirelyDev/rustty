import { type Doubled, fmt, type Point } from "./model";
import s from "./TestContract.module.css";

/**
 * Notes on the modules with at least twice as many Rust tests as Zig tests
 * (counts from the data, reasons from their test names and the check's
 * comments), and the scatter's method note.
 */
export default function DoubledNotes({
  points,
  two,
}: {
  points: Point[];
  two: Doubled;
}) {
  /** Returns "module zig → rust" for a module name (the name alone if absent). */
  const pair = (m: string) => {
    const q = points.find((p) => p.mod.m === m);
    return q ? `${m} ${q.z} → ${q.r}` : m;
  };
  return (
    <>
      <div className={s.why}>
        <div>
          <span className={s.m}>{two.mods.length} modules at ≥2×</span> (
          {fmt(two.zig)} Zig → {fmt(two.rust)} Rust). Their test names show why:
        </div>
        <div>
          <span className={s.m}>{pair("xev_file")}</span>: double-mapped.
          libxev's file-watcher tests live inside the epoll and io_uring backend
          files, so this row counts every backend test. The check's own comment
          calls this row "bookkeeping, not proof".
        </div>
        <div>
          <span className={s.m}>{pair("termio_exec")}</span>: Windows
          read-thread and pty paths, plus crash regressions:{" "}
          <code>posix_waitpid_checked_reports_echild_instead_of_aborting</code>,{" "}
          <code>
            osc7_percent_00_pwd_is_ignored_as_a_cwd_instead_of_panicking
          </code>
        </div>
        <div>
          <span className={s.m}>{pair("core_surface")}</span>: Ghostty has
          almost no Surface tests. The Rust ones pin numeric edge cases found
          while porting:{" "}
          <code>
            a_click_repeat_interval_above_u32_nanoseconds_is_widened_not_trapped
          </code>
          , <code>y_gesture_cast_out_of_range_scrolls_nothing</code>
        </div>
        <div>
          <span className={s.m}>{pair("termio_stream_handler")}</span>: OSC 7
          host-length and tmux control-block regressions, e.g.{" "}
          <code>
            tmux_reattach_after_an_oversized_control_block_does_not_crash
          </code>
        </div>
        <div>
          <span className={s.m}>{pair("xev_dynamic")}</span>: one Zig test is
          generic over every backend at compile time. The Rust port runs both
          candidate backends explicitly, one known-answer test each.
        </div>
      </div>
      <p className={s.note}>
        Axes are log(1 + n), so modules with 0 Zig tests sit on the left edge.
        The dashed line is Rust = Zig, and the dotted line is Rust = 2× Zig. A
        row's Rust side is the set of files the check maps to it. 689 tests are
        counted by two rows (the C API's aggregate row, libxev's file-watcher
        row, the C API hub), so the mapped files hold <b>5,353</b> distinct Rust
        tests. Test names in the notes are real function names at HEAD.
      </p>
    </>
  );
}
