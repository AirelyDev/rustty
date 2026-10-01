//! PORTS: ghostty src/config/Config.zig — the `Keybinds` tests.

use super::*;

// -----------------------------------------------------------------------
// Keybinds (Config.zig:6415)
//
// PORT NOTE: the reference tests use an `ArenaAllocator` threaded through
// `parseCLI`/`clone`. The Rust APIs are allocator-free (owned values), so the
// arena setup/teardown collapses to nothing; the assertions are reproduced
// verbatim. `formatterpkg.entryFormatter("a", &buf.writer)` -> `entry_formatter`
// over an `AllocatingWriter`; `buf.written()` -> `buf.written()`.
// -----------------------------------------------------------------------

// test "parseCLI"
#[test]
fn keybinds_parse_cli() {
    let mut set: Keybinds = Keybinds::default();
    // try set.parseCLI(alloc, "shift+a=copy_to_clipboard");
    set.parse_cli(Some(b"shift+a=copy_to_clipboard")).unwrap();
    // try set.parseCLI(alloc, "shift+a=csi:hello");
    set.parse_cli(Some(b"shift+a=csi:hello")).unwrap();
}

// test "formatConfig single"
#[test]
fn keybinds_format_config_single() {
    let mut buf = AllocatingWriter::init();

    let mut list: Keybinds = Keybinds::default();
    // try list.parseCLI(alloc, "shift+a=csi:hello");
    list.parse_cli(Some(b"shift+a=csi:hello")).unwrap();
    // try list.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
    let mut f = entry_formatter(b"a", &mut buf);
    list.format_entry(&mut f).unwrap();
    // try std.testing.expectEqualSlices(u8, "a = shift+a=csi:hello\n", buf.written());
    assert_eq!(b"a = shift+a=csi:hello\n".as_slice(), buf.written());
}

// No reference test. LEDGER S76-F1: the reference's docs branch compares
// `@tagName(v)` — the tag of the `Binding.Set.Value` union, i.e. `leader`,
// `leaf` or `leaf_chained` — against `help_strings.KeybindAction`'s decls,
// which are the ACTION names (`helpgen.zig` `genKeybindActions`, from
// `Binding.Action`'s union fields). No action has one of those three names,
// so the reference writes only the leading `"\n"` for each root binding and
// never a `# <action>` help block (UPSTREAM-DEFECTS UD180, latent). The port
// keeps that output.
#[test]
fn keybinds_format_entry_docs_writes_only_the_blank_line() {
    let mut list: Keybinds = Keybinds::default();
    // One of each `Set.Value` kind: a leaf, a chained leaf, a leader.
    list.parse_cli(Some(b"shift+a=new_window")).unwrap();
    list.parse_cli(Some(b"shift+b=new_tab")).unwrap();
    list.parse_cli(Some(b"chain=close_surface")).unwrap();
    list.parse_cli(Some(b"ctrl+a>b=new_split:right")).unwrap();

    let mut docs_buf = AllocatingWriter::init();
    let mut f = entry_formatter(b"a", &mut docs_buf);
    list.format_entry_docs(&mut f, true).unwrap();
    let docs = String::from_utf8(docs_buf.written().to_vec()).unwrap();

    let mut plain_buf = AllocatingWriter::init();
    let mut f = entry_formatter(b"a", &mut plain_buf);
    list.format_entry_docs(&mut f, false).unwrap();
    let plain = String::from_utf8(plain_buf.written().to_vec()).unwrap();

    // The docs branch ran: one "\n" per root binding (3: the chained
    // leaf, the plain leaf, the leader), and nothing else was added.
    assert_eq!(3, list.set.bindings_count());
    assert!(!docs.contains('#'), "no help block expected:\n{docs}");
    let mut blank_lines = docs.lines().filter(|l| l.is_empty()).count();
    assert_eq!(3, blank_lines, "{docs}");
    blank_lines = plain.lines().filter(|l| l.is_empty()).count();
    assert_eq!(0, blank_lines, "{plain}");
    assert_eq!(
        plain.lines().collect::<Vec<_>>(),
        docs.lines().filter(|l| !l.is_empty()).collect::<Vec<_>>()
    );
}

// Regression test for https://github.com/ghostty-org/ghostty/issues/2734
// test "formatConfig multiple items"
#[test]
fn keybinds_format_config_multiple_items() {
    let mut buf = AllocatingWriter::init();

    let mut list: Keybinds = Keybinds::default();
    // try list.parseCLI(alloc, "ctrl+z>1=goto_tab:1");
    list.parse_cli(Some(b"ctrl+z>1=goto_tab:1")).unwrap();
    // try list.parseCLI(alloc, "ctrl+z>2=goto_tab:2");
    list.parse_cli(Some(b"ctrl+z>2=goto_tab:2")).unwrap();
    // try list.formatEntry(formatterpkg.entryFormatter("keybind", &buf.writer));
    let mut f = entry_formatter(b"keybind", &mut buf);
    list.format_entry(&mut f).unwrap();

    // Note they turn into translated keys because they match
    // their ASCII mapping.
    let want = concat!(
        "keybind = ctrl+z>1=goto_tab:1\n",
        "keybind = ctrl+z>2=goto_tab:2\n",
    );
    // try std.testing.expectEqualStrings(want, buf.written());
    assert_eq!(want.as_bytes(), buf.written());
}

// test "formatConfig multiple items nested"
#[test]
fn keybinds_format_config_multiple_items_nested() {
    let mut buf = AllocatingWriter::init();

    let mut list: Keybinds = Keybinds::default();
    // try list.parseCLI(alloc, "ctrl+a>ctrl+b>n=new_window");
    list.parse_cli(Some(b"ctrl+a>ctrl+b>n=new_window")).unwrap();
    // try list.parseCLI(alloc, "ctrl+a>ctrl+b>w=close_window");
    list.parse_cli(Some(b"ctrl+a>ctrl+b>w=close_window"))
        .unwrap();
    // try list.parseCLI(alloc, "ctrl+a>ctrl+c>t=new_tab");
    list.parse_cli(Some(b"ctrl+a>ctrl+c>t=new_tab")).unwrap();
    // try list.parseCLI(alloc, "ctrl+b>ctrl+d>a=previous_tab");
    list.parse_cli(Some(b"ctrl+b>ctrl+d>a=previous_tab"))
        .unwrap();
    // try list.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
    let mut f = entry_formatter(b"a", &mut buf);
    list.format_entry(&mut f).unwrap();

    // NB: This does not currently retain the order of the keybinds.
    let want = concat!(
        "a = ctrl+a>ctrl+b>n=new_window\n",
        "a = ctrl+a>ctrl+b>w=close_window\n",
        "a = ctrl+a>ctrl+c>t=new_tab\n",
        "a = ctrl+b>ctrl+d>a=previous_tab\n",
    );
    // try std.testing.expectEqualStrings(want, buf.written());
    assert_eq!(want.as_bytes(), buf.written());
}

// test "parseCLI table definition"
#[test]
fn keybinds_parse_cli_table_definition() {
    let mut keybinds: Keybinds = Keybinds::default();

    // Define a table by adding a binding to it
    // try keybinds.parseCLI(alloc, "foo/shift+a=copy_to_clipboard");
    keybinds
        .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
        .unwrap();
    // try testing.expectEqual(1, keybinds.tables.count());
    assert_eq!(1, keybinds.tables.count());
    // try testing.expect(keybinds.tables.contains("foo"));
    assert!(keybinds.tables.contains(&b"foo".to_vec()));

    // const table = keybinds.tables.get("foo").?;
    let table = keybinds.tables.get(&b"foo".to_vec()).unwrap();
    // try testing.expectEqual(1, table.bindings.count());
    assert_eq!(1, table.bindings_count());
}

// test "parseCLI table clear"
#[test]
fn keybinds_parse_cli_table_clear() {
    let mut keybinds: Keybinds = Keybinds::default();

    // Add a binding to a table
    keybinds
        .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
        .unwrap();
    // try testing.expectEqual(1, keybinds.tables.get("foo").?.bindings.count());
    assert_eq!(
        1,
        keybinds
            .tables
            .get(&b"foo".to_vec())
            .unwrap()
            .bindings_count()
    );

    // Clear the table with "foo/"
    keybinds.parse_cli(Some(b"foo/")).unwrap();
    // try testing.expectEqual(0, keybinds.tables.get("foo").?.bindings.count());
    assert_eq!(
        0,
        keybinds
            .tables
            .get(&b"foo".to_vec())
            .unwrap()
            .bindings_count()
    );
}

// test "parseCLI table multiple bindings"
#[test]
fn keybinds_parse_cli_table_multiple_bindings() {
    let mut keybinds: Keybinds = Keybinds::default();

    keybinds
        .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
        .unwrap();
    keybinds
        .parse_cli(Some(b"foo/shift+b=paste_from_clipboard"))
        .unwrap();
    keybinds
        .parse_cli(Some(b"bar/ctrl+c=close_window"))
        .unwrap();

    // try testing.expectEqual(2, keybinds.tables.count());
    assert_eq!(2, keybinds.tables.count());
    // try testing.expectEqual(2, keybinds.tables.get("foo").?.bindings.count());
    assert_eq!(
        2,
        keybinds
            .tables
            .get(&b"foo".to_vec())
            .unwrap()
            .bindings_count()
    );
    // try testing.expectEqual(1, keybinds.tables.get("bar").?.bindings.count());
    assert_eq!(
        1,
        keybinds
            .tables
            .get(&b"bar".to_vec())
            .unwrap()
            .bindings_count()
    );
}

// test "parseCLI table does not affect root set"
#[test]
fn keybinds_parse_cli_table_does_not_affect_root_set() {
    let mut keybinds: Keybinds = Keybinds::default();

    keybinds
        .parse_cli(Some(b"shift+a=copy_to_clipboard"))
        .unwrap();
    keybinds
        .parse_cli(Some(b"foo/shift+b=paste_from_clipboard"))
        .unwrap();

    // Root set should have the first binding
    // try testing.expectEqual(1, keybinds.set.bindings.count());
    assert_eq!(1, keybinds.set.bindings_count());
    // Table should have the second binding
    // try testing.expectEqual(1, keybinds.tables.get("foo").?.bindings.count());
    assert_eq!(
        1,
        keybinds
            .tables
            .get(&b"foo".to_vec())
            .unwrap()
            .bindings_count()
    );
}

// test "parseCLI table empty name is invalid"
#[test]
fn keybinds_parse_cli_table_empty_name_is_invalid() {
    let mut keybinds: Keybinds = Keybinds::default();
    // try testing.expectError(error.InvalidFormat, keybinds.parseCLI(alloc, "/shift+a=copy_to_clipboard"));
    assert_eq!(
        Err(KeybindsParseError::InvalidFormat),
        keybinds.parse_cli(Some(b"/shift+a=copy_to_clipboard")),
    );
}

// test "parseCLI table with key sequence"
#[test]
fn keybinds_parse_cli_table_with_key_sequence() {
    let mut keybinds: Keybinds = Keybinds::default();

    // Key sequences should work within tables
    keybinds
        .parse_cli(Some(b"foo/ctrl+a>ctrl+b=new_window"))
        .unwrap();

    // const table = keybinds.tables.get("foo").?;
    let table = keybinds.tables.get(&b"foo".to_vec()).unwrap();
    // try testing.expectEqual(1, table.bindings.count());
    assert_eq!(1, table.bindings_count());
}

// test "parseCLI slash in action argument is not a table"
#[test]
fn keybinds_parse_cli_slash_in_action_argument_is_not_a_table() {
    let mut keybinds: Keybinds = Keybinds::default();

    // A slash after the = should not be interpreted as a table delimiter
    keybinds.parse_cli(Some(b"ctrl+a=text:/hello")).unwrap();

    // Should be in root set, not a table
    assert_eq!(1, keybinds.set.bindings_count());
    assert_eq!(0, keybinds.tables.count());
}

// test "parseCLI slash as key with modifier is not a table"
#[test]
fn keybinds_parse_cli_slash_as_key_with_modifier_is_not_a_table() {
    let mut keybinds: Keybinds = Keybinds::default();

    // ctrl+/ should be parsed as a keybind with '/' as the key, not a table
    keybinds.parse_cli(Some(b"ctrl+/=text:foo")).unwrap();

    // Should be in root set, not a table
    assert_eq!(1, keybinds.set.bindings_count());
    assert_eq!(0, keybinds.tables.count());
}

// test "parseCLI shift+slash as key is not a table"
#[test]
fn keybinds_parse_cli_shift_slash_as_key_is_not_a_table() {
    let mut keybinds: Keybinds = Keybinds::default();

    // shift+/ should be parsed as a keybind, not a table
    keybinds.parse_cli(Some(b"shift+/=ignore")).unwrap();

    // Should be in root set, not a table
    assert_eq!(1, keybinds.set.bindings_count());
    assert_eq!(0, keybinds.tables.count());
}

// test "parseCLI bare slash as key is not a table"
#[test]
fn keybinds_parse_cli_bare_slash_as_key_is_not_a_table() {
    let mut keybinds: Keybinds = Keybinds::default();

    // Bare / as a key should work (empty table name is rejected)
    keybinds.parse_cli(Some(b"/=text:foo")).unwrap();

    // Should be in root set, not a table
    assert_eq!(1, keybinds.set.bindings_count());
    assert_eq!(0, keybinds.tables.count());
}

// test "parseCLI slash in key sequence is not a table"
#[test]
fn keybinds_parse_cli_slash_in_key_sequence_is_not_a_table() {
    let mut keybinds: Keybinds = Keybinds::default();

    // Key sequence ending with / should work
    keybinds
        .parse_cli(Some(b"ctrl+a>ctrl+/=new_window"))
        .unwrap();

    // Should be in root set, not a table
    assert_eq!(1, keybinds.set.bindings_count());
    assert_eq!(0, keybinds.tables.count());
}

// test "parseCLI table with slash in binding"
#[test]
fn keybinds_parse_cli_table_with_slash_in_binding() {
    let mut keybinds: Keybinds = Keybinds::default();

    // Table with a binding that uses / as the key
    keybinds.parse_cli(Some(b"mytable//=text:foo")).unwrap();

    // Should be in the table
    assert_eq!(0, keybinds.set.bindings_count());
    assert_eq!(1, keybinds.tables.count());
    assert!(keybinds.tables.contains(&b"mytable".to_vec()));
    assert_eq!(
        1,
        keybinds
            .tables
            .get(&b"mytable".to_vec())
            .unwrap()
            .bindings_count()
    );
}

// test "parseCLI table with sequence containing slash"
#[test]
fn keybinds_parse_cli_table_with_sequence_containing_slash() {
    let mut keybinds: Keybinds = Keybinds::default();

    // Table with a key sequence that ends with /
    keybinds.parse_cli(Some(b"mytable/a>/=new_window")).unwrap();

    // Should be in the table
    assert_eq!(0, keybinds.set.bindings_count());
    assert_eq!(1, keybinds.tables.count());
    assert!(keybinds.tables.contains(&b"mytable".to_vec()));
}

// test "parseCLI chain without prior parsed binding is invalid"
#[test]
fn keybinds_parse_cli_chain_without_prior_parsed_binding_is_invalid() {
    let mut keybinds: Keybinds = Keybinds::default();

    assert_eq!(
        Err(KeybindsParseError::InvalidFormat),
        keybinds.parse_cli(Some(b"chain=new_tab")),
    );
}

// test "parseCLI table chain syntax is invalid"
#[test]
fn keybinds_parse_cli_table_chain_syntax_is_invalid() {
    let mut keybinds: Keybinds = Keybinds::default();

    keybinds.parse_cli(Some(b"foo/a=text:hello")).unwrap();
    assert_eq!(
        Err(KeybindsParseError::InvalidFormat),
        keybinds.parse_cli(Some(b"foo/chain=deactivate_key_table")),
    );
}

// test "parseCLI chain applies to most recent table binding"
#[test]
fn keybinds_parse_cli_chain_applies_to_most_recent_table_binding() {
    use rustty_input::binding::{Action, Trigger, TriggerKey, Value};
    use rustty_vt::input::key_mods::Mods;

    let mut keybinds: Keybinds = Keybinds::default();

    keybinds
        .parse_cli(Some(b"ctrl+n=activate_key_table:foo"))
        .unwrap();
    keybinds.parse_cli(Some(b"foo/a=text:hello")).unwrap();
    keybinds
        .parse_cli(Some(b"chain=deactivate_key_table"))
        .unwrap();

    // const root_entry = keybinds.set.get(.{ .mods = .{ .ctrl = true },
    //     .key = .{ .unicode = 'n' } }).?.value_ptr.*;
    let root_entry = keybinds
        .set
        .get_value(Trigger {
            mods: Mods {
                ctrl: true,
                ..Mods::default()
            },
            key: TriggerKey::Unicode('n' as u32),
        })
        .unwrap();
    // try testing.expect(root_entry == .leaf);
    // try testing.expect(root_entry.leaf.action == .activate_key_table);
    match root_entry {
        Value::Leaf(leaf) => {
            assert!(matches!(leaf.action, Action::ActivateKeyTable(_)));
        }
        _ => panic!("expected leaf"),
    }

    // const foo_entry = keybinds.tables.get("foo").?.get(.{
    //     .key = .{ .unicode = 'a' } }).?.value_ptr.*;
    let foo_entry = keybinds
        .tables
        .get(&b"foo".to_vec())
        .unwrap()
        .get_value(Trigger {
            key: TriggerKey::Unicode('a' as u32),
            ..Trigger::default()
        })
        .unwrap();
    // try testing.expect(foo_entry == .leaf_chained);
    match foo_entry {
        Value::LeafChained(chain) => {
            // try testing.expectEqual(@as(usize, 2), foo_entry.leaf_chained.actions.items.len);
            assert_eq!(2usize, chain.actions.len());
            // try testing.expect(foo_entry.leaf_chained.actions.items[0] == .text);
            assert!(matches!(chain.actions[0], Action::Text(_)));
            // try testing.expect(foo_entry.leaf_chained.actions.items[1] == .deactivate_key_table);
            assert!(matches!(chain.actions[1], Action::DeactivateKeyTable));
        }
        _ => panic!("expected leaf_chained"),
    }
}

// test "clone with tables"
#[test]
fn keybinds_clone_with_tables() {
    let mut keybinds: Keybinds = Keybinds::default();
    keybinds
        .parse_cli(Some(b"shift+a=copy_to_clipboard"))
        .unwrap();
    keybinds
        .parse_cli(Some(b"foo/shift+b=paste_from_clipboard"))
        .unwrap();
    keybinds
        .parse_cli(Some(b"bar/ctrl+c=close_window"))
        .unwrap();

    // const cloned = try keybinds.clone(alloc);
    let cloned = keybinds.clone_in();

    // Verify the clone has the same structure
    // try testing.expectEqual(keybinds.set.bindings.count(), cloned.set.bindings.count());
    assert_eq!(keybinds.set.bindings_count(), cloned.set.bindings_count());
    // try testing.expectEqual(keybinds.tables.count(), cloned.tables.count());
    assert_eq!(keybinds.tables.count(), cloned.tables.count());
    // try testing.expectEqual(keybinds.tables.get("foo").?.bindings.count(),
    //     cloned.tables.get("foo").?.bindings.count());
    assert_eq!(
        keybinds
            .tables
            .get(&b"foo".to_vec())
            .unwrap()
            .bindings_count(),
        cloned
            .tables
            .get(&b"foo".to_vec())
            .unwrap()
            .bindings_count(),
    );
    // try testing.expectEqual(keybinds.tables.get("bar").?.bindings.count(),
    //     cloned.tables.get("bar").?.bindings.count());
    assert_eq!(
        keybinds
            .tables
            .get(&b"bar".to_vec())
            .unwrap()
            .bindings_count(),
        cloned
            .tables
            .get(&b"bar".to_vec())
            .unwrap()
            .bindings_count(),
    );
}

// test "equal with tables"
#[test]
fn keybinds_equal_with_tables() {
    let mut keybinds1: Keybinds = Keybinds::default();
    keybinds1
        .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
        .unwrap();

    let mut keybinds2: Keybinds = Keybinds::default();
    keybinds2
        .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
        .unwrap();

    // try testing.expect(keybinds1.equal(keybinds2));
    assert!(keybinds1.equal(&keybinds2));
}

// test "equal with tables different table count"
#[test]
fn keybinds_equal_with_tables_different_table_count() {
    let mut keybinds1: Keybinds = Keybinds::default();
    keybinds1
        .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
        .unwrap();

    let mut keybinds2: Keybinds = Keybinds::default();
    keybinds2
        .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
        .unwrap();
    keybinds2
        .parse_cli(Some(b"bar/shift+b=paste_from_clipboard"))
        .unwrap();

    // try testing.expect(!keybinds1.equal(keybinds2));
    assert!(!keybinds1.equal(&keybinds2));
}

// test "equal with tables different table names"
#[test]
fn keybinds_equal_with_tables_different_table_names() {
    let mut keybinds1: Keybinds = Keybinds::default();
    keybinds1
        .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
        .unwrap();

    let mut keybinds2: Keybinds = Keybinds::default();
    keybinds2
        .parse_cli(Some(b"bar/shift+a=copy_to_clipboard"))
        .unwrap();

    // try testing.expect(!keybinds1.equal(keybinds2));
    assert!(!keybinds1.equal(&keybinds2));
}

// test "equal with tables different bindings"
#[test]
fn keybinds_equal_with_tables_different_bindings() {
    let mut keybinds1: Keybinds = Keybinds::default();
    keybinds1
        .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
        .unwrap();

    let mut keybinds2: Keybinds = Keybinds::default();
    keybinds2
        .parse_cli(Some(b"foo/shift+b=paste_from_clipboard"))
        .unwrap();

    // try testing.expect(!keybinds1.equal(keybinds2));
    assert!(!keybinds1.equal(&keybinds2));
}

// test "formatEntry with tables"
#[test]
fn keybinds_format_entry_with_tables() {
    let mut buf = AllocatingWriter::init();

    let mut keybinds: Keybinds = Keybinds::default();
    keybinds.parse_cli(Some(b"foo/shift+a=csi:hello")).unwrap();
    let mut f = entry_formatter(b"keybind", &mut buf);
    keybinds.format_entry(&mut f).unwrap();

    // try testing.expectEqualStrings("keybind = foo/shift+a=csi:hello\n", buf.written());
    assert_eq!(
        b"keybind = foo/shift+a=csi:hello\n".as_slice(),
        buf.written()
    );
}

// PORT NOTE: no reference test — this pins the `var buf: [1024]u8` bound of
// `Keybinds.formatEntryDocs` (Config.zig:7446). The table-binding prefix is
// printed with `writer.print("{s}/{f}", ...) catch return error.OutOfMemory`
// into that fixed writer, and `formatEntries` appends `"={f}"` into the same
// buffer, so the WHOLE `<table>/<trigger>=<action>` entry must fit in 1024
// bytes or the format fails with `error.OutOfMemory`.
#[test]
fn keybinds_format_entry_table_prefix_fixed_buffer_bound() {
    // "<name>/a=csi:hello" — 12 bytes on top of the table name.
    const OVERHEAD: usize = b"/a=csi:hello".len();
    const CAP: usize = 1024;

    let format_with_table_name_len = |n: usize| {
        let mut input = vec![b'a'; n];
        input.extend_from_slice(b"/a=csi:hello");

        let mut keybinds: Keybinds = Keybinds::default();
        keybinds.parse_cli(Some(&input)).unwrap();

        let mut buf = AllocatingWriter::init();
        let mut f = entry_formatter(b"keybind", &mut buf);
        match keybinds.format_entry(&mut f) {
            Ok(()) => Ok(buf.written().to_vec()),
            Err(FormatError::OutOfMemory) => Err("OutOfMemory"),
            Err(FormatError::Writer(_)) => panic!("unexpected writer error"),
        }
    };

    // Comfortably under the bound: formats normally.
    let out = format_with_table_name_len(1000).unwrap();
    let mut want = b"keybind = ".to_vec();
    want.extend_from_slice(&vec![b'a'; 1000]);
    want.extend_from_slice(b"/a=csi:hello\n");
    assert_eq!(want, out);

    // Exactly at the bound: the entry is 1024 bytes, which still fits.
    assert!(format_with_table_name_len(CAP - OVERHEAD).is_ok());

    // One byte over: the leaf's `"={f}"` write no longer fits.
    assert_eq!(
        Err("OutOfMemory"),
        format_with_table_name_len(CAP - OVERHEAD + 1)
    );

    // A table name at/over the buffer size fails on the prefix write itself.
    assert_eq!(Err("OutOfMemory"), format_with_table_name_len(CAP));
}

// test "formatEntry with tables and root set"
#[test]
fn keybinds_format_entry_with_tables_and_root_set() {
    let mut buf = AllocatingWriter::init();

    let mut keybinds: Keybinds = Keybinds::default();
    keybinds.parse_cli(Some(b"shift+b=csi:world")).unwrap();
    keybinds.parse_cli(Some(b"foo/shift+a=csi:hello")).unwrap();
    let mut f = entry_formatter(b"keybind", &mut buf);
    keybinds.format_entry(&mut f).unwrap();

    // const output = buf.written();
    let output = buf.written();
    // try testing.expect(std.mem.indexOf(u8, output, "keybind = shift+b=csi:world\n") != null);
    assert!(find_sub(output, b"keybind = shift+b=csi:world\n").is_some());
    // try testing.expect(std.mem.indexOf(u8, output, "keybind = foo/shift+a=csi:hello\n") != null);
    assert!(find_sub(output, b"keybind = foo/shift+a=csi:hello\n").is_some());
}

// test "parseCLI clear clears tables"
#[test]
fn keybinds_parse_cli_clear_clears_tables() {
    let mut keybinds: Keybinds = Keybinds::default();

    // Add bindings to root set and tables
    keybinds
        .parse_cli(Some(b"shift+a=copy_to_clipboard"))
        .unwrap();
    keybinds
        .parse_cli(Some(b"foo/shift+b=paste_from_clipboard"))
        .unwrap();
    keybinds
        .parse_cli(Some(b"bar/ctrl+c=close_window"))
        .unwrap();

    assert_eq!(1, keybinds.set.bindings_count());
    assert_eq!(2, keybinds.tables.count());

    // Clear all keybinds
    keybinds.parse_cli(Some(b"clear")).unwrap();

    // Both root set and tables should be cleared
    assert_eq!(0, keybinds.set.bindings_count());
    assert_eq!(0, keybinds.tables.count());
}

// test "parseCLI reset clears tables"
#[test]
fn keybinds_parse_cli_reset_clears_tables() {
    let mut keybinds: Keybinds = Keybinds::default();

    // Add bindings to tables
    keybinds
        .parse_cli(Some(b"foo/shift+a=copy_to_clipboard"))
        .unwrap();
    keybinds
        .parse_cli(Some(b"bar/shift+b=paste_from_clipboard"))
        .unwrap();

    assert_eq!(2, keybinds.tables.count());

    // Reset to defaults (empty value)
    keybinds.parse_cli(Some(b"")).unwrap();

    // Tables should be cleared, root set has defaults
    assert_eq!(0, keybinds.tables.count());
    // try testing.expect(keybinds.set.bindings.count() > 0);
    assert!(keybinds.set.bindings_count() > 0);
}

// PORT NOTE: `std.mem.indexOf(u8, haystack, needle)` — first index of `needle`
// in `haystack` (or `null`).
fn find_sub(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}
