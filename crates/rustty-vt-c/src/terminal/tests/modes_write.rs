//! PORTS: ghostty src/terminal/c/terminal.zig — tests: mode get/set
//! (`.mode` / `.mode_default`) and `vt_write` / `vt_write_until_ground`.

use super::*;

// test "set and get mode"
#[test]
fn set_and_get_mode() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // DEC mode 25 (cursor_visible) defaults to true
        // const cursor_visible: modes.ModeTag.Backing = @bitCast(modes.ModeTag{ .value = 25, .ansi = false });
        let cursor_visible = modes::ModeTag {
            value: 25,
            ansi: false,
        }
        .to_backing();
        // var config: ModeConfig = .{ .mode = cursor_visible, .value = undefined };
        let mut config = ModeConfig {
            mode: cursor_visible,
            value: false,
        };
        // try testing.expectEqual(Result.success, get(t, .mode, @ptrCast(&config)));
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::Mode,
                &mut config as *mut ModeConfig as *mut c_void,
            )
        });
        // try testing.expect(config.value);
        assert!(config.value);

        // Set it to false
        config.value = false;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::Mode,
                &config as *const ModeConfig as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::Mode,
                &mut config as *mut ModeConfig as *mut c_void,
            )
        });
        assert!(!config.value);

        // ANSI mode 4 (insert) defaults to false
        let insert = modes::ModeTag {
            value: 4,
            ansi: true,
        }
        .to_backing();
        config.mode = insert;
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::Mode,
                &mut config as *mut ModeConfig as *mut c_void,
            )
        });
        assert!(!config.value);

        config.value = true;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::Mode,
                &config as *const ModeConfig as *const c_void,
            )
        });
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::Mode,
                &mut config as *mut ModeConfig as *mut c_void,
            )
        });
        assert!(config.value);
        unsafe { free(t) };
    });
}

// test "set mode default updates current and reset value"
#[test]
fn set_mode_default_updates_current_and_reset_value() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // const tag: modes.ModeTag.Backing = @bitCast(modes.ModeTag{ .value = 2027, .ansi = false });
        let tag = modes::ModeTag {
            value: 2027,
            ansi: false,
        }
        .to_backing();
        // var config: ModeConfig = .{ .mode = tag, .value = true };
        let mut config = ModeConfig {
            mode: tag,
            value: true,
        };
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ModeDefault,
                &config as *const ModeConfig as *const c_void,
            )
        });
        // try testing.expect(t.?.terminal.modes.get(.grapheme_cluster));
        assert!(unsafe { (*(*t).terminal).modes.get(modes::Mode::GraphemeCluster) });
        // try testing.expect(t.?.terminal.modes.default.grapheme_cluster);
        assert!(unsafe {
            (*(*t).terminal)
                .modes
                .default
                .get_mode(modes::Mode::GraphemeCluster)
        });

        config.value = false;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::Mode,
                &config as *const ModeConfig as *const c_void,
            )
        });
        assert!(!unsafe { (*(*t).terminal).modes.get(modes::Mode::GraphemeCluster) });
        assert!(unsafe {
            (*(*t).terminal)
                .modes
                .default
                .get_mode(modes::Mode::GraphemeCluster)
        });

        // Setting the default also unconditionally replaces the current value.
        config.value = true;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ModeDefault,
                &config as *const ModeConfig as *const c_void,
            )
        });
        assert!(unsafe { (*(*t).terminal).modes.get(modes::Mode::GraphemeCluster) });

        config.value = false;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::Mode,
                &config as *const ModeConfig as *const c_void,
            )
        });
        // vt_write(t, "\x1bc", 2);
        let ris = b"\x1bc";
        unsafe { vt_write(t, ris.as_ptr(), ris.len()) };
        assert!(unsafe { (*(*t).terminal).modes.get(modes::Mode::GraphemeCluster) });

        config.value = false;
        assert_eq!(Result::Success, unsafe {
            set(
                t,
                Option_::ModeDefault,
                &config as *const ModeConfig as *const c_void,
            )
        });
        assert!(!unsafe { (*(*t).terminal).modes.get(modes::Mode::GraphemeCluster) });
        assert!(!unsafe {
            (*(*t).terminal)
                .modes
                .default
                .get_mode(modes::Mode::GraphemeCluster)
        });
        unsafe { free(t) };
    });
}

// test "set mode default validates configuration"
#[test]
fn set_mode_default_validates_configuration() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // try testing.expectEqual(Result.invalid_value, set(t, .mode_default, null));
        assert_eq!(Result::InvalidValue, unsafe {
            set(t, Option_::ModeDefault, std::ptr::null())
        });

        // var config: ModeConfig = .{ .mode = @bitCast(modes.ModeTag{ .value = 9999, .ansi = false }), .value = true };
        let mut config = ModeConfig {
            mode: modes::ModeTag {
                value: 9999,
                ansi: false,
            }
            .to_backing(),
            value: true,
        };
        assert_eq!(Result::InvalidValue, unsafe {
            set(
                t,
                Option_::ModeDefault,
                &config as *const ModeConfig as *const c_void,
            )
        });

        // config.mode = @bitCast(modes.ModeTag{ .value = 1047, .ansi = false });
        config.mode = modes::ModeTag {
            value: 1047,
            ansi: false,
        }
        .to_backing();
        assert_eq!(Result::InvalidValue, unsafe {
            set(
                t,
                Option_::ModeDefault,
                &config as *const ModeConfig as *const c_void,
            )
        });
        unsafe { free(t) };
    });
}

// test "set and get mode null"
#[test]
fn set_and_get_mode_null() {
    // const tag: modes.ModeTag.Backing = @bitCast(modes.ModeTag{ .value = 25, .ansi = false });
    let tag = modes::ModeTag {
        value: 25,
        ansi: false,
    }
    .to_backing();
    // var config: ModeConfig = .{ .mode = tag, .value = true };
    let mut config = ModeConfig {
        mode: tag,
        value: true,
    };
    // try testing.expectEqual(Result.invalid_value, set(null, .mode, @ptrCast(&config)));
    assert_eq!(Result::InvalidValue, unsafe {
        set(
            std::ptr::null_mut(),
            Option_::Mode,
            &config as *const ModeConfig as *const c_void,
        )
    });
    // try testing.expectEqual(Result.invalid_value, get(null, .mode, @ptrCast(&config)));
    assert_eq!(Result::InvalidValue, unsafe {
        get(
            std::ptr::null_mut(),
            TerminalData::Mode,
            &mut config as *mut ModeConfig as *mut c_void,
        )
    });
}

// test "set and get mode validate configuration"
#[test]
fn set_and_get_mode_validate_configuration() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // try testing.expectEqual(Result.invalid_value, set(t, .mode, null));
        assert_eq!(Result::InvalidValue, unsafe {
            set(t, Option_::Mode, std::ptr::null())
        });
        // try testing.expectEqual(Result.invalid_value, get(t, .mode, null));
        assert_eq!(Result::InvalidValue, unsafe {
            get(t, TerminalData::Mode, std::ptr::null_mut())
        });

        // var config: ModeConfig = .{ .mode = @bitCast(modes.ModeTag{ .value = 9999, .ansi = false }), .value = true };
        let mut config = ModeConfig {
            mode: modes::ModeTag {
                value: 9999,
                ansi: false,
            }
            .to_backing(),
            value: true,
        };
        assert_eq!(Result::InvalidValue, unsafe {
            set(
                t,
                Option_::Mode,
                &config as *const ModeConfig as *const c_void,
            )
        });
        assert_eq!(Result::InvalidValue, unsafe {
            get(
                t,
                TerminalData::Mode,
                &mut config as *mut ModeConfig as *mut c_void,
            )
        });
        unsafe { free(t) };
    });
}

// test "vt_write"
#[test]
fn vt_write_test() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        unsafe { vt_write(t, b"Hello".as_ptr(), 5) };

        assert_eq!(b"Hello", plain_string(t).as_slice());
        unsafe { free(t) };
    });
}

// test "vt_write_until_ground result contract"
#[test]
fn vt_write_until_ground_result_contract() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // Input remains untouched when the stream is already at ground.
        let mut consumed: usize = 99;
        let untouched = b"unprocessed";
        assert_eq!(Result::Success, unsafe {
            vt_write_until_ground(t, untouched.as_ptr(), untouched.len(), &mut consumed)
        });
        assert_eq!(0usize, consumed);

        // Complete a split CSI and stop before inspecting the printable suffix.
        vtw(t, b"\x1b[31");
        let input = b"mABC\x1b[";
        assert_eq!(Result::Success, unsafe {
            vt_write_until_ground(t, input.as_ptr(), input.len(), &mut consumed)
        });
        assert_eq!(1usize, consumed);
        assert!(unsafe { (*t).stream.ground() });

        assert_eq!(b"", plain_string(t).as_slice());

        // The untouched suffix can be processed after work at the boundary.
        unsafe { vt_write(t, input.as_ptr().add(consumed), input.len() - consumed) };
        assert_eq!(b"ABC", plain_string(t).as_slice());
        assert!(!unsafe { (*t).stream.ground() });

        // Exhausting input while unfinished is distinct from reaching ground on
        // the final byte, even though both consume the entire input.
        assert_eq!(Result::NoValue, unsafe {
            vt_write_until_ground(t, b"123".as_ptr(), 3, &mut consumed)
        });
        assert_eq!(3usize, consumed);
        assert!(!unsafe { (*t).stream.ground() });

        assert_eq!(Result::Success, unsafe {
            vt_write_until_ground(t, b"m".as_ptr(), 1, &mut consumed)
        });
        assert_eq!(1usize, consumed);
        assert!(unsafe { (*t).stream.ground() });
        unsafe { free(t) };
    });
}

// test "vt_write_until_ground handles UTF-8 and abort boundaries"
#[test]
fn vt_write_until_ground_handles_utf8_and_abort_boundaries() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut ground = false;
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::VtGround,
                &mut ground as *mut bool as *mut c_void,
            )
        });
        assert!(ground);

        vtw(t, &[0xF0]);
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::VtGround,
                &mut ground as *mut bool as *mut c_void,
            )
        });
        assert!(!ground);

        let utf8_suffix = [0x9Fu8, 0x98, 0x84];
        let mut consumed: usize = 99;
        assert_eq!(Result::Success, unsafe {
            vt_write_until_ground(t, utf8_suffix.as_ptr(), utf8_suffix.len(), &mut consumed)
        });
        assert_eq!(utf8_suffix.len(), consumed);
        assert!(unsafe { (*t).stream.ground() });

        // A malformed continuation resets the decoder and reaches ground after
        // processing the retry byte.
        vtw(t, &[0xE0, 0xA0]);
        assert_eq!(Result::Success, unsafe {
            vt_write_until_ground(t, b"A".as_ptr(), 1, &mut consumed)
        });
        assert_eq!(1usize, consumed);
        assert!(unsafe { (*t).stream.ground() });

        vtw(t, b"\x1b[123");
        assert_eq!(Result::Success, unsafe {
            vt_write_until_ground(t, [0x18u8].as_ptr(), 1, &mut consumed)
        });
        assert_eq!(1usize, consumed);
        assert!(unsafe { (*t).stream.ground() });
        unsafe { free(t) };
    });
}

// test "vt_write_until_ground invokes effects only for consumed input"
#[test]
fn vt_write_until_ground_invokes_effects_only_for_consumed_input() {
    static mut UG_BELL_COUNT: usize = 0;
    extern "C" fn bell(_t: Terminal, _ud: *mut c_void) {
        unsafe { UG_BELL_COUNT += 1 };
    }

    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        unsafe { UG_BELL_COUNT = 0 };
        assert_eq!(Result::Success, unsafe {
            set(t, Option_::Bell, bell as *const c_void)
        });

        vtw(t, b"\x1b[31");
        let input = b"m\x07";
        let mut consumed: usize = 99;
        assert_eq!(Result::Success, unsafe {
            vt_write_until_ground(t, input.as_ptr(), input.len(), &mut consumed)
        });
        assert_eq!(1usize, consumed);
        assert_eq!(0usize, unsafe { UG_BELL_COUNT });

        unsafe { vt_write(t, input.as_ptr().add(consumed), input.len() - consumed) };
        assert_eq!(1usize, unsafe { UG_BELL_COUNT });
        unsafe { free(t) };
    });
}

// test "vt_write_until_ground validates arguments"
#[test]
fn vt_write_until_ground_validates_arguments() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        let mut consumed: usize = 99;
        assert_eq!(Result::InvalidValue, unsafe {
            vt_write_until_ground(std::ptr::null_mut(), b"x".as_ptr(), 1, &mut consumed)
        });
        assert_eq!(0usize, consumed);

        consumed = 99;
        assert_eq!(Result::InvalidValue, unsafe {
            vt_write_until_ground(t, std::ptr::null(), 1, &mut consumed)
        });
        assert_eq!(0usize, consumed);

        assert_eq!(Result::InvalidValue, unsafe {
            vt_write_until_ground(t, b"".as_ptr(), 0, std::ptr::null_mut())
        });

        // NULL represents a valid empty slice. While unfinished it consumes zero
        // bytes and reports that no ground boundary was found.
        vtw(t, b"\x1b[");
        consumed = 99;
        assert_eq!(Result::NoValue, unsafe {
            vt_write_until_ground(t, std::ptr::null(), 0, &mut consumed)
        });
        assert_eq!(0usize, consumed);
        unsafe { free(t) };
    });
}

// test "vt_write split escape sequence"
#[test]
fn vt_write_split_escape_sequence() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // Write "Hello" in bold by splitting the CSI bold sequence across two writes.
        // ESC [ 1 m  = bold on, ESC [ 0 m = reset
        // Split ESC from the rest of the CSI sequence.
        unsafe { vt_write(t, b"Hello \x1b".as_ptr(), 7) };
        unsafe { vt_write(t, b"[1mBold\x1b[0m".as_ptr(), 10) };

        // If the escape sequence leaked, we'd see "[1mBold" as literal text.
        assert_eq!(b"Hello Bold", plain_string(t).as_slice());
        unsafe { free(t) };
    });
}

// test "vt_write split combining mark after base at right edge"
#[test]
fn vt_write_split_combining_mark_after_base_at_right_edge() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 2, 2);
        assert_eq!(Result::Success, new_result);

        // Put "å" in the final column, then send its combining low line in a
        // separate write so the mark arrives while the cursor has a pending wrap.
        unsafe { vt_write(t, "xå".as_bytes().as_ptr(), 3) };
        unsafe { vt_write(t, b"\xcc\xb2".as_ptr(), 2) };

        assert_eq!("xå̲".as_bytes(), plain_string(t).as_slice());
        unsafe { free(t) };
    });
}
