//! PORT: `synthetic/cli.zig`.

pub mod ascii;
pub mod osc;
pub mod utf8;

/// The available actions for the CLI. This is the list of available
/// synthetic generators. View docs for each individual one in the
/// predictably named files under `cli/`.
///
/// PORT: `pub const Action = enum { ascii, osc, utf8 }`. The `Struct(action)`
/// comptime mapping (action → impl module) is expressed in Rust as the per-arm
/// dispatch in `main_action` below; each impl module exposes `Options`,
/// `create`, `destroy`, and `run`.
pub enum Action {
    Ascii,
    Osc,
    Utf8,
}

// PORT: `pub fn Struct(comptime action: Action) type` — the action → impl
// module mapping (`.ascii => cli/Ascii`, `.osc => cli/Osc`, `.utf8 => cli/Utf8`):
//
// Returns the struct associated with the action. The struct
// should have a few decls:
//
//   - `const Options`: The CLI options for the action.
//   - `fn create`: Create a new instance of the action from options.
//   - `fn destroy`: Destroy the instance of the action.
//
// See TerminalStream for an example.

/// PORT: `pub const Args = union(enum) { cli, string: []const u8 }`.
pub enum Args<'a> {
    /// The arguments passed to the CLI via argc/argv.
    Cli,

    /// Simple string arguments, parsed via std.process.ArgIteratorGeneral.
    String(&'a [u8]),
}

/// An entrypoint for the synthetic generator CLI.
///
/// PORT (binary-entry seam, per the placeholder-leaf-doctrine): `main()` uses
/// `cli.action.detectArgs` (generic `+action` detection over an args iterator),
/// which lives above `synthetic` in the acyclic Rust split and is not reachable
/// here without a cycle. None of the ported `test {}` exercise it. The
/// substantive glue (`main_action` / `main_action_impl`) is ported in structure
/// below.
#[allow(clippy::result_unit_err)]
pub fn main() -> Result<(), ()> {
    // const alloc = std.heap.c_allocator;
    // const action_ = try cli.action.detectArgs(Action, alloc);
    // const action = action_ orelse return error.NoAction;
    // try mainAction(alloc, action, .cli);
    unimplemented!("binary-entry seam: cli.action.detectArgs")
}

/// PORT: `pub fn mainAction(alloc, action, args)` — dispatches on the action to
/// the concrete impl. The `.cli` argv branch and the `.string`
/// `ArgIteratorGeneral` branch (both feeding `cli.args.parse`) are binary-entry
/// seams, not exercised by the ported tests.
#[allow(clippy::result_unit_err)]
pub fn main_action(action: Action, _args: Args) -> Result<(), ()> {
    match action {
        Action::Ascii => main_action_impl_ascii(_args),
        Action::Osc => main_action_impl_osc(_args),
        Action::Utf8 => main_action_impl_utf8(_args),
    }
}

// PORT: `mainActionImpl(comptime Impl, alloc, args)`. The Zig implementation
// parses CLI options (via `cli.args.parse` over an argv/string iterator), seeds
// a DefaultPrng from `std.time.nanoTimestamp`, and runs the impl into stdout,
// flushing at the end. The option-parsing + stdout wiring is a binary-entry
// seam; the three monomorphisations are named explicitly (mirroring Zig's
// `inline else`). The steps of the Zig body, verbatim:
//
//   // First, parse our CLI options.
//   // TODO: Make this a command line option.
//   // Our output always goes to stdout.
//   // Create our implementation
//   // Always flush
fn main_action_impl_ascii(_args: Args) -> Result<(), ()> {
    unimplemented!("binary-entry seam: option parse + stdout")
}
fn main_action_impl_osc(_args: Args) -> Result<(), ()> {
    unimplemented!("binary-entry seam: option parse + stdout")
}
fn main_action_impl_utf8(_args: Args) -> Result<(), ()> {
    unimplemented!("binary-entry seam: option parse + stdout")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_all_actions() {
        // Make sure we ref all our actions
        // (PORT: `inline for (@typeInfo(Action).@"enum".fields) |field| { … _ = Impl; }`)
        let _ = core::mem::size_of::<ascii::Ascii>();
        let _ = core::mem::size_of::<osc::Osc>();
        let _ = core::mem::size_of::<utf8::Utf8>();
        let _ = Action::Ascii;
        let _ = Action::Osc;
        let _ = Action::Utf8;
    }
}
