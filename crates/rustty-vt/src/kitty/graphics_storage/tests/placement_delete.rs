//! PORTS: ghostty src/terminal/kitty/graphics_storage.zig — the
//! `storage: add placement` / `storage: delete ...` tests (by visibility, id,
//! intersection, column, row and image range) and the aspect-ratio test.

use super::*;

// test "storage: add placement with zero placement id"
#[test]
fn add_placement_with_zero_placement_id() {
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
            width: 50,
            height: 50,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2,
            width: 25,
            height: 25,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        0,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 25, y: 25 }).unwrap(),
        )),
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        0,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 25, y: 25 }).unwrap(),
        )),
    )
    .unwrap();

    assert_eq!(2usize, s.placements.len());
    assert_eq!(2usize, s.images.len());

    // verify the placement is what we expect
    assert!(s
        .placements
        .get(&PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::Internal,
                id: 0
            },
        })
        .is_some());
    assert!(s
        .placements
        .get(&PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::Internal,
                id: 1
            },
        })
        .is_some());

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete all visible placements and matching images"
// (upstream `f8b40a023` DELETED the pin-era "storage: delete all placements
// and images" test and replaced it with this one: d=A only frees images
// selected by a matching visible placement, so the unplaced image 3
// survives.)
#[test]
fn delete_all_visible_placements_and_matching_images() {
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
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 3,
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

    s.dirty = false;
    s.delete(alloc, &mut t, command::DeleteAction::All(true));
    assert!(s.dirty);
    assert_eq!(1usize, s.images.len());
    assert!(s.images.contains_key(&3));
    assert_eq!(0usize, s.placements.len());
    assert_eq!(
        tracked,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete all placements and images preserves limit"
#[test]
fn delete_all_placements_and_images_preserves_limit() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();
    let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

    let mut s: ImageStorage = ImageStorage::default();
    s.total_limit = 5000;
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

    s.dirty = false;
    s.delete(alloc, &mut t, command::DeleteAction::All(true));
    assert!(s.dirty);
    assert_eq!(1usize, s.images.len());
    assert!(s.images.contains_key(&3));
    assert_eq!(0usize, s.placements.len());
    assert_eq!(5000usize, s.total_limit);
    assert_eq!(
        tracked,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete all placements"
#[test]
fn delete_all_placements() {
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
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 3,
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

    s.dirty = false;
    s.delete(alloc, &mut t, command::DeleteAction::All(false));
    assert!(s.dirty);
    assert_eq!(0usize, s.placements.len());
    assert_eq!(3usize, s.images.len());
    assert_eq!(
        tracked,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete all placements by image id"
#[test]
fn delete_all_placements_by_image_id() {
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
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 3,
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

    s.dirty = false;
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Id {
            delete: false,
            image_id: 2,
            placement_id: 0,
        },
    );
    assert!(s.dirty);
    assert_eq!(1usize, s.placements.len());
    assert_eq!(3usize, s.images.len());
    assert_eq!(
        tracked + 1,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete all placements by image id and unused images"
#[test]
fn delete_all_placements_by_image_id_and_unused_images() {
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
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 3,
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

    s.dirty = false;
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Id {
            delete: true,
            image_id: 2,
            placement_id: 0,
        },
    );
    assert!(s.dirty);
    assert_eq!(1usize, s.placements.len());
    assert_eq!(2usize, s.images.len());
    assert_eq!(
        tracked + 1,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete placement by specific id"
#[test]
fn delete_placement_by_specific_id() {
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
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 3,
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

    s.dirty = false;
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Id {
            delete: true,
            image_id: 1,
            placement_id: 2,
        },
    );
    assert!(s.dirty);
    assert_eq!(2usize, s.placements.len());
    assert_eq!(3usize, s.images.len());
    assert_eq!(
        tracked + 2,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete intersecting cursor"
#[test]
fn delete_intersecting_cursor() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
    t.width_px = 100;
    t.height_px = 100;
    let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 50,
            height: 50,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2,
            width: 25,
            height: 25,
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
            track_pin(&mut t, point::Coordinate { x: 25, y: 25 }).unwrap(),
        )),
    )
    .unwrap();

    unsafe { (*t.screens.active).cursor_absolute(12, 12) };

    s.dirty = false;
    s.delete(alloc, &mut t, command::DeleteAction::IntersectCursor(false));
    assert!(s.dirty);
    assert_eq!(1usize, s.placements.len());
    assert_eq!(2usize, s.images.len());
    assert_eq!(
        tracked + 1,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    // verify the placement is what we expect
    assert!(s
        .placements
        .get(&PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 2
            },
        })
        .is_some());

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete intersecting cursor plus unused"
#[test]
fn delete_intersecting_cursor_plus_unused() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
    t.width_px = 100;
    t.height_px = 100;
    let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 50,
            height: 50,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2,
            width: 25,
            height: 25,
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
            track_pin(&mut t, point::Coordinate { x: 25, y: 25 }).unwrap(),
        )),
    )
    .unwrap();

    unsafe { (*t.screens.active).cursor_absolute(12, 12) };

    s.dirty = false;
    s.delete(alloc, &mut t, command::DeleteAction::IntersectCursor(true));
    assert!(s.dirty);
    assert_eq!(1usize, s.placements.len());
    assert_eq!(2usize, s.images.len());
    assert_eq!(
        tracked + 1,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    // verify the placement is what we expect
    assert!(s
        .placements
        .get(&PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 2
            },
        })
        .is_some());

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete intersecting cursor hits multiple"
#[test]
fn delete_intersecting_cursor_hits_multiple() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
    t.width_px = 100;
    t.height_px = 100;
    let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 50,
            height: 50,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2,
            width: 25,
            height: 25,
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
            track_pin(&mut t, point::Coordinate { x: 25, y: 25 }).unwrap(),
        )),
    )
    .unwrap();

    unsafe { (*t.screens.active).cursor_absolute(26, 26) };

    s.dirty = false;
    s.delete(alloc, &mut t, command::DeleteAction::IntersectCursor(true));
    assert!(s.dirty);
    assert_eq!(0usize, s.placements.len());
    assert_eq!(1usize, s.images.len());
    assert_eq!(
        tracked,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete by column"
#[test]
fn delete_by_column() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
    t.width_px = 100;
    t.height_px = 100;
    let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 50,
            height: 50,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2,
            width: 25,
            height: 25,
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
            track_pin(&mut t, point::Coordinate { x: 25, y: 25 }).unwrap(),
        )),
    )
    .unwrap();

    s.dirty = false;
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Column {
            delete: false,
            x: 60,
        },
    );
    assert!(s.dirty);
    assert_eq!(1usize, s.placements.len());
    assert_eq!(2usize, s.images.len());
    assert_eq!(
        tracked + 1,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    // verify the placement is what we expect
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

// test "storage: delete by column 1x1"
#[test]
fn delete_by_column_1x1() {
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
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        2,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 1, y: 0 }).unwrap(),
        )),
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        3,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 2, y: 0 }).unwrap(),
        )),
    )
    .unwrap();

    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Column {
            delete: false,
            x: 2,
        },
    );
    assert_eq!(2usize, s.placements.len());
    assert_eq!(1usize, s.images.len());

    // verify the placement is what we expect
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
    assert!(s
        .placements
        .get(&PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 3
            },
        })
        .is_some());

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete by row"
#[test]
fn delete_by_row() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
    t.width_px = 100;
    t.height_px = 100;
    let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

    let mut s: ImageStorage = ImageStorage::default();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 1,
            width: 50,
            height: 50,
            ..Image::default()
        },
    )
    .unwrap();
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 2,
            width: 25,
            height: 25,
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
            track_pin(&mut t, point::Coordinate { x: 25, y: 25 }).unwrap(),
        )),
    )
    .unwrap();

    s.dirty = false;
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Row {
            delete: false,
            y: 60,
        },
    );
    assert!(s.dirty);
    assert_eq!(1usize, s.placements.len());
    assert_eq!(2usize, s.images.len());
    assert_eq!(
        tracked + 1,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    // verify the placement is what we expect
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

// test "storage: delete by row 1x1"
#[test]
fn delete_by_row_1x1() {
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
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        2,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 0, y: 1 }).unwrap(),
        )),
    )
    .unwrap();
    s.add_placement(
        alloc,
        t.screens.active,
        1,
        3,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 0, y: 2 }).unwrap(),
        )),
    )
    .unwrap();

    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Row {
            delete: false,
            y: 2,
        },
    );
    assert_eq!(2usize, s.placements.len());
    assert_eq!(1usize, s.images.len());

    // verify the placement is what we expect
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
    assert!(s
        .placements
        .get(&PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 3
            },
        })
        .is_some());

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// S190-F1: `delete`'s `DeleteAction::Column { x: 0, .. }` arm logs
// `log.warn("delete column must be greater than zero", .{})`
// (graphics_storage.zig:1294-1297) — the reference's 1-indexed column
// rejects a literal 0.
#[test]
fn delete_column_zero_logs_and_is_a_no_op() {
    let alloc = crate::alloc::Global;
    let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();
    let mut s: ImageStorage = ImageStorage::default();

    let cap = crate::log_capture::start();
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Column {
            delete: false,
            x: 0,
        },
    );
    let messages = cap.messages();
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Warn
                && target == "kitty_gfx"
                && *msg == "delete column must be greater than zero"),
        "expected the delete-column-must-be-positive diagnostic, got: {messages:?}"
    );
    drop(cap);
    assert!(!s.dirty);

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// S190-F1: `evictImageExcept` logs each image it evicts
// (`log.info("evicting image id={} bytes={}", .{ c.id, image_len })`,
// graphics_storage.zig:1691) — both interpolated values are asserted.
#[test]
fn eviction_logs_the_evicted_image_id_and_size() {
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
            id: 7,
            data: Data::Complete(crate::alloc::heap_bytes(&[b'*'; 64])),
            ..Image::default()
        },
    )
    .unwrap();
    let image_len = s.images.get(&7).unwrap().storage_size();

    let cap = crate::log_capture::start();
    assert!(s.evict_image(alloc, t.screens.active, 1));
    let messages = cap.messages();
    assert!(!s.images.contains_key(&7));
    assert!(
        messages
            .iter()
            .any(|(level, target, msg)| *level == log::Level::Info
                && target == "kitty_gfx"
                && *msg == format!("evicting image id=7 bytes={image_len}")),
        "expected the eviction diagnostic, got: {messages:?}"
    );
    // Guard against `image_len` being a vacuous 0 that would make the
    // interpolated-size half of the assertion unfalsifiable.
    assert_eq!(64, image_len);
    drop(cap);

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete images by range 1"
#[test]
fn delete_images_by_range_1() {
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
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 3,
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
    assert_eq!(3usize, s.images.len());
    assert_eq!(2usize, s.placements.len());

    s.dirty = false;
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Range {
            delete: false,
            first: 1,
            last: 2,
        },
    );
    assert!(s.dirty);
    assert_eq!(3usize, s.images.len());
    assert_eq!(0usize, s.placements.len());
    assert_eq!(
        tracked,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete images by range 2"
#[test]
fn delete_images_by_range_2() {
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
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 3,
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
    assert_eq!(3usize, s.images.len());
    assert_eq!(2usize, s.placements.len());

    s.dirty = false;
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Range {
            delete: true,
            first: 1,
            last: 2,
        },
    );
    assert!(s.dirty);
    assert_eq!(1usize, s.images.len());
    assert_eq!(0usize, s.placements.len());
    assert_eq!(
        tracked,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete images by range 3"
#[test]
fn delete_images_by_range_3() {
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
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 3,
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
    s.add_placement(
        alloc,
        t.screens.active,
        3,
        1,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
        )),
    )
    .unwrap();
    assert_eq!(3usize, s.images.len());
    assert_eq!(3usize, s.placements.len());

    s.dirty = false;
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Range {
            delete: false,
            first: 2,
            last: 2,
        },
    );
    assert!(s.dirty);
    assert_eq!(3usize, s.images.len());
    assert_eq!(2usize, s.placements.len());
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
    assert!(s
        .placements
        .get(&PlacementKey {
            image_id: 3,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 1
            },
        })
        .is_some());
    assert_eq!(
        tracked + 2,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: delete images by range 4"
#[test]
fn delete_images_by_range_4() {
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
    s.add_image(
        alloc,
        t.screens.active,
        Image {
            id: 3,
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
    s.add_placement(
        alloc,
        t.screens.active,
        3,
        1,
        Placement::with_location(Location::Pin(
            track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
        )),
    )
    .unwrap();
    assert_eq!(3usize, s.images.len());
    assert_eq!(3usize, s.placements.len());

    s.dirty = false;
    s.delete(
        alloc,
        &mut t,
        command::DeleteAction::Range {
            delete: true,
            first: 2,
            last: 2,
        },
    );
    assert!(s.dirty);
    assert_eq!(2usize, s.images.len());
    assert_eq!(2usize, s.placements.len());
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
    assert!(s
        .placements
        .get(&PlacementKey {
            image_id: 3,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 1
            },
        })
        .is_some());
    assert_eq!(
        tracked + 2,
        unsafe { &(*t.screens.active).pages }.count_tracked_pins()
    );

    s.deinit(alloc, t.screens.active);
    t.deinit(alloc);
}

// test "storage: aspect ratio calculation when only columns or rows specified"
#[test]
fn aspect_ratio_calculation_when_only_columns_or_rows_specified() {
    let alloc = crate::alloc::Global;

    let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
    t.width_px = 1000; // 10 px per col
    t.height_px = 2000; // 20 px per row

    // Case 1: Only columns specified
    {
        let image = Image {
            id: 1,
            width: 16,
            height: 9,
            ..Image::default()
        };
        let placement = Placement {
            location: Location::Virtual,
            columns: 10,
            rows: 0,
            ..Placement::with_location(Location::Virtual)
        };

        // Image is 16x9, set to a width of 10 columns, at 10px per column
        // that's 100px width. 100px * (9 / 16) = 56.25, which should round
        // to a height of 56px.

        let calc_size = placement.pixel_size(&image, &t);
        assert_eq!(100u32, calc_size.width);
        assert_eq!(56u32, calc_size.height);
    }

    // Case 2: Only rows specified
    {
        let image = Image {
            id: 2,
            width: 16,
            height: 9,
            ..Image::default()
        };
        let placement = Placement {
            location: Location::Virtual,
            columns: 0,
            rows: 5,
            ..Placement::with_location(Location::Virtual)
        };

        // Image is 16x9, set to a height of 5 rows, at 20px per row that's
        // 100px height. 100px * (16 / 9) = 177.77..., which should round to
        // a width of 178px.

        let calc_size = placement.pixel_size(&image, &t);
        assert_eq!(178u32, calc_size.width);
        assert_eq!(100u32, calc_size.height);
    }

    t.deinit(alloc);
}
