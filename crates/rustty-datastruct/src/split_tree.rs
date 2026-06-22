//! SplitTree represents a tree of view types that can be divided.
//
// PORT NOTE: The Zig reference is `fn SplitTree(comptime V: type) type` — an
// immutable tree of ref-counted views stored in an `ArenaAllocator`-backed
// `[]const Node`. Operations return brand-new trees. We reproduce this exactly
// with `V: View` (the view trait below carries `ref`/`unref`/`eql` and the
// optional `splitTreeLabel`), and store nodes in an owned `Vec<Node>` (the arena
// in Zig owns one allocation per tree; an owned `Vec` is the Rust equivalent and
// frees on `Drop`). Split ratios and spatial math use `f16` in the reference; we
// use the software `F16` type (see `f16.rs`) so the ASCII-diagram tests — which
// depend on exact half-precision rounding — match byte-for-byte. The GTK
// `getGObjectType` member is gated on `build_config.app_runtime == .gtk` in the
// reference; this Miri-able datastruct leaf is built only for the `.none`
// runtime (where the reference's value is `void`), so it is ported as a
// documented no-op (see end of file).
//
// Concretely for Ghostty, it represents a tree of terminal views. In
// its basic state, there are no splits and it is a single full-sized
// terminal. However, it can be split arbitrarily many times among two
// axes (horizontal and vertical) to create a tree of terminal views.
//
// This is an immutable tree structure, meaning all operations on it
// will return a new tree with the operation applied. This allows us to
// store versions of the tree in a history for easy undo/redo. To facilitate
// this, the stored View type must implement reference counting; this is left
// as an implementation detail of the View type.
//
// The View type will be stored as a pointer within the tree and must
// implement a number of functions to work properly:
//
//   - `fn ref(*View, Allocator) Allocator.Error!*View` - Increase a
//     reference count of the view. The Allocator will be the allocator provided
//     to the tree operation. This is allowed to copy the value if it wants to;
//     the returned value is expected to be a new reference (but that may
//     just be a copy).
//
//   - `fn unref(*View, Allocator) void` - Decrease the reference count of a
//     view. The Allocator will be the allocator provided to the tree
//     operation.
//
//   - `fn eql(*const View, *const View) bool` - Check if two views are equal.
//
// Optionally the following functions can also be implemented:
//
//   - `fn splitTreeLabel(*const View) []const u8` - Return a label that is used
//     for the debug view. If this isn't specified then the node handle
//     will be used.
//
// Note: for both the ref and unref functions, the allocator is optional.
// If the functions take less arguments, then the allocator will not be
// passed.

use crate::f16::F16;
use rustty_util::quirks::inlineAssert as assert;
use std::fmt::Write as _;

/// Error type (mirror of `Allocator.Error` / `error.OutOfMemory`).
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    OutOfMemory,
}

/// The View trait that the stored view type must implement (mirror of the
/// reference's `ref`/`unref`/`eql` and optional `splitTreeLabel` requirements).
pub trait View: Sized {
    /// Increase a reference count of the view. This is allowed to copy the
    /// value if it wants to; the returned value is expected to be a new
    /// reference (but that may just be a copy).
    ///
    /// # Safety
    /// `self` must be a valid pointer to a live view.
    unsafe fn view_ref(view: *mut Self) -> Result<*mut Self, Error>;

    /// Decrease the reference count of a view.
    ///
    /// # Safety
    /// `view` must be a valid pointer to a live view ref produced by `view_ref`
    /// (or the caller-provided root view).
    unsafe fn view_unref(view: *mut Self);

    /// Check if two views are equal.
    ///
    /// # Safety
    /// Both pointers must be valid.
    unsafe fn eql(a: *const Self, b: *const Self) -> bool;

    /// Return a label that is used for the debug view. If this isn't specified
    /// (the impl returns `None`) then the node handle will be used. The default
    /// returns `None`, modeling the reference's "splitTreeLabel not declared"
    /// case (`@hasDecl(View, "splitTreeLabel") == false`).
    ///
    /// # Safety
    /// `view` must be a valid pointer to a live view.
    unsafe fn split_tree_label(_view: *const Self) -> Option<Vec<u8>> {
        None
    }

    /// Whether this view type declares `splitTreeLabel` (mirror of
    /// `@hasDecl(View, "splitTreeLabel")`). Defaults to `false`.
    const HAS_SPLIT_TREE_LABEL: bool = false;
}

/// The layout of a split.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layout {
    Horizontal,
    Vertical,
}

impl Layout {
    /// The `{t}` tag name (mirror of Zig enum tag printing).
    fn tag_name(self) -> &'static str {
        match self {
            Layout::Horizontal => "horizontal",
            Layout::Vertical => "vertical",
        }
    }
}

/// The direction of a split insert.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Left,
    Right,
    Down,
    Up,
}

/// A handle into the nodes array. This lets us keep track of
/// nodes with 16-bit handles rather than full pointer-width
/// values.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Handle(pub u16);

/// The backing integer type for a `Handle` (mirror of `Handle.Backing = u16`).
pub type HandleBacking = u16;

impl Handle {
    /// The root handle (`= 0`).
    pub const ROOT: Handle = Handle(0);

    #[inline]
    pub fn idx(self) -> usize {
        self.0 as usize
    }

    /// Offset the handle by a given amount.
    pub fn offset(self, v: usize) -> Handle {
        let self_usize: usize = self.0 as usize;
        let final_v = self_usize + v;
        assert(final_v < u16::MAX as usize);
        Handle(final_v as u16)
    }

    #[inline]
    fn from_int(v: usize) -> Handle {
        Handle(v as u16)
    }
}

/// A split node.
#[derive(Clone, Copy)]
pub struct Split {
    pub layout: Layout,
    pub ratio: F16,
    pub left: Handle,
    pub right: Handle,
}

/// A node in the tree: either a leaf (a view pointer) or a split.
pub enum Node<V> {
    Leaf(*mut V),
    Split(Split),
}

impl<V> Clone for Node<V> {
    fn clone(&self) -> Self {
        match self {
            Node::Leaf(v) => Node::Leaf(*v),
            Node::Split(s) => Node::Split(*s),
        }
    }
}

/// A view entry yielded by the iterator.
pub struct ViewEntry<V> {
    pub handle: Handle,
    pub view: *mut V,
}

/// What kind of `goto` to perform.
pub enum Goto {
    /// Previous view, null if we're the first view.
    Previous,
    /// Next view, null if we're the last view.
    Next,
    /// Previous view, but wrapped around to the last view. May
    /// return the same view if this is the first view.
    PreviousWrapped,
    /// Next view, but wrapped around to the first view. May return
    /// the same view if this is the last view.
    NextWrapped,
    /// A spatial direction. "Spatial" means that the direction is
    /// based on the nearest surface in the given direction visually
    /// as the surfaces are laid out on a 2D grid.
    Spatial(SpatialDirection),
}

/// Side of the tree (for `deepest`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// Backtracking result for the recursive search helpers.
// PARITY: the reference `split_tree.zig:322` names this union `Backtrack` with a
// `.backtrack` tag; we mirror the variant name exactly rather than renaming it,
// so we silence `enum_variant_names`.
#[allow(clippy::enum_variant_names)]
enum Backtrack {
    Deadend,
    Backtrack,
    Result(Handle),
}

/// A spatial direction.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SpatialDirection {
    Left,
    Right,
    Down,
    Up,
}

/// A spatial slot (normalized x/y/width/height of a node).
#[derive(Clone, Copy)]
pub struct Slot {
    pub x: F16,
    pub y: F16,
    pub width: F16,
    pub height: F16,
}

impl Slot {
    fn max_x(&self) -> F16 {
        self.x + self.width
    }
    fn max_y(&self) -> F16 {
        self.y + self.height
    }
}

/// Spatial representation of the split tree. This can be used to
/// better understand the layout of the tree in a 2D space.
pub struct Spatial {
    /// The slots of the spatial representation in the same order
    /// as the tree it was created from.
    pub slots: Vec<Slot>,
}

impl Spatial {
    /// An empty spatial representation.
    pub fn empty() -> Spatial {
        Spatial { slots: Vec::new() }
    }
}

/// SplitTree represents a tree of view types that can be divided.
///
/// (See the module-level doc comment for the full contract.)
pub struct SplitTree<V: View> {
    /// All the nodes in the tree. Node at index 0 is always the root.
    ///
    /// PORT: In Zig this is `[]const Node` backed by a per-tree
    /// `ArenaAllocator`. In Rust the owned `Vec<Node>` plays both roles (it
    /// owns the single allocation and frees on `Drop`).
    pub nodes: Vec<Node<V>>,

    /// The handle of the zoomed node. A "zoomed" node is one that is
    /// expected to be made the full size of the split tree. Various
    /// operations may unzoom (e.g. resize).
    pub zoomed: Option<Handle>,
}

impl<V: View> SplitTree<V> {
    /// An empty tree.
    pub fn empty() -> Self {
        SplitTree {
            nodes: Vec::new(),
            zoomed: None,
        }
    }

    /// Initialize a new tree with a single view.
    ///
    /// # Safety
    ///
    /// PARITY: the reference `split_tree.zig:112` takes `view: *View`, a
    /// non-null Zig pointer whose validity is the caller's responsibility. We
    /// encode that contract by marking `init` `unsafe`: the caller must pass a
    /// `view` pointer that is valid for the lifetime of the tree (it is
    /// dereferenced via `view_ref`).
    pub unsafe fn init(view: *mut V) -> Result<Self, Error> {
        let mut nodes: Vec<Node<V>> = Vec::with_capacity(1);
        // nodes[0] = .{ .leaf = try viewRef(view, gpa) };
        let reffed = unsafe { Self::view_ref(view)? };
        nodes.push(Node::Leaf(reffed));

        Ok(SplitTree {
            nodes,
            zoomed: None,
        })
    }

    /// Clone this tree, returning a new tree with the same nodes.
    pub fn clone_tree(&self) -> Result<Self, Error> {
        // If we're empty then return an empty tree.
        if self.is_empty() {
            return Ok(Self::empty());
        }

        // Allocate a new nodes array and copy the existing nodes into it.
        let mut nodes: Vec<Node<V>> = self.nodes.clone();

        // Increase the reference count of all the views in the nodes.
        Self::ref_nodes(&mut nodes)?;

        Ok(SplitTree {
            nodes,
            zoomed: self.zoomed,
        })
    }

    /// Returns true if this is an empty tree.
    pub fn is_empty(&self) -> bool {
        // An empty tree has no nodes.
        self.nodes.is_empty()
    }

    /// Returns true if this tree has more than one split (i.e., the root
    /// is a split node). This is useful for determining if actions like
    /// resize_split or toggle_split_zoom are performable.
    pub fn is_split(&self) -> bool {
        // An empty tree is not split.
        if self.is_empty() {
            return false;
        }
        // The root node is at index 0. If it's a split, we have multiple splits.
        match &self.nodes[0] {
            Node::Split(_) => true,
            Node::Leaf(_) => false,
        }
    }

    /// An iterator over all the views in the tree.
    pub fn iterator(&self) -> Iterator<'_, V> {
        Iterator {
            i: Handle::ROOT,
            nodes: &self.nodes,
        }
    }

    /// Change the zoomed state to the given node. Assumes the handle
    /// is valid.
    pub fn zoom(&mut self, handle: Option<Handle>) {
        if let Some(v) = handle {
            // assert(@intFromEnum(v) >= 0); — always true for u16.
            assert((v.0 as usize) < self.nodes.len());
        }
        self.zoomed = handle;
    }

    /// Goto a view from a certain point in the split tree. Returns null
    /// if the direction results in no visitable view.
    ///
    /// Allocator is only used for temporary state for spatial navigation.
    pub fn goto(&self, from: Handle, to: Goto) -> Result<Option<Handle>, Error> {
        Ok(match to {
            Goto::Previous => self.previous(from),
            Goto::Next => self.next(from),
            Goto::PreviousWrapped => self
                .previous(from)
                .or_else(|| Some(self.deepest(Side::Right, Handle::ROOT))),
            Goto::NextWrapped => self
                .next(from)
                .or_else(|| Some(self.deepest(Side::Left, Handle::ROOT))),
            Goto::Spatial(d) => {
                // Get our spatial representation.
                let sp = self.spatial()?;
                self.nearest_wrapped(&sp, from, d)
            }
        })
    }

    /// Returns the deepest view in the tree in the given direction.
    /// This can be used to find the leftmost/rightmost surface within
    /// a given split structure.
    pub fn deepest(&self, side: Side, from: Handle) -> Handle {
        let mut current: Handle = from;
        loop {
            match &self.nodes[current.idx()] {
                Node::Leaf(_) => return current,
                Node::Split(s) => {
                    current = match side {
                        Side::Left => s.left,
                        Side::Right => s.right,
                    }
                }
            }
        }
    }

    /// Returns the previous view from the given node handle (which itself
    /// doesn't need to be a view). If there is no previous (this is the
    /// most previous view) then this will return null.
    ///
    /// "Previous" is defined as the previous node in an in-order
    /// traversal of the tree. This isn't a perfect definition and we
    /// may want to change this to something that better matches a
    /// spatial view of the tree later.
    fn previous(&self, from: Handle) -> Option<Handle> {
        match self.previous_backtrack(from, Handle::ROOT) {
            Backtrack::Result(v) => Some(v),
            Backtrack::Backtrack | Backtrack::Deadend => None,
        }
    }

    /// Same as `previous`, but returns the next view instead.
    fn next(&self, from: Handle) -> Option<Handle> {
        match self.next_backtrack(from, Handle::ROOT) {
            Backtrack::Result(v) => Some(v),
            Backtrack::Backtrack | Backtrack::Deadend => None,
        }
    }

    // Design note: we use a recursive backtracking search because
    // split trees are never that deep, so we can abuse the stack as
    // a safe allocator (stack overflow unlikely unless the kernel is
    // tuned in some really weird way).

    fn previous_backtrack(&self, from: Handle, current: Handle) -> Backtrack {
        // If we reached the point that we're trying to find the previous
        // value of, then we need to backtrack from here.
        if from == current {
            return Backtrack::Backtrack;
        }

        match &self.nodes[current.idx()] {
            // If we hit a leaf that isn't our target, then deadend.
            Node::Leaf(_) => Backtrack::Deadend,

            Node::Split(s) => match self.previous_backtrack(from, s.left) {
                Backtrack::Result(v) => Backtrack::Result(v),

                // Backtrack from the left means we have to continue
                // backtracking because we can't see what's before the left.
                Backtrack::Backtrack => Backtrack::Backtrack,

                // If we hit a deadend on the left then let's move right.
                Backtrack::Deadend => match self.previous_backtrack(from, s.right) {
                    Backtrack::Result(v) => Backtrack::Result(v),

                    // Deadend means its not in this split at all since
                    // we already tracked the left.
                    Backtrack::Deadend => Backtrack::Deadend,

                    // Backtrack means that its in our left view because
                    // we can see the immediate previous and there MUST
                    // be leaves (we can't have split-only leaves).
                    Backtrack::Backtrack => Backtrack::Result(self.deepest(Side::Right, s.left)),
                },
            },
        }
    }

    // See previousBacktrack for detailed comments. This is a mirror
    // of that.
    fn next_backtrack(&self, from: Handle, current: Handle) -> Backtrack {
        if from == current {
            return Backtrack::Backtrack;
        }
        match &self.nodes[current.idx()] {
            Node::Leaf(_) => Backtrack::Deadend,
            Node::Split(s) => match self.next_backtrack(from, s.right) {
                Backtrack::Result(v) => Backtrack::Result(v),
                Backtrack::Backtrack => Backtrack::Backtrack,
                Backtrack::Deadend => match self.next_backtrack(from, s.left) {
                    Backtrack::Result(v) => Backtrack::Result(v),
                    Backtrack::Deadend => Backtrack::Deadend,
                    Backtrack::Backtrack => Backtrack::Result(self.deepest(Side::Left, s.right)),
                },
            },
        }
    }

    /// Returns the nearest leaf node (view) in the given direction.
    /// This does not handle wrapping and will return null if there
    /// is no node in that direction.
    fn nearest(
        &self,
        sp: &Spatial,
        from: Handle,
        direction: SpatialDirection,
        target: Slot,
    ) -> Option<Handle> {
        struct Nearest {
            handle: Handle,
            distance: F16,
        }
        let mut result: Option<Nearest> = None;
        for (handle, slot) in sp.slots.iter().enumerate() {
            let slot = *slot;
            // Never match ourself
            if handle == from.idx() {
                continue;
            }

            // Only match leaves
            match &self.nodes[handle] {
                Node::Leaf(_) => {}
                Node::Split(_) => continue,
            }

            // Ensure it is in the proper direction
            let in_dir = match direction {
                SpatialDirection::Left => slot.max_x() <= target.x,
                SpatialDirection::Right => slot.x >= target.max_x(),
                SpatialDirection::Up => slot.max_y() <= target.y,
                SpatialDirection::Down => slot.y >= target.max_y(),
            };
            if !in_dir {
                continue;
            }

            // Track our distance
            let dx = slot.x - target.x;
            let dy = slot.y - target.y;
            let distance = (dx * dx + dy * dy).sqrt();

            // If we have a nearest it must be closer.
            if let Some(n) = &result {
                if distance >= n.distance {
                    continue;
                }
            }
            result = Some(Nearest {
                handle: Handle::from_int(handle),
                distance,
            });
        }

        result.map(|n| n.handle)
    }

    /// Same as nearest but supports wrapping.
    fn nearest_wrapped(
        &self,
        sp: &Spatial,
        from: Handle,
        direction: SpatialDirection,
    ) -> Option<Handle> {
        // If we can find a nearest value without wrapping, then
        // use that.
        let mut target = sp.slots[from.idx()];
        if let Some(v) = self.nearest(sp, from, direction, target) {
            return Some(v);
        }

        // The spatial grid is normalized to 1x1, so wrapping is modeled
        // by shifting the target slot by one full grid in the opposite
        // direction and reusing the same nearest distance logic.
        // We don't actually modify the grid or spatial representation,
        // this just fakes it.
        assert(target.x >= F16::from_int(0) && target.y >= F16::from_int(0));
        assert(target.max_x() <= F16::from_int(1) && target.max_y() <= F16::from_int(1));
        let one = F16::from_int(1);
        match direction {
            SpatialDirection::Left => target.x = target.x + one,
            SpatialDirection::Right => target.x = target.x - one,
            SpatialDirection::Up => target.y = target.y + one,
            SpatialDirection::Down => target.y = target.y - one,
        }

        self.nearest(sp, from, direction, target)
    }

    /// Resize the given node in place. The node MUST be a split (asserted).
    ///
    /// In general, this is an immutable data structure so this is
    /// heavily discouraged. However, this is provided for convenience
    /// and performance reasons where its very important for GUIs to
    /// update the ratio during a live resize than to redraw the entire
    /// widget tree.
    pub fn resize_in_place(&mut self, at: Handle, ratio: F16) {
        // Let's talk about this constCast. Our member are const but
        // we actually always own their memory. We don't want consumers
        // who directly access the nodes to be able to modify them
        // (without nasty stuff like this), but given this is internal
        // usage its perfectly fine to modify the node in-place.
        match &mut self.nodes[at.idx()] {
            Node::Split(s) => s.ratio = ratio,
            Node::Leaf(_) => unreachable!(),
        }
    }

    /// Insert another tree into this tree at the given node in the
    /// specified direction. The other tree will be inserted in the
    /// new direction. For example, if the direction is "right" then
    /// `insert` is inserted right of the existing node.
    ///
    /// The allocator will be used for the newly created tree.
    /// The previous trees will not be freed, but reference counts
    /// for the views will be increased accordingly for the new tree.
    pub fn split(
        &self,
        at: Handle,
        direction: Direction,
        ratio: F16,
        insert: &Self,
    ) -> Result<Self, Error> {
        // We know we're going to need the sum total of the nodes
        // between the two trees plus one for the new split node.
        let total = self.nodes.len() + insert.nodes.len() + 1;
        if total > u16::MAX as usize {
            return Err(Error::OutOfMemory);
        }
        let mut nodes: Vec<Node<V>> = Vec::with_capacity(total);

        // We can copy our nodes exactly as they are, since they're
        // mostly not changing (only `at` is changing).
        for n in self.nodes.iter() {
            nodes.push(n.clone());
        }

        // We can copy the destination nodes as well directly next to
        // the source nodes. We just have to go through and offset
        // all the handles in the destination tree to account for
        // the shift.
        let base = self.nodes.len();
        for n in insert.nodes.iter() {
            nodes.push(n.clone());
        }
        for node in nodes[base..base + insert.nodes.len()].iter_mut() {
            match node {
                Node::Leaf(_) => {}
                Node::Split(s) => {
                    // We need to offset the handles in the split
                    s.left = s.left.offset(self.nodes.len());
                    s.right = s.right.offset(self.nodes.len());
                }
            }
        }

        // Determine our split layout and if we're on the left
        let (layout, left): (Layout, bool) = match direction {
            Direction::Left => (Layout::Horizontal, true),
            Direction::Right => (Layout::Horizontal, false),
            Direction::Up => (Layout::Vertical, true),
            Direction::Down => (Layout::Vertical, false),
        };

        // Copy our previous value to the end of the nodes list and
        // create our new split node.
        let nodes_len = nodes.len();
        nodes.push(nodes[at.idx()].clone());
        // The line above pushes a copy at index nodes_len; total length is now
        // `total`. In Zig: nodes[nodes.len-1] = nodes[at.idx()]; here we pushed
        // the copy to the last slot (index total-1 == nodes_len).
        debug_assert_eq!(nodes.len(), total);
        nodes[at.idx()] = Node::Split(Split {
            layout,
            ratio,
            left: Handle::from_int(if left { self.nodes.len() } else { nodes_len }),
            right: Handle::from_int(if left { nodes_len } else { self.nodes.len() }),
        });

        // We need to increase the reference count of all the nodes.
        Self::ref_nodes(&mut nodes)?;

        Ok(SplitTree {
            nodes,
            // Splitting always resets zoom state.
            zoomed: None,
        })
    }

    /// Remove a node from the tree.
    pub fn remove(&self, at: Handle) -> Result<Self, Error> {
        assert(at.idx() < self.nodes.len());

        // If we're removing node zero then we're clearing the tree.
        if at == Handle::ROOT {
            return Ok(Self::empty());
        }

        // Allocate our new nodes list with the number of nodes we'll
        // need after the removal.
        let count = self.count_after_removal(Handle::ROOT, at, 0);
        let mut nodes: Vec<Node<V>> = Vec::with_capacity(count);
        // Initialize placeholders; removeNode writes every slot.
        for _ in 0..count {
            nodes.push(Node::Leaf(core::ptr::null_mut()));
        }

        let mut result: Self = SplitTree {
            nodes,
            zoomed: None,
        };

        // Traverse the tree and copy all our nodes into place.
        let written = self.remove_node(&mut result, 0, Handle::ROOT, at);
        assert(written != 0);

        // Increase the reference count of all the nodes.
        Self::ref_nodes(&mut result.nodes)?;

        Ok(result)
    }

    fn remove_node(
        &self,
        new: &mut Self,
        new_offset: usize,
        current: Handle,
        target: Handle,
    ) -> usize {
        assert(current != target);

        // If we have a zoomed node and this is it then we migrate it.
        if let Some(v) = self.zoomed {
            if v == current {
                assert(new.zoomed.is_none());
                new.zoomed = Some(Handle::from_int(new_offset));
            }
        }

        match &self.nodes[current.idx()] {
            // Leaf is simple, just copy it over. We don't ref anything
            // yet because it'd make undo (errdefer) harder. We do that
            // all at once later.
            Node::Leaf(view) => {
                new.nodes[new_offset] = Node::Leaf(*view);
                1
            }

            Node::Split(s) => {
                let s = *s;
                // If we're removing one of the split node sides then
                // we remove the split node itself as well and only add
                // the other (non-removed) side.
                if s.left == target {
                    return self.remove_node(new, new_offset, s.right, target);
                }
                if s.right == target {
                    return self.remove_node(new, new_offset, s.left, target);
                }

                // Neither side is being directly removed, so we traverse.
                let left = self.remove_node(new, new_offset + 1, s.left, target);
                assert(left != 0);
                let right = self.remove_node(new, new_offset + left + 1, s.right, target);
                assert(right != 0);
                new.nodes[new_offset] = Node::Split(Split {
                    layout: s.layout,
                    ratio: s.ratio,
                    left: Handle::from_int(new_offset + 1),
                    right: Handle::from_int(new_offset + 1 + left),
                });

                left + right + 1
            }
        }
    }

    /// Returns the number of nodes that would be needed to store
    /// the tree if the target node is removed.
    fn count_after_removal(&self, current: Handle, target: Handle, acc: usize) -> usize {
        assert(current != target);

        match &self.nodes[current.idx()] {
            // Leaf is simple, always takes one node.
            Node::Leaf(_) => acc + 1,

            // Split is slightly more complicated. If either side is the
            // target to remove, then we remove the split node as well
            // so our count is just the count of the other side.
            //
            // If neither side is the target, then we count both sides
            // and add one to account for the split node itself.
            Node::Split(s) => {
                let s = *s;
                if s.left == target {
                    self.count_after_removal(s.right, target, acc)
                } else if s.right == target {
                    self.count_after_removal(s.left, target, acc)
                } else {
                    self.count_after_removal(s.left, target, acc)
                        + self.count_after_removal(s.right, target, acc)
                        + 1
                }
            }
        }
    }

    /// Reference all the nodes in the given slice, handling unref if
    /// any fail. This should be called LAST so you don't have to undo
    /// the refs at any further point after this.
    fn ref_nodes(nodes: &mut [Node<V>]) -> Result<(), Error> {
        // We need to increase the reference count of all the nodes.
        // Careful accounting here so that we properly unref on error
        // only the nodes we referenced.
        let mut reffed: usize = 0;
        let mut err: Option<Error> = None;
        let len = nodes.len();
        // PARITY: mirrors reference index loop; `i` is needed to write back
        // `nodes[i]` and to track `reffed` for the errdefer unref accounting.
        #[allow(clippy::needless_range_loop)]
        for i in 0..len {
            match &nodes[i] {
                Node::Split(_) => {}
                Node::Leaf(view) => {
                    let v = *view;
                    match unsafe { Self::view_ref(v) } {
                        Ok(r) => nodes[i] = Node::Leaf(r),
                        Err(e) => {
                            err = Some(e);
                            break;
                        }
                    }
                }
            }
            reffed = i;
        }

        if let Some(e) = err {
            // errdefer: unref the nodes we referenced.
            for node in nodes.iter().take(reffed) {
                match node {
                    Node::Split(_) => {}
                    Node::Leaf(view) => unsafe { Self::view_unref(*view) },
                }
            }
            return Err(e);
        }

        assert(reffed == nodes.len() - 1);
        Ok(())
    }

    /// Equalize this node and all its children, returning a new node with splits
    /// adjusted so that each split's ratio is based on the relative weight
    /// (number of leaves) of its children.
    pub fn equalize(&self) -> Result<Self, Error> {
        if self.is_empty() {
            return Ok(Self::empty());
        }

        // Allocate a new nodes array and copy the existing nodes into it.
        let mut nodes: Vec<Node<V>> = self.nodes.clone();

        // Go through and equalize our ratios based on weights.
        for node in nodes.iter_mut() {
            match node {
                Node::Leaf(_) => {}
                Node::Split(s) => {
                    let weight_left = self.weight(s.left, s.layout, 0);
                    let weight_right = self.weight(s.right, s.layout, 0);
                    assert(weight_left > 0);
                    assert(weight_right > 0);
                    let total_f16: F16 = F16::from_int((weight_left + weight_right) as i64);
                    let weight_left_f16: F16 = F16::from_int(weight_left as i64);
                    s.ratio = weight_left_f16 / total_f16;
                }
            }
        }

        // Increase the reference count of all the views in the nodes.
        Self::ref_nodes(&mut nodes)?;

        Ok(SplitTree {
            nodes,
            zoomed: self.zoomed,
        })
    }

    fn weight(&self, from: Handle, layout: Layout, acc: usize) -> usize {
        match &self.nodes[from.idx()] {
            Node::Leaf(_) => acc + 1,
            Node::Split(s) => {
                if s.layout == layout {
                    self.weight(s.left, layout, acc) + self.weight(s.right, layout, acc)
                } else {
                    1
                }
            }
        }
    }

    /// Resize the nearest split matching the layout by the given ratio.
    /// Positive is right and down.
    ///
    /// The ratio is a signed delta representing the percentage to move
    /// the divider. The percentage is of the entire grid size, not just
    /// the specific split size.
    /// We use the entire grid size because that's what Ghostty's
    /// `resize_split` keybind does, because it maps to a general human
    /// understanding of moving a split relative to the entire window
    /// (generally).
    ///
    /// For example, a ratio of 0.1 and a layout of `vertical` will find
    /// the nearest vertical split and move the divider down by 10% of
    /// the total grid height.
    ///
    /// If no matching split is found, this does nothing, but will always
    /// still return a cloned tree.
    pub fn resize(&self, from: Handle, layout: Layout, ratio: F16) -> Result<Self, Error> {
        assert(ratio >= F16::from_int(-1) && ratio <= F16::from_int(1));
        assert(!ratio.to_f32().is_nan());
        assert(!ratio.to_f32().is_infinite());

        // Fast path empty trees.
        if self.is_empty() {
            return Ok(Self::empty());
        }

        // From this point forward worst case we return a clone.
        let mut result = self.clone_tree()?;

        // Find our nearest parent split node matching the layout.
        let parent_handle = match self.find_parent_split(layout, from, Handle::ROOT) {
            Backtrack::Deadend | Backtrack::Backtrack => return Ok(result),
            Backtrack::Result(v) => v,
        };

        // Get our spatial layout, because we need the dimensions of this
        // split with regards to the entire grid.
        let sp = result.spatial()?;

        // Our scale is the amount we need to divide our ratio delta by to
        // get a delta relative to the split, not the entire grid.
        // Its actually a ratio on its own but I'm trying to avoid that word:
        // its the ratio of our spatial width/height to the total.
        let scale = match layout {
            Layout::Horizontal => sp.slots[parent_handle.idx()].width / sp.slots[0].width,
            Layout::Vertical => sp.slots[parent_handle.idx()].height / sp.slots[0].height,
        };

        // If the split has spatial width/height 0, resizing by a percentage
        // of the total grid size doesn't make sense.
        if scale == F16::from_int(0) {
            return Ok(result);
        }

        // Adjust the old split ratio by the scaled ratio delta.
        let old_ratio = match &result.nodes[parent_handle.idx()] {
            Node::Split(s) => s.ratio,
            Node::Leaf(_) => unreachable!(),
        };
        let new_ratio = old_ratio + (ratio / scale);

        // Set the new ratio, clamping it to [0, 1]
        result.resize_in_place(
            parent_handle,
            new_ratio.max(F16::from_int(0)).min(F16::from_int(1)),
        );
        Ok(result)
    }

    fn find_parent_split(&self, layout: Layout, from: Handle, current: Handle) -> Backtrack {
        if from == current {
            return Backtrack::Backtrack;
        }
        match &self.nodes[current.idx()] {
            Node::Leaf(_) => Backtrack::Deadend,
            Node::Split(s) => {
                let s = *s;
                match self.find_parent_split(layout, from, s.left) {
                    Backtrack::Result(v) => Backtrack::Result(v),
                    Backtrack::Backtrack => {
                        if s.layout == layout {
                            Backtrack::Result(current)
                        } else {
                            Backtrack::Backtrack
                        }
                    }
                    Backtrack::Deadend => match self.find_parent_split(layout, from, s.right) {
                        Backtrack::Deadend => Backtrack::Deadend,
                        Backtrack::Result(v) => Backtrack::Result(v),
                        Backtrack::Backtrack => {
                            if s.layout == layout {
                                Backtrack::Result(current)
                            } else {
                                Backtrack::Backtrack
                            }
                        }
                    },
                }
            }
        }
    }

    /// Spatial representation of the split tree. This can be used to
    /// better understand the layout of the tree in a 2D space.
    ///
    /// The bounds of the representation are always based on the total
    /// 2D space being 1x1. The x/y coordinates and width/height dimensions
    /// of each individual split and leaf are relative to this.
    /// This means that the spatial representation is a normalized
    /// representation of the actual space.
    ///
    /// The top-left corner of the tree is always (0, 0).
    ///
    /// We use a normalized form because we can calculate it without
    /// accessing to the actual rendered view sizes. These actual sizes
    /// may not be available at various times because GUI toolkits often
    /// only make them available once they're part of a widget tree and
    /// a SplitTree can represent views that aren't currently visible.
    pub fn spatial(&self) -> Result<Spatial, Error> {
        // No nodes, empty spatial representation.
        if self.nodes.is_empty() {
            return Ok(Spatial::empty());
        }

        // Get our total dimensions.
        let dim = self.dimensions(Handle::ROOT);

        // Create our slots which will match our nodes exactly.
        let mut slots: Vec<Slot> = vec![
            Slot {
                x: F16::from_int(0),
                y: F16::from_int(0),
                width: F16::from_int(0),
                height: F16::from_int(0),
            };
            self.nodes.len()
        ];
        slots[0] = Slot {
            x: F16::from_int(0),
            y: F16::from_int(0),
            width: F16::from_int(dim.0 as i64),
            height: F16::from_int(dim.1 as i64),
        };
        self.fill_spatial_slots(&mut slots, Handle::ROOT);

        // Normalize the dimensions to 1x1 grid.
        let dw = F16::from_int(dim.0 as i64);
        let dh = F16::from_int(dim.1 as i64);
        for slot in slots.iter_mut() {
            slot.x = slot.x / dw;
            slot.y = slot.y / dh;
            slot.width = slot.width / dw;
            slot.height = slot.height / dh;
        }

        Ok(Spatial { slots })
    }

    fn fill_spatial_slots(&self, slots: &mut [Slot], current_: Handle) {
        let current = current_.idx();
        assert(
            slots[current].width >= F16::from_int(0) && slots[current].height >= F16::from_int(0),
        );
        match &self.nodes[current] {
            // Leaf node, current slot is already filled by caller.
            Node::Leaf(_) => {}

            Node::Split(s) => {
                let s = *s;
                let cur = slots[current];
                let one = F16::from_int(1);
                match s.layout {
                    Layout::Horizontal => {
                        slots[s.left.idx()] = Slot {
                            x: cur.x,
                            y: cur.y,
                            width: cur.width * s.ratio,
                            height: cur.height,
                        };
                        slots[s.right.idx()] = Slot {
                            x: cur.x + cur.width * s.ratio,
                            y: cur.y,
                            width: cur.width * (one - s.ratio),
                            height: cur.height,
                        };
                    }

                    Layout::Vertical => {
                        slots[s.left.idx()] = Slot {
                            x: cur.x,
                            y: cur.y,
                            width: cur.width,
                            height: cur.height * s.ratio,
                        };
                        slots[s.right.idx()] = Slot {
                            x: cur.x,
                            y: cur.y + cur.height * s.ratio,
                            width: cur.width,
                            height: cur.height * (one - s.ratio),
                        };
                    }
                }

                self.fill_spatial_slots(slots, s.left);
                self.fill_spatial_slots(slots, s.right);
            }
        }
    }

    /// Get the dimensions of the tree starting from the given node.
    ///
    /// This creates relative dimensions (see Spatial) by assuming each
    /// leaf is exactly 1x1 unit in size.
    fn dimensions(&self, current: Handle) -> (u16, u16) {
        match &self.nodes[current.idx()] {
            Node::Leaf(_) => (1, 1),
            Node::Split(s) => {
                let s = *s;
                let left = self.dimensions(s.left);
                let right = self.dimensions(s.right);
                match s.layout {
                    Layout::Horizontal => (left.0 + right.0, core::cmp::max(left.1, right.1)),
                    Layout::Vertical => (core::cmp::max(left.0, right.0), left.1 + right.1),
                }
            }
        }
    }

    /// Format the tree in a human-readable format. By default this will
    /// output a diagram followed by a textual representation.
    pub fn format(&self) -> String {
        let mut out = String::new();
        if self.nodes.is_empty() {
            out.push_str("empty");
            return out;
        }
        // self.formatDiagram(writer) catch {};
        let _ = self.format_diagram_into(&mut out);
        // try self.formatText(writer);
        self.format_text_into(&mut out);
        out
    }

    /// Format only the textual representation.
    pub fn format_text(&self) -> String {
        let mut out = String::new();
        self.format_text_into(&mut out);
        out
    }

    fn format_text_into(&self, writer: &mut String) {
        if self.nodes.is_empty() {
            writer.push_str("empty");
            return;
        }
        self.format_text_inner(writer, Handle::ROOT, 0);
    }

    fn format_text_inner(&self, writer: &mut String, current: Handle, depth: usize) {
        for _ in 0..depth {
            writer.push_str("  ");
        }

        if let Some(zoomed) = self.zoomed {
            if zoomed == current {
                writer.push_str("(zoomed) ");
            }
        }

        match &self.nodes[current.idx()] {
            Node::Leaf(v) => {
                if V::HAS_SPLIT_TREE_LABEL {
                    let label = unsafe { V::split_tree_label(*v) }.unwrap_or_default();
                    let _ = writeln!(writer, "leaf: {}", String::from_utf8_lossy(&label));
                } else {
                    let _ = writeln!(writer, "leaf: {}", current.idx());
                }
            }

            Node::Split(s) => {
                let s = *s;
                let _ = writeln!(
                    writer,
                    "split (layout: {}, ratio: {:.2})",
                    s.layout.tag_name(),
                    s.ratio.to_f32()
                );
                self.format_text_inner(writer, s.left, depth + 1);
                self.format_text_inner(writer, s.right, depth + 1);
            }
        }
    }

    /// Format only the ASCII diagram.
    pub fn format_diagram(&self) -> String {
        let mut out = String::new();
        let _ = self.format_diagram_into(&mut out);
        out
    }

    fn format_diagram_into(&self, writer: &mut String) -> Result<(), ()> {
        if self.nodes.is_empty() {
            writer.push_str("empty");
            return Ok(());
        }

        // Use our arena's GPA to allocate some intermediate memory.
        // Requiring allocation for formatting is nasty but this is really
        // only used for debugging and testing and shouldn't hit OOM
        // scenarios.

        // Get our spatial representation.
        let sp: Spatial = {
            let sp = match self.spatial() {
                Ok(v) => v,
                Err(_) => return Err(()), // error.WriteFailed
            };

            // Scale our spatial representation to have minimum width/height 1.
            let mut min_w: F16 = F16::from_int(1);
            let mut min_h: F16 = F16::from_int(1);
            for slot in sp.slots.iter() {
                if slot.width > F16::from_int(0) {
                    min_w = min_w.min(slot.width);
                }
                if slot.height > F16::from_int(0) {
                    min_h = min_h.min(slot.height);
                }
            }

            let ratio_w: F16 = F16::from_int(1) / min_w;
            let ratio_h: F16 = F16::from_int(1) / min_h;
            let mut slots: Vec<Slot> = sp.slots.clone();
            for slot in slots.iter_mut() {
                slot.x = slot.x * ratio_w;
                slot.y = slot.y * ratio_h;
                slot.width = slot.width * ratio_w;
                slot.height = slot.height * ratio_h;
            }

            Spatial { slots }
        };

        // The width we need for the largest label.
        let max_label_width: usize = if !V::HAS_SPLIT_TREE_LABEL {
            // std.math.log10(sp.slots.len) + 1
            log10_usize(sp.slots.len()) + 1
        } else {
            let mut max: usize = 0;
            for node in self.nodes.iter() {
                match node {
                    Node::Split(_) => {}
                    Node::Leaf(view) => {
                        let label = unsafe { V::split_tree_label(*view) }.unwrap_or_default();
                        max = core::cmp::max(max, label.len());
                    }
                }
            }
            max
        };

        // We need space for whitespace and ASCII art so add that.
        // We need to accommodate the leaf handle, whitespace, and
        // then the border.
        // Border + whitespace + label + whitespace + border.
        let cell_width = 2 + max_label_width + 2;
        // Border + label + border. No whitespace needed on the
        // vertical axis.
        let cell_height = 1 + 1 + 1;

        // Make a grid that can fit our entire ASCII diagram. We know
        // the width/height based on node 0.
        let mut grid: Vec<Vec<u8>> = {
            // Get our initial width/height. Each leaf is 1x1 in this.
            // We round up for this because partial widths/heights should
            // take up an extra cell.
            let mut width: usize = sp.slots[0].width.ceil().to_int_trunc() as usize;
            let mut height: usize = sp.slots[0].height.ceil().to_int_trunc() as usize;

            // We need space for whitespace and ASCII art so add that.
            // We need to accommodate the leaf handle, whitespace, and
            // then the border.
            width *= cell_width;
            height *= cell_height;

            let mut rows: Vec<Vec<u8>> = Vec::with_capacity(height);
            for _y in 0..height {
                let mut row = vec![b' '; width + 1];
                row[width] = b'\n';
                rows.push(row);
            }
            rows
        };

        // Draw each node
        for (handle, slot) in sp.slots.iter().enumerate() {
            let slot = *slot;
            // We only draw leaf nodes. Splits are only used for layout.
            let node = &self.nodes[handle];
            match node {
                Node::Leaf(_) => {}
                Node::Split(_) => continue,
            }

            // If our width/height is zero then we skip this.
            if slot.width == F16::from_int(0) || slot.height == F16::from_int(0) {
                continue;
            }

            let mut x: usize = slot.x.floor().to_int_trunc() as usize;
            let mut y: usize = slot.y.floor().to_int_trunc() as usize;
            let mut width: usize = slot.width.floor().max(F16::from_int(1)).to_int_trunc() as usize;
            let mut height: usize =
                slot.height.floor().max(F16::from_int(1)).to_int_trunc() as usize;
            x *= cell_width;
            y *= cell_height;
            width *= cell_width;
            height *= cell_height;

            // Top border
            {
                let top = &mut grid[y][x..x + width];
                top[0] = b'+';
                // PARITY: mirrors reference index loop (`for (1..width-1) |i| top[i] = '-'`).
                #[allow(clippy::needless_range_loop)]
                for i in 1..width - 1 {
                    top[i] = b'-';
                }
                top[width - 1] = b'+';
            }

            // Bottom border
            {
                let bottom = &mut grid[y + height - 1][x..x + width];
                bottom[0] = b'+';
                // PARITY: mirrors reference index loop (`for (1..width-1) |i| bottom[i] = '-'`).
                #[allow(clippy::needless_range_loop)]
                for i in 1..width - 1 {
                    bottom[i] = b'-';
                }
                bottom[width - 1] = b'+';
            }

            // Left border
            // PARITY: mirrors reference index loops (`for (y+1..y+height-1) |y_cur| ...`);
            // `y_cur` indexes distinct rows of the 2-D `grid`.
            #[allow(clippy::needless_range_loop)]
            for y_cur in y + 1..y + height - 1 {
                grid[y_cur][x] = b'|';
            }
            #[allow(clippy::needless_range_loop)]
            for y_cur in y + 1..y + height - 1 {
                grid[y_cur][x + width - 1] = b'|';
            }

            // Get our label text
            let label: Vec<u8> = if V::HAS_SPLIT_TREE_LABEL {
                match &self.nodes[handle] {
                    Node::Leaf(view) => unsafe { V::split_tree_label(*view) }.unwrap_or_default(),
                    Node::Split(_) => unreachable!(),
                }
            } else {
                // std.fmt.bufPrint(&buf, "{d}", .{handle})
                format!("{}", handle).into_bytes()
            };

            // Draw the handle in the center
            let x_mid = width / 2 + x;
            let y_mid = height / 2 + y;
            let label_width = label.len();
            let label_start = x_mid - label_width / 2;
            // _ = std.fmt.bufPrint(row, "{s}", .{label});
            let row = &mut grid[y_mid];
            row[label_start..label_start + label.len()].copy_from_slice(&label);
        }

        // Output every row
        for row in grid.iter() {
            // We currently have a bug in our height calculation that
            // results in trailing blank lines. Ignore those. We should
            // really fix our height calculation instead. If someone wants
            // to do that just remove this line and see the tests that fail
            // and go from there.
            if row[0] == b' ' {
                break;
            }
            writer.push_str(&String::from_utf8_lossy(row));
        }

        Ok(())
    }

    // viewRef/viewUnref dispatch on the View's ref/unref arity in Zig
    // (allocator optional). Here the View trait's `view_ref`/`view_unref`
    // encapsulate that; rustty uses the global allocator so no allocator is
    // threaded through.
    #[inline]
    unsafe fn view_ref(view: *mut V) -> Result<*mut V, Error> {
        unsafe { V::view_ref(view) }
    }

    #[inline]
    unsafe fn view_unref(view: *mut V) {
        unsafe { V::view_unref(view) }
    }
}

impl<V: View> Drop for SplitTree<V> {
    fn drop(&mut self) {
        // Important: only free memory if we have memory to free,
        // because we use an undefined arena for empty trees.
        if !self.nodes.is_empty() {
            // Unref all our views
            for node in self.nodes.iter() {
                match node {
                    Node::Leaf(view) => unsafe { Self::view_unref(*view) },
                    Node::Split(_) => {}
                }
            }
            // self.arena.deinit(); — the owned Vec frees here.
        }
        // self.* = undefined; — no-op in Rust.
    }
}

/// Iterator over the views in the tree.
pub struct Iterator<'a, V: View> {
    i: Handle,
    nodes: &'a [Node<V>],
}

impl<'a, V: View> Iterator<'a, V> {
    // PARITY: the reference `split_tree.zig:202` exposes an inherent,
    // iterator-like `next()` on its `Iterator` type. We keep that public API
    // shape rather than reshaping it into `std::iter::Iterator`, so we silence
    // `should_implement_trait`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<ViewEntry<V>> {
        // If we have no nodes, return null.
        if self.i.0 as usize >= self.nodes.len() {
            return None;
        }

        // Get the current node and increment the index.
        let handle = self.i;
        self.i = Handle::from_int(handle.idx() + 1);
        let node = &self.nodes[handle.idx()];

        match node {
            Node::Leaf(v) => Some(ViewEntry { handle, view: *v }),
            Node::Split(_) => self.next(),
        }
    }
}

/// `std.math.log10` for a usize (integer log base 10).
fn log10_usize(mut n: usize) -> usize {
    // Matches std.math.log10 for unsigned integers (floor of log10).
    let mut result = 0usize;
    while n >= 10 {
        n /= 10;
        result += 1;
    }
    result
}

/// Make this a valid gobject if we're in a GTK environment.
//
// PORT NOTE: In Zig:
//   pub const getGObjectType = switch (build_config.app_runtime) {
//       .gtk => @import("gobject").ext.defineBoxed(Self, .{ ... copy/free ... }),
//       .none => void,
//   };
// This Miri-able pure-Rust datastruct leaf is only ever built for the `.none`
// app runtime (the GTK boxed-type registration lives in the apprt/GTK layer and
// would pull in gobject/glib, violating R3/R4 for this leaf). For `.none` the
// reference's value is literally `void`, so there is nothing to port here. The
// GTK `copy`/`free` boxed funcs are just `clone`/`deinit` wrappers (already
// ported as `clone_tree`/`Drop`) plus glib create/destroy plumbing, which the
// GTK crate will wire when it lands. No reference logic is lost.
#[allow(dead_code)]
const GET_GOBJECT_TYPE: () = ();

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    // const TestTree = SplitTree(TestView);
    type TestTree = SplitTree<TestView>;

    // const TestView = struct { label: []const u8; ref/unref/splitTreeLabel };
    struct TestView {
        label: &'static [u8],
    }

    impl View for TestView {
        const HAS_SPLIT_TREE_LABEL: bool = true;

        // pub fn ref(self, alloc) !*Self { ptr = create; ptr.* = self.*; return ptr; }
        unsafe fn view_ref(view: *mut Self) -> Result<*mut Self, Error> {
            let copy = TestView {
                label: unsafe { (*view).label },
            };
            Ok(Box::into_raw(Box::new(copy)))
        }

        // pub fn unref(self, alloc) void { alloc.destroy(self); }
        unsafe fn view_unref(view: *mut Self) {
            unsafe {
                drop(Box::from_raw(view));
            }
        }

        unsafe fn eql(a: *const Self, b: *const Self) -> bool {
            unsafe { (*a).label == (*b).label }
        }

        // pub fn splitTreeLabel(self) []const u8 { return self.label; }
        unsafe fn split_tree_label(view: *const Self) -> Option<Vec<u8>> {
            Some(unsafe { (*view).label }.to_vec())
        }
    }

    // Helper: find the handle of the view with the given label by iterating,
    // mirroring the Zig `while (it.next()) |entry| { if eql ... break } else
    // return error.NotFound` pattern.
    fn find_handle(t: &TestTree, label: &[u8]) -> Result<Handle, ()> {
        let mut it = t.iterator();
        while let Some(entry) = it.next() {
            if unsafe { (*entry.view).label } == label {
                return Ok(entry.handle);
            }
        }
        Err(())
    }

    #[test]
    fn SplitTree_isSplit() {
        // Empty tree should not be split
        let empty: TestTree = TestTree::empty();
        assert!(!empty.is_split());

        // Single node tree should not be split
        let mut v1 = TestView { label: b"A" };
        let single: TestTree = unsafe { TestTree::init(&mut v1) }.unwrap();
        assert!(!single.is_split());

        // Split tree should be split
        let mut v2 = TestView { label: b"B" };
        let tree2: TestTree = unsafe { TestTree::init(&mut v2) }.unwrap();
        let split = single
            .split(Handle::ROOT, Direction::Right, F16::from_f32(0.5), &tree2)
            .unwrap();
        assert!(split.is_split());
    }

    #[test]
    fn SplitTree_empty_tree() {
        let t: TestTree = TestTree::empty();

        let str = t.format();
        assert_eq!(str, "empty");
    }

    #[test]
    fn SplitTree_single_node() {
        let mut v = TestView { label: b"A" };
        let t: TestTree = unsafe { TestTree::init(&mut v) }.unwrap();

        let str = t.format_diagram();
        assert_eq!(
            str,
            "+---+\n\
             | A |\n\
             +---+\n"
        );
    }

    #[test]
    fn SplitTree_split_horizontal() {
        let mut v1 = TestView { label: b"A" };
        let t1: TestTree = unsafe { TestTree::init(&mut v1) }.unwrap();
        let mut v2 = TestView { label: b"B" };
        let t2: TestTree = unsafe { TestTree::init(&mut v2) }.unwrap();
        let t3 = t1
            .split(
                Handle::ROOT,     // at root
                Direction::Right, // split right
                F16::from_f32(0.5),
                &t2, // insert t2
            )
            .unwrap();

        {
            let str = t3.format();
            assert_eq!(
                str,
                "+---++---+\n\
                 | A || B |\n\
                 +---++---+\n\
                 split (layout: horizontal, ratio: 0.50)\n\
                 \x20 leaf: A\n\
                 \x20 leaf: B\n"
            );
        }

        // Split right at B
        let mut v_c = TestView { label: b"C" };
        let t_c: TestTree = unsafe { TestTree::init(&mut v_c) }.unwrap();
        let handle_b = find_handle(&t3, b"B").unwrap();
        let t4 = t3
            .split(handle_b, Direction::Right, F16::from_f32(0.5), &t_c)
            .unwrap();

        {
            let str = t4.format();
            assert_eq!(
                str,
                "+--------++---++---+\n\
                 |    A   || B || C |\n\
                 +--------++---++---+\n\
                 split (layout: horizontal, ratio: 0.50)\n\
                 \x20 leaf: A\n\
                 \x20 split (layout: horizontal, ratio: 0.50)\n\
                 \x20   leaf: B\n\
                 \x20   leaf: C\n"
            );
        }

        // Split right at C
        let mut v_d = TestView { label: b"D" };
        let t_d: TestTree = unsafe { TestTree::init(&mut v_d) }.unwrap();
        let handle_c = find_handle(&t4, b"C").unwrap();
        let t5 = t4
            .split(handle_c, Direction::Right, F16::from_f32(0.5), &t_d)
            .unwrap();

        {
            let str = t5.format();
            assert_eq!(
                "+------------------++--------++---++---+\n\
                 |         A        ||    B   || C || D |\n\
                 +------------------++--------++---++---+\n\
                 split (layout: horizontal, ratio: 0.50)\n\
                 \x20 leaf: A\n\
                 \x20 split (layout: horizontal, ratio: 0.50)\n\
                 \x20   leaf: B\n\
                 \x20   split (layout: horizontal, ratio: 0.50)\n\
                 \x20     leaf: C\n\
                 \x20     leaf: D\n",
                str
            );
        }

        // Find "previous" from D back.
        {
            let mut current: u8 = b'D';
            while current != b'A' {
                let from = find_handle(&t5, &[current]).unwrap();
                let handle = t5.previous(from).unwrap();

                let entry = match &t5.nodes[handle.idx()] {
                    Node::Leaf(v) => *v,
                    Node::Split(_) => unreachable!(),
                };
                assert_eq!(unsafe { (*entry).label }, &[current - 1]);
                current -= 1;
            }

            let from = find_handle(&t5, &[current]).unwrap();
            assert!(t5.previous(from).is_none());
        }

        // Find "next" from A forward.
        {
            let mut current: u8 = b'A';
            while current != b'D' {
                let from = find_handle(&t5, &[current]).unwrap();
                let handle = t5.next(from).unwrap();

                let entry = match &t5.nodes[handle.idx()] {
                    Node::Leaf(v) => *v,
                    Node::Split(_) => unreachable!(),
                };
                assert_eq!(unsafe { (*entry).label }, &[current + 1]);
                current += 1;
            }

            let from = find_handle(&t5, &[current]).unwrap();
            assert!(t5.next(from).is_none());
        }
    }

    #[test]
    fn SplitTree_split_vertical() {
        let mut v1 = TestView { label: b"A" };
        let t1: TestTree = unsafe { TestTree::init(&mut v1) }.unwrap();
        let mut v2 = TestView { label: b"B" };
        let t2: TestTree = unsafe { TestTree::init(&mut v2) }.unwrap();

        let t3 = t1
            .split(
                Handle::ROOT,    // at root
                Direction::Down, // split down
                F16::from_f32(0.5),
                &t2, // insert t2
            )
            .unwrap();

        let str = t3.format_diagram();
        assert_eq!(
            str,
            "+---+\n\
             | A |\n\
             +---+\n\
             +---+\n\
             | B |\n\
             +---+\n"
        );
    }

    #[test]
    fn SplitTree_split_horizontal_with_zero_ratio() {
        let mut v1 = TestView { label: b"A" };
        let t1: TestTree = unsafe { TestTree::init(&mut v1) }.unwrap();
        let mut v2 = TestView { label: b"B" };
        let t2: TestTree = unsafe { TestTree::init(&mut v2) }.unwrap();

        // A | B horizontal
        let split_ab = t1
            .split(
                Handle::ROOT,     // at root
                Direction::Right, // split right
                F16::from_f32(0.0),
                &t2, // insert t2
            )
            .unwrap();
        let split = split_ab;

        {
            let str = split.format_diagram();
            assert_eq!(
                str,
                "+---+\n\
                 | B |\n\
                 +---+\n"
            );
        }
    }

    #[test]
    fn SplitTree_split_vertical_with_zero_ratio() {
        let mut v1 = TestView { label: b"A" };
        let t1: TestTree = unsafe { TestTree::init(&mut v1) }.unwrap();
        let mut v2 = TestView { label: b"B" };
        let t2: TestTree = unsafe { TestTree::init(&mut v2) }.unwrap();

        // A | B horizontal
        let split_ab = t1
            .split(
                Handle::ROOT,    // at root
                Direction::Down, // split right
                F16::from_f32(0.0),
                &t2, // insert t2
            )
            .unwrap();
        let split = split_ab;

        {
            let str = split.format_diagram();
            assert_eq!(
                str,
                "+---+\n\
                 | B |\n\
                 +---+\n"
            );
        }
    }

    #[test]
    fn SplitTree_split_horizontal_with_full_width() {
        let mut v1 = TestView { label: b"A" };
        let t1: TestTree = unsafe { TestTree::init(&mut v1) }.unwrap();
        let mut v2 = TestView { label: b"B" };
        let t2: TestTree = unsafe { TestTree::init(&mut v2) }.unwrap();

        // A | B horizontal
        let split_ab = t1
            .split(
                Handle::ROOT,     // at root
                Direction::Right, // split right
                F16::from_f32(1.0),
                &t2, // insert t2
            )
            .unwrap();
        let split = split_ab;

        {
            let str = split.format_diagram();
            assert_eq!(
                str,
                "+---+\n\
                 | A |\n\
                 +---+\n"
            );
        }
    }

    #[test]
    fn SplitTree_split_vertical_with_full_width() {
        let mut v1 = TestView { label: b"A" };
        let t1: TestTree = unsafe { TestTree::init(&mut v1) }.unwrap();
        let mut v2 = TestView { label: b"B" };
        let t2: TestTree = unsafe { TestTree::init(&mut v2) }.unwrap();

        // A | B horizontal
        let split_ab = t1
            .split(
                Handle::ROOT,    // at root
                Direction::Down, // split right
                F16::from_f32(1.0),
                &t2, // insert t2
            )
            .unwrap();
        let split = split_ab;

        {
            let str = split.format_diagram();
            assert_eq!(
                str,
                "+---+\n\
                 | A |\n\
                 +---+\n"
            );
        }
    }

    #[test]
    fn SplitTree_remove_leaf() {
        let mut v1 = TestView { label: b"A" };
        let t1: TestTree = unsafe { TestTree::init(&mut v1) }.unwrap();
        let mut v2 = TestView { label: b"B" };
        let t2: TestTree = unsafe { TestTree::init(&mut v2) }.unwrap();
        let t3 = t1
            .split(
                Handle::ROOT,     // at root
                Direction::Right, // split right
                F16::from_f32(0.5),
                &t2, // insert t2
            )
            .unwrap();

        // Remove "A"
        let handle_a = find_handle(&t3, b"A").unwrap();
        let t4 = t3.remove(handle_a).unwrap();

        let str = t4.format_diagram();
        assert_eq!(
            str,
            "+---+\n\
             | B |\n\
             +---+\n"
        );
    }

    #[test]
    fn SplitTree_split_twice_remove_intermediary() {
        let mut v1 = TestView { label: b"A" };
        let t1: TestTree = unsafe { TestTree::init(&mut v1) }.unwrap();
        let mut v2 = TestView { label: b"B" };
        let t2: TestTree = unsafe { TestTree::init(&mut v2) }.unwrap();
        let mut v3 = TestView { label: b"C" };
        let t3: TestTree = unsafe { TestTree::init(&mut v3) }.unwrap();

        // A | B horizontal.
        let split1 = t1
            .split(
                Handle::ROOT,     // at root
                Direction::Right, // split right
                F16::from_f32(0.5),
                &t2, // insert t2
            )
            .unwrap();

        // Insert C below that.
        let split2 = split1
            .split(
                Handle::ROOT,    // at root
                Direction::Down, // split down
                F16::from_f32(0.5),
                &t3, // insert t3
            )
            .unwrap();

        {
            let str = split2.format_diagram();
            assert_eq!(
                str,
                "+---++---+\n\
                 | A || B |\n\
                 +---++---+\n\
                 +--------+\n\
                 |    C   |\n\
                 +--------+\n"
            );
        }

        // Remove "B"
        let handle_b = find_handle(&split2, b"B").unwrap();
        let split3 = split2.remove(handle_b).unwrap();

        {
            let str = split3.format_diagram();
            assert_eq!(
                str,
                "+---+\n\
                 | A |\n\
                 +---+\n\
                 +---+\n\
                 | C |\n\
                 +---+\n"
            );
        }

        // Remove every node from split2 (our most complex one), which should
        // never crash. We don't test the result is correct, this just verifies
        // we don't hit any assertion failures.
        for i in 0..split2.nodes.len() {
            let t = split2.remove(Handle::from_int(i)).unwrap();
            drop(t);
        }
    }

    #[test]
    fn SplitTree_spatial_goto() {
        let mut v1 = TestView { label: b"A" };
        let t1: TestTree = unsafe { TestTree::init(&mut v1) }.unwrap();
        let mut v2 = TestView { label: b"B" };
        let t2: TestTree = unsafe { TestTree::init(&mut v2) }.unwrap();
        let mut v3 = TestView { label: b"C" };
        let t3: TestTree = unsafe { TestTree::init(&mut v3) }.unwrap();
        let mut v4 = TestView { label: b"D" };
        let t4: TestTree = unsafe { TestTree::init(&mut v4) }.unwrap();

        // A | B horizontal
        let split_ab = t1
            .split(
                Handle::ROOT,     // at root
                Direction::Right, // split right
                F16::from_f32(0.5),
                &t2, // insert t2
            )
            .unwrap();

        // A | C vertical
        let split_ac = split_ab
            .split(
                find_handle(&split_ab, b"A").unwrap(),
                Direction::Down, // split down
                F16::from_f32(0.8),
                &t3, // insert t3
            )
            .unwrap();

        // B | D vertical
        let split_bd = split_ac
            .split(
                find_handle(&split_ab, b"B").unwrap(),
                Direction::Down, // split down
                F16::from_f32(0.3),
                &t4, // insert t4
            )
            .unwrap();
        let split = split_bd;

        {
            let str = split.format_diagram();
            assert_eq!(
                str,
                "+---++---+\n\
                 |   || B |\n\
                 |   |+---+\n\
                 |   |+---+\n\
                 | A ||   |\n\
                 |   ||   |\n\
                 |   ||   |\n\
                 |   || D |\n\
                 +---+|   |\n\
                 +---+|   |\n\
                 | C ||   |\n\
                 +---++---+\n"
            );
        }

        // Spatial C => right
        {
            let from = find_handle(&split, b"C").unwrap();
            let target = split
                .goto(from, Goto::Spatial(SpatialDirection::Right))
                .unwrap()
                .unwrap();
            let view = match &split.nodes[target.idx()] {
                Node::Leaf(v) => *v,
                Node::Split(_) => unreachable!(),
            };
            assert_eq!(unsafe { (*view).label }, b"D");
        }

        // Spatial D => left
        {
            let from = find_handle(&split, b"D").unwrap();
            let target = split
                .goto(from, Goto::Spatial(SpatialDirection::Left))
                .unwrap()
                .unwrap();
            let view = match &split.nodes[target.idx()] {
                Node::Leaf(v) => *v,
                Node::Split(_) => unreachable!(),
            };
            assert_eq!(b"A", unsafe { (*view).label });
        }

        // Spatial A => left (wrapped)
        {
            let from = find_handle(&split, b"A").unwrap();
            let target = split
                .goto(from, Goto::Spatial(SpatialDirection::Left))
                .unwrap()
                .unwrap();
            let view = match &split.nodes[target.idx()] {
                Node::Leaf(v) => *v,
                Node::Split(_) => unreachable!(),
            };
            assert_eq!(b"B", unsafe { (*view).label });
        }

        // Spatial B => right (wrapped)
        {
            let from = find_handle(&split, b"B").unwrap();
            let target = split
                .goto(from, Goto::Spatial(SpatialDirection::Right))
                .unwrap()
                .unwrap();
            let view = match &split.nodes[target.idx()] {
                Node::Leaf(v) => *v,
                Node::Split(_) => unreachable!(),
            };
            assert_eq!(b"A", unsafe { (*view).label });
        }

        // Spatial C => down (wrapped)
        {
            let from = find_handle(&split, b"C").unwrap();
            let target = split
                .goto(from, Goto::Spatial(SpatialDirection::Down))
                .unwrap()
                .unwrap();
            let view = match &split.nodes[target.idx()] {
                Node::Leaf(v) => *v,
                Node::Split(_) => unreachable!(),
            };
            assert_eq!(b"A", unsafe { (*view).label });
        }

        // Equalize
        let equal = split.equalize().unwrap();

        {
            let str = equal.format_diagram();
            assert_eq!(
                str,
                "+---++---+\n\
                 | A || B |\n\
                 +---++---+\n\
                 +---++---+\n\
                 | C || D |\n\
                 +---++---+\n"
            );
        }
    }

    #[test]
    fn SplitTree_resize() {
        let mut v1 = TestView { label: b"A" };
        let t1: TestTree = unsafe { TestTree::init(&mut v1) }.unwrap();
        let mut v2 = TestView { label: b"B" };
        let t2: TestTree = unsafe { TestTree::init(&mut v2) }.unwrap();

        // A | B horizontal
        let split = t1
            .split(
                Handle::ROOT,     // at root
                Direction::Right, // split right
                F16::from_f32(0.5),
                &t2, // insert t2
            )
            .unwrap();

        {
            let str = split.format_diagram();
            assert_eq!(
                str,
                "+---++---+\n\
                 | A || B |\n\
                 +---++---+\n"
            );
        }

        // Resize
        {
            let from = find_handle(&split, b"B").unwrap();
            let resized = split
                .resize(from, Layout::Horizontal, F16::from_f32(0.25))
                .unwrap();
            let str = resized.format_diagram();
            assert_eq!(
                str,
                "+-------------++---+\n\
                 |      A      || B |\n\
                 +-------------++---+\n"
            );
        }

        // Resize the other direction (negative ratio)
        {
            let from = find_handle(&split, b"B").unwrap();
            let resized = split
                .resize(from, Layout::Horizontal, F16::from_f32(-0.25))
                .unwrap();
            let str = resized.format_diagram();
            assert_eq!(
                str,
                "+---++-------------+\n\
                 | A ||      B      |\n\
                 +---++-------------+\n"
            );
        }
    }

    #[test]
    fn SplitTree_resize_nested_split() {
        let mut v1 = TestView { label: b"A" };
        let t1: TestTree = unsafe { TestTree::init(&mut v1) }.unwrap();
        let mut v2 = TestView { label: b"B" };
        let t2: TestTree = unsafe { TestTree::init(&mut v2) }.unwrap();
        let mut v3 = TestView { label: b"C" };
        let t3: TestTree = unsafe { TestTree::init(&mut v3) }.unwrap();

        // A | B vertical
        let split_ab = t1
            .split(
                Handle::ROOT,    // at root
                Direction::Down, // split down
                F16::from_f32(0.5),
                &t2, // insert t2
            )
            .unwrap();

        let split_bc = split_ab
            .split(
                find_handle(&split_ab, b"B").unwrap(),
                Direction::Down, // split down
                F16::from_f32(0.5),
                &t3, // insert t3
            )
            .unwrap();

        {
            let str = split_bc.format_diagram();
            assert_eq!(
                str,
                "+---+\n\
                 |   |\n\
                 |   |\n\
                 | A |\n\
                 |   |\n\
                 +---+\n\
                 +---+\n\
                 | B |\n\
                 +---+\n\
                 +---+\n\
                 | C |\n\
                 +---+\n"
            );
        }

        // Resize
        {
            let from = find_handle(&split_bc, b"B").unwrap();
            let resized = split_bc
                .resize(from, Layout::Vertical, F16::from_f32(0.125))
                .unwrap();
            let str = resized.format_diagram();
            assert_eq!(
                str,
                "+---+\n\
                 |   |\n\
                 |   |\n\
                 |   |\n\
                 |   |\n\
                 |   |\n\
                 | A |\n\
                 |   |\n\
                 |   |\n\
                 |   |\n\
                 |   |\n\
                 +---+\n\
                 +---+\n\
                 |   |\n\
                 |   |\n\
                 |   |\n\
                 | B |\n\
                 |   |\n\
                 |   |\n\
                 |   |\n\
                 +---+\n\
                 +---+\n\
                 | C |\n\
                 +---+\n"
            );
        }

        // Resize the other direction (negative ratio)
        {
            let from = find_handle(&split_bc, b"B").unwrap();
            let resized = split_bc
                .resize(from, Layout::Vertical, F16::from_f32(-0.0833))
                .unwrap();
            let str = resized.format_diagram();
            assert_eq!(
                str,
                "+---+\n\
                 |   |\n\
                 |   |\n\
                 |   |\n\
                 | A |\n\
                 |   |\n\
                 |   |\n\
                 |   |\n\
                 +---+\n\
                 +---+\n\
                 | B |\n\
                 +---+\n\
                 +---+\n\
                 |   |\n\
                 |   |\n\
                 | C |\n\
                 |   |\n\
                 +---+\n"
            );
        }
    }

    #[test]
    fn SplitTree_clone_empty_tree() {
        let t: TestTree = TestTree::empty();

        let t2 = t.clone_tree().unwrap();

        {
            let str = t2.format();
            assert_eq!(str, "empty");
        }
    }

    #[test]
    fn SplitTree_zoom() {
        let mut v1 = TestView { label: b"A" };
        let t1: TestTree = unsafe { TestTree::init(&mut v1) }.unwrap();
        let mut v2 = TestView { label: b"B" };
        let t2: TestTree = unsafe { TestTree::init(&mut v2) }.unwrap();

        // A | B horizontal
        let mut split = t1
            .split(
                Handle::ROOT,     // at root
                Direction::Right, // split right
                F16::from_f32(0.5),
                &t2, // insert t2
            )
            .unwrap();
        let handle_b = find_handle(&split, b"B").unwrap();
        split.zoom(Some(handle_b));

        {
            let str = split.format_text();
            assert_eq!(
                str,
                "split (layout: horizontal, ratio: 0.50)\n\
                 \x20 leaf: A\n\
                 \x20 (zoomed) leaf: B\n"
            );
        }

        // Clone preserves zoom
        let clone = split.clone_tree().unwrap();

        {
            let str = clone.format_text();
            assert_eq!(
                str,
                "split (layout: horizontal, ratio: 0.50)\n\
                 \x20 leaf: A\n\
                 \x20 (zoomed) leaf: B\n"
            );
        }
    }

    #[test]
    fn SplitTree_split_resets_zoom() {
        let mut v1 = TestView { label: b"A" };
        let mut t1: TestTree = unsafe { TestTree::init(&mut v1) }.unwrap();
        let mut v2 = TestView { label: b"B" };
        let t2: TestTree = unsafe { TestTree::init(&mut v2) }.unwrap();

        // Zoom A
        let handle_a = find_handle(&t1, b"A").unwrap();
        t1.zoom(Some(handle_a));

        // A | B horizontal
        let split = t1
            .split(
                Handle::ROOT,     // at root
                Direction::Right, // split right
                F16::from_f32(0.5),
                &t2, // insert t2
            )
            .unwrap();

        {
            let str = split.format_text();
            assert_eq!(
                str,
                "split (layout: horizontal, ratio: 0.50)\n\
                 \x20 leaf: A\n\
                 \x20 leaf: B\n"
            );
        }
    }

    #[test]
    fn SplitTree_remove_and_zoom() {
        let mut v1 = TestView { label: b"A" };
        let t1: TestTree = unsafe { TestTree::init(&mut v1) }.unwrap();
        let mut v2 = TestView { label: b"B" };
        let t2: TestTree = unsafe { TestTree::init(&mut v2) }.unwrap();

        // A | B horizontal
        let mut split = t1
            .split(
                Handle::ROOT,     // at root
                Direction::Right, // split right
                F16::from_f32(0.5),
                &t2, // insert t2
            )
            .unwrap();
        let handle_a = find_handle(&split, b"A").unwrap();
        split.zoom(Some(handle_a));

        // Remove A, should unzoom
        {
            let from = find_handle(&split, b"A").unwrap();
            let removed = split.remove(from).unwrap();
            assert!(removed.zoomed.is_none());

            let str = removed.format_text();
            assert_eq!(str, "leaf: B\n");
        }

        // Remove B, should keep zoom
        {
            let from = find_handle(&split, b"B").unwrap();
            let removed = split.remove(from).unwrap();

            let str = removed.format_text();
            assert_eq!(str, "(zoomed) leaf: A\n");
        }
    }
}
