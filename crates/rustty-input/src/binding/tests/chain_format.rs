//! PORTS: ghostty src/input/Binding.zig — the `Set.appendChain` and `Set.formatEntries` tests.

use super::*;

// test "set: appendChain with no parent returns error"
#[test]
fn set_append_chain_with_no_parent_returns_error() {
    let mut s = Set::default();

    assert_eq!(
        Err(AppendChainError::NoChainParent),
        s.append_chain(Action::NewTab)
    );
}

// test "set: appendChain after put converts to leaf_chained"
#[test]
fn set_append_chain_after_put_converts_to_leaf_chained() {
    let mut s = Set::default();

    s.put(
        Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        },
        Action::NewWindow,
    );

    // First appendChain converts leaf to leaf_chained and appends the new action
    s.append_chain(Action::NewTab).unwrap();

    let entry = s
        .get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap();
    assert!(matches!(entry, Value::LeafChained(_)));

    let chained = as_leaf_chained(entry);
    assert_eq!(2usize, chained.actions.len());
    assert!(chained.actions[0] == Action::NewWindow);
    assert!(chained.actions[1] == Action::NewTab);
}

// test "set: appendChain after putFlags preserves flags"
#[test]
fn set_append_chain_after_put_flags_preserves_flags() {
    let mut s = Set::default();

    s.put_flags(
        Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        },
        Action::NewWindow,
        Flags {
            consumed: false,
            ..Default::default()
        },
    );
    s.append_chain(Action::NewTab).unwrap();

    let entry = s
        .get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap();
    assert!(matches!(entry, Value::LeafChained(_)));

    let chained = as_leaf_chained(entry);
    assert!(!chained.flags.consumed);
    assert_eq!(2usize, chained.actions.len());
    assert!(chained.actions[0] == Action::NewWindow);
    assert!(chained.actions[1] == Action::NewTab);
}

// test "set: appendChain multiple times"
#[test]
fn set_append_chain_multiple_times() {
    let mut s = Set::default();

    s.put(
        Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        },
        Action::NewWindow,
    );
    s.append_chain(Action::NewTab).unwrap();
    s.append_chain(Action::CloseSurface).unwrap();

    let entry = s
        .get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap();
    assert!(matches!(entry, Value::LeafChained(_)));

    let chained = as_leaf_chained(entry);
    assert_eq!(3usize, chained.actions.len());
    assert!(chained.actions[0] == Action::NewWindow);
    assert!(chained.actions[1] == Action::NewTab);
    assert!(chained.actions[2] == Action::CloseSurface);
}

// test "set: appendChain removes reverse mapping"
#[test]
fn set_append_chain_removes_reverse_mapping() {
    let mut s = Set::default();

    s.put(
        Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        },
        Action::NewWindow,
    );

    // Verify reverse mapping exists before chaining
    assert!(s.get_trigger(&Action::NewWindow).is_some());

    // Chaining should remove the reverse mapping
    s.append_chain(Action::NewTab).unwrap();

    // Reverse mapping should be gone since chained actions are not in reverse map
    assert!(s.get_trigger(&Action::NewWindow).is_none());
}

// test "set: appendChain with performable does not affect reverse mapping"
#[test]
fn set_append_chain_with_performable_does_not_affect_reverse_mapping() {
    let mut s = Set::default();

    // Add a non-performable binding first
    s.put(
        Trigger {
            key: uni(b'b' as u32),
            ..Default::default()
        },
        Action::NewWindow,
    );
    assert!(s.get_trigger(&Action::NewWindow).is_some());

    // Add a performable binding (not in reverse map) and chain it
    s.put_flags(
        Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        },
        Action::CloseSurface,
        Flags {
            performable: true,
            ..Default::default()
        },
    );

    // close_surface was performable, so not in reverse map
    assert!(s.get_trigger(&Action::CloseSurface).is_none());

    // Chaining the performable binding should not crash or affect anything
    s.append_chain(Action::NewTab).unwrap();

    // The non-performable new_window binding should still be in reverse map
    assert!(s.get_trigger(&Action::NewWindow).is_some());
}

// test "set: appendChain restores next valid reverse mapping"
#[test]
fn set_append_chain_restores_next_valid_reverse_mapping() {
    let mut s = Set::default();

    // Add two bindings for the same action
    s.put(
        Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        },
        Action::NewWindow,
    );
    s.put(
        Trigger {
            key: uni(b'b' as u32),
            ..Default::default()
        },
        Action::NewWindow,
    );

    // Reverse mapping should point to 'b' (most recent)
    {
        let trigger = s.get_trigger(&Action::NewWindow).unwrap();
        assert!(trigger.key == uni(b'b' as u32));
    }

    // Chain an action to 'b', which should restore 'a' in the reverse map
    s.append_chain(Action::NewTab).unwrap();

    // Now reverse mapping should point to 'a'
    {
        let trigger = s.get_trigger(&Action::NewWindow).unwrap();
        assert!(trigger.key == uni(b'a' as u32));
    }
}

// PORT NOTE: helper reproducing the reference's formatEntries test prologue:
// `try entry.key_ptr.format(&writer);` writes the trigger into the shared
// fixed buffer, then `entry.value_ptr.formatEntries(&writer, formatter)`.
// Returns the formatter's accumulated output bytes.
fn format_entries_to_string(s: &Set, t: Trigger) -> Vec<u8> {
    let value = s.get_value(t).unwrap();
    let key = s.get_key(t).unwrap();

    let mut output = AllocatingWriter::init();
    let mut buf = RewindBuffer::new();

    // Write the trigger first (as formatEntry in Config.zig does)
    {
        let mut w = RewindWriter { inner: &mut buf };
        key.format(&mut w).unwrap();
    }
    let mut formatter = entry_formatter(b"keybind", &mut output);
    match value.format_entries(&mut buf, &mut formatter) {
        Ok(()) => {}
        Err(FormatEntriesError::OutOfMemory) => panic!("OutOfMemory"),
        Err(FormatEntriesError::Writer(_)) => unreachable!(),
    }
    output.written().to_vec()
}

// test "set: formatEntries leaf_chained"
#[test]
fn set_format_entries_leaf_chained() {
    let mut s = Set::default();

    // Create a chained binding
    s.parse_and_put(b"a=new_window").unwrap();
    s.parse_and_put(b"chain=new_tab").unwrap();

    // Verify it's a leaf_chained
    let t = Trigger {
        key: uni(b'a' as u32),
        ..Default::default()
    };
    assert!(matches!(s.get_value(t).unwrap(), Value::LeafChained(_)));

    // Format the entries
    let output = format_entries_to_string(&s, t);

    let expected: &[u8] = b"keybind = a=new_window\nkeybind = chain=new_tab\n";
    assert_eq!(expected, output.as_slice());
}

// test "set: formatEntries leaf_chained multiple chains"
#[test]
fn set_format_entries_leaf_chained_multiple_chains() {
    let mut s = Set::default();

    // Create a chained binding with 3 actions
    s.parse_and_put(b"ctrl+a=new_window").unwrap();
    s.parse_and_put(b"chain=new_tab").unwrap();
    s.parse_and_put(b"chain=close_surface").unwrap();

    // Verify it's a leaf_chained with 3 actions
    let t = Trigger {
        key: uni(b'a' as u32),
        mods: Mods {
            ctrl: true,
            ..Default::default()
        },
    };
    assert!(matches!(s.get_value(t).unwrap(), Value::LeafChained(_)));
    assert_eq!(
        3usize,
        as_leaf_chained(s.get_value(t).unwrap()).actions.len()
    );

    // Format the entries
    let output = format_entries_to_string(&s, t);

    let expected: &[u8] =
        b"keybind = ctrl+a=new_window\nkeybind = chain=new_tab\nkeybind = chain=close_surface\n";
    assert_eq!(expected, output.as_slice());
}

// test "set: formatEntries leaf_chained with text action"
#[test]
fn set_format_entries_leaf_chained_with_text_action() {
    let mut s = Set::default();

    // Create a chained binding with text actions
    s.parse_and_put(b"a=text:hello").unwrap();
    s.parse_and_put(b"chain=text:world").unwrap();

    // Format the entries
    let t = Trigger {
        key: uni(b'a' as u32),
        ..Default::default()
    };
    let output = format_entries_to_string(&s, t);

    let expected: &[u8] = b"keybind = a=text:hello\nkeybind = chain=text:world\n";
    assert_eq!(expected, output.as_slice());
}
