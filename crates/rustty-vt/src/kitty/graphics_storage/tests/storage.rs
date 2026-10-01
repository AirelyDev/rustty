//! PORTS: ghostty src/terminal/kitty/graphics_storage.zig — the
//! `storage: ...` generation, eviction, pending-image, uppercase-delete,
//! geometry, `nextImageId`, `resolveChain` and `placeholderTarget` tests.

use super::*;

// test "storage: generation stamps on image add and replace"
#[test]
fn storage_generation_stamps_on_image_add_and_replace() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();

    // Fresh storage has generation zero (never mutated).
    assert_eq!(0u64, s.generation);

    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 1,
            height: 1,
            ..Image::default()
        },
    )
    .unwrap();
    let gen1 = s.generation;
    assert!(gen1 > 0);

    let img1 = s.image_by_id(1).unwrap();
    assert_eq!(gen1, img1.generation);

    // A second image gets a strictly greater stamp.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2,
            width: 1,
            height: 1,
            ..Image::default()
        },
    )
    .unwrap();
    let gen2 = s.generation;
    assert!(gen2 > gen1);
    assert_eq!(gen2, s.image_by_id(2).unwrap().generation);

    // Retransmitting the same image ID (identical dimensions) gets a
    // fresh stamp: this is what makes same-sized retransmissions
    // detectable by renderers.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 1,
            height: 1,
            ..Image::default()
        },
    )
    .unwrap();
    let gen3 = s.generation;
    assert!(gen3 > gen2);
    assert_eq!(gen3, s.image_by_id(1).unwrap().generation);

    // Image 2 kept its stamp.
    assert_eq!(gen2, s.image_by_id(2).unwrap().generation);

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: generation bumps on placement and delete"
#[test]
fn storage_generation_bumps_on_placement_and_delete() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            ..Image::default()
        },
    )
    .unwrap();
    let gen_add = s.generation;

    let pin = track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Pin(pin)),
    )
    .unwrap();
    let gen_place = s.generation;
    assert!(gen_place > gen_add);

    // Reads don't change the generation.
    let _ = s.image_by_id(1);
    let _ = s.image_by_number(1);
    assert_eq!(gen_place, s.generation);

    s.delete(alloc, &mut t, command::DeleteAction::All(true));
    assert!(s.generation > gen_place);

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: generation bumps when setLimit evicts or disables"
#[test]
fn storage_generation_bumps_when_set_limit_evicts_or_disables() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();

    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 1,
            height: 1,
            data: Data::Complete(crate::alloc::heap_bytes(b"1234")),
            ..Image::default()
        },
    )
    .unwrap();
    let gen_add = s.generation;

    // Lowering the limit evicts the image and must mark a mutation.
    s.dirty = false;
    s.set_limit(alloc, t.screens.active, 1);
    assert!(s.dirty);
    assert!(s.generation > gen_add);
    assert_eq!(0usize, s.images.len());
    let gen_evict = s.generation;

    // Disabling (limit=0) resets the storage and must mark a mutation.
    s.dirty = false;
    s.set_limit(alloc, t.screens.active, 0);
    assert!(s.dirty);
    assert!(s.generation > gen_evict);

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: imageByNumber returns most recently transmitted"
#[test]
fn storage_image_by_number_returns_most_recently_transmitted() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();

    // Two images sharing a number: the newest transmission wins,
    // regardless of insertion order or clock resolution.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            number: 7,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2,
            number: 7,
            ..Image::default()
        },
    )
    .unwrap();
    assert_eq!(2u32, s.image_by_number(7).unwrap().id);

    // Retransmit the first: it becomes the newest.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            number: 7,
            ..Image::default()
        },
    )
    .unwrap();
    assert_eq!(1u32, s.image_by_number(7).unwrap().id);

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: nextGeneration is unique and monotonic"
#[test]
fn storage_next_generation_is_unique_and_monotonic() {
    let a = next_generation();
    let b = next_generation();
    assert!(b > a);
    assert!(a > 0);
}

// test "storage: no-op delete does not mark a mutation"
#[test]
fn storage_no_op_delete_does_not_mark_a_mutation() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();

    // A delete-all on an empty storage (this runs on every screen
    // clear) must not dirty the state or bump the generation.
    s.delete(alloc, &mut t, command::DeleteAction::All(true));
    assert!(!s.dirty);
    assert_eq!(0u64, s.generation);

    // Same for a delete that matches nothing.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            ..Image::default()
        },
    )
    .unwrap();
    let pin = track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Pin(pin)),
    )
    .unwrap();
    let gen = s.generation;
    s.dirty = false;
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Id {
            delete: false,
            image_id: 42,
            placement_id: 0,
        },
    );
    assert!(!s.dirty);
    assert_eq!(gen, s.generation);

    // But a delete that removes something does mark a mutation.
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Id {
            delete: false,
            image_id: 1,
            placement_id: 0,
        },
    );
    assert!(s.dirty);
    assert!(s.generation > gen);

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: evicts images in priority order"
// (supersedes a65e11cc9's "storage: evict unused transient image")
#[test]
fn storage_evicts_images_in_priority_order() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage {
        total_limit: 192,
        ..ImageStorage::default()
    };

    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            data: Data::Complete(crate::alloc::heap_bytes(&[b'*'; 64])),
            metadata: Metadata {
                transient: false,
                ..Metadata::default()
            },
            ..Image::default()
        },
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2,
            data: Data::Complete(crate::alloc::heap_bytes(&[b'*'; 64])),
            metadata: Metadata {
                transient: true,
                ..Metadata::default()
            },
            ..Image::default()
        },
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 3,
            data: Data::Complete(crate::alloc::heap_bytes(&[b'*'; 64])),
            metadata: Metadata {
                transient: true,
                ..Metadata::default()
            },
            ..Image::default()
        },
    )
    .unwrap();
    let pin = track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        2,
        1,
        Placement::with_location(Location::Pin(pin)),
    )
    .unwrap();

    let gen = s.generation;
    let result = s.evict_image(alloc, t.screens.active, 96);
    assert!(s.dirty);
    assert!(s.generation > gen);
    assert!(result);
    assert_eq!(1, s.images.len());
    assert!(!s.images.contains_key(&1));
    assert!(s.images.contains_key(&2));
    assert!(!s.images.contains_key(&3));

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

/// NEGATIVE CONTROL for `a65e11cc9`'s re-keyed eviction order — the test
/// UPSTREAM DOES NOT HAVE (`P3-W6 T5` Step 2 / area D §6).
///
/// Upstream's only new test evicts one image and can be satisfied without
/// the `used` term ever being consulted. This one pins the FULL block
/// ordering by evicting every image one at a time and recording the order:
///
///   block 0: transient + unused   (evicted first)
///   block 1: not transient + unused
///   block 2: transient + used
///   block 3: not transient + used (evicted last)
///
/// Without it, `block` could be computed and never consulted — or consulted
/// with the two terms swapped — and every other test would still pass.
#[test]
fn storage_eviction_order_is_transient_then_unused() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    // Four images, one per block. IDs are assigned so that ID order is the
    // REVERSE of the expected eviction order: if `block` were ignored the
    // tie-break (generation, then id) would produce 1,2,3,4 and the
    // assertion below would fail.
    //
    //   id 1: not transient, used   -> block 3
    //   id 2: transient,     used   -> block 2
    //   id 3: not transient, unused -> block 1
    //   id 4: transient,     unused -> block 0
    let mut s: ImageStorage = ImageStorage {
        total_limit: 4 * 64,
        ..ImageStorage::default()
    };
    for (id, transient) in [(1u32, false), (2, true), (3, false), (4, true)] {
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id,
                data: Data::Complete(crate::alloc::heap_bytes(&[b'*'; 64])),
                metadata: Metadata {
                    transient,
                    ..Metadata::default()
                },
                ..Image::default()
            },
        )
        .unwrap();
    }
    // Only 1 and 2 are "used" (have a placement).
    for (i, id) in [1u32, 2].iter().enumerate() {
        let pin = track_pin(&mut t, point::Coordinate { x: 1, y: i as u32 }).unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            *id,
            1,
            Placement::with_location(Location::Pin(pin)),
        )
        .unwrap();
    }

    // Evict one image at a time and record which one went.
    let mut order: Vec<u32> = Vec::new();
    for _ in 0..4 {
        let before: std::collections::BTreeSet<u32> = s.images.keys().copied().collect();
        assert!(s.evict_image(alloc, t.screens.active, 1));
        let after: std::collections::BTreeSet<u32> = s.images.keys().copied().collect();
        let gone: Vec<u32> = before.difference(&after).copied().collect();
        assert_eq!(
            1,
            gone.len(),
            "exactly one image should be evicted per call"
        );
        order.push(gone[0]);
    }

    assert_eq!(
        vec![4u32, 3, 2, 1],
        order,
        "eviction must go transient+unused, not-transient+unused, \
         transient+used, not-transient+used"
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: replacing placement releases tracked pin"
#[test]
fn storage_replacing_placement_releases_tracked_pin() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            ..Image::default()
        },
    )
    .unwrap();

    let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
        )),
    )
    .unwrap();
    assert_eq!(
        tracked + 1,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    let replacement_pin = track_pin(&mut t, point::Coordinate { x: 2, y: 2 }).unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Pin(replacement_pin)),
    )
    .unwrap();
    assert_eq!(1usize, s.placements.len());
    assert_eq!(
        tracked + 1,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );
    // try testing.expectEqual(replacement_pin, s.placements.get(...).?.location.pin);
    match s
        .placements
        .get(&PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 1,
            },
        })
        .unwrap()
        .location
    {
        Location::Pin(p) => assert_eq!(replacement_pin, p),
        Location::Virtual | Location::Relative(_) => panic!("expected pin location"),
    }

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: adding placement reclaims garbage placements"
#[test]
fn storage_adding_placement_reclaims_garbage_placements() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            ..Image::default()
        },
    )
    .unwrap();

    let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();
    let old_pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        0,
        Placement::with_location(Location::Pin(old_pin)),
    )
    .unwrap();
    // old_pin.garbage = true;
    // SAFETY: `old_pin` is a live tracked pin owned by the pagelist.
    unsafe { (*old_pin).garbage = true };

    let new_pin = track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        0,
        Placement::with_location(Location::Pin(new_pin)),
    )
    .unwrap();

    assert_eq!(1usize, s.placements.len());
    assert_eq!(
        tracked + 1,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );
    match s
        .placements
        .get(&PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::Internal,
                id: 1,
            },
        })
        .unwrap()
        .location
    {
        Location::Pin(p) => assert_eq!(new_pin, p),
        Location::Virtual | Location::Relative(_) => panic!("expected pin location"),
    }

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: placement count limit permits replacement"
#[test]
fn storage_placement_count_limit_permits_replacement() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Virtual),
    )
    .unwrap();

    // const img = s.images.getPtr(1).?;
    // img.metadata.placement_count = std.math.maxInt(@TypeOf(img.metadata.placement_count));
    let img = s.images.get_mut(&1).unwrap();
    img.metadata.placement_count = Metadata::PLACEMENT_COUNT_MAX;
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Virtual),
    )
    .unwrap();
    assert_eq!(
        Err(OutOfMemory),
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            2,
            Placement::with_location(Location::Virtual),
        )
    );
    assert_eq!(1usize, s.placements.len());

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete intersecting cursor checks interior row column"
#[test]
fn storage_delete_intersecting_cursor_checks_interior_row_column() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
    t.width_px = 100;
    t.height_px = 100;

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
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
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
        )),
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        2,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 20, y: 0 }).unwrap(),
        )),
    )
    .unwrap();

    // This is inside the right placement and on an interior row shared by
    // both placements, but it is outside the left placement's columns.
    unsafe { (*t.screens.active).cursor_absolute(21, 5) };
    s.delete(alloc, &mut t, command::DeleteAction::IntersectCursor(false));

    assert_eq!(1usize, s.placements.len());
    assert!(s
        .placements
        .get(&PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 1
            },
        })
        .is_some());

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete intersecting cell checks interior row column"
#[test]
fn storage_delete_intersecting_cell_checks_interior_row_column() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
    t.width_px = 100;
    t.height_px = 100;

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
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
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
        )),
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        2,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 20, y: 0 }).unwrap(),
        )),
    )
    .unwrap();

    // Protocol coordinates are one-based, so this targets grid cell (21, 5).
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::IntersectCell {
            delete: false,
            x: 22,
            y: 6,
        },
    );

    assert_eq!(1usize, s.placements.len());
    assert!(s
        .placements
        .get(&PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 1
            },
        })
        .is_some());

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: placement geometry handles untrusted dimensions"
#[test]
fn storage_placement_geometry_handles_untrusted_dimensions() {
    let alloc = crate::alloc::Global;
    let max = u32::MAX;

    let mut t = Terminal::init(alloc, opts(2, 2)).unwrap();
    t.width_px = max;
    t.height_px = max;

    // Test helper: the reference passes the image as an anonymous literal
    // `.{ .width = 1, .height = 1 }`.
    let image_1x1 = Image {
        width: 1,
        height: 1,
        ..Image::default()
    };

    // Cell dimensions multiplied by protocol-controlled row and column
    // counts saturate instead of panicking or wrapping.
    {
        let placement: Placement = Placement {
            columns: 3,
            rows: 3,
            ..Placement::with_location(Location::Virtual)
        };
        let actual = placement.pixel_size(&image_1x1, &t);
        assert_eq!(max, actual.width);
        assert_eq!(max, actual.height);
    }

    // Aspect-ratio scaling also saturates when the derived dimension does
    // not fit in the public u32 geometry type.
    {
        let placement: Placement = Placement {
            columns: 3,
            source_height: max,
            ..Placement::with_location(Location::Virtual)
        };
        let actual = placement.pixel_size(&image_1x1, &t);
        assert_eq!(max, actual.width);
        assert_eq!(max, actual.height);
    }
    {
        let placement: Placement = Placement {
            rows: 3,
            source_width: max,
            ..Placement::with_location(Location::Virtual)
        };
        let actual = placement.pixel_size(&image_1x1, &t);
        assert_eq!(max, actual.width);
        assert_eq!(max, actual.height);
    }

    // Pixel offsets are protocol-controlled too. Clamp them to the cell
    // before including them in grid geometry.
    t.width_px = 2;
    t.height_px = 2;
    {
        let placement: Placement = Placement {
            x_offset: max,
            y_offset: max,
            ..Placement::with_location(Location::Virtual)
        };
        let actual = placement.grid_size(&image_1x1, &t);
        assert_eq!(1u32, actual.cols);
        assert_eq!(1u32, actual.rows);
    }

    let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
    // defer t.screens.active.pages.untrackPin(pin); — explicit below.

    // Explicit maximum dimensions must clamp the rectangle to the terminal
    // without overflowing its horizontal extent.
    {
        let placement: Placement = Placement {
            columns: max,
            rows: 1,
            ..Placement::with_location(Location::Pin(pin))
        };
        let rect = placement.rect(&image_1x1, &t).unwrap();
        assert_eq!(1 as size::CellCountInt, rect.bottom_right.x);
    }

    // A garbage pin represents content that has been pruned from retained
    // history. Its fallback location must not make the placement visible.
    // SAFETY: `pin` is a live tracked pin owned by the pagelist.
    unsafe { (*pin).garbage = true };
    {
        let placement: Placement = Placement {
            columns: 1,
            rows: 1,
            ..Placement::with_location(Location::Pin(pin))
        };
        assert!(placement.rect(&image_1x1, &t).is_none());
    }
    unsafe { (*pin).garbage = false };

    // Terminals can temporarily have no pixel geometry. A placement whose
    // computed grid is empty has no rectangle, so rect must not subtract one
    // from a zero row count.
    t.width_px = 0;
    t.height_px = 0;
    {
        let placement: Placement = Placement {
            columns: 1,
            ..Placement::with_location(Location::Pin(pin))
        };
        let actual = placement.grid_size(&image_1x1, &t);
        assert_eq!(0u32, actual.cols);
        assert_eq!(0u32, actual.rows);
        assert!(placement.rect(&image_1x1, &t).is_none());
    }

    // (deferred) t.screens.active.pages.untrackPin(pin);
    unsafe { (*t.screens.active).pages.untrack_pin(pin) };
    t.deinit(alloc);
}

// test "storage: eviction releases placement pins"
#[test]
fn storage_eviction_releases_placement_pins() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage {
        total_limit: 128,
        ..ImageStorage::default()
    };

    let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            data: Data::Complete(crate::alloc::heap_bytes(&[b'*'; 64])),
            ..Image::default()
        },
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
        )),
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2,
            data: Data::Complete(crate::alloc::heap_bytes(&[b'*'; 64])),
            ..Image::default()
        },
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        2,
        1,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
        )),
    )
    .unwrap();
    assert_eq!(
        tracked + 2,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    // Adding a third image evicts the oldest equally-ranked image and its
    // placement. The newer image's placement and tracked pin remain intact.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 3,
            data: Data::Complete(crate::alloc::heap_bytes(&[b'*'; 64])),
            ..Image::default()
        },
    )
    .unwrap();
    assert!(!s.images.contains_key(&1));
    assert!(s.images.contains_key(&2));
    assert!(s.images.contains_key(&3));
    assert_eq!(1usize, s.placements.len());
    assert!(s.placements.contains_key(&PlacementKey {
        image_id: 2,
        placement_id: PlacementId {
            tag: PlacementIdTag::External,
            id: 1
        },
    }));
    assert_eq!(
        tracked + 1,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: pending image completes once and preserves age"
#[test]
fn storage_pending_image_completes_once_and_preserves_age() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage {
        total_limit: 16,
        ..ImageStorage::default()
    };

    let pending = s
        .add_pending_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                number: 7,
                width: 2,
                height: 2,
                format: command::Format::Rgba,
                data: Data::Pending(16),
                ..Image::default()
            },
        )
        .unwrap();
    assert_eq!(16usize, s.total_bytes);
    assert_eq!(1u32, s.image_by_number(7).unwrap().id);
    assert!(s.image_by_id(1).unwrap().data.is_pending());

    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
        )),
    )
    .unwrap();
    assert_eq!(1usize, s.placements.len());

    // const wrong = try alloc.dupe(u8, "short");
    // const wrong_completed = pending.complete(&s, io, wrong);
    // try testing.expect(!wrong_completed);
    // defer alloc.free(wrong); — the Vec drops inside `complete` on false.
    let wrong = b"short".to_vec();
    let wrong_completed = pending.complete(&mut s, crate::alloc::heap_bytes(&wrong));
    assert!(!wrong_completed);

    let storage_generation = s.generation;
    s.dirty = false;
    let pixels = vec![b'*'; 16];
    assert!(pending.complete(&mut s, crate::alloc::heap_bytes(&pixels.clone())));
    assert!(s.dirty);
    assert!(s.generation > storage_generation);
    assert_eq!(pending.generation, s.image_by_id(1).unwrap().generation);
    assert_eq!(16usize, s.total_bytes);
    // try testing.expectEqualSlices(u8, pixels, s.imageById(1).?.data.bytes().?);
    let stored = s.image_by_id(1).unwrap();
    assert_eq!(pixels.as_slice(), stored.data.bytes().unwrap());

    let duplicate = vec![b'!'; 16];
    let duplicate_completed = pending.complete(&mut s, crate::alloc::heap_bytes(&duplicate));
    assert!(!duplicate_completed);

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: stale pending completion loses to delete replacement and eviction"
#[test]
fn storage_stale_pending_completion_loses_to_delete_replacement_and_eviction() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage {
        total_limit: 8,
        ..ImageStorage::default()
    };

    let deleted = s
        .add_pending_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                data: Data::Pending(4),
                ..Image::default()
            },
        )
        .unwrap();
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Id {
            delete: true,
            image_id: 1,
            placement_id: 0,
        },
    );
    let deleted_data = b"gone".to_vec();
    let deleted_completed = deleted.complete(&mut s, crate::alloc::heap_bytes(&deleted_data));
    assert!(!deleted_completed);

    let replaced = s
        .add_pending_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                data: Data::Pending(4),
                ..Image::default()
            },
        )
        .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2,
            data: Data::Complete(crate::alloc::heap_bytes(b"live")),
            ..Image::default()
        },
    )
    .unwrap();
    let replaced_data = b"late".to_vec();
    let replaced_completed = replaced.complete(&mut s, crate::alloc::heap_bytes(&replaced_data));
    assert!(!replaced_completed);
    // try testing.expectEqualStrings("live", s.imageById(2).?.data.bytes().?);
    let live = s.image_by_id(2).unwrap();
    assert_eq!(b"live".as_slice(), live.data.bytes().unwrap());

    let evicted = s
        .add_pending_image(
            alloc,
            t.screens.active,
            Image {
                id: 3,
                data: Data::Pending(4),
                ..Image::default()
            },
        )
        .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 4,
            data: Data::Complete(crate::alloc::heap_bytes(b"12345678")),
            ..Image::default()
        },
    )
    .unwrap();
    assert!(s.image_by_id(3).is_none());
    let evicted_data = b"late".to_vec();
    let evicted_completed = evicted.complete(&mut s, crate::alloc::heap_bytes(&evicted_data));
    assert!(!evicted_completed);

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: replacement reuses pending reservation and removes placements"
#[test]
fn storage_replacement_reuses_pending_reservation_and_removes_placements() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage {
        total_limit: 12,
        ..ImageStorage::default()
    };
    let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

    let pending = s
        .add_pending_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                width: 2,
                height: 1,
                format: command::Format::Rgba,
                data: Data::Pending(8),
                ..Image::default()
            },
        )
        .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
        )),
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2,
            data: Data::Complete(crate::alloc::heap_bytes(b"keep")),
            ..Image::default()
        },
    )
    .unwrap();

    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 2,
            height: 1,
            format: command::Format::Rgba,
            data: Data::Complete(crate::alloc::heap_bytes(b"12345678")),
            ..Image::default()
        },
    )
    .unwrap();
    assert!(s.images.contains_key(&1));
    assert!(s.images.contains_key(&2));
    assert_eq!(0usize, s.placements.len());
    assert_eq!(
        tracked,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );
    // try testing.expectEqual(@as(u30, 0), s.imageById(1).?.metadata.placement_count);
    assert_eq!(0u32, s.image_by_id(1).unwrap().metadata.placement_count);
    assert_eq!(12usize, s.total_bytes);

    let stale = b"snapshot".to_vec();
    let stale_completed = pending.complete(&mut s, crate::alloc::heap_bytes(&stale));
    assert!(!stale_completed);

    // Growing the replacement requires eviction, but the replacement ID is
    // excluded. The other image supplies the needed bytes.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            data: Data::Complete(crate::alloc::heap_bytes(b"1234567890")),
            ..Image::default()
        },
    )
    .unwrap();
    assert!(s.images.contains_key(&1));
    assert!(!s.images.contains_key(&2));
    assert_eq!(0usize, s.placements.len());
    assert_eq!(10usize, s.total_bytes);

    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Id {
            delete: true,
            image_id: 1,
            placement_id: 0,
        },
    );
    assert_eq!(0usize, s.images.len());
    assert_eq!(0usize, s.placements.len());

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: pending images share exact eviction ordering"
#[test]
fn storage_pending_images_share_exact_eviction_ordering() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage {
        total_limit: 192,
        ..ImageStorage::default()
    };

    let _ = s
        .add_pending_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                data: Data::Pending(64),
                metadata: Metadata {
                    transient: false,
                    ..Metadata::default()
                },
                ..Image::default()
            },
        )
        .unwrap();
    let _ = s
        .add_pending_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                data: Data::Pending(64),
                metadata: Metadata {
                    transient: true,
                    ..Metadata::default()
                },
                ..Image::default()
            },
        )
        .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 3,
            data: Data::Complete(crate::alloc::heap_bytes(&[b'*'; 64])),
            metadata: Metadata {
                transient: true,
                ..Metadata::default()
            },
            ..Image::default()
        },
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        2,
        1,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
        )),
    )
    .unwrap();

    // ID 3 is transient and unused, so one exact-size eviction is enough.
    assert!(s.evict_image(alloc, t.screens.active, 64));
    assert!(s.images.contains_key(&1));
    assert!(s.images.contains_key(&2));
    assert!(!s.images.contains_key(&3));
    assert_eq!(128usize, s.total_bytes);

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}
// test "storage: delete all visible placements preserves scrollback"
#[test]
fn delete_all_visible_placements_preserves_scrollback() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(
        alloc,
        crate::terminal::Options {
            max_scrollback_bytes: Some(4096),
            ..opts(2, 2)
        },
    )
    .unwrap();
    t.width_px = 2;
    t.height_px = 2;

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 1,
            height: 1,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
        )),
    )
    .unwrap();
    t.scroll_up(1).unwrap();

    let history_key: PlacementKey = PlacementKey {
        image_id: 1,
        placement_id: PlacementId {
            tag: PlacementIdTag::External,
            id: 1,
        },
    };
    {
        let pin = match s.placements.get(&history_key).unwrap().location {
            Location::Pin(pin) => pin,
            _ => unreachable!(),
        };
        assert!(unsafe { &(*t.screens.active).pages }
            .point_from_pin(point::Tag::Active, unsafe { *pin })
            .is_none());
    }

    s.add_placement(
        alloc,
        t.screens.active,
        1,
        2,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
        )),
    )
    .unwrap();

    s.delete(alloc, &mut t, command::DeleteAction::All(true));
    assert_eq!(1usize, s.placements.len());
    assert!(s.placements.contains_key(&history_key));
    assert_eq!(1usize, s.images.len());
    assert!(s.images.contains_key(&1));

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete all includes placements spanning into active area"
#[test]
fn delete_all_includes_placements_spanning_into_active_area() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(
        alloc,
        crate::terminal::Options {
            max_scrollback_bytes: Some(4096),
            ..opts(2, 2)
        },
    )
    .unwrap();
    t.width_px = 2;
    t.height_px = 2;

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 1,
            height: 2,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
        )),
    )
    .unwrap();
    t.scroll_up(1).unwrap();

    let placement = *s
        .placements
        .get(&PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 1,
            },
        })
        .unwrap();
    {
        let pin = match placement.location {
            Location::Pin(pin) => pin,
            _ => unreachable!(),
        };
        assert!(unsafe { &(*t.screens.active).pages }
            .point_from_pin(point::Tag::Active, unsafe { *pin })
            .is_none());
    }
    let rect = placement.rect(s.image_by_id(1).unwrap(), &t).unwrap();
    assert!(unsafe { &(*t.screens.active).pages }
        .point_from_pin(point::Tag::Active, rect.bottom_right)
        .is_some());

    s.delete(alloc, &mut t, command::DeleteAction::All(true));
    assert_eq!(0usize, s.placements.len());
    assert_eq!(0usize, s.images.len());

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: uppercase id delete preserves image when placement does not match"
#[test]
fn storage_uppercase_id_delete_preserves_image_when_placement_does_not_match() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            ..Image::default()
        },
    )
    .unwrap();

    s.dirty = false;
    let generation = s.generation;
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Id {
            delete: true,
            image_id: 1,
            placement_id: 7,
        },
    );

    assert!(s.image_by_id(1).is_some());
    assert!(!s.dirty);
    assert_eq!(generation, s.generation);

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: uppercase id delete frees image after placement matches"
#[test]
fn storage_uppercase_id_delete_frees_image_after_placement_matches() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();
    let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        9,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
        )),
    )
    .unwrap();

    s.dirty = false;
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Id {
            delete: true,
            image_id: 1,
            placement_id: 9,
        },
    );

    assert_eq!(0usize, s.placements.len());
    assert_eq!(0usize, s.images.len());
    assert!(s.dirty);
    assert_eq!(
        tracked,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: uppercase id delete without placement frees unplaced image"
#[test]
fn storage_uppercase_id_delete_without_placement_frees_unplaced_image() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            ..Image::default()
        },
    )
    .unwrap();

    s.dirty = false;
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Id {
            delete: true,
            image_id: 1,
            placement_id: 0,
        },
    );

    assert_eq!(0usize, s.images.len());
    assert!(s.dirty);

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: uppercase range deletes unplaced image data"
#[test]
fn storage_uppercase_range_deletes_unplaced_image_data() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 3,
            ..Image::default()
        },
    )
    .unwrap();

    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Range {
            delete: true,
            // Zero is the default lower bound when x is omitted.
            first: 0,
            last: 2,
        },
    );
    assert_eq!(1usize, s.images.len());
    assert!(s.images.contains_key(&3));

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete images by empty range"
#[test]
fn delete_images_by_empty_range() {
    // Ranges that select nothing are a silent no-op, matching kitty, which
    // performs no validation and filters with `x <= image_id <= y`.
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();
    let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
        )),
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        2,
        1,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
        )),
    )
    .unwrap();

    // Inverted range.
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Range {
            delete: true,
            first: 5,
            last: 4,
        },
    );
    assert_eq!(2usize, s.images.len());
    assert_eq!(2usize, s.placements.len());

    // Upper bound omitted, so it defaults to zero.
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Range {
            delete: true,
            first: 5,
            last: 0,
        },
    );
    assert_eq!(2usize, s.images.len());
    assert_eq!(2usize, s.placements.len());

    // Both bounds omitted. Image IDs are never zero so this matches nothing.
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Range {
            delete: true,
            first: 0,
            last: 0,
        },
    );
    assert_eq!(2usize, s.images.len());
    assert_eq!(2usize, s.placements.len());

    // Both placements survive, so both of their pins are still tracked.
    assert_eq!(
        tracked + 2,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: erase display preserves scrollback and reclaims unplaced images"
#[test]
fn storage_erase_display_preserves_scrollback_and_reclaims_unplaced_images() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(
        alloc,
        crate::terminal::Options {
            max_scrollback_bytes: Some(4096),
            ..opts(2, 2)
        },
    )
    .unwrap();
    t.width_px = 2;
    t.height_px = 2;

    // const s = &t.screens.active.kitty_images;
    // PORT NOTE: this test drives the TERMINAL's own storage (so
    // `t.eraseDisplay` reaches it); every access re-derives the reference
    // from the raw screen pointer so no alias survives a `&mut t` call.
    let sp = t.screens.active;
    unsafe { &mut (*sp).kitty_images }
        .add_image(
            alloc,
            sp,
            Image {
                id: 1,
                width: 1,
                height: 1,
                ..Image::default()
            },
        )
        .unwrap();
    {
        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
        unsafe { &mut (*sp).kitty_images }
            .add_placement(
                alloc,
                sp,
                1,
                1,
                Placement::with_location(Location::Pin(pin)),
            )
            .unwrap();
    }
    t.scroll_up(1).unwrap();

    let history_key: PlacementKey = PlacementKey {
        image_id: 1,
        placement_id: PlacementId {
            tag: PlacementIdTag::External,
            id: 1,
        },
    };
    {
        let s = unsafe { &(*sp).kitty_images };
        let pin = match s.placements.get(&history_key).unwrap().location {
            Location::Pin(pin) => pin,
            _ => unreachable!(),
        };
        assert!(unsafe { &(*sp).pages }
            .point_from_pin(point::Tag::Active, unsafe { *pin })
            .is_none());
    }

    unsafe { &mut (*sp).kitty_images }
        .add_image(
            alloc,
            sp,
            Image {
                id: 2,
                width: 1,
                height: 1,
                ..Image::default()
            },
        )
        .unwrap();
    {
        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
        unsafe { &mut (*sp).kitty_images }
            .add_placement(
                alloc,
                sp,
                2,
                1,
                Placement::with_location(Location::Pin(pin)),
            )
            .unwrap();
    }
    unsafe { &mut (*sp).kitty_images }
        .add_image(
            alloc,
            sp,
            Image {
                id: 3,
                width: 1,
                height: 1,
                ..Image::default()
            },
        )
        .unwrap();

    t.erase_display(crate::csi::EraseDisplay::Complete, false);
    {
        let s = unsafe { &(*sp).kitty_images };
        assert_eq!(1usize, s.placements.len());
        assert!(s.placements.contains_key(&history_key));
        assert_eq!(1usize, s.images.len());
        assert!(s.images.contains_key(&1));
        assert!(!s.images.contains_key(&2));
        assert!(!s.images.contains_key(&3));
    }

    t.deinit(alloc);
}

// test "storage: cell offsets stay within explicit destination rectangle"
#[test]
fn storage_cell_offsets_stay_within_explicit_destination_rectangle() {
    let alloc = crate::alloc::Global;

    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
    t.width_px = 50; // 10 px per col
    t.height_px = 100; // 20 px per row

    // Explicit columns and rows describe the far cell boundaries. Offsets
    // move the near edge inward without moving those far edges.
    let placement: Placement = Placement {
        x_offset: 3,
        y_offset: 4,
        columns: 2,
        rows: 1,
        ..Placement::with_location(Location::Virtual)
    };
    let image_4x3 = Image {
        width: 4,
        height: 3,
        ..Image::default()
    };
    let actual = placement.pixel_size(&image_4x3, &t);
    assert_eq!(17u32, actual.width);
    assert_eq!(16u32, actual.height);
    assert_eq!(2u32, placement.grid_size(&image_4x3, &t).cols);
    assert_eq!(1u32, placement.grid_size(&image_4x3, &t).rows);

    t.deinit(alloc);
}

// test "storage: cell offsets clamp to cell bounds"
#[test]
fn storage_cell_offsets_clamp_to_cell_bounds() {
    let alloc = crate::alloc::Global;

    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
    t.width_px = 50; // 10 px per col
    t.height_px = 100; // 20 px per row

    let placement: Placement = Placement {
        x_offset: u32::MAX,
        y_offset: u32::MAX,
        columns: 1,
        rows: 1,
        ..Placement::with_location(Location::Virtual)
    };
    let offset = placement.cell_offset(&t);
    assert_eq!(9u32, offset.x);
    assert_eq!(19u32, offset.y);

    // Even hostile offsets leave one pixel inside the requested cell.
    let image_1x1 = Image {
        width: 1,
        height: 1,
        ..Image::default()
    };
    let actual = placement.pixel_size(&image_1x1, &t);
    assert_eq!(1u32, actual.width);
    assert_eq!(1u32, actual.height);

    t.deinit(alloc);
}

// test "storage: default source rectangle is intersected before sizing"
#[test]
fn storage_default_source_rectangle_is_intersected_before_sizing() {
    let alloc = crate::alloc::Global;

    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    let placement: Placement = Placement {
        source_x: 3,
        source_y: 1,
        ..Placement::with_location(Location::Virtual)
    };
    let actual = placement.pixel_size(
        &Image {
            width: 4,
            height: 3,
            ..Image::default()
        },
        &t,
    );
    assert_eq!(1u32, actual.width);
    assert_eq!(2u32, actual.height);

    t.deinit(alloc);
}

// test "storage: explicit source rectangle is intersected before sizing"
#[test]
fn storage_explicit_source_rectangle_is_intersected_before_sizing() {
    let alloc = crate::alloc::Global;

    let mut t = Terminal::init(alloc, opts(10, 10)).unwrap();
    t.width_px = 100;
    t.height_px = 100;

    // The requested 8x8 rectangle intersects this 10x10 image as 2x3.
    // With a two-column destination, the clipped 2:3 aspect ratio produces
    // a 20x30 pixel destination.
    let placement: Placement = Placement {
        source_x: 8,
        source_y: 7,
        source_width: 8,
        source_height: 8,
        columns: 2,
        ..Placement::with_location(Location::Virtual)
    };
    let actual = placement.pixel_size(
        &Image {
            width: 10,
            height: 10,
            ..Image::default()
        },
        &t,
    );
    assert_eq!(20u32, actual.width);
    assert_eq!(30u32, actual.height);

    t.deinit(alloc);
}

// test "storage: nextImageId number matches Kitty get_free_client_id"
#[test]
fn storage_next_image_id_number_matches_kitty_get_free_client_id() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();

    // Empty storage returns 1, as does Kitty.
    assert_eq!(1u32, s.next_image_id_for(NextImageIdMode::Explicit));

    // Contiguous IDs from 1: the next ID after them.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 3,
            ..Image::default()
        },
    )
    .unwrap();
    assert_eq!(4u32, s.next_image_id_for(NextImageIdMode::Explicit));

    // The first gap is filled, even when higher IDs exist.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 100,
            ..Image::default()
        },
    )
    .unwrap();
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Id {
            image_id: 2,
            delete: true,
            placement_id: 0,
        },
    );
    assert_eq!(2u32, s.next_image_id_for(NextImageIdMode::Explicit));

    // 1 is reused as soon as it is free.
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Id {
            image_id: 1,
            delete: true,
            placement_id: 0,
        },
    );
    assert_eq!(1u32, s.next_image_id_for(NextImageIdMode::Explicit));

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: nextImageId implicit skips in-use ids and zero"
#[test]
fn storage_next_image_id_implicit_skips_in_use_ids_and_zero() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();

    // A client image already sits on the counter's first two values.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2147483647,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2147483648,
            ..Image::default()
        },
    )
    .unwrap();
    assert_eq!(
        2147483649u32,
        s.next_image_id_for(NextImageIdMode::Implicit)
    );
    assert_eq!(
        2147483650u32,
        s.next_image_id_for(NextImageIdMode::Implicit)
    );

    // Wrapping skips zero, which means "no ID" protocol-wide.
    s.next_image_id = u32::MAX;
    assert_eq!(u32::MAX, s.next_image_id_for(NextImageIdMode::Implicit));
    assert_eq!(1u32, s.next_image_id_for(NextImageIdMode::Implicit));

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: resolveChain accumulates offsets to the pin root"
#[test]
fn storage_resolve_chain_accumulates_offsets_to_the_pin_root() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
        )),
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        2,
        Placement::with_location(Location::Relative(Relative {
            parent: PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 1,
                },
            },
            horizontal_offset: 2,
            vertical_offset: 1,
        })),
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        3,
        Placement::with_location(Location::Relative(Relative {
            parent: PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 2,
                },
            },
            horizontal_offset: -1,
            vertical_offset: 4,
        })),
    )
    .unwrap();

    let grandchild = *s
        .placements
        .get(&PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 3,
            },
        })
        .unwrap();
    let rel = match grandchild.location {
        Location::Relative(rel) => rel,
        _ => unreachable!(),
    };
    let chain = s.resolve_chain(rel).unwrap();
    assert!(chain.root_key.eql(PlacementKey {
        image_id: 1,
        placement_id: PlacementId {
            tag: PlacementIdTag::External,
            id: 1,
        },
    }));
    assert!(matches!(chain.root.location, Location::Pin(_)));
    assert_eq!(1i32, chain.horizontal_offset);
    assert_eq!(5i32, chain.vertical_offset);

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: resolveChain finds virtual roots"
#[test]
fn storage_resolve_chain_finds_virtual_roots() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Virtual),
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        2,
        Placement::with_location(Location::Relative(Relative {
            parent: PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 1,
                },
            },
            horizontal_offset: 1,
            vertical_offset: 0,
        })),
    )
    .unwrap();

    let child = *s
        .placements
        .get(&PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 2,
            },
        })
        .unwrap();
    let rel = match child.location {
        Location::Relative(rel) => rel,
        _ => unreachable!(),
    };
    let chain = s.resolve_chain(rel).unwrap();
    assert!(matches!(chain.root.location, Location::Virtual));
    assert!(chain.root_key.eql(PlacementKey {
        image_id: 1,
        placement_id: PlacementId {
            tag: PlacementIdTag::External,
            id: 1,
        },
    }));
    assert_eq!(1i32, chain.horizontal_offset);
    assert_eq!(0i32, chain.vertical_offset);

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: eviction removes orphaned relative placements"
#[test]
fn storage_eviction_removes_orphaned_relative_placements() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();
    s.total_limit = 8;

    // Image 1 holds most of the byte budget and has a pin placement.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 2,
            height: 1,
            data: Data::Complete(crate::alloc::heap_bytes(&[0, 0, 0, 0, 0, 0])),
            ..Image::default()
        },
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        1,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
        )),
    )
    .unwrap();

    // Image 2's placement is relative to image 1's placement.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        2,
        1,
        Placement::with_location(Location::Relative(Relative {
            parent: PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 1,
                },
            },
            horizontal_offset: 0,
            vertical_offset: 0,
        })),
    )
    .unwrap();

    // Adding image 3 exceeds the limit and evicts image 1 (oldest),
    // removing its placement, which orphans image 2's placement.
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 3,
            width: 2,
            height: 1,
            data: Data::Complete(crate::alloc::heap_bytes(&[0, 0, 0, 0, 0, 0])),
            ..Image::default()
        },
    )
    .unwrap();

    assert!(s.image_by_id(1).is_none());
    assert_eq!(0usize, s.placements.len());
    assert!(s.image_by_id(2).is_some());

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: placeholderTarget lookup"
#[test]
fn storage_placeholder_target_lookup() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        5,
        Placement::with_location(Location::Virtual),
    )
    .unwrap();

    // Exact external ID match.
    {
        let target = s.placeholder_target(1, 5).unwrap();
        let expected: PlacementKey = PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 5,
            },
        };
        assert_eq!(expected, target.key);
        assert!(matches!(target.placement.location, Location::Virtual));
    }

    // Zero placement ID falls back to the image's virtual placement.
    {
        let target = s.placeholder_target(1, 0).unwrap();
        let expected: PlacementKey = PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 5,
            },
        };
        assert_eq!(expected, target.key);
    }

    assert!(s.placeholder_target(1, 9).is_none());
    assert!(s.placeholder_target(2, 0).is_none());

    // With multiple virtual placements, the zero-ID fallback picks
    // deterministically by PlacementId.preferredOver: lowest external.
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        3,
        Placement::with_location(Location::Virtual),
    )
    .unwrap();
    {
        let expected: PlacementKey = PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 3,
            },
        };
        assert_eq!(expected, s.placeholder_target(1, 0).unwrap().key);
    }

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}
