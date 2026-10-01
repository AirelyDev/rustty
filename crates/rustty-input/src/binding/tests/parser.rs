//! PORTS: ghostty src/input/Binding.zig — the `Parser` chain and sequence tests.

use super::*;

// test "parse: chain"
#[test]
fn parse_chain() {
    // Valid
    {
        let mut p = Parser::init(b"chain=new_tab").unwrap();
        assert_eq!(Some(Elem::Chain(Action::NewTab)), p.next().unwrap());
        assert!(p.next().unwrap().is_none());
    }

    // Chain can't have flags
    assert_eq!(
        Error::InvalidFormat,
        Parser::init(b"global:chain=ignore").err().unwrap()
    );

    // Chain can't be part of a sequence
    {
        let mut p = Parser::init(b"a>chain=ignore").unwrap();
        // PORT NOTE: `_ = try p.next();` — the leader element must PARSE.
        // `let _ = p.next()` discarded an `Err`, so the block passed even
        // when the leader itself failed to parse and the `InvalidFormat`
        // below came from the wrong element (S102-F1).
        p.next().unwrap();
        assert_eq!(Err(Error::InvalidFormat), p.next());
    }
}

// test "sequence iterator"
#[test]
fn sequence_iterator() {
    // single character
    {
        let mut it = SequenceIterator {
            input: b"a".to_vec(),
            i: 0,
        };
        assert_eq!(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            it.next().unwrap().unwrap()
        );
        assert!(it.next().unwrap().is_none());
    }

    // multi character
    {
        let mut it = SequenceIterator {
            input: b"a>b".to_vec(),
            i: 0,
        };
        assert_eq!(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            it.next().unwrap().unwrap()
        );
        assert_eq!(
            Trigger {
                key: uni(b'b' as u32),
                ..Default::default()
            },
            it.next().unwrap().unwrap()
        );
        assert!(it.next().unwrap().is_none());
    }

    // empty
    {
        let mut it = SequenceIterator {
            input: b"".to_vec(),
            i: 0,
        };
        assert_eq!(Err(Error::InvalidFormat), it.next());
    }

    // empty starting sequence
    {
        let mut it = SequenceIterator {
            input: b">a".to_vec(),
            i: 0,
        };
        assert_eq!(Err(Error::InvalidFormat), it.next());
    }

    // empty ending sequence
    {
        let mut it = SequenceIterator {
            input: b"a>".to_vec(),
            i: 0,
        };
        assert_eq!(
            Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            },
            it.next().unwrap().unwrap()
        );
        assert_eq!(Err(Error::InvalidFormat), it.next());
    }
}

// test "parse: sequences"
#[test]
fn parse_sequences() {
    // single character
    {
        let mut p = Parser::init(b"ctrl+a=ignore").unwrap();
        assert_eq!(
            Some(Elem::Binding(Binding {
                trigger: Trigger {
                    mods: Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                    key: uni(b'a' as u32),
                },
                action: Action::Ignore,
                flags: Flags::default(),
            })),
            p.next().unwrap()
        );
        assert!(p.next().unwrap().is_none());
    }

    // sequence
    {
        let mut p = Parser::init(b"a>b=ignore").unwrap();
        assert_eq!(
            Some(Elem::Leader(Trigger {
                key: uni(b'a' as u32),
                ..Default::default()
            })),
            p.next().unwrap()
        );
        assert_eq!(
            Some(Elem::Binding(Binding {
                trigger: Trigger {
                    key: uni(b'b' as u32),
                    ..Default::default()
                },
                action: Action::Ignore,
                flags: Flags::default(),
            })),
            p.next().unwrap()
        );
        assert!(p.next().unwrap().is_none());
    }
}
