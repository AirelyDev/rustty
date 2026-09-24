//! rustty-vt tmux/layout
//!
//! PORTS: ghostty src/terminal/tmux/layout.zig
//!
//! PORT STATUS: done (vt-16). tmux layout parser + CRC16 checksum. Faithful port
//! of all 40 reference tests.

// const std = @import("std");
// const testing = std.testing;
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
//
// PORT NOTE: the reference parses into a tree of `[]const Layout` slices and
// leaves allocation to the caller, recommending an `ArenaAllocator` purely for
// the convenience of freeing the whole tree in one shot (individual nodes can't
// be freed). In Rust the tree is owned outright: `[]const Layout` -> owning
// `Vec<Layout>`, so dropping the root frees the entire tree — the exact "free
// the whole layout at once" property the arena recommendation buys, without an
// explicit allocator parameter. The `alloc: Allocator` parameter is therefore
// dropped from the signatures (there is nothing for it to do that ownership does
// not already do); the only error path it contributed (`Allocator.Error`) cannot
// occur, so `ParseError` reduces to the `SyntaxError` the parse logic raises.

/// A tmux layout.
///
/// This is a tree structure so by definition it pretty much needs to be
/// allocated. We leave allocation up to the user of this struct, but
/// a general recommendation is to use an arena allocator for simplicity
/// in freeing the entire layout at once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Width, height of the node
    pub width: usize,
    pub height: usize,

    /// X and Y offset from the top-left corner of the window.
    pub x: usize,
    pub y: usize,

    /// The content of this node, either a pane (leaf) or more nodes
    /// (split) horizontally or vertically.
    pub content: Content,
}

/// PORTS: ghostty `Layout.Content`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Pane(usize),
    Horizontal(Vec<Layout>),
    Vertical(Vec<Layout>),
}

/// The deepest container nesting `Layout::parse` accepts. No reference
/// counterpart; see the R13 PORT NOTE on `parse_next`.
pub const MAX_DEPTH: usize = 1000;

// PORT NOTE: no reference counterpart (the reference frees the tree with its
// arena). The derived drop glue would recurse once per nesting level, so the
// tree is torn down with an explicit stack instead (R13, UPSTREAM-DEFECTS.md
// UD10): each node's children are moved onto the stack before the node itself
// drops, so every node drops with an empty child list.
impl Drop for Layout {
    fn drop(&mut self) {
        let mut stack: Vec<Layout> = match &mut self.content {
            Content::Pane(_) => return,
            Content::Horizontal(children) | Content::Vertical(children) => {
                core::mem::take(children)
            }
        };
        while let Some(mut node) = stack.pop() {
            if let Content::Horizontal(children) | Content::Vertical(children) = &mut node.content {
                stack.append(children);
            }
        }
    }
}

/// PORTS: ghostty `Layout.ParseError`.
//
// PORT NOTE: the reference is `Allocator.Error || error{SyntaxError}`. With
// owning `Vec`s there is no fallible allocation in the parse path, so only
// `SyntaxError` remains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    SyntaxError,
}

/// PORTS: ghostty `parseWithChecksum`'s error set
/// (`ParseError || error{ChecksumMismatch}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseWithChecksumError {
    SyntaxError,
    ChecksumMismatch,
}

impl Layout {
    /// Parse a layout string that includes a 4-character checksum prefix.
    ///
    /// The expected format is: `XXXX,layout_string` where XXXX is the
    /// 4-character hexadecimal checksum and the layout string follows
    /// after the comma. For example: `f8f9,80x24,0,0{40x24,0,0,1,40x24,40,0,2}`.
    ///
    /// Returns `ChecksumMismatch` if the checksum doesn't match the layout.
    /// Returns `SyntaxError` if the format is invalid.
    pub fn parse_with_checksum(str: &str) -> Result<Layout, ParseWithChecksumError> {
        // If the string is less than 5 characters, it can't possibly
        // be correct. 4-char checksum + comma. In practice it should
        // be even longer, but that'll fail parse later.
        // if (str.len < 5) return error.SyntaxError;
        if str.len() < 5 {
            return Err(ParseWithChecksumError::SyntaxError);
        }
        // if (str[4] != ',') return error.SyntaxError;
        if str.as_bytes()[4] != b',' {
            return Err(ParseWithChecksumError::SyntaxError);
        }

        // The layout string should start with a 4-character checksum.
        // const checksum: Checksum = .calculate(str[5..]);
        let checksum = Checksum::calculate(&str.as_bytes()[5..]);
        // if (!std.mem.startsWith(u8, str, &checksum.asString()))
        //     return error.ChecksumMismatch;
        if !str.as_bytes().starts_with(&checksum.as_string()) {
            return Err(ParseWithChecksumError::ChecksumMismatch);
        }

        // Checksum matches, parse the rest.
        // return try parse(alloc, str[5..]);
        Layout::parse(&str[5..]).map_err(|_| ParseWithChecksumError::SyntaxError)
    }

    /// Parse a layout string into a Layout structure. The given allocator
    /// will be used for all allocations within the layout. Note that
    /// individual nodes can't be freed so this allocator must be some
    /// kind of arena allocator.
    ///
    /// The layout string must be fully provided as a single string.
    /// Layouts are generally small so this should not be a problem.
    ///
    /// Tmux layout strings have the following format:
    ///
    /// - WxH,X,Y,ID Leaf pane: width×height, x-offset, y-offset, pane ID
    /// - WxH,X,Y{...} Horizontal split (left-right), children comma-separated
    /// - WxH,X,Y[...] Vertical split (top-bottom), children comma-separated
    pub fn parse(str: &str) -> Result<Layout, ParseError> {
        // var offset: usize = 0;
        let mut offset: usize = 0;
        // const root = try parseNext(alloc, str, &offset);
        let root = Layout::parse_next(str, &mut offset, 0)?;
        // if (offset != str.len) return error.SyntaxError;
        if offset != str.len() {
            return Err(ParseError::SyntaxError);
        }
        Ok(root)
    }

    // PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md UD10). `parseNext`
    // recurses once per `{`/`[` with no limit, and the string comes from tmux
    // (`%layout-change`, the `list-windows` reply), so anything written to the
    // pty inside `ESC P 1000 p` can nest it as deep as the control parser's
    // 1 MiB line allows: about 116,000 levels at 9 bytes each. Measured on
    // macOS arm64 with Zig 0.16 (a default 16 MiB thread, as the io reader
    // thread uses): the reference overflows its stack at about 9,200 levels in
    // Debug and about 39,000 in ReleaseSafe and ReleaseFast. The port's
    // walkers run on a Rust thread's default 2 MiB stack, and several of them
    // recurse over the tree (this parser, `init_layout`, the derived `Clone`,
    // and the derived `Debug` the viewer's action log uses). Measured the same
    // way, the first to overflow do so at about 1,500 levels in a debug build
    // and about 2,400 in a release build. The port therefore rejects a layout
    // nested deeper than `MAX_DEPTH` with the parser's existing `SyntaxError`,
    // which the viewer already turns into its defunct state. The limit matches
    // tmux: commit `b7c5030004` (2026-09-09) added `LAYOUT_V1_MAX_DEPTH 1000` to
    // `layout_construct_v1`, counting depth the same way (root is 0, a cell
    // deeper than 1000 is rejected). Older tmux has no cap, so a hand-written
    // `select-layout` string could nest deeper there, but a layout built by
    // splitting cannot realistically reach 1000: each nested split takes at
    // least a row or a column. `Drop` is iterative as well (see above
    // `Layout`). `depth`
    // counts the containers enclosing this node. Tests:
    // `deeply_nested_layout_is_a_syntax_error_beyond_max_depth`,
    // `dropping_a_very_deep_layout_does_not_recurse`, and the viewer's
    // `a_layout_nested_deeper_than_max_depth_makes_the_viewer_defunct`.
    fn parse_next(str: &str, offset: &mut usize, depth: usize) -> Result<Layout, ParseError> {
        if depth > MAX_DEPTH {
            return Err(ParseError::SyntaxError);
        }
        let bytes = str.as_bytes();

        // Find the first `x` to grab the width.
        // const width: usize = if (std.mem.indexOfScalar(u8, str[offset.*..], 'x')) |idx| ...
        //     else return error.SyntaxError;
        let width: usize = match index_of_scalar(&bytes[*offset..], b'x') {
            Some(idx) => {
                // break :width std.fmt.parseInt(usize, str[offset.* .. offset.* + idx], 10)
                //     catch return error.SyntaxError;
                let parsed =
                    parse_int(&str[*offset..*offset + idx]).map_err(|_| ParseError::SyntaxError)?;
                // defer offset.* += idx + 1; // Consume `x`
                *offset += idx + 1;
                parsed
            }
            None => return Err(ParseError::SyntaxError),
        };

        // Find the height, up to a comma.
        // const height: usize = if (std.mem.indexOfScalar(u8, str[offset.*..], ',')) |idx| ...
        //     else return error.SyntaxError;
        let height: usize = match index_of_scalar(&bytes[*offset..], b',') {
            Some(idx) => {
                // break :height std.fmt.parseInt(usize, str[offset.* .. offset.* + idx], 10)
                //     catch return error.SyntaxError;
                let parsed =
                    parse_int(&str[*offset..*offset + idx]).map_err(|_| ParseError::SyntaxError)?;
                // defer offset.* += idx + 1; // Consume `,`
                *offset += idx + 1;
                parsed
            }
            None => return Err(ParseError::SyntaxError),
        };

        // Find X
        // const x: usize = if (std.mem.indexOfScalar(u8, str[offset.*..], ',')) |idx| ...
        //     else return error.SyntaxError;
        let x: usize = match index_of_scalar(&bytes[*offset..], b',') {
            Some(idx) => {
                // break :x std.fmt.parseInt(usize, str[offset.* .. offset.* + idx], 10)
                //     catch return error.SyntaxError;
                let parsed =
                    parse_int(&str[*offset..*offset + idx]).map_err(|_| ParseError::SyntaxError)?;
                // defer offset.* += idx + 1; // Consume `,`
                *offset += idx + 1;
                parsed
            }
            None => return Err(ParseError::SyntaxError),
        };

        // Find Y, which can end in any of `,{,[`
        // const y: usize = if (std.mem.indexOfAny(u8, str[offset.*..], ",{[")) |idx| ...
        //     else return error.SyntaxError;
        let y: usize = match index_of_any(&bytes[*offset..], b",{[") {
            Some(idx) => {
                // break :y std.fmt.parseInt(usize, str[offset.* .. offset.* + idx], 10)
                //     catch return error.SyntaxError;
                let parsed =
                    parse_int(&str[*offset..*offset + idx]).map_err(|_| ParseError::SyntaxError)?;
                // defer offset.* += idx; // Don't consume the delimiter!
                *offset += idx;
                parsed
            }
            None => return Err(ParseError::SyntaxError),
        };

        // Determine our child node.
        // const content: Layout.Content = switch (str[offset.*]) {
        let content: Content = match bytes[*offset] {
            b',' => {
                // Consume the delimiter
                // offset.* += 1;
                *offset += 1;

                // Leaf pane. Read up to `,}]` because we may be in
                // a set of nodes. If none exist, end of string is fine.
                // const idx = std.mem.indexOfAny(u8, str[offset.*..], ",}]")
                //     orelse str.len - offset.*;
                let idx = index_of_any(&bytes[*offset..], b",}]").unwrap_or(str.len() - *offset);

                // const pane_id = std.fmt.parseInt(usize, str[offset.* .. offset.* + idx], 10)
                //     catch return error.SyntaxError;
                let pane_id =
                    parse_int(&str[*offset..*offset + idx]).map_err(|_| ParseError::SyntaxError)?;
                // defer offset.* += idx; // Consume the pane ID, not the delimiter
                *offset += idx;

                // break :content .{ .pane = pane_id };
                Content::Pane(pane_id)
            }

            // '{', '[' => |opening| content: {
            opening @ (b'{' | b'[') => {
                // var nodes: std.ArrayList(Layout) = .empty;
                // defer nodes.deinit(alloc);
                let mut nodes: Vec<Layout> = Vec::new();

                // Move beyond our opening
                // offset.* += 1;
                *offset += 1;

                // while (true) {
                loop {
                    // try nodes.append(alloc, try parseNext(alloc, str, offset));
                    nodes.push(Layout::parse_next(str, offset, depth + 1)?);

                    // We should not reach the end of string here because
                    // we expect a closing bracket.
                    // if (offset.* >= str.len) return error.SyntaxError;
                    if *offset >= str.len() {
                        return Err(ParseError::SyntaxError);
                    }

                    // If it is a comma, we expect another node.
                    // if (str[offset.*] == ',') {
                    if bytes[*offset] == b',' {
                        // offset.* += 1; // Consume
                        *offset += 1;
                        // continue;
                        continue;
                    }

                    // We expect a closing bracket now.
                    // switch (opening) {
                    //     '{' => if (str[offset.*] != '}') return error.SyntaxError,
                    //     '[' => if (str[offset.*] != ']') return error.SyntaxError,
                    //     else => return error.SyntaxError,
                    // }
                    match opening {
                        b'{' => {
                            if bytes[*offset] != b'}' {
                                return Err(ParseError::SyntaxError);
                            }
                        }
                        b'[' => {
                            if bytes[*offset] != b']' {
                                return Err(ParseError::SyntaxError);
                            }
                        }
                        _ => return Err(ParseError::SyntaxError),
                    }

                    // Successfully parsed all children.
                    // offset.* += 1; // Consume closing bracket
                    *offset += 1;
                    // break :content switch (opening) {
                    //     '{' => .{ .horizontal = try nodes.toOwnedSlice(alloc) },
                    //     '[' => .{ .vertical = try nodes.toOwnedSlice(alloc) },
                    //     else => unreachable,
                    // };
                    break match opening {
                        b'{' => Content::Horizontal(nodes),
                        b'[' => Content::Vertical(nodes),
                        _ => unreachable!(),
                    };
                }
            }

            // indexOfAny above guarantees we have only the above
            // else => unreachable,
            _ => unreachable!(),
        };

        // return .{ .width = width, .height = height, .x = x, .y = y, .content = content };
        Ok(Layout {
            width,
            height,
            x,
            y,
            content,
        })
    }
}

/// PORTS: ghostty `Checksum` (a `enum(u16)`).
//
// PORT NOTE: Zig's `enum(u16) { _ }` is a non-exhaustive `u16`-backed enum; in
// Rust the faithful map is a transparent newtype over `u16` (`@intFromEnum` ->
// `.0`, `@enumFromInt` -> `Checksum(v)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checksum(pub u16);

impl Checksum {
    /// Calculate the checksum of a tmux layout string.
    /// The algorithm rotates the checksum right by 1 bit (with wraparound)
    /// and adds the ASCII value of each character.
    pub fn calculate(str: &[u8]) -> Checksum {
        // var result: u16 = 0;
        let mut result: u16 = 0;
        // for (str) |c| {
        for &c in str {
            // Rotate right by 1: (result >> 1) + ((result & 1) << 15)
            // result = (result >> 1) | ((result & 1) << 15);
            result = (result >> 1) | ((result & 1) << 15);
            // result +%= c;
            result = result.wrapping_add(c as u16);
        }

        // return @enumFromInt(result);
        Checksum(result)
    }

    /// Convert the checksum to a 4-character hexadecimal string. This
    /// is always zero-padded to match the tmux implementation
    /// (in layout-custom.c).
    pub fn as_string(self) -> [u8; 4] {
        // const value = @intFromEnum(self);
        let value = self.0;
        // const charset = "0123456789abcdef";
        let charset = b"0123456789abcdef";
        // return .{
        //     charset[(value >> 12) & 0xf],
        //     charset[(value >> 8) & 0xf],
        //     charset[(value >> 4) & 0xf],
        //     charset[value & 0xf],
        // };
        [
            charset[((value >> 12) & 0xf) as usize],
            charset[((value >> 8) & 0xf) as usize],
            charset[((value >> 4) & 0xf) as usize],
            charset[(value & 0xf) as usize],
        ]
    }
}

/// Faithful map of `std.mem.indexOfScalar(u8, slice, value)`: the index of the
/// first occurrence of `value` in `slice`, or `None`.
fn index_of_scalar(slice: &[u8], value: u8) -> Option<usize> {
    slice.iter().position(|&b| b == value)
}

/// Faithful map of `std.mem.indexOfAny(u8, slice, values)`: the index of the
/// first byte in `slice` that appears in `values`, or `None`.
fn index_of_any(slice: &[u8], values: &[u8]) -> Option<usize> {
    slice.iter().position(|b| values.contains(b))
}

/// Faithful map of `std.fmt.parseInt(usize, str, 10)` as the layout parser uses
/// it, through the port of the std function in `rustty_zigstd::fmt`.
/// The layout parser collapses every error into `SyntaxError`, so the error
/// payload is irrelevant here (unlike `output.rs`, which distinguishes them).
// CORRECTION (parity N100): this was `str.parse::<usize>()`, documented as
// "any non-decimal-digit content (including the empty string) is an error";
// the reference also accepts `_` digit separators and `-0`.
fn parse_int(str: &str) -> Result<usize, ()> {
    rustty_zigstd::fmt::parse_int::<usize>(str.as_bytes(), 10).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE: the reference threads a per-test `ArenaAllocator` into `parse`
    // for tree allocation and `deinit`s it on scope exit. The Rust tree owns its
    // `Vec` children, so the root simply drops at end of scope — the arena setup
    // (`var arena ... defer arena.deinit()`) has no Rust counterpart and is
    // elided; behavior and assertions are identical.

    #[test]
    fn simple_single_pane() {
        let layout = Layout::parse("80x24,0,0,42").unwrap();
        assert_eq!(80, layout.width);
        assert_eq!(24, layout.height);
        assert_eq!(0, layout.x);
        assert_eq!(0, layout.y);
        assert_eq!(Content::Pane(42), layout.content);
    }

    // Parity N100 (no reference test): the numbers are parsed with
    // `std.fmt.parseInt(usize, .., 10)`, which accepts `_` digit separators
    // and `-0`.
    #[test]
    fn numbers_use_zig_parse_int() {
        let layout = Layout::parse("8_0x2_4,-0,0,4_2").unwrap();
        assert_eq!(80, layout.width);
        assert_eq!(24, layout.height);
        assert_eq!(0, layout.x);
        assert_eq!(Content::Pane(42), layout.content);
        assert!(Layout::parse("80_x24,0,0,42").is_err());
    }

    #[test]
    fn single_pane_with_offset() {
        let layout = Layout::parse("40x12,10,5,7").unwrap();
        assert_eq!(40, layout.width);
        assert_eq!(12, layout.height);
        assert_eq!(10, layout.x);
        assert_eq!(5, layout.y);
        assert_eq!(Content::Pane(7), layout.content);
    }

    #[test]
    fn single_pane_large_values() {
        let layout = Layout::parse("1920x1080,100,200,999").unwrap();
        assert_eq!(1920, layout.width);
        assert_eq!(1080, layout.height);
        assert_eq!(100, layout.x);
        assert_eq!(200, layout.y);
        assert_eq!(Content::Pane(999), layout.content);
    }

    #[test]
    fn horizontal_split_two_panes() {
        let layout = Layout::parse("80x24,0,0{40x24,0,0,1,40x24,40,0,2}").unwrap();
        assert_eq!(80, layout.width);
        assert_eq!(24, layout.height);
        assert_eq!(0, layout.x);
        assert_eq!(0, layout.y);

        let children = match &layout.content {
            Content::Horizontal(c) => c,
            _ => unreachable!(),
        };
        assert_eq!(2, children.len());

        assert_eq!(40, children[0].width);
        assert_eq!(24, children[0].height);
        assert_eq!(0, children[0].x);
        assert_eq!(0, children[0].y);
        assert_eq!(Content::Pane(1), children[0].content);

        assert_eq!(40, children[1].width);
        assert_eq!(24, children[1].height);
        assert_eq!(40, children[1].x);
        assert_eq!(0, children[1].y);
        assert_eq!(Content::Pane(2), children[1].content);
    }

    #[test]
    fn vertical_split_two_panes() {
        let layout = Layout::parse("80x24,0,0[80x12,0,0,1,80x12,0,12,2]").unwrap();
        assert_eq!(80, layout.width);
        assert_eq!(24, layout.height);
        assert_eq!(0, layout.x);
        assert_eq!(0, layout.y);

        let children = match &layout.content {
            Content::Vertical(c) => c,
            _ => unreachable!(),
        };
        assert_eq!(2, children.len());

        assert_eq!(80, children[0].width);
        assert_eq!(12, children[0].height);
        assert_eq!(0, children[0].x);
        assert_eq!(0, children[0].y);
        assert_eq!(Content::Pane(1), children[0].content);

        assert_eq!(80, children[1].width);
        assert_eq!(12, children[1].height);
        assert_eq!(0, children[1].x);
        assert_eq!(12, children[1].y);
        assert_eq!(Content::Pane(2), children[1].content);
    }

    #[test]
    fn horizontal_split_three_panes() {
        let layout = Layout::parse("120x24,0,0{40x24,0,0,1,40x24,40,0,2,40x24,80,0,3}").unwrap();
        assert_eq!(120, layout.width);
        assert_eq!(24, layout.height);

        let children = match &layout.content {
            Content::Horizontal(c) => c,
            _ => unreachable!(),
        };
        assert_eq!(3, children.len());
        assert_eq!(Content::Pane(1), children[0].content);
        assert_eq!(Content::Pane(2), children[1].content);
        assert_eq!(Content::Pane(3), children[2].content);
    }

    #[test]
    fn nested_horizontal_in_vertical() {
        // Vertical split with top pane and bottom horizontal split
        let layout =
            Layout::parse("80x24,0,0[80x12,0,0,1,80x12,0,12{40x12,0,12,2,40x12,40,12,3}]").unwrap();
        assert_eq!(80, layout.width);
        assert_eq!(24, layout.height);

        let vert_children = match &layout.content {
            Content::Vertical(c) => c,
            _ => unreachable!(),
        };
        assert_eq!(2, vert_children.len());

        // First child is a simple pane
        assert_eq!(Content::Pane(1), vert_children[0].content);

        // Second child is a horizontal split
        let horiz_children = match &vert_children[1].content {
            Content::Horizontal(c) => c,
            _ => unreachable!(),
        };
        assert_eq!(2, horiz_children.len());
        assert_eq!(Content::Pane(2), horiz_children[0].content);
        assert_eq!(Content::Pane(3), horiz_children[1].content);
    }

    #[test]
    fn nested_vertical_in_horizontal() {
        // Horizontal split with left pane and right vertical split
        let layout =
            Layout::parse("80x24,0,0{40x24,0,0,1,40x24,40,0[40x12,40,0,2,40x12,40,12,3]}").unwrap();
        assert_eq!(80, layout.width);
        assert_eq!(24, layout.height);

        let horiz_children = match &layout.content {
            Content::Horizontal(c) => c,
            _ => unreachable!(),
        };
        assert_eq!(2, horiz_children.len());

        // First child is a simple pane
        assert_eq!(Content::Pane(1), horiz_children[0].content);

        // Second child is a vertical split
        let vert_children = match &horiz_children[1].content {
            Content::Vertical(c) => c,
            _ => unreachable!(),
        };
        assert_eq!(2, vert_children.len());
        assert_eq!(Content::Pane(2), vert_children[0].content);
        assert_eq!(Content::Pane(3), vert_children[1].content);
    }

    #[test]
    fn deeply_nested_layout() {
        // Three levels deep
        let layout =
            Layout::parse("80x24,0,0{40x24,0,0[40x12,0,0,1,40x12,0,12,2],40x24,40,0,3}").unwrap();

        let horiz = match &layout.content {
            Content::Horizontal(c) => c,
            _ => unreachable!(),
        };
        assert_eq!(2, horiz.len());

        let vert = match &horiz[0].content {
            Content::Vertical(c) => c,
            _ => unreachable!(),
        };
        assert_eq!(2, vert.len());
        assert_eq!(Content::Pane(1), vert[0].content);
        assert_eq!(Content::Pane(2), vert[1].content);

        assert_eq!(Content::Pane(3), horiz[1].content);
    }

    #[test]
    fn syntax_error_empty_string() {
        assert_eq!(Err(ParseError::SyntaxError), Layout::parse(""));
    }

    #[test]
    fn syntax_error_missing_width() {
        assert_eq!(Err(ParseError::SyntaxError), Layout::parse("x24,0,0,1"));
    }

    #[test]
    fn syntax_error_missing_height() {
        assert_eq!(Err(ParseError::SyntaxError), Layout::parse("80x,0,0,1"));
    }

    #[test]
    fn syntax_error_missing_x() {
        assert_eq!(Err(ParseError::SyntaxError), Layout::parse("80x24,,0,1"));
    }

    #[test]
    fn syntax_error_missing_y() {
        assert_eq!(Err(ParseError::SyntaxError), Layout::parse("80x24,0,,1"));
    }

    #[test]
    fn syntax_error_missing_pane_id() {
        assert_eq!(Err(ParseError::SyntaxError), Layout::parse("80x24,0,0,"));
    }

    #[test]
    fn syntax_error_non_numeric_width() {
        assert_eq!(Err(ParseError::SyntaxError), Layout::parse("abcx24,0,0,1"));
    }

    #[test]
    fn syntax_error_non_numeric_pane_id() {
        assert_eq!(Err(ParseError::SyntaxError), Layout::parse("80x24,0,0,abc"));
    }

    #[test]
    fn syntax_error_unclosed_horizontal_bracket() {
        assert_eq!(
            Err(ParseError::SyntaxError),
            Layout::parse("80x24,0,0{40x24,0,0,1")
        );
    }

    #[test]
    fn syntax_error_unclosed_vertical_bracket() {
        assert_eq!(
            Err(ParseError::SyntaxError),
            Layout::parse("80x24,0,0[40x24,0,0,1")
        );
    }

    #[test]
    fn syntax_error_mismatched_brackets() {
        assert_eq!(
            Err(ParseError::SyntaxError),
            Layout::parse("80x24,0,0{40x24,0,0,1]")
        );
        assert_eq!(
            Err(ParseError::SyntaxError),
            Layout::parse("80x24,0,0[40x24,0,0,1}")
        );
    }

    #[test]
    fn syntax_error_trailing_data() {
        assert_eq!(
            Err(ParseError::SyntaxError),
            Layout::parse("80x24,0,0,1extra")
        );
    }

    #[test]
    fn syntax_error_no_x_separator() {
        assert_eq!(Err(ParseError::SyntaxError), Layout::parse("8024,0,0,1"));
    }

    #[test]
    fn syntax_error_no_content_delimiter() {
        assert_eq!(Err(ParseError::SyntaxError), Layout::parse("80x24,0,0"));
    }

    // parseWithChecksum tests

    #[test]
    fn parse_with_checksum_valid() {
        let layout =
            Layout::parse_with_checksum("f8f9,80x24,0,0{40x24,0,0,1,40x24,40,0,2}").unwrap();
        assert_eq!(80, layout.width);
        assert_eq!(24, layout.height);
    }

    #[test]
    fn parse_with_checksum_mismatch() {
        assert_eq!(
            Err(ParseWithChecksumError::ChecksumMismatch),
            Layout::parse_with_checksum("0000,80x24,0,0{40x24,0,0,1,40x24,40,0,2}")
        );
    }

    #[test]
    fn parse_with_checksum_too_short() {
        assert_eq!(
            Err(ParseWithChecksumError::SyntaxError),
            Layout::parse_with_checksum("bb62")
        );
        assert_eq!(
            Err(ParseWithChecksumError::SyntaxError),
            Layout::parse_with_checksum("")
        );
    }

    #[test]
    fn parse_with_checksum_missing_comma() {
        assert_eq!(
            Err(ParseWithChecksumError::SyntaxError),
            Layout::parse_with_checksum("bb62x159x48,0,0")
        );
    }

    // Checksum tests

    #[test]
    fn checksum_empty_string() {
        let checksum = Checksum::calculate(b"");
        assert_eq!(0u16, checksum.0);
        assert_eq!(b"0000", &checksum.as_string());
    }

    #[test]
    fn checksum_single_character() {
        // 'A' = 65, first iteration: csum = 0 >> 1 | 0 = 0, then 0 + 65 = 65
        let checksum = Checksum::calculate(b"A");
        assert_eq!(65u16, checksum.0);
        assert_eq!(b"0041", &checksum.as_string());
    }

    #[test]
    fn checksum_two_characters() {
        // 'A' (65): csum = 0, rotate = 0, add 65 => 65
        // 'B' (66): csum = 65, rotate => (65 >> 1) | ((65 & 1) << 15) = 32 | 32768 = 32800
        //           add 66 => 32800 + 66 = 32866
        let checksum = Checksum::calculate(b"AB");
        assert_eq!(32866u16, checksum.0);
        assert_eq!(b"8062", &checksum.as_string());
    }

    #[test]
    fn checksum_simple_layout() {
        let checksum = Checksum::calculate(b"80x24,0,0,42");
        assert_eq!(b"d962", &checksum.as_string());
    }

    #[test]
    fn checksum_horizontal_split_layout() {
        let checksum = Checksum::calculate(b"80x24,0,0{40x24,0,0,1,40x24,40,0,2}");
        assert_eq!(b"f8f9", &checksum.as_string());
    }

    #[test]
    fn checksum_as_string_zero_padding() {
        // Value 0x000f should produce "000f"
        let checksum = Checksum(0x000f);
        assert_eq!(b"000f", &checksum.as_string());
    }

    #[test]
    fn checksum_as_string_all_digits() {
        // Value 0x1234 should produce "1234"
        let checksum = Checksum(0x1234);
        assert_eq!(b"1234", &checksum.as_string());
    }

    #[test]
    fn checksum_as_string_with_letters() {
        // Value 0xabcd should produce "abcd"
        let checksum = Checksum(0xabcd);
        assert_eq!(b"abcd", &checksum.as_string());
    }

    #[test]
    fn checksum_as_string_max_value() {
        // Value 0xffff should produce "ffff"
        let checksum = Checksum(0xffff);
        assert_eq!(b"ffff", &checksum.as_string());
    }

    #[test]
    fn checksum_wraparound() {
        let checksum = Checksum::calculate(b"\xff\xff\xff\xff\xff\xff\xff\xff");
        assert_eq!(b"03fc", &checksum.as_string());
    }

    #[test]
    fn checksum_deterministic() {
        // Same input should always produce same output
        let str = b"159x48,0,0{79x48,0,0,79x48,80,0}";
        let checksum1 = Checksum::calculate(str);
        let checksum2 = Checksum::calculate(str);
        assert_eq!(checksum1, checksum2);
    }

    #[test]
    fn checksum_different_inputs_different_outputs() {
        let checksum1 = Checksum::calculate(b"80x24,0,0,1");
        let checksum2 = Checksum::calculate(b"80x24,0,0,2");
        assert!(checksum1.0 != checksum2.0);
    }

    #[test]
    fn checksum_known_tmux_layout_bb62() {
        // From tmux documentation: "bb62,159x48,0,0{79x48,0,0,79x48,80,0}"
        // The checksum "bb62" corresponds to the layout "159x48,0,0{79x48,0,0,79x48,80,0}"
        let checksum = Checksum::calculate(b"159x48,0,0{79x48,0,0,79x48,80,0}");
        assert_eq!(b"bb62", &checksum.as_string());
    }

    // ---- R13 (task R13-A, UPSTREAM-DEFECTS UD10): nesting depth -------------

    /// `n` nested horizontal containers around one pane, 9 bytes per level.
    fn nested(n: usize) -> String {
        let mut s = String::with_capacity(n * 9 + 9);
        for _ in 0..n {
            s.push_str("1x1,0,0{");
        }
        s.push_str("1x1,0,0,1");
        for _ in 0..n {
            s.push('}');
        }
        s
    }

    /// Runs `f` on a thread with Rust's default spawn stack, the stack the
    /// termio reader thread runs the viewer on.
    fn on_default_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        std::thread::spawn(f).join().unwrap()
    }

    /// The deepest nesting the control parser's 1 MiB line limit lets tmux
    /// output carry is about 116,000 levels. The reference recurses into all of
    /// it and overflows; the port stops at `MAX_DEPTH` with `SyntaxError`, and
    /// accepts exactly `MAX_DEPTH`.
    #[test]
    fn deeply_nested_layout_is_a_syntax_error_beyond_max_depth() {
        let levels = (1024 * 1024) / 9;
        let result = on_default_thread(move || Layout::parse(&nested(levels)).map(|_| ()));
        assert_eq!(Err(ParseError::SyntaxError), result);

        let result = on_default_thread(|| Layout::parse(&nested(MAX_DEPTH + 1)).map(|_| ()));
        assert_eq!(Err(ParseError::SyntaxError), result);

        let depth = on_default_thread(|| {
            let layout = Layout::parse(&nested(MAX_DEPTH)).unwrap();
            let mut depth = 0;
            let mut node = &layout;
            while let Content::Horizontal(children) = &node.content {
                depth += 1;
                node = &children[0];
            }
            assert_eq!(Content::Pane(1), node.content);
            depth
        });
        assert_eq!(MAX_DEPTH, depth, "MAX_DEPTH itself must parse");
    }

    /// Dropping a tree does not recurse. The tree is built without the parser,
    /// 200,000 levels deep, far past what recursive drop glue survives on a
    /// 2 MiB stack.
    #[test]
    fn dropping_a_very_deep_layout_does_not_recurse() {
        let levels = on_default_thread(|| {
            let mut node = Layout {
                width: 1,
                height: 1,
                x: 0,
                y: 0,
                content: Content::Pane(1),
            };
            for _ in 0..200_000 {
                node = Layout {
                    width: 1,
                    height: 1,
                    x: 0,
                    y: 0,
                    content: Content::Vertical(vec![node]),
                };
            }
            let mut levels = 0;
            let mut cursor = &node;
            while let Content::Vertical(children) = &cursor.content {
                levels += 1;
                cursor = &children[0];
            }
            drop(node);
            levels
        });
        assert_eq!(200_000, levels);
    }
}
