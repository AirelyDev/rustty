//! PORTS: ghostty src/terminal/c/terminal.zig — tests: `free null`,
//! `scroll_viewport`, compression, `reset`, and `resize`.

use super::*;

// test "free null"
#[test]
fn free_null() {
    unsafe { free(std::ptr::null_mut()) };
}

// test "scroll_viewport"
#[test]
fn scroll_viewport_test() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 5, 2);
        assert_eq!(Result::Success, new_result);

        let mut viewport_active: bool = false;
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ViewportActive,
                &mut viewport_active as *mut bool as *mut c_void,
            )
        });
        assert!(viewport_active);

        // Write "hello" on the first line
        unsafe { vt_write(t, b"hello".as_ptr(), 5) };

        // Push "hello" into scrollback with 3 newlines (index = ESC D)
        unsafe { vt_write(t, b"\x1bD\x1bD\x1bD".as_ptr(), 6) };
        // Viewport should be empty now since hello scrolled off
        assert_eq!(b"", plain_string(t).as_slice());

        // Scroll to top: "hello" should be visible again
        unsafe {
            scroll_viewport(
                t,
                ScrollViewportC {
                    tag: ScrollViewportTag::Top as c_int,
                    value: ScrollViewportCValue { _padding: [0; 2] },
                },
            )
        };
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ViewportActive,
                &mut viewport_active as *mut bool as *mut c_void,
            )
        });
        assert!(!viewport_active);
        assert_eq!(b"hello", plain_string(t).as_slice());

        // Scroll to bottom: viewport should be empty again
        unsafe {
            scroll_viewport(
                t,
                ScrollViewportC {
                    tag: ScrollViewportTag::Bottom as c_int,
                    value: ScrollViewportCValue { _padding: [0; 2] },
                },
            )
        };
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ViewportActive,
                &mut viewport_active as *mut bool as *mut c_void,
            )
        });
        assert!(viewport_active);
        assert_eq!(b"", plain_string(t).as_slice());

        // Scroll up by delta to bring "hello" back into view
        unsafe {
            scroll_viewport(
                t,
                ScrollViewportC {
                    tag: ScrollViewportTag::Delta as c_int,
                    value: ScrollViewportCValue { delta: -3 },
                },
            )
        };
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ViewportActive,
                &mut viewport_active as *mut bool as *mut c_void,
            )
        });
        assert!(!viewport_active);
        assert_eq!(b"hello", plain_string(t).as_slice());

        unsafe { free(t) };
    });
}

// test "scroll_viewport null"
#[test]
fn scroll_viewport_null() {
    unsafe {
        scroll_viewport(
            std::ptr::null_mut(),
            ScrollViewportC {
                tag: ScrollViewportTag::Top as c_int,
                value: ScrollViewportCValue { _padding: [0; 2] },
            },
        )
    };
    // scroll_viewport(null, .{ .tag = .row, .value = .{ .row = 1 } });
    //
    // FIX (S180-F1): the second call was dropped. It is the one whose union
    // payload is READ, so it is the call that exercises the null guard for a
    // tag the implementation must decode.
    unsafe {
        scroll_viewport(
            std::ptr::null_mut(),
            ScrollViewportC {
                tag: ScrollViewportTag::Row as c_int,
                value: ScrollViewportCValue { row: 1 },
            },
        )
    };
}

// PROVING TEST (W35-B07; inverted by task R13-D, UD113): no reference
// analog. The reference switches directly on the typed tag, so its
// safety-checked builds panic "switch on corrupt value" on an undeclared
// tag while ReleaseFast takes an arbitrary arm. Under R13 the port takes
// the defined silent return in EVERY build, debug included (this test used
// to require a debug-build panic). `catch_unwind` (not `#[should_panic]`)
// so a regression still frees the terminal and fails on the assertion.
#[test]
fn scroll_viewport_invalid_tag() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 5, 2);
        assert_eq!(Result::Success, new_result);

        // A valid call must still work: "hello" into scrollback, then Top
        // brings it back into view.
        unsafe { vt_write(t, b"hello".as_ptr(), 5) };
        unsafe { vt_write(t, b"\x1bD\x1bD\x1bD".as_ptr(), 6) };
        assert_eq!(b"", plain_string(t).as_slice());
        unsafe {
            scroll_viewport(
                t,
                ScrollViewportC {
                    tag: ScrollViewportTag::Top as c_int,
                    value: ScrollViewportCValue { _padding: [0; 2] },
                },
            )
        };
        assert_eq!(b"hello", plain_string(t).as_slice());

        // Undeclared tags, including negative ones and the first value past
        // `row`: every build returns silently with the viewport untouched.
        for tag in [c_int::MAX, -1, ScrollViewportTag::Row as c_int + 1] {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                scroll_viewport(
                    t,
                    ScrollViewportC {
                        tag,
                        value: ScrollViewportCValue { _padding: [0; 2] },
                    },
                )
            }));
            assert!(
                result.is_ok(),
                "an undeclared tag ({tag}) from the C caller must not panic (R13, UD113)"
            );
            // The viewport is untouched: "hello" still visible.
            assert_eq!(b"hello", plain_string(t).as_slice());
        }

        // The positive control after the invalid calls: a declared tag
        // still acts (Bottom scrolls "hello" back out of view).
        unsafe {
            scroll_viewport(
                t,
                ScrollViewportC {
                    tag: ScrollViewportTag::Bottom as c_int,
                    value: ScrollViewportCValue { _padding: [0; 2] },
                },
            )
        };
        assert_eq!(b"", plain_string(t).as_slice());

        unsafe { free(t) };
    });
}

// test "scroll_viewport row" (oracle 3a2e28329)
#[test]
fn scroll_viewport_row_test() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 5, 2);
        assert_eq!(Result::Success, new_result);

        // Write 4 rows so rows "1" and "2" are pushed into scrollback:
        // total rows is 4, viewport length is 2.
        unsafe { vt_write(t, b"1\r\n2\r\n3\r\n4".as_ptr(), 10) };

        let mut viewport_active: bool = false;
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ViewportActive,
                &mut viewport_active as *mut bool as *mut c_void,
            )
        });
        assert!(viewport_active);

        // Row 0 is the top of the scrollback.
        unsafe {
            scroll_viewport(
                t,
                ScrollViewportC {
                    tag: ScrollViewportTag::Row as c_int,
                    value: ScrollViewportCValue { row: 0 },
                },
            )
        };
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ViewportActive,
                &mut viewport_active as *mut bool as *mut c_void,
            )
        });
        assert!(!viewport_active);
        assert_eq!(b"1\n2", plain_string(t).as_slice());

        // An absolute row within the scrollback becomes the first visible
        // row and round-trips through the scrollbar offset.
        unsafe {
            scroll_viewport(
                t,
                ScrollViewportC {
                    tag: ScrollViewportTag::Row as c_int,
                    value: ScrollViewportCValue { row: 1 },
                },
            )
        };
        assert_eq!(b"2\n3", plain_string(t).as_slice());

        let mut scrollbar_data: TerminalScrollbar = TerminalScrollbar {
            total: 0,
            offset: 0,
            len: 0,
        };
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::Scrollbar,
                &mut scrollbar_data as *mut TerminalScrollbar as *mut c_void,
            )
        });
        assert_eq!(4, scrollbar_data.total);
        assert_eq!(1, scrollbar_data.offset);
        assert_eq!(2, scrollbar_data.len);

        // A row past the end clamps to the active area.
        unsafe {
            scroll_viewport(
                t,
                ScrollViewportC {
                    tag: ScrollViewportTag::Row as c_int,
                    value: ScrollViewportCValue { row: 9999 },
                },
            )
        };
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ViewportActive,
                &mut viewport_active as *mut bool as *mut c_void,
            )
        });
        assert!(viewport_active);
        assert_eq!(b"3\n4", plain_string(t).as_slice());
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::Scrollbar,
                &mut scrollbar_data as *mut TerminalScrollbar as *mut c_void,
            )
        });
        assert_eq!(2, scrollbar_data.offset);

        unsafe { free(t) };
    });
}

// test "scroll_viewport row alt screen" (oracle 3a2e28329)
#[test]
fn scroll_viewport_row_alt_screen_test() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 5, 2);
        assert_eq!(Result::Success, new_result);

        // Enter the alternate screen, which has no scrollback.
        unsafe { vt_write(t, b"\x1b[?1049h".as_ptr(), 8) };
        let mut screen: TerminalScreen = TerminalScreen::Primary;
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ActiveScreen,
                &mut screen as *mut TerminalScreen as *mut c_void,
            )
        });
        assert_eq!(TerminalScreen::Alternate, screen);

        // Scrolling to any row keeps the viewport on the active area.
        let mut viewport_active: bool = false;
        unsafe {
            scroll_viewport(
                t,
                ScrollViewportC {
                    tag: ScrollViewportTag::Row as c_int,
                    value: ScrollViewportCValue { row: 0 },
                },
            )
        };
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ViewportActive,
                &mut viewport_active as *mut bool as *mut c_void,
            )
        });
        assert!(viewport_active);
        unsafe {
            scroll_viewport(
                t,
                ScrollViewportC {
                    tag: ScrollViewportTag::Row as c_int,
                    value: ScrollViewportCValue { row: 9999 },
                },
            )
        };
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::ViewportActive,
                &mut viewport_active as *mut bool as *mut c_void,
            )
        });
        assert!(viewport_active);

        // With no scrollback the scrollbar covers exactly the active area.
        let mut scrollbar_data: TerminalScrollbar = TerminalScrollbar {
            total: 0,
            offset: 0,
            len: 0,
        };
        assert_eq!(Result::Success, unsafe {
            get(
                t,
                TerminalData::Scrollbar,
                &mut scrollbar_data as *mut TerminalScrollbar as *mut c_void,
            )
        });
        assert_eq!(2, scrollbar_data.total);
        assert_eq!(0, scrollbar_data.offset);
        assert_eq!(2, scrollbar_data.len);

        unsafe { free(t) };
    });
}

// test "compression invalid arguments" (oracle 172f15da3)
#[test]
fn compression_invalid_arguments() {
    with_test_allocator(|alloc| {
        // var activity: u64 = undefined;
        // var compression_result: CompressionResult = undefined;
        let mut activity: u64 = 0;
        let mut compression_result = CompressionResult::Unsupported;

        // try testing.expectEqual(Result.invalid_value, compression_activity(null, &activity));
        assert_eq!(Result::InvalidValue, unsafe {
            compression_activity(std::ptr::null_mut(), &mut activity)
        });
        // try testing.expectEqual(Result.invalid_value, compress(null, @intFromEnum(.incremental), &compression_result));
        assert_eq!(Result::InvalidValue, unsafe {
            compress(
                std::ptr::null_mut(),
                CompressionMode::Incremental as c_int,
                &mut compression_result,
            )
        });

        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // try testing.expectEqual(Result.invalid_value, compression_activity(t, null));
        assert_eq!(Result::InvalidValue, unsafe {
            compression_activity(t, std::ptr::null_mut())
        });
        // try testing.expectEqual(Result.invalid_value, compress(t, @intFromEnum(.incremental), null));
        assert_eq!(Result::InvalidValue, unsafe {
            compress(
                t,
                CompressionMode::Incremental as c_int,
                std::ptr::null_mut(),
            )
        });
        // try testing.expectEqual(Result.invalid_value, compress(t, -1, &compression_result));
        assert_eq!(Result::InvalidValue, unsafe {
            compress(t, -1, &mut compression_result)
        });

        // defer free(t);
        unsafe { free(t) };
    });
}

// test "compression activity and incremental scheduling" (oracle 172f15da3)
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-vt-c, parity tail C-1): intractable under Miri, 4,000 lines of history written through the terminal, then an incremental compression pass; measured past a 20-minute bound; runs natively and under ASan (see MIRI_EXCEPTIONS.md)"
)]
fn compression_activity_and_incremental_scheduling() {
    with_test_allocator(|alloc| {
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // var initial_activity: u64 = undefined;
        // try testing.expectEqual(Result.success, compression_activity(t, &initial_activity));
        let mut initial_activity: u64 = 0;
        assert_eq!(Result::Success, unsafe {
            compression_activity(t, &mut initial_activity)
        });

        // const line = "repeated and compressible terminal history\r\n";
        // const repeat = 4_000;
        // const input = try testing.allocator.alloc(u8, line.len * repeat);
        // for (0..repeat) |i| @memcpy(input[i * line.len ..][0..line.len], line);
        // vt_write(t, input.ptr, input.len);
        let line: &[u8] = b"repeated and compressible terminal history\r\n";
        const REPEAT: usize = 4_000;
        let mut input: Vec<u8> = Vec::with_capacity(line.len() * REPEAT);
        for _ in 0..REPEAT {
            input.extend_from_slice(line);
        }
        unsafe { vt_write(t, input.as_ptr(), input.len()) };

        // var activity: u64 = undefined;
        // try testing.expectEqual(Result.success, compression_activity(t, &activity));
        // try testing.expect(activity != initial_activity);
        let mut activity: u64 = 0;
        assert_eq!(Result::Success, unsafe {
            compression_activity(t, &mut activity)
        });
        assert_ne!(activity, initial_activity);

        // var compression_result: CompressionResult = undefined;
        // for (0..1_000) |_| { ... } else return error.TestUnexpectedResult;
        let mut compression_result = CompressionResult::Unsupported;
        let mut completed = false;
        for _ in 0..1_000 {
            assert_eq!(Result::Success, unsafe {
                compress(
                    t,
                    CompressionMode::Incremental as c_int,
                    &mut compression_result,
                )
            });

            match compression_result {
                // .pending => continue,
                CompressionResult::Pending => continue,
                // .complete => break,
                CompressionResult::Complete => {
                    completed = true;
                    break;
                }
                // .unsupported => unreachable,
                CompressionResult::Unsupported => {
                    unreachable!("test builds always support strict reclamation")
                }
            }
        }
        // The reference's `else` clause on the `for` loop: falling out
        // without a `break` is `error.TestUnexpectedResult`.
        assert!(completed);

        // Compression changes storage representation, not the activity token.
        // var final_activity: u64 = undefined;
        // try testing.expectEqual(Result.success, compression_activity(t, &final_activity));
        // try testing.expectEqual(activity, final_activity);
        let mut final_activity: u64 = 0;
        assert_eq!(Result::Success, unsafe {
            compression_activity(t, &mut final_activity)
        });
        assert_eq!(activity, final_activity);

        // try testing.expectEqual(Result.success, compress(t, @intFromEnum(.full), &compression_result));
        // try testing.expectEqual(CompressionResult.complete, compression_result);
        assert_eq!(Result::Success, unsafe {
            compress(t, CompressionMode::Full as c_int, &mut compression_result)
        });
        assert_eq!(CompressionResult::Complete, compression_result);

        // defer free(t);
        unsafe { free(t) };
    });
}

// test "reset"
#[test]
fn reset_test() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        unsafe { vt_write(t, b"Hello".as_ptr(), 5) };
        unsafe { reset(t) };

        assert_eq!(b"", plain_string(t).as_slice());
        unsafe { free(t) };
    });
}

// test "reset null"
#[test]
fn reset_null() {
    unsafe { reset(std::ptr::null_mut()) };
}

// test "resize"
#[test]
fn resize_test() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        assert_eq!(Result::Success, unsafe { resize(t, 40, 12, 9, 18) });
        assert_eq!(40, unsafe { (*(*t).terminal).cols });
        assert_eq!(12, unsafe { (*(*t).terminal).rows });
        unsafe { free(t) };
    });
}

// test "resize null"
#[test]
fn resize_null() {
    assert_eq!(Result::InvalidValue, unsafe {
        resize(std::ptr::null_mut(), 80, 24, 9, 18)
    });
}

// test "resize invalid value"
#[test]
fn resize_invalid_value() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        assert_eq!(Result::InvalidValue, unsafe { resize(t, 0, 24, 9, 18) });
        assert_eq!(Result::InvalidValue, unsafe { resize(t, 80, 0, 9, 18) });
        unsafe { free(t) };
    });
}

// test "resize shrinks both axes with cursor at bottom"
#[test]
fn resize_shrinks_both_axes_with_cursor_at_bottom() {
    with_test_allocator(|alloc| {
        // try testing.expectEqual(Result.success, new(&test_allocator, &t, .{ ... }));
        let (new_result, t) = new_t(alloc, 80, 24);
        assert_eq!(Result::Success, new_result);

        // CSI 24;1H -> park the cursor on the bottom row (1-based).
        let mv = b"\x1b[24;1H";
        unsafe { vt_write(t, mv.as_ptr(), mv.len()) };

        // Shrink both axes; pre-resize cursor.y sits past the new bottom row.
        // Previously this underflowed in PageList.resizeCols.
        assert_eq!(Result::Success, unsafe { resize(t, 79, 23, 8, 16) });
        assert_eq!(79, unsafe { (*(*t).terminal).cols });
        assert_eq!(23, unsafe { (*(*t).terminal).rows });
        unsafe { free(t) };
    });
}
