//! PORTS: ghostty src/config/Config.zig — the MACHINERY tests (load/loadReader/
//! finalize).

// ===========================================================================
// MACHINERY tests (load/loadReader/finalize). PORTS: the `test {}` blocks in the
// `load`..`Replay` range of Config.zig (Phase-0 inventory: ONE test in range —
// "handle bom in config files", assertion-for-assertion below).
// ===========================================================================
use super::*;

// test "handle bom in config files" {
#[test]
fn handle_bom_in_config_files() {
    // const testing = std.testing;
    // const alloc = testing.allocator;

    // {
    {
        // const data = "\xef\xbb\xbfabnormal-command-exit-runtime = 2500\n";
        let data: &[u8] = b"\xef\xbb\xbfabnormal-command-exit-runtime = 2500\n";
        // var reader: std.Io.Reader = .fixed(data);
        // var cfg = try Config.default(alloc);
        // defer cfg.deinit();
        let mut cfg = Config::default_config();
        // try cfg.loadReader(
        //     alloc,
        //     &reader,
        //     "/home/ghostty/.config/ghostty/config.ghostty",
        // );
        cfg.load_reader(data, b"/home/ghostty/.config/ghostty/config.ghostty")
            .unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // try testing.expect(cfg._diagnostics.empty());
        assert!(cfg._diagnostics.empty());
        // try testing.expectEqual(
        //     2500,
        //     cfg.@"abnormal-command-exit-runtime",
        // );
        assert_eq!(2500, cfg.abnormal_command_exit_runtime);
    }

    // {
    {
        // const data = "abnormal-command-exit-runtime = 2500\n";
        let data: &[u8] = b"abnormal-command-exit-runtime = 2500\n";
        // var reader: std.Io.Reader = .fixed(data);
        // var cfg = try Config.default(alloc);
        // defer cfg.deinit();
        let mut cfg = Config::default_config();
        // try cfg.loadReader(
        //     alloc,
        //     &reader,
        //     "/home/ghostty/.config/ghostty/config.ghostty",
        // );
        cfg.load_reader(data, b"/home/ghostty/.config/ghostty/config.ghostty")
            .unwrap();
        // try cfg.finalize();
        cfg.finalize().unwrap();

        // try testing.expect(cfg._diagnostics.empty());
        assert!(cfg._diagnostics.empty());
        // try testing.expectEqual(
        //     2500,
        //     cfg.@"abnormal-command-exit-runtime",
        // );
        assert_eq!(2500, cfg.abnormal_command_exit_runtime);
    }
}

// No reference `test {}` — PROVING test for S74-F5. `finalize`'s
// `quit-after-last-window-closed-delay ... very short value ({f})` warning
// (`Config.zig:4866`) dropped its formatted value, so every too-short delay
// produced the same string and the user could not see WHICH value was
// rejected. `{f}` calls `Duration.format` (`4s`, `1m 30s`), not the builtin
// aggregate formatter.
//
// S74-F5's OTHER site, `Config.zig:4746`'s `log.info("shell src=config
// value={}", .{cmd})`, has no unit test by construction: the reference wraps
// that whole `command:` block in `comptime !builtin.is_test`
// (`Config.zig:4739`) and the port ports the gate as `#[cfg(not(test))]`
// (`config_struct.rs:7285`), so it cannot execute in a `cargo test` binary
// on either side. Its restored payload is asserted by inspection against the
// oracle, not by a test that would have to defeat the reference's own gate.
#[test]
fn finalize_warns_with_the_formatted_short_quit_delay() {
    let mut cfg = Config::default_config();
    cfg.quit_after_last_window_closed_delay = Some(Duration {
        // 4s — under the reference's `5 * std.time.ns_per_s` threshold.
        duration: 4 * NS_PER_S,
    });

    log_capture::arm();
    cfg.finalize().unwrap();
    let records = log_capture::take();

    assert_eq!(
        records,
        vec![concat!(
            "warning(config): quit-after-last-window-closed-delay is set to a very ",
            "short value (4s), which might cause problems"
        )
        .to_string()],
    );
}

// No reference `test {}` — PROVING tests for the `.config` scoped logger's
// stderr gate (reference logFn: `if (!global.logging().stderr) break
// :stderr;`, main_ghostty.zig; `global.zig` init sets `logging.stderr =
// false` when a CLI action runs). `format_log` is the SINGLE formatting
// site `log_info`/`log_warn` write through, so gating is asserted on its
// return instead of fragile stderr capture.
/// `std.log.Level.err.asText()` is **`"error"`**, not `"err"` — and `logFn`
/// prefixes with `level_txt`. Two stages on this plan got this rendering
/// wrong, so it is pinned separately from any call site (task-14e-2).
///
/// Asserted through `format_log` rather than by calling `log_err` here on
/// purpose: `check_log_reconcile.py` counts every call site of a `log_*`
/// sink as an EMISSION, test call sites included, so a direct call in this
/// file would credit `src/config/Config.zig`'s arm with a diagnostic the
/// port does not actually make and let its ratchet be tightened onto a
/// phantom. The helper itself is pinned behaviourally by
/// `c_api::log_site_tests::load_file_logs_the_reference_error_under_the_config_scope`,
/// which drives a REAL failure through `log_err` and asserts the rendered
/// `error(config): ` prefix.
#[test]
fn config_log_err_renders_the_error_token() {
    let on = rustty_global::Logging {
        stderr: true,
        ..Default::default()
    };
    assert_eq!(
        Some("error(config): boom".to_string()),
        format_log("error", "boom", on)
    );
    assert_ne!(
        Some("err(config): boom".to_string()),
        format_log("error", "boom", on)
    );
}

#[test]
fn config_log_gate_honors_logging_stderr() {
    let gated = rustty_global::Logging {
        stderr: false,
        macos: false,
    };
    let ungated = rustty_global::Logging {
        stderr: true,
        macos: false,
    };

    // Action path (stderr=false): NOTHING is emitted for either level.
    assert_eq!(
        format_log("info", "skipping UTF-8 byte order mark", gated),
        None
    );
    assert_eq!(
        format_log("warning", "loading them both in that order", gated),
        None
    );

    // Non-action path (stderr=true): logFn's `.config`-scope prefix bytes.
    assert_eq!(
        format_log("info", "reading configuration file path=/tmp/c", ungated).as_deref(),
        Some("info(config): reading configuration file path=/tmp/c"),
    );
    assert_eq!(
        format_log("warning", "error expanding paths err=X", ungated).as_deref(),
        Some("warning(config): error expanding paths err=X"),
    );
}

// End-to-end through the process-global accessor the live `log_info` reads:
// publish the action-detected value (as `rustty_apprt::init_with_action`
// does), run the BOM'd-config load (a `log_info` call site), and assert the
// published gate reaches `format_log`. Publishing is safe crate-wide: no
// other test in this binary asserts stderr contents or calls `set_logging`.
#[test]
fn config_log_gate_reads_published_global() {
    rustty_global::set_logging(rustty_global::Logging {
        stderr: false,
        macos: false,
    });
    assert!(!rustty_global::logging().stderr);

    // The load still succeeds; its `log_info` lines are gated off.
    let data: &[u8] = b"\xef\xbb\xbfabnormal-command-exit-runtime = 2500\n";
    let mut cfg = Config::default_config();
    cfg.load_reader(data, b"/home/ghostty/.config/ghostty/config.ghostty")
        .unwrap();
    assert_eq!(2500, cfg.abnormal_command_exit_runtime);

    // What every `log_info`/`log_warn` in that path consulted:
    assert_eq!(
        format_log(
            "info",
            "skipping UTF-8 byte order mark",
            rustty_global::logging()
        ),
        None,
    );
}
