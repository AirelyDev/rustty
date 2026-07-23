//! rustty-vt x11_color
//!
//! PORTS: ghostty src/terminal/x11_color.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const assert = @import("../quirks.zig").inlineAssert;
//   const RGB = @import("color.zig").RGB;
// The reference builds a `std.StaticStringMapWithEql(RGB,
// eqlAsciiIgnoreCase)` at comptime from the embedded `res/rgb.txt`. Rust has no
// comptime string-map builder, so we parse the same embedded data lazily into a
// case-insensitive `HashMap` on first access (a `OnceLock`). The parsing logic
// and assertions are ported verbatim from `colorMap()` below.

use crate::color::RGB;
use rustty_util::quirks::inlineAssert as assert;
use std::collections::HashMap;
use std::sync::OnceLock;

/// The map of all available X11 colors.
// PORT NOTE: `pub const map = colorMap();` — a comptime value in the reference.
// In Rust this is a lazily-initialized global (`color_map()`); the public `get`
// helper mirrors `map.get(name)`.
pub struct Map;

/// The map type, case-insensitive ASCII keys to RGB.
// PORT NOTE: `pub const ColorMap = std.StaticStringMapWithEql(RGB,
// std.static_string_map.eqlAsciiIgnoreCase);`. We store keys pre-lowercased and
// lowercase the lookup key, which matches ASCII-case-insensitive equality.
pub type ColorMap = HashMap<String, RGB>;

/// The map of all available X11 colors.
// PORT NOTE: the reference name is the lowercase `map`; keep it verbatim.
#[allow(non_upper_case_globals)]
pub static map: Map = Map;

impl Map {
    /// Look up a color by name, case-insensitively. Returns `None` if absent.
    // PORT NOTE: mirrors `ColorMap.get` from the reference's StaticStringMap.
    pub fn get(&self, name: &str) -> Option<RGB> {
        color_map().get(&name.to_ascii_lowercase()).copied()
    }

    /// The color names in `rgb.txt` (insertion) order, ORIGINAL case preserved.
    // PORT NOTE: mirrors `StaticStringMap.keys()` — the reference
    // `StaticStringMapWithEql` stores the ORIGINAL-case names as keys (comparing
    // them ASCII-case-insensitively via `eqlAsciiIgnoreCase`), so `map.keys()`
    // yields e.g. `GhostWhite` / `ghost white` verbatim. `+list-colors` prints
    // these names directly, so the case MUST be preserved; `build_color_map`
    // above lowercases keys for the case-insensitive `get`, so the displayed key
    // set is parsed separately here (same `rgb.txt` parse, original-case names).
    pub fn keys(&self) -> &'static [String] {
        color_keys()
    }
}

fn color_map() -> &'static ColorMap {
    static MAP: OnceLock<ColorMap> = OnceLock::new();
    MAP.get_or_init(build_color_map)
}

// PORT NOTE: the original-case name list backing `Map::keys()`. `build_color_map`
// discards case (it stores lowercased lookup keys), so the verbatim `rgb.txt`
// names are collected here in file (insertion) order — exactly the key set the
// reference `StaticStringMap.keys()` returns. Lazily built once, like `color_map`.
fn color_keys() -> &'static [String] {
    static KEYS: OnceLock<Vec<String>> = OnceLock::new();
    KEYS.get_or_init(build_color_keys)
}

// PORT NOTE: the name-only half of `colorMap()`'s parse — same trimming as
// `build_color_map` but keeping the ORIGINAL-case name (no RGB, no lowercasing).
fn build_color_keys() -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for raw_line in DATA.split('\n') {
        // Trim \r so this works with both LF and CRLF line endings.
        let line = if !raw_line.is_empty() && raw_line.as_bytes()[raw_line.len() - 1] == b'\r' {
            &raw_line[0..raw_line.len() - 1]
        } else {
            raw_line
        };
        if line.is_empty() {
            continue;
        }
        let name = line[12..].trim_matches(|c| c == ' ' || c == '\t');
        keys.push(name.to_string());
    }
    keys
}

/// A single X11 color entry.
// pub const Entry = struct { name: [:0]const u8, color: RGB };
// PORT NOTE: the reference name is null-terminated for the C API. In rustty the
// static C table (with its own null-terminated copies) is built in
// rustty-vt-c::color; here `name` is the plain rgb.txt name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: &'static str,
    pub color: RGB,
}

/// All X11 colors in rgb.txt file order.
///
/// This is kept separate from `map` because the C API needs stable file order.
// pub const entries: []const Entry = entriesArray();
pub fn entries() -> &'static [Entry] {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(build_entries)
}

// PORT NOTE: shares the same rgb.txt line parse as `build_color_map` (columns
// 0..3, 4..7, 8..11 are r/g/b, name from column 12 to the trailing-whitespace-
// trimmed end) but in FILE ORDER, no case folding, duplicates kept — matching
// the reference `entriesArray()` comptime build.
fn build_entries() -> Vec<Entry> {
    let mut result: Vec<Entry> = Vec::new();
    for raw_line in DATA.split('\n') {
        // Trim \r so this works with both LF and CRLF line endings.
        let line = if !raw_line.is_empty() && raw_line.as_bytes()[raw_line.len() - 1] == b'\r' {
            &raw_line[0..raw_line.len() - 1]
        } else {
            raw_line
        };
        if line.is_empty() {
            continue;
        }
        let r: u8 = line[0..3].trim_matches(' ').parse().unwrap();
        let g: u8 = line[4..7].trim_matches(' ').parse().unwrap();
        let b: u8 = line[8..11].trim_matches(' ').parse().unwrap();
        let name = line[12..].trim_matches(|c| c == ' ' || c == '\t');
        result.push(Entry {
            name,
            color: RGB { r, g, b },
        });
    }
    result
}

// PORT NOTE: ported from the reference `fn colorMap() ColorMap`. The reference
// runs this at comptime with `@setEvalBranchQuota(500_000)`; in Rust it runs
// once at first access. The `const KV = struct { []const u8, RGB };` declaration
// is implicit in the `(String, RGB)` pairs we insert.
fn build_color_map() -> ColorMap {
    // The length of our data is the number of lines in the rgb file.
    let len = DATA.matches('\n').count();
    let mut kvs: ColorMap = HashMap::new();

    // Parse the line. This is not very robust parsing, because we expect
    // a very exact format for rgb.txt. However, this is all done at comptime
    // so if our data is bad, we should hopefully get an error here or one
    // of our unit tests will catch it.
    let mut i: usize = 0;
    for raw_line in DATA.split('\n') {
        // Trim \r so this works with both LF and CRLF line endings,
        // since git may convert rgb.txt to CRLF on Windows checkouts.
        let line = if !raw_line.is_empty() && raw_line.as_bytes()[raw_line.len() - 1] == b'\r' {
            &raw_line[0..raw_line.len() - 1]
        } else {
            raw_line
        };
        if line.is_empty() {
            continue;
        }
        let r: u8 = line[0..3].trim_matches(' ').parse().unwrap();
        let g: u8 = line[4..7].trim_matches(' ').parse().unwrap();
        let b: u8 = line[8..11].trim_matches(' ').parse().unwrap();
        let name = line[12..].trim_matches(|c| c == ' ' || c == '\t');
        kvs.insert(name.to_ascii_lowercase(), RGB { r, g, b });
        i += 1;
    }
    assert(i == len);

    // return .initComptime(kvs);
    kvs
}

/// This is the rgb.txt file from the X11 project. This was last sourced
/// from this location: https://gitlab.freedesktop.org/xorg/app/rgb
/// This data is licensed under the MIT/X11 license while this Zig file is
/// licensed under the same license as Ghostty.
// PORT NOTE: `const data = @embedFile("res/rgb.txt");` — the file is copied into
// the Rustty tree (R3) at `src/res/rgb.txt` and embedded with `include_str!`.
const DATA: &str = include_str!("res/rgb.txt");

#[cfg(test)]
mod tests {
    use super::*;

    // entries table: rgb.txt order, matches the map (oracle 2970e9a2a)
    #[test]
    fn x11_entries_order_and_map_parity() {
        let ents = entries();
        assert!(ents.len() > 700);
        for e in ents {
            assert_eq!(map.get(e.name), Some(e.color));
        }
        // rgb.txt starts with "snow"
        assert_eq!(ents[0].name, "snow");
        assert_eq!(
            ents[0].color,
            RGB {
                r: 255,
                g: 250,
                b: 250
            }
        );
    }

    #[test]
    fn x11_color_map() {
        assert_eq!(None, map.get("nosuchcolor"));
        assert_eq!(
            RGB {
                r: 255,
                g: 255,
                b: 255
            },
            map.get("white").unwrap()
        );
        assert_eq!(
            Some(RGB {
                r: 0,
                g: 250,
                b: 154
            }),
            map.get("medium spring green")
        );
        assert_eq!(
            Some(RGB {
                r: 34,
                g: 139,
                b: 34
            }),
            map.get("ForestGreen")
        );
        assert_eq!(
            Some(RGB {
                r: 34,
                g: 139,
                b: 34
            }),
            map.get("FoReStGReen")
        );
        assert_eq!(Some(RGB { r: 0, g: 0, b: 0 }), map.get("black"));
        assert_eq!(Some(RGB { r: 255, g: 0, b: 0 }), map.get("red"));
        assert_eq!(Some(RGB { r: 0, g: 255, b: 0 }), map.get("green"));
        assert_eq!(Some(RGB { r: 0, g: 0, b: 255 }), map.get("blue"));
        assert_eq!(
            Some(RGB {
                r: 255,
                g: 255,
                b: 255
            }),
            map.get("white")
        );
        assert_eq!(
            Some(RGB {
                r: 124,
                g: 252,
                b: 0
            }),
            map.get("lawngreen")
        );
        assert_eq!(
            Some(RGB {
                r: 0,
                g: 250,
                b: 154
            }),
            map.get("mediumspringgreen")
        );
        assert_eq!(
            Some(RGB {
                r: 34,
                g: 139,
                b: 34
            }),
            map.get("forestgreen")
        );
    }
}
