//! rustty-font-shaper::shape::noop
//!
//! PORTS: ghostty `src/font/shaper/noop.zig` (143 loc, 1 test) — the no-op
//! shaper: each codepoint maps directly to the run font's glyph index.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const font = @import("../main.zig");
// const Face = font.Face;
// const Collection = font.Collection;
// const DeferredFace = font.DeferredFace;
// const Group = font.Group;
// const GroupCache = font.GroupCache;
// const Library = font.Library;
// const SharedGrid = font.SharedGrid;
// const Style = font.Style;
// const Presentation = font.Presentation;
// const terminal = @import("../../terminal/main.zig");
//
// PORT NOTE: the reference imports the whole `font` namespace for readability;
// rustty imports only what this file references (the shared grid, the run types,
// and `shape.Cell`/`shape.Options`). The remaining imports (`Group`/`GroupCache`
// don't exist in rustty; `DeferredFace`/`Library`/`Style`/`Presentation` are
// unused here) are preserved above as comments.
use allocator_api2::alloc::Allocator;

use crate::shape;

// const log = std.log.scoped(.font_shaper);
// PORT NOTE: the reference declares `log` but never USES it in this file, so
// there is no call site to carry (task-14c-4).

/// Shaper that doesn't do any shaping. Each individual codepoint is mapped
/// directly to the detected text run font's glyph index.
// pub const Shaper = struct { ... };
pub struct Shaper {
    /// The allocated used for the feature list and cell buf.
    // alloc: Allocator,
    //
    // PORT NOTE: the reference stores the allocator to back `cell_buf` /
    // `run_state`. rustty backs both with `Vec` (the crate-wide `Global`
    // convention; allocation failure aborts), so no allocator is stored.

    /// The string used for shaping the current run.
    run_state: RunState,

    /// The shared memory used for shaping results.
    // cell_buf: CellBuf,  (= std.ArrayListUnmanaged(font.shape.Cell))
    cell_buf: Vec<shape::Cell>,
}

// const CodepointList = std.ArrayListUnmanaged(Codepoint);
// const Codepoint = struct { codepoint: u32, cluster: u32 };
struct Codepoint {
    codepoint: u32,
    cluster: u32,
}

// const RunState = struct { codepoints: CodepointList = .{}, ... };
struct RunState {
    codepoints: Vec<Codepoint>,
}

impl RunState {
    // fn deinit(self: *RunState, alloc: Allocator) void {
    //     self.codepoints.deinit(alloc);
    // }
    fn deinit(&mut self) {
        // `Vec` frees on drop; clear for parity with the reference `deinit`.
        self.codepoints.clear();
    }

    // fn reset(self: *RunState) !void {
    //     self.codepoints.clearRetainingCapacity();
    // }
    fn reset(&mut self) {
        self.codepoints.clear();
    }
}

impl Shaper {
    /// The cell_buf argument is the buffer to use for storing shaped results.
    /// This should be at least the number of columns in the terminal.
    // pub fn init(alloc: Allocator, opts: font.shape.Options) !Shaper {
    //
    // PORT NOTE: the reference `!Shaper` return is infallible (no fallible op in
    // the body); rustty returns `Shaper`. `alloc` is accepted for parity but
    // unused (see the `alloc` field note); `opts` is discarded exactly as the
    // reference does (`_ = opts;`).
    pub fn init<A: Allocator>(_alloc: A, opts: shape::Options) -> Shaper {
        let _ = opts;

        Shaper {
            cell_buf: Vec::new(),
            run_state: RunState {
                codepoints: Vec::new(),
            },
        }
    }

    // pub fn deinit(self: *Shaper) void {
    //     self.cell_buf.deinit(self.alloc);
    //     self.run_state.deinit(self.alloc);
    // }
    pub fn deinit(&mut self) {
        self.cell_buf.clear();
        self.run_state.deinit();
    }

    // pub fn endFrame(self: *const Shaper) void { _ = self; }
    pub fn end_frame(&self) {}

    // pub fn runIterator(self, opts) font.shape.RunIterator {
    //     return .{ .hooks = .{ .shaper = self }, .opts = opts };
    // }
    pub fn run_iterator<'a>(
        &mut self,
        opts: shape::RunOptions<'a>,
    ) -> shape::run::RunIterator<'a, RunIteratorHook> {
        shape::run::RunIterator {
            hooks: RunIteratorHook { shaper: self },
            opts,
            i: 0,
        }
    }

    // pub fn shape(self: *Shaper, run: font.shape.TextRun) ![]const font.shape.Cell {
    pub fn shape(
        &mut self,
        run: shape::TextRun,
    ) -> Result<&[shape::Cell], crate::collection::Error> {
        // const state = &self.run_state;

        // Special fonts aren't shaped and their codepoint == glyph so we
        // can just return the codepoints as-is.
        if run.font_index.special().is_some() {
            self.cell_buf.clear();
            self.cell_buf.reserve(self.run_state.codepoints.len());
            for entry in &self.run_state.codepoints {
                self.cell_buf.push(shape::Cell {
                    x: entry.cluster as u16,
                    x_offset: 0,
                    y_offset: 0,
                    glyph_index: entry.codepoint,
                });
            }

            return Ok(&self.cell_buf);
        }

        // Go through the run and map each codepoint to a glyph index.
        self.cell_buf.clear();

        // Note: this is digging into some internal details, we should maybe
        // expose a public API for this.
        // const face = try run.grid.resolver.collection.getFace(run.font_index);
        let face = unsafe { (*run.grid).resolver.collection.get_face(run.font_index) }?;
        for entry in &self.run_state.codepoints {
            let glyph_index = match unsafe { (*face).glyph_index(entry.codepoint) } {
                Some(g) => g,
                None => {
                    // The run iterator shared logic should guarantee that
                    // there is a glyph index for all codepoints in the run.
                    // This is not well tested because we don't use the noop
                    // shaper in any release builds.
                    unreachable!();
                }
            };
            self.cell_buf.push(shape::Cell {
                x: entry.cluster as u16,
                x_offset: 0,
                y_offset: 0,
                glyph_index,
            });
        }

        Ok(&self.cell_buf)
    }
}

/// The hooks for RunIterator.
// pub const RunIteratorHook = struct { shaper: *Shaper, ... };
pub struct RunIteratorHook {
    pub shaper: *mut Shaper,
}

impl shape::run::RunIteratorHook for RunIteratorHook {
    // pub fn prepare(self: *RunIteratorHook) !void {
    //     try self.shaper.run_state.reset();
    // }
    fn prepare(&mut self) {
        unsafe {
            (*self.shaper).run_state.reset();
        }
    }

    // pub fn addCodepoint(self: RunIteratorHook, cp: u32, cluster: u32) !void {
    //     try self.shaper.run_state.codepoints.append(self.shaper.alloc, .{
    //         .codepoint = cp, .cluster = cluster,
    //     });
    // }
    fn add_codepoint(&mut self, cp: u32, cluster: u32) {
        unsafe {
            (*self.shaper).run_state.codepoints.push(Codepoint {
                codepoint: cp,
                cluster,
            });
        }
    }

    // pub fn finalize(self: RunIteratorHook) !void { _ = self; }
    fn finalize(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    // test {
    //     @import("std").testing.refAllDecls(@This());
    // }
    //
    // PORT NOTE: `refAllDecls` forces analysis of every declaration. Rust
    // type-checks all items unconditionally; we mirror it concretely by touching
    // each public decl so a removed/renamed item surfaces as a test failure.
    #[test]
    fn ref_all_decls() {
        let _ = core::mem::size_of::<Shaper>();
        let _ = core::mem::size_of::<RunIteratorHook>();
        let _ = Shaper::init::<allocator_api2::alloc::Global> as fn(_, _) -> Shaper;
        let _ = Shaper::deinit as fn(&mut Shaper);
        let _ = Shaper::end_frame as fn(&Shaper);
        let _ = Shaper::shape
            as fn(&mut Shaper, shape::TextRun) -> Result<&[shape::Cell], crate::collection::Error>;
    }
}
