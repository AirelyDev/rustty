//! PORTS: ghostty src/input/Binding.zig — the `Set` tests (`parseAndPut`, `chain_parent`, `clone`,
//! reverse mapping, `getEvent`).

use super::*;

// test "set: parseAndPut typical binding"
#[test]
fn set_parse_and_put_typical_binding() {
    let mut s = Set::default();

    s.parse_and_put(b"a=new_window").unwrap();

    // Creates forward mapping
    {
        let action = as_leaf(
            s.get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap(),
        );
        assert!(action.action == Action::NewWindow);
        assert_eq!(Flags::default(), action.flags);
    }

    // Creates reverse mapping
    {
        let trigger = s.get_trigger(&Action::NewWindow).unwrap();
        assert!(trigger.key == uni(b'a' as u32));
    }

    // Sets up the chain parent properly
    assert!(s.chain_parent().is_some());
    assert_eq!(b"a", chain_parent_key_str(&s).as_slice());
}

// test "set: parseAndPut unconsumed binding"
#[test]
fn set_parse_and_put_unconsumed_binding() {
    let mut s = Set::default();

    s.parse_and_put(b"unconsumed:a=new_window").unwrap();

    // Creates forward mapping
    {
        let trigger = Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        };
        let action = as_leaf(s.get_value(trigger).unwrap());
        assert!(action.action == Action::NewWindow);
        assert_eq!(
            Flags {
                consumed: false,
                ..Default::default()
            },
            action.flags
        );
    }

    // Creates reverse mapping
    {
        let trigger = s.get_trigger(&Action::NewWindow).unwrap();
        assert!(trigger.key == uni(b'a' as u32));
    }

    // Sets up the chain parent properly
    assert!(s.chain_parent().is_some());
    assert_eq!(b"a", chain_parent_key_str(&s).as_slice());
}

// test "set: parseAndPut removed binding"
#[test]
fn set_parse_and_put_removed_binding() {
    let mut s = Set::default();

    s.parse_and_put(b"a=new_window").unwrap();
    s.parse_and_put(b"a=unbind").unwrap();

    // Creates forward mapping
    {
        let trigger = Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        };
        assert!(s.get_value(trigger).is_none());
    }
    assert!(s.get_trigger(&Action::NewWindow).is_none());

    // Sets up the chain parent properly
    assert!(s.chain_parent().is_none());
}

// test "set: put sets chain_parent"
#[test]
fn set_put_sets_chain_parent() {
    let mut s = Set::default();

    s.put(
        Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        },
        Action::NewWindow,
    );

    // chain_parent should be set
    assert!(s.chain_parent().is_some());
    assert_eq!(b"a", chain_parent_key_str(&s).as_slice());

    // chain_parent value should be a leaf
    assert!(matches!(chain_parent_value(&s), Value::Leaf(_)));
}

// test "set: putFlags sets chain_parent"
#[test]
fn set_put_flags_sets_chain_parent() {
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

    // chain_parent should be set
    assert!(s.chain_parent().is_some());
    assert_eq!(b"a", chain_parent_key_str(&s).as_slice());

    // chain_parent value should be a leaf with correct flags
    assert!(matches!(chain_parent_value(&s), Value::Leaf(_)));
    assert!(!as_leaf(chain_parent_value(&s)).flags.consumed);
}

// test "set: sequence sets chain_parent to final leaf"
#[test]
fn set_sequence_sets_chain_parent_to_final_leaf() {
    let mut s = Set::default();

    s.parse_and_put(b"a>b=new_window").unwrap();

    // chain_parent should be set and point to 'b' (the final leaf)
    assert!(s.chain_parent().is_some());
    assert_eq!(b"b", chain_parent_key_str(&s).as_slice());

    // chain_parent value should be a leaf
    assert!(matches!(chain_parent_value(&s), Value::Leaf(_)));
    assert!(as_leaf(chain_parent_value(&s)).action == Action::NewWindow);
}

// test "set: multiple leaves under leader updates chain_parent"
#[test]
fn set_multiple_leaves_under_leader_updates_chain_parent() {
    let mut s = Set::default();

    s.parse_and_put(b"a>b=new_window").unwrap();

    // After first binding, chain_parent should be 'b'
    assert!(s.chain_parent().is_some());
    assert_eq!(b"b", chain_parent_key_str(&s).as_slice());

    s.parse_and_put(b"a>c=new_tab").unwrap();

    // After second binding, chain_parent should be updated to 'c'
    assert!(s.chain_parent().is_some());
    assert_eq!(b"c", chain_parent_key_str(&s).as_slice());
    assert!(as_leaf(chain_parent_value(&s)).action == Action::NewTab);
}

// test "set: sequence unbind clears chain_parent"
#[test]
fn set_sequence_unbind_clears_chain_parent() {
    let mut s = Set::default();

    s.parse_and_put(b"a>b=new_window").unwrap();
    assert!(s.chain_parent().is_some());

    s.parse_and_put(b"a>b=unbind").unwrap();

    // After unbind, chain_parent should be cleared
    assert!(s.chain_parent().is_none());
}

// test "set: sequence unbind with remaining leaves clears chain_parent"
#[test]
fn set_sequence_unbind_with_remaining_leaves_clears_chain_parent() {
    let mut s = Set::default();

    s.parse_and_put(b"a>b=new_window").unwrap();
    s.parse_and_put(b"a>c=new_tab").unwrap();
    s.parse_and_put(b"a>b=unbind").unwrap();

    // After unbind, chain_parent should be cleared even though 'c' remains
    assert!(s.chain_parent().is_none());

    // But 'c' should still exist
    let a_entry = s
        .get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap();
    assert!(matches!(a_entry, Value::Leader(_)));
    if let Value::Leader(inner_set) = a_entry {
        assert!(inner_set
            .get_value(Trigger {
                key: uni(b'c' as u32),
                ..Default::default()
            })
            .is_some());
    }
}

// test "set: direct remove clears chain_parent"
#[test]
fn set_direct_remove_clears_chain_parent() {
    let mut s = Set::default();

    s.put(
        Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        },
        Action::NewWindow,
    );
    assert!(s.chain_parent().is_some());

    s.remove(Trigger {
        key: uni(b'a' as u32),
        ..Default::default()
    });

    // After removal, chain_parent should be cleared
    assert!(s.chain_parent().is_none());
}

// test "set: invalid format preserves chain_parent"
#[test]
fn set_invalid_format_preserves_chain_parent() {
    let mut s = Set::default();

    s.parse_and_put(b"a=new_window").unwrap();
    let before = s.chain_parent().unwrap().clone();
    // const before_key = s.chain_parent.?.key_ptr;
    // const before_value = s.chain_parent.?.value_ptr;
    //
    // PORT NOTE (S-8c): the Rust chain parent records the entry's trigger
    // rather than raw entry pointers, so the reference's two pointer
    // captures are the addresses of the entry's stored key and value,
    // resolved through that trigger.
    let before_key: *const Trigger = s.get_key(before.key).unwrap();
    let before_value: *const Value = s.get_value(before.key).unwrap();

    // Try an invalid parse - should fail
    assert_eq!(
        Err(ParseAndPutError::InvalidAction),
        s.parse_and_put(b"a=invalid_action_xyz")
    );

    // chain_parent should be unchanged
    assert!(s.chain_parent().is_some());
    assert_eq!(&before, s.chain_parent().unwrap());
    // try testing.expect(s.chain_parent.?.key_ptr == before_key);
    let after = s.chain_parent().unwrap().key;
    assert!(std::ptr::eq(s.get_key(after).unwrap(), before_key));
    // try testing.expect(s.chain_parent.?.value_ptr == before_value);
    assert!(std::ptr::eq(s.get_value(after).unwrap(), before_value));
}

// test "set: clone produces null chain_parent"
#[test]
fn set_clone_produces_null_chain_parent() {
    let mut s = Set::default();

    s.parse_and_put(b"a=new_window").unwrap();
    assert!(s.chain_parent().is_some());

    let cloned = s.clone_set();

    // Clone should have null chain_parent
    assert!(cloned.chain_parent().is_none());

    // But should have the binding
    assert!(cloned
        .get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .is_some());
}

// PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD365. The
// reference's `GenericLeaf.actions.many` is a slice INTO the set's
// `leaf_chained` list, which lives in the surface's (or the embedded
// app's) config arena. A chained `reload_config` replaces that config
// synchronously on macOS (the action callback calls
// `ghostty_app_update_config`), so `maybeHandleBinding`'s `for (actions)`
// loop and its closing/ignore checks, and `App.keyEvent` ->
// `performAllChainedAction`, then read freed memory. The port's
// `GenericLeaf` owns a copy of the actions, so they outlive the set. This
// test fails on the reference's shape: with a borrowed slice, `leaf` could
// not outlive the set it borrows (replacing `s` below, `drop(mem::take(&mut
// s))`, would not compile; the `compile_fail` doctest on `GenericLeaf` is that
// arm), and an unchecked borrow would be the use-after-free Miri reports.
#[test]
fn ud365_generic_leaf_actions_outlive_a_replaced_set() {
    let mut s = Set::default();
    s.parse_and_put(b"a=reload_config").unwrap();
    s.parse_and_put(b"chain=text:world").unwrap();
    let leaf = match s
        .get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap()
    {
        Value::LeafChained(l) => l.generic(),
        other => panic!("expected leaf_chained, got {other:?}"),
    };
    // The config reload: the set (and everything it owns) is replaced
    // while the chain is being performed.
    drop(std::mem::take(&mut s));
    s.parse_and_put(b"a=new_window").unwrap();

    let actions = leaf.actions_slice();
    assert_eq!(2, actions.len());
    assert!(actions[0] == Action::ReloadConfig);
    assert!(actions[1] == Action::Text(b"world".to_vec()));
}

// test "set: clone with leaf_chained"
#[test]
fn set_clone_with_leaf_chained() {
    let mut s = Set::default();

    // Create a chained binding using parseAndPut with chain=
    s.parse_and_put(b"a=new_window").unwrap();
    s.parse_and_put(b"chain=new_tab").unwrap();

    // Verify we have a leaf_chained
    let entry = s
        .get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap();
    assert!(matches!(entry, Value::LeafChained(_)));
    assert_eq!(2usize, as_leaf_chained(entry).actions.len());

    // Clone the set
    let cloned = s.clone_set();

    // Verify the cloned set has the leaf_chained with same actions
    let cloned_entry = cloned
        .get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap();
    assert!(matches!(cloned_entry, Value::LeafChained(_)));
    assert_eq!(2usize, as_leaf_chained(cloned_entry).actions.len());
    assert!(as_leaf_chained(cloned_entry).actions[0] == Action::NewWindow);
    assert!(as_leaf_chained(cloned_entry).actions[1] == Action::NewTab);
}

// test "set: clone with leaf_chained containing allocated data"
#[test]
fn set_clone_with_leaf_chained_containing_allocated_data() {
    let mut s = Set::default();

    // Create a chained binding with text actions (which have allocated strings)
    s.parse_and_put(b"a=text:hello").unwrap();
    s.parse_and_put(b"chain=text:world").unwrap();

    // Verify we have a leaf_chained
    let entry = s
        .get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap();
    assert!(matches!(entry, Value::LeafChained(_)));

    // Clone the set
    let cloned = s.clone_set();

    // Verify the cloned set has independent copies of the text
    let cloned_entry = cloned
        .get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap();
    assert!(matches!(cloned_entry, Value::LeafChained(_)));
    assert_eq!(b"hello", text_of(&as_leaf_chained(cloned_entry).actions[0]));
    assert_eq!(b"world", text_of(&as_leaf_chained(cloned_entry).actions[1]));

    // Verify the pointers are different (truly cloned, not shared)
    //
    // PORT NOTE (S-8c): ported as the reference's pointer comparison. It
    // used to be dropped as "guaranteed by the type" (a cloned `Vec<u8>`
    // always owns a fresh buffer); R1 keeps the assertion anyway.
    assert!(
        text_of(&as_leaf_chained(entry).actions[0]).as_ptr()
            != text_of(&as_leaf_chained(cloned_entry).actions[0]).as_ptr()
    );
}

fn text_of(a: &Action) -> &[u8] {
    match a {
        Action::Text(v) => v,
        _ => panic!("expected text"),
    }
}

// test "set: parseAndPut sequence"
#[test]
fn set_parse_and_put_sequence() {
    let mut s = Set::default();

    s.parse_and_put(b"a>b=new_window").unwrap();
    {
        let t = Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        };
        let e = s.get_value(t).unwrap();
        assert!(matches!(e, Value::Leader(_)));
        if let Value::Leader(inner) = e {
            let t = Trigger {
                key: uni(b'b' as u32),
                ..Default::default()
            };
            let e = inner.get_value(t).unwrap();
            assert!(matches!(e, Value::Leaf(_)));
            assert!(as_leaf(e).action == Action::NewWindow);
            assert_eq!(Flags::default(), as_leaf(e).flags);
        }
    }
}

// test "set: parseAndPut sequence with two actions"
#[test]
fn set_parse_and_put_sequence_with_two_actions() {
    let mut s = Set::default();

    s.parse_and_put(b"a>b=new_window").unwrap();
    s.parse_and_put(b"a>c=new_tab").unwrap();
    let e = s
        .get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap();
    assert!(matches!(e, Value::Leader(_)));
    if let Value::Leader(inner) = e {
        {
            let e = inner
                .get_value(Trigger {
                    key: uni(b'b' as u32),
                    ..Default::default()
                })
                .unwrap();
            assert!(matches!(e, Value::Leaf(_)));
            assert!(as_leaf(e).action == Action::NewWindow);
            assert_eq!(Flags::default(), as_leaf(e).flags);
        }
        {
            let e = inner
                .get_value(Trigger {
                    key: uni(b'c' as u32),
                    ..Default::default()
                })
                .unwrap();
            assert!(matches!(e, Value::Leaf(_)));
            assert!(as_leaf(e).action == Action::NewTab);
            assert_eq!(Flags::default(), as_leaf(e).flags);
        }
    }
}

// test "set: parseAndPut overwrite sequence"
#[test]
fn set_parse_and_put_overwrite_sequence() {
    let mut s = Set::default();

    s.parse_and_put(b"a>b=new_tab").unwrap();
    s.parse_and_put(b"a>b=new_window").unwrap();
    let e = s
        .get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap();
    assert!(matches!(e, Value::Leader(_)));
    if let Value::Leader(inner) = e {
        let e = inner
            .get_value(Trigger {
                key: uni(b'b' as u32),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(e, Value::Leaf(_)));
        assert!(as_leaf(e).action == Action::NewWindow);
        assert_eq!(Flags::default(), as_leaf(e).flags);
    }
}

// test "set: parseAndPut overwrite leader"
#[test]
fn set_parse_and_put_overwrite_leader() {
    let mut s = Set::default();

    s.parse_and_put(b"a=new_tab").unwrap();
    s.parse_and_put(b"a>b=new_window").unwrap();
    let e = s
        .get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap();
    assert!(matches!(e, Value::Leader(_)));
    if let Value::Leader(inner) = e {
        let e = inner
            .get_value(Trigger {
                key: uni(b'b' as u32),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(e, Value::Leaf(_)));
        assert!(as_leaf(e).action == Action::NewWindow);
        assert_eq!(Flags::default(), as_leaf(e).flags);
    }
}

// test "set: parseAndPut unbind sequence unbinds leader"
#[test]
fn set_parse_and_put_unbind_sequence_unbinds_leader() {
    let mut s = Set::default();

    s.parse_and_put(b"a>b=new_window").unwrap();
    s.parse_and_put(b"a>b=unbind").unwrap();
    {
        let t = Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        };
        assert!(s.get_value(t).is_none());
    }
}

// test "set: parseAndPut unbind sequence unbinds leader if not set"
#[test]
fn set_parse_and_put_unbind_sequence_unbinds_leader_if_not_set() {
    let mut s = Set::default();

    s.parse_and_put(b"a>b=unbind").unwrap();
    {
        let t = Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        };
        assert!(s.get_value(t).is_none());
    }
}

// test "set: parseAndPut sequence preserves reverse mapping"
#[test]
fn set_parse_and_put_sequence_preserves_reverse_mapping() {
    let mut s = Set::default();

    s.parse_and_put(b"a=new_window").unwrap();
    s.parse_and_put(b"ctrl+a>b=new_window").unwrap();

    // Creates reverse mapping
    {
        let trigger = s.get_trigger(&Action::NewWindow).unwrap();
        assert!(trigger.key == uni(b'a' as u32));
    }
}

// test "set: put overwrites sequence"
#[test]
fn set_put_overwrites_sequence() {
    let mut s = Set::default();

    s.parse_and_put(b"ctrl+a>b=new_window").unwrap();
    s.put(
        Trigger {
            mods: Mods {
                ctrl: true,
                ..Default::default()
            },
            key: uni(b'a' as u32),
        },
        Action::NewWindow,
    );

    // Creates reverse mapping
    {
        let trigger = s.get_trigger(&Action::NewWindow).unwrap();
        assert!(trigger.key == uni(b'a' as u32));
    }
}

// test "set: maintains reverse mapping"
#[test]
fn set_maintains_reverse_mapping() {
    let mut s = Set::default();

    s.put(
        Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        },
        Action::NewWindow,
    );
    {
        let trigger = s.get_trigger(&Action::NewWindow).unwrap();
        assert!(trigger.key == uni(b'a' as u32));
    }

    // should be most recent
    s.put(
        Trigger {
            key: uni(b'b' as u32),
            ..Default::default()
        },
        Action::NewWindow,
    );
    {
        let trigger = s.get_trigger(&Action::NewWindow).unwrap();
        assert!(trigger.key == uni(b'b' as u32));
    }

    // removal should replace
    s.remove(Trigger {
        key: uni(b'b' as u32),
        ..Default::default()
    });
    {
        let trigger = s.get_trigger(&Action::NewWindow).unwrap();
        assert!(trigger.key == uni(b'a' as u32));
    }
}

// test "set: performable is not part of reverse mappings"
#[test]
fn set_performable_is_not_part_of_reverse_mappings() {
    let mut s = Set::default();

    s.put(
        Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        },
        Action::NewWindow,
    );
    {
        let trigger = s.get_trigger(&Action::NewWindow).unwrap();
        assert!(trigger.key == uni(b'a' as u32));
    }

    // trigger should be non-performable
    s.put_flags(
        Trigger {
            key: uni(b'b' as u32),
            ..Default::default()
        },
        Action::NewWindow,
        Flags {
            performable: true,
            ..Default::default()
        },
    );
    {
        let trigger = s.get_trigger(&Action::NewWindow).unwrap();
        assert!(trigger.key == uni(b'a' as u32));
    }

    // removal of performable should do nothing
    s.remove(Trigger {
        key: uni(b'b' as u32),
        ..Default::default()
    });
    {
        let trigger = s.get_trigger(&Action::NewWindow).unwrap();
        assert!(trigger.key == uni(b'a' as u32));
    }
}

// test "set: overriding a mapping updates reverse"
#[test]
fn set_overriding_a_mapping_updates_reverse() {
    let mut s = Set::default();

    s.put(
        Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        },
        Action::NewWindow,
    );
    {
        let trigger = s.get_trigger(&Action::NewWindow).unwrap();
        assert!(trigger.key == uni(b'a' as u32));
    }

    // should be most recent
    s.put(
        Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        },
        Action::NewTab,
    );
    {
        let trigger = s.get_trigger(&Action::NewWindow);
        assert!(trigger.is_none());
    }
}

// test "set: consumed state"
#[test]
fn set_consumed_state() {
    let mut s = Set::default();

    s.put(
        Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        },
        Action::NewWindow,
    );
    assert!(matches!(
        s.get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap(),
        Value::Leaf(_)
    ));
    assert!(
        as_leaf(
            s.get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap()
        )
        .flags
        .consumed
    );

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
    assert!(matches!(
        s.get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap(),
        Value::Leaf(_)
    ));
    assert!(
        !as_leaf(
            s.get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap()
        )
        .flags
        .consumed
    );

    s.put(
        Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        },
        Action::NewWindow,
    );
    assert!(matches!(
        s.get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap(),
        Value::Leaf(_)
    ));
    assert!(
        as_leaf(
            s.get_value(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })
            .unwrap()
        )
        .flags
        .consumed
    );
}

// test "set: parseAndPut chain"
#[test]
fn set_parse_and_put_chain() {
    let mut s = Set::default();

    s.parse_and_put(b"a=new_window").unwrap();
    s.parse_and_put(b"chain=new_tab").unwrap();

    // Creates forward mapping as leaf_chained
    {
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

    // Does not create reverse mapping, because reverse mappings are only for
    // non-chained actions.
    {
        assert!(s.get_trigger(&Action::NewWindow).is_none());
    }
}

// test "set: parseAndPut chain without parent is error"
#[test]
fn set_parse_and_put_chain_without_parent_is_error() {
    let mut s = Set::default();

    // Chain without a prior binding should fail
    assert_eq!(
        Err(ParseAndPutError::InvalidFormat),
        s.parse_and_put(b"chain=new_tab")
    );
}

// test "set: parseAndPut chain multiple times"
#[test]
fn set_parse_and_put_chain_multiple_times() {
    let mut s = Set::default();

    s.parse_and_put(b"a=new_window").unwrap();
    s.parse_and_put(b"chain=new_tab").unwrap();
    s.parse_and_put(b"chain=close_surface").unwrap();

    // Should have 3 actions chained
    {
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
}

// test "set: parseAndPut chain preserves flags"
#[test]
fn set_parse_and_put_chain_preserves_flags() {
    let mut s = Set::default();

    s.parse_and_put(b"unconsumed:a=new_window").unwrap();
    s.parse_and_put(b"chain=new_tab").unwrap();

    // Should preserve unconsumed flag
    {
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
    }
}

// test "set: parseAndPut chain after unbind is error"
#[test]
fn set_parse_and_put_chain_after_unbind_is_error() {
    let mut s = Set::default();

    s.parse_and_put(b"a=new_window").unwrap();
    s.parse_and_put(b"a=unbind").unwrap();

    // Chain after unbind should fail because chain_parent is cleared
    assert_eq!(
        Err(ParseAndPutError::InvalidFormat),
        s.parse_and_put(b"chain=new_tab")
    );
}

// test "set: parseAndPut chain on sequence"
#[test]
fn set_parse_and_put_chain_on_sequence() {
    let mut s = Set::default();

    s.parse_and_put(b"a>b=new_window").unwrap();
    s.parse_and_put(b"chain=new_tab").unwrap();

    // Navigate to the inner set
    let a_entry = s
        .get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap();
    assert!(matches!(a_entry, Value::Leader(_)));
    if let Value::Leader(inner_set) = a_entry {
        // Check the chained binding
        let b_entry = inner_set
            .get_value(Trigger {
                key: uni(b'b' as u32),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(b_entry, Value::LeafChained(_)));
        let chained = as_leaf_chained(b_entry);
        assert_eq!(2usize, chained.actions.len());
        assert!(chained.actions[0] == Action::NewWindow);
        assert!(chained.actions[1] == Action::NewTab);
    }
}

// test "set: parseAndPut chain with unbind is error"
#[test]
fn set_parse_and_put_chain_with_unbind_is_error() {
    let mut s = Set::default();

    s.parse_and_put(b"a=new_window").unwrap();

    // chain=unbind is not valid
    assert_eq!(
        Err(ParseAndPutError::InvalidFormat),
        s.parse_and_put(b"chain=unbind")
    );

    // Original binding should still exist
    let entry = s
        .get_value(Trigger {
            key: uni(b'a' as u32),
            ..Default::default()
        })
        .unwrap();
    assert!(matches!(entry, Value::Leaf(_)));
    assert!(as_leaf(entry).action == Action::NewWindow);
}

// PORT NOTE: helper to construct a KeyEvent the way the reference's getEvent
// tests do (`.{ .key = ..., .mods = ..., .utf8 = ..., .unshifted_codepoint = ... }`).
fn ev(key: KeyKey, mods: Mods, utf8: &[u8], unshifted_codepoint: u32) -> KeyEvent {
    KeyEvent {
        key,
        mods,
        utf8: utf8.to_vec(),
        unshifted_codepoint,
        ..Default::default()
    }
}

// test "set: getEvent physical"
#[test]
fn set_get_event_physical() {
    let mut s = Set::default();

    s.parse_and_put(b"ctrl+quote=new_window").unwrap();

    // Physical matches on physical
    {
        let idx = s
            .get_event(&ev(
                KeyKey::Quote,
                Mods {
                    ctrl: true,
                    ..Default::default()
                },
                b"",
                0,
            ))
            .unwrap();
        let action = as_leaf(s.value_at(idx));
        assert!(action.action == Action::NewWindow);
    }

    // Physical does not match on UTF8/codepoint
    {
        let action = s.get_event(&ev(
            KeyKey::KeyA,
            Mods {
                ctrl: true,
                ..Default::default()
            },
            b"'",
            b'\'' as u32,
        ));
        assert!(action.is_none());
    }
}

// test "set: getEvent codepoint"
#[test]
fn set_get_event_codepoint() {
    let mut s = Set::default();

    s.parse_and_put(b"ctrl+'=new_window").unwrap();

    // Matches on codepoint
    {
        let idx = s
            .get_event(&ev(
                KeyKey::KeyA,
                Mods {
                    ctrl: true,
                    ..Default::default()
                },
                b"",
                b'\'' as u32,
            ))
            .unwrap();
        let action = as_leaf(s.value_at(idx));
        assert!(action.action == Action::NewWindow);
    }

    // Matches on UTF-8
    {
        let idx = s
            .get_event(&ev(
                KeyKey::KeyA,
                Mods {
                    ctrl: true,
                    ..Default::default()
                },
                b"'",
                0,
            ))
            .unwrap();
        let action = as_leaf(s.value_at(idx));
        assert!(action.action == Action::NewWindow);
    }

    // Doesn't match on physical
    {
        let action = s.get_event(&ev(
            KeyKey::KeyA,
            Mods {
                ctrl: true,
                ..Default::default()
            },
            b"",
            0,
        ));
        assert!(action.is_none());
    }
}

// test "set: getEvent codepoint case folding"
#[test]
fn set_get_event_codepoint_case_folding() {
    let mut s = Set::default();

    s.parse_and_put(b"ctrl+A=new_window").unwrap();

    // Lowercase codepoint
    {
        let idx = s
            .get_event(&ev(
                KeyKey::KeyJ,
                Mods {
                    ctrl: true,
                    ..Default::default()
                },
                b"",
                b'a' as u32,
            ))
            .unwrap();
        let action = as_leaf(s.value_at(idx));
        assert!(action.action == Action::NewWindow);
    }

    // Uppercase codepoint
    {
        let idx = s
            .get_event(&ev(
                KeyKey::KeyA,
                Mods {
                    ctrl: true,
                    ..Default::default()
                },
                b"",
                b'A' as u32,
            ))
            .unwrap();
        let action = as_leaf(s.value_at(idx));
        assert!(action.action == Action::NewWindow);
    }

    // Negative case for sanity
    {
        let action = s.get_event(&ev(
            KeyKey::KeyJ,
            Mods {
                ctrl: true,
                ..Default::default()
            },
            b"",
            0,
        ));
        assert!(action.is_none());
    }
}

// test "set: getEvent catch_all fallback"
#[test]
fn set_get_event_catch_all_fallback() {
    let mut s = Set::default();

    s.parse_and_put(b"catch_all=ignore").unwrap();

    // Matches unbound key without modifiers
    {
        let idx = s
            .get_event(&ev(KeyKey::KeyA, Mods::default(), b"", 0))
            .unwrap();
        let action = as_leaf(s.value_at(idx));
        assert!(action.action == Action::Ignore);
    }

    // Matches unbound key with modifiers (falls back to catch_all without mods)
    {
        let idx = s
            .get_event(&ev(
                KeyKey::KeyA,
                Mods {
                    ctrl: true,
                    ..Default::default()
                },
                b"",
                0,
            ))
            .unwrap();
        let action = as_leaf(s.value_at(idx));
        assert!(action.action == Action::Ignore);
    }

    // Specific binding takes precedence over catch_all
    s.parse_and_put(b"ctrl+b=new_window").unwrap();
    {
        let idx = s
            .get_event(&ev(
                KeyKey::KeyB,
                Mods {
                    ctrl: true,
                    ..Default::default()
                },
                b"",
                b'b' as u32,
            ))
            .unwrap();
        let action = as_leaf(s.value_at(idx));
        assert!(action.action == Action::NewWindow);
    }
}

// test "set: getEvent catch_all with modifiers"
#[test]
fn set_get_event_catch_all_with_modifiers() {
    let mut s = Set::default();

    s.parse_and_put(b"ctrl+catch_all=close_surface").unwrap();
    s.parse_and_put(b"catch_all=ignore").unwrap();

    // Key with ctrl matches catch_all with ctrl modifier
    {
        let idx = s
            .get_event(&ev(
                KeyKey::KeyA,
                Mods {
                    ctrl: true,
                    ..Default::default()
                },
                b"",
                0,
            ))
            .unwrap();
        let action = as_leaf(s.value_at(idx));
        assert!(action.action == Action::CloseSurface);
    }

    // Key without mods matches catch_all without mods
    {
        let idx = s
            .get_event(&ev(KeyKey::KeyA, Mods::default(), b"", 0))
            .unwrap();
        let action = as_leaf(s.value_at(idx));
        assert!(action.action == Action::Ignore);
    }

    // Key with different mods falls back to catch_all without mods
    {
        let idx = s
            .get_event(&ev(
                KeyKey::KeyA,
                Mods {
                    alt: true,
                    ..Default::default()
                },
                b"",
                0,
            ))
            .unwrap();
        let action = as_leaf(s.value_at(idx));
        assert!(action.action == Action::Ignore);
    }
}
