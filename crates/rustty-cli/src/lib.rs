//! rustty-cli
//!
//! PORTS: ghostty src/cli.zig (+ the src/cli/ action impls). The CLI action
//! dispatcher + `+<action>` command surface, renamed to `rustty` (R2).
//! `args.zig`/`diagnostics.zig`/`CommaSplitter.zig` are the separate
//! `rustty-args` crate (break #7) and are re-exported below, NOT re-ported here.
//!
//! PORT STATUS: C1 (CLI_BLUEPRINT.md §3+§7) — the action dispatcher
//! (`action.zig` 7t, `ghostty.zig` Action enum + `run`/`runMain` 4t), `help.zig`,
//! `version.zig` (PARTIAL, ADV-FIX 1), and the `cli.zig` re-export surface. The
//! remaining action impls (list_*, ssh*, show/validate/explain_config, boo,
//! new_window, toggle_quick_terminal, edit_config, crash_report, show_face) are
//! faithful SEQUENCING STUBS here — their full ports land in chunks C2–C5.

// cli.zig:
//   const diags = @import("cli/diagnostics.zig");
//   pub const args = @import("cli/args.zig");
//   pub const action = @import("cli/action.zig");
//   pub const ghostty = @import("cli/ghostty.zig");
//   pub const CompatibilityHandler = args.CompatibilityHandler;
//   pub const compatibilityRenamed = args.compatibilityRenamed;
//   pub const DiagnosticList = diags.DiagnosticList;
//   pub const Diagnostic = diags.Diagnostic;
//   pub const Location = diags.Location;
//   test { @import("std").testing.refAllDecls(@This()); }
//
// PORT NOTE: `args`/`diagnostics` are the separate `rustty-args` crate (break #7,
// config<->cli); the re-exports below name that crate's items to preserve the
// reference `cli.*` surface.

pub mod action;
pub mod ghostty;

// The action impls. `help`/`version` are the real C1 ports; the rest are
// SEQUENCING STUBS (C2–C5, CLI_BLUEPRINT.md §7) so `ghostty::Action::run_main`
// compiles today. They are `pub mod` so their (future) `Options`/`run` are part
// of the crate's action surface.
pub mod help;
pub mod version;

pub mod boo;
pub mod crash_report;
pub mod edit_config;
pub mod explain_config;
pub mod list_actions;
pub mod list_colors;
pub mod list_fonts;
pub mod list_keybinds;
pub mod list_themes;
pub mod new_window;
pub mod show_config;
pub mod show_face;
pub mod ssh;
pub mod ssh_cache;
pub mod toggle_quick_terminal;
pub mod validate_config;

#[cfg(test)]
mod test_util;

// pub const args = @import("cli/args.zig");
pub use rustty_args::args;
// pub const CompatibilityHandler = args.CompatibilityHandler;
pub use rustty_args::args::CompatibilityHandler;
// pub const compatibilityRenamed = args.compatibilityRenamed;
pub use rustty_args::args::compatibility_renamed;
// pub const DiagnosticList = diags.DiagnosticList;
pub use rustty_args::diagnostics::DiagnosticList;
// pub const Diagnostic = diags.Diagnostic;
pub use rustty_args::diagnostics::Diagnostic;
// pub const Location = diags.Location;
pub use rustty_args::diagnostics::Location;

#[cfg(test)]
mod tests {
    // test { @import("std").testing.refAllDecls(@This()); }
    //
    // PORT NOTE: `refAllDecls` forces every public decl to be semantically
    // analyzed/compiled. The re-export `pub use` lines above are themselves
    // compile-time-checked (a wrong path fails to build), so this test just
    // touches the dispatcher surface + names each re-exported type. (Anonymous
    // `test {}` — skipped by check_test_counts.py's zig counter; ported for R1
    // completeness.)
    #[test]
    fn ref_all_decls() {
        use crate::action::DetectError;
        use crate::ghostty::Action;

        // The Action enum + its dispatcher-facing surface.
        assert_eq!(Action::ALL.len(), 18);
        assert_eq!(Action::Version.tag_name(), "version");
        assert_eq!(Action::Version.file().as_str(), "cli/version.rs");
        assert_eq!(
            Action::ToggleQuickTerminal.file().as_str(),
            "cli/toggle_quick_terminal.rs",
        );
        let _ = DetectError::InvalidAction;

        // The re-export surface (cli.zig's `pub const args`/`Diagnostic`/… ).
        let _loc = crate::Location::Cli(0);
        let _diag: Option<crate::Diagnostic> = None;
        let _list: Option<crate::DiagnosticList> = None;
        let _ch: Option<crate::CompatibilityHandler<crate::help::Options>> = None;
    }
}
