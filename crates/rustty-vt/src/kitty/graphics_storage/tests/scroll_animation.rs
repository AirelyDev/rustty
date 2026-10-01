//! PORTS: ghostty src/terminal/kitty/graphics_storage.zig — the
//! `storage: scroll margins ...` tests and the `storage: animation tick ...`
//! tests.

use super::*;

/// Test helper: the reference reads
/// `t.screens.active.pages.pointFromPin(.active, p.location.pin.*)` for a
/// placement fetched with `storage.placements.get(...)`.
fn external_placement_coord(t: &Terminal, image_id: u32, id: u32) -> Option<point::Coordinate> {
    let storage = unsafe { &(*t.screens.active).kitty_images };
    let p = storage
        .placements
        .get(&PlacementKey {
            image_id,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id,
            },
        })
        .unwrap();
    let pin = match p.location {
        Location::Pin(pin) => pin,
        Location::Virtual | Location::Relative(_) => unreachable!(),
    };
    // SAFETY: `pin` is a live tracked pin owned by the pagelist.
    unsafe {
        (*t.screens.active)
            .pages
            .point_from_pin(point::Tag::Active, *pin)
    }
    .map(|pt| pt.coord())
}

/// Test helper: fetch the external placement (copy) for assertions on its
/// source rectangle / rows / columns fields.
fn external_placement(t: &Terminal, image_id: u32, id: u32) -> Placement {
    let storage = unsafe { &(*t.screens.active).kitty_images };
    let p = storage
        .placements
        .get(&PlacementKey {
            image_id,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id,
            },
        })
        .unwrap();
    Placement {
        location: Location::Virtual, // location is asserted via the coord helper
        ..*p
    }
}

// test "storage: scroll margins placement inside region scrolls and clips at top"
#[test]
fn scroll_margins_placement_inside_region_scrolls_and_clips_at_top() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
    t.width_px = 100;
    t.height_px = 60;

    // 2-row image (10x10px cells) inside the region (rows 1-4).
    // const storage: *ImageStorage = &t.screens.active.kitty_images;
    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 2 }).unwrap();
    {
        let storage = unsafe { &mut (*t.screens.active).kitty_images };
        storage
            .add_image(
                alloc,
                t.screens.active,
                Image {
                    id: 1,
                    width: 10,
                    height: 20,
                    ..Image::default()
                },
            )
            .unwrap();
        storage
            .add_placement(
                alloc,
                t.screens.active,
                1,
                1,
                Placement::with_location(Location::Pin(pin)),
            )
            .unwrap();
    }
    t.set_top_and_bottom_margin(2, 5);
    t.set_cursor_pos(5, 1);

    // Entirely inside: moves up with the content.
    t.index().unwrap();
    {
        assert_eq!(
            point::Coordinate { x: 0, y: 1 },
            external_placement_coord(&t, 1, 1).unwrap()
        );
    }

    // Would extend above the top margin: anchored at the top margin
    // with the top cell row clipped off the source.
    t.index().unwrap();
    {
        assert_eq!(
            point::Coordinate { x: 0, y: 1 },
            external_placement_coord(&t, 1, 1).unwrap()
        );
        let p = external_placement(&t, 1, 1);
        assert_eq!(10u32, p.source_y);
        assert_eq!(10u32, p.source_height);
    }

    // Fully clipped: deleted.
    t.index().unwrap();
    {
        let storage = unsafe { &(*t.screens.active).kitty_images };
        assert_eq!(0usize, storage.placements.len());
        // The image itself is retained.
        assert_eq!(1usize, storage.images.len());
    }

    t.deinit(alloc);
}

// Parity S190-F2 (no reference test): when the restores append fails, the
// reference logs "OOM adjusting image placement for scroll" and leaves
// the placement to the generic pin tracking. It is neither restored nor
// deleted.
#[test]
fn scroll_margins_restores_append_failure_is_logged_and_skipped() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
    t.width_px = 100;
    t.height_px = 60;

    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 2 }).unwrap();
    {
        let storage = unsafe { &mut (*t.screens.active).kitty_images };
        storage
            .add_image(
                alloc,
                t.screens.active,
                Image {
                    id: 1,
                    width: 10,
                    height: 20,
                    ..Image::default()
                },
            )
            .unwrap();
        storage
            .add_placement(
                alloc,
                t.screens.active,
                1,
                1,
                Placement::with_location(Location::Pin(pin)),
            )
            .unwrap();
    }
    t.set_top_and_bottom_margin(2, 5);

    // The restores list grows through the screen's allocator (`s.alloc`),
    // swapped for `std.mem.Allocator.failing` for the begin call (parity
    // tail S7-4 retired the `SCROLL_MARGINS_TW` stand-in).
    let original_alloc = unsafe { (*t.screens.active).alloc };
    unsafe { (*t.screens.active).alloc = crate::alloc::failing() };
    let cap = crate::log_capture::start();
    let result = {
        let storage = unsafe { &mut (*t.screens.active).kitty_images };
        storage.scroll_margins_begin(&mut t, -1, ScrollOp::InPlace)
    };
    let messages = cap.messages();
    drop(cap);
    unsafe { (*t.screens.active).alloc = original_alloc };

    assert_eq!(0usize, result.restores.len());
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Warn
                && target == "kitty_gfx"
                && msg == "OOM adjusting image placement for scroll"),
        "expected the OOM diagnostic, got: {messages:?}"
    );
    {
        let storage = unsafe { &(*t.screens.active).kitty_images };
        assert_eq!(1usize, storage.placements.len());
    }
    result.end();

    t.deinit(alloc);
}

// test "storage: scroll margins placement straddling region does not move"
#[test]
fn scroll_margins_placement_straddling_region_does_not_move() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
    t.width_px = 100;
    t.height_px = 60;

    // 3-row image anchored at the top of a 2-row region: not entirely
    // within the region, so region scrolls must not move it. This
    // exercises the scrollback-creating path (region top at row 0).
    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
    {
        let storage = unsafe { &mut (*t.screens.active).kitty_images };
        storage
            .add_image(
                alloc,
                t.screens.active,
                Image {
                    id: 1,
                    width: 10,
                    height: 30,
                    ..Image::default()
                },
            )
            .unwrap();
        storage
            .add_placement(
                alloc,
                t.screens.active,
                1,
                1,
                Placement::with_location(Location::Pin(pin)),
            )
            .unwrap();
    }
    t.set_top_and_bottom_margin(1, 2);
    t.set_cursor_pos(2, 1);

    t.index().unwrap();
    assert_eq!(
        point::Coordinate { x: 0, y: 0 },
        external_placement_coord(&t, 1, 1).unwrap()
    );
    let p = external_placement(&t, 1, 1);
    assert_eq!(0u32, p.source_y);
    assert_eq!(0u32, p.source_height);

    t.deinit(alloc);
}

// test "storage: scroll margins placement below region does not move"
#[test]
fn scroll_margins_placement_below_region_does_not_move() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
    t.width_px = 100;
    t.height_px = 60;

    // A "status bar" image below the bottom margin must stay put while
    // the region above it scrolls (scrollback-creating path).
    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 5 }).unwrap();
    {
        let storage = unsafe { &mut (*t.screens.active).kitty_images };
        storage
            .add_image(
                alloc,
                t.screens.active,
                Image {
                    id: 1,
                    width: 10,
                    height: 10,
                    ..Image::default()
                },
            )
            .unwrap();
        storage
            .add_placement(
                alloc,
                t.screens.active,
                1,
                1,
                Placement::with_location(Location::Pin(pin)),
            )
            .unwrap();
    }
    t.set_top_and_bottom_margin(1, 4);
    t.set_cursor_pos(4, 1);

    t.index().unwrap();
    t.index().unwrap();
    assert_eq!(
        point::Coordinate { x: 0, y: 5 },
        external_placement_coord(&t, 1, 1).unwrap()
    );

    t.deinit(alloc);
}

// test "storage: scroll margins reverse index clips at bottom"
#[test]
fn scroll_margins_reverse_index_clips_at_bottom() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
    t.width_px = 100;
    t.height_px = 60;

    // 2-row image at the bottom of the region (rows 1-4).
    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 3 }).unwrap();
    {
        let storage = unsafe { &mut (*t.screens.active).kitty_images };
        storage
            .add_image(
                alloc,
                t.screens.active,
                Image {
                    id: 1,
                    width: 10,
                    height: 20,
                    ..Image::default()
                },
            )
            .unwrap();
        storage
            .add_placement(
                alloc,
                t.screens.active,
                1,
                1,
                Placement::with_location(Location::Pin(pin)),
            )
            .unwrap();
    }
    t.set_top_and_bottom_margin(2, 5);
    t.set_cursor_pos(2, 1);

    // Moves down, with the bottom cell row clipped off the source.
    t.reverse_index();
    {
        assert_eq!(
            point::Coordinate { x: 0, y: 4 },
            external_placement_coord(&t, 1, 1).unwrap()
        );
        let p = external_placement(&t, 1, 1);
        assert_eq!(0u32, p.source_y);
        assert_eq!(10u32, p.source_height);
    }

    // Fully clipped: deleted.
    t.reverse_index();
    assert_eq!(
        0usize,
        unsafe { &(*t.screens.active).kitty_images }
            .placements
            .len()
    );

    t.deinit(alloc);
}

// test "storage: scroll margins scaled placement clips proportionally"
#[test]
fn scroll_margins_scaled_placement_clips_proportionally() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
    t.width_px = 100;
    t.height_px = 60;

    // A 40px-tall image scaled into 2 rows, inside a 2-row region.
    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 1 }).unwrap();
    {
        let storage = unsafe { &mut (*t.screens.active).kitty_images };
        storage
            .add_image(
                alloc,
                t.screens.active,
                Image {
                    id: 1,
                    width: 10,
                    height: 40,
                    ..Image::default()
                },
            )
            .unwrap();
        storage
            .add_placement(
                alloc,
                t.screens.active,
                1,
                1,
                Placement {
                    columns: 1,
                    rows: 2,
                    ..Placement::with_location(Location::Pin(pin))
                },
            )
            .unwrap();
    }
    t.set_top_and_bottom_margin(2, 3);
    t.set_cursor_pos(3, 1);

    t.index().unwrap();
    assert_eq!(
        point::Coordinate { x: 0, y: 1 },
        external_placement_coord(&t, 1, 1).unwrap()
    );
    let p = external_placement(&t, 1, 1);
    assert_eq!(20u32, p.source_y);
    assert_eq!(20u32, p.source_height);
    assert_eq!(1u32, p.rows);
    assert_eq!(1u32, p.columns);

    t.deinit(alloc);
}

// test "storage: scroll margins left/right margins respect columns"
#[test]
fn scroll_margins_left_right_margins_respect_columns() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
    t.width_px = 100;
    t.height_px = 60;

    // One placement outside the left/right margin columns, one inside.
    let pin1 = track_pin(&mut t, point::Coordinate { x: 0, y: 2 }).unwrap();
    let pin2 = track_pin(&mut t, point::Coordinate { x: 2, y: 2 }).unwrap();
    {
        let storage = unsafe { &mut (*t.screens.active).kitty_images };
        storage
            .add_image(
                alloc,
                t.screens.active,
                Image {
                    id: 1,
                    width: 10,
                    height: 10,
                    ..Image::default()
                },
            )
            .unwrap();
        storage
            .add_placement(
                alloc,
                t.screens.active,
                1,
                1,
                Placement::with_location(Location::Pin(pin1)),
            )
            .unwrap();
        storage
            .add_placement(
                alloc,
                t.screens.active,
                1,
                2,
                Placement::with_location(Location::Pin(pin2)),
            )
            .unwrap();
    }

    t.modes
        .set(crate::modes::Mode::EnableLeftAndRightMargin, true);
    t.set_left_and_right_margin(2, 8);

    t.scroll_up(1).unwrap();

    // Outside the margin columns: stays.
    assert_eq!(
        point::Coordinate { x: 0, y: 2 },
        external_placement_coord(&t, 1, 1).unwrap()
    );

    // Inside: moves up.
    assert_eq!(
        point::Coordinate { x: 2, y: 1 },
        external_placement_coord(&t, 1, 2).unwrap()
    );

    t.deinit(alloc);
}

// test "storage: scroll margins insert/delete lines do not move placements"
#[test]
fn scroll_margins_insert_delete_lines_do_not_move_placements() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
    t.width_px = 100;
    t.height_px = 60;

    // Kitty does not scroll images for IL/DL, only for IND/RI/SU/SD.
    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 2 }).unwrap();
    {
        let storage = unsafe { &mut (*t.screens.active).kitty_images };
        storage
            .add_image(
                alloc,
                t.screens.active,
                Image {
                    id: 1,
                    width: 10,
                    height: 10,
                    ..Image::default()
                },
            )
            .unwrap();
        storage
            .add_placement(
                alloc,
                t.screens.active,
                1,
                1,
                Placement::with_location(Location::Pin(pin)),
            )
            .unwrap();
    }
    t.set_top_and_bottom_margin(2, 5);
    t.set_cursor_pos(2, 1);

    t.delete_lines(1);
    assert_eq!(
        point::Coordinate { x: 0, y: 2 },
        external_placement_coord(&t, 1, 1).unwrap()
    );

    t.insert_lines(1);
    assert_eq!(
        point::Coordinate { x: 0, y: 2 },
        external_placement_coord(&t, 1, 1).unwrap()
    );

    t.deinit(alloc);
}

// test "storage: scroll without margins moves placement into scrollback"
#[test]
fn scroll_without_margins_moves_placement_into_scrollback() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
    t.width_px = 100;
    t.height_px = 60;

    // No margins: the placement follows its anchored row into the
    // scrollback via pin tracking (kitty's marginless behavior).
    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
    {
        let storage = unsafe { &mut (*t.screens.active).kitty_images };
        storage
            .add_image(
                alloc,
                t.screens.active,
                Image {
                    id: 1,
                    width: 10,
                    height: 20,
                    ..Image::default()
                },
            )
            .unwrap();
        storage
            .add_placement(
                alloc,
                t.screens.active,
                1,
                1,
                Placement::with_location(Location::Pin(pin)),
            )
            .unwrap();
    }
    t.set_cursor_pos(6, 1);

    t.index().unwrap();
    // try testing.expect(t.screens.active.pages.pointFromPin(.active, ...) == null);
    assert!(external_placement_coord(&t, 1, 1).is_none());
    // const pt = t.screens.active.pages.pointFromPin(.screen, p.location.pin.*).?;
    {
        let storage = unsafe { &(*t.screens.active).kitty_images };
        let p = storage
            .placements
            .get(&PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 1,
                },
            })
            .unwrap();
        let pin = match p.location {
            Location::Pin(pin) => pin,
            Location::Virtual | Location::Relative(_) => unreachable!(),
        };
        // SAFETY: `pin` is a live tracked pin owned by the pagelist.
        let pt = unsafe {
            (*t.screens.active)
                .pages
                .point_from_pin(point::Tag::Screen, *pin)
        }
        .unwrap();
        assert_eq!(point::Coordinate { x: 0, y: 0 }, pt.coord());
    }

    t.deinit(alloc);
}

// test "storage: scroll margins large scroll deletes inside placement"
#[test]
fn scroll_margins_large_scroll_deletes_inside_placement() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
    t.width_px = 100;
    t.height_px = 60;

    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 2 }).unwrap();
    {
        let storage = unsafe { &mut (*t.screens.active).kitty_images };
        storage
            .add_image(
                alloc,
                t.screens.active,
                Image {
                    id: 1,
                    width: 10,
                    height: 10,
                    ..Image::default()
                },
            )
            .unwrap();
        storage
            .add_placement(
                alloc,
                t.screens.active,
                1,
                1,
                Placement::with_location(Location::Pin(pin)),
            )
            .unwrap();
    }
    t.set_top_and_bottom_margin(2, 5);

    t.scroll_up(10).unwrap();
    assert_eq!(
        0usize,
        unsafe { &(*t.screens.active).kitty_images }
            .placements
            .len()
    );

    t.deinit(alloc);
}

// test "storage: scroll margins straddling placement pin inside region restored"
#[test]
fn scroll_margins_straddling_placement_pin_inside_region_restored() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
    t.width_px = 100;
    t.height_px = 60;

    // 2-row image anchored on the bottom margin row, extending below
    // the region: not entirely inside, so it must not move even though
    // the region scroll moves every tracked pin within the region rows.
    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 4 }).unwrap();
    {
        let storage = unsafe { &mut (*t.screens.active).kitty_images };
        storage
            .add_image(
                alloc,
                t.screens.active,
                Image {
                    id: 1,
                    width: 10,
                    height: 20,
                    ..Image::default()
                },
            )
            .unwrap();
        storage
            .add_placement(
                alloc,
                t.screens.active,
                1,
                1,
                Placement::with_location(Location::Pin(pin)),
            )
            .unwrap();
    }
    t.set_top_and_bottom_margin(2, 5);
    t.set_cursor_pos(5, 1);

    t.index().unwrap();
    assert_eq!(
        point::Coordinate { x: 0, y: 4 },
        external_placement_coord(&t, 1, 1).unwrap()
    );
    let p = external_placement(&t, 1, 1);
    assert_eq!(0u32, p.source_y);
    assert_eq!(0u32, p.source_height);

    t.deinit(alloc);
}

// test "storage: scroll margins multi-line scroll up with scrollback"
#[test]
fn scroll_margins_multi_line_scroll_up_with_scrollback() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
    t.width_px = 100;
    t.height_px = 60;

    // 1-row image inside a region whose top is the screen top, so the
    // scroll pushes text into the scrollback (window shift) while the
    // image moves within the region.
    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 2 }).unwrap();
    {
        let storage = unsafe { &mut (*t.screens.active).kitty_images };
        storage
            .add_image(
                alloc,
                t.screens.active,
                Image {
                    id: 1,
                    width: 10,
                    height: 10,
                    ..Image::default()
                },
            )
            .unwrap();
        storage
            .add_placement(
                alloc,
                t.screens.active,
                1,
                1,
                Placement::with_location(Location::Pin(pin)),
            )
            .unwrap();
    }
    t.set_top_and_bottom_margin(1, 4);

    t.scroll_up(2).unwrap();
    {
        assert_eq!(
            point::Coordinate { x: 0, y: 0 },
            external_placement_coord(&t, 1, 1).unwrap()
        );
    }

    // Another two rows scrolls it out of the region: deleted.
    t.scroll_up(2).unwrap();
    assert_eq!(
        0usize,
        unsafe { &(*t.screens.active).kitty_images }
            .placements
            .len()
    );

    t.deinit(alloc);
}

// test "storage: animation tick advances and schedules"
#[test]
fn animation_tick_advances_and_schedules() {
    use crate::kitty::graphics_animation::{Animation, Frame, State};

    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 10)).unwrap();
    t.width_px = 100;
    t.height_px = 100;

    let mut s: ImageStorage = ImageStorage::default();

    // A running 1x1 RGBA image whose animation has one extra frame.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 1,
            height: 1,
            format: command::Format::Rgba,
            data: Data::Complete(crate::alloc::heap_bytes(&[255, 0, 0, 255])),
            ..Image::default()
        },
    )
    .unwrap();
    // const img = s.images.getPtr(1).?;
    // const anim = try alloc.create(animation.Animation);
    // anim.* = .{ .state = .running };
    // img.animation = anim;
    // try anim.frames.append(alloc, .{ .data = ..., .gap_ms = 40 });
    {
        let img = s.images.get_mut(&1).unwrap();
        img.animation = Some(crate::alloc::heap_box(Animation {
            state: State::Running,
            ..Animation::default()
        }));
        img.animation.as_mut().unwrap().frames.push(Frame {
            data: crate::alloc::heap_bytes(&[0, 0, 255, 255]),
            gap_ms: 40,
        });
    }

    // Without a placement the animation doesn't advance (our
    // approximation of Kitty's visibility gate).
    assert!(s.animation_tick(0).is_none());
    assert_eq!(
        0u32,
        s.images
            .get(&1)
            .unwrap()
            .animation
            .as_ref()
            .unwrap()
            .current_index
    );

    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        0,
        Placement::with_location(Location::Pin(pin)),
    )
    .unwrap();
    let gen1 = s.images.get(&1).unwrap().generation;
    s.dirty = false;

    // First tick: the gapless root frame is due immediately and is
    // skipped over to frame 2, which is due again in its 40ms gap.
    assert_eq!(Some(40u64), s.animation_tick(0));
    assert_eq!(
        1u32,
        s.images
            .get(&1)
            .unwrap()
            .animation
            .as_ref()
            .unwrap()
            .current_index
    );
    assert!(s.images.get(&1).unwrap().generation > gen1);
    assert!(s.dirty);

    // Nothing due yet: no advance, and the delay counts down.
    let gen2 = s.images.get(&1).unwrap().generation;
    assert_eq!(Some(30u64), s.animation_tick(10));
    assert_eq!(gen2, s.images.get(&1).unwrap().generation);

    // Wrapping is fine with an infinite loop budget: the gapless
    // root is skipped and frame 2 is shown again.
    assert_eq!(Some(40u64), s.animation_tick(40));
    {
        let anim = s.images.get(&1).unwrap().animation.as_ref().unwrap();
        assert_eq!(1u32, anim.current_index);
        assert_eq!(1u32, anim.current_loop);
    }

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: animation tick loading state parks on last frame"
#[test]
fn animation_tick_loading_state_parks_on_last_frame() {
    use crate::kitty::graphics_animation::{Animation, Frame, State};

    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 10)).unwrap();
    t.width_px = 100;
    t.height_px = 100;

    let mut s: ImageStorage = ImageStorage::default();

    // A placed 1x1 RGBA image in the loading state (a=a s=2) whose
    // animation has one extra frame.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 1,
            height: 1,
            format: command::Format::Rgba,
            data: Data::Complete(crate::alloc::heap_bytes(&[255, 0, 0, 255])),
            ..Image::default()
        },
    )
    .unwrap();
    {
        let img = s.images.get_mut(&1).unwrap();
        img.animation = Some(crate::alloc::heap_box(Animation {
            state: State::Loading,
            ..Animation::default()
        }));
        img.animation.as_mut().unwrap().frames.push(Frame {
            data: crate::alloc::heap_bytes(&[0, 0, 255, 255]),
            gap_ms: 40,
        });
    }
    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        0,
        Placement::with_location(Location::Pin(pin)),
    )
    .unwrap();

    // Reach the last frame, then park: no wakeup is scheduled while
    // waiting for more frames and the loop counter stays untouched.
    assert_eq!(Some(40u64), s.animation_tick(0));
    assert_eq!(
        1u32,
        s.images
            .get(&1)
            .unwrap()
            .animation
            .as_ref()
            .unwrap()
            .current_index
    );
    assert!(s.animation_tick(100).is_none());
    {
        let anim = s.images.get(&1).unwrap().animation.as_ref().unwrap();
        assert_eq!(1u32, anim.current_index);
        assert_eq!(0u32, anim.current_loop);
    }

    // A new frame arriving un-parks playback.
    s.images
        .get_mut(&1)
        .unwrap()
        .animation
        .as_mut()
        .unwrap()
        .frames
        .push(Frame {
            data: crate::alloc::heap_bytes(&[0, 255, 0, 255]),
            gap_ms: 25,
        });
    assert_eq!(Some(25u64), s.animation_tick(150));
    assert_eq!(
        2u32,
        s.images
            .get(&1)
            .unwrap()
            .animation
            .as_ref()
            .unwrap()
            .current_index
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: animation tick exhausts loop budget"
#[test]
fn animation_tick_exhausts_loop_budget() {
    use crate::kitty::graphics_animation::{Animation, Frame, State};

    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 10)).unwrap();
    t.width_px = 100;
    t.height_px = 100;

    let mut s: ImageStorage = ImageStorage::default();

    // A placed, running 1x1 RGBA image with a gapped root frame, one
    // extra frame, and a one-loop budget (a=a v=2).
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 1,
            height: 1,
            format: command::Format::Rgba,
            data: Data::Complete(crate::alloc::heap_bytes(&[255, 0, 0, 255])),
            ..Image::default()
        },
    )
    .unwrap();
    {
        let img = s.images.get_mut(&1).unwrap();
        img.animation = Some(crate::alloc::heap_box(Animation {
            state: State::Running,
            root_gap_ms: 10,
            max_loops: 1,
            ..Animation::default()
        }));
        img.animation.as_mut().unwrap().frames.push(Frame {
            data: crate::alloc::heap_bytes(&[0, 0, 255, 255]),
            gap_ms: 40,
        });
    }
    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        0,
        Placement::with_location(Location::Pin(pin)),
    )
    .unwrap();

    // Root shows for 10ms, frame 2 for 40ms, then the wrap exhausts
    // the budget and playback freezes on the last frame for good.
    assert_eq!(Some(10u64), s.animation_tick(0));
    assert_eq!(
        0u32,
        s.images
            .get(&1)
            .unwrap()
            .animation
            .as_ref()
            .unwrap()
            .current_index
    );
    assert_eq!(Some(40u64), s.animation_tick(10));
    assert_eq!(
        1u32,
        s.images
            .get(&1)
            .unwrap()
            .animation
            .as_ref()
            .unwrap()
            .current_index
    );
    assert!(s.animation_tick(50).is_none());
    assert_eq!(
        1u32,
        s.images
            .get(&1)
            .unwrap()
            .animation
            .as_ref()
            .unwrap()
            .current_index
    );
    assert!(s.animation_tick(500).is_none());

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: animation tick ignores ineligible animations"
#[test]
fn animation_tick_ignores_ineligible_animations() {
    use crate::kitty::graphics_animation::{Animation, Frame, State};

    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 10)).unwrap();
    t.width_px = 100;
    t.height_px = 100;

    let mut s: ImageStorage = ImageStorage::default();

    // A placed 1x1 RGBA image with one extra frame, in the default
    // stopped state.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 1,
            height: 1,
            format: command::Format::Rgba,
            data: Data::Complete(crate::alloc::heap_bytes(&[255, 0, 0, 255])),
            ..Image::default()
        },
    )
    .unwrap();
    {
        let img = s.images.get_mut(&1).unwrap();
        img.animation = Some(crate::alloc::heap_box(Animation::default()));
        img.animation.as_mut().unwrap().frames.push(Frame {
            data: crate::alloc::heap_bytes(&[0, 0, 255, 255]),
            gap_ms: 40,
        });
    }
    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        0,
        Placement::with_location(Location::Pin(pin)),
    )
    .unwrap();

    // Stopped (the default) never advances.
    assert!(s.animation_tick(0).is_none());

    // An all-gapless animation can never advance either.
    {
        let anim = s.images.get_mut(&1).unwrap().animation.as_mut().unwrap();
        anim.state = State::Running;
        anim.frames[0].gap_ms = 0;
    }
    assert!(s.animation_tick(0).is_none());
    assert_eq!(
        0u32,
        s.images
            .get(&1)
            .unwrap()
            .animation
            .as_ref()
            .unwrap()
            .current_index
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: animation tick re-anchors a restarted clock"
#[test]
fn animation_tick_re_anchors_a_restarted_clock() {
    use crate::kitty::graphics_animation::{Animation, Frame, State};

    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(10, 10)).unwrap();
    t.width_px = 100;
    t.height_px = 100;

    let mut s: ImageStorage = ImageStorage::default();

    // A placed, running 1x1 RGBA image displaying its extra frame,
    // with a shown-at timestamp far ahead of the tick clock.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 1,
            height: 1,
            format: command::Format::Rgba,
            data: Data::Complete(crate::alloc::heap_bytes(&[255, 0, 0, 255])),
            ..Image::default()
        },
    )
    .unwrap();
    {
        let img = s.images.get_mut(&1).unwrap();
        img.animation = Some(crate::alloc::heap_box(Animation {
            state: State::Running,
            current_index: 1,
            frame_shown_at_ms: Some(1000),
            ..Animation::default()
        }));
        img.animation.as_mut().unwrap().frames.push(Frame {
            data: crate::alloc::heap_bytes(&[0, 0, 255, 255]),
            gap_ms: 40,
        });
    }
    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        0,
        Placement::with_location(Location::Pin(pin)),
    )
    .unwrap();

    // A timestamp in the future relative to now means the caller's
    // clock restarted; the animation must not stall until the old
    // timestamp comes around again.
    assert_eq!(Some(40u64), s.animation_tick(5));
    assert_eq!(
        Some(5u64),
        s.images
            .get(&1)
            .unwrap()
            .animation
            .as_ref()
            .unwrap()
            .frame_shown_at_ms
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}
