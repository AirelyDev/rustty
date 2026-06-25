//! Types and functions related to tmux protocols.
//!
//! PORTS: ghostty src/terminal/tmux.zig
//!
//! PORT STATUS: done (vt-16). `output` + `layout` + `control` parsers + `viewer`
//! (the control-mode multiplexer driving per-pane Terminals) all ported.

// const control = @import("tmux/control.zig");
pub mod control;
// const layout = @import("tmux/layout.zig");
pub mod layout;
// pub const output = @import("tmux/output.zig");
pub mod output;
pub mod viewer;

// pub const ControlParser = control.Parser;
pub use control::Parser as ControlParser;
// pub const ControlNotification = control.Notification;
pub use control::Notification as ControlNotification;
// pub const Layout = layout.Layout;
pub use layout::Layout;
// pub const Viewer = @import("tmux/viewer.zig").Viewer;
pub use viewer::Viewer;
