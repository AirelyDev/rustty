//! PORTS: Zig stdlib `std.fs.path.dirnamePosix` (`lib/std/fs/path.zig`, Zig
//! 0.16) AS THE PORT USES IT — `std.fs.path.dirname` on POSIX, in ghostty's
//! `os/resourcesdir.zig` and `os/i18n.zig`.
//!
//! Pure byte-slice logic with no OS access, so unlike rustty-os's `path` module
//! it compiles on every target, including wasm32 (where rustty-os's `i18n` is
//! compiled and reaches it).

/// zig 0.16.0 `std.fs.path.dirnamePosix` (`std/fs/path.zig:1330-1339`, over
/// `ComponentIterator(.posix)` at `:1953-2090`): the path up to the end of the
/// component before the last one, the root `/` when there is none but the
/// path is absolute, and `null` for a relative single component, an empty
/// path, or a path that is only separators. Trailing separators are ignored.
pub fn dirname_posix(path: &[u8]) -> Option<&[u8]> {
    let is_sep = |c: u8| c == b'/';
    // init: root_len, then root_end_index past repeated separators.
    let root_len = usize::from(path.first().is_some_and(|&c| is_sep(c)));
    let mut root_end = root_len;
    while root_end < path.len() && is_sep(path[root_end]) {
        root_end += 1;
    }
    // _ = it.last() orelse return null;
    let mut end = path.len();
    loop {
        if end == root_end {
            return None;
        }
        if !is_sep(path[end - 1]) {
            break;
        }
        end -= 1;
    }
    let mut start = end;
    while start != root_end && !is_sep(path[start - 1]) {
        start -= 1;
    }
    // const up = it.previous() orelse return it.root();
    let root = || (root_end != 0).then(|| &path[..root_len]);
    let mut prev_end = start;
    loop {
        if prev_end == root_end {
            return root();
        }
        if !is_sep(path[prev_end - 1]) {
            break;
        }
        prev_end -= 1;
    }
    // return up.path;
    Some(&path[..prev_end])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// zig 0.16.0 `std/fs/path.zig` `test dirnamePosix` (`:1341-1353`),
    /// verbatim. rustty-os `resourcesDir`'s executable-path walk and `i18n.init` use
    /// it (S112-F3): `"a"` and `"a/"` have no dirname, where `Path::parent` gives
    /// `""` and the walk probed `/share/...` one extra time.
    #[test]
    fn dirname_posix_matches_zig() {
        let t = |input: &[u8], expected: Option<&[u8]>| {
            assert_eq!(dirname_posix(input), expected, "{:?}", input);
        };
        t(b"/a/b/c", Some(b"/a/b"));
        t(b"/a/b/c///", Some(b"/a/b"));
        t(b"/a", Some(b"/"));
        t(b"/", None);
        t(b"//", None);
        t(b"///", None);
        t(b"////", None);
        t(b"", None);
        t(b"a", None);
        t(b"a/", None);
        t(b"a//", None);
    }
}
